//! Normalized reference records: the Terse-owned schema that provider
//! adapters (group 14) map into, and that the lock file (group 13) stores.
//!
//! This module is pure data + string normalization. It performs no I/O
//! and knows nothing about DOI/arXiv HTTP protocols.

use serde::{Deserialize, Serialize};

/// A person or organization credited on a work. Distinguishing these three
/// forms means a provider that only supplies a full name string is never
/// silently guessed into family/given parts, and organizations are never
/// mistaken for unparsed people.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "form", rename_all = "kebab-case")]
pub enum PersonName {
    /// Family/given name parts, as explicitly separated by the provider.
    Structured { family: String, given: String },
    /// A single full name string the provider did not decompose.
    Unparsed { name: String },
    /// A corporate/organizational author or editor.
    Organization { name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkType {
    JournalArticle,
    ConferencePaper,
    Preprint,
    Book,
    BookChapter,
    Report,
    Thesis,
    Dataset,
    Software,
    Misc,
}

/// The complete normalized, provider-independent metadata for one work.
/// `Option` fields are genuinely optional in the schema (e.g. a date may
/// be absent); required effective fields (title, work type, and at least
/// one of authors/editors/organization or an explicit `anonymous` marker)
/// are enforced by [`NormalizedRecord::validate_effective`], not by the
/// type itself, so a provider record may be incomplete before an override
/// fills the gap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedRecord {
    pub title: Option<String>,
    pub work_type: Option<WorkType>,
    #[serde(default)]
    pub authors: Vec<PersonName>,
    #[serde(default)]
    pub editors: Vec<PersonName>,
    /// An explicit marker that this work has no credited authors/editors,
    /// distinct from "the provider simply didn't say" (empty vectors with
    /// `anonymous: false`), which fails validation instead of silently
    /// rendering as anonymous.
    #[serde(default)]
    pub anonymous: bool,
    pub container: Option<String>,
    /// ISO 8601 date string, kept as provided; a missing date stays
    /// `None` rather than being invented.
    pub date: Option<String>,
    pub publisher: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub pages: Option<String>,
}

impl Default for NormalizedRecord {
    fn default() -> Self {
        NormalizedRecord {
            title: None,
            work_type: None,
            authors: Vec::new(),
            editors: Vec::new(),
            anonymous: false,
            container: None,
            date: None,
            publisher: None,
            volume: None,
            issue: None,
            pages: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectiveError {
    MissingTitle,
    MissingWorkType,
    MissingContributors,
}

impl NormalizedRecord {
    /// Checks the required-effective-field contract: title, work type, and
    /// (authors, editors, an organization contributor, or the explicit
    /// `anonymous` marker).
    pub fn validate_effective(&self) -> Result<(), EffectiveError> {
        if self.title.is_none() {
            return Err(EffectiveError::MissingTitle);
        }
        if self.work_type.is_none() {
            return Err(EffectiveError::MissingWorkType);
        }
        if self.authors.is_empty() && self.editors.is_empty() && !self.anonymous {
            return Err(EffectiveError::MissingContributors);
        }
        Ok(())
    }
}

/// Normalizes a DOI to its canonical lowercase form, stripping any
/// `doi:`, `https://doi.org/`, or `http://dx.doi.org/` prefix and
/// surrounding whitespace. Case-folds the whole identifier: DOI registrant
/// codes and suffixes are conventionally treated case-insensitively.
pub fn normalize_doi(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    let without_prefix = ["https://doi.org/", "http://doi.org/", "http://dx.doi.org/", "https://dx.doi.org/", "doi:"]
        .iter()
        .find(|prefix| lower.starts_with(*prefix))
        .map(|prefix| &trimmed[prefix.len()..])
        .unwrap_or(trimmed);
    let candidate = without_prefix.trim().to_ascii_lowercase();
    if !candidate.starts_with("10.") || !candidate.contains('/') {
        return Err(format!("'{raw}' is not a well-formed DOI"));
    }
    Ok(candidate)
}

#[cfg(test)]
mod normalize_tests {
    use super::*;

    #[test]
    fn strips_prefixes_and_case_folds() {
        assert_eq!(normalize_doi("10.1000/ABC").unwrap(), "10.1000/abc");
        assert_eq!(normalize_doi("DOI:10.1000/ABC").unwrap(), "10.1000/abc");
        assert_eq!(normalize_doi(" https://doi.org/10.1000/ABC ").unwrap(), "10.1000/abc");
    }

    #[test]
    fn rejects_malformed_doi() {
        assert!(normalize_doi("not-a-doi").is_err());
    }
}
