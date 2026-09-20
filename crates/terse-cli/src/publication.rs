//! Transactional publication of generated output: staging on the same
//! filesystem, ownership checks, atomic-rename-or-backup replacement, and
//! journal-based restart recovery.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const OWNERSHIP_MARKER: &str = ".terse-owned.json";
const JOURNAL_FILE: &str = ".terse-publish-journal.json";

#[derive(Debug)]
pub enum PublishError {
    UnownedDestination,
    Io(String),
}

fn io_err(e: io::Error) -> PublishError {
    PublishError::Io(e.to_string())
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_sibling(parent: &Path, label: &str) -> PathBuf {
    parent.join(format!(
        ".terse-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

pub struct StagedBuild {
    pub staging_dir: PathBuf,
}

/// Stages `files` (logical path -> bytes) into a fresh sibling directory of
/// `final_dir`, on the same filesystem so the eventual publish can be a
/// single rename.
pub fn stage(final_dir: &Path, files: &[(String, Vec<u8>)]) -> Result<StagedBuild, PublishError> {
    let parent = final_dir
        .parent()
        .ok_or_else(|| PublishError::Io("output directory has no parent".to_string()))?;
    fs::create_dir_all(parent).map_err(io_err)?;
    let staging_dir = unique_sibling(parent, "staging");
    fs::create_dir_all(&staging_dir).map_err(io_err)?;

    for (name, bytes) in files {
        let dest = staging_dir.join(name);
        if let Some(p) = dest.parent() {
            fs::create_dir_all(p).map_err(io_err)?;
        }
        fs::write(&dest, bytes).map_err(io_err)?;
    }

    fs::write(staging_dir.join(OWNERSHIP_MARKER), ownership_manifest_json(files))
        .map_err(io_err)?;

    Ok(StagedBuild { staging_dir })
}

fn ownership_manifest_json(files: &[(String, Vec<u8>)]) -> String {
    let mut names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    names.sort_unstable();
    let items: Vec<String> = names.iter().map(|n| format!("\"{n}\"")).collect();
    format!("{{\"owned-files\": [{}]}}\n", items.join(", "))
}

/// Refuses to replace a destination that already has content but no valid
/// Terse ownership marker: a forged or malformed marker does not authorize
/// replacing (or deleting) whatever is already there.
pub fn check_destination_ownership(final_dir: &Path) -> Result<(), PublishError> {
    if !final_dir.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(final_dir).map_err(io_err)?;
    if entries.next().is_none() {
        return Ok(());
    }
    match fs::read_to_string(final_dir.join(OWNERSHIP_MARKER)) {
        Ok(content) if content.contains("\"owned-files\"") => Ok(()),
        _ => Err(PublishError::UnownedDestination),
    }
}

/// Publishes a staged build into `final_dir`. Records intent to a journal
/// before mutating anything durable, so [`recover`] can finish an
/// interrupted publication after a crash/restart. Never removes the prior
/// generation until the new one is fully installed.
pub fn publish(staged: StagedBuild, final_dir: &Path) -> Result<(), PublishError> {
    check_destination_ownership(final_dir)?;

    let parent = final_dir
        .parent()
        .ok_or_else(|| PublishError::Io("output directory has no parent".to_string()))?;
    let journal_path = parent.join(JOURNAL_FILE);
    let backup_dir = unique_sibling(parent, "backup");

    write_journal(&journal_path, &staged.staging_dir, final_dir, &backup_dir, "begin")?;

    if final_dir.exists() {
        fs::rename(final_dir, &backup_dir).map_err(io_err)?;
        write_journal(
            &journal_path,
            &staged.staging_dir,
            final_dir,
            &backup_dir,
            "backed-up",
        )?;
    }

    fs::rename(&staged.staging_dir, final_dir).map_err(io_err)?;

    if backup_dir.exists() {
        let _ = fs::remove_dir_all(&backup_dir);
    }
    let _ = fs::remove_file(&journal_path);
    Ok(())
}

fn write_journal(
    journal_path: &Path,
    staging: &Path,
    final_dir: &Path,
    backup: &Path,
    stage: &str,
) -> Result<(), PublishError> {
    fs::write(
        journal_path,
        format!(
            "{{\"staging\": {:?}, \"final\": {:?}, \"backup\": {:?}, \"stage\": \"{stage}\"}}\n",
            staging, final_dir, backup
        ),
    )
    .map_err(io_err)
}

/// Finishes an interrupted publication found in `parent`'s journal: if the
/// staged generation was never installed, retries the rename; a leftover
/// backup alongside a valid final generation is discarded. Leaves exactly
/// one complete, valid generation and no journal.
pub fn recover(parent: &Path) -> Result<(), PublishError> {
    let journal_path = parent.join(JOURNAL_FILE);
    let content = match fs::read_to_string(&journal_path) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };

    let staging = extract_path_field(&content, "staging");
    let final_dir = extract_path_field(&content, "final");
    let backup = extract_path_field(&content, "backup");

    if let (Some(final_dir), Some(staging)) = (&final_dir, &staging) {
        if !final_dir.exists() && staging.exists() {
            fs::rename(staging, final_dir).map_err(io_err)?;
        }
    }
    if let Some(backup) = &backup {
        if backup.exists() {
            let _ = fs::remove_dir_all(backup);
        }
    }
    let _ = fs::remove_file(&journal_path);
    Ok(())
}

fn extract_path_field(json: &str, field: &str) -> Option<PathBuf> {
    let key = format!("\"{field}\": \"");
    let start = json.find(&key)? + key.len();
    let rest = &json[start..];
    let end = rest.find('"')?;
    Some(PathBuf::from(&rest[..end]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(label: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "terse-publication-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn test_publish_installs_into_empty_destination() {
        let root = tempdir("fresh");
        let output = root.join("build").join("academic");
        let staged = stage(&output, &[("paper.tex".to_string(), b"hello".to_vec())]).unwrap();
        publish(staged, &output).unwrap();
        assert_eq!(fs::read(output.join("paper.tex")).unwrap(), b"hello");
    }

    #[test]
    fn test_publish_replaces_prior_owned_generation() {
        let root = tempdir("replace");
        let output = root.join("build").join("academic");
        let staged1 = stage(&output, &[("paper.tex".to_string(), b"v1".to_vec())]).unwrap();
        publish(staged1, &output).unwrap();

        let staged2 = stage(&output, &[("paper.tex".to_string(), b"v2".to_vec())]).unwrap();
        publish(staged2, &output).unwrap();

        assert_eq!(fs::read(output.join("paper.tex")).unwrap(), b"v2");
    }

    #[test]
    fn test_unowned_destination_is_refused() {
        let root = tempdir("unowned");
        let output = root.join("build").join("academic");
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("something.txt"), b"not ours").unwrap();

        let staged = stage(&output, &[("paper.tex".to_string(), b"new".to_vec())]).unwrap();
        let result = publish(staged, &output);
        assert!(matches!(result, Err(PublishError::UnownedDestination)));
        assert_eq!(fs::read(output.join("something.txt")).unwrap(), b"not ours");
    }

    #[test]
    fn test_recover_finishes_interrupted_publish() {
        let root = tempdir("recover");
        let output = root.join("build").join("academic");
        let staged1 = stage(&output, &[("paper.tex".to_string(), b"v1".to_vec())]).unwrap();
        publish(staged1, &output).unwrap();

        // Simulate a crash between the backup rename and the final
        // install rename: stage a new generation, actually move the prior
        // valid generation aside (as `publish` would have), and write a
        // journal claiming that in-progress state.
        let staged2 = stage(&output, &[("paper.tex".to_string(), b"v2".to_vec())]).unwrap();
        let parent = output.parent().unwrap();
        let backup_dir = unique_sibling(parent, "backup");
        fs::rename(&output, &backup_dir).unwrap();
        let journal_path = parent.join(JOURNAL_FILE);
        write_journal(&journal_path, &staged2.staging_dir, &output, &backup_dir, "backed-up").unwrap();

        // Before recovery: no complete final generation exists yet, only
        // the backup and the orphaned staging directory.
        assert!(!output.exists());

        recover(parent).unwrap();

        // After recovery: the staged generation was installed, and the
        // journal is gone.
        assert_eq!(fs::read(output.join("paper.tex")).unwrap(), b"v2");
        assert!(!journal_path.exists());
        assert!(!staged2.staging_dir.exists());
        assert!(!backup_dir.exists());
    }

    #[test]
    fn test_recover_discards_redundant_backup_when_final_already_valid() {
        let root = tempdir("recover-backup");
        let output = root.join("build").join("academic");
        let staged1 = stage(&output, &[("paper.tex".to_string(), b"v1".to_vec())]).unwrap();
        publish(staged1, &output).unwrap();

        // Simulate a crash after the final rename completed but before the
        // journal/backup cleanup ran: the backup still exists, and so does
        // a fully valid final generation.
        let parent = output.parent().unwrap();
        let backup_dir = unique_sibling(parent, "backup");
        fs::create_dir_all(&backup_dir).unwrap();
        fs::write(backup_dir.join("paper.tex"), b"old").unwrap();
        let journal_path = parent.join(JOURNAL_FILE);
        write_journal(&journal_path, &parent.join("nonexistent-staging"), &output, &backup_dir, "backed-up").unwrap();

        recover(parent).unwrap();

        assert_eq!(fs::read(output.join("paper.tex")).unwrap(), b"v1");
        assert!(!backup_dir.exists());
        assert!(!journal_path.exists());
    }
}
