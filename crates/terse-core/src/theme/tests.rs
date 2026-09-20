use super::*;
use crate::semantic::{Node, NodeKind, ParsedModule};
use crate::source::{FileId, SourceFile, SourceSpan};
use crate::syntax::inlines::Inline;

#[test]
fn test_academic_defaults_are_stable() {
    assert_eq!(academic(), academic());
}

fn resolve_str(name: &str, text: &str) -> Result<ResolvedTheme, ThemeError> {
    let file = SourceFile::new(FileId(0), "t.theme", text.as_bytes().to_vec()).unwrap();
    resolve_theme(name, &file)
}

#[test]
fn test_base_and_wide_selectors() {
    let theme = resolve_str(
        "academic",
        "figure:\n  align: center\n  default-width: 60%\n\nfigure[role=wide]:\n  default-width: 95%\n",
    )
    .unwrap();
    assert_eq!(theme.figure_align, "center");
    assert_eq!(theme.figure_default_width_pct, 60);
    assert_eq!(theme.figure_wide_width_pct, 95);
}

#[test]
fn test_themes_cannot_select_content() {
    for bad in [
        "#intro:\n  color: red\n",
        "figure > table:\n  align: center\n",
        "@if dark:\n  color: black\n",
    ] {
        assert!(resolve_str("academic", bad).is_err());
    }
}

#[test]
fn test_role_overrides_base_width() {
    let theme = resolve_str(
        "academic",
        "figure:\n  default-width: 75%\n\nfigure[role=wide]:\n  default-width: 100%\n",
    )
    .unwrap();
    assert_eq!(theme.figure_default_width_pct, 75);
    assert_eq!(theme.figure_wide_width_pct, 100);
}

#[test]
fn test_theme_selector_order_irrelevant() {
    let a = resolve_str(
        "academic",
        "page:\n  size: letter\n\nfigure[role=wide]:\n  default-width: 90%\n",
    )
    .unwrap();
    let b = resolve_str(
        "academic",
        "figure[role=wide]:\n  default-width: 90%\n\npage:\n  size: letter\n",
    )
    .unwrap();
    assert_eq!(a, b);
}

#[test]
fn test_typed_theme_values_rejected() {
    for bad in [
        "page:\n  size: tabloid\n",
        "figure:\n  default-width: wide\n",
        "citation:\n  style: footnote\n",
        "watermark:\n  kind: confidential\n",
    ] {
        assert!(
            resolve_str("academic", bad).is_err(),
            "expected rejection for: {bad}"
        );
    }
}

#[test]
fn test_theme_visibility_and_geometry_bounds() {
    for bad in [
        "page:\n  margin: 0cm\n",
        "page:\n  margin: 12cm\n",
        "figure:\n  default-width: 0%\n",
        "watermark:\n  opacity: 0.9\n",
        "body:\n  color: #ffffff\n",
    ] {
        assert!(
            resolve_str("academic", bad).is_err(),
            "expected bounds rejection for: {bad}"
        );
    }
}

#[test]
fn test_external_theme_resources_fail() {
    for bad in [
        "logo:\n  source: https://example.com/logo.png\n",
        "logo:\n  source: /etc/passwd\n",
        "logo:\n  source: ../../secret.png\n",
    ] {
        assert!(
            resolve_str("magalu", bad).is_err(),
            "expected external-resource rejection for: {bad}"
        );
    }
    let ok = resolve_str("magalu", "logo:\n  source: assets/logo.png\n").unwrap();
    assert_eq!(ok.logo_path.as_deref(), Some("assets/logo.png"));
}

#[test]
fn test_theme_switch_keeps_body_bytes() {
    let academic = academic();
    let magalu = resolve_str(
        "magalu",
        "watermark:\n  kind: internal-use\n  opacity: 0.08\n\nlogo:\n  source: assets/logo.png\n",
    )
    .unwrap();
    assert_ne!(academic.watermark_kind, magalu.watermark_kind);
    // The body/bibliography generator ignores theme settings entirely
    // (see `crate::latex::generate_document`), so two distinct resolved
    // themes never change generated body bytes.
    let module = ParsedModule {
        file_id: FileId(0),
        metadata: None,
        references: vec![],
        blocks: vec![Node {
            kind: NodeKind::Paragraph {
                inlines: vec![Inline::Text("Same body under any theme.".to_string())],
            },
            span: SourceSpan::new(FileId(0), 0, 0),
        }],
    };
    let a = crate::latex::generate_document(&module, &academic);
    let m = crate::latex::generate_document(&module, &magalu);
    assert_eq!(a, m);
}

#[test]
fn test_furniture_excluded_from_authored_ast() {
    // Applying a theme with watermark/logo furniture enabled must not add,
    // remove, or reorder authored nodes: furniture is drawn by the style
    // layer, never injected into the semantic AST.
    let module = ParsedModule {
        file_id: FileId(0),
        metadata: None,
        references: vec![],
        blocks: vec![Node {
            kind: NodeKind::Paragraph {
                inlines: vec![Inline::Text("Body content.".to_string())],
            },
            span: SourceSpan::new(FileId(0), 0, 0),
        }],
    };
    let plain = academic();
    let furnished = resolve_str(
        "magalu",
        "watermark:\n  kind: internal-use\n  opacity: 0.1\n\nlogo:\n  source: assets/logo.png\n",
    )
    .unwrap();
    assert_eq!(module.blocks.len(), 1);
    // Resolving/using either theme never reads or mutates `module.blocks`;
    // generation only reads theme fields, proven by identical output length
    // characteristics (body text present, no extra furniture text).
    let a = crate::latex::generate_document(&module, &plain);
    let b = crate::latex::generate_document(&module, &furnished);
    assert_eq!(a, b);
    assert!(!a.contains("INTERNAL USE"));
    assert!(!a.contains("TerseLogo"));
}
