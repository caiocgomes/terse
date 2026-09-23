//! Lossless indentation lexing.
//!
//! Splits source text into logical lines and validates structural
//! indentation (exactly two spaces per level, no tabs, no dedent to a level
//! that was never opened). This pass is content-agnostic for ORDINARY
//! lines: it knows nothing about reserved words or block grammar, only
//! indentation structure. It IS aware of one thing beyond that: opaque
//! payload regions (`math:`/`tex:` bodies, multi-line `$$` displays, and
//! fenced code blocks), recognized via [`crate::syntax::opaque`], the
//! same recognizer the block parser uses -- so a payload line may contain
//! tabs or any amount of indentation beyond its region's own fixed
//! prefix, tolerated here rather than rejected, while every other line
//! keeps the strict two-space rule.

use crate::diagnostic::Diagnostic;
use crate::source::{FileId, SourceSpan};
use crate::syntax::opaque::{self, Opener};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndentError {
    /// A tab was found where structural indentation was expected.
    Tab { byte_offset: u32 },
    /// Leading spaces were not a multiple of two.
    NotMultipleOfTwo { byte_offset: u32 },
    /// A dedent landed on an indentation level that was never opened.
    UnopenedDedent { byte_offset: u32 },
}

pub fn indent_error_to_diagnostic(err: IndentError, file_id: FileId, base: u32) -> Diagnostic {
    let (code, message, offset) = match err {
        IndentError::Tab { byte_offset } => (
            "E-PARSE-010",
            "structural indentation must use spaces, not tabs".to_string(),
            byte_offset,
        ),
        IndentError::NotMultipleOfTwo { byte_offset } => (
            "E-PARSE-011",
            "structural indentation must use exactly two spaces per level".to_string(),
            byte_offset,
        ),
        IndentError::UnopenedDedent { byte_offset } => (
            "E-PARSE-012",
            "dedent does not match a previously opened indentation level".to_string(),
            byte_offset,
        ),
    };
    Diagnostic::error(
        code,
        message,
        SourceSpan::new(file_id, base + offset, base + offset),
    )
}

/// A logical line after indentation lexing.
#[derive(Debug, Clone)]
pub struct StructLine<'a> {
    /// Byte offset (within the decoded text) of the start of the line.
    pub byte_start: u32,
    /// Byte offset (within the decoded text) of the end of the line,
    /// excluding the line-ending bytes.
    pub byte_end: u32,
    /// Indentation level in units of two spaces.
    pub indent: u32,
    /// Line content after structural indentation.
    pub content: &'a str,
    /// Byte offset of `content`'s first byte.
    pub content_byte_start: u32,
    pub is_blank: bool,
    /// True for a line inside an opaque payload region (a `math:`/`tex:`
    /// body, a multi-line `$$` display, or a fenced code block): recovery
    /// parsing (`parse_module_with_recovery`) skips these when searching
    /// for its next safe boundary, so an unterminated region at module
    /// scope -- where every payload line's `indent` is 0, indistinguishable
    /// from a fresh top-level line -- does not cascade into one spurious
    /// diagnostic per payload line.
    pub opaque: bool,
}

fn split_lines(text: &str) -> Vec<(u32, u32, &str)> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0usize;
    for i in 0..bytes.len() {
        if bytes[i] == b'\n' {
            let mut end = i;
            if end > start && bytes[end - 1] == b'\r' {
                end -= 1;
            }
            lines.push((start as u32, end as u32, &text[start..end]));
            start = i + 1;
        }
    }
    if start < bytes.len() || lines.is_empty() {
        lines.push((start as u32, bytes.len() as u32, &text[start..bytes.len()]));
    }
    lines
}

/// An open opaque-payload region: the kind of opener that started it, the
/// number of leading-space bytes every subsequent line must have to still
/// count as payload, and the fixed `.indent` value payload lines are
/// emitted with (never computed from their own leading-space count, so
/// extra indentation beyond the required prefix stays literal payload
/// content rather than raising the structural level).
struct Region {
    opener: Opener,
    required_prefix: usize,
    fixed_indent: u32,
}

fn leading_space_count(raw: &str) -> usize {
    raw.as_bytes().iter().take_while(|&&b| b == b' ').count()
}

pub fn lex_lines(text: &str) -> Result<Vec<StructLine<'_>>, IndentError> {
    let mut out = Vec::new();
    let mut stack: Vec<u32> = vec![0];
    let mut region: Option<Region> = None;

    for (start, end, raw) in split_lines(text) {
        // A line that is entirely whitespace (including tabs, and
        // including inside an opaque payload region) carries no
        // structural meaning of its own: it never opens/closes an
        // indentation level and never triggers the tab/two-space checks
        // below, which exist only to keep *structural* indentation
        // unambiguous. It stays part of an open region (a blank line
        // inside `math:`/`tex:` is ordinary; one inside `$$` is rejected
        // later, at the block-parsing stage, exactly as today; one inside
        // a fenced code block is literal content).
        if raw.trim().is_empty() {
            out.push(StructLine {
                byte_start: start,
                byte_end: end,
                indent: 0,
                content: "",
                content_byte_start: start,
                is_blank: true,
                opaque: region.is_some(),
            });
            continue;
        }

        if let Some(r) = &region {
            let leading = leading_space_count(raw);
            if leading >= r.required_prefix {
                let content = &raw[r.required_prefix..];
                let closes = match r.opener {
                    Opener::Indented => false,
                    Opener::Dollar => opaque::dollar_closes(content),
                    Opener::Fence { len } => opaque::fence_closes(content, len),
                };
                out.push(StructLine {
                    byte_start: start,
                    byte_end: end,
                    indent: r.fixed_indent,
                    content,
                    content_byte_start: start + r.required_prefix as u32,
                    is_blank: false,
                    opaque: true,
                });
                if closes {
                    region = None;
                }
                continue;
            }
            // Insufficient prefix: the region ends here, without this
            // line being part of it. It falls through to ordinary
            // structural lexing below, using the same (unmodified) stack
            // the region left it in. `region` is reassigned unconditionally
            // at the end of that path (`None` if this line opens nothing),
            // so no explicit reset is needed here.
        }

        let bytes = raw.as_bytes();
        let mut spaces = 0usize;
        let mut idx = 0usize;
        while idx < bytes.len() && bytes[idx] == b' ' {
            spaces += 1;
            idx += 1;
        }
        if idx < bytes.len() && bytes[idx] == b'\t' {
            return Err(IndentError::Tab {
                byte_offset: start + idx as u32,
            });
        }

        if spaces % 2 != 0 {
            return Err(IndentError::NotMultipleOfTwo {
                byte_offset: start,
            });
        }
        let indent = (spaces / 2) as u32;

        if indent > *stack.last().unwrap() {
            stack.push(indent);
        } else if indent < *stack.last().unwrap() {
            while *stack.last().unwrap() > indent {
                stack.pop();
            }
            if *stack.last().unwrap() != indent {
                return Err(IndentError::UnopenedDedent {
                    byte_offset: start,
                });
            }
        }

        let content = &raw[idx..];
        out.push(StructLine {
            byte_start: start,
            byte_end: end,
            indent,
            content,
            content_byte_start: start + idx as u32,
            is_blank: false,
            opaque: false,
        });

        region = opaque::opener(content).map(|o| match o {
            Opener::Indented => Region {
                opener: o,
                required_prefix: 2 * (indent as usize + 1),
                fixed_indent: indent + 1,
            },
            Opener::Dollar | Opener::Fence { .. } => Region {
                opener: o,
                required_prefix: 2 * indent as usize,
                fixed_indent: indent,
            },
        });
    }

    Ok(out)
}
