//! Recognizer for opaque payload regions, shared between the lexer (which
//! decides when to stop validating structural indentation) and the block
//! parser (which decides when to open `math:`/`tex:`/`$$`/fenced-code
//! blocks), so the two can never disagree about where such a region
//! starts or ends.
//!
//! Three kinds of opener exist:
//! - `Indented`: `math:`, `math [...]:`, or `tex:`. Its body sits one
//!   structural level deeper than the header and never auto-closes; the
//!   region only ends at a shorter prefix or EOF.
//! - `Dollar`: a `$$` display-math opener with no closing `$$` later on
//!   the same line. Its body sits at the SAME level as the opener and
//!   closes at the first line whose content, after the region's fixed
//!   prefix, contains `$$` anywhere.
//! - `Fence { len }`: a fenced code block opener (a run of at least three
//!   backticks with no backtick in its info string). Its body sits at the
//!   same level as the opener and closes at the first line whose content,
//!   after the region's fixed prefix, is (trimmed at the end) a run of at
//!   least `len` backticks and nothing else.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opener {
    Indented,
    Dollar,
    Fence { len: usize },
}

/// Recognizes a structural line's content (already stripped of its OWN
/// structural indentation, exactly what [`crate::syntax::lexer::StructLine::content`]
/// holds for a non-opaque line) as an opaque-region opener, or `None` if
/// it stays an ordinary structural line. A malformed header (e.g. `math:
/// nope`, which doesn't match either recognized shape) is deliberately
/// NOT an opener: it stays fully structural, so the block parser still
/// reports its own "malformed header" diagnostic exactly as it does
/// today, and any following mis-indented line fails at the lexer as
/// before (not silently swallowed as tolerated payload).
pub fn opener(content: &str) -> Option<Opener> {
    if is_indented_opener(content) {
        return Some(Opener::Indented);
    }
    if let Some(len) = fence_len(content) {
        return Some(Opener::Fence { len });
    }
    if is_dollar_opener(content) {
        return Some(Opener::Dollar);
    }
    None
}

/// The same shapes [`crate::syntax::blocks`]'s `parse_equation` and
/// `parse_raw_tex` accept as a header: exactly `math:`, `math [...]:`, or
/// exactly `tex:`. Attribute *content* is not validated here (that stays
/// `parse_equation`'s job); only the header's outer shape matters for
/// deciding whether its body is opaque.
fn is_indented_opener(content: &str) -> bool {
    let trimmed = content.trim_end();
    if trimmed == "math:" || trimmed == "tex:" {
        return true;
    }
    content.starts_with("math [") && trimmed.ends_with(':')
}

/// A `$$` opener with no closing `$$` on the same line: a genuine
/// multi-line display, not the single-line `$$ x $$` case the block
/// parser's own `parse_dollar_display` resolves entirely from the header
/// line without any lexer region.
fn is_dollar_opener(content: &str) -> bool {
    content.starts_with("$$") && !content[2..].contains("$$")
}

/// Whether `content` (a payload line's bytes after a `Dollar` region's
/// fixed prefix has been stripped) closes that region: it contains `$$`
/// anywhere. Mirrors `parse_dollar_display`'s own closer search, so the
/// lexer and the block parser agree on which line ends the region.
pub fn dollar_closes(content: &str) -> bool {
    content.contains("$$")
}

/// A fence opener's backtick-run length, or `None` if `content` is not
/// one: it must start with a run of at least three backticks whose
/// remainder (the info string) contains no backtick.
pub fn fence_len(content: &str) -> Option<usize> {
    let len = content.chars().take_while(|&c| c == '`').count();
    if len < 3 {
        return None;
    }
    if content[len..].contains('`') {
        return None;
    }
    Some(len)
}

/// Whether `content` (a payload line's bytes after a `Fence { len }`
/// region's fixed prefix has been stripped) closes that region: trimmed
/// at the end, it is a run of at least `len` backticks and nothing else.
pub fn fence_closes(content: &str, len: usize) -> bool {
    let trimmed = content.trim_end();
    !trimmed.is_empty() && trimmed.chars().count() >= len && trimmed.chars().all(|c| c == '`')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_indented_opener_shapes() {
        assert_eq!(opener("math:"), Some(Opener::Indented));
        assert_eq!(opener("math [id: eq1]:"), Some(Opener::Indented));
        assert_eq!(opener("tex:"), Some(Opener::Indented));
        assert_eq!(opener("math: nope"), None);
        assert_eq!(opener("mathematics:"), None);
        assert_eq!(opener("\\math:"), None);
    }

    #[test]
    fn test_dollar_opener_excludes_single_line_close() {
        assert_eq!(opener("$$"), Some(Opener::Dollar));
        assert_eq!(opener("$$ x"), Some(Opener::Dollar));
        assert_eq!(opener("$$ x $$"), None);
        assert_eq!(opener("$$$$"), None);
    }

    #[test]
    fn test_fence_opener_requires_backtick_free_info_string() {
        assert_eq!(opener("```"), Some(Opener::Fence { len: 3 }));
        assert_eq!(opener("```python"), Some(Opener::Fence { len: 3 }));
        assert_eq!(opener("````"), Some(Opener::Fence { len: 4 }));
        assert_eq!(opener("``x``"), None);
        assert_eq!(opener("``"), None);
    }

    #[test]
    fn test_dollar_and_fence_closers() {
        assert!(dollar_closes("x $$"));
        assert!(!dollar_closes("x"));
        assert!(fence_closes("```", 3));
        assert!(fence_closes("```   ", 3));
        assert!(!fence_closes("``` x", 3));
        assert!(!fence_closes("``", 3));
        assert!(fence_closes("````", 3));
    }
}
