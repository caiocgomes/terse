//! Typed theme resolution: turns [`RawRule`]s into a [`ResolvedTheme`],
//! enforcing closed per-component property sets, typed values, base/role
//! precedence, and the geometry/visibility bounds that keep a theme from
//! deliberately hiding authored content.

use super::parse::RawRule;
use super::ResolvedTheme;
use crate::source::SourceSpan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeResolveError {
    UnknownComponent { span: SourceSpan, component: String },
    UnsupportedRole { span: SourceSpan, component: String },
    UnknownProperty {
        span: SourceSpan,
        component: String,
        property: String,
    },
    InvalidValue {
        span: SourceSpan,
        component: String,
        property: String,
        expected: &'static str,
    },
    OutOfBounds {
        span: SourceSpan,
        component: String,
        property: String,
        reason: &'static str,
    },
    ExternalResource { span: SourceSpan, property: String },
}

/// Components with a defined typed property schema.
///
/// Every entry here must have a non-empty [`known_properties`] set: a
/// component that is recognized but accepts no property would take a
/// theme's rule, reject each of its properties individually, and read as
/// "you spelled the property wrong" when the truth is "this component does
/// nothing yet". `header`, `footer`, `contents`, `equation` and `proof`
/// were exactly that until `close-verification-gaps` removed them, so they
/// now fail honestly as unknown components until a later change implements
/// them.
pub(crate) const KNOWN_COMPONENTS: &[&str] = &[
    "page",
    "body",
    "heading.1",
    "heading.2",
    "heading.3",
    "title",
    "theorem",
    "figure",
    "table",
    "citation",
    "bibliography",
    "logo",
    "watermark",
];

pub(crate) fn known_properties(component: &str) -> &'static [&'static str] {
    match component {
        "page" => &["size", "margin", "columns"],
        "body" => &["font", "color"],
        "heading.1" | "heading.2" | "heading.3" => &["weight", "numbering"],
        "title" => &["layout", "align"],
        "theorem" => &["style"],
        "figure" => &["align", "default-width", "placement"],
        "table" => &["padding", "rules", "header"],
        "watermark" => &["kind", "opacity", "angle"],
        "logo" => &["source", "width"],
        "citation" => &["style"],
        "bibliography" => &["size", "item-spacing"],
        _ => &[],
    }
}

/// The theorem-like kinds a `theorem[kind=...]` selector may name, in the
/// semantic model's own order so the index into
/// [`ResolvedTheme::theorem_style`] matches `TheoremKind`.
pub(crate) const THEOREM_KINDS: [&str; 6] = [
    "theorem",
    "proposition",
    "lemma",
    "definition",
    "example",
    "remark",
];

/// Resolves parsed rules against the versioned compiler defaults for
/// `base`, applying base-component settings before their `role`-specific
/// override, independent of the rules' declaration order.
pub fn resolve(
    name: &str,
    base: ResolvedTheme,
    rules: &[RawRule],
) -> Result<ResolvedTheme, Vec<ThemeResolveError>> {
    let mut theme = base;
    theme.name = name.to_string();
    let mut errors = Vec::new();
    // Where each accepted property was written, so a bounds violation can
    // be reported at the rule that caused it rather than at a placeholder
    // position: the requirement is that checking "fails at the offending
    // property", and a resolved theme alone no longer knows where its
    // values came from.
    let mut spans: PropertySpans = PropertySpans::new();

    // Apply base rules first, then role rules, so a role override always
    // wins regardless of source order (there is at most one of each kind
    // per component: duplicates were already rejected while parsing).
    for rule in rules.iter().filter(|r| r.role.is_none()) {
        apply_rule(&mut theme, rule, &mut spans, &mut errors);
    }
    for rule in rules.iter().filter(|r| r.role.is_some()) {
        apply_rule(&mut theme, rule, &mut spans, &mut errors);
    }

    if !errors.is_empty() {
        return Err(errors);
    }
    validate_bounds(&theme, &spans, &mut errors);
    if errors.is_empty() {
        Ok(theme)
    } else {
        Err(errors)
    }
}

/// Where each accepted `(component, role, property)` was declared. Keyed by
/// role as well as component because `figure` and `figure[role=wide]` set
/// the same property name to different values, and a bounds violation must
/// point at whichever of the two actually caused it.
type PropertySpans = std::collections::BTreeMap<(String, Option<String>, String), SourceSpan>;

fn apply_rule(
    theme: &mut ResolvedTheme,
    rule: &RawRule,
    spans: &mut PropertySpans,
    errors: &mut Vec<ThemeResolveError>,
) {
    if !KNOWN_COMPONENTS.contains(&rule.component.as_str()) {
        errors.push(ThemeResolveError::UnknownComponent {
            span: rule.selector_span,
            component: rule.component.clone(),
        });
        return;
    }
    if let Some(role) = &rule.role {
        // The attribute key is part of the selector, not decoration:
        // `figure` is varied by semantic role, `theorem` by kind, and using
        // the other component's key is as wrong as inventing a value.
        let supported = match (rule.component.as_str(), rule.role_key) {
            ("figure", Some("role")) => role == "wide",
            ("theorem", Some("kind")) => THEOREM_KINDS.contains(&role.as_str()),
            _ => false,
        };
        if !supported {
            errors.push(ThemeResolveError::UnsupportedRole {
                span: rule.selector_span,
                component: rule.component.clone(),
            });
            return;
        }
    }
    let allowed = known_properties(&rule.component);
    for prop in &rule.properties {
        if !allowed.contains(&prop.name.as_str()) {
            errors.push(ThemeResolveError::UnknownProperty {
                span: prop.span,
                component: rule.component.clone(),
                property: prop.name.clone(),
            });
            continue;
        }
        match apply_property(theme, &rule.component, &rule.role, prop) {
            Ok(()) => {
                spans.insert(
                    (rule.component.clone(), rule.role.clone(), prop.name.clone()),
                    prop.span,
                );
            }
            Err(e) => errors.push(e),
        }
    }
}

fn apply_property(
    theme: &mut ResolvedTheme,
    component: &str,
    role: &Option<String>,
    prop: &super::parse::RawProperty,
) -> Result<(), ThemeResolveError> {
    let invalid = |expected: &'static str| ThemeResolveError::InvalidValue {
        span: prop.span,
        component: component.to_string(),
        property: prop.name.clone(),
        expected,
    };
    match (component, role.as_deref(), prop.name.as_str()) {
        ("page", None, "size") => {
            if prop.value != "a4" && prop.value != "letter" {
                return Err(invalid("'a4' or 'letter'"));
            }
            theme.page_size = prop.value.clone();
        }
        ("page", None, "margin") => {
            theme.page_margin_cm = parse_dimension_cm(&prop.value).ok_or_else(|| invalid("a dimension like '2.5cm'"))?;
        }
        ("page", None, "columns") => {
            theme.page_columns = match prop.value.as_str() {
                "1" => 1,
                "2" => 2,
                _ => return Err(invalid("'1' or '2'")),
            };
        }
        ("body", None, "font") => {
            theme.body_font = parse_font_token(&prop.value).ok_or_else(|| invalid("a supported font token"))?;
        }
        ("body", None, "color") => {
            theme.body_color = parse_color(&prop.value).ok_or_else(|| invalid("a hex color or 'black'"))?;
        }
        ("heading.1" | "heading.2" | "heading.3", None, "weight") => {
            if !["bold", "regular", "italic", "small-caps"].contains(&prop.value.as_str()) {
                return Err(invalid("'bold', 'regular', 'italic', or 'small-caps'"));
            }
            let idx = heading_index(component);
            theme.heading_weight[idx] = prop.value.clone();
        }
        ("heading.1" | "heading.2" | "heading.3", None, "numbering") => {
            if !["decimal", "roman", "none"].contains(&prop.value.as_str()) {
                return Err(invalid("'decimal', 'roman', or 'none'"));
            }
            let idx = heading_index(component);
            theme.heading_numbering[idx] = prop.value.clone();
        }
        ("figure", None, "align") => {
            if !["left", "center", "right"].contains(&prop.value.as_str()) {
                return Err(invalid("'left', 'center', or 'right'"));
            }
            theme.figure_align = prop.value.clone();
        }
        ("figure", None, "default-width") => {
            theme.figure_default_width_pct =
                parse_percentage(&prop.value).ok_or_else(|| invalid("a percentage like '75%'"))?;
        }
        ("figure", Some("wide"), "default-width") => {
            theme.figure_wide_width_pct =
                parse_percentage(&prop.value).ok_or_else(|| invalid("a percentage like '100%'"))?;
        }
        ("figure", None, "placement") => {
            if !["here", "top", "bottom"].contains(&prop.value.as_str()) {
                return Err(invalid("'here', 'top', or 'bottom'"));
            }
            theme.figure_placement = prop.value.clone();
        }
        ("citation", None, "style") => {
            if !["author-year", "numeric"].contains(&prop.value.as_str()) {
                return Err(invalid("'author-year' or 'numeric'"));
            }
            theme.citation_style = prop.value.clone();
        }
        ("title", None, "layout") => {
            if !["paper", "cover"].contains(&prop.value.as_str()) {
                return Err(invalid("'paper' or 'cover'"));
            }
            theme.title_layout = prop.value.clone();
        }
        ("title", None, "align") => {
            if !["left", "center"].contains(&prop.value.as_str()) {
                return Err(invalid("'left' or 'center'"));
            }
            theme.title_align = prop.value.clone();
        }
        // A base `theorem` rule sets every theorem-like kind; a
        // `theorem[kind=...]` rule overrides exactly one. Base rules are
        // applied before role rules by `resolve`, so the override wins
        // regardless of declaration order.
        ("theorem", role, "style") => {
            if !["plain", "definition", "remark"].contains(&prop.value.as_str()) {
                return Err(invalid("'plain', 'definition', or 'remark'"));
            }
            match role {
                None => theme.theorem_style = std::array::from_fn(|_| prop.value.clone()),
                Some(kind) => {
                    let index = THEOREM_KINDS
                        .iter()
                        .position(|k| *k == kind)
                        .expect("the role was validated against THEOREM_KINDS");
                    theme.theorem_style[index] = prop.value.clone();
                }
            }
        }
        ("table", None, "padding") => {
            theme.table_padding = parse_finite(&prop.value)
                .ok_or_else(|| invalid("a row-height multiplier like '1.2'"))?;
        }
        ("table", None, "rules") => {
            if !["booktabs", "plain"].contains(&prop.value.as_str()) {
                return Err(invalid("'booktabs' or 'plain'"));
            }
            theme.table_rules = prop.value.clone();
        }
        ("table", None, "header") => {
            if !["bold", "plain"].contains(&prop.value.as_str()) {
                return Err(invalid("'bold' or 'plain'"));
            }
            theme.table_header = prop.value.clone();
        }
        ("bibliography", None, "size") => {
            if !["normal", "small", "footnotesize"].contains(&prop.value.as_str()) {
                return Err(invalid("'normal', 'small', or 'footnotesize'"));
            }
            theme.bibliography_size = prop.value.clone();
        }
        ("bibliography", None, "item-spacing") => {
            theme.bibliography_item_spacing_em =
                parse_dimension_em(&prop.value).ok_or_else(|| invalid("a dimension like '0.5em'"))?;
        }
        ("watermark", None, "kind") => {
            if !["none", "internal-use", "draft"].contains(&prop.value.as_str()) {
                return Err(invalid("'none', 'internal-use', or 'draft'"));
            }
            theme.watermark_kind = prop.value.clone();
        }
        ("watermark", None, "opacity") => {
            theme.watermark_opacity =
                parse_finite(&prop.value).ok_or_else(|| invalid("a number between 0 and 1"))?;
        }
        ("watermark", None, "angle") => {
            theme.watermark_angle =
                parse_finite(&prop.value).ok_or_else(|| invalid("an angle in degrees"))?;
        }
        ("logo", None, "source") => {
            if is_external_resource(&prop.value) {
                return Err(ThemeResolveError::ExternalResource {
                    span: prop.span,
                    property: prop.name.clone(),
                });
            }
            // The path is the only theme string emitted verbatim, as a
            // macro argument: a `}` closes that argument early and
            // everything after it executes. Its charset is part of its
            // type, not a matter of taste.
            if !is_tex_safe_path(&prop.value) {
                return Err(invalid("a relative path without TeX-special characters"));
            }
            theme.logo_path = Some(prop.value.clone());
        }
        ("logo", None, "width") => {
            theme.logo_width_pct =
                parse_percentage(&prop.value).ok_or_else(|| invalid("a percentage"))?;
        }
        _ => {
            return Err(ThemeResolveError::UnknownProperty {
                span: prop.span,
                component: component.to_string(),
                property: prop.name.clone(),
            })
        }
    }
    Ok(())
}

fn heading_index(component: &str) -> usize {
    match component {
        "heading.1" => 0,
        "heading.2" => 1,
        _ => 2,
    }
}

fn parse_percentage(v: &str) -> Option<u8> {
    let n = v.strip_suffix('%')?;
    n.parse::<u8>().ok()
}

/// `f64::from_str` accepts `NaN` and `inf`, and every bounds check here is
/// two-sided (`x <= lo || x > hi`), which is false on both sides for NaN.
/// A non-finite value would therefore pass validation and be formatted
/// into the style as a word — `margin=NaNcm`. Finiteness is part of what
/// makes these values numbers.
fn parse_finite(v: &str) -> Option<f64> {
    let n = v.parse::<f64>().ok()?;
    n.is_finite().then_some(n)
}

fn parse_dimension_cm(v: &str) -> Option<f64> {
    parse_finite(v.strip_suffix("cm")?)
}

fn parse_dimension_em(v: &str) -> Option<f64> {
    parse_finite(v.strip_suffix("em")?)
}

/// A logo path reaches LaTeX as a macro argument. Everything TeX treats as
/// syntax (group and escape characters, comment, math shift, alignment,
/// parameter, superscript, subscript, tilde) is rejected, as are control
/// characters, so the argument cannot be closed or the preamble reopened.
fn is_tex_safe_path(v: &str) -> bool {
    !v.is_empty()
        && !v
            .chars()
            .any(|c| c.is_control() || "{}\\%$&#^_~".contains(c))
}

/// Selects a TeX-distributed font by filename token; no host-family
/// lookup, matching the "Portable font and asset selection" requirement.
fn parse_font_token(v: &str) -> Option<String> {
    match v {
        "libertinus-otf" | "latin-modern" | "tex-gyre-pagella" | "tex-gyre-heros" => {
            Some(v.to_string())
        }
        _ => None,
    }
}

fn parse_color(v: &str) -> Option<String> {
    if v == "black" {
        return Some("000000".to_string());
    }
    let hex = v.strip_prefix('#')?;
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(hex.to_lowercase())
    } else {
        None
    }
}

/// Any absolute path, parent-directory escape, or URL scheme is an
/// external resource: logos must be explicit root-contained local assets.
fn is_external_resource(v: &str) -> bool {
    v.contains("://") || v.starts_with('/') || v.starts_with('\\') || v.split('/').any(|seg| seg == "..")
}

fn validate_bounds(theme: &ResolvedTheme, spans: &PropertySpans, errors: &mut Vec<ThemeResolveError>) {
    let zero_span = SourceSpan::new(crate::source::FileId(0), 0, 0);
    // A bound is violated by a value, and every value that a theme can put
    // out of range was written somewhere; fall back to the placeholder only
    // for a compiler default, which shipped defaults never violate.
    let at = |component: &str, role: Option<&str>, property: &str| {
        spans
            .get(&(
                component.to_string(),
                role.map(str::to_string),
                property.to_string(),
            ))
            .copied()
            .unwrap_or(zero_span)
    };
    if theme.page_margin_cm <= 0.0 || theme.page_margin_cm * 2.0 >= 21.0 {
        errors.push(ThemeResolveError::OutOfBounds {
            span: at("page", None, "margin"),
            component: "page".to_string(),
            property: "margin".to_string(),
            reason: "margins must leave a usable content box on the page",
        });
    }
    if theme.figure_default_width_pct == 0 || theme.figure_default_width_pct > 100 {
        errors.push(ThemeResolveError::OutOfBounds {
            span: at("figure", None, "default-width"),
            component: "figure".to_string(),
            property: "default-width".to_string(),
            reason: "width must be within the container (1-100%)",
        });
    }
    if theme.figure_wide_width_pct == 0 || theme.figure_wide_width_pct > 100 {
        errors.push(ThemeResolveError::OutOfBounds {
            span: at("figure", Some("wide"), "default-width"),
            component: "figure".to_string(),
            property: "default-width".to_string(),
            reason: "width must be within the container (1-100%)",
        });
    }
    if !(0.0..=0.3).contains(&theme.watermark_opacity) {
        errors.push(ThemeResolveError::OutOfBounds {
            span: at("watermark", None, "opacity"),
            component: "watermark".to_string(),
            property: "opacity".to_string(),
            reason: "watermark opacity must stay bounded so authored content remains readable",
        });
    }
    // Row height and bibliography spacing are the two new numeric knobs:
    // a nonpositive stretch collapses rows onto each other, and a negative
    // item spacing pulls entries into the one above.
    if theme.table_padding <= 0.0 || theme.table_padding > 5.0 {
        errors.push(ThemeResolveError::OutOfBounds {
            span: at("table", None, "padding"),
            component: "table".to_string(),
            property: "padding".to_string(),
            reason: "row padding must keep table rows legible and on the page (0-5)",
        });
    }
    if theme.bibliography_item_spacing_em < 0.0 || theme.bibliography_item_spacing_em > 5.0 {
        errors.push(ThemeResolveError::OutOfBounds {
            span: at("bibliography", None, "item-spacing"),
            component: "bibliography".to_string(),
            property: "item-spacing".to_string(),
            reason: "bibliography item spacing must be nonnegative and bounded (0-5em)",
        });
    }
    if theme.body_color == "ffffff" {
        errors.push(ThemeResolveError::OutOfBounds {
            span: at("body", None, "color"),
            component: "body".to_string(),
            property: "color".to_string(),
            reason: "body text color must not match the page background",
        });
    }
}
