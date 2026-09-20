//! Pure arXiv Atom normalization: bytes in, [`NormalizedRecord`] plus the
//! exact resolved identifier/version out.
//!
//! This module performs no I/O and knows nothing about HTTP or the
//! `id_list` query protocol — the request handling lives in
//! `terse-cli/src/references/arxiv.rs` (group 14's effectful half).
//!
//! `quick-xml`'s pull parser never resolves DTDs or external entities (it
//! has no such feature at all), so parsing here is inherently immune to
//! XXE regardless of configuration.

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use super::record::{NormalizedRecord, PersonName, WorkType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtomError {
    Xml(String),
    /// The feed contained no `<entry>` (arXiv returns this for an unknown
    /// id rather than an HTTP error).
    NoSuchEntry,
    IdentityMismatch { requested: String, returned: String },
}

impl std::fmt::Display for AtomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AtomError::Xml(msg) => write!(f, "invalid Atom feed: {msg}"),
            AtomError::NoSuchEntry => write!(f, "arXiv id_list returned no entry"),
            AtomError::IdentityMismatch { requested, returned } => {
                write!(f, "resolved arXiv id '{returned}' does not match requested '{requested}'")
            }
        }
    }
}

/// The result of normalizing one arXiv Atom entry: the metadata plus the
/// exact identifier/version the feed actually returned, which a
/// versionless request must be pinned to.
#[derive(Debug, Clone, PartialEq)]
pub struct ArxivResolution {
    pub record: NormalizedRecord,
    pub resolved_id: String,
    pub resolved_version: Option<String>,
}

/// Splits an arXiv id (modern `YYMM.NNNNN` or legacy
/// `subject-class/YYMMNNN`) into its bare identifier and optional explicit
/// `vN` version suffix.
pub fn split_version(id: &str) -> (String, Option<String>) {
    if let Some(pos) = id.rfind('v') {
        let (base, version_part) = id.split_at(pos);
        let version_digits = &version_part[1..];
        if !version_digits.is_empty() && version_digits.chars().all(|c| c.is_ascii_digit()) {
            return (base.to_string(), Some(version_part.to_string()));
        }
    }
    (id.to_string(), None)
}

/// Extracts the bare arXiv id (no version) from an `arxiv.org/abs/...`
/// URL, as it appears in an Atom entry's `<id>` element.
fn id_from_abs_url(url: &str) -> Option<String> {
    let marker = "/abs/";
    let idx = url.find(marker)?;
    Some(url[idx + marker.len()..].to_string())
}

fn text_of(reader: &mut Reader<&[u8]>, buf: &mut Vec<u8>) -> Result<String, AtomError> {
    let mut out = String::new();
    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Text(e)) => {
                out.push_str(&e.unescape().map_err(|e| AtomError::Xml(e.to_string()))?);
            }
            Ok(Event::End(_)) => break,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(AtomError::Xml(e.to_string())),
        }
        buf.clear();
    }
    Ok(out.trim().to_string())
}

/// Parses one arXiv `id_list` Atom feed response and checks the resolved
/// bare id against the one requested (version-insensitively: a
/// versionless request accepts any version, an explicit version request
/// requires an exact match).
pub fn parse_atom_entry(bytes: &[u8], requested_id: &str) -> Result<ArxivResolution, AtomError> {
    let (requested_base, requested_version) = split_version(requested_id);

    let text = std::str::from_utf8(bytes).map_err(|e| AtomError::Xml(e.to_string()))?;
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut in_entry = false;
    let mut found_entry = false;
    let mut title = None;
    let mut summary = None;
    let mut published = None;
    let mut authors = Vec::new();
    let mut entry_id_url = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.local_name();
                let name = name.as_ref();
                match name {
                    b"entry" => {
                        in_entry = true;
                        found_entry = true;
                    }
                    b"title" if in_entry => title = Some(text_of(&mut reader, &mut buf)?),
                    b"summary" if in_entry => summary = Some(text_of(&mut reader, &mut buf)?),
                    b"published" if in_entry => published = Some(text_of(&mut reader, &mut buf)?),
                    b"id" if in_entry => entry_id_url = Some(text_of(&mut reader, &mut buf)?),
                    b"name" if in_entry => authors.push(text_of(&mut reader, &mut buf)?),
                    _ => {}
                }
            }
            Ok(Event::End(e)) if e.local_name().as_ref() == b"entry" => in_entry = false,
            Ok(Event::Eof) => break,
            Err(e) => return Err(AtomError::Xml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    if !found_entry {
        return Err(AtomError::NoSuchEntry);
    }

    let id_url = entry_id_url.ok_or_else(|| AtomError::Xml("entry missing <id>".to_string()))?;
    let full_id = id_from_abs_url(&id_url).ok_or_else(|| AtomError::Xml(format!("unrecognized entry id '{id_url}'")))?;
    let (resolved_base, resolved_version) = split_version(&full_id);

    if resolved_base != requested_base {
        return Err(AtomError::IdentityMismatch { requested: requested_id.to_string(), returned: full_id });
    }
    if let Some(req_v) = &requested_version {
        if resolved_version.as_deref() != Some(req_v.as_str()) {
            return Err(AtomError::IdentityMismatch { requested: requested_id.to_string(), returned: full_id });
        }
    }

    let record = NormalizedRecord {
        title: title.map(|t| t.split_whitespace().collect::<Vec<_>>().join(" ")),
        work_type: Some(WorkType::Preprint),
        authors: authors.into_iter().map(|name| PersonName::Unparsed { name }).collect(),
        editors: Vec::new(),
        anonymous: false,
        container: None,
        date: published.map(|p| p.chars().take(10).collect()),
        publisher: None,
        volume: None,
        issue: None,
        pages: None,
    };
    let _ = summary;

    Ok(ArxivResolution { record, resolved_id: resolved_base, resolved_version })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>http://arxiv.org/abs/2301.12345v2</id>
    <published>2023-01-30T18:00:00Z</published>
    <title>An Example Paper</title>
    <summary>An example abstract.</summary>
    <author><name>Jane Doe</name></author>
    <author><name>John Roe</name></author>
  </entry>
</feed>"#;

    const EMPTY_FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom"></feed>"#;

    #[test]
    fn versionless_request_pins_resolved_version() {
        let res = parse_atom_entry(FEED.as_bytes(), "2301.12345").unwrap();
        assert_eq!(res.resolved_id, "2301.12345");
        assert_eq!(res.resolved_version.as_deref(), Some("v2"));
        assert_eq!(res.record.authors.len(), 2);
    }

    #[test]
    fn exact_version_must_match() {
        assert!(parse_atom_entry(FEED.as_bytes(), "2301.12345v2").is_ok());
        assert!(matches!(
            parse_atom_entry(FEED.as_bytes(), "2301.12345v1").unwrap_err(),
            AtomError::IdentityMismatch { .. }
        ));
    }

    #[test]
    fn no_entry_is_reported() {
        assert_eq!(parse_atom_entry(EMPTY_FEED.as_bytes(), "2301.12345").unwrap_err(), AtomError::NoSuchEntry);
    }

    #[test]
    fn split_version_handles_legacy_and_modern_ids() {
        assert_eq!(split_version("2301.12345"), ("2301.12345".to_string(), None));
        assert_eq!(split_version("2301.12345v3"), ("2301.12345".to_string(), Some("v3".to_string())));
        assert_eq!(split_version("math.GT/0309136"), ("math.GT/0309136".to_string(), None));
        assert_eq!(split_version("math.GT/0309136v1"), ("math.GT/0309136".to_string(), Some("v1".to_string())));
    }
}
