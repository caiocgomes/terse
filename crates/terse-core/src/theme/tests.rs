use super::*;
use crate::semantic::{Node, NodeKind, ParsedModule};
use crate::theme::parse::ThemeParseError;
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

/// The source text a diagnostic points at. A theme file created here has
/// no BOM, so its `base_offset` is zero and span bytes index the text
/// directly. Slicing rather than comparing raw offsets keeps the
/// assertions readable and makes "failed at the wrong line" visible.
fn span_text<'a>(text: &'a str, span: SourceSpan) -> &'a str {
    &text[span.byte_start as usize..span.byte_end as usize]
}

/// The single resolve error a theme must produce, failing loudly if it
/// produced none or several: a test that accepts "one of the errors
/// matched" cannot tell a precise diagnostic from a scattergun.
fn sole_resolve_error(text: &str) -> ThemeResolveError {
    match resolve_str("academic", text) {
        Ok(_) => panic!("expected rejection for:\n{text}"),
        Err(ThemeError::Resolve(mut errors)) => {
            assert_eq!(errors.len(), 1, "expected exactly one error, got {errors:?}");
            errors.remove(0)
        }
        Err(other) => panic!("expected a resolve error, got {other:?}"),
    }
}

#[test]
fn test_themes_cannot_select_content() {
    // Every one of these is a selector that would reach into authored
    // content; the requirement is that validation fails *at that rule*,
    // so assert the offending selector text, not merely that it failed.
    for (bad, offending) in [
        ("#intro:\n  color: red\n", "#intro:"),
        ("figure > table:\n  align: center\n", "figure > table:"),
        ("@if dark:\n  color: black\n", "@if dark:"),
        (
            "page:\n  size: a4\n\n.dark table:\n  padding: 1.0\n",
            ".dark table:",
        ),
    ] {
        let file = SourceFile::new(FileId(0), "t.theme", bad.as_bytes().to_vec()).unwrap();
        match resolve_theme("academic", &file) {
            Err(ThemeError::Parse(ThemeParseError::InvalidSelector { span })) => {
                assert_eq!(
                    span_text(bad, span),
                    offending,
                    "diagnostic must point at the offending selector in:\n{bad}"
                );
            }
            other => panic!("expected InvalidSelector for {bad:?}, got {other:?}"),
        }
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
        // The components this change adds carry typed values too: an
        // unknown value must fail at the property rather than reach LaTeX.
        "title:\n  layout: poster\n",
        "title:\n  align: justified\n",
        "theorem:\n  style: fancy\n",
        "theorem[kind=lemma]:\n  style: fancy\n",
        "theorem[kind=corollary]:\n  style: plain\n",
        "table:\n  padding: loose\n",
        "table:\n  rules: double\n",
        "table:\n  header: italic\n",
        "bibliography:\n  size: enormous\n",
        "bibliography:\n  item-spacing: wide\n",
        // Heading weight had no row here, and that absence is exactly why
        // it shipped as the one unvalidated property in the schema.
        "heading.1:\n  weight: bfseries\n",
        "heading.2:\n  weight: heavy\n",
        "heading.3:\n  weight: 700\n",
        // A logo path is emitted into a macro argument, so its charset is
        // part of its type, not just its shape.
        "logo:\n  source: assets/logo}.png\n",
        // Non-finite numbers pass a two-sided bounds check (`x <= 0.0 ||
        // x > N` is false-false for NaN) and reach LaTeX as a word.
        "page:\n  margin: NaNcm\n",
        "table:\n  padding: NaN\n",
        "watermark:\n  opacity: inf\n",
    ] {
        assert!(
            resolve_str("academic", bad).is_err(),
            "expected rejection for: {bad}"
        );
    }
}

/// A theme is declarative data, never code: the "Independent declarative
/// theme files" requirement says arbitrary expressions, raw TeX and
/// executable hooks MUST fail validation. Two properties used to reach a
/// TeX-executable position carrying the theme's own string — `heading.N
/// weight` was interpolated straight after a backslash, and `logo source`
/// into a macro argument a `}` can close early. Both are rejected at the
/// offending property, and a valid theme's generated style contains no
/// trace of an injection attempt.
#[test]
fn test_theme_cannot_inject_raw_tex() {
    for (text, component, property) in [
        (
            "heading.1:\n  weight: bfseries\\LaTeX\\ INJECTED\n",
            "heading.1",
            "weight",
        ),
        (
            "logo:\n  source: logo.png}\\input{/etc/passwd\n",
            "logo",
            "source",
        ),
    ] {
        let error = sole_resolve_error(text);
        let (span, got_component, got_property) = match &error {
            ThemeResolveError::InvalidValue { span, component, property, .. } => {
                (*span, component.clone(), property.clone())
            }
            other => panic!("expected an InvalidValue rejection, got {other:?}"),
        };
        assert_eq!((got_component.as_str(), got_property.as_str()), (component, property));
        assert!(
            span_text(text, span).contains(property),
            "the diagnostic must point at the offending property, got {:?}",
            span_text(text, span)
        );
    }

    // Nothing an injection attempt carries survives into a valid theme's
    // style: the accepted tokens are mapped to fixed commands, never
    // interpolated.
    let theme = resolve_str(
        "academic",
        "heading.1:\n  weight: bold\n\nlogo:\n  source: assets/logo.png\n",
    )
    .expect("the semantic tokens resolve");
    let style = crate::latex::generate_style(&theme, &[], None);
    for forbidden in ["INJECTED", "\\input", "/etc/passwd", "\\bold"] {
        assert!(
            !style.contains(forbidden),
            "generated style must not contain {forbidden:?}"
        );
    }
    assert!(style.contains("\\bfseries"), "`bold` maps to the real command");
}

/// `close-verification-gaps` scenario "Theorem kind inherits its base".
///
/// The precedence requirement says the theorem base provides defaults for
/// every theorem-like variant, with kind-specific settings overriding it.
/// Before this change `resolve` rejected every role selector except
/// `figure[role=wide]`, so the sentence was unimplementable.
#[test]
fn test_theorem_kind_overrides_base() {
    let base_then_kind = resolve_str(
        "academic",
        "theorem:\n  style: plain\n\ntheorem[kind=lemma]:\n  style: remark\n",
    )
    .expect("a theorem base plus one kind override resolves");

    // Lemma takes the kind value; every other theorem-like kind inherits
    // the base, including the kind named after the component itself.
    assert_eq!(theorem_style_of(&base_then_kind, "lemma"), "remark");
    for kind in ["theorem", "proposition", "definition", "example", "remark"] {
        assert_eq!(
            theorem_style_of(&base_then_kind, kind),
            "plain",
            "{kind} must inherit the theorem base"
        );
    }

    // Declaration order is not a cascade: the same two rules reversed
    // resolve identically.
    let kind_then_base = resolve_str(
        "academic",
        "theorem[kind=lemma]:\n  style: remark\n\ntheorem:\n  style: plain\n",
    )
    .expect("the reversed declaration order resolves");
    assert_eq!(base_then_kind, kind_then_base);

    // And the distinction survives into the generated style, which is
    // where it actually has to matter.
    let style = crate::latex::generate_style(&base_then_kind, &[], None);
    assert_eq!(governing_theorem_style(&style, "terselemma"), "remark");
    assert_eq!(governing_theorem_style(&style, "tersetheorem"), "plain");
    assert_eq!(governing_theorem_style(&style, "terseexample"), "plain");
}

/// The `\theoremstyle` in force where `env` is declared. `\theoremstyle` is
/// sticky in `amsthm`, so the style that governs an environment is the last
/// one declared before its `\newtheorem`, not anything on the same line.
fn governing_theorem_style(style: &str, env: &str) -> String {
    let mut current = String::new();
    for line in style.lines() {
        if let Some(rest) = line.strip_prefix("\\theoremstyle{") {
            current = rest.trim_end_matches('}').to_string();
        }
        if line.contains(&format!("{{{env}}}")) {
            return current;
        }
    }
    panic!("the {env} environment is defined");
}

/// Reads one theorem-like kind's resolved style, by the same kind order the
/// semantic model uses.
fn theorem_style_of(theme: &ResolvedTheme, kind: &str) -> String {
    let index = ["theorem", "proposition", "lemma", "definition", "example", "remark"]
        .iter()
        .position(|k| *k == kind)
        .expect("a theorem-like kind");
    theme.theorem_style[index].clone()
}

#[test]
fn test_theme_visibility_and_geometry_bounds() {
    // The scenario requires checking to fail *at the offending property*,
    // so each case pins the component, the property and the exact source
    // text the diagnostic points at. Reporting the right component with a
    // placeholder position used to pass here and no longer does.
    for (bad, component, property, offending) in [
        (
            "page:\n  margin: 0cm\n",
            "page",
            "margin",
            "margin: 0cm",
        ),
        (
            "page:\n  margin: 12cm\n",
            "page",
            "margin",
            "margin: 12cm",
        ),
        (
            "figure:\n  default-width: 0%\n",
            "figure",
            "default-width",
            "default-width: 0%",
        ),
        (
            "watermark:\n  opacity: 0.9\n",
            "watermark",
            "opacity",
            "opacity: 0.9",
        ),
        (
            "body:\n  color: #ffffff\n",
            "body",
            "color",
            "color: #ffffff",
        ),
        (
            "table:\n  padding: 0.0\n",
            "table",
            "padding",
            "padding: 0.0",
        ),
    ] {
        match sole_resolve_error(bad) {
            ThemeResolveError::OutOfBounds {
                span,
                component: c,
                property: p,
                ..
            } => {
                assert_eq!((c.as_str(), p.as_str()), (component, property), "in:\n{bad}");
                assert_eq!(
                    span_text(bad, span),
                    offending,
                    "bounds diagnostic must point at the offending property in:\n{bad}"
                );
            }
            other => panic!("expected OutOfBounds for {bad:?}, got {other:?}"),
        }
    }

    // A role override violating the bound is reported at the role's rule,
    // not at the base rule that set the same property legally.
    let both = "figure:\n  default-width: 80%\n\nfigure[role=wide]:\n  default-width: 140%\n";
    match sole_resolve_error(both) {
        ThemeResolveError::OutOfBounds { span, .. } => {
            assert_eq!(span_text(both, span), "default-width: 140%");
        }
        other => panic!("expected OutOfBounds, got {other:?}"),
    }

    // "Valid values at documented bounds pass" — the other half of the
    // contract, which an is_err()-only test could never express.
    for good in [
        "figure:\n  default-width: 100%\n",
        "watermark:\n  kind: draft\n  opacity: 0.3\n",
        "bibliography:\n  item-spacing: 0.0em\n",
    ] {
        assert!(
            resolve_str("academic", good).is_ok(),
            "documented boundary value must be accepted:\n{good}"
        );
    }
}

#[test]
fn test_variant_attribute_key_is_fixed_per_component() {
    // The attribute key is part of the selector's meaning: `figure` varies
    // by semantic role, `theorem` by kind. Using the other component's key
    // is as wrong as inventing a value, and the two forms must not be
    // conflated by duplicate detection either.
    assert!(resolve_str("academic", "theorem[kind=lemma]:\n  style: plain\n").is_ok());

    for (bad, component) in [
        ("theorem[role=lemma]:\n  style: plain\n", "theorem"),
        ("figure[kind=wide]:\n  default-width: 90%\n", "figure"),
    ] {
        match sole_resolve_error(bad) {
            ThemeResolveError::UnsupportedRole { span, component: c } => {
                assert_eq!(c, component, "in:\n{bad}");
                assert_eq!(
                    span_text(bad, span),
                    bad.lines().next().unwrap(),
                    "must point at the offending selector in:\n{bad}"
                );
            }
            other => panic!("expected UnsupportedRole for {bad:?}, got {other:?}"),
        }
    }

    // Declaring both forms fails on the invalid key rather than being
    // reported as a duplicate selector, which would wrongly imply the two
    // spellings address the same thing.
    let both = "theorem[kind=lemma]:\n  style: plain\n\ntheorem[role=lemma]:\n  style: definition\n";
    match resolve_str("academic", both) {
        Err(ThemeError::Resolve(errors)) => {
            assert!(
                errors
                    .iter()
                    .any(|e| matches!(e, ThemeResolveError::UnsupportedRole { .. })),
                "expected UnsupportedRole, got {errors:?}"
            );
        }
        other => panic!("expected a resolve error, got {other:?}"),
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
    // Comparing `generate_document` across two themes cannot fail: that
    // function ignores the theme by construction, so the old assertion
    // held even if furniture *were* injected into the AST. The real claim
    // is a division of labour, so assert both halves of it: the body is
    // free of furniture, and the furniture genuinely exists in the style.
    let a = crate::latex::generate_document(&module, &plain);
    let b = crate::latex::generate_document(&module, &furnished);
    assert_eq!(a, b, "the body is theme-blind");
    assert!(a.contains("Body content."), "authored content survives");
    for furniture in ["INTERNAL USE", "TerseLogo", "AddToShipoutPictureBG"] {
        assert!(
            !a.contains(furniture),
            "{furniture} must not appear in the authored body"
        );
    }

    // The other half: the furnished theme really does draw this furniture,
    // from the style layer. Without this, a theme that silently dropped
    // its watermark and logo would satisfy the test above perfectly.
    let style = crate::latex::generate_style(&furnished, &[], None);
    assert!(style.contains("INTERNAL USE"), "the watermark is drawn by the style");
    assert!(style.contains("TerseLogo"), "the logo is defined by the style");
    let plain_style = crate::latex::generate_style(&plain, &[], None);
    assert!(
        !plain_style.contains("INTERNAL USE"),
        "a theme without a watermark draws none"
    );
}

/// `close-verification-gaps` scenario "Declared settings reach the output".
///
/// Every property the schema accepts must produce an observable difference
/// in the emitted `terse-style.sty`. This is table-driven on purpose: the
/// coverage assertion at the end compares the table's (component, property)
/// pairs against the schema itself, so a property added to
/// `known_properties` without a row here fails this test rather than
/// silently joining the set of settings that validate and do nothing.
#[test]
fn test_every_accepted_property_reaches_the_style() {
    // (component, role, property, declared value, substring the style must carry)
    let rows: &[(&str, Option<&str>, &str, &str, &str)] = &[
        ("page", None, "size", "letter", "letterpaper"),
        ("page", None, "margin", "3cm", "margin=3cm"),
        ("page", None, "columns", "2", "\\twocolumn"),
        ("body", None, "font", "tex-gyre-heros", "\\setmainfont{texgyreheros-regular.otf}"),
        ("body", None, "color", "#112233", "112233"),
        ("heading.1", None, "weight", "regular", "\\mdseries"),
        ("heading.1", None, "numbering", "roman", "\\Roman{terseheadingone}"),
        ("heading.2", None, "weight", "italic", "\\itshape"),
        ("heading.2", None, "numbering", "none", "TerseHeadingTwo"),
        ("heading.3", None, "weight", "small-caps", "\\scshape"),
        ("heading.3", None, "numbering", "decimal", "\\arabic{terseheadingthree}"),
        ("figure", None, "align", "left", "\\TerseFigureAlign"),
        ("figure", None, "default-width", "60%", "0.600\\linewidth"),
        ("figure", Some("wide"), "default-width", "95%", "0.950\\linewidth"),
        ("figure", None, "placement", "top", "\\TerseFigurePlacement"),
        ("citation", None, "style", "numeric", "style=numeric"),
        ("title", None, "layout", "cover", "\\clearpage"),
        ("title", None, "align", "left", "\\raggedright"),
        ("theorem", None, "style", "remark", "\\theoremstyle{remark}"),
        ("table", None, "padding", "1.4", "\\arraystretch}{1.4}"),
        ("table", None, "rules", "plain", "\\let\\toprule\\hline"),
        ("table", None, "header", "plain", "\\TerseTableHeaderCell}[1]{#1}"),
        ("bibliography", None, "size", "small", "\\bibfont}{\\small}"),
        ("bibliography", None, "item-spacing", "1.5em", "\\bibitemsep}{1.5em}"),
        ("watermark", None, "kind", "draft", "DRAFT"),
        ("watermark", None, "opacity", "0.2", "0.8"),
        ("watermark", None, "angle", "30", "{30}"),
        ("logo", None, "source", "assets/logo.png", "assets/logo.png"),
        ("logo", None, "width", "40%", "0.4\\linewidth"),
    ];

    // One selector block per (component, role), preserving row order.
    let mut blocks: Vec<(String, Vec<String>)> = Vec::new();
    for (component, role, property, value, _) in rows {
        let selector = match role {
            Some(r) => format!("{component}[role={r}]"),
            None => (*component).to_string(),
        };
        match blocks.iter_mut().find(|(s, _)| *s == selector) {
            Some((_, props)) => props.push(format!("  {property}: {value}\n")),
            None => blocks.push((selector, vec![format!("  {property}: {value}\n")])),
        }
    }
    let text: String = blocks
        .iter()
        .map(|(selector, props)| format!("{selector}:\n{}\n", props.concat()))
        .collect();

    let theme = resolve_str("coverage", &text).expect("the coverage theme resolves");
    // `Some(language)` so the BibLaTeX line exists and `citation.style` has
    // somewhere to land.
    let style = crate::latex::generate_style(&theme, &[], Some("en"));
    for (component, role, property, value, expected) in rows {
        let selector = match role {
            Some(r) => format!("{component}[role={r}]"),
            None => (*component).to_string(),
        };
        assert!(
            style.contains(expected),
            "{selector}.{property} = {value} produced no `{expected}` in the generated style; \
             the property validates and is then discarded"
        );
    }

    // The table may not lag the schema.
    let covered: std::collections::BTreeSet<(String, String)> = rows
        .iter()
        .map(|(c, _, p, _, _)| ((*c).to_string(), (*p).to_string()))
        .collect();
    let schema: std::collections::BTreeSet<(String, String)> = resolve::KNOWN_COMPONENTS
        .iter()
        .flat_map(|c| {
            resolve::known_properties(c)
                .iter()
                .map(move |p| ((*c).to_string(), (*p).to_string()))
        })
        .collect();
    let uncovered: Vec<&(String, String)> = schema.difference(&covered).collect();
    assert!(
        uncovered.is_empty(),
        "schema properties with no coverage row, which would validate and do nothing unnoticed: {uncovered:?}"
    );

    // The body stays theme-blind: the whole reason every setting is routed
    // through the style file instead of the document.
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
    assert_eq!(
        crate::latex::generate_document(&module, &theme),
        crate::latex::generate_document(&module, &academic()),
    );
}

/// `close-verification-gaps` scenario "Deferred component is not silently
/// accepted". The five components this change removes from the schema must
/// fail as unknown *components*, not be accepted and then reject their
/// properties one by one.
#[test]
fn test_deferred_components_are_unknown() {
    for (component, property, value) in [
        ("header", "content", "title"),
        ("footer", "content", "page"),
        ("contents", "depth", "2"),
        ("equation", "spacing", "1cm"),
        ("proof", "weight", "itshape"),
    ] {
        let err = resolve_str("academic", &format!("{component}:\n  {property}: {value}\n"))
            .expect_err("a deferred component is not part of the v1 schema");
        let ThemeError::Resolve(errors) = err else {
            panic!("{component} must fail during resolution, not parsing");
        };
        assert!(
            errors.iter().any(|e| matches!(
                e,
                ThemeResolveError::UnknownComponent { component: c, .. } if c == component
            )),
            "{component} must be reported as an unknown component, got {errors:?}"
        );
    }
}
