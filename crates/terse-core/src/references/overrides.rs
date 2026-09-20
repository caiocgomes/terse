//! `references.overrides.toml` schema and patching.
//!
//! An override patch is applied to a provider's [`NormalizedRecord`] to
//! produce the effective record. Patches replace list fields wholesale
//! (never merge element-by-element) and can explicitly remove an optional
//! field. Identity fields (the DOI/arXiv identifier and provider kind) are
//! simply absent from this schema, so they can never be overridden; a
//! patch that would leave a required effective field unset fails at
//! apply time rather than silently producing an incomplete record.

use serde::{Deserialize, Serialize};

use super::record::{NormalizedRecord, PersonName, WorkType};

/// One field's patch operation: replace it, or remove it (only valid for
/// optional fields).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", content = "value", rename_all = "kebab-case")]
pub enum FieldPatch<T> {
    Set(T),
    Remove,
}

/// The complete override schema for one alias. Every field is optional:
/// an absent field leaves the provider's value untouched. List fields
/// (`authors`, `editors`) are replaced as a whole array, never merged.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct OverridePatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<FieldPatch<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_type: Option<FieldPatch<WorkType>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authors: Option<FieldPatch<Vec<PersonName>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editors: Option<FieldPatch<Vec<PersonName>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anonymous: Option<FieldPatch<bool>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<FieldPatch<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<FieldPatch<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<FieldPatch<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<FieldPatch<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue: Option<FieldPatch<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<FieldPatch<String>>,
}

/// Decodes the optional `references.overrides.toml` file: a top-level
/// table keyed by alias, each value an [`OverridePatch`].
pub fn decode_file(text: &str) -> Result<std::collections::HashMap<String, OverridePatch>, String> {
    toml::from_str(text).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverrideError {
    /// Applying the patch would leave a required effective field unset
    /// (title, work type, or every contributor field with no explicit
    /// `anonymous` marker).
    RequiredFieldRemoved(&'static str),
}

fn apply_field<T: Clone>(base: &Option<T>, patch: &Option<FieldPatch<T>>) -> Option<T> {
    match patch {
        None => base.clone(),
        Some(FieldPatch::Set(value)) => Some(value.clone()),
        Some(FieldPatch::Remove) => None,
    }
}

fn apply_list_field<T: Clone>(base: &[T], patch: &Option<FieldPatch<Vec<T>>>) -> Vec<T> {
    match patch {
        None => base.to_vec(),
        Some(FieldPatch::Set(values)) => values.clone(),
        Some(FieldPatch::Remove) => Vec::new(),
    }
}

/// Applies an override patch to a provider record, producing the
/// effective record. Fails if the result would be missing a required
/// effective field (title, work type, or every contributor with no
/// `anonymous` marker) — this is how "identity/required-field removal
/// MUST fail" and "unsupported override" are enforced, without needing a
/// separate protected-field list.
pub fn apply_override(base: &NormalizedRecord, patch: &OverridePatch) -> Result<NormalizedRecord, OverrideError> {
    let title = apply_field(&base.title, &patch.title);
    let work_type = apply_field(&base.work_type, &patch.work_type);
    let authors = apply_list_field(&base.authors, &patch.authors);
    let editors = apply_list_field(&base.editors, &patch.editors);
    let anonymous = apply_field(&Some(base.anonymous), &patch.anonymous).unwrap_or(base.anonymous);

    if title.is_none() {
        return Err(OverrideError::RequiredFieldRemoved("title"));
    }
    if work_type.is_none() {
        return Err(OverrideError::RequiredFieldRemoved("work_type"));
    }
    if authors.is_empty() && editors.is_empty() && !anonymous {
        return Err(OverrideError::RequiredFieldRemoved("authors/editors/anonymous"));
    }

    Ok(NormalizedRecord {
        title,
        work_type,
        authors,
        editors,
        anonymous,
        container: apply_field(&base.container, &patch.container),
        date: apply_field(&base.date, &patch.date),
        publisher: apply_field(&base.publisher, &patch.publisher),
        volume: apply_field(&base.volume, &patch.volume),
        issue: apply_field(&base.issue, &patch.issue),
        pages: apply_field(&base.pages, &patch.pages),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_record() -> NormalizedRecord {
        NormalizedRecord {
            title: Some("Original Title".to_string()),
            work_type: Some(WorkType::JournalArticle),
            authors: vec![PersonName::Unparsed { name: "A. Author".to_string() }],
            ..Default::default()
        }
    }

    #[test]
    fn set_replaces_and_remove_clears() {
        let patch = OverridePatch {
            title: Some(FieldPatch::Set("Corrected Title".to_string())),
            date: Some(FieldPatch::Remove),
            ..Default::default()
        };
        let effective = apply_override(&base_record(), &patch).unwrap();
        assert_eq!(effective.title.as_deref(), Some("Corrected Title"));
        assert_eq!(effective.date, None);
    }

    #[test]
    fn whole_array_replacement_not_merge() {
        let patch = OverridePatch {
            authors: Some(FieldPatch::Set(vec![PersonName::Organization { name: "Org".to_string() }])),
            ..Default::default()
        };
        let effective = apply_override(&base_record(), &patch).unwrap();
        assert_eq!(effective.authors.len(), 1);
        assert_eq!(effective.authors[0], PersonName::Organization { name: "Org".to_string() });
    }

    #[test]
    fn removing_required_title_fails() {
        let patch = OverridePatch {
            title: Some(FieldPatch::Remove),
            ..Default::default()
        };
        assert_eq!(
            apply_override(&base_record(), &patch).unwrap_err(),
            OverrideError::RequiredFieldRemoved("title")
        );
    }

    #[test]
    fn removing_only_contributors_without_anonymous_fails() {
        let patch = OverridePatch {
            authors: Some(FieldPatch::Remove),
            ..Default::default()
        };
        assert_eq!(
            apply_override(&base_record(), &patch).unwrap_err(),
            OverrideError::RequiredFieldRemoved("authors/editors/anonymous")
        );
    }

    #[test]
    fn removing_contributors_with_explicit_anonymous_succeeds() {
        let patch = OverridePatch {
            authors: Some(FieldPatch::Remove),
            anonymous: Some(FieldPatch::Set(true)),
            ..Default::default()
        };
        assert!(apply_override(&base_record(), &patch).is_ok());
    }
}
