//! `fmt`/`fmt --check`: batch, all-or-nothing lossless formatting of
//! `.trs` and `.theme` files.
//!
//! Formatting is a syntax-level operation: it never invokes reference
//! binding or symbol resolution (an unresolved `refs:` alias is not a
//! formatting error), and it never touches a file until every selected
//! file has parsed successfully -- a batch either writes every changed
//! file or writes none of them.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use terse_core::diagnostic::Diagnostic;
use terse_core::source::{FileId, SourceFile};
use terse_core::theme::parse::format_theme;

use crate::diagnostics::{render_human, FileIndex};
use crate::project::{self, ProjectError};

pub const FMT_JSON_VERSION: u32 = 1;

#[derive(Serialize)]
struct FmtEntry {
    path: String,
    would_reformat: bool,
}

#[derive(Serialize)]
struct FmtJsonEnvelope {
    version: u32,
    files: Vec<FmtEntry>,
}

enum Kind {
    Terse,
    Theme,
}

fn kind_of(path: &Path) -> Option<Kind> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("trs") => Some(Kind::Terse),
        Some("theme") => Some(Kind::Theme),
        _ => None,
    }
}

/// Formats `source`'s bytes according to its extension, discovered from
/// `path`. Both formatters share `terse_core::syntax::format`'s
/// structural-whitespace canonicalization; only their front-end parse
/// differs.
fn format_bytes(path: &Path, source: &SourceFile) -> Result<Vec<u8>, Diagnostic> {
    match kind_of(path) {
        Some(Kind::Terse) => terse_core::syntax::format::format_source(source),
        Some(Kind::Theme) => format_theme(source).map_err(|e| {
            // `.theme` grammar errors have no shared `Diagnostic`
            // conversion (task group 9 scoped that grammar as a
            // syntax-only, pre-resolution stage); render a minimal
            // diagnostic so fmt's error path stays uniform.
            Diagnostic::error(
                "E-THEME-000",
                format!("invalid theme file: {e:?}"),
                terse_core::source::SourceSpan::new(source.id, 0, 0),
            )
        }),
        None => Err(Diagnostic::error(
            "E-FMT-001",
            format!("{}: not a .trs or .theme file", path.display()),
            terse_core::source::SourceSpan::new(source.id, 0, 0),
        )),
    }
}

/// Discovers the sorted, deduplicated set of reachable `.trs` files (the
/// entry plus its transitive includes) and declared `.theme` files, when
/// no explicit paths were given.
fn discover_default_targets(cwd: &Path) -> Result<Vec<PathBuf>, ProjectError> {
    let (ctx, entry_path) = project::resolve_project(None, cwd)?;
    let loaded = project::load_modules(&ctx.root, &entry_path)?;

    let mut set: BTreeSet<PathBuf> = BTreeSet::new();
    set.insert(entry_path);
    for (_, (path, _)) in loaded.file_index.iter() {
        set.insert(path.clone());
    }
    for theme_path in ctx.manifest.themes.values() {
        set.insert(ctx.root.join(theme_path));
    }
    Ok(set.into_iter().collect())
}

/// Deduplicates explicit paths by their canonical form where possible
/// (falling back to the joined path itself if it doesn't exist yet),
/// preserving first-occurrence order.
fn dedupe_explicit(cwd: &Path, paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    let mut out = Vec::new();
    for p in paths {
        let joined = cwd.join(p);
        let key = std::fs::canonicalize(&joined).unwrap_or_else(|_| joined.clone());
        if seen.insert(key) {
            out.push(joined);
        }
    }
    out
}

pub fn run_fmt(cwd: &Path, paths: &[PathBuf], check: bool, json: bool) -> i32 {
    let targets: Vec<PathBuf> = if paths.is_empty() {
        match discover_default_targets(cwd) {
            Ok(v) => v,
            Err(e) => return report_project_error(&e),
        }
    } else {
        dedupe_explicit(cwd, paths)
    };

    // Parse (and format) every target before writing anything: a batch
    // with one bad file must leave every file on disk untouched.
    let mut prepared: Vec<(PathBuf, Vec<u8>, Vec<u8>)> = Vec::new();
    let mut file_index: FileIndex = FileIndex::new();
    for (i, path) in targets.iter().enumerate() {
        let original = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("error: cannot read {}: {e}", path.display());
                return 2;
            }
        };
        let file_id = FileId(i as u32);
        let source = match SourceFile::new(file_id, path.to_string_lossy(), original.clone()) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("error[E-SOURCE-001]: {}: {e:?}", path.display());
                return 1;
            }
        };
        let text = source.text().to_string();
        file_index.insert(file_id, (path.clone(), text));

        match format_bytes(path, &source) {
            Ok(formatted) => prepared.push((path.clone(), original, formatted)),
            Err(diag) => {
                eprintln!("{}", render_human(&diag, &path.to_string_lossy(), &file_index[&file_id].1));
                return 1;
            }
        }
    }

    let changed: Vec<&(PathBuf, Vec<u8>, Vec<u8>)> =
        prepared.iter().filter(|(_, orig, fmt)| orig != fmt).collect();
    let any_diff = !changed.is_empty();

    if json {
        let envelope = FmtJsonEnvelope {
            version: FMT_JSON_VERSION,
            files: prepared
                .iter()
                .map(|(path, orig, fmt)| FmtEntry {
                    path: path.to_string_lossy().into_owned(),
                    would_reformat: orig != fmt,
                })
                .collect(),
        };
        println!(
            "{}",
            serde_json::to_string(&envelope).expect("fmt JSON envelope is always serializable")
        );
    } else if check {
        for (path, _, _) in &changed {
            eprintln!("would reformat {}", path.display());
        }
    }

    if check {
        // Read-only: never writes, regardless of drift.
        return if any_diff { 1 } else { 0 };
    }

    for (path, _, formatted) in &changed {
        if let Err(e) = write_atomic(path, formatted) {
            eprintln!("error: cannot write {}: {e}", path.display());
            return 3;
        }
    }
    0
}

/// Writes `bytes` to `path` atomically: a sibling temp file, then a
/// same-filesystem rename, so a formatting run is never observed as a
/// partially-written file.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp_path = dir.join(format!(".{file_name}.terse-fmt-tmp"));
    std::fs::write(&tmp_path, bytes)?;
    std::fs::rename(&tmp_path, path)
}

fn report_project_error(e: &ProjectError) -> i32 {
    // One rendering for every command: `build` owns the code table.
    crate::build::report_project_error(e)
}
