//! Lossless indentation lexing.
//!
//! Splits source text into logical lines and validates structural
//! indentation (exactly two spaces per level, no tabs, no dedent to a level
//! that was never opened). This pass is content-agnostic: it knows nothing
//! about reserved words or block grammar, only indentation structure.

use crate::diagnostic::Diagnostic;
use crate::source::{FileId, SourceSpan};

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

pub fn lex_lines(text: &str) -> Result<Vec<StructLine<'_>>, IndentError> {
    let mut out = Vec::new();
    let mut stack: Vec<u32> = vec![0];

    for (start, end, raw) in split_lines(text) {
        // A line that is entirely whitespace (including tabs, and
        // including inside an opaque `math:`/`tex:` payload) carries no
        // structural meaning of its own: it never opens/closes an
        // indentation level and never triggers the tab/two-space checks
        // below, which exist only to keep *structural* indentation
        // unambiguous.
        if raw.trim().is_empty() {
            out.push(StructLine {
                byte_start: start,
                byte_end: end,
                indent: 0,
                content: "",
                content_byte_start: start,
                is_blank: true,
            });
            continue;
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

        out.push(StructLine {
            byte_start: start,
            byte_end: end,
            indent,
            content: &raw[idx..],
            content_byte_start: start + idx as u32,
            is_blank: false,
        });
    }

    Ok(out)
}
