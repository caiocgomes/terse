//! Portable export membership (group 23): the minimal, explicit file set a
//! self-contained arXiv package may contain, plus the sorted content-hash
//! `MANIFEST.json` an extracted package is checked against. Pure and
//! effect-free -- it operates only on already-generated [`GeneratedFile`]
//! bytes and logical paths; no filesystem or archive-format concerns live
//! here (those belong to `terse-cli`'s `export::archive`).

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

use super::GeneratedFile;

/// Generated names group 22's ordinary source-artifact plan includes that
/// an arXiv package must NOT carry: `COMPILE.txt` is human instructions
/// for a Terse-driven build (irrelevant once nothing here needs Terse
/// again), and `build-manifest.json` is this compiler's own internal
/// determinism-check manifest, superseded here by the export's own
/// [`MANIFEST_FILE_NAME`] with a different, export-specific schema.
/// `paper.map.json` is a generated-to-source diagnostic map: the target
/// forbids maps and reports, and it would be meaningless in a package
/// from which the original `.trs` sources are deliberately absent.
const EXCLUDED_FROM_EXPORT: &[&str] = &[
    "COMPILE.txt",
    "build-manifest.json",
    crate::latex::source_map::SOURCE_MAP_FILE_NAME,
];

pub const MANIFEST_FILE_NAME: &str = "MANIFEST.json";
pub const EXPORT_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathViolation {
    Absolute(String),
    Traversal(String),
    DriveOrUnc(String),
    CaseFoldCollision(String, String),
    ReservedName(String),
}

/// Filters an ordinary source-artifact file set down to the explicit
/// arXiv export allowlist: every generated file except the ones this
/// target never carries (see [`EXCLUDED_FROM_EXPORT`]), plus whatever a
/// caller separately appends (used assets/support files, an optional
/// verified `.bbl`). Order is preserved; sorting for the manifest and
/// archive happens later, over the final combined set.
pub fn export_membership(files: &[GeneratedFile]) -> Vec<GeneratedFile> {
    files
        .iter()
        .filter(|f| !EXCLUDED_FROM_EXPORT.contains(&f.logical_path.as_str()))
        .cloned()
        .collect()
}

/// Rejects any logical path that would not be safe to extract on an
/// arbitrary host: absolute paths, `..` traversal, Windows drive letters
/// or UNC prefixes (meaningful even on a Unix build host, since the
/// package may be extracted on Windows), the reserved manifest name
/// appearing twice, and any two paths that would collide on a
/// case-insensitive filesystem (macOS default, Windows) even though they
/// are distinct on a case-sensitive one. Every violation is collected, not
/// just the first, so a single validation pass reports the complete
/// problem set.
pub fn validate_portable_paths(paths: &[String]) -> Vec<PathViolation> {
    let mut violations = Vec::new();
    let mut seen_exact: BTreeSet<&str> = BTreeSet::new();
    let mut seen_fold: std::collections::HashMap<String, &str> = std::collections::HashMap::new();

    for p in paths {
        if p.starts_with('/') || p.starts_with('\\') {
            violations.push(PathViolation::Absolute(p.clone()));
        }
        if p.contains("..") {
            violations.push(PathViolation::Traversal(p.clone()));
        }
        let looks_like_drive = p.len() >= 2 && p.as_bytes()[1] == b':' && p.as_bytes()[0].is_ascii_alphabetic();
        if looks_like_drive || p.starts_with("\\\\") {
            violations.push(PathViolation::DriveOrUnc(p.clone()));
        }
        if p == MANIFEST_FILE_NAME && !seen_exact.insert(p.as_str()) {
            violations.push(PathViolation::ReservedName(p.clone()));
        } else {
            seen_exact.insert(p.as_str());
        }

        let folded = p.to_ascii_lowercase();
        if let Some(other) = seen_fold.get(&folded) {
            if *other != p.as_str() {
                violations.push(PathViolation::CaseFoldCollision((*other).to_string(), p.clone()));
            }
        } else {
            seen_fold.insert(folded, p.as_str());
        }
    }
    violations
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Builds the sorted, deterministic `MANIFEST.json` payload for a final
/// export file set: relative path, byte size, and SHA-256 hex digest per
/// entry, sorted by path for reproducibility independent of generation
/// order. The manifest's own file explicitly excludes itself from this
/// list -- it is computed once, over exactly the payload files, before
/// the manifest itself is added to the archive.
pub fn build_export_manifest(files: &[GeneratedFile]) -> GeneratedFile {
    let mut entries: Vec<&GeneratedFile> = files.iter().filter(|f| f.logical_path != MANIFEST_FILE_NAME).collect();
    entries.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));

    let body: Vec<String> = entries
        .iter()
        .map(|f| {
            format!(
                "    {{\"path\": \"{}\", \"size\": {}, \"sha256\": \"{}\"}}",
                f.logical_path,
                f.bytes.len(),
                sha256_hex(&f.bytes)
            )
        })
        .collect();
    let json = format!(
        "{{\n  \"schema-version\": {},\n  \"files\": [\n{}\n  ]\n}}\n",
        EXPORT_MANIFEST_SCHEMA_VERSION,
        body.join(",\n")
    );
    GeneratedFile { logical_path: MANIFEST_FILE_NAME.to_string(), bytes: json.into_bytes() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gf(path: &str, bytes: &[u8]) -> GeneratedFile {
        GeneratedFile { logical_path: path.to_string(), bytes: bytes.to_vec() }
    }

    #[test]
    fn test_export_membership_excludes_internal_files() {
        let files = vec![
            gf("paper.tex", b"a"),
            gf("terse-style.sty", b"b"),
            gf("references.bib", b"c"),
            gf("build-manifest.json", b"d"),
            gf("COMPILE.txt", b"e"),
            gf("figs/plot.pdf", b"f"),
        ];
        let kept = export_membership(&files);
        let names: Vec<&str> = kept.iter().map(|f| f.logical_path.as_str()).collect();
        assert_eq!(names, vec!["paper.tex", "terse-style.sty", "references.bib", "figs/plot.pdf"]);
    }

    #[test]
    fn test_portable_paths_accepts_ordinary_relative_paths() {
        let paths = vec!["paper.tex".to_string(), "figs/plot.pdf".to_string(), MANIFEST_FILE_NAME.to_string()];
        assert!(validate_portable_paths(&paths).is_empty());
    }

    #[test]
    fn test_portable_paths_rejects_absolute_and_traversal() {
        let paths = vec!["/etc/passwd".to_string(), "../outside.tex".to_string()];
        let violations = validate_portable_paths(&paths);
        assert!(violations.contains(&PathViolation::Absolute("/etc/passwd".to_string())));
        assert!(violations.contains(&PathViolation::Traversal("../outside.tex".to_string())));
    }

    #[test]
    fn test_portable_paths_rejects_drive_and_unc() {
        let paths = vec!["C:\\Users\\x\\paper.tex".to_string(), "\\\\server\\share\\paper.tex".to_string()];
        let violations = validate_portable_paths(&paths);
        assert!(violations
            .iter()
            .any(|v| matches!(v, PathViolation::DriveOrUnc(p) if p == "C:\\Users\\x\\paper.tex")));
        assert!(violations
            .iter()
            .any(|v| matches!(v, PathViolation::DriveOrUnc(p) if p == "\\\\server\\share\\paper.tex")));
    }

    #[test]
    fn test_portable_paths_rejects_case_fold_collision() {
        let paths = vec!["figs/Plot.pdf".to_string(), "figs/plot.pdf".to_string()];
        let violations = validate_portable_paths(&paths);
        assert_eq!(
            violations,
            vec![PathViolation::CaseFoldCollision("figs/Plot.pdf".to_string(), "figs/plot.pdf".to_string())]
        );
    }

    #[test]
    fn test_manifest_excludes_its_own_hash_and_is_sorted() {
        let files = vec![gf("paper.tex", b"aaa"), gf("figs/a.pdf", b"bb")];
        let manifest = build_export_manifest(&files);
        let text = String::from_utf8(manifest.bytes).unwrap();
        assert!(!text.contains(MANIFEST_FILE_NAME));
        let a_idx = text.find("figs/a.pdf").unwrap();
        let paper_idx = text.find("paper.tex").unwrap();
        assert!(a_idx < paper_idx, "entries must be sorted by path");
    }

    #[test]
    fn test_manifest_is_deterministic_across_input_order() {
        let files_a = vec![gf("paper.tex", b"aaa"), gf("figs/a.pdf", b"bb")];
        let files_b = vec![gf("figs/a.pdf", b"bb"), gf("paper.tex", b"aaa")];
        assert_eq!(build_export_manifest(&files_a).bytes, build_export_manifest(&files_b).bytes);
    }
}
