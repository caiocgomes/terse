//! `doi.org` content-negotiation adapter: the effectful half of DOI
//! resolution. Normalization itself is pure (`terse_core::references::doi`).

use terse_core::references::doi::{parse_csl_json, CslError};
use terse_core::references::record::NormalizedRecord;

use super::transport::{MetadataTransport, TransportError, TransportRequest};

pub const CSL_ACCEPT: &str = "application/vnd.citationstyles.csl+json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoiResolveError {
    Transport(TransportError),
    /// doi.org responded but not with 200 (e.g. 404 for an unregistered DOI).
    UnexpectedStatus(u16),
    Parse(CslError),
}

impl std::fmt::Display for DoiResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DoiResolveError::Transport(e) => write!(f, "{e}"),
            DoiResolveError::UnexpectedStatus(s) => write!(f, "doi.org responded with status {s}"),
            DoiResolveError::Parse(e) => write!(f, "{e}"),
        }
    }
}

/// Resolves one normalized (lowercase) DOI via `doi.org` content
/// negotiation. Either returns a complete, identity-checked
/// [`NormalizedRecord`] or fails outright — there is no partial result.
pub fn resolve_doi(transport: &mut dyn MetadataTransport, doi: &str) -> Result<NormalizedRecord, DoiResolveError> {
    let request = TransportRequest { url: format!("https://doi.org/{doi}"), accept: Some(CSL_ACCEPT.to_string()) };
    let response = transport.fetch(&request).map_err(DoiResolveError::Transport)?;
    if response.status != 200 {
        return Err(DoiResolveError::UnexpectedStatus(response.status));
    }
    parse_csl_json(&response.body, doi).map_err(DoiResolveError::Parse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::references::transport::fake::{ok, FakeTransport};

    const RECORD: &str = r#"{"DOI":"10.1000/abc","title":"T","type":"journal-article","author":[{"family":"Doe","given":"Jane"}]}"#;

    #[test]
    fn resolves_on_matching_identity() {
        let mut t = FakeTransport::new();
        t.script("https://doi.org/10.1000/abc", ok(RECORD, "https://doi.org/10.1000/abc"));
        let record = resolve_doi(&mut t, "10.1000/abc").unwrap();
        assert_eq!(record.title.as_deref(), Some("T"));
        assert_eq!(t.request_count("https://doi.org/10.1000/abc"), 1);
    }

    #[test]
    fn identity_mismatch_fails_completely_no_partial_record() {
        let mismatched = r#"{"DOI":"10.1000/other","title":"T","type":"journal-article"}"#;
        let mut t = FakeTransport::new();
        t.script("https://doi.org/10.1000/abc", ok(mismatched, "https://doi.org/10.1000/abc"));
        let err = resolve_doi(&mut t, "10.1000/abc").unwrap_err();
        assert!(matches!(err, DoiResolveError::Parse(CslError::IdentityMismatch { .. })));
    }

    #[test]
    fn non_200_status_fails_without_parsing() {
        let mut t = FakeTransport::new();
        t.script(
            "https://doi.org/10.1000/missing",
            Ok(terse_core_test_response(404, "https://doi.org/10.1000/missing")),
        );
        assert_eq!(resolve_doi(&mut t, "10.1000/missing").unwrap_err(), DoiResolveError::UnexpectedStatus(404));
    }

    fn terse_core_test_response(status: u16, url: &str) -> crate::references::transport::TransportResponse {
        crate::references::transport::TransportResponse { status, body: Vec::new(), final_url: url.to_string() }
    }
}
