//! Offline arXiv export target (group 22): shared current-input
//! validation for `check --target arxiv` and `export --target arxiv`
//! against a versioned, offline compatibility profile. Building the full
//! portable ZIP/allowlist and its determinism guarantees is group 23's
//! job; this module owns readiness validation and the one export-only
//! membership rule already well-defined at this stage: default `.bib`,
//! no prebuilt `.bbl` unless `--include-bbl` is explicitly requested and
//! verified compatible.
//!
//! This module never constructs a network transport, directly or
//! transitively -- it only ever calls into `build::compile_entry` (pure
//! recompilation from current sources) and the same theme/asset
//! resolution `build` already uses for ordinary builds.

pub mod archive;
pub mod validate;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use terse_core::artifact::profile::{self, ExportProfile};
use terse_core::diagnostic::Diagnostic;
use terse_core::latex::UnknownPackage;
use terse_core::references::record::NormalizedRecord;
use terse_core::theme::ResolvedTheme;
use terse_core::ArtifactPlan;

use crate::build;
use crate::project::ProjectContext;

pub const DEFAULT_TARGET_PROFILE: &str = "texlive-2025-xelatex";

/// Support-file extensions considered inert (never executable, never a
/// format Terse cannot account for) for MVP export. Anything else --
/// including every extension a normal build already permits via
/// `[latex] support-files` -- is rejected with `E-EXPORT-004`.
const EXPORT_SAFE_SUPPORT_EXTENSIONS: &[&str] = &["tex", "sty", "cls", "bib", "cfg", "clo", "bst"];

#[derive(Debug)]
pub enum TargetError {
    /// Compile-time diagnostics (real semantic/binding errors, or
    /// synthesized `E-EXPORT-004` raw-`tex:` rejections, which carry a
    /// genuine source span like any other diagnostic).
    Diagnostics(Vec<Diagnostic>),
    UnknownProfile(String),
    /// A declared package, font, locale, or support-file extension this
    /// profile does not guarantee. Has no single source span (it may
    /// span a manifest declaration and a resolved theme setting at
    /// once), so it is reported as a plain configuration error rather
    /// than a positioned diagnostic, matching this codebase's existing
    /// split between `Diagnostic`-bearing content errors and plain
    /// manifest/tool errors (see `build::report_project_error`).
    UnsupportedDependency(String),
    /// A lower CLI layer (project resolution, theme resolution, asset
    /// resolution) already printed its own error and chose an exit code.
    Cli(i32),
}

pub struct ValidatedTarget {
    pub plan: ArtifactPlan,
    pub theme: ResolvedTheme,
    pub theme_dir: PathBuf,
    pub extra_packages: Vec<&'static str>,
    pub support_files: Vec<(String, Vec<u8>)>,
    pub cited: BTreeMap<String, NormalizedRecord>,
    pub needs_biber: bool,
    pub profile: ExportProfile,
}

/// Shared by `check --target arxiv` and `export --target arxiv`. Always
/// recompiles from current sources -- it never consults or trusts a
/// previous build's output directory, and it deliberately does not use
/// the disposable source-generation cache from group 20 (export must
/// prove the *current* state is exportable, not a possibly-stale one;
/// since that cache is content-addressed this only matters when inputs
/// genuinely changed, which is exactly the case this must not paper
/// over).
pub fn validate_target(
    project: &ProjectContext,
    entry_path: &Path,
    theme_name: &str,
    profile_name: &str,
) -> Result<ValidatedTarget, TargetError> {
    let profile = profile::resolve_profile(profile_name)
        .map_err(|profile::UnknownProfile(name)| TargetError::UnknownProfile(name))?;

    let (diags, plan, _file_index) = build::compile_entry(project, entry_path).map_err(TargetError::Cli)?;
    if !diags.is_empty() {
        return Err(TargetError::Diagnostics(diags));
    }
    let mut plan: ArtifactPlan = plan.expect("no diagnostics implies a valid artifact plan");

    let raw_spans = terse_core::semantic::collect_raw_tex_spans(&plan.module);
    if !raw_spans.is_empty() {
        let diags = raw_spans
            .into_iter()
            .map(|span| {
                Diagnostic::error(
                    "E-EXPORT-004",
                    "raw `tex:` blocks are unsupported by the MVP arXiv export target; \
                     normal-build support for raw TeX is unaffected",
                    span,
                )
            })
            .collect();
        return Err(TargetError::Diagnostics(diags));
    }

    let (mut theme, theme_dir) = build::resolve_theme(project, theme_name).map_err(TargetError::Cli)?;

    let entry_dir = entry_path.parent().unwrap_or(&project.root);
    let mut asset_files = build::resolve_assets(project, entry_dir, &theme_dir, &mut plan.module, &mut theme)
        .map_err(TargetError::Cli)?;

    for declared in &project.manifest.latex.support_files {
        let ext = declared.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if !EXPORT_SAFE_SUPPORT_EXTENSIONS.contains(&ext.as_str()) {
            return Err(TargetError::Diagnostics(vec![Diagnostic::error(
                "E-EXPORT-004",
                format!(
                    "declared support file '{declared}' has an extension the MVP arXiv export \
                     target treats as an unverifiable custom executable dependency"
                ),
                plan.module
                    .blocks
                    .first()
                    .map(|n| n.span)
                    .unwrap_or_else(|| terse_core::source::SourceSpan::new(plan.module.file_id, 0, 0)),
            )]));
        }
    }
    let mut support_files =
        build::load_support_files(&project.root, &project.manifest.latex.support_files).map_err(TargetError::Cli)?;
    support_files.append(&mut asset_files);

    let extra_packages = terse_core::latex::validate_packages(&project.manifest.latex.packages)
        .map_err(|UnknownPackage(name)| TargetError::UnsupportedDependency(format!("unknown declared package '{name}'")))?;

    let cited_aliases = terse_core::semantic::collect_cited_aliases(&plan.module);
    let cited: BTreeMap<_, _> = plan
        .bindings
        .authorized
        .iter()
        .filter(|(alias, _)| cited_aliases.contains(*alias))
        .map(|(alias, record)| (alias.clone(), record.clone()))
        .collect();
    let needs_biber = !cited.is_empty();

    let font_package = theme
        .body_font
        .as_deref()
        .map(terse_core::latex::font_package_for);
    let babel_language = if needs_biber {
        Some(plan.module.metadata.as_ref().map(|m| m.language.clone()).unwrap_or_else(|| "en".to_string()))
    } else {
        None
    };
    let violations = profile::check_requirements(
        &profile,
        &extra_packages,
        font_package,
        babel_language.as_deref(),
    );
    if !violations.is_empty() {
        return Err(TargetError::UnsupportedDependency(format!(
            "{profile_name} does not guarantee: {violations:?}"
        )));
    }

    Ok(ValidatedTarget {
        plan,
        theme,
        theme_dir,
        extra_packages,
        support_files,
        cited,
        needs_biber,
        profile,
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum BblError {
    /// `--include-bbl` was requested but this export's bibliography
    /// strategy does not use Biber/BibLaTeX at all (no citations), so
    /// there is nothing to verify compatibility against.
    NoBibliography,
    NotFound(PathBuf),
    /// The `.bbl`'s filename stem does not match the generated main
    /// TeX file's stem (`paper`), which arXiv/LaTeX both require.
    StemMismatch { found: String, expected: String },
    /// The file exists and has the right stem, but its content does not
    /// carry any recognizable BibLaTeX/Biber marker -- it cannot be
    /// verified compatible, so it is rejected rather than trusted.
    Unverifiable(PathBuf),
}

/// Verifies a requested pre-built `.bbl` for `--include-bbl`: it must
/// exist, its stem must match the generated main TeX file's stem
/// (`paper`, per this compiler's fixed naming), and its content must
/// carry a recognizable BibLaTeX/Biber marker. Never renames or
/// otherwise alters the file to make it pass; a failed verification is
/// always an explicit error, never a silent substitution.
pub fn verify_include_bbl(bbl_path: &Path, needs_biber: bool) -> Result<Vec<u8>, BblError> {
    if !needs_biber {
        return Err(BblError::NoBibliography);
    }
    let stem = bbl_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if stem != "paper" {
        return Err(BblError::StemMismatch {
            found: stem.to_string(),
            expected: "paper".to_string(),
        });
    }
    let bytes = std::fs::read(bbl_path).map_err(|_| BblError::NotFound(bbl_path.to_path_buf()))?;
    let text = String::from_utf8_lossy(&bytes);
    if !text.contains("biblatex") && !text.contains("Biber") {
        return Err(BblError::Unverifiable(bbl_path.to_path_buf()));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_include_bbl_requires_bibliography() {
        assert_eq!(verify_include_bbl(Path::new("/nonexistent/paper.bbl"), false), Err(BblError::NoBibliography));
    }

    #[test]
    fn test_include_bbl_rejects_stem_mismatch() {
        let err = verify_include_bbl(Path::new("/nonexistent/other.bbl"), true).unwrap_err();
        assert_eq!(
            err,
            BblError::StemMismatch {
                found: "other".to_string(),
                expected: "paper".to_string(),
            }
        );
    }
}
