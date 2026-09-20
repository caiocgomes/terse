//! Syntax-level `.theme` parsing: a closed selector/property grammar,
//! deliberately far smaller than the `.trs` language. There are no
//! expressions, imports, per-ID selectors, combinators, content-selection
//! conditions, raw TeX, or executable hooks; a theme file is only a flat
//! list of `component[role=value]:` rules, each holding indented
//! `property: value` lines.

use crate::source::{SourceFile, SourceSpan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawProperty {
    pub name: String,
    pub value: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRule {
    pub component: String,
    pub role: Option<String>,
    pub properties: Vec<RawProperty>,
    pub selector_span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeParseError {
    /// A top-level line is not a valid `component` or `component[role=value]:`
    /// selector (covers per-ID selectors, combinators, `@` directives,
    /// braces, and any other unsupported selection syntax).
    InvalidSelector { span: SourceSpan },
    /// A selector (component + role) is declared more than once.
    DuplicateSelector { span: SourceSpan, selector: String },
    /// An indented line is not a valid `property: value` pair.
    InvalidProperty { span: SourceSpan },
    /// A property is set twice within the same rule.
    DuplicateProperty {
        span: SourceSpan,
        selector: String,
        property: String,
    },
    /// A property line appears before any selector has been declared.
    PropertyWithoutSelector { span: SourceSpan },
    /// Indentation used tabs or an unsupported width; only exactly two
    /// leading spaces are accepted for a property line.
    InvalidIndentation { span: SourceSpan },
}

/// Parses a `.theme` file's raw rule structure. This stage only enforces
/// the grammar shape (selector syntax, duplicate rejection, indentation);
/// it does not know which components/properties are semantically valid or
/// what type a value must have. See [`crate::theme::resolve`] for that.
pub fn parse(source: &SourceFile) -> Result<Vec<RawRule>, ThemeParseError> {
    let text = source.text();
    let file_id = source.id;
    let base = source.base_offset();

    let mut rules: Vec<RawRule> = Vec::new();
    let mut byte_offset: usize = 0;

    for line in text.split_inclusive('\n') {
        let line_start = byte_offset;
        byte_offset += line.len();
        let trimmed_end = line.trim_end_matches(['\n', '\r']);
        let content_len = trimmed_end.len();
        let span_of = |start: usize, end: usize| {
            SourceSpan::new(
                file_id,
                base + (line_start + start) as u32,
                base + (line_start + end) as u32,
            )
        };

        if trimmed_end.trim().is_empty() {
            continue;
        }
        if trimmed_end.trim_start().starts_with("//") {
            continue;
        }

        if let Some(rest) = trimmed_end.strip_prefix("  ") {
            // Property line: must belong to the most recently opened rule.
            if rest.starts_with(' ') || rest.starts_with('\t') {
                return Err(ThemeParseError::InvalidIndentation {
                    span: span_of(0, content_len),
                });
            }
            let Some(rule) = rules.last_mut() else {
                return Err(ThemeParseError::PropertyWithoutSelector {
                    span: span_of(0, content_len),
                });
            };
            let prop_start = 2;
            let Some((name, value)) = rest.split_once(':') else {
                return Err(ThemeParseError::InvalidProperty {
                    span: span_of(prop_start, content_len),
                });
            };
            let name = name.trim();
            let value = value.trim();
            if name.is_empty() || value.is_empty() || !is_property_name(name) {
                return Err(ThemeParseError::InvalidProperty {
                    span: span_of(prop_start, content_len),
                });
            }
            if rule.properties.iter().any(|p| p.name == name) {
                return Err(ThemeParseError::DuplicateProperty {
                    span: span_of(prop_start, content_len),
                    selector: selector_text(&rule.component, &rule.role),
                    property: name.to_string(),
                });
            }
            rule.properties.push(RawProperty {
                name: name.to_string(),
                value: value.to_string(),
                span: span_of(prop_start, content_len),
            });
            continue;
        }

        if trimmed_end.starts_with(' ') || trimmed_end.starts_with('\t') {
            return Err(ThemeParseError::InvalidIndentation {
                span: span_of(0, content_len),
            });
        }

        // Selector line: `component:` or `component[role=value]:`.
        let Some(head) = trimmed_end.strip_suffix(':') else {
            return Err(ThemeParseError::InvalidSelector {
                span: span_of(0, content_len),
            });
        };
        let (component, role) = match parse_selector_head(head) {
            Some(parts) => parts,
            None => {
                return Err(ThemeParseError::InvalidSelector {
                    span: span_of(0, content_len),
                })
            }
        };
        let selector = selector_text(&component, &role);
        if rules
            .iter()
            .any(|r| selector_text(&r.component, &r.role) == selector)
        {
            return Err(ThemeParseError::DuplicateSelector {
                span: span_of(0, content_len),
                selector,
            });
        }
        rules.push(RawRule {
            component,
            role,
            properties: Vec::new(),
            selector_span: span_of(0, content_len),
        });
    }

    Ok(rules)
}

fn selector_text(component: &str, role: &Option<String>) -> String {
    match role {
        Some(r) => format!("{component}[role={r}]"),
        None => component.to_string(),
    }
}

fn is_component_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().unwrap().is_ascii_lowercase()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
}

fn is_property_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().unwrap().is_ascii_lowercase()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_role_value(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Accepts exactly `component` or `component[role=value]`; anything else
/// (per-ID `#foo`, combinators `>`/`+`/`~`, multiple comma-separated
/// selectors, conditions, imports, `@` directives) is rejected here.
fn parse_selector_head(head: &str) -> Option<(String, Option<String>)> {
    if let Some(bracket_start) = head.find('[') {
        let component = &head[..bracket_start];
        let attr = head.strip_prefix(component)?.strip_prefix('[')?;
        let attr = attr.strip_suffix(']')?;
        let (key, value) = attr.split_once('=')?;
        if key != "role" || !is_component_name(component) || !is_role_value(value) {
            return None;
        }
        Some((component.to_string(), Some(value.to_string())))
    } else {
        is_component_name(head).then(|| (head.to_string(), None))
    }
}

/// Canonicalizes a `.theme` file's structural whitespace: newline
/// normalization and blank-run collapsing, identical in spirit to
/// [`crate::syntax::format::format_source`]. A theme file has no opaque
/// payload regions (no math/raw content), so every line is eligible for
/// canonicalization; selector/property text itself is left untouched.
pub fn format_theme(source: &SourceFile) -> Result<Vec<u8>, ThemeParseError> {
    parse(source)?;
    Ok(crate::syntax::format::canonicalize(
        source.original_bytes(),
        source.base_offset(),
        &[],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{FileId, SourceFile};

    fn parse_str(text: &str) -> Result<Vec<RawRule>, ThemeParseError> {
        let file = SourceFile::new(FileId(0), "t.theme", text.as_bytes().to_vec()).unwrap();
        parse(&file)
    }

    #[test]
    fn test_base_and_wide_selectors() {
        let rules = parse_str(
            "figure:\n  align: center\n  default-width: 75%\n\nfigure[role=wide]:\n  default-width: 100%\n",
        )
        .unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].component, "figure");
        assert_eq!(rules[0].role, None);
        assert_eq!(rules[1].component, "figure");
        assert_eq!(rules[1].role.as_deref(), Some("wide"));
    }

    #[test]
    fn test_themes_cannot_select_content() {
        for bad in [
            "#intro:\n  color: red\n",
            "figure > table:\n  align: center\n",
            "@if dark:\n  color: black\n",
            ".figure:\n  align: center\n",
            "figure, table:\n  align: center\n",
        ] {
            assert!(
                parse_str(bad).is_err(),
                "expected rejection for selector text: {bad}"
            );
        }
    }

    #[test]
    fn test_theme_selector_order_irrelevant_parse() {
        let a = parse_str("page:\n  size: a4\n\nbody:\n  size: 11pt\n").unwrap();
        let b = parse_str("body:\n  size: 11pt\n\npage:\n  size: a4\n").unwrap();
        let mut a_sorted: Vec<_> = a.iter().map(|r| r.component.clone()).collect();
        let mut b_sorted: Vec<_> = b.iter().map(|r| r.component.clone()).collect();
        a_sorted.sort();
        b_sorted.sort();
        assert_eq!(a_sorted, b_sorted);
    }

    #[test]
    fn test_duplicate_selector_rejected() {
        assert!(parse_str("figure:\n  align: center\n\nfigure:\n  align: left\n").is_err());
    }

    #[test]
    fn test_duplicate_property_rejected() {
        assert!(parse_str("figure:\n  align: center\n  align: left\n").is_err());
    }
}
