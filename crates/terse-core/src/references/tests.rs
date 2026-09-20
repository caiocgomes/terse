use super::lock::{AdapterIdentity, DeclaredIdentity, LockEntry, LockFile, ProviderKind, ResolvedIdentity};
use super::record::{normalize_doi, NormalizedRecord, PersonName, WorkType};

#[test]
fn test_doi_normalization_is_canonical() {
    let bare = normalize_doi("10.1000/ABC123").unwrap();
    let prefixed = normalize_doi("doi:10.1000/abc123").unwrap();
    let url = normalize_doi("https://doi.org/10.1000/Abc123").unwrap();
    let padded = normalize_doi("  10.1000/abc123  ").unwrap();
    assert_eq!(bare, "10.1000/abc123");
    assert_eq!(bare, prefixed);
    assert_eq!(bare, url);
    assert_eq!(bare, padded);

    // A non-Crossref registration agency identifier (e.g. Datacite/mEDRA
    // prefixes) normalizes the same way: only case/whitespace/prefix
    // stripping, no agency-specific rewriting.
    assert_eq!(normalize_doi("10.5281/ZENODO.1234").unwrap(), "10.5281/zenodo.1234");

    assert!(normalize_doi("not-a-doi").is_err());
}

fn sample_record(title: &str, authors: Vec<PersonName>) -> NormalizedRecord {
    NormalizedRecord {
        title: Some(title.to_string()),
        work_type: Some(WorkType::JournalArticle),
        authors,
        container: Some("Journal of Examples".to_string()),
        date: Some("1986".to_string()),
        ..Default::default()
    }
}

fn sample_entry(identifier: &str, authors: Vec<PersonName>) -> LockEntry {
    let record = sample_record("A Study", authors);
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
fn test_lock_serialization_has_fixed_order() {
    let authors_order_a = vec![
        PersonName::Structured { family: "Robins".to_string(), given: "James".to_string() },
        PersonName::Structured { family: "Hernan".to_string(), given: "Miguel".to_string() },
    ];

    let mut lock_a = LockFile::default();
    lock_a.entries.insert("zeta".to_string(), sample_entry("10.1/z", authors_order_a.clone()));
    lock_a.entries.insert("alpha".to_string(), sample_entry("10.1/a", authors_order_a.clone()));

    // Same data, inserted in a different alias order: since entries are a
    // BTreeMap, the resulting document must be byte-identical.
    let mut lock_b = LockFile::default();
    lock_b.entries.insert("alpha".to_string(), sample_entry("10.1/a", authors_order_a.clone()));
    lock_b.entries.insert("zeta".to_string(), sample_entry("10.1/z", authors_order_a.clone()));

    let text_a = super::lock::encode(&lock_a);
    let text_b = super::lock::encode(&lock_b);
    assert_eq!(text_a, text_b);

    // Alias keys appear in sorted order in the serialized document.
    let alpha_pos = text_a.find("[entries.alpha.declared]").expect("alpha section present");
    let zeta_pos = text_a.find("[entries.zeta.declared]").expect("zeta section present");
    assert!(alpha_pos < zeta_pos);

    // No volatile fields (timestamps, absolute paths) appear anywhere.
    assert!(!text_a.contains("timestamp"));
    assert!(!text_a.contains("/Users/"));

    // Author order within a record is preserved exactly as declared,
    // never re-sorted.
    let decoded = super::lock::decode(&text_a).unwrap();
    assert_eq!(decoded.entries["alpha"].effective.authors, authors_order_a);

    // An unsupported lock version is rejected outright.
    let bad = "lock-version = 2\nnormalization-version = 1\n";
    assert!(super::lock::decode(bad).is_err());
}

#[test]
fn test_unparsed_person_name_not_guessed() {
    let unparsed = PersonName::Unparsed { name: "J. R. Robins".to_string() };
    let structured = PersonName::Structured { family: "Robins".to_string(), given: "James".to_string() };
    let org = PersonName::Organization { name: "World Health Organization".to_string() };

    assert_ne!(unparsed, structured);
    assert_ne!(unparsed, org);

    let record = NormalizedRecord {
        title: Some("Untitled".to_string()),
        work_type: Some(WorkType::Report),
        authors: vec![unparsed.clone()],
        ..Default::default()
    };
    // The unparsed author round-trips through TOML without being split
    // into family/given parts or reclassified as an organization.
    let entry = sample_entry("10.1/unparsed", vec![unparsed.clone()]);
    let mut lock = LockFile::default();
    lock.entries.insert("who1986".to_string(), LockEntry { effective: record, ..entry });
    let text = super::lock::encode(&lock);
    let decoded = super::lock::decode(&text).unwrap();
    assert_eq!(decoded.entries["who1986"].effective.authors[0], unparsed);

    // A missing date remains absent, never invented.
    assert_eq!(decoded.entries["who1986"].effective.date, None);
}

mod provider_fixtures {
    use super::super::arxiv::{parse_atom_entry, AtomError};
    use super::super::doi::{parse_csl_json, CslError};

    fn fixture(name: &str) -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/providers").join(name);
        std::fs::read(&path).unwrap_or_else(|e| panic!("reading fixture {path:?}: {e}"))
    }

    #[test]
    fn doi_match_fixture_normalizes() {
        let record = parse_csl_json(&fixture("doi-match.json"), "10.1000/abc").unwrap();
        assert_eq!(record.title.as_deref(), Some("Example & Title"));
    }

    #[test]
    fn doi_mismatch_fixture_is_rejected() {
        let err = parse_csl_json(&fixture("doi-mismatch.json"), "10.1000/abc").unwrap_err();
        assert!(matches!(err, CslError::IdentityMismatch { .. }));
    }

    #[test]
    fn doi_malformed_fixture_fails_to_parse() {
        assert!(matches!(parse_csl_json(&fixture("doi-malformed.json"), "10.1000/abc"), Err(CslError::Json(_))));
    }

    #[test]
    fn arxiv_entry_fixture_pins_resolved_version() {
        let res = parse_atom_entry(&fixture("arxiv-entry.atom"), "2301.12345").unwrap();
        assert_eq!(res.resolved_version.as_deref(), Some("v2"));
    }

    #[test]
    fn arxiv_empty_fixture_has_no_entry() {
        assert_eq!(parse_atom_entry(&fixture("arxiv-empty.atom"), "2301.12345").unwrap_err(), AtomError::NoSuchEntry);
    }

    #[test]
    fn arxiv_dtd_fixture_never_leaks_external_entity_content() {
        // quick-xml has no DTD/external-entity resolution at all: an
        // undeclared-to-it entity reference like `&xxe;` either fails to
        // unescape (a clean parse error) or passes through literally. In
        // no case does the referenced file's content ever appear in the
        // result — proving XXE is structurally impossible here, not
        // merely configured off.
        match parse_atom_entry(&fixture("arxiv-dtd.atom"), "2301.12345") {
            Ok(res) => {
                let title = res.record.title.unwrap_or_default();
                assert!(!title.contains("root:"), "external file content must never leak into normalized output");
            }
            Err(AtomError::Xml(_)) => {}
            Err(other) => panic!("unexpected error: {other}"),
        }
    }
}

mod citation_rendering {
    //! Locator/form preservation in generated LaTeX (group 16): these
    //! exercise `latex::generate_document`'s citation rendering, not the
    //! reference-record model above, but tests.md places them alongside
    //! the other citation-locator cases in this file.

    use crate::latex::generate_document;
    use crate::semantic::{Node, NodeKind, ParsedModule};
    use crate::source::{FileId, SourceSpan};
    use crate::syntax::inlines::{Citation, CiteItem, Inline, Locator, LocatorKind};
    use crate::theme;

    fn span() -> SourceSpan {
        SourceSpan::new(FileId(0), 0, 1)
    }

    fn paragraph(inlines: Vec<Inline>) -> ParsedModule {
        ParsedModule {
            file_id: FileId(0),
            metadata: None,
            references: vec![],
            blocks: vec![Node { kind: NodeKind::Paragraph { inlines }, span: span() }],
        }
    }

    #[test]
    fn test_per_work_locators_preserved() {
        let module = paragraph(vec![Inline::Citation(Citation::Group(vec![
            CiteItem {
                alias: "robins1986".to_string(),
                locator: Some(Locator { kind: Some(LocatorKind::Pages), raw: "pp. 10-12".to_string() }),
            },
            CiteItem { alias: "pearl2009".to_string(), locator: Some(Locator { kind: Some(LocatorKind::Page), raw: "p. 42".to_string() }) },
        ]))]);
        let tex = generate_document(&module, &theme::academic());
        assert!(tex.contains("\\cites[{pp. 10-12}]{robins1986}[{p. 42}]{pearl2009}"), "got: {tex}");
    }

    #[test]
    fn test_narrative_and_parenthetical_distinct() {
        let module = paragraph(vec![
            Inline::Citation(Citation::Narrative("paper".to_string())),
            Inline::Text(" and ".to_string()),
            Inline::Citation(Citation::Group(vec![CiteItem { alias: "paper".to_string(), locator: None }])),
        ]);
        let tex = generate_document(&module, &theme::academic());
        assert!(tex.contains("\\textcite{paper}"), "narrative form must use \\textcite: {tex}");
        assert!(tex.contains("\\cite{paper}"), "parenthetical form must use \\cite: {tex}");
    }
}
