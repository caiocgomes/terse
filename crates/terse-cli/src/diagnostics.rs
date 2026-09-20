//! Human-readable and JSON rendering of `terse_core::diagnostic::Diagnostic`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use terse_core::diagnostic::{Diagnostic, Severity};
pub use terse_core::diagnostic::JSON_ENVELOPE_VERSION;
use terse_core::source::FileId;

/// Computes a one-based (line, column) position from a byte offset into
/// `text`. Columns are counted in Unicode scalar values.
pub fn line_col(text: &str, byte_offset: u32) -> (u32, u32) {
    let byte_offset = byte_offset as usize;
    let mut line = 1u32;
    let mut col = 1u32;
    for (idx, ch) in text.char_indices() {
        if idx >= byte_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else if ch != '\r' {
            col += 1;
        }
    }
    (line, col)
}

/// Renders one diagnostic as a human-readable string, given the source path
/// and text of the file its primary span points into.
pub fn render_human(diag: &Diagnostic, file_path: &str, file_text: &str) -> String {
    let severity = match diag.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    let mut out = format!("{severity}[{}]: {}", diag.code, diag.message);
    if let Some(span) = diag.primary {
        let (line, col) = line_col(file_text, span.byte_start);
        out.push_str(&format!("\n  --> {file_path}:{line}:{col}"));
    }
    for related in &diag.related {
        out.push_str(&format!("\n  note: {}", related.message));
    }
    if let Some(help) = &diag.help {
        out.push_str(&format!("\n  help: {help}"));
    }
    out
}

/// A file index mapping each `FileId` to the (display path, full text) it
/// came from -- built by whichever loader discovered the project's
/// modules -- lets a diagnostic anchored anywhere in the include graph be
/// rendered with its own original path and byte-accurate position, not
/// always the entry's.
pub type FileIndex = HashMap<FileId, (PathBuf, String)>;

#[derive(Serialize)]
pub struct JsonPosition {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub byte_start: u32,
    pub byte_end: u32,
}

#[derive(Serialize)]
pub struct JsonRelated {
    pub message: String,
    pub position: Option<JsonPosition>,
}

#[derive(Serialize)]
pub struct JsonDiagnostic {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub position: Option<JsonPosition>,
    pub related: Vec<JsonRelated>,
    pub help: Option<String>,
}

#[derive(Serialize)]
pub struct JsonEnvelope {
    pub version: u32,
    pub diagnostics: Vec<JsonDiagnostic>,
}

fn position_of(span: terse_core::source::SourceSpan, index: &FileIndex, fallback: &Path) -> JsonPosition {
    match index.get(&span.file_id) {
        Some((path, text)) => {
            let (line, column) = line_col(text, span.byte_start);
            JsonPosition {
                file: path.to_string_lossy().into_owned(),
                line,
                column,
                byte_start: span.byte_start,
                byte_end: span.byte_end,
            }
        }
        None => JsonPosition {
            file: fallback.to_string_lossy().into_owned(),
            line: 0,
            column: 0,
            byte_start: span.byte_start,
            byte_end: span.byte_end,
        },
    }
}

fn to_json_diagnostic(diag: &Diagnostic, index: &FileIndex, fallback: &Path) -> JsonDiagnostic {
    JsonDiagnostic {
        severity: match diag.severity {
            Severity::Error => "error".to_string(),
            Severity::Warning => "warning".to_string(),
        },
        code: diag.code.to_string(),
        message: diag.message.clone(),
        position: diag.primary.map(|s| position_of(s, index, fallback)),
        related: diag
            .related
            .iter()
            .map(|r| JsonRelated {
                message: r.message.clone(),
                position: r.span.map(|s| position_of(s, index, fallback)),
            })
            .collect(),
        help: diag.help.clone(),
    }
}

/// Builds the versioned JSON envelope for a full diagnostic list, in the
/// same (already deterministic) order they were produced. `fallback` is
/// used only for a diagnostic whose span points at a file not present in
/// `index` (should not normally happen, but avoids ever inventing a fake
/// path silently).
pub fn to_json_envelope(diags: &[Diagnostic], index: &FileIndex, fallback: &Path) -> JsonEnvelope {
    JsonEnvelope {
        version: JSON_ENVELOPE_VERSION,
        diagnostics: diags.iter().map(|d| to_json_diagnostic(d, index, fallback)).collect(),
    }
}

/// Renders the versioned JSON envelope as a single serialized string,
/// suitable for printing to stdout on its own (this function does not
/// print; callers control whether it goes to stdout, and must keep all
/// other progress/log output on stderr so stdout stays parseable).
pub fn render_json(diags: &[Diagnostic], index: &FileIndex, fallback: &Path) -> String {
    serde_json::to_string(&to_json_envelope(diags, index, fallback))
        .expect("diagnostic JSON envelope is always serializable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_col_counts_unicode_scalars() {
        let text = "ab\ncd\n";
        assert_eq!(line_col(text, 0), (1, 1));
        assert_eq!(line_col(text, 2), (1, 3));
        assert_eq!(line_col(text, 3), (2, 1));
    }

    #[test]
    fn test_render_human_includes_position_and_help() {
        use terse_core::source::{FileId, SourceSpan};
        let span = SourceSpan::new(FileId(0), 3, 3);
        let diag = Diagnostic::error("E-PARSE-002", "malformed header", span)
            .with_help("use a quoted path");
        let rendered = render_human(&diag, "entry.trs", "a:\nb\n");
        assert!(rendered.contains("E-PARSE-002"));
        assert!(rendered.contains("entry.trs:2:1"));
        assert!(rendered.contains("use a quoted path"));
    }
}
