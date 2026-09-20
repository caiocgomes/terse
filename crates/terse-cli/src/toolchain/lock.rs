//! The managed prefix's lock: what was installed, from where, at which
//! revisions. Field order is the struct order; there are no timestamps, so
//! two installs of the same pins produce identical bytes.

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const LOCK_FILE_NAME: &str = "terse-toolchain.lock.json";
pub const LOCK_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedPackage {
    pub name: String,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolchainLock {
    pub version: u32,
    pub texlive_year: String,
    pub repository: String,
    pub install_tl_sha512: String,
    pub packages: Vec<LockedPackage>,
    pub terse_version: String,
    pub platform: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockError {
    Malformed(String),
    UnsupportedVersion(u32),
}

impl ToolchainLock {
    pub fn encode(&self) -> String {
        let mut sorted = self.clone();
        sorted.packages.sort_by(|a, b| a.name.cmp(&b.name));
        let mut text = serde_json::to_string_pretty(&sorted).expect("lock serializes");
        text.push('\n');
        text
    }

    pub fn decode(text: &str) -> Result<Self, LockError> {
        let lock: ToolchainLock = serde_json::from_str(text).map_err(|e| LockError::Malformed(e.to_string()))?;
        if lock.version != LOCK_VERSION {
            return Err(LockError::UnsupportedVersion(lock.version));
        }
        Ok(lock)
    }

    /// Reads `<prefix>/terse-toolchain.lock.json`; `None` when the file is
    /// absent or unreadable, `Some(Err)` when it exists but is invalid.
    pub fn read_from_prefix(prefix: &Path) -> Option<Result<Self, LockError>> {
        let text = std::fs::read_to_string(prefix.join(LOCK_FILE_NAME)).ok()?;
        Some(Self::decode(&text))
    }
}
