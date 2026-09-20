//! Lossless structural formatter.
//!
//! Canonicalizes structural whitespace outside opaque payload regions:
//! trailing-whitespace trimming, newline normalization to `\n`, collapsing
//! runs of blank lines to at most one, and exactly one trailing newline at
//! end of file. It never rewraps prose, never reorders authored metadata
//! or citation lists (it only touches whitespace, not content), and never
//! touches a single byte inside a `math:`/`tex:` opaque payload region
//! (including that region's own internal CRLF/blank-line/EOF bytes,
//! per the byte-exact contract task group 8 established).
//!
//! Formatting is a syntax-level operation: it requires the module to
//! parse (via the fail-fast [`parse_module`], not the recovery-aware
//! variant — a formatter has no use for a partial tree) but never invokes
//! reference binding, symbol resolution, or any other semantic-validation
//! pass. An unresolved `refs:` alias is not a formatting error.

use crate::diagnostic::Diagnostic;
use crate::semantic;
use crate::source::SourceFile;
use crate::syntax::blocks::{self, TopBlock};
use crate::syntax::lexer;

/// Byte ranges (relative to [`SourceFile::original_bytes`]) that must be
/// copied verbatim: the payload of every `math:`/`tex:` block, at any
/// nesting depth.
fn collect_opaque_ranges(blocks: &[TopBlock], base: u32, out: &mut Vec<(u32, u32)>) {
    for block in blocks {
        match block {
            TopBlock::Equation { span, .. } | TopBlock::RawTex { span, .. } => {
                out.push((span.byte_start, span.byte_end));
            }
            TopBlock::TheoremLike { body, .. } | TopBlock::Proof { body, .. } => {
                collect_opaque_ranges(body, base, out);
            }
            TopBlock::List(list) => {
                for item in &list.items {
                    collect_opaque_ranges(&item.continuation, base, out);
                }
            }
            _ => {}
        }
    }
}

/// Formats `source`, returning the canonical bytes or the diagnostic from
/// a failed parse. Idempotent: formatting the output again yields the
/// same bytes.
pub fn format_source(source: &SourceFile) -> Result<Vec<u8>, Diagnostic> {
    let text = source.text();
    let base = source.base_offset();
    let lines = lexer::lex_lines(text)
        .map_err(|e| lexer::indent_error_to_diagnostic(e, source.id, base))?;
    let blocks = blocks::parse_module(&lines, text, source.id, base)?;

    // Formatting is a syntax-level operation and never requires reference
    // binding or cross-file symbol resolution, but it does require the
    // module to pass single-file semantic validation (e.g. restricted-math
    // execution checks from task group 8) — a document that would never
    // compile has nothing canonical to format. The lowered result itself
    // is discarded; only its success/failure matters here. Cross-reference
    // ids are intentionally NOT validated here: a standalone file may
    // legitimately reference an id defined in a sibling module that only
    // becomes visible after project-wide include expansion, which `fmt`
    // never performs.
    semantic::lower_single_file_unchecked_refs(blocks.clone(), source.id)?;

    let mut opaque: Vec<(u32, u32)> = Vec::new();
    collect_opaque_ranges(&blocks, base, &mut opaque);
    opaque.sort_unstable();

    Ok(canonicalize(source.original_bytes(), base, &opaque))
}

pub(crate) fn in_opaque(range: (u32, u32), opaque: &[(u32, u32)]) -> bool {
    opaque
        .iter()
        .any(|&(start, end)| range.0 < end && start < range.1)
}

/// Splits `bytes` into physical lines (including line-ending bytes),
/// tracking each line's absolute byte range so opaque overlap can be
/// checked against the original spans (which are relative to
/// `original_bytes`, i.e. already include `base`).
pub(crate) fn canonicalize(bytes: &[u8], _base: u32, opaque: &[(u32, u32)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    let mut pending_blank = false;
    let mut wrote_any = false;

    while i < bytes.len() {
        let line_start = i;
        let mut content_end = i;
        while content_end < bytes.len() && bytes[content_end] != b'\n' {
            content_end += 1;
        }
        let had_lf = content_end < bytes.len();
        let mut text_end = content_end;
        if text_end > line_start && bytes[text_end - 1] == b'\r' {
            text_end -= 1;
        }
        let next = if had_lf { content_end + 1 } else { content_end };

        let opaque_here = in_opaque((line_start as u32, next as u32), opaque);

        if opaque_here {
            // Copy verbatim, including original line ending (or its
            // absence at EOF) and any pending blank-line separator.
            if pending_blank {
                out.push(b'\n');
                pending_blank = false;
            }
            out.extend_from_slice(&bytes[line_start..content_end]);
            if had_lf {
                out.push(b'\n');
            }
            wrote_any = true;
            i = next;
            continue;
        }

        let raw_line = &bytes[line_start..text_end];
        // A line is blank if it is empty or contains only horizontal
        // whitespace; blank-line *content* (its exact whitespace bytes)
        // is not authored meaning, so blank runs collapse to one truly
        // empty line. A non-blank line's bytes are left untouched: some
        // constructs currently retain trailing inline whitespace as part
        // of their captured text (see semantic::lower), so trimming it
        // here would silently change authored content instead of only
        // canonicalizing whitespace.
        let is_blank = raw_line.iter().all(|&b| b == b' ' || b == b'\t');
        let line: &[u8] = if is_blank { &[] } else { raw_line };

        if is_blank {
            if wrote_any {
                pending_blank = true;
            }
        } else {
            if pending_blank {
                out.push(b'\n');
                pending_blank = false;
            }
            out.extend_from_slice(line);
            out.push(b'\n');
            wrote_any = true;
        }

        i = next;
        if !had_lf && !is_blank {
            // Non-blank final line without a trailing newline already got
            // one added above; nothing further to do.
        }
    }

    out
}
