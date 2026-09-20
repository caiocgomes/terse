//! A disposable, content-addressed cache for generated source artifacts.
//!
//! The cache is never authoritative: any lookup miss (key not found,
//! corrupted entry, unreadable directory) is treated as a plain cache
//! miss and silently triggers a recompute. Deleting the whole cache
//! directory must never change program behavior, only performance. The
//! reference lock (`references.lock`) is never served from here — it is
//! its own persistent source of truth, read fresh from disk on every run.
//!
//! Cache keys fold in the running compiler's package version, so a
//! `terse` upgrade invalidates every prior entry rather than risking a
//! stale-schema hit.

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const CACHE_DIR_NAME: &str = ".terse-cache";

/// A generated file as `(logical_path, bytes)`, matching the shape
/// produced by artifact generation.
pub type CachedFiles = Vec<(String, Vec<u8>)>;

pub fn cache_dir(project_root: &Path) -> PathBuf {
    project_root.join(CACHE_DIR_NAME)
}

/// Computes a stable cache key from the compiler version plus every
/// caller-supplied input fragment, in order. Callers pass every byte
/// sequence that can affect generated output (source bytes, resolved
/// theme data, declared packages/support files, bound citation data).
pub fn compute_key(parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(env!("CARGO_PKG_VERSION").as_bytes());
    hasher.update([0u8]);
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// Reads a cached entry for `key`, returning `None` on any miss:
/// nonexistent file, unreadable directory, or malformed/corrupted
/// content. Never panics or propagates an error to the caller.
pub fn load(project_root: &Path, key: &str) -> Option<CachedFiles> {
    let path = cache_dir(project_root).join(format!("{key}.bin"));
    let bytes = fs::read(path).ok()?;
    decode(&bytes)
}

/// Writes a cache entry for `key`. Best-effort: any I/O failure (e.g. a
/// read-only cache directory) is silently ignored, since the cache is
/// disposable and never required for correctness.
pub fn store(project_root: &Path, key: &str, files: &CachedFiles) {
    let dir = cache_dir(project_root);
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let encoded = encode(files);
    let final_path = dir.join(format!("{key}.bin"));
    let tmp_path = dir.join(format!("{key}.bin.tmp-{}", std::process::id()));
    if fs::write(&tmp_path, &encoded).is_err() {
        return;
    }
    let _ = fs::rename(&tmp_path, &final_path);
}

fn encode(files: &CachedFiles) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(files.len() as u64).to_le_bytes());
    for (path, bytes) in files {
        let path_bytes = path.as_bytes();
        out.extend_from_slice(&(path_bytes.len() as u64).to_le_bytes());
        out.extend_from_slice(path_bytes);
        out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    out
}

/// Decodes a cache entry, returning `None` (never panicking) on any
/// malformed input: truncated length prefixes, invalid UTF-8 paths, or
/// lengths that would read past the end of the buffer.
fn decode(bytes: &[u8]) -> Option<CachedFiles> {
    let mut pos = 0usize;
    let count = read_u64(bytes, &mut pos)? as usize;
    let mut files = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let path_len = read_u64(bytes, &mut pos)? as usize;
        let path_bytes = read_slice(bytes, &mut pos, path_len)?;
        let path = String::from_utf8(path_bytes.to_vec()).ok()?;
        let content_len = read_u64(bytes, &mut pos)? as usize;
        let content = read_slice(bytes, &mut pos, content_len)?.to_vec();
        files.push((path, content));
    }
    if pos != bytes.len() {
        return None;
    }
    Some(files)
}

fn read_u64(bytes: &[u8], pos: &mut usize) -> Option<u64> {
    let slice = read_slice(bytes, pos, 8)?;
    Some(u64::from_le_bytes(slice.try_into().ok()?))
}

fn read_slice<'a>(bytes: &'a [u8], pos: &mut usize, len: usize) -> Option<&'a [u8]> {
    let end = pos.checked_add(len)?;
    let slice = bytes.get(*pos..end)?;
    *pos = end;
    Some(slice)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let files: CachedFiles = vec![
            ("main.tex".to_string(), b"hello".to_vec()),
            ("style.sty".to_string(), b"\x00\x01binary".to_vec()),
        ];
        let encoded = encode(&files);
        assert_eq!(decode(&encoded), Some(files));
    }

    #[test]
    fn test_corrupted_bytes_are_a_miss_not_a_panic() {
        assert_eq!(decode(b"not a valid cache entry"), None);
        assert_eq!(decode(&[]), None);
        assert_eq!(decode(&[9, 0, 0, 0, 0, 0, 0, 0]), None);
    }

    #[test]
    fn test_key_changes_with_any_input_or_version_relevant_part() {
        let a = compute_key(&[b"one", b"two"]);
        let b = compute_key(&[b"one", b"three"]);
        let c = compute_key(&[b"onetwo"]);
        assert_ne!(a, b);
        assert_ne!(a, c, "length-prefixing must prevent part-boundary collisions");
    }

    #[test]
    fn test_missing_and_corrupted_entries_are_misses() {
        let tmp = std::env::temp_dir().join(format!("terse-cache-unit-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        assert_eq!(load(&tmp, "absent"), None);

        let files: CachedFiles = vec![("a.tex".to_string(), b"x".to_vec())];
        store(&tmp, "k1", &files);
        assert_eq!(load(&tmp, "k1"), Some(files));

        fs::write(cache_dir(&tmp).join("k1.bin"), b"garbage").unwrap();
        assert_eq!(load(&tmp, "k1"), None);

        let _ = fs::remove_dir_all(&tmp);
    }
}
