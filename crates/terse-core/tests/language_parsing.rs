use terse_core::source::{FileId, SourceFile};
use terse_core::syntax::{blocks, lexer};
use terse_core::{compile, semantic, InputSnapshot};

fn try_parse(bytes: &[u8]) -> Result<semantic::ParsedModule, ()> {
    let file = SourceFile::new(FileId(0), "entry.trs", bytes.to_vec()).map_err(|_| ())?;
    let lines = lexer::lex_lines(file.text()).map_err(|_| ())?;
    let module_blocks =
        blocks::parse_module(&lines, file.text(), file.id, file.base_offset()).map_err(|_| ())?;
    semantic::lower(module_blocks, file.id).map_err(|_| ())
}

const HEADER: &str = "document:\n  title: \"T\"\n\n";

#[test]
fn test_invalid_metadata_context() {
    // Missing title fails at the document header.
    assert!(try_parse(b"document:\n  subtitle: \"No Title\"\n\nHi.\n").is_err());

    // Both affiliation forms on one author.
    let both = concat!(
        "document:\n  title: \"T\"\n  authors:\n",
        "    - name: \"A\"\n      affiliation: \"One\"\n      affiliations: [\"Two\"]\n\nHi.\n",
    );
    assert!(try_parse(both.as_bytes()).is_err());

    // Duplicate title field.
    assert!(try_parse(b"document:\n  title: \"X\"\n  title: \"Y\"\n\nHi.\n").is_err());

    // Metadata after content.
    assert!(try_parse(b"# Heading\n\ndocument:\n  title: \"X\"\n").is_err());

    // Unsupported abstract headings/declarations.
    let bad_abstract_heading =
        "document:\n  title: \"T\"\n  abstract:\n    # not allowed\n\nHi.\n";
    assert!(try_parse(bad_abstract_heading.as_bytes()).is_err());
}

#[test]
fn test_visual_attributes_rejected() {
    let cases: Vec<(&str, String)> = vec![
        (
            "figure width",
            format!("{HEADER}figure \"a.pdf\" [width: 80mm]:\n  caption: \"C\"\n  alt: \"A\"\n"),
        ),
        (
            "heading font",
            format!("{HEADER}# Title [font: 18pt]\n"),
        ),
        (
            "equation margin",
            format!("{HEADER}math [margin: 2em]:\n  x = 1\n"),
        ),
        (
            "table color",
            format!(
                "{HEADER}table [color: red]:\n  caption: \"C\"\n  header: [\"A\"]\n  rows:\n    - [\"1\"]\n"
            ),
        ),
        (
            "proof placement",
            format!("{HEADER}proof [placement: here]:\n  Body.\n"),
        ),
        (
            "unknown id-context attribute",
            format!("{HEADER}# Title [bogus: x]\n"),
        ),
        (
            "invalid figure role",
            format!("{HEADER}figure \"a.pdf\" [role: floating]:\n  caption: \"C\"\n  alt: \"A\"\n"),
        ),
    ];

    for (name, src) in cases {
        assert!(
            try_parse(src.as_bytes()).is_err(),
            "case '{name}' should fail static validation without any engine call"
        );
    }
}

#[test]
fn test_raw_tex_build_and_strict_warning() {
    // Scenario: Raw TeX escape hatch [Acceptance J]. Exercises the public
    // `compile` entrypoint (the same one the CLI's `build`/`check` commands
    // use) end to end: normal generation preserves the exact dedented raw
    // bytes, and strict-mode warning collection reports `W-TEX-001` at the
    // raw block's own span without ever treating it as an error.
    let src = concat!(
        "document:\n  title: \"T\"\n\n",
        "tex:\n",
        "  \\begin{tikzpicture}\n",
        "    \\draw (0,0) -- (1,1);\n",
        "  \\end{tikzpicture}\n",
    );
    let file = SourceFile::new(FileId(0), "entry.trs", src.as_bytes().to_vec()).unwrap();
    let (diagnostics, plan) = compile(&InputSnapshot::single(file));
    assert!(diagnostics.is_empty(), "normal (non-strict) validation accepts raw TeX: {diagnostics:?}");
    let plan = plan.expect("valid artifact plan");

    let theme = terse_core::theme::academic();
    let generated = terse_core::latex::generate_document(&plan.module, &theme);
    assert!(
        generated.contains("\\begin{tikzpicture}\n  \\draw (0,0) -- (1,1);\n\\end{tikzpicture}"),
        "generated TeX must contain the exact dedented raw bytes:\n{generated}"
    );

    let warnings = semantic::collect_warnings(&plan.module);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].code, "W-TEX-001");
    let raw_node_span = plan
        .module
        .blocks
        .iter()
        .find_map(|n| matches!(n.kind, semantic::NodeKind::RawTex { .. }).then_some(n.span))
        .expect("a raw tex node exists");
    assert_eq!(warnings[0].primary, Some(raw_node_span));

    // A trailing EOF without a final newline is preserved too (no
    // synthesized trailing blank line in the payload).
    let no_trailing_newline = "document:\n  title: \"T\"\n\ntex:\n  \\relax";
    let file = SourceFile::new(FileId(0), "entry.trs", no_trailing_newline.as_bytes().to_vec()).unwrap();
    let (diagnostics, plan) = compile(&InputSnapshot::single(file));
    assert!(diagnostics.is_empty());
    match &plan.unwrap().module.blocks[0].kind {
        semantic::NodeKind::RawTex { payload } => assert_eq!(payload, "\\relax"),
        other => panic!("expected raw tex, got {other:?}"),
    }
}

#[test]
fn test_declarations_require_module_scope_via_compile() {
    // Uses the same public `compile` entrypoint as the CLI, confirming the
    // whole pipeline (not just the internal parser) enforces this rule
    // and that this never needs a running engine.
    let bytes = format!("{HEADER}theorem:\n  refs:\n    x: doi:10.1/x\n")
        .into_bytes();
    let file = SourceFile::new(FileId(0), "entry.trs", bytes).unwrap();
    let (diagnostics, plan) = compile(&InputSnapshot::single(file));
    assert!(!diagnostics.is_empty());
    assert!(plan.is_none());
}
