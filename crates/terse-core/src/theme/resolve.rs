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

/// Components with a defined typed property schema. Only `figure` accepts
/// the `role=wide` selector in v1.
const KNOWN_COMPONENTS: &[&str] = &[
    "page",
    "body",
    "heading.1",
    "heading.2",
    "heading.3",
    "title",
    "theorem",
    "proof",
    "equation",
    "figure",
    "table",
    "citation",
    "bibliography",
    "header",
    "footer",
    "logo",
    "watermark",
    "contents",
];

fn known_properties(component: &str) -> &'static [&'static str] {
    match component {
        "page" => &["size", "margin", "columns"],
        "body" => &["font", "color"],
        "heading.1" | "heading.2" | "heading.3" => &["weight", "numbering"],
        "figure" => &["align", "default-width", "placement"],
        "watermark" => &["kind", "opacity", "angle"],
        "logo" => &["source", "width"],
        "citation" => &["style"],
        _ => &[],
    }
}

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

    // Apply base rules first, then role rules, so a role override always
    // wins regardless of source order (there is at most one of each kind
    // per component: duplicates were already rejected while parsing).
    for rule in rules.iter().filter(|r| r.role.is_none()) {
        apply_rule(&mut theme, rule, &mut errors);
    }
    for rule in rules.iter().filter(|r| r.role.is_some()) {
        apply_rule(&mut theme, rule, &mut errors);
    }

    if !errors.is_empty() {
        return Err(errors);
    }
    validate_bounds(&theme, &mut errors);
    if errors.is_empty() {
        Ok(theme)
    } else {
        Err(errors)
    }
}

fn apply_rule(theme: &mut ResolvedTheme, rule: &RawRule, errors: &mut Vec<ThemeResolveError>) {
    if !KNOWN_COMPONENTS.contains(&rule.component.as_str()) {
        errors.push(ThemeResolveError::UnknownComponent {
            span: rule.selector_span,
            component: rule.component.clone(),
        });
        return;
    }
    if let Some(role) = &rule.role {
        if rule.component != "figure" || role != "wide" {
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
        if let Err(e) = apply_property(theme, &rule.component, &rule.role, prop) {
            errors.push(e);
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
        ("watermark", None, "kind") => {
            if !["none", "internal-use", "draft"].contains(&prop.value.as_str()) {
                return Err(invalid("'none', 'internal-use', or 'draft'"));
            }
            theme.watermark_kind = prop.value.clone();
        }
        ("watermark", None, "opacity") => {
            theme.watermark_opacity =
                prop.value.parse::<f64>().ok().ok_or_else(|| invalid("a number between 0 and 1"))?;
        }
        ("watermark", None, "angle") => {
            theme.watermark_angle =
                prop.value.parse::<f64>().ok().ok_or_else(|| invalid("an angle in degrees"))?;
        }
        ("logo", None, "source") => {
            if is_external_resource(&prop.value) {
                return Err(ThemeResolveError::ExternalResource {
                    span: prop.span,
                    property: prop.name.clone(),
                });
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

fn parse_dimension_cm(v: &str) -> Option<f64> {
    let n = v.strip_suffix("cm")?;
    n.parse::<f64>().ok()
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

fn validate_bounds(theme: &ResolvedTheme, errors: &mut Vec<ThemeResolveError>) {
    let zero_span = SourceSpan::new(crate::source::FileId(0), 0, 0);
    if theme.page_margin_cm <= 0.0 || theme.page_margin_cm * 2.0 >= 21.0 {
        errors.push(ThemeResolveError::OutOfBounds {
            span: zero_span,
            component: "page".to_string(),
            property: "margin".to_string(),
            reason: "margins must leave a usable content box on the page",
        });
    }
    if theme.figure_default_width_pct == 0 || theme.figure_default_width_pct > 100 {
        errors.push(ThemeResolveError::OutOfBounds {
            span: zero_span,
            component: "figure".to_string(),
            property: "default-width".to_string(),
            reason: "width must be within the container (1-100%)",
        });
    }
    if theme.figure_wide_width_pct == 0 || theme.figure_wide_width_pct > 100 {
        errors.push(ThemeResolveError::OutOfBounds {
            span: zero_span,
            component: "figure".to_string(),
            property: "default-width".to_string(),
            reason: "width must be within the container (1-100%)",
        });
    }
    if !(0.0..=0.3).contains(&theme.watermark_opacity) {
        errors.push(ThemeResolveError::OutOfBounds {
            span: zero_span,
            component: "watermark".to_string(),
            property: "opacity".to_string(),
            reason: "watermark opacity must stay bounded so authored content remains readable",
        });
    }
    if theme.body_color == "ffffff" {
        errors.push(ThemeResolveError::OutOfBounds {
            span: zero_span,
            component: "body".to_string(),
            property: "color".to_string(),
            reason: "body text color must not match the page background",
        });
    }
}
