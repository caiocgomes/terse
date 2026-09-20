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
    let cases: [&[u8]; 5] = [
        b"document:\n  title: \"T\"\n\n*emph [link*text](dest)\n",
        b"document:\n  title: \"T\"\n\n^[outer ^[inner] still]\n",
        b"document:\n  title: \"T\"\n\n`unterminated code\n",
        b"document:\n  title: \"T\"\n\nAn unknown \\q escape.\n",
        b"document:\n  title: \"T\"\n\nAn unescaped ***run*** here.\n",
    ];
    for src in cases {
        assert!(try_parse_text(src).is_err(), "expected failure for {src:?}");
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
                NodeKind::Equation { id, payload } => {
                    assert_eq!(id.as_deref(), Some("transfer-eq"));
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
        NodeKind::Table { id, header, rows, .. } => {
            assert_eq!(id.as_deref(), Some("costs"));
            assert_eq!(header, &vec!["Policy".to_string(), "Cost, USD".to_string()]);
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0], vec!["Baseline".to_string(), "12".to_string()]);
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

    let ragged_table = concat!(
        "document:\n  title: \"T\"\n\n",
        "table:\n",
        "  caption: \"C\"\n",
        "  header: [\"A\", \"B\"]\n",
        "  rows:\n",
        "    - [\"only one\"]\n",
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
        NodeKind::Equation { id, payload } => {
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
