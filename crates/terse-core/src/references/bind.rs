//! Offline citation binding: combines source `refs:` declarations, the
//! decoded lock file, and any override patches into the set of aliases a
//! citation is allowed to reference, plus their effective metadata.
//!
//! This module never contacts a provider — it only consumes already
//! loaded/decoded data, so it stays in `terse-core`.

use std::collections::{BTreeMap, HashMap};

use super::lock::{LockFile, ProviderKind};
use super::overrides::{apply_override, OverridePatch};
use super::record::{normalize_doi, NormalizedRecord};
use crate::diagnostic::Diagnostic;
use crate::syntax::blocks::{RefEntry, RefKind};

/// The outcome of binding every declared alias against the lock/overrides:
/// aliases with a valid, current binding are authorized to be cited, with
/// their effective metadata available for bibliography generation.
/// Declared-but-unresolved aliases are reported separately so the caller
/// can decide whether an actual citation makes that an error or a warning.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bindings {
    pub authorized: BTreeMap<String, NormalizedRecord>,
    /// Declared, supported (DOI/arXiv) aliases with no matching lock entry
    /// yet — not an error by themselves, but a citation of one is.
    pub unresolved: Vec<String>,
}

fn declared_identifier(entry: &RefEntry) -> Result<(ProviderKind, String), Diagnostic> {
    match entry.kind {
        RefKind::Doi => {
            let normalized = normalize_doi(&entry.identifier).map_err(|msg| {
                Diagnostic::error("E-REF-005", format!("declared DOI for '{}' is invalid: {msg}", entry.alias), entry.span)
            })?;
            Ok((ProviderKind::Doi, normalized))
        }
        RefKind::Arxiv => Ok((ProviderKind::Arxiv, entry.identifier.trim().to_string())),
        RefKind::Isbn | RefKind::Url => unreachable!("reserved kinds are rejected before this point"),
    }
}

fn reserved_provider_error(entry: &RefEntry) -> Diagnostic {
    let provider = match entry.kind {
        RefKind::Isbn => "isbn",
        RefKind::Url => "generic url",
        _ => unreachable!("only called for reserved kinds"),
    };
    Diagnostic::error(
        "E-REF-003",
        format!("'{}' declares a {provider} reference, which is not a supported provider in this version", entry.alias),
        entry.span,
    )
}

/// Classifies one declaration: reserved provider kinds (ISBN, generic
/// URL) always fail, whether or not the alias is ever cited, since this
/// version cannot resolve them at all. Used by `refs resolve` (group 15)
/// to reject reserved declarations up front, before attempting any
/// fetch, and by [`bind`] itself.
pub fn classify_declaration(entry: &RefEntry) -> Result<(ProviderKind, String), Diagnostic> {
    if matches!(entry.kind, RefKind::Isbn | RefKind::Url) {
        return Err(reserved_provider_error(entry));
    }
    declared_identifier(entry)
}

/// Binds every declared reference alias. Reserved provider kinds (ISBN,
/// generic URL) fail immediately, whether or not they are ever cited — the
/// MVP does not support resolving them at all. Declared identifiers that
/// no longer match their lock entry fail with stale-binding guidance.
/// Unsealed overrides (a patch that differs from the one sealed into the
/// lock at resolution time) also fail rather than being silently applied.
pub fn bind(
    declarations: &HashMap<String, RefEntry>,
    lock: &LockFile,
    overrides: &HashMap<String, OverridePatch>,
) -> Result<Bindings, Diagnostic> {
    let mut aliases: Vec<&String> = declarations.keys().collect();
    aliases.sort();

    let mut out = Bindings::default();

    for alias in aliases {
        let entry = &declarations[alias];
        let (provider, identifier) = classify_declaration(entry)?;

        let Some(locked) = lock.entries.get(alias) else {
            out.unresolved.push(alias.clone());
            continue;
        };

        if locked.declared.provider != provider || locked.declared.identifier != identifier {
            return Err(Diagnostic::error(
                "E-REF-004",
                format!(
                    "'{alias}' now declares '{identifier}', but the lock was resolved for '{}'; run 'terse refs resolve' to update it",
                    locked.declared.identifier
                ),
                entry.span,
            ));
        }

        let current_patch = overrides.get(alias).cloned().unwrap_or_default();
        if current_patch != locked.override_patch.clone().unwrap_or_default() {
            return Err(Diagnostic::error(
                "E-REF-006",
                format!("'{alias}' has an override that was changed after resolution; run 'terse refs resolve' to reseal it"),
                entry.span,
            ));
        }

        // Recompute rather than trust the stored effective record, so a
        // resolve-time bug can never silently diverge from what a fresh
        // apply would produce; this must be a no-op given the patch match
        // above.
        let effective = apply_override(&locked.provider_data, &current_patch)
            .map_err(|_| {
                Diagnostic::error(
                    "E-REF-006",
                    format!("'{alias}' has a sealed override that no longer produces valid effective metadata"),
                    entry.span,
                )
            })?;

        out.authorized.insert(alias.clone(), effective);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::references::lock::{AdapterIdentity, DeclaredIdentity, LockEntry, ResolvedIdentity};
    use crate::references::record::WorkType;
    use crate::source::{FileId, SourceSpan};

    fn span() -> SourceSpan {
        SourceSpan::new(FileId(0), 0, 1)
    }

    fn entry(alias: &str, kind: RefKind, identifier: &str) -> RefEntry {
        RefEntry {
            alias: alias.to_string(),
            kind,
            identifier: identifier.to_string(),
            span: span(),
        }
    }

    fn locked_entry(identifier: &str) -> LockEntry {
        let record = NormalizedRecord {
            title: Some("T".to_string()),
            work_type: Some(WorkType::JournalArticle),
            authors: vec![],
            anonymous: true,
            ..Default::default()
        };
        LockEntry {
            declared: DeclaredIdentity { provider: ProviderKind::Doi, identifier: identifier.to_string() },
            resolved: ResolvedIdentity { provider: ProviderKind::Doi, identifier: identifier.to_string(), version: None },
            adapter: AdapterIdentity { name: "doi".to_string(), version: "1".to_string() },
            provider_data: record.clone(),
            override_patch: None,
            effective: record,
        }
    }

    #[test]
    fn reserved_provider_fails_even_if_unused() {
        let mut declarations = HashMap::new();
        declarations.insert("book".to_string(), entry("book", RefKind::Isbn, "978-0-13-468599-1"));
        let err = bind(&declarations, &LockFile::default(), &HashMap::new()).unwrap_err();
        assert_eq!(err.code, "E-REF-003");
    }

    #[test]
    fn matching_lock_authorizes_alias() {
        let mut declarations = HashMap::new();
        declarations.insert("robins1986".to_string(), entry("robins1986", RefKind::Doi, "10.1/X"));
        let mut lock = LockFile::default();
        lock.entries.insert("robins1986".to_string(), locked_entry("10.1/x"));
        let bindings = bind(&declarations, &lock, &HashMap::new()).unwrap();
        assert!(bindings.authorized.contains_key("robins1986"));
        assert!(bindings.unresolved.is_empty());
    }

    #[test]
    fn unresolved_declaration_is_not_an_error_by_itself() {
        let mut declarations = HashMap::new();
        declarations.insert("robins1986".to_string(), entry("robins1986", RefKind::Doi, "10.1/x"));
        let bindings = bind(&declarations, &LockFile::default(), &HashMap::new()).unwrap();
        assert_eq!(bindings.unresolved, vec!["robins1986".to_string()]);
    }

    #[test]
    fn changed_identifier_fails() {
        let mut declarations = HashMap::new();
        declarations.insert("robins1986".to_string(), entry("robins1986", RefKind::Doi, "10.1/new"));
        let mut lock = LockFile::default();
        lock.entries.insert("robins1986".to_string(), locked_entry("10.1/old"));
        let err = bind(&declarations, &lock, &HashMap::new()).unwrap_err();
        assert_eq!(err.code, "E-REF-004");
    }

    #[test]
    fn unsealed_override_fails() {
        let mut declarations = HashMap::new();
        declarations.insert("robins1986".to_string(), entry("robins1986", RefKind::Doi, "10.1/x"));
        let mut lock = LockFile::default();
        lock.entries.insert("robins1986".to_string(), locked_entry("10.1/x"));
        let mut overrides = HashMap::new();
        overrides.insert(
            "robins1986".to_string(),
            OverridePatch { title: Some(super::super::overrides::FieldPatch::Set("New".to_string())), ..Default::default() },
        );
        let err = bind(&declarations, &lock, &overrides).unwrap_err();
        assert_eq!(err.code, "E-REF-006");
    }
}
