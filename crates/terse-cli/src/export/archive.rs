//! Deterministic ZIP archive construction for arXiv export (group 23):
//! given a final, already-validated file set, produces byte-identical ZIP
//! output for byte-identical input regardless of source mtimes, build
//! host, or member insertion order at the call site -- every source of
//! nondeterminism the `zip` crate would otherwise pick up from the
//! environment (per-file timestamps, platform attribute bits, Unix mode
//! bits inherited from a real file) is fixed explicitly instead.

use std::io::Write;

use terse_core::artifact::GeneratedFile;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

/// The classic ZIP epoch (1980-01-01, the earliest date the DOS-style
/// timestamp field the format uses can represent) rather than the actual
/// build time, so identical inputs always produce identical archive
/// bytes no matter when export runs.
fn fixed_timestamp() -> DateTime {
    DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0).expect("1980-01-01 00:00:00 is always a valid ZIP timestamp")
}

/// Builds a deterministic ZIP archive from `files`, which MUST already be
/// sorted by logical path (callers pass the same sorted set used to build
/// the manifest, so member order matches manifest order) and validated
/// portable (see `terse_core::artifact::export::validate_portable_paths`).
/// Every member gets the same fixed modification time, the same
/// `0o644` Unix permission bits (never whatever the source file's real
/// mode happened to be), and the same compression method/level.
pub fn write_zip(files: &[GeneratedFile]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut writer = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(6))
            .last_modified_time(fixed_timestamp())
            .unix_permissions(0o644);
        for f in files {
            writer.start_file(&f.logical_path, options).expect("in-memory zip write cannot fail");
            writer.write_all(&f.bytes).expect("in-memory zip write cannot fail");
        }
        writer.finish().expect("in-memory zip finish cannot fail");
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gf(path: &str, bytes: &[u8]) -> GeneratedFile {
        GeneratedFile { logical_path: path.to_string(), bytes: bytes.to_vec() }
    }

    #[test]
    fn test_zip_bytes_are_deterministic_across_calls() {
        let files = vec![gf("figs/a.pdf", b"aaa"), gf("paper.tex", b"bbb")];
        assert_eq!(write_zip(&files), write_zip(&files));
    }

    #[test]
    fn test_zip_roundtrips_content() {
        let files = vec![gf("paper.tex", b"hello world")];
        let bytes = write_zip(&files);
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut entry = archive.by_name("paper.tex").unwrap();
        let mut out = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut out).unwrap();
        assert_eq!(out, b"hello world");
    }
}
