//! Pure DOI CSL-JSON normalization: bytes in, [`NormalizedRecord`] out.
//!
//! This module performs no I/O and knows nothing about HTTP or content
//! negotiation — the `doi.org` request/response handling lives in
//! `terse-cli/src/references/doi.rs` (group 14's effectful half).

use serde::Deserialize;

use super::record::{NormalizedRecord, PersonName, WorkType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CslError {
    Json(String),
    /// The CSL record's own `DOI` field does not match the identifier the
    /// caller resolved, case-insensitively. This is the identity check
    /// required by 14.3: a provider must not silently substitute a
    /// different work than the one requested.
    IdentityMismatch { requested: String, returned: String },
}

impl std::fmt::Display for CslError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CslError::Json(msg) => write!(f, "invalid CSL-JSON: {msg}"),
            CslError::IdentityMismatch { requested, returned } => {
                write!(f, "resolved DOI '{returned}' does not match requested '{requested}'")
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct CslDate {
    #[serde(rename = "date-parts")]
    date_parts: Option<Vec<Vec<i64>>>,
}

#[derive(Debug, Deserialize)]
struct CslName {
    family: Option<String>,
    given: Option<String>,
    literal: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CslRecord {
    #[serde(rename = "DOI")]
    doi: Option<String>,
    title: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    author: Option<Vec<CslName>>,
    editor: Option<Vec<CslName>>,
    #[serde(rename = "container-title")]
    container_title: Option<String>,
    issued: Option<CslDate>,
    publisher: Option<String>,
    volume: Option<String>,
    issue: Option<String>,
    page: Option<String>,
}

/// Bounded, safe decoding of the small set of HTML entities registration
/// agencies sometimes leave in CSL-JSON title/author text (e.g. Crossref
/// occasionally emits `&amp;`). Only a fixed, non-recursive allowlist is
/// decoded; anything else (numeric entities beyond a small bound, unknown
/// named entities) is passed through unchanged rather than guessed at, and
/// there is no way for this to trigger arbitrary expansion.
fn decode_safe_entities(input: &str) -> String {
    const MAX_LEN: usize = 1_000_000;
    let input = if input.len() > MAX_LEN { &input[..MAX_LEN] } else { input };
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn map_name(name: CslName) -> PersonName {
    match (name.family, name.given, name.literal) {
        (Some(family), Some(given), _) => PersonName::Structured {
            family: decode_safe_entities(&family),
            given: decode_safe_entities(&given),
        },
        (_, _, Some(literal)) => PersonName::Organization { name: decode_safe_entities(&literal) },
        (Some(family), None, None) => PersonName::Unparsed { name: decode_safe_entities(&family) },
        (None, Some(given), None) => PersonName::Unparsed { name: decode_safe_entities(&given) },
        (None, None, None) => PersonName::Unparsed { name: String::new() },
    }
}

fn map_work_type(kind: Option<&str>) -> WorkType {
    match kind {
        Some("journal-article") => WorkType::JournalArticle,
        Some("paper-conference") => WorkType::ConferencePaper,
        Some("book") => WorkType::Book,
        Some("chapter") => WorkType::BookChapter,
        Some("report") => WorkType::Report,
        Some("thesis") => WorkType::Thesis,
        Some("dataset") => WorkType::Dataset,
        Some("software") => WorkType::Software,
        _ => WorkType::Misc,
    }
}

fn map_date(date: Option<CslDate>) -> Option<String> {
    let parts = date?.date_parts?.into_iter().next()?;
    match parts.as_slice() {
        [y] => Some(format!("{y:04}")),
        [y, m] => Some(format!("{y:04}-{m:02}")),
        [y, m, d] => Some(format!("{y:04}-{m:02}-{d:02}")),
        _ => None,
    }
}

/// Parses one CSL-JSON record and checks its `DOI` field against the
/// identifier that was actually requested (case-insensitively), refusing
/// to normalize a record whose identity does not match.
pub fn parse_csl_json(bytes: &[u8], requested_doi: &str) -> Result<NormalizedRecord, CslError> {
    let record: CslRecord = serde_json::from_slice(bytes).map_err(|e| CslError::Json(e.to_string()))?;

    let returned = record.doi.clone().unwrap_or_default();
    if !returned.eq_ignore_ascii_case(requested_doi) {
        return Err(CslError::IdentityMismatch { requested: requested_doi.to_string(), returned });
    }

    let authors = record.author.unwrap_or_default().into_iter().map(map_name).collect::<Vec<_>>();
    let editors = record.editor.unwrap_or_default().into_iter().map(map_name).collect::<Vec<_>>();

    Ok(NormalizedRecord {
        title: record.title.map(|t| decode_safe_entities(&t)),
        work_type: Some(map_work_type(record.kind.as_deref())),
        anonymous: false,
        container: record.container_title.map(|t| decode_safe_entities(&t)),
        date: map_date(record.issued),
        publisher: record.publisher.map(|p| decode_safe_entities(&p)),
        volume: record.volume,
        issue: record.issue,
        pages: record.page,
        authors,
        editors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "DOI": "10.1000/abc",
        "title": "Example &amp; Title",
        "type": "journal-article",
        "author": [{"family": "Doe", "given": "Jane"}],
        "container-title": "Journal of Examples",
        "issued": {"date-parts": [[2024, 3]]},
        "publisher": "Example Press",
        "volume": "12",
        "issue": "3",
        "page": "1-10"
    }"#;

    #[test]
    fn normalizes_matching_record() {
        let record = parse_csl_json(SAMPLE.as_bytes(), "10.1000/abc").unwrap();
        assert_eq!(record.title.as_deref(), Some("Example & Title"));
        assert_eq!(record.date.as_deref(), Some("2024-03"));
        assert_eq!(record.authors, vec![PersonName::Structured { family: "Doe".into(), given: "Jane".into() }]);
    }

    #[test]
    fn rejects_identity_mismatch() {
        let err = parse_csl_json(SAMPLE.as_bytes(), "10.1000/other").unwrap_err();
        assert!(matches!(err, CslError::IdentityMismatch { .. }));
    }

    #[test]
    fn identity_check_is_case_insensitive() {
        assert!(parse_csl_json(SAMPLE.as_bytes(), "10.1000/ABC").is_ok());
    }
}
