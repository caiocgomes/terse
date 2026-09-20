//! Ownership mapping over a `texlive.tlpdb` catalog: which package owns a
//! given runtime file. Pure text processing.
//!
//! The catalog is a sequence of blank-line-separated records starting
//! with `name <package>`. File lists follow `runfiles size=N`, `binfiles
//! arch=<arch> size=N`, `docfiles size=N`, and `srcfiles size=N` headers,
//! one indented path per line (optionally followed by `details=...`).
//! Relocated packages list paths under `RELOC/`, which is `texmf-dist/`
//! on disk.

use std::collections::BTreeMap;

/// Maps each of `files` (repository-relative, e.g.
/// `texmf-dist/tex/latex/booktabs/booktabs.sty` or
/// `bin/universal-darwin/biber`) to the package whose `runfiles` or
/// `binfiles` section lists it. Files nobody owns are absent from the
/// result; documentation and source files never count as ownership.
pub fn owning_packages(tlpdb_text: &str, files: &[&str]) -> BTreeMap<String, String> {
    let wanted: std::collections::BTreeSet<&str> = files.iter().copied().collect();
    let mut owners = BTreeMap::new();
    let mut package: Option<&str> = None;
    let mut in_owned_section = false;

    for line in tlpdb_text.lines() {
        if let Some(name) = line.strip_prefix("name ") {
            package = Some(name.trim());
            in_owned_section = false;
            continue;
        }
        if line.is_empty() {
            in_owned_section = false;
            continue;
        }
        if line.starts_with("runfiles ") || line.starts_with("binfiles ") {
            in_owned_section = true;
            continue;
        }
        if line.starts_with("docfiles ") || line.starts_with("srcfiles ") {
            in_owned_section = false;
            continue;
        }
        if !line.starts_with(' ') {
            // Any other top-level field (revision, depend, execute, ...)
            // ends a file section.
            in_owned_section = false;
            continue;
        }
        if !in_owned_section {
            continue;
        }
        let Some(pkg) = package else { continue };
        let path = line.trim_start().split(' ').next().unwrap_or("");
        let normalized = path.strip_prefix("RELOC/").map(|rest| format!("texmf-dist/{rest}"));
        let key = normalized.as_deref().unwrap_or(path);
        if wanted.contains(key) {
            owners.entry(key.to_string()).or_insert_with(|| pkg.to_string());
        }
    }
    owners
}
