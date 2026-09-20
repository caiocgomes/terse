//! `refs resolve`: the only command that fetches reference metadata.
//!
//! Declarations are parsed and their project-wide alias namespace is
//! collected *before* requiring valid citation bindings (unlike ordinary
//! `check`/`build`, which fail on an unresolved citation) — resolving is
//! exactly how an unresolved declaration becomes resolvable. Every
//! requested record is normalized and validated in memory first; the
//! on-disk lock is only replaced after everything succeeds, and is left
//! completely untouched on any failure.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use terse_core::project::symbols::ProjectSymbols;
use terse_core::references::bind;
use terse_core::references::lock::{
    AdapterIdentity, DeclaredIdentity, LockEntry, LockFile, ProviderKind, ResolvedIdentity,
};
use terse_core::references::overrides::{apply_override, OverridePatch};
use terse_core::references::record::NormalizedRecord;
use terse_core::syntax::blocks::RefEntry;

use crate::project::{self, ProjectError, LOCK_FILE_NAME};

use super::arxiv::{ArxivResolveError, ArxivWorker};
use super::clock::Clock;
use super::doi::{DoiResolveError, self as doi_adapter};
use super::transport::MetadataTransport;

const DOI_ADAPTER_VERSION: &str = "1";
const ARXIV_ADAPTER_VERSION: &str = "1";

#[derive(Debug, Clone, Copy, Default)]
pub struct ResolveOptions {
    pub refresh: bool,
    pub offline: bool,
    pub prune: bool,
}

#[derive(Debug)]
pub enum ResolveError {
    /// `--refresh` and `--offline` were both given.
    ConflictingFlags,
    Project(ProjectError),
    /// A declaration failed to classify (reserved provider, malformed
    /// DOI) or the project-wide alias/id namespaces collided.
    Declaration(String),
    /// `--offline` was given but `alias` has no usable existing record
    /// (no lock entry yet, or its declared identifier changed).
    OfflineUnresolvable(String),
    Doi(String, DoiResolveError),
    Arxiv(String, ArxivResolveError),
    Override(String, String),
    /// The staged lock failed its own internal consistency re-check
    /// (should not happen if the staging logic above is correct; treated
    /// as a hard abort rather than ever writing a lock that would not
    /// itself validate).
    StagedLockInvalid(String),
    Io(String),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::ConflictingFlags => write!(f, "--refresh and --offline cannot be used together"),
            ResolveError::Project(e) => write!(f, "{e:?}"),
            ResolveError::Declaration(msg) => write!(f, "{msg}"),
            ResolveError::OfflineUnresolvable(alias) => {
                write!(f, "'{alias}' cannot be resolved offline: no matching existing record; run without --offline")
            }
            ResolveError::Doi(alias, e) => write!(f, "resolving '{alias}': {e}"),
            ResolveError::Arxiv(alias, e) => write!(f, "resolving '{alias}': {e}"),
            ResolveError::Override(alias, msg) => write!(f, "'{alias}' override: {msg}"),
            ResolveError::StagedLockInvalid(msg) => write!(f, "internal error: staged lock failed validation: {msg}"),
            ResolveError::Io(msg) => write!(f, "{msg}"),
        }
    }
}

/// The outcome of a successful resolution run, for reporting.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolveReport {
    pub fetched: Vec<String>,
    pub unchanged: Vec<String>,
    pub resealed: Vec<String>,
    pub pruned: Vec<String>,
}

fn provider_str(p: ProviderKind) -> &'static str {
    match p {
        ProviderKind::Doi => "doi",
        ProviderKind::Arxiv => "arxiv",
    }
}

/// True when `entry`'s already-locked identity and current override
/// patch exactly match what is declared/configured now, so no fetch is
/// needed unless `--refresh` forces one.
fn is_unchanged(
    provider: ProviderKind,
    identifier: &str,
    current_patch: &OverridePatch,
    locked: &LockEntry,
) -> bool {
    locked.declared.provider == provider
        && locked.declared.identifier == identifier
        && locked.override_patch.clone().unwrap_or_default() == *current_patch
}

fn seal_patch(patch: OverridePatch) -> Option<OverridePatch> {
    if patch == OverridePatch::default() {
        None
    } else {
        Some(patch)
    }
}

fn fetch_record(
    alias: &str,
    provider: ProviderKind,
    identifier: &str,
    transport: &mut dyn MetadataTransport,
    clock: &mut dyn Clock,
    arxiv_worker: &mut ArxivWorker,
) -> Result<(ResolvedIdentity, AdapterIdentity, NormalizedRecord), ResolveError> {
    match provider {
        ProviderKind::Doi => {
            let record = doi_adapter::resolve_doi(transport, identifier)
                .map_err(|e| ResolveError::Doi(alias.to_string(), e))?;
            Ok((
                ResolvedIdentity { provider, identifier: identifier.to_string(), version: None },
                AdapterIdentity { name: "doi".to_string(), version: DOI_ADAPTER_VERSION.to_string() },
                record,
            ))
        }
        ProviderKind::Arxiv => {
            let resolution = arxiv_worker
                .resolve(transport, clock, identifier)
                .map_err(|e| ResolveError::Arxiv(alias.to_string(), e))?;
            Ok((
                ResolvedIdentity {
                    provider,
                    identifier: resolution.resolved_id,
                    version: resolution.resolved_version,
                },
                AdapterIdentity { name: "arxiv".to_string(), version: ARXIV_ADAPTER_VERSION.to_string() },
                resolution.record,
            ))
        }
    }
}

/// Resolves every declared reference alias in the project rooted at
/// `entry`'s discovered manifest, per `options`. Every requested record
/// is staged and validated in memory before the on-disk lock is ever
/// touched: on any failure, the previous `references.lock` is left
/// completely unchanged.
pub fn run(
    root: &Path,
    entry_path: &Path,
    options: ResolveOptions,
    transport: &mut dyn MetadataTransport,
    clock: &mut dyn Clock,
) -> Result<ResolveReport, ResolveError> {
    if options.refresh && options.offline {
        return Err(ResolveError::ConflictingFlags);
    }

    let loaded = project::load_modules(root, entry_path).map_err(ResolveError::Project)?;
    let (_expanded, symbols): (_, ProjectSymbols) =
        terse_core::collect_declarations(&loaded.snapshot).map_err(|d| ResolveError::Declaration(d.message))?;

    let declarations: &HashMap<String, RefEntry> = &symbols.reference_aliases;

    // Classify every declaration first: a reserved provider fails the
    // whole run immediately, even if it would otherwise be unused, and
    // even before any fetch is attempted for other aliases.
    let mut classified: HashMap<String, (ProviderKind, String)> = HashMap::new();
    for (alias, entry) in declarations {
        let (provider, identifier) =
            bind::classify_declaration(entry).map_err(|d| ResolveError::Declaration(d.message))?;
        classified.insert(alias.clone(), (provider, identifier));
    }

    let old_lock = loaded.snapshot.lock.clone().unwrap_or_default();
    let overrides = &loaded.snapshot.overrides;

    let mut staged_entries: HashMap<String, LockEntry> = HashMap::new();
    let mut report = ResolveReport::default();
    let mut arxiv_worker = ArxivWorker::new();

    let mut aliases: Vec<&String> = declarations.keys().collect();
    aliases.sort();

    for alias in aliases {
        let (provider, identifier) = classified[alias].clone();
        let current_patch = overrides.get(alias).cloned().unwrap_or_default();
        let existing = old_lock.entries.get(alias);

        let unchanged = !options.refresh
            && existing.is_some_and(|locked| is_unchanged(provider, &identifier, &current_patch, locked));

        if unchanged {
            staged_entries.insert(alias.clone(), existing.unwrap().clone());
            report.unchanged.push(alias.clone());
            continue;
        }

        if options.offline {
            let Some(locked) = existing else {
                return Err(ResolveError::OfflineUnresolvable(alias.clone()));
            };
            if locked.declared.provider != provider || locked.declared.identifier != identifier {
                return Err(ResolveError::OfflineUnresolvable(alias.clone()));
            }
            let effective = apply_override(&locked.provider_data, &current_patch)
                .map_err(|e| ResolveError::Override(alias.clone(), format!("{e:?}")))?;
            staged_entries.insert(
                alias.clone(),
                LockEntry {
                    declared: locked.declared.clone(),
                    resolved: locked.resolved.clone(),
                    adapter: locked.adapter.clone(),
                    provider_data: locked.provider_data.clone(),
                    override_patch: seal_patch(current_patch),
                    effective,
                },
            );
            report.resealed.push(alias.clone());
            continue;
        }

        let (resolved, adapter, record) =
            fetch_record(alias, provider, &identifier, transport, clock, &mut arxiv_worker)?;
        let effective = apply_override(&record, &current_patch)
            .map_err(|e| ResolveError::Override(alias.clone(), format!("{e:?}")))?;
        staged_entries.insert(
            alias.clone(),
            LockEntry {
                declared: DeclaredIdentity { provider, identifier },
                resolved,
                adapter,
                provider_data: record,
                override_patch: seal_patch(current_patch),
                effective,
            },
        );
        report.fetched.push(alias.clone());
    }

    // Orphans: locked aliases no longer declared. Retained unless
    // `--prune` is explicit.
    for (alias, entry) in &old_lock.entries {
        if declarations.contains_key(alias) {
            continue;
        }
        if options.prune {
            report.pruned.push(alias.clone());
        } else {
            staged_entries.insert(alias.clone(), entry.clone());
        }
    }

    let mut new_lock = LockFile::default();
    new_lock.entries = staged_entries.into_iter().collect();

    // Re-validate the complete staged lock before ever touching disk: a
    // fresh `bind()` over exactly what will be written must authorize
    // every non-orphan declared alias with no error.
    bind::bind(declarations, &new_lock, overrides).map_err(|d| ResolveError::StagedLockInvalid(d.message))?;

    write_lock_atomically(root, &new_lock)?;

    report.fetched.sort();
    report.unchanged.sort();
    report.resealed.sort();
    report.pruned.sort();
    Ok(report)
}

fn write_lock_atomically(root: &Path, lock: &LockFile) -> Result<(), ResolveError> {
    let text = terse_core::references::lock::encode(lock);
    let final_path = root.join(LOCK_FILE_NAME);
    let tmp_path = root.join(format!(".{LOCK_FILE_NAME}.tmp-{}", std::process::id()));
    fs::write(&tmp_path, &text).map_err(|e| ResolveError::Io(e.to_string()))?;
    fs::rename(&tmp_path, &final_path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        ResolveError::Io(e.to_string())
    })?;
    Ok(())
}

pub fn provider_name_of(p: ProviderKind) -> &'static str {
    provider_str(p)
}
