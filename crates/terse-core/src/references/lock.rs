//! Versioned `references.lock` decoding/encoding.
//!
//! Pure data layer: this module parses lock TOML from a string and
//! serializes it back to a string. Actual file I/O (reading/writing
//! `references.lock` on disk) belongs to `terse-cli` (group 15).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::overrides::OverridePatch;
use super::record::NormalizedRecord;
use crate::syntax::blocks::RefKind;

pub const LOCK_VERSION: i64 = 1;
pub const NORMALIZATION_VERSION: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    Doi,
    Arxiv,
}

impl From<RefKind> for Option<ProviderKind> {
    fn from(kind: RefKind) -> Self {
        match kind {
            RefKind::Doi => Some(ProviderKind::Doi),
            RefKind::Arxiv => Some(ProviderKind::Arxiv),
            RefKind::Isbn | RefKind::Url => None,
        }
    }
}

/// The declared identifier for an alias, as the source `refs:` block
/// stated it (post-normalization for DOIs, verbatim for arXiv ids).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredIdentity {
    pub provider: ProviderKind,
    pub identifier: String,
}

/// The exact identity/version the provider actually resolved, which may
/// differ from the declared identifier only in an arXiv version suffix
/// (a versionless declaration pins to the exact version resolved).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedIdentity {
    pub provider: ProviderKind,
    pub identifier: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterIdentity {
    pub name: String,
    pub version: String,
}

/// One lock entry: everything needed to bind a citation offline without
/// contacting a provider again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LockEntry {
    pub declared: DeclaredIdentity,
    pub resolved: ResolvedIdentity,
    pub adapter: AdapterIdentity,
    pub provider_data: NormalizedRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_patch: Option<OverridePatch>,
    pub effective: NormalizedRecord,
}

/// The complete decoded `references.lock` file. Alias entries are kept in
/// a `BTreeMap` so serialization is always alias-sorted, independent of
/// resolution order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LockFile {
    #[serde(rename = "lock-version")]
    pub lock_version: i64,
    #[serde(rename = "normalization-version")]
    pub normalization_version: i64,
    #[serde(default)]
    pub entries: BTreeMap<String, LockEntry>,
}

impl Default for LockFile {
    fn default() -> Self {
        LockFile {
            lock_version: LOCK_VERSION,
            normalization_version: NORMALIZATION_VERSION,
            entries: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockError {
    Toml(String),
    UnsupportedLockVersion(i64),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::Toml(msg) => write!(f, "invalid references.lock: {msg}"),
            LockError::UnsupportedLockVersion(v) => write!(f, "unsupported references.lock version {v}"),
        }
    }
}

/// Decodes a lock file from TOML text, rejecting unsupported lock
/// versions without attempting to reinterpret their contents.
pub fn decode(text: &str) -> Result<LockFile, LockError> {
    let lock: LockFile = toml::from_str(text).map_err(|e| LockError::Toml(e.to_string()))?;
    if lock.lock_version != LOCK_VERSION {
        return Err(LockError::UnsupportedLockVersion(lock.lock_version));
    }
    Ok(lock)
}

/// Serializes a lock file to canonical TOML: deterministic key order
/// (alias-sorted via `BTreeMap`, fixed field order via struct declaration
/// order), and no volatile fields (no timestamps, no machine paths).
pub fn encode(lock: &LockFile) -> String {
    toml::to_string_pretty(lock).expect("LockFile serializes without error")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsupported_lock_version() {
        let text = "lock-version = 99\nnormalization-version = 1\n";
        assert_eq!(decode(text).unwrap_err(), LockError::UnsupportedLockVersion(99));
    }

    #[test]
    fn empty_lock_round_trips() {
        let lock = LockFile::default();
        let text = encode(&lock);
        let decoded = decode(&text).unwrap();
        assert_eq!(decoded, lock);
    }
}
