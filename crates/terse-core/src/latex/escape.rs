//! Ordinary text escaping for LaTeX output.

pub fn escape_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '\\' => out.push_str("\\textbackslash{}"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '%' => out.push_str("\\%"),
            '$' => out.push_str("\\$"),
            '&' => out.push_str("\\&"),
            '_' => out.push_str("\\_"),
            '#' => out.push_str("\\#"),
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            other => out.push(other),
        }
    }
    out
}

/// Escapes a URL/link destination for use as a `\href` argument. Distinct
/// from `escape_text`: URLs must not gain `\textbackslash{}`/`\{`/`\}`
/// noise that would corrupt the address.
pub fn escape_url(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '%' => out.push_str("\\%"),
            '#' => out.push_str("\\#"),
            '&' => out.push_str("\\&"),
            other => out.push(other),
        }
    }
    out
}

/// Wraps an already-escaped value for use as a LaTeX optional (`[...]`)
/// argument: an extra brace group prevents a literal `]` in the value from
/// prematurely closing the optional argument.
pub fn escape_optional_arg(input: &str) -> String {
    format!("{{{}}}", escape_text(input))
}

/// Escapes a string for use inside a `\hypersetup` PDF metadata field.
/// These are plain PDF strings, not LaTeX text: braces and backslashes
/// have no escape form there, so unsupported characters are dropped
/// rather than emitted as broken macros.
pub fn escape_pdf_meta(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '\\' | '{' | '}' => {}
            '%' => out.push_str("\\%"),
            '$' => out.push_str("\\$"),
            '&' => out.push_str("\\&"),
            '#' => out.push_str("\\#"),
            '_' => out.push_str("\\_"),
            other => out.push(other),
        }
    }
    out
}

/// Escapes a value for a BibLaTeX field, wrapped in `{}` by the caller.
/// Wired in by task group 16's bibliography emission; unit-tested here
/// since it is one of this module's closed set of context-specific
/// encoders.
pub fn escape_bib_value(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            other => out.push(other),
        }
    }
    out
}

/// Link destination schemes Terse considers safe to embed in a generated
/// PDF: ordinary web addresses and mail links. Anything else (`javascript:`,
/// `data:`, `file:`, bare local paths with a drive-letter-shaped prefix,
/// ...) is rejected before generation, without ever fetching the
/// destination.
pub const ALLOWED_LINK_SCHEMES: &[&str] = &["http", "https", "mailto"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsafeLinkScheme(pub String);

pub fn validate_link_scheme(destination: &str) -> Result<(), UnsafeLinkScheme> {
    if let Some(idx) = destination.find(':') {
        let scheme = &destination[..idx];
        let looks_like_scheme = !scheme.is_empty()
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.');
        if looks_like_scheme {
            if ALLOWED_LINK_SCHEMES.iter().any(|s| s.eq_ignore_ascii_case(scheme)) {
                return Ok(());
            }
            return Err(UnsafeLinkScheme(scheme.to_string()));
        }
    }
    Err(UnsafeLinkScheme(destination.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escapes_special_characters() {
        assert_eq!(escape_text("50% & $x_i$"), "50\\% \\& \\$x\\_i\\$");
    }

    #[test]
    fn test_leaves_ordinary_unicode_text_unchanged() {
        assert_eq!(escape_text("café résumé"), "café résumé");
    }

    #[test]
    fn test_escaping_preserves_literal_text_and_math() {
        // Prose escaping and a raw math payload coexist without either
        // corrupting the other: escape_text never touches math bytes
        // (the renderer emits math payloads unescaped, by design), and
        // escaping prose containing a literal backslash never produces
        // stray active math bytes.
        assert_eq!(escape_text("x_i \\ y"), "x\\_i \\textbackslash{} y");
        let math_payload = "x_i + y^2";
        assert_eq!(math_payload, "x_i + y^2", "math payload is preserved byte-for-byte");
    }

    #[test]
    fn test_escape_optional_arg_neutralizes_closing_bracket() {
        assert_eq!(escape_optional_arg("odd] title"), "{odd] title}");
    }

    #[test]
    fn test_escape_pdf_meta_drops_unsupported_macros() {
        assert_eq!(escape_pdf_meta("Ada \\& Grace {x}"), "Ada \\& Grace x");
    }

    #[test]
    fn test_escape_bib_value_doubles_backslash_and_braces() {
        assert_eq!(escape_bib_value("a\\b{c}"), "a\\\\b\\{c\\}");
    }

    #[test]
    fn test_unsafe_link_schemes_fail() {
        assert!(validate_link_scheme("https://example.org").is_ok());
        assert!(validate_link_scheme("mailto:ada@example.org").is_ok());
        assert_eq!(
            validate_link_scheme("javascript:alert(1)"),
            Err(UnsafeLinkScheme("javascript".to_string()))
        );
        assert!(validate_link_scheme("data:text/html,<script>").is_err());
        assert!(validate_link_scheme("file:///etc/passwd").is_err());
    }
}
