use crate::semantic::{self, Affiliation, NodeKind, ParsedModule};
use crate::source::{FileId, SourceFile};
use crate::syntax::inlines::{plain_text, Citation, Inline, LocatorKind};
use crate::syntax::{blocks, lexer};

fn parse_text(bytes: &[u8]) -> ParsedModule {
    let file = SourceFile::new(FileId(0), "entry.trs", bytes.to_vec()).expect("valid source");
    let lines = lexer::lex_lines(file.text()).expect("valid indentation");
    let module_blocks =
        blocks::parse_module(&lines, file.text(), file.id, file.base_offset()).expect("valid module");
    semantic::lower(module_blocks, file.id).expect("valid semantic module")
}

fn try_parse_text(bytes: &[u8]) -> Result<ParsedModule, ()> {
    let file = SourceFile::new(FileId(0), "entry.trs", bytes.to_vec()).map_err(|_| ())?;
    let lines = lexer::lex_lines(file.text()).map_err(|_| ())?;
    let module_blocks = blocks::parse_module(&lines, file.text(), file.id, file.base_offset()).map_err(|_| ())?;
    semantic::lower(module_blocks, file.id).map_err(|_| ())
}

/// Like [`try_parse_text`] but keeps the diagnostic instead of discarding
/// it, so a test can assert *which* failure happened and *where* rather
/// than only that something failed.
fn try_parse_diagnostics(bytes: &[u8]) -> Result<ParsedModule, crate::diagnostic::Diagnostic> {
    let file = SourceFile::new(FileId(0), "entry.trs", bytes.to_vec())
        .expect("test inputs are valid UTF-8 without a bare CR");
    let lines = lexer::lex_lines(file.text()).expect("these inputs are structurally well indented");
    let module_blocks = blocks::parse_module(&lines, file.text(), file.id, file.base_offset())?;
    semantic::lower(module_blocks, file.id)
}

fn node_text(node: &crate::semantic::Node) -> String {
    match &node.kind {
        NodeKind::Heading { inlines, .. } => plain_text(inlines),
        NodeKind::Paragraph { inlines } => plain_text(inlines),
        other => panic!("expected heading/paragraph, got {other:?}"),
    }
}

#[test]
fn test_line_endings_preserve_semantics() {
    let lf: &[u8] = b"# Title\n\nHello world.\n";
    let crlf: &[u8] = b"# Title\r\n\r\nHello world.\r\n";
    let bom_lf: Vec<u8> = {
        let mut v = vec![0xEF, 0xBB, 0xBF];
        v.extend_from_slice(lf);
        v
    };
    let no_final_newline: &[u8] = b"# Title\n\nHello world.";

    let m_lf = parse_text(lf);
    let m_crlf = parse_text(crlf);
    let m_bom = parse_text(&bom_lf);
    let m_no_nl = parse_text(no_final_newline);

    assert_eq!(m_lf, m_crlf);
    assert_eq!(m_lf, m_bom);
    assert_eq!(m_lf, m_no_nl);

    // Original byte spans reference the original (not normalized) bytes.
    let heading = &m_lf.blocks[0];
    let slice = &lf[heading.span.byte_start as usize..heading.span.byte_end as usize];
    assert_eq!(slice, b"# Title");

    let heading_crlf = &m_crlf.blocks[0];
    let slice_crlf = &crlf[heading_crlf.span.byte_start as usize..heading_crlf.span.byte_end as usize];
    assert_eq!(slice_crlf, b"# Title");

    // No BOM text node: the BOM-prefixed input parses to the same heading
    // text as the others, not a leading empty/BOM node.
    assert_eq!(node_text(&m_bom.blocks[0]), "Title");
}

#[test]
fn test_invalid_structural_indentation() {
    let tab = "a:\n\tb\n";
    let file = SourceFile::new(FileId(0), "entry.trs", tab.as_bytes().to_vec()).unwrap();
    assert!(matches!(
        lexer::lex_lines(file.text()),
        Err(lexer::IndentError::Tab { .. })
    ));

    let three_spaces = "a:\n   b\n";
    let file = SourceFile::new(FileId(0), "entry.trs", three_spaces.as_bytes().to_vec()).unwrap();
    assert!(matches!(
        lexer::lex_lines(file.text()),
        Err(lexer::IndentError::NotMultipleOfTwo { .. })
    ));

    // Indent jumps to level 2 (4 spaces), then dedents to level 1 (2
    // spaces), which was never opened.
    let unopened_dedent = "a:\n    b\n  c\n";
    let file = SourceFile::new(
        FileId(0),
        "entry.trs",
        unopened_dedent.as_bytes().to_vec(),
    )
    .unwrap();
    assert!(matches!(
        lexer::lex_lines(file.text()),
        Err(lexer::IndentError::UnopenedDedent { .. })
    ));

    // Valid two-space nesting with blank indented lines remains accepted.
    let valid = "a:\n  b\n\n  \n  c\n";
    let file = SourceFile::new(FileId(0), "entry.trs", valid.as_bytes().to_vec()).unwrap();
    assert!(lexer::lex_lines(file.text()).is_ok());
}

#[test]
fn test_escaped_reserved_start_is_prose() {
    let src = b"\\include and more text.\n\n\\figure also prose.\n";
    let module = parse_text(src);
    assert_eq!(module.blocks.len(), 2);
    assert!(node_text(&module.blocks[0]).starts_with("include"));
    assert!(node_text(&module.blocks[1]).starts_with("figure"));

    // Unescaped valid headers still create structural nodes (not prose).
    let src_ok = b"include \"other.trs\"\n\n# Heading\n";
    let file = SourceFile::new(FileId(0), "entry.trs", src_ok.to_vec()).unwrap();
    let lines = lexer::lex_lines(file.text()).unwrap();
    let module_blocks = blocks::parse_module(&lines, file.text(), file.id, file.base_offset()).unwrap();
    assert!(matches!(module_blocks[0], blocks::TopBlock::Include { .. }));
    assert!(matches!(module_blocks[1], blocks::TopBlock::Heading { .. }));
}

#[test]
fn test_malformed_reserved_header_fails() {
    let cases = ["figure without quotes\n", "include no-quotes.trs\n", "math oops\n"];
    for src in cases {
        let file = SourceFile::new(FileId(0), "entry.trs", src.as_bytes().to_vec()).unwrap();
        let lines = lexer::lex_lines(file.text()).unwrap();
        let result = blocks::parse_module(&lines, file.text(), file.id, file.base_offset());
        assert!(result.is_err(), "expected malformed header error for: {src:?}");
    }

    // Quoted paths with spaces and JSON escapes parse correctly.
    let ok_include = "include \"a path/with spaces \\\"quoted\\\".trs\"\n";
    let file = SourceFile::new(FileId(0), "entry.trs", ok_include.as_bytes().to_vec()).unwrap();
    let lines = lexer::lex_lines(file.text()).unwrap();
    let result = blocks::parse_module(&lines, file.text(), file.id, file.base_offset());
    match result.expect("valid include").into_iter().next() {
        Some(blocks::TopBlock::Include { path, .. }) => {
            assert_eq!(path, "a path/with spaces \"quoted\".trs");
        }
        other => panic!("expected include block, got {other:?}"),
    }
}

#[test]
fn test_all_metadata_fields_survive() {
    let src = concat!(
        "document:\n",
        "  title: \"A Study of Everything\"\n",
        "  subtitle: \"An Exhaustive Account\"\n",
        "  authors:\n",
        "    - name: \"Ada Lovelace\"\n",
        "      affiliation: \"Analytical Society\"\n",
        "    - name: \"Grace Hopper\"\n",
        "      affiliations: [\"US Navy\", \"Eckert-Mauchly\"]\n",
        "  date: \"2026-03-01\"\n",
        "  language: \"pt-BR\"\n",
        "  abstract:\n",
        "    This is a São Paulo abstract with *emphasis*.\n",
        "  keywords: [\"alpha\", \"beta\"]\n",
        "\n",
        "# Introduction\n",
        "\n",
        "Body text.\n",
    );

    let module = parse_text(src.as_bytes());
    let metadata = module.metadata.expect("document metadata");

    assert_eq!(metadata.title, "A Study of Everything");
    assert_eq!(metadata.subtitle.as_deref(), Some("An Exhaustive Account"));
    assert_eq!(metadata.authors.len(), 2);
    assert_eq!(metadata.authors[0].name, "Ada Lovelace");
    assert_eq!(
        metadata.authors[0].affiliation,
        Some(Affiliation::Single("Analytical Society".to_string()))
    );
    assert_eq!(metadata.authors[1].name, "Grace Hopper");
    assert_eq!(
        metadata.authors[1].affiliation,
        Some(Affiliation::Multiple(vec![
            "US Navy".to_string(),
            "Eckert-Mauchly".to_string()
        ]))
    );
    assert_eq!(metadata.date.as_deref(), Some("2026-03-01"));
    assert_eq!(metadata.language, "pt-BR");
    assert_eq!(metadata.keywords, vec!["alpha".to_string(), "beta".to_string()]);
    assert_eq!(metadata.abstract_blocks.len(), 1);
    assert!(plain_text(&metadata.abstract_blocks[0]).contains("São Paulo"));
    assert!(matches!(metadata.abstract_blocks[0][1], Inline::Emphasis(_)));

    // Absent date stays absent; absent language defaults to en.
    let minimal = parse_text(b"document:\n  title: \"Minimal\"\n\nHi.\n");
    let minimal_metadata = minimal.metadata.unwrap();
    assert_eq!(minimal_metadata.date, None);
    assert_eq!(minimal_metadata.language, "en");

    // Unsupported locale is diagnosed during validation (lowering).
    let unsupported = try_parse_text(b"document:\n  title: \"X\"\n  language: \"fr\"\n\nHi.\n");
    assert!(unsupported.is_err());
}

#[test]
fn test_invalid_metadata_context() {
    // Missing title.
    assert!(try_parse_text(b"document:\n  subtitle: \"No Title\"\n\nHi.\n").is_err());

    // Both affiliation forms on one author.
    let both_forms = "document:\n  title: \"X\"\n  authors:\n    - name: \"A\"\n      affiliation: \"One\"\n      affiliations: [\"Two\"]\n\nHi.\n";
    assert!(try_parse_text(both_forms.as_bytes()).is_err());

    // Duplicate field.
    let dup = "document:\n  title: \"X\"\n  title: \"Y\"\n\nHi.\n";
    assert!(try_parse_text(dup.as_bytes()).is_err());

    // Metadata after content.
    let after_content = "# Heading\n\ndocument:\n  title: \"X\"\n";
    assert!(try_parse_text(after_content.as_bytes()).is_err());
}

#[test]
fn test_mixed_inline_paragraph() {
    let src = b"document:\n  title: \"T\"\n\nA *wrapped* paragraph with **strong** text, a [link](https://example.org),\nliteral `code@*x` text, a footnote^[note here], math \\(x^2\\), and {ref: fig-1}.\n\nfigure \"a.pdf\" [id: fig-1]:\n  caption: A figure.\n  alt: An alt description.\n";
    let module = parse_text(src);
    let inlines = match &module.blocks[0].kind {
        NodeKind::Paragraph { inlines } => inlines,
        other => panic!("expected paragraph, got {other:?}"),
    };

    // Wrapped source lines join with a single semantic space.
    let text = plain_text(inlines);
    assert!(text.contains("paragraph with strong text"));

    assert!(inlines.iter().any(|i| matches!(i, Inline::Emphasis(_))));
    assert!(inlines.iter().any(|i| matches!(i, Inline::Strong(_))));
    assert!(inlines.iter().any(|i| matches!(i, Inline::Link { .. })));
    assert!(inlines.iter().any(|i| matches!(i, Inline::Footnote(_))));
    assert!(inlines.iter().any(|i| matches!(i, Inline::CrossRef(_))));

    // Code/math preserve literal '@' and delimiter characters, not parsed
    // as citations or formatting.
    let code = inlines
        .iter()
        .find_map(|i| match i {
            Inline::Code(t) => Some(t.clone()),
            _ => None,
        })
        .expect("a code span");
    assert_eq!(code, "code@*x");

    // Escaped punctuation and word-internal asterisks.
    let escaped = parse_text(b"document:\n  title: \"T\"\n\nA co*author*ship and \\@literal.\n");
    let text = node_text(&escaped.blocks[0]);
    assert!(text.contains("co*author*ship"));
    assert!(text.contains("@literal"));
}

#[test]
fn test_crossing_and_nested_delimiters_fail() {
    // Each case must fail for its *own* reason, at the line that carries
    // the offending delimiter. Asserting only `is_err()` let any one of
    // these pass for any other's cause, so a single over-broad rejection
    // would have looked like five working checks.
    let cases: [(&[u8], &str, &str, &str); 8] = [
        (
            b"document:\n  title: \"T\"\n\n*emph [link*text](dest)\n",
            "E-PARSE-050",
            // Crossing delimiters surface as the emphasis span never
            // closing before the link does, which is the real mechanism.
            "unterminated emphasis span",
            "*emph [link*text](dest)",
        ),
        (
            b"document:\n  title: \"T\"\n\n^[outer ^[inner] still]\n",
            "E-PARSE-050",
            "footnotes cannot nest",
            "^[outer ^[inner] still]",
        ),
        (
            b"document:\n  title: \"T\"\n\n`unterminated code\n",
            "E-PARSE-050",
            "unterminated code span",
            "`unterminated code",
        ),
        (
            b"document:\n  title: \"T\"\n\nAn unknown \\q escape.\n",
            "E-PARSE-050",
            "unknown backslash escape",
            "An unknown \\q escape.",
        ),
        (
            b"document:\n  title: \"T\"\n\nAn unescaped ***run*** here.\n",
            "E-PARSE-050",
            "",
            "An unescaped ***run*** here.",
        ),
        // Edge cases the scenario names and nothing asserted: the parser
        // implements all three and no test could tell.
        (
            b"document:\n  title: \"T\"\n\nA [nested [inner](a)](b) link.\n",
            "E-LINK-001",
            "",
            "A [nested [inner](a)](b) link.",
        ),
        (
            b"document:\n  title: \"T\"\n\nA [label ^[note]](dest) link.\n",
            "E-LINK-001",
            "",
            "A [label ^[note]](dest) link.",
        ),
        (
            b"document:\n  title: \"T\"\n\nA [label](dest with space) link.\n",
            "E-PARSE-050",
            "",
            "A [label](dest with space) link.",
        ),
    ];
    for (src, expected_code, expected_message, offending_line) in cases {
        let text = std::str::from_utf8(src).unwrap();
        let diagnostic = match try_parse_diagnostics(src) {
            Err(d) => d,
            Ok(_) => panic!("expected failure for {text:?}"),
        };
        assert_eq!(
            diagnostic.code, expected_code,
            "wrong diagnostic code for {text:?}"
        );
        assert!(
            diagnostic.message.contains(expected_message),
            "message {:?} must identify the cause {expected_message:?} for {text:?}",
            diagnostic.message
        );
        let span = diagnostic.primary.expect("an inline failure has a position");
        assert_eq!(
            &text[span.byte_start as usize..span.byte_end as usize],
            offending_line,
            "diagnostic must point at the line carrying the delimiter, for {text:?}"
        );
    }

    // Escaped delimiter text is accepted.
    assert!(try_parse_text(b"document:\n  title: \"T\"\n\nLiteral \\* not emphasis.\n").is_ok());
}

#[test]
fn test_citation_lexing_retains_intent() {
    let src = b"document:\n  title: \"T\"\n\nSee @robins1986 and [@robins1986; @pearl2009, p. 42] but not jane@example.com or \\@literal.\n";
    let module = parse_text(src);
    let inlines = match &module.blocks[0].kind {
        NodeKind::Paragraph { inlines } => inlines,
        other => panic!("expected paragraph, got {other:?}"),
    };

    let narrative_count = inlines
        .iter()
        .filter(|i| matches!(i, Inline::Citation(Citation::Narrative(_))))
        .count();
    assert_eq!(narrative_count, 1, "exactly one narrative citation");

    let group = inlines
        .iter()
        .find_map(|i| match i {
            Inline::Citation(Citation::Group(items)) => Some(items),
            _ => None,
        })
        .expect("a citation group");
    assert_eq!(group.len(), 2);
    assert_eq!(group[0].alias, "robins1986");
    assert!(group[0].locator.is_none());
    assert_eq!(group[1].alias, "pearl2009");
    assert_eq!(group[1].locator.as_ref().unwrap().kind, Some(LocatorKind::Page));

    let text = plain_text(inlines);
    assert!(text.contains("jane@example.com"));
    assert!(text.contains("@literal"));
}

#[test]
fn test_declarations_require_module_scope() {
    // A declaration nested inside an abstract body (not module scope)
    // fails, and produces no executable/preprocessor action.
    let nested_refs =
        b"document:\n  title: \"T\"\n  abstract:\n    refs:\n      x: doi:10.1/x\n\nHi.\n";
    assert!(try_parse_text(nested_refs).is_err());

    let nested_include = b"document:\n  title: \"T\"\n  abstract:\n    include \"x.trs\"\n\nHi.\n";
    assert!(try_parse_text(nested_include).is_err());

    // The module-level equivalent parses fine, independent of provider
    // availability (no network/resolution happens here).
    let module_level = parse_text(
        b"document:\n  title: \"T\"\n\nrefs:\n  robins1986: doi:10.1000/abc\n  paper2: arxiv:2101.00001\n\nHi.\n",
    );
    assert_eq!(module_level.references.len(), 2);
    assert_eq!(module_level.references[0].alias, "robins1986");
}

#[test]
fn test_headings_and_nested_lists() {
    let src = concat!(
        "document:\n",
        "  title: \"T\"\n",
        "\n",
        "# Level One\n",
        "\n",
        "## Level Two\n",
        "\n",
        "### Level Three\n",
        "\n",
        "9. Ninth item\n",
        "  - nested a\n",
        "  - nested b\n",
        "10. Tenth item\n",
        "  Continued text for tenth.\n",
        "11. Eleventh item\n",
    );
    let module = parse_text(src.as_bytes());

    let levels: Vec<u8> = module
        .blocks
        .iter()
        .filter_map(|n| match &n.kind {
            NodeKind::Heading { level, .. } => Some(*level),
            _ => None,
        })
        .collect();
    assert_eq!(levels, vec![1, 2, 3]);

    let list = module
        .blocks
        .iter()
        .find_map(|n| match &n.kind {
            NodeKind::List { ordered, start, items } => Some((*ordered, *start, items)),
            _ => None,
        })
        .expect("an ordered list");
    let (ordered, start, items) = list;
    assert!(ordered);
    assert_eq!(start, Some(9));
    assert_eq!(items.len(), 3);
    assert!(plain_text(&items[0].inlines).contains("Ninth item"));
    assert_eq!(items[0].continuation.len(), 1);
    assert!(matches!(
        items[0].continuation[0].kind,
        NodeKind::List { ordered: false, .. }
    ));
    assert!(plain_text(&items[1].inlines).contains("Tenth item"));
    assert_eq!(items[1].continuation.len(), 1, "wrapped continuation becomes a nested block");
}

#[test]
fn test_ordered_markers_are_sequential() {
    let bad = "document:\n  title: \"T\"\n\n3. three\n5. five\n";
    assert!(try_parse_text(bad.as_bytes()).is_err());

    let good = "document:\n  title: \"T\"\n\n3. three\n4. four\n";
    let module = parse_text(good.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::List { start, .. } => assert_eq!(*start, Some(3)),
        other => panic!("expected list, got {other:?}"),
    }

    // Unsupported blocks inside list items fail.
    let bad_nested = "document:\n  title: \"T\"\n\n- item\n  math:\n    x = 1\n";
    assert!(try_parse_text(bad_nested.as_bytes()).is_err());
}

#[test]
fn test_theorem_proof_equation_structure() {
    let src = concat!(
        "document:\n",
        "  title: \"T\"\n",
        "\n",
        "theorem Transfer [id: transfer]:\n",
        "  The rate of transfer is bounded.\n",
        "\n",
        "proof [of: transfer]:\n",
        "  By direct computation.\n",
        "\n",
        "  math [id: transfer-eq]:\n",
        "    \\dot V = -k V\n",
    );
    let module = parse_text(src.as_bytes());
    assert_eq!(module.blocks.len(), 2);

    match &module.blocks[0].kind {
        NodeKind::TheoremLike { kind, title, id, body } => {
            assert_eq!(*kind, semantic::TheoremKind::Theorem);
            assert_eq!(title.as_deref(), Some("Transfer"));
            assert_eq!(id.as_deref(), Some("transfer"));
            assert_eq!(body.len(), 1);
        }
        other => panic!("expected theorem, got {other:?}"),
    }

    match &module.blocks[1].kind {
        NodeKind::Proof { of, body, .. } => {
            assert_eq!(of.as_deref(), Some("transfer"));
            assert_eq!(body.len(), 2);
            match &body[1].kind {
                NodeKind::Equation {
                    id,
                    numbered,
                    payload,
                } => {
                    assert_eq!(id.as_deref(), Some("transfer-eq"));
                    assert!(*numbered, "math: equations are numbered");
                    assert!(payload.contains("\\dot V"));
                }
                other => panic!("expected equation, got {other:?}"),
            }
        }
        other => panic!("expected proof, got {other:?}"),
    }

    // A proof without `of` is independent; no adjacency-based inference.
    let independent = concat!(
        "document:\n  title: \"T\"\n\n",
        "lemma:\n  A helper fact.\n\n",
        "proof:\n  Trivial.\n",
    );
    let module2 = parse_text(independent.as_bytes());
    match &module2.blocks[1].kind {
        NodeKind::Proof { of, .. } => assert_eq!(*of, None),
        other => panic!("expected proof, got {other:?}"),
    }
}

#[test]
fn test_nested_module_structures_fail() {
    let cases: [&[u8]; 4] = [
        b"document:\n  title: \"T\"\n\ntheorem:\n  # nested heading\n  body\n",
        b"document:\n  title: \"T\"\n\ntheorem:\n  include \"x.trs\"\n",
        b"document:\n  title: \"T\"\n\ntheorem:\n  refs:\n    a: doi:10.1/a\n",
        b"document:\n  title: \"T\"\n\ntheorem:\n  bibliography:\n",
    ];
    for src in cases {
        assert!(try_parse_text(src).is_err(), "expected failure for {src:?}");
    }

    // Nested proof/theorem and supported list/equation bodies remain valid.
    let ok = concat!(
        "document:\n  title: \"T\"\n\n",
        "theorem:\n",
        "  An outer statement.\n",
        "  - a list item\n",
        "  proof:\n",
        "    An inner proof.\n",
    );
    assert!(try_parse_text(ok.as_bytes()).is_ok());
}

#[test]
fn test_figure_table_fields_are_semantic() {
    let src = concat!(
        "document:\n  title: \"T\"\n\n",
        "figure \"figures/plot.pdf\" [id: transition, role: wide]:\n",
        "  caption: \"A wide plot\"\n",
        "  alt: \"Plot of transition dynamics\"\n",
        "\n",
        "table [id: costs]:\n",
        "  caption: \"Inventory costs\"\n",
        "  header: [\"Policy\", \"Cost, USD\"]\n",
        "  rows:\n",
        "    - [\"Baseline\", \"12\"]\n",
        "    - [\"Forecast\", \"9\"]\n",
    );
    let module = parse_text(src.as_bytes());

    match &module.blocks[0].kind {
        NodeKind::Figure { path, id, role, alt, caption } => {
            assert_eq!(path, "figures/plot.pdf");
            assert_eq!(id.as_deref(), Some("transition"));
            assert_eq!(role.as_deref(), Some("wide"));
            assert_eq!(alt, "Plot of transition dynamics");
            assert!(plain_text(caption).contains("A wide plot"));
        }
        other => panic!("expected figure, got {other:?}"),
    }

    match &module.blocks[1].kind {
        NodeKind::Table {
            id,
            header,
            rows,
            align,
            ..
        } => {
            assert_eq!(id.as_deref(), Some("costs"));
            assert_eq!(header, &vec![vec![Inline::Text("Policy".to_string())], vec![Inline::Text("Cost, USD".to_string())]]);
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0], vec![vec![Inline::Text("Baseline".to_string())], vec![Inline::Text("12".to_string())]]);
            assert_eq!(align, &vec![crate::semantic::ColumnAlign::Default, crate::semantic::ColumnAlign::Default]);
        }
        other => panic!("expected table, got {other:?}"),
    }
}

#[test]
fn test_missing_figure_fields_and_ragged_tables() {
    let missing_alt = concat!(
        "document:\n  title: \"T\"\n\n",
        "figure \"a.pdf\":\n  caption: \"A caption\"\n",
    );
    assert!(try_parse_text(missing_alt.as_bytes()).is_err());

    let missing_caption = concat!(
        "document:\n  title: \"T\"\n\n",
        "figure \"a.pdf\":\n  alt: \"Some alt text\"\n",
    );
    assert!(try_parse_text(missing_caption.as_bytes()).is_err());

    // A row longer than the header is still rejected; a shorter one is now
    // padded rather than an error (test_invalid_tables_fail's edge case).
    let ragged_table = concat!(
        "document:\n  title: \"T\"\n\n",
        "table:\n",
        "  caption: \"C\"\n",
        "  header: [\"A\", \"B\"]\n",
        "  rows:\n",
        "    - [\"x\", \"y\", \"z\"]\n",
    );
    assert!(try_parse_text(ragged_table.as_bytes()).is_err());

    let missing_header = concat!(
        "document:\n  title: \"T\"\n\n",
        "table:\n",
        "  caption: \"C\"\n",
        "  rows:\n",
        "    - [\"x\", \"y\"]\n",
    );
    assert!(try_parse_text(missing_header.as_bytes()).is_err());

    // A footnote in a caption is rejected.
    let footnote_caption = concat!(
        "document:\n  title: \"T\"\n\n",
        "figure \"a.pdf\":\n",
        "  caption: \"See^[note] this\"\n",
        "  alt: \"Alt text\"\n",
    );
    assert!(try_parse_text(footnote_caption.as_bytes()).is_err());
}

#[test]
fn test_invalid_tables_fail() {
    // (a) a field-form row with three cells under a two-cell header.
    let long_row = concat!(
        "document:\n  title: \"T\"\n\n",
        "table:\n",
        "  caption: \"C\"\n",
        "  header: [\"A\", \"B\"]\n",
        "  rows:\n",
        "    - [\"x\", \"y\", \"z\"]\n",
    );
    let err = try_parse_diagnostics(long_row.as_bytes()).unwrap_err();
    assert_eq!(err.code, "E-META-015", "{err:?}");

    // (b, pipe form) same overlong-row rule, covered once pipe tables
    // exist: crate::syntax::tests::test_pipe_prose_and_malformed_pipe_tables.

    // (c) an id with a header/rows but no caption.
    let no_caption = concat!(
        "document:\n  title: \"T\"\n\n",
        "table [id: t1]:\n",
        "  header: [\"A\", \"B\"]\n",
        "  rows:\n",
        "    - [\"x\", \"y\"]\n",
    );
    let err = try_parse_diagnostics(no_caption.as_bytes()).unwrap_err();
    assert_eq!(err.code, "E-META-017", "{err:?}");

    // (d) a figure without alt text still fails as today.
    let no_alt = concat!(
        "document:\n  title: \"T\"\n\n",
        "figure \"a.pdf\":\n  caption: \"A caption\"\n",
    );
    assert!(try_parse_text(no_alt.as_bytes()).is_err());

    // Edge case: a field-form row with one cell under a two-cell header
    // now succeeds, padded with an empty cell.
    let short_row = concat!(
        "document:\n  title: \"T\"\n\n",
        "table:\n",
        "  caption: \"C\"\n",
        "  header: [\"A\", \"B\"]\n",
        "  rows:\n",
        "    - [\"only one\"]\n",
    );
    let module = parse_text(short_row.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::Table { rows, .. } => {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].len(), 2, "short row padded to header width");
            assert_eq!(rows[0][1], Vec::<Inline>::new(), "padding cell is empty");
        }
        other => panic!("expected table, got {other:?}"),
    }

    // Edge case: a footnote in a caption or a cell fails with E-META-018.
    let footnote_caption = concat!(
        "document:\n  title: \"T\"\n\n",
        "table:\n",
        "  caption: \"See^[note] this\"\n",
        "  header: [\"A\"]\n",
        "  rows:\n",
        "    - [\"x\"]\n",
    );
    let err = try_parse_diagnostics(footnote_caption.as_bytes()).unwrap_err();
    assert_eq!(err.code, "E-META-018", "{err:?}");

    let footnote_cell = concat!(
        "document:\n  title: \"T\"\n\n",
        "table:\n",
        "  caption: \"C\"\n",
        "  header: [\"A\"]\n",
        "  rows:\n",
        "    - [\"x^[note]\"]\n",
    );
    let err = try_parse_diagnostics(footnote_cell.as_bytes()).unwrap_err();
    assert_eq!(err.code, "E-META-018", "{err:?}");
}

#[test]
fn test_bare_pipe_table() {
    let src = concat!(
        "document:\n  title: \"T\"\n\n",
        "| Metric | Value |\n",
        "|:--|--:|\n",
        "| *Accuracy* | $0.97$ |\n",
        "| Latency |\n",
        "\n",
        "After.\n",
    );
    let module = parse_text(src.as_bytes());
    assert_eq!(module.blocks.len(), 2, "{module:?}");
    match &module.blocks[0].kind {
        NodeKind::Table {
            id,
            caption,
            align,
            header,
            rows,
        } => {
            assert_eq!(*id, None);
            assert_eq!(*caption, None);
            assert_eq!(align, &vec![crate::semantic::ColumnAlign::Left, crate::semantic::ColumnAlign::Right]);
            assert_eq!(header, &vec![vec![Inline::Text("Metric".to_string())], vec![Inline::Text("Value".to_string())]]);
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0][0], vec![Inline::Emphasis(vec![Inline::Text("Accuracy".to_string())])]);
            assert_eq!(rows[0][1], vec![Inline::Math("0.97".to_string())]);
            assert_eq!(rows[1], vec![vec![Inline::Text("Latency".to_string())], vec![]]);
        }
        other => panic!("expected table, got {other:?}"),
    }
    match &module.blocks[1].kind {
        NodeKind::Paragraph { inlines } => assert_eq!(plain_text(inlines), "After."),
        other => panic!("expected paragraph, got {other:?}"),
    }

    // `|---|:-:|` gives [Default, Center].
    let centered = concat!(
        "document:\n  title: \"T\"\n\n",
        "| A | B |\n",
        "|---|:-:|\n",
        "| x | y |\n",
    );
    let module = parse_text(centered.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::Table { align, .. } => {
            assert_eq!(
                align,
                &vec![
                    crate::semantic::ColumnAlign::Default,
                    crate::semantic::ColumnAlign::Center
                ]
            );
        }
        other => panic!("expected table, got {other:?}"),
    }

    // A bare pipe table is accepted inside a theorem body.
    let in_theorem = concat!(
        "document:\n  title: \"T\"\n\n",
        "theorem:\n",
        "  A statement.\n",
        "\n",
        "  | A | B |\n",
        "  |---|---|\n",
        "  | x | y |\n",
    );
    assert!(try_parse_text(in_theorem.as_bytes()).is_ok());

    // A bare pipe table is rejected inside a list item, as the field form
    // already is.
    let in_list_item = concat!(
        "document:\n  title: \"T\"\n\n",
        "1. Item:\n",
        "  | A | B |\n",
        "  |---|---|\n",
        "  | x | y |\n",
    );
    assert!(try_parse_text(in_list_item.as_bytes()).is_err());
}

#[test]
fn test_pipe_cell_splitting_protects_math_and_code() {
    let src = concat!(
        "document:\n  title: \"T\"\n\n",
        "| A | B | C | D | E |\n",
        "|---|---|---|---|---|\n",
        "| $|x|$ | `a|b` | a \\| b | $5 | $10 |\n",
    );
    let module = parse_text(src.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::Table { rows, .. } => {
            assert_eq!(rows.len(), 1);
            let row = &rows[0];
            assert_eq!(row[0], vec![Inline::Math("|x|".to_string())]);
            assert_eq!(row[1], vec![Inline::Code("a|b".to_string())]);
            assert_eq!(row[2], vec![Inline::Text("a | b".to_string())]);
            assert_eq!(row[3], vec![Inline::Text("$5".to_string())]);
            assert_eq!(row[4], vec![Inline::Text("$10".to_string())]);
        }
        other => panic!("expected table, got {other:?}"),
    }

    // `` | `a\|b` | `` yields Code("a|b"): `\|` also reads as `|` inside a
    // code span, for GFM compatibility.
    let code_escape = concat!(
        "document:\n  title: \"T\"\n\n",
        "| A |\n",
        "|---|\n",
        "| `a\\|b` |\n",
    );
    let module = parse_text(code_escape.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::Table { rows, .. } => {
            assert_eq!(rows[0][0], vec![Inline::Code("a|b".to_string())])
        }
        other => panic!("expected table, got {other:?}"),
    }

    // `| $\|x\|$ |` yields Math("\|x\|"): inside math, `\|` is left as
    // written (it is TeX's double bar), not unescaped.
    let math_bar = concat!(
        "document:\n  title: \"T\"\n\n",
        "| A |\n",
        "|---|\n",
        "| $\\|x\\|$ |\n",
    );
    let module = parse_text(math_bar.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::Table { rows, .. } => {
            assert_eq!(rows[0][0], vec![Inline::Math("\\|x\\|".to_string())])
        }
        other => panic!("expected table, got {other:?}"),
    }

    // `| \(a|b\) |` is one math cell.
    let paren_math = concat!(
        "document:\n  title: \"T\"\n\n",
        "| A |\n",
        "|---|\n",
        "| \\(a|b\\) |\n",
    );
    let module = parse_text(paren_math.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::Table { rows, .. } => {
            assert_eq!(rows[0][0], vec![Inline::Math("a|b".to_string())])
        }
        other => panic!("expected table, got {other:?}"),
    }

    // A lone backtick protects nothing at the splitter level (covered
    // directly, without going through inline parsing, by
    // `blocks::pipe_row_splitter_tests::unmatched_protected_span_protects_nothing_and_splits_normally`,
    // which asserts `| ` | x |` splits into two cells `["`", "x"]`, not
    // one swallowed cell). Once that lone backtick reaches inline
    // parsing as its own cell's content, it fails the same way a lone
    // backtick in a paragraph already does: an unterminated code span.
    let lone_backtick = concat!(
        "document:\n  title: \"T\"\n\n",
        "| A | B |\n",
        "|---|---|\n",
        "| ` | x |\n",
    );
    let err = try_parse_diagnostics(lone_backtick.as_bytes()).unwrap_err();
    assert_eq!(err.code, "E-PARSE-050", "{err:?}");
}

#[test]
fn test_captioned_pipe_table_block() {
    let src = concat!(
        "document:\n  title: \"T\"\n\n",
        "table [id: tbl-results]:\n",
        "  caption: \"Summary of *results*\"\n",
        "  | Metric | Value |\n",
        "  |---|---|\n",
        "  | Accuracy | 0.97 |\n",
    );
    let module = parse_text(src.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::Table {
            id,
            caption,
            header,
            rows,
            ..
        } => {
            assert_eq!(id.as_deref(), Some("tbl-results"));
            assert_eq!(
                caption,
                &Some(vec![
                    Inline::Text("Summary of ".to_string()),
                    Inline::Emphasis(vec![Inline::Text("results".to_string())])
                ])
            );
            assert_eq!(header, &vec![vec![Inline::Text("Metric".to_string())], vec![Inline::Text("Value".to_string())]]);
            assert_eq!(rows.len(), 1);
        }
        other => panic!("expected table, got {other:?}"),
    }

    // The field-form caption becomes inline too.
    let field_caption = concat!(
        "document:\n  title: \"T\"\n\n",
        "table:\n",
        "  caption: \"A *b*\"\n",
        "  header: [\"H\"]\n",
        "  rows:\n",
        "    - [\"x\"]\n",
    );
    let module = parse_text(field_caption.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::Table { caption, .. } => {
            assert_eq!(
                caption,
                &Some(vec![
                    Inline::Text("A ".to_string()),
                    Inline::Emphasis(vec![Inline::Text("b".to_string())])
                ])
            );
        }
        other => panic!("expected table, got {other:?}"),
    }
}

#[test]
fn test_pipe_prose_and_malformed_pipe_tables() {
    // (a) A `|` line with no following delimiter row stays prose.
    let prose = concat!(
        "document:\n  title: \"T\"\n\n",
        "| not a table\n",
        "just prose\n",
    );
    let module = parse_text(prose.as_bytes());
    assert_eq!(module.blocks.len(), 1);
    match &module.blocks[0].kind {
        NodeKind::Paragraph { inlines } => {
            assert_eq!(plain_text(inlines), "| not a table just prose")
        }
        other => panic!("expected paragraph, got {other:?}"),
    }

    // (b) A header/delimiter cell-count mismatch is rejected at the
    // delimiter line.
    let mismatch = concat!(
        "document:\n  title: \"T\"\n\n",
        "| a | b |\n",
        "|---|---|---|\n",
    );
    let err = try_parse_diagnostics(mismatch.as_bytes()).unwrap_err();
    assert_eq!(err.code, "E-PARSE-002", "{err:?}");

    // (c) A `table:` block cannot mix `rows:` with pipe lines.
    let mixed = concat!(
        "document:\n  title: \"T\"\n\n",
        "table:\n",
        "  caption: \"C\"\n",
        "  header: [\"A\"]\n",
        "  rows:\n",
        "    - [\"x\"]\n",
        "  | y |\n",
    );
    let err = try_parse_diagnostics(mixed.as_bytes()).unwrap_err();
    assert_eq!(err.code, "E-PARSE-002", "{err:?}");

    // Edge case: a paragraph running straight into a pipe table (no blank
    // line between them) ends before the table.
    let running_in = concat!(
        "document:\n  title: \"T\"\n\n",
        "Lead-in text.\n",
        "| a | b |\n",
        "|---|---|\n",
        "| x | y |\n",
    );
    let module = parse_text(running_in.as_bytes());
    assert_eq!(module.blocks.len(), 2, "{module:?}");
    match &module.blocks[0].kind {
        NodeKind::Paragraph { inlines } => assert_eq!(plain_text(inlines), "Lead-in text."),
        other => panic!("expected paragraph, got {other:?}"),
    }
    assert!(matches!(module.blocks[1].kind, NodeKind::Table { .. }));
}

#[test]
fn test_wide_role_has_no_dimensions() {
    let src = concat!(
        "document:\n  title: \"T\"\n\n",
        "figure \"a.pdf\" [id: transition, role: wide]:\n",
        "  caption: \"C\"\n",
        "  alt: \"A\"\n",
    );
    let module = parse_text(src.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::Figure { id, role, .. } => {
            assert_eq!(id.as_deref(), Some("transition"));
            assert_eq!(role.as_deref(), Some("wide"));
        }
        other => panic!("expected figure, got {other:?}"),
    }

    // Unknown/case-variant roles fail (case-sensitive grammar).
    let unknown_role = concat!(
        "document:\n  title: \"T\"\n\n",
        "figure \"a.pdf\" [role: Wide]:\n  caption: \"C\"\n  alt: \"A\"\n",
    );
    assert!(try_parse_text(unknown_role.as_bytes()).is_err());

    // Duplicate roles fail.
    let dup_role = concat!(
        "document:\n  title: \"T\"\n\n",
        "figure \"a.pdf\" [role: wide, role: wide]:\n  caption: \"C\"\n  alt: \"A\"\n",
    );
    assert!(try_parse_text(dup_role.as_bytes()).is_err());
}

#[test]
fn test_tex_math_bytes_are_preserved() {
    // Inline math survives with its exact payload bytes.
    let src = concat!(
        "document:\n  title: \"T\"\n\n",
        "The condition \\(\\frac{\\partial \\dot V}{\\partial H} > 0.\\) holds.\n",
    );
    let module = parse_text(src.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::Paragraph { inlines } => {
            let math = inlines
                .iter()
                .find_map(|i| match i {
                    Inline::Math(t) => Some(t.as_str()),
                    _ => None,
                })
                .expect("inline math present");
            assert_eq!(math, r"\frac{\partial \dot V}{\partial H} > 0.");
        }
        other => panic!("expected paragraph, got {other:?}"),
    }

    // Display math with aligned/matrix/cases survives, structurally
    // dedented but otherwise byte-identical.
    let display = concat!(
        "document:\n  title: \"T\"\n\n",
        "math [id: eq1]:\n",
        "  \\begin{aligned}\n",
        "    a &= b \\\\\n",
        "    c &= \\begin{pmatrix} 1 & 0 \\\\ 0 & 1 \\end{pmatrix}\n",
        "  \\end{aligned}\n",
    );
    let module = parse_text(display.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::Equation { id, payload, .. } => {
            assert_eq!(id.as_deref(), Some("eq1"));
            assert_eq!(
                payload,
                "\\begin{aligned}\n  a &= b \\\\\n  c &= \\begin{pmatrix} 1 & 0 \\\\ 0 & 1 \\end{pmatrix}\n\\end{aligned}"
            );
        }
        other => panic!("expected equation, got {other:?}"),
    }

    // Edge cases: unbalanced braces and a mismatched environment end fail.
    let unbalanced = concat!("document:\n  title: \"T\"\n\n", "math:\n", "  \\frac{a}{b\n");
    assert!(try_parse_text(unbalanced.as_bytes()).is_err());

    let mismatched_env = concat!(
        "document:\n  title: \"T\"\n\n",
        "math:\n",
        "  \\begin{aligned} a \\end{matrix}\n",
    );
    assert!(try_parse_text(mismatched_env.as_bytes()).is_err());
}

#[test]
fn test_opaque_payload_lines_are_not_structural() {
    // `math:` payload lines may use indentation that would be invalid
    // structurally (five spaces on the second line, not a multiple of two
    // relative to the required two-space body prefix).
    let math_src = concat!(
        "document:\n  title: \"T\"\n\n",
        "math:\n",
        "  a = b\n",
        "     + c\n",
        "\n",
        "After math.\n",
    );
    let module = parse_text(math_src.as_bytes());
    assert_eq!(module.blocks.len(), 2, "{:?}", module.blocks);
    match &module.blocks[0].kind {
        NodeKind::Equation { payload, .. } => assert_eq!(payload, "a = b\n   + c"),
        other => panic!("expected equation, got {other:?}"),
    }
    match &module.blocks[1].kind {
        NodeKind::Paragraph { .. } => {}
        other => panic!("expected paragraph, got {other:?}"),
    }

    // `tex:` payload tolerates a tab right after its own structural prefix.
    let tex_src = concat!(
        "document:\n  title: \"T\"\n\n",
        "tex:\n",
        "  \t\\draw;\n",
        "\n",
        "After tex.\n",
    );
    let module = parse_text(tex_src.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::RawTex { payload } => assert_eq!(payload, "\t\\draw;"),
        other => panic!("expected raw tex, got {other:?}"),
    }

    // A multi-line `$$` payload tolerates a three-space middle line.
    let dollar_src = concat!(
        "document:\n  title: \"T\"\n\n",
        "$$\n",
        "x =\n",
        "   y\n",
        "$$\n",
        "\n",
        "After dollar.\n",
    );
    let module = parse_text(dollar_src.as_bytes());
    assert_eq!(module.blocks.len(), 2, "{:?}", module.blocks);
    match &module.blocks[0].kind {
        NodeKind::Equation {
            payload, numbered, ..
        } => {
            assert!(!numbered);
            assert_eq!(payload, "x =\n   y");
        }
        other => panic!("expected equation, got {other:?}"),
    }
    match &module.blocks[1].kind {
        NodeKind::Paragraph { .. } => {}
        other => panic!("expected paragraph, got {other:?}"),
    }

    // A fenced code block tolerates a tab-indented content line.
    let fence_src = concat!(
        "document:\n  title: \"T\"\n\n",
        "```\n",
        "\tindented\n",
        "```\n",
        "\n",
        "After code.\n",
    );
    let module = parse_text(fence_src.as_bytes());
    assert_eq!(module.blocks.len(), 2, "{:?}", module.blocks);
    match &module.blocks[0].kind {
        NodeKind::CodeBlock { code, language } => {
            assert_eq!(language, &None);
            assert_eq!(code, "\tindented");
        }
        other => panic!("expected code block, got {other:?}"),
    }
    match &module.blocks[1].kind {
        NodeKind::Paragraph { .. } => {}
        other => panic!("expected paragraph, got {other:?}"),
    }

    // Edge case: a paragraph line indented by three spaces (not inside any
    // opaque block) still fails structurally.
    let bad_indent = concat!(
        "document:\n  title: \"T\"\n\n",
        "Intro.\n",
        "   Bad indent line.\n",
    );
    assert!(try_parse_text(bad_indent.as_bytes()).is_err());

    // Edge case: a malformed `math:`-shaped header followed by a
    // three-space line still fails -- it never opened a payload region.
    let malformed_header = concat!("document:\n  title: \"T\"\n\n", "math: nope\n", "   x\n",);
    assert!(try_parse_text(malformed_header.as_bytes()).is_err());
}

#[test]
fn test_fenced_code_block_is_byte_exact() {
    let src = concat!(
        "document:\n  title: \"T\"\n\n",
        "```python\n",
        "\tif x:\n",
        "   aligned = (1,\n",
        "// not a comment\n",
        "$x$ and *y* and [@ref]\n",
        "\n",
        "end\n",
        "```\n",
    );
    let module = parse_text(src.as_bytes());
    assert_eq!(module.blocks.len(), 1, "{:?}", module.blocks);
    match &module.blocks[0].kind {
        NodeKind::CodeBlock { language, code } => {
            assert_eq!(language.as_deref(), Some("python"));
            assert_eq!(
                code,
                "\tif x:\n   aligned = (1,\n// not a comment\n$x$ and *y* and [@ref]\n\nend"
            );
        }
        other => panic!("expected code block, got {other:?}"),
    }

    // A CRLF-encoded source keeps CRLF inside the code.
    let crlf = "document:\r\n  title: \"T\"\r\n\r\n```\r\nline one\r\nline two\r\n```\r\n";
    let module = parse_text(crlf.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::CodeBlock { code, .. } => assert_eq!(code, "line one\r\nline two"),
        other => panic!("expected code block, got {other:?}"),
    }

    // A whitespace-only content line keeps its exact spaces.
    let ws_line = concat!(
        "document:\n  title: \"T\"\n\n",
        "```\n",
        "a\n",
        "   \n",
        "b\n",
        "```\n",
    );
    let module = parse_text(ws_line.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::CodeBlock { code, .. } => assert_eq!(code, "a\n   \nb"),
        other => panic!("expected code block, got {other:?}"),
    }
}

#[test]
fn test_fence_ends_running_paragraph() {
    let src = concat!(
        "document:\n  title: \"T\"\n\n",
        "Run this:\n",
        "```bash\n",
        "make all\n",
        "```\n",
        "Then continue.\n",
    );
    let module = parse_text(src.as_bytes());
    assert_eq!(module.blocks.len(), 3, "{:?}", module.blocks);
    assert_eq!(node_text(&module.blocks[0]), "Run this:");
    match &module.blocks[1].kind {
        NodeKind::CodeBlock { language, code } => {
            assert_eq!(language.as_deref(), Some("bash"));
            assert_eq!(code, "make all");
        }
        other => panic!("expected code block, got {other:?}"),
    }
    assert_eq!(node_text(&module.blocks[2]), "Then continue.");
}

#[test]
fn test_code_block_in_list_item() {
    let src = concat!(
        "document:\n  title: \"T\"\n\n",
        "1. Install:\n",
        "  ```bash\n",
        "  pip install terse\n",
        "  ```\n",
    );
    let module = parse_text(src.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::List { items, .. } => {
            assert_eq!(items.len(), 1);
            let item = &items[0];
            assert_eq!(plain_text(&item.inlines), "Install:");
            assert_eq!(item.continuation.len(), 1);
            match &item.continuation[0].kind {
                NodeKind::CodeBlock { language, code } => {
                    assert_eq!(language.as_deref(), Some("bash"));
                    assert_eq!(code, "pip install terse");
                }
                other => panic!("expected code block, got {other:?}"),
            }
        }
        other => panic!("expected list, got {other:?}"),
    }

    // A whitespace-only content line in a nested block loses the block's
    // structural prefix like any other content line (six spaces in the
    // source, two of them the item's prefix, four kept); one shorter than
    // the prefix has only prefix spaces, so it becomes empty.
    let blank_lines = concat!(
        "document:\n  title: \"T\"\n\n",
        "1. Install:\n",
        "  ```bash\n",
        "  a\n",
        "      \n",
        " \n",
        "  b\n",
        "  ```\n",
    );
    let module = parse_text(blank_lines.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::List { items, .. } => match &items[0].continuation[0].kind {
            NodeKind::CodeBlock { code, .. } => assert_eq!(code, "a\n    \n\nb"),
            other => panic!("expected code block, got {other:?}"),
        },
        other => panic!("expected list, got {other:?}"),
    }

    // The same position holding `math:` still fails with the existing
    // list-item diagnostic.
    let math_in_item = concat!(
        "document:\n  title: \"T\"\n\n",
        "1. Install:\n",
        "  math:\n",
        "    x = 1\n",
    );
    assert!(try_parse_text(math_in_item.as_bytes()).is_err());

    // A fence inside a theorem body is accepted and nested under the
    // theorem.
    let in_theorem = concat!(
        "document:\n  title: \"T\"\n\n",
        "theorem:\n",
        "  Claim.\n",
        "  ```\n",
        "  x = y\n",
        "  ```\n",
    );
    let module = parse_text(in_theorem.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::TheoremLike { body, .. } => {
            assert_eq!(body.len(), 2);
            match &body[1].kind {
                NodeKind::CodeBlock { code, .. } => assert_eq!(code, "x = y"),
                other => panic!("expected code block, got {other:?}"),
            }
        }
        other => panic!("expected theorem, got {other:?}"),
    }
}

#[test]
fn test_long_fences_and_untagged_blocks() {
    let four_backtick = concat!("document:\n  title: \"T\"\n\n", "````\n", "```\n", "````\n",);
    let module = parse_text(four_backtick.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::CodeBlock { language, code } => {
            assert_eq!(language, &None);
            assert_eq!(code, "```");
        }
        other => panic!("expected code block, got {other:?}"),
    }

    let untagged = concat!("document:\n  title: \"T\"\n\n", "```\n", "x = 1\n", "```\n",);
    let module = parse_text(untagged.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::CodeBlock { language, .. } => assert_eq!(language, &None),
        other => panic!("expected code block, got {other:?}"),
    }

    let extra_info = concat!(
        "document:\n  title: \"T\"\n\n",
        "```python extra words\n",
        "x = 1\n",
        "```\n",
    );
    let module = parse_text(extra_info.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::CodeBlock { language, .. } => assert_eq!(language.as_deref(), Some("python")),
        other => panic!("expected code block, got {other:?}"),
    }
}

#[test]
fn test_inline_triple_backticks_stay_prose() {
    let src = concat!("document:\n  title: \"T\"\n\n", "```x``` is inline code\n",);
    let module = parse_text(src.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::Paragraph { inlines } => {
            assert!(inlines
                .iter()
                .any(|i| matches!(i, Inline::Code(c) if c == "x")));
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}

#[test]
fn test_malformed_code_blocks_fail() {
    let unterminated = concat!("document:\n  title: \"T\"\n\n", "```python\n", "x = 1\n",);
    let err = try_parse_diagnostics(unterminated.as_bytes()).expect_err("unterminated");
    assert!(err.message.contains("unterminated"), "{}", err.message);
    let start = unterminated.find("```python").unwrap() as u32;
    assert_eq!(err.primary.expect("located").byte_start, start);

    let forbidden = concat!(
        "document:\n  title: \"T\"\n\n",
        "```\n",
        "print(\"\\end{TerseCode}\")\n",
        "```\n",
    );
    let err = try_parse_diagnostics(forbidden.as_bytes()).expect_err("forbidden sequence");
    assert!(err.message.contains("TerseCode"), "{}", err.message);
    let line_start = forbidden.find("print(").unwrap() as u32;
    assert_eq!(err.primary.expect("located").byte_start, line_start);

    let forbidden_spaced = concat!(
        "document:\n  title: \"T\"\n\n",
        "```\n",
        "\\end {TerseCode}\n",
        "```\n",
    );
    assert!(try_parse_diagnostics(forbidden_spaced.as_bytes()).is_err());

    // A content line `\end{lstlisting}` is accepted (harmless under
    // `TerseCode`).
    let harmless = concat!(
        "document:\n  title: \"T\"\n\n",
        "```\n",
        "\\end{lstlisting}\n",
        "```\n",
    );
    assert!(try_parse_text(harmless.as_bytes()).is_ok());
}

#[test]
fn test_math_rejects_execution() {
    let cases = [
        (
            "document:\n  title: \"T\"\n\nmath:\n  \\input{evil}\n",
            "direct \\input",
        ),
        (
            "document:\n  title: \"T\"\n\nmath:\n  \\write18{rm -rf /}\n",
            "direct \\write18",
        ),
        (
            "document:\n  title: \"T\"\n\nmath:\n  \\csname foo\\endcsname\n",
            "direct \\csname",
        ),
        (
            "document:\n  title: \"T\"\n\nmath:\n  \\def\\x{1}\n",
            "macro definition",
        ),
        (
            "document:\n  title: \"T\"\n\nmath:\n  \\catcode`\\~=13\n",
            "catcode change",
        ),
        (
            "document:\n  title: \"T\"\n\nmath:\n  \\notacommand\n",
            "unknown command",
        ),
        (
            "document:\n  title: \"T\"\n\nmath:\n  ^^49\n",
            "^^ input encoding",
        ),
        (
            "document:\n  title: \"T\"\n\nmath:\n  \\frac{\\input{x}}{2}\n",
            "nested \\input inside an allowed command's argument",
        ),
    ];
    for (src, label) in cases {
        assert!(
            try_parse_text(src.as_bytes()).is_err(),
            "expected rejection for: {label}"
        );
    }

    // TeX comment rules still apply: a `%`-introduced comment does not
    // itself trigger a rejection, and an escaped `\%` is a literal percent
    // rather than the start of a comment.
    let commented = concat!(
        "document:\n  title: \"T\"\n\n",
        "math:\n  a + b % a harmless comment mentioning \\input\n  \\% literal percent\n",
    );
    assert!(try_parse_text(commented.as_bytes()).is_ok());

    // No process runner is available to inline math validation at all;
    // rejection happens purely from the payload bytes.
    let inline_forbidden = "document:\n  title: \"T\"\n\nUnsafe: \\(\\input{evil}\\)\n";
    assert!(try_parse_text(inline_forbidden.as_bytes()).is_err());
}

#[test]
fn test_raw_payload_trivia_is_opaque() {
    let src = concat!(
        "document:\n  title: \"T\"\n\n",
        "tex:\n",
        "  \\begin{tikzpicture} % a comment\n",
        "  // not a Terse comment, just bytes\n",
        "\n",
        "    \\draw (0,0) -- (1,1);\n",
        "  \\end{tikzpicture}\n",
        "\n",
        "After the raw block.\n",
    );
    let module = parse_text(src.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::RawTex { payload } => {
            assert_eq!(
                payload,
                "\\begin{tikzpicture} % a comment\n// not a Terse comment, just bytes\n\n  \\draw (0,0) -- (1,1);\n\\end{tikzpicture}"
            );
        }
        other => panic!("expected raw tex, got {other:?}"),
    }
    // The following dedented paragraph is a distinct node, outside the raw
    // block.
    match &module.blocks[1].kind {
        NodeKind::Paragraph { .. } => {}
        other => panic!("expected paragraph after raw block, got {other:?}"),
    }

    // CRLF payload line endings and leading tabs on a whitespace-only
    // blank line inside the payload survive without perturbing the
    // structural stack (the blank line carries no structural indent, so
    // its own leading bytes are unconstrained).
    let crlf_src: &[u8] = b"document:\r\n  title: \"T\"\r\n\r\ntex:\r\n  a\r\n \t \r\n  b\r\n";
    let module = parse_text(crlf_src);
    match &module.blocks[0].kind {
        NodeKind::RawTex { payload } => {
            assert_eq!(payload, "a\r\n \t \r\nb");
        }
        other => panic!("expected raw tex, got {other:?}"),
    }
}

#[test]
fn test_dollar_math_rejects_execution() {
    let pairs = [
        (r"$\input{evil}$", r"\(\input{evil}\)"),
        (r"$\write18{x}$", r"\(\write18{x}\)"),
        (
            r"$\frac{\csname x\endcsname}{2}$",
            r"\(\frac{\csname x\endcsname}{2}\)",
        ),
    ];
    for (dollar, paren) in pairs {
        let dollar_src = format!("document:\n  title: \"T\"\n\nUnsafe: {dollar}\n");
        let paren_src = format!("document:\n  title: \"T\"\n\nUnsafe: {paren}\n");
        let dollar_err = try_parse_diagnostics(dollar_src.as_bytes()).expect_err(dollar);
        let paren_err = try_parse_diagnostics(paren_src.as_bytes()).expect_err(paren);
        assert_eq!(dollar_err.code, paren_err.code, "{dollar}");
    }
}

fn equation_parts(node: &crate::semantic::Node) -> (Option<&str>, bool, &str) {
    match &node.kind {
        NodeKind::Equation {
            id,
            numbered,
            payload,
        } => (id.as_deref(), *numbered, payload.as_str()),
        other => panic!("expected equation, got {other:?}"),
    }
}

#[test]
fn test_multiline_dollar_display_splits_paragraph() {
    let src = "document:\n  title: \"T\"\n\nwhere\n$$\na = b +\n  c\n$$\nholds.\n";
    let module = parse_text(src.as_bytes());
    assert_eq!(module.blocks.len(), 3, "{:?}", module.blocks);
    assert_eq!(node_text(&module.blocks[0]), "where");
    assert_eq!(
        equation_parts(&module.blocks[1]),
        (None, false, "a = b +\n  c")
    );
    assert_eq!(node_text(&module.blocks[2]), "holds.");

    // CRLF inside the payload survives.
    let crlf = "document:\r\n  title: \"T\"\r\n\r\n$$\r\na = b +\r\n  c\r\n$$\r\n";
    let module = parse_text(crlf.as_bytes());
    assert_eq!(
        equation_parts(&module.blocks[0]),
        (None, false, "a = b +\r\n  c")
    );

    // Accepted inside a theorem body, nested under it.
    let thm = "document:\n  title: \"T\"\n\ntheorem:\n  Claim.\n  $$\n  x = y\n  $$\n";
    let module = parse_text(thm.as_bytes());
    match &module.blocks[0].kind {
        NodeKind::TheoremLike { body, .. } => {
            assert_eq!(body.len(), 2);
            assert_eq!(equation_parts(&body[1]), (None, false, "x = y"));
        }
        other => panic!("expected theorem, got {other:?}"),
    }
}

#[test]
fn test_single_line_dollar_display() {
    let src = "document:\n  title: \"T\"\n\n$$ E = mc^2 $$\n";
    let module = parse_text(src.as_bytes());
    assert_eq!(module.blocks.len(), 1);
    assert_eq!(
        equation_parts(&module.blocks[0]),
        (None, false, " E = mc^2 ")
    );

    let split = "document:\n  title: \"T\"\n\n$$ a = b +\nc $$\n";
    let module = parse_text(split.as_bytes());
    assert_eq!(
        equation_parts(&module.blocks[0]),
        (None, false, " a = b +\nc ")
    );
}

#[test]
fn test_malformed_dollar_display_fails() {
    let head = "document:\n  title: \"T\"\n\n";
    let cases = [
        (format!("{head}$$\nx\n"), "$$", "unterminated"),
        (
            format!("{head}$$ x $$ trailing\n"),
            "$$ x $$ trailing",
            "after",
        ),
        (format!("{head}- item\n  $$ x $$\n"), "$$ x $$", "list item"),
    ];
    for (src, at, needle) in cases {
        let err = try_parse_diagnostics(src.as_bytes()).expect_err(&src);
        assert!(err.message.contains(needle), "{src:?}: {}", err.message);
        let span = err.primary.expect("located");
        let start = src.rfind(at).expect("marker present") as u32;
        assert_eq!(span.byte_start, start, "{src:?}: span {span:?}");
    }

    // An escaped leading dollar is prose.
    let escaped = format!("{head}\\$$ signs are fine\n");
    let module = parse_text(escaped.as_bytes());
    assert_eq!(node_text(&module.blocks[0]), "$$ signs are fine");
}

#[test]
fn test_blank_or_empty_dollar_display_fails() {
    let head = "document:\n  title: \"T\"\n\n";
    // A blank line inside the delimiters is located at the blank line.
    let trailing = format!("{head}$$\nx = 1\n\n$$\n");
    let leading = format!("{head}$$\n\nx = 1\n$$\n");
    for (src, blank_after) in [(&trailing, "x = 1\n"), (&leading, "$$\n")] {
        let err = try_parse_diagnostics(src.as_bytes()).expect_err(src);
        assert!(
            err.message.contains("blank line"),
            "{src:?}: {}",
            err.message
        );
        let blank_start = (src.find(blank_after).unwrap() + blank_after.len()) as u32;
        assert_eq!(
            err.primary.expect("located").byte_start,
            blank_start,
            "{src:?}"
        );
    }
    // An empty or whitespace-only payload is located at the opening `$$`.
    for body in ["$$ $$\n", "$$$$\n", "$$\n$$\n"] {
        let src = format!("{head}{body}");
        let err = try_parse_diagnostics(src.as_bytes()).expect_err(&src);
        assert!(err.message.contains("empty"), "{src:?}: {}", err.message);
        assert_eq!(
            err.primary.expect("located").byte_start,
            head.len() as u32,
            "{src:?}"
        );
    }
}
