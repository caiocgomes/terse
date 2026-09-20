//! Extracted-package validation (group 24): extracting a completed export
//! ZIP into a clean directory, verifying it against its own
//! `MANIFEST.json`, and -- when a compatible local toolchain is available
//! -- actually compiling it with restricted resource paths and an empty
//! personal TeX tree, using the recorder log to confirm nothing outside
//! the packaged files (or the system TeX distribution itself) was
//! touched. Honest, narrow reporting: a result never claims more than
//! what was actually observed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use terse_core::artifact::profile::ExportProfile;

use crate::build::{self, EngineAvailability};
use crate::engine::{self, ProcessRunner};
use crate::toolchain::{prepare_child_env, probe, HostEnv, ResolvedToolchain, TexmfDirs};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractError {
    /// A ZIP entry name is unsafe to extract (absolute, traversal, or
    /// otherwise escaping the destination directory).
    UnsafeEntry(String),
    Io(String),
}

/// Extracts `zip_bytes` into `dest` (which must already exist and be
/// empty), rejecting any entry whose name is not a plain relative path
/// confined to `dest`. Every entry is validated before any file is
/// written, so a single unsafe member fails the whole extraction rather
/// than leaving a partially-written directory.
pub fn extract_zip(zip_bytes: &[u8], dest: &Path) -> Result<(), ExtractError> {
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).map_err(|e| ExtractError::Io(e.to_string()))?;

    let mut planned: Vec<(String, usize)> = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| ExtractError::Io(e.to_string()))?;
        let name = entry.name().to_string();
        if name.starts_with('/') || name.contains("..") || name.contains('\\') {
            return Err(ExtractError::UnsafeEntry(name));
        }
        let target = dest.join(&name);
        if !target.starts_with(dest) {
            return Err(ExtractError::UnsafeEntry(name));
        }
        planned.push((name, i));
    }

    for (name, i) in planned {
        let mut entry = archive.by_index(i).map_err(|e| ExtractError::Io(e.to_string()))?;
        let target = dest.join(&name);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ExtractError::Io(e.to_string()))?;
        }
        let mut out = std::fs::File::create(&target).map_err(|e| ExtractError::Io(e.to_string()))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| ExtractError::Io(e.to_string()))?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    Unreadable(String),
    Malformed(String),
    Missing(String),
    HashMismatch { path: String },
    SizeMismatch { path: String, expected: u64, found: u64 },
}

/// Re-reads every file `manifest_json` (the export's own `MANIFEST.json`
/// content) lists, from the already-extracted `dest`, and confirms each
/// one's size and SHA-256 hash actually match. This is a plain re-check
/// against real bytes on disk, not a trust of whatever the archive
/// claimed about itself.
pub fn verify_extracted_manifest(dest: &Path, manifest_json: &str) -> Result<(), ManifestError> {
    let value: serde_json::Value =
        serde_json::from_str(manifest_json).map_err(|e| ManifestError::Malformed(e.to_string()))?;
    let files = value
        .get("files")
        .and_then(|f| f.as_array())
        .ok_or_else(|| ManifestError::Malformed("missing \"files\" array".to_string()))?;

    for entry in files {
        let path = entry
            .get("path")
            .and_then(|p| p.as_str())
            .ok_or_else(|| ManifestError::Malformed("entry missing \"path\"".to_string()))?;
        let expected_size = entry
            .get("size")
            .and_then(|s| s.as_u64())
            .ok_or_else(|| ManifestError::Malformed(format!("{path}: missing \"size\"")))?;
        let expected_sha = entry
            .get("sha256")
            .and_then(|s| s.as_str())
            .ok_or_else(|| ManifestError::Malformed(format!("{path}: missing \"sha256\"")))?;

        let full = dest.join(path);
        let bytes = std::fs::read(&full).map_err(|_| ManifestError::Missing(path.to_string()))?;
        if bytes.len() as u64 != expected_size {
            return Err(ManifestError::SizeMismatch { path: path.to_string(), expected: expected_size, found: bytes.len() as u64 });
        }
        let found_sha = sha256_hex(&bytes);
        if found_sha != expected_sha {
            return Err(ManifestError::HashMismatch { path: path.to_string() });
        }
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileReport {
    /// No compatible engine was found locally. Describes exactly what was
    /// missing; never claims the package does or does not compile.
    StaticOnly { reason: String },
    /// Compiled successfully with whatever local tools were found, but
    /// full compatibility with the target profile's exact assumptions
    /// (specific package/font/engine versions) was not independently
    /// verified -- only that *some* compatible-looking local toolchain
    /// produced a PDF.
    CompiledLocal { tested_assumptions: Vec<String> },
    /// Compiled successfully AND the local toolchain's observed
    /// engine/package/font usage was checked against the declared
    /// profile's guarantees. Still not a guarantee of arXiv acceptance
    /// (see the module doc): only that this specific, checked set of
    /// conditions held.
    CompiledProfile,
}

impl CompileReport {
    /// Plain-text rendering, worded so it never claims more than what was
    /// actually observed -- in particular, it never says or implies that
    /// arXiv or any other venue will accept the result.
    pub fn render(&self, profile_name: &str) -> String {
        match self {
            CompileReport::StaticOnly { reason } => format!(
                "compilation report: static-only ({reason}); only declared package/font/locale names were \
                 checked against the '{profile_name}' profile, nothing was actually compiled"
            ),
            CompileReport::CompiledLocal { tested_assumptions } => format!(
                "compilation report: compiled-local (compiled successfully with the XeLaTeX/Biber installed on \
                 this machine -- passes: {tested_assumptions:?} -- but their exact toolchain generation was not \
                 confirmed to match the '{profile_name}' profile; this is not a guarantee that arXiv or any other \
                 venue will accept the result)"
            ),
            CompileReport::CompiledProfile => format!(
                "compilation report: compiled-profile (compiled successfully with a toolchain reporting the same \
                 TeX Live generation the '{profile_name}' profile documents; this is still not a guarantee that \
                 arXiv or any other venue will accept the result, only that it compiled cleanly under a matching, \
                 documented toolchain generation)"
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidateError {
    RequiredButUnavailable,
    CompileFailed(String),
    /// The recorder log shows the compile touched a file that is neither
    /// one of the packaged files nor a recognized system TeX distribution
    /// resource -- most plausibly, something the package silently relies
    /// on without shipping (a "hidden dependency").
    HiddenDependency(Vec<String>),
}

/// Compiles an already-extracted, already-manifest-verified package
/// in place, with kpathsea restricted to paranoid mode (`openin_any=p`)
/// so `\input`/graphics opens cannot escape via `..` or absolute paths,
/// and an empty personal TeX tree (`TEXMFHOME`/`TEXMFVAR` pointed at a
/// fresh, unrelated scratch directory) so no ambient host configuration
/// can supply anything the package itself does not carry. `require_compile`
/// controls whether a missing local toolchain is a hard failure or an
/// honest [`CompileReport::StaticOnly`].
#[allow(clippy::too_many_arguments)]
pub fn compile_extracted(
    dest: &Path,
    main_stem: &str,
    needs_biber: bool,
    require_compile: bool,
    profile: &ExportProfile,
    toolchain: &ResolvedToolchain,
    host: &HostEnv,
    runner: &mut dyn ProcessRunner,
) -> Result<CompileReport, ValidateError> {
    let availability = build::find_pdf_engine(toolchain, require_compile);
    let (xelatex, biber) = match availability {
        EngineAvailability::Present { xelatex, biber } => (xelatex, biber),
        EngineAvailability::MissingButRequired => return Err(ValidateError::RequiredButUnavailable),
        EngineAvailability::Missing => {
            return Ok(CompileReport::StaticOnly {
                reason: "no local XeLaTeX installation was found; the package was not compiled".to_string(),
            })
        }
    };
    if needs_biber && biber.is_none() {
        return if require_compile {
            Err(ValidateError::RequiredButUnavailable)
        } else {
            Ok(CompileReport::StaticOnly {
                reason: "the package requires Biber for its bibliography, but none was found locally".to_string(),
            })
        };
    }

    // The prepared environment (allowlisted host variables, managed bin
    // first on PATH) with this validation's own fresh, empty user TeX tree
    // in place of the persistent one, plus kpathsea's paranoid mode.
    let empty_tex_tree = super::super::build::unique_temp_dir("export-empty-texmf");
    std::fs::create_dir_all(&empty_tex_tree).map_err(|e| ValidateError::CompileFailed(e.to_string()))?;
    let texmf = TexmfDirs::under(&empty_tex_tree);
    for dir in [&texmf.home, &texmf.var, &texmf.config] {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut env = prepare_child_env(toolchain, host, &texmf);
    env.push(("openin_any".to_string(), "p".to_string()));
    env.push(("openout_any".to_string(), "p".to_string()));
    let config = engine::EngineConfig {
        xelatex: xelatex.clone(),
        biber: biber.clone().unwrap_or_default(),
        timeout: std::time::Duration::from_secs(120),
    };

    let result = engine::compile_bounded_with_env(runner, dest, main_stem, &config, needs_biber, &env);
    let passes = match result {
        Ok(passes) => passes,
        Err((_, e)) => {
            let _ = std::fs::remove_dir_all(&empty_tex_tree);
            return Err(ValidateError::CompileFailed(format!("{e:?}")));
        }
    };

    let fls_path = dest.join(format!("{main_stem}.fls"));
    if let Ok(fls_text) = std::fs::read_to_string(&fls_path) {
        // The distribution roots come from `kpsewhich` through the same
        // bounded runner as every other tool, computed once per validation.
        let roots = match &toolchain.kpsewhich {
            Some(kpsewhich) => probe::distribution_roots(runner, kpsewhich, dest, &env),
            None => Vec::new(),
        };
        let hidden = hidden_dependencies(&fls_text, dest, &roots);
        if !hidden.is_empty() {
            let _ = std::fs::remove_dir_all(&empty_tex_tree);
            return Err(ValidateError::HiddenDependency(hidden));
        }
    }

    // `compiled-profile` requires the engine year AND every profile
    // package, font, and babel definition verified through `kpsewhich`
    // in the toolchain that just compiled; a matching banner alone is
    // `compiled-local` with the unverified items recorded.
    let mut tested: Vec<String> = passes.iter().map(|p| format!("{:?}", p.kind)).collect();
    let report = match probe::detect_toolchain_year(runner, &xelatex, dest, &env) {
        Some(year) if year == profile.texlive_year => {
            match probe::verify_profile(runner, toolchain.kpsewhich.as_deref(), dest, &env, profile) {
                Ok(()) => CompileReport::CompiledProfile,
                Err(unverified) => {
                    tested.extend(unverified.into_iter().map(|u| format!("unverified: {u}")));
                    CompileReport::CompiledLocal { tested_assumptions: tested }
                }
            }
        }
        _ => CompileReport::CompiledLocal { tested_assumptions: tested },
    };
    let _ = std::fs::remove_dir_all(&empty_tex_tree);
    Ok(report)
}

/// Parses a `.fls` recorder log's `INPUT` lines and returns every input
/// path that resolves neither inside `package_root` nor under one of the
/// TeX distribution `roots` (as `kpsewhich` reported them), so this only
/// ever flags something genuinely absent from both the package and the
/// toolchain that compiled it. Root-prefix membership is the right test:
/// a per-file `kpsewhich` lookup cannot see files XeLaTeX reads directly
/// (its own format, `texmf.cnf`), which the recorder still logs.
fn hidden_dependencies(fls_text: &str, package_root: &Path, roots: &[PathBuf]) -> Vec<String> {
    let canonical_root = std::fs::canonicalize(package_root).unwrap_or_else(|_| package_root.to_path_buf());
    let mut seen = BTreeSet::new();
    let mut hidden = Vec::new();

    for line in fls_text.lines() {
        let Some(raw) = line.strip_prefix("INPUT ") else { continue };
        let raw_path = PathBuf::from(raw.trim());
        // `.fls` paths are relative to the compile working directory
        // (the extracted package root) for anything under it, and
        // already absolute for anything resolved via the TeX
        // distribution search path. A relative path must be joined
        // against `package_root` before canonicalizing -- resolving it
        // against the current process's CWD instead (as a bare
        // `std::fs::canonicalize` would) checks the wrong directory
        // entirely and both under- and over-reports hidden dependencies.
        // Joined against the already-canonicalized root (not the raw,
        // possibly-symlinked `package_root`) so the `starts_with` check
        // below is consistent even when the referenced file does not
        // exist and a further `canonicalize` of it therefore fails and
        // falls back to this joined path unchanged.
        let path = if raw_path.is_absolute() { raw_path } else { canonical_root.join(&raw_path) };
        let canonical = std::fs::canonicalize(&path).unwrap_or(path.clone());
        if canonical.starts_with(&canonical_root) {
            continue;
        }
        let key = canonical.to_string_lossy().into_owned();
        if !seen.insert(key.clone()) {
            continue;
        }
        if roots.iter().any(|root| canonical.starts_with(root)) {
            continue;
        }
        hidden.push(key);
    }
    hidden
}

#[cfg(test)]
mod tests {
    use super::*;
    use terse_core::artifact::GeneratedFile;

    fn gf(path: &str, bytes: &[u8]) -> GeneratedFile {
        GeneratedFile { logical_path: path.to_string(), bytes: bytes.to_vec() }
    }

    fn tempdir(label: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "terse-export-validate-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_extract_rejects_traversal_and_absolute_entries() {
        let files = vec![gf("../escape.tex", b"x")];
        let bytes = super::super::archive::write_zip(&files);
        let dest = tempdir("traversal");
        let err = extract_zip(&bytes, &dest).unwrap_err();
        assert!(matches!(err, ExtractError::UnsafeEntry(_)));
    }

    #[test]
    fn test_extract_and_verify_manifest_round_trips() {
        let files = vec![gf("paper.tex", b"hello")];
        let manifest = terse_core::artifact::export::build_export_manifest(&files);
        let mut all = files.clone();
        all.push(manifest.clone());
        let bytes = super::super::archive::write_zip(&all);

        let dest = tempdir("roundtrip");
        extract_zip(&bytes, &dest).unwrap();
        let manifest_text = String::from_utf8(manifest.bytes).unwrap();
        assert!(verify_extracted_manifest(&dest, &manifest_text).is_ok());
    }

    #[test]
    fn test_verify_manifest_detects_tampering() {
        let files = vec![gf("paper.tex", b"hello")];
        let manifest = terse_core::artifact::export::build_export_manifest(&files);
        let dest = tempdir("tampered");
        std::fs::write(dest.join("paper.tex"), b"tampered content, different length!").unwrap();
        let manifest_text = String::from_utf8(manifest.bytes).unwrap();
        let err = verify_extracted_manifest(&dest, &manifest_text).unwrap_err();
        assert!(matches!(err, ManifestError::SizeMismatch { .. }));
    }

    #[test]
    fn test_verify_manifest_detects_missing_file() {
        let files = vec![gf("paper.tex", b"hello"), gf("figs/a.pdf", b"pdfdata")];
        let manifest = terse_core::artifact::export::build_export_manifest(&files);
        let dest = tempdir("missing");
        std::fs::write(dest.join("paper.tex"), b"hello").unwrap();
        let manifest_text = String::from_utf8(manifest.bytes).unwrap();
        let err = verify_extracted_manifest(&dest, &manifest_text).unwrap_err();
        assert_eq!(err, ManifestError::Missing("figs/a.pdf".to_string()));
    }

    #[test]
    fn test_hidden_dependency_flags_path_outside_package_and_distribution() {
        let dest = tempdir("hidden-dep-root");
        let outside = tempdir("hidden-dep-outside");
        let leaked = outside.join("leaked.tex");
        std::fs::write(&leaked, b"leaked").unwrap();
        let fls = format!("PWD {}\nINPUT paper.tex\nINPUT {}\n", dest.display(), leaked.display());
        let hidden = hidden_dependencies(&fls, &dest, &[]);
        assert_eq!(hidden.len(), 1);
        assert!(hidden[0].contains("leaked.tex"));

        // The same path under a reported distribution root is not hidden.
        let root = std::fs::canonicalize(&outside).unwrap();
        assert!(hidden_dependencies(&fls, &dest, &[root]).is_empty());
    }

    #[test]
    fn test_hidden_dependency_ignores_files_inside_package() {
        let dest = tempdir("no-hidden-dep-root");
        std::fs::write(dest.join("paper.tex"), b"x").unwrap();
        let fls = format!("PWD {}\nINPUT paper.tex\n", dest.display());
        assert!(hidden_dependencies(&fls, &dest, &[]).is_empty());
    }
}
