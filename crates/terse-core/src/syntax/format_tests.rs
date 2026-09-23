use crate::semantic;
use crate::semantic::projection;
use crate::source::{FileId, SourceFile};
use crate::syntax::blocks;
use crate::syntax::format::format_source;
use crate::syntax::lexer;

fn file(text: &str) -> SourceFile {
    SourceFile::new(FileId(0), "test.trs", text.as_bytes().to_vec()).unwrap()
}

fn project_str(text: &str) -> projection::Projection {
    let f = file(text);
    let lines = lexer::lex_lines(f.text()).unwrap();
    let blocks = blocks::parse_module(&lines, f.text(), f.id, f.base_offset()).unwrap();
    let module = semantic::lower(blocks, f.id).unwrap();
    projection::project(&module, &std::collections::BTreeMap::new())
}

#[test]
fn test_formatter_preserves_opaque_bytes() {
    let src = "# Title\n\nmath:\n  a  +   b\t\n\n  \\alpha\n\ntex:\n  \\foo{  bar  }\r\n";
    let f = file(src);
    let formatted = format_source(&f).expect("valid module");
    let formatted_text = String::from_utf8(formatted).unwrap();

    // Non-opaque structural spacing (heading + surrounding blank lines)
    // was canonicalized...
    assert!(formatted_text.starts_with("# Title\n\n"));

    // ...but the math/tex opaque payload bytes, including internal
    // trailing whitespace, blank lines, and CRLF, are byte-identical.
    assert!(formatted_text.contains("a  +   b\t\n"));
    assert!(formatted_text.contains("\\foo{  bar  }\r\n"));
}

#[test]
fn test_format_allows_unresolved_cross_file_reference() {
    // A forward reference to an id defined in a sibling module (only
    // visible after project-wide include expansion, which `fmt` never
    // performs) must not fail formatting with a spurious E-XREF-001 —
    // found via the group-26 clean-checkout e2e test against the
    // multi-file full-paper fixture.
    let src = "# Title\n\nSee {ref: defined-elsewhere} for details.\n";
    let f = file(src);
    let formatted = format_source(&f).expect("cross-file forward reference must not fail single-file formatting");
    assert!(String::from_utf8(formatted).unwrap().contains("{ref: defined-elsewhere}"));
}

#[test]
fn test_format_is_semantic_and_idempotent() {
    let src = "# Title\n\n\n\nThis is a paragraph.\n\n\n## Sub\n\nAnother paragraph.\n";
    let f = file(src);
    let once = format_source(&f).expect("valid module");
    let once_text = String::from_utf8(once.clone()).unwrap();

    // Second pass over the formatted output must be a fixed point.
    let f2 = SourceFile::new(FileId(0), "test.trs", once.clone()).unwrap();
    let twice = format_source(&f2).expect("still valid after formatting");
    assert_eq!(once, twice, "formatting must be idempotent");

    // Formatting must not change parsed semantic meaning.
    let original_text = f.text().to_string();
    let before = project_str(&original_text);
    let after = project_str(&once_text);
    assert_eq!(
        before, after,
        "formatting changed the semantic projection of the document"
    );
}

#[test]
fn test_format_rejects_invalid_input_without_side_effects() {
    let src = "math:\n  \\input{evil}\n";
    let f = file(src);
    let result = format_source(&f);
    assert!(
        result.is_err(),
        "rejected math (execution attempt) must fail to format, not silently pass through"
    );
}

#[test]
fn test_format_preserves_dollar_math() {
    let src = "# Title\n\nInline $x^2$ here.\n\n$$ E = mc^2 $$\n\n$$\na  +   b\n    c\n$$\n";
    let once = String::from_utf8(format_source(&file(src)).expect("valid module")).unwrap();
    assert!(once.contains("Inline $x^2$ here."), "{once}");
    assert!(once.contains("$$ E = mc^2 $$"), "{once}");
    assert!(once.contains("$$\na  +   b\n    c\n$$"), "{once}");
    assert!(!once.contains("\\(") && !once.contains("math:"), "{once}");
    let twice = String::from_utf8(format_source(&file(&once)).expect("valid module")).unwrap();
    assert_eq!(once, twice);
}
