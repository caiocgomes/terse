//! Relocatable archives of an installed managed prefix (`toolchain status
//! --archive`) and their verified extraction (`toolchain install --offline
//! --from`). The archive is a gzip tarball whose members are relative to
//! the prefix root, plus a `terse-toolchain.sha512sums` manifest listing
//! every regular file's SHA-512 so the consumer can verify the payload
//! before installing anything. Symlinks are preserved (TeX Live's `bin/`
//! directories rely on them) but must stay inside the tree.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use super::download::sha512_hex;

pub const SUMS_FILE_NAME: &str = "terse-toolchain.sha512sums";

/// Writes `<out>` as a gzip tarball of `prefix` with the checksum manifest
/// as its first member.
pub fn create_archive(prefix: &Path, out: &Path) -> Result<(), String> {
    let sums = compute_sums(prefix)?;
    let mut manifest = String::new();
    for (rel, digest) in &sums {
        manifest.push_str(digest);
        manifest.push_str("  ");
        manifest.push_str(rel);
        manifest.push('\n');
    }
    let file = fs::File::create(out).map_err(|e| format!("cannot create '{}': {e}", out.display()))?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    builder.follow_symlinks(false);

    let bytes = manifest.as_bytes();
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_entry_type(tar::EntryType::Regular);
    header.set_cksum();
    builder.append_data(&mut header, SUMS_FILE_NAME, bytes).map_err(|e| e.to_string())?;

    append_tree(&mut builder, prefix, prefix)?;
    let encoder = builder.into_inner().map_err(|e| e.to_string())?;
    let mut file = encoder.finish().map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    Ok(())
}

fn append_tree<W: Write>(builder: &mut tar::Builder<W>, base: &Path, dir: &Path) -> Result<(), String> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("cannot read '{}': {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        let rel = path.strip_prefix(base).map_err(|e| e.to_string())?;
        if rel == Path::new(SUMS_FILE_NAME) {
            continue;
        }
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            builder.append_path_with_name(&path, rel).map_err(|e| e.to_string())?;
        } else if meta.is_dir() {
            builder.append_dir(rel, &path).map_err(|e| e.to_string())?;
            append_tree(builder, base, &path)?;
        } else if meta.is_file() {
            builder.append_path_with_name(&path, rel).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// SHA-512 of every regular file under `prefix`, keyed by `/`-separated
/// relative path, excluding the manifest itself and symlinks.
pub fn compute_sums(prefix: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut sums = BTreeMap::new();
    fn walk(base: &Path, dir: &Path, sums: &mut BTreeMap<String, String>) -> Result<(), String> {
        for entry in fs::read_dir(dir).map_err(|e| format!("cannot read '{}': {e}", dir.display()))? {
            let path = entry.map_err(|e| e.to_string())?.path();
            let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                walk(base, &path, sums)?;
            } else if meta.is_file() {
                let rel = relative_key(base, &path)?;
                if rel == SUMS_FILE_NAME {
                    continue;
                }
                let bytes = fs::read(&path).map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
                sums.insert(rel, sha512_hex(&bytes));
            }
        }
        Ok(())
    }
    walk(prefix, prefix, &mut sums)?;
    Ok(sums)
}

fn relative_key(base: &Path, path: &Path) -> Result<String, String> {
    let rel = path.strip_prefix(base).map_err(|e| e.to_string())?;
    Ok(rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/"))
}

/// Extracts a gzip tarball into `dest`, refusing absolute members, `..`
/// traversal, and symlinks whose targets leave the tree. `dest` is
/// created if needed.
pub fn extract_archive(archive: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| format!("cannot create '{}': {e}", dest.display()))?;
    let file = fs::File::open(archive).map_err(|e| format!("cannot open '{}': {e}", archive.display()))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    tar.set_preserve_permissions(true);
    tar.set_overwrite(true);
    for entry in tar.entries().map_err(|e| format!("not a tar archive: {e}"))? {
        let mut entry = entry.map_err(|e| format!("corrupt archive entry: {e}"))?;
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        check_member_path(&path)?;
        let kind = entry.header().entry_type();
        if kind.is_symlink() || kind.is_hard_link() {
            let target = entry
                .link_name()
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("link member '{}' has no target", path.display()))?
                .into_owned();
            check_link_target(&path, &target)?;
        } else if !(kind.is_file() || kind.is_dir()) {
            return Err(format!("unsupported archive member type for '{}'", path.display()));
        }
        let unpacked = entry.unpack_in(dest).map_err(|e| format!("cannot extract '{}': {e}", path.display()))?;
        if !unpacked {
            return Err(format!("refused to extract '{}' outside the destination", path.display()));
        }
    }
    Ok(())
}

fn check_member_path(path: &Path) -> Result<(), String> {
    if path.is_absolute() {
        return Err(format!("absolute archive member '{}'", path.display()));
    }
    for component in path.components() {
        match component {
            Component::ParentDir => return Err(format!("archive member '{}' traverses upward", path.display())),
            Component::Prefix(_) | Component::RootDir => {
                return Err(format!("archive member '{}' is not relative", path.display()))
            }
            _ => {}
        }
    }
    Ok(())
}

/// A link target is acceptable when, joined to the link's own directory
/// and normalized lexically, it never rises above the archive root.
fn check_link_target(link: &Path, target: &Path) -> Result<(), String> {
    if target.is_absolute() {
        return Err(format!("symlink '{}' targets an absolute path", link.display()));
    }
    let mut depth: i64 = link.parent().map(|p| p.components().filter(|c| matches!(c, Component::Normal(_))).count()).unwrap_or(0) as i64;
    for component in target.components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return Err(format!("symlink '{}' escapes the archive root", link.display()));
                }
            }
            Component::CurDir => {}
            _ => return Err(format!("symlink '{}' has an unsupported target", link.display())),
        }
    }
    Ok(())
}

/// Verifies every file listed in `<dest>/terse-toolchain.sha512sums`
/// against its recorded digest, and that no regular file outside the
/// manifest exists. Removes the manifest on success.
pub fn verify_sums(dest: &Path) -> Result<usize, String> {
    let sums_path = dest.join(SUMS_FILE_NAME);
    let text = fs::read_to_string(&sums_path).map_err(|_| "archive has no checksum manifest".to_string())?;
    let mut expected = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let Some((digest, rel)) = line.split_once("  ") else {
            return Err(format!("checksum manifest line {} is malformed", n + 1));
        };
        if digest.len() != 128 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("checksum manifest line {} has no SHA-512 digest", n + 1));
        }
        expected.insert(rel.to_string(), digest.to_ascii_lowercase());
    }
    let actual = compute_sums(dest)?;
    for (rel, digest) in &expected {
        match actual.get(rel) {
            Some(found) if found == digest => {}
            Some(_) => return Err(format!("'{rel}' does not match its recorded checksum")),
            None => return Err(format!("'{rel}' is listed in the manifest but missing from the archive")),
        }
    }
    for rel in actual.keys() {
        if !expected.contains_key(rel) {
            return Err(format!("'{rel}' is present in the archive but not in its manifest"));
        }
    }
    fs::remove_file(&sums_path).map_err(|e| e.to_string())?;
    Ok(expected.len())
}

/// Reads a small regular file from an extracted archive.
pub fn read_member(dest: &Path, rel: &str) -> Option<String> {
    let mut text = String::new();
    fs::File::open(dest.join(rel)).ok()?.read_to_string(&mut text).ok()?;
    Some(text)
}
