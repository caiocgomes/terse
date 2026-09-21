//! Integration coverage for the closed `.theme` schema against the
//! `tests/fixtures/themes/` public two-theme fixture (task group 9). The
//! compiler core never touches the filesystem itself; these tests load
//! fixture bytes the same way an application layer would before calling
//! into `terse_core::theme`.

use std::path::Path;
use terse_core::source::{FileId, SourceFile};
use terse_core::theme::{resolve_theme, ResolvedTheme};

fn fixtures_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/themes")
}

fn load_theme(name: &str, file: &str) -> ResolvedTheme {
    let path = fixtures_dir().join(file);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("reading {path:?}: {e}"));
    let source = SourceFile::new(FileId(0), file, bytes).expect("valid theme source");
    resolve_theme(name, &source).expect("fixture theme resolves")
}

#[test]
fn test_public_theme_has_no_private_dependencies() {
    // The demonstration "magalu" theme resolves using only TeX-distributed
    // fonts and a repo-local logo path: no host-only font family, network
    // resource, or absolute/escaping path is accepted by the schema.
    let magalu = load_theme("magalu", "magalu.theme");
    assert_eq!(magalu.body_font.as_deref(), Some("tex-gyre-heros"));
    let logo = magalu.logo_path.as_deref().expect("declares a logo");
    assert!(!logo.contains("://"));
    assert!(!logo.starts_with('/'));
    assert!(!logo.split('/').any(|seg| seg == ".."));

    let academic = load_theme("academic", "academic.theme");
    assert_eq!(academic.body_font.as_deref(), Some("libertinus-otf"));
    assert_eq!(academic.watermark_kind, "none");
    assert_eq!(academic.logo_path, None);
}

/// `plain-latex-default` scenario "No theme is the plain article", at the
/// resolution level: a theme file that sets nothing resolves to exactly the
/// compiler defaults, and its style pulls neither a font nor a page
/// package. The scaffold `terse init` writes is such a file.
#[test]
fn test_empty_theme_equals_defaults() {
    let source = SourceFile::new(
        FileId(0),
        "empty.theme",
        b"// A theme that sets nothing is the plain article.\n".to_vec(),
    )
    .unwrap();
    let empty = resolve_theme("academic", &source).expect("a comment-only theme resolves");
    assert_eq!(empty, terse_core::theme::academic());

    let style = terse_core::latex::generate_style(&empty, &[], None);
    for absent in ["fontspec", "\\setmainfont", "geometry", "libertinus"] {
        assert!(!style.contains(absent), "the plain default must not load {absent}:\n{style}");
    }
}

#[test]
fn test_logo_resolves_from_theme_directory() {
    // The logo path in the theme file is declared relative to the
    // declaring `.theme` file; resolving it against that file's directory
    // (not the invocation directory or project root) must find the actual
    // asset shipped alongside the fixture.
    let magalu = load_theme("magalu", "magalu.theme");
    let logo_rel = magalu.logo_path.expect("declares a logo");
    let resolved = fixtures_dir().join(&logo_rel);
    assert!(
        resolved.is_file(),
        "expected {resolved:?} to exist relative to the theme's own directory"
    );
}

#[test]
fn test_theme_invariant_projection() {
    // A lightweight, group-9-scoped precursor to the full canonical
    // semantic projection built in group 17: resolving both fixture
    // themes must never observe or alter any authored document content,
    // only produce independent typed presentation values.
    let academic = load_theme("academic", "academic.theme");
    let magalu = load_theme("magalu", "magalu.theme");
    assert_ne!(academic.watermark_kind, magalu.watermark_kind);
    assert_ne!(academic.body_font, magalu.body_font);
    assert!(academic.body_font.is_some() && magalu.body_font.is_some());
    assert_ne!(academic.citation_style, magalu.citation_style);

    let module = terse_core::semantic::ParsedModule {
        file_id: FileId(0),
        metadata: None,
        references: vec![],
        blocks: vec![terse_core::semantic::Node {
            kind: terse_core::semantic::NodeKind::Paragraph {
                inlines: vec![terse_core::syntax::inlines::Inline::Text(
                    "Invariant body.".to_string(),
                )],
            },
            span: terse_core::source::SourceSpan::new(FileId(0), 0, 0),
        }],
    };
    let a = terse_core::latex::generate_document(&module, &academic);
    let m = terse_core::latex::generate_document(&module, &magalu);
    assert_eq!(a, m, "authored body/main TeX bytes must be theme-invariant");
}
