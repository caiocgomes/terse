//! Reserved-header and block recognition.
//!
//! Recognizes: `document:` (full metadata), `refs:` (declarations only, no
//! provider resolution), `include`, headings (`#`/`##`/`###`), paragraphs,
//! ordered/unordered lists, `math:` equations, figures, tables,
//! theorem-like blocks, proofs, `tex:` raw blocks, and the explicit
//! `bibliography` marker (global placement is validated in task group 12).
//!
//! A generalized recursive block-sequence parser (`parse_block_sequence`)
//! backs module level, nested theorem/proof bodies, and list-item
//! continuations, each with its own allowed-construct context.

use std::collections::HashSet;

use crate::diagnostic::Diagnostic;
use crate::source::{FileId, SourceSpan};
use crate::syntax::lexer::StructLine;

/// Reserved textual starts recognized at structural line start. Escaping the
/// first character with `\` turns any of these into ordinary prose.
const RESERVED_WORDS: &[&str] = &[
    "document",
    "include",
    "refs",
    "figure",
    "table",
    "math",
    "tex",
    "bibliography",
    "theorem",
    "proposition",
    "lemma",
    "definition",
    "example",
    "remark",
    "proof",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Doi,
    Arxiv,
    Isbn,
    Url,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RefEntry {
    pub alias: String,
    pub kind: RefKind,
    pub identifier: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RawAffiliation {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawAuthor {
    pub name: (String, SourceSpan),
    pub affiliation: Option<RawAffiliation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawMetadata {
    pub title: (String, SourceSpan),
    pub subtitle: Option<String>,
    pub authors: Vec<RawAuthor>,
    pub affiliations: Vec<String>,
    pub date: Option<String>,
    pub language: Option<(String, SourceSpan)>,
    pub abstract_paragraphs: Vec<(String, SourceSpan)>,
    pub keywords: Vec<String>,
}

/// A pipe-table delimiter cell's column alignment, or `Default` for a
/// field-form table (which has no delimiter row) and for a plain `---`
/// delimiter cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnAlign {
    Default,
    Left,
    Center,
    Right,
}

/// A raw (not yet inline-parsed) table header or row: its cell texts
/// plus one span for the whole line, shared by both table syntaxes and
/// every stage between parsing and `finish_table`.
type TableCells = (Vec<String>, SourceSpan);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TheoremKind {
    Theorem,
    Proposition,
    Lemma,
    Definition,
    Example,
    Remark,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawListItem {
    pub text: String,
    pub continuation: Vec<TopBlock>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawList {
    pub ordered: bool,
    /// The authored first marker value for an ordered list (e.g. `9` for a
    /// list starting `9.`). Always `None` for unordered lists.
    pub start: Option<u32>,
    pub items: Vec<RawListItem>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TopBlock {
    Document {
        metadata: RawMetadata,
    },
    Refs {
        entries: Vec<RefEntry>,
        span: SourceSpan,
    },
    Include {
        path: String,
        span: SourceSpan,
    },
    Heading {
        level: u8,
        text: String,
        id: Option<String>,
        span: SourceSpan,
    },
    Paragraph {
        text: String,
        span: SourceSpan,
    },
    List(RawList),
    Equation {
        id: Option<String>,
        numbered: bool,
        payload: String,
        span: SourceSpan,
    },
    Figure {
        path: String,
        id: Option<String>,
        role: Option<String>,
        caption: String,
        caption_span: SourceSpan,
        alt: String,
        span: SourceSpan,
    },
    /// `header` and each of `rows` carry raw, not-yet-inline-parsed cell
    /// text plus one span for their whole line: every cell on a header or
    /// row line shares that line's diagnostic location, the same
    /// granularity a figure's `caption` field has (one span for the whole
    /// field, not one per character). Lowering parses each cell with
    /// `parse_inline_at`, using that shared span.
    Table {
        id: Option<String>,
        caption: Option<(String, SourceSpan)>,
        align: Vec<ColumnAlign>,
        header: TableCells,
        rows: Vec<TableCells>,
        span: SourceSpan,
    },
    TheoremLike {
        kind: TheoremKind,
        title: Option<String>,
        id: Option<String>,
        body: Vec<TopBlock>,
        span: SourceSpan,
    },
    Proof {
        id: Option<String>,
        of: Option<String>,
        body: Vec<TopBlock>,
        span: SourceSpan,
    },
    RawTex {
        payload: String,
        span: SourceSpan,
    },
    /// A fenced code block. `language` is the fence's first
    /// whitespace-delimited info-string word, kept as written; `code` is
    /// the opaque content, kept byte for byte. Carries no ID.
    CodeBlock {
        language: Option<String>,
        code: String,
        span: SourceSpan,
    },
    /// The explicit `bibliography` marker: authors place this to control
    /// where the derived bibliography renders. At most one is permitted
    /// per project (task group 12); zero markers with citations present
    /// appends a derived bibliography at the end instead.
    Bibliography {
        span: SourceSpan,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockContext {
    /// Top of an entry/included module: only place metadata, `refs:`,
    /// `include`, headings, and a bibliography marker may occur.
    Module,
    /// Inside a theorem-like or proof body: no headings or declarations.
    Nested,
    /// Inside a list item's continuation: paragraphs/lists only.
    ListItem,
}

pub fn parse_module(
    lines: &[StructLine<'_>],
    source: &str,
    file_id: FileId,
    base: u32,
) -> Result<Vec<TopBlock>, Diagnostic> {
    let (blocks, _next_i) =
        parse_block_sequence(lines, source, 0, 0, BlockContext::Module, file_id, base)?;
    Ok(blocks)
}

/// Versioned cap (`E-LIMIT-002`) on the number of parse diagnostics a
/// single module reports before recovery stops: bounded so a
/// pathological input cannot produce unbounded diagnostic output.
pub const MAX_DIAGNOSTICS: usize = 20;

/// Like [`parse_module`], but recovers at safe structural boundaries (the
/// next module-scope line, i.e. one at zero structural indentation) after
/// an error instead of stopping at the first one, so a single `check` run
/// can report several independent errors. The returned blocks are a
/// best-effort partial tree and must never be used to build a publishable
/// artifact plan: callers must treat any non-empty diagnostic list as a
/// hard failure regardless of how many blocks were recovered.
pub fn parse_module_with_recovery(
    lines: &[StructLine<'_>],
    source: &str,
    file_id: FileId,
    base: u32,
) -> (Vec<TopBlock>, Vec<Diagnostic>) {
    let mut blocks = Vec::new();
    let mut diags: Vec<Diagnostic> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        match parse_block_sequence(lines, source, i, 0, BlockContext::Module, file_id, base) {
            Ok((mut new_blocks, next_i)) => {
                blocks.append(&mut new_blocks);
                i = next_i;
            }
            Err(diag) => {
                diags.push(diag);
                if diags.len() >= MAX_DIAGNOSTICS {
                    let offset = lines.get(i).map(|l| base + l.byte_start).unwrap_or(base);
                    diags.push(Diagnostic::error(
                        "E-LIMIT-002",
                        format!(
                            "stopped after the versioned limit of {} diagnostics in one module; \
                             further errors were not reported",
                            MAX_DIAGNOSTICS
                        ),
                        SourceSpan::new(file_id, offset, offset),
                    ));
                    break;
                }
                // Recover at the next safe structural boundary: the next
                // line back at module scope (zero indentation). Opaque
                // payload lines (inside an unterminated `math:`/`tex:`
                // body, `$$` display, or fenced code block) are skipped
                // regardless of their own `.indent`, which for a `$$` or
                // fence opened at module scope is fixed at 0 --
                // indistinguishable from a fresh top-level line -- and
                // would otherwise make recovery resynchronize on every
                // single payload line, one spurious diagnostic each.
                let mut j = i + 1;
                while j < lines.len()
                    && (lines[j].is_blank || lines[j].opaque || lines[j].indent > 0)
                {
                    j += 1;
                }
                i = j.max(i + 1);
            }
        }
    }

    (blocks, diags)
}

/// Versioned nesting-depth bound (`E-LIMIT-001`): the maximum structural
/// indentation level (one level per two authored indentation columns)
/// any line may carry. This admits 64 levels of nested lists/theorem/proof
/// bodies -- far beyond any realistic document -- before rejecting the
/// document outright with an explicit diagnostic, rather than a partial
/// plan or a recursive-descent stack overflow.
pub const MAX_STRUCTURAL_INDENT: u32 = 64;

fn parse_block_sequence<'a>(
    lines: &[StructLine<'a>],
    source: &str,
    start: usize,
    indent: u32,
    context: BlockContext,
    file_id: FileId,
    base: u32,
) -> Result<(Vec<TopBlock>, usize), Diagnostic> {
    if indent > MAX_STRUCTURAL_INDENT {
        let offset = lines
            .get(start)
            .map(|l| base + l.byte_start)
            .unwrap_or(base);
        return Err(Diagnostic::error(
            "E-LIMIT-001",
            format!(
                "structural nesting exceeds the versioned limit of {} indentation columns",
                MAX_STRUCTURAL_INDENT
            ),
            SourceSpan::new(file_id, offset, offset),
        ));
    }
    let mut blocks = Vec::new();
    let mut i = start;

    while i < lines.len() {
        let line = &lines[i];
        if line.is_blank {
            i += 1;
            continue;
        }
        if line.indent < indent {
            break;
        }
        if line.indent > indent {
            return Err(malformed(file_id, base, line, "unexpected indentation"));
        }

        let content = line.content;

        if let Some(level) = heading_level(content) {
            if context != BlockContext::Module {
                return Err(malformed(
                    file_id,
                    base,
                    line,
                    "headings must remain at module scope",
                ));
            }
            let (title_text, attrs) = split_trailing_attributes(&content[level as usize..]);
            let text = title_text.trim_start().to_string();
            let attrs = attrs.unwrap_or_default();
            validate_attributes(&["id"], &attrs, line, file_id, base)?;
            let id = attrs.iter().find(|(k, _)| k == "id").map(|(_, v)| v.clone());
            let span = mk_span(file_id, base, line.content_byte_start, line.byte_end);
            blocks.push(TopBlock::Heading { level, text, id, span });
            i += 1;
            continue;
        }

        if let Some(rest) = content.strip_prefix('\\') {
            if starts_with_reserved_word(rest).is_some() {
                let (text, next_i) = consume_paragraph(lines, i, rest, indent);
                let span = mk_span(
                    file_id,
                    base,
                    line.content_byte_start,
                    lines[next_i - 1].byte_end,
                );
                blocks.push(TopBlock::Paragraph { text, span });
                i = next_i;
                continue;
            }
        }

        if detect_list_marker(content).is_some() {
            let (block, next_i) = parse_list(lines, source, i, file_id, base)?;
            blocks.push(block);
            i = next_i;
            continue;
        }

        if content.starts_with("$$") {
            if context == BlockContext::ListItem {
                return Err(malformed(
                    file_id,
                    base,
                    line,
                    "display math is not supported inside a list item",
                ));
            }
            let (block, next_i) = parse_dollar_display(lines, source, i, file_id, base)?;
            blocks.push(block);
            i = next_i;
            continue;
        }

        // Fenced code blocks are accepted in every context, including
        // list items: unlike `$$`/`math:`, a code block is the one
        // non-paragraph, non-list construct a list item's continuation
        // may contain (installation-step snippets are the common case).
        if let Some(len) = crate::syntax::opaque::fence_len(content) {
            let (block, next_i) = parse_code_block(lines, source, i, len, file_id, base)?;
            blocks.push(block);
            i = next_i;
            continue;
        }

        // A bare pipe table, like `table:`, is not content a list item's
        // continuation accepts (D1); unlike `$$`/`math:`, whose openers
        // are unambiguous keywords, a `|` line with no delimiter row
        // below it is not a table at all, so it falls through to prose.
        if pipe_table_opens(lines, i) {
            if context == BlockContext::ListItem {
                return Err(malformed(
                    file_id,
                    base,
                    line,
                    "a bare pipe table is not supported inside a list item",
                ));
            }
            let (block, next_i) = parse_pipe_table(lines, i, file_id, base)?;
            blocks.push(block);
            i = next_i;
            continue;
        }

        if let Some(word) = starts_with_reserved_word(content) {
            if context == BlockContext::ListItem {
                return Err(malformed(
                    file_id,
                    base,
                    line,
                    "this block kind is not supported inside a list item",
                ));
            }
            match word {
                "document" => {
                    if context != BlockContext::Module {
                        return Err(malformed(file_id, base, line, "declarations require module scope"));
                    }
                    if !blocks.is_empty() {
                        return Err(malformed(
                            file_id,
                            base,
                            line,
                            "document metadata must appear before any content, and only once",
                        ));
                    }
                    let (block, next_i) = parse_document(lines, i, file_id, base)?;
                    blocks.push(block);
                    i = next_i;
                }
                "refs" => {
                    if context != BlockContext::Module {
                        return Err(malformed(file_id, base, line, "declarations require module scope"));
                    }
                    let (block, next_i) = parse_refs(lines, i, file_id, base)?;
                    blocks.push(block);
                    i = next_i;
                }
                "include" => {
                    if context != BlockContext::Module {
                        return Err(malformed(file_id, base, line, "declarations require module scope"));
                    }
                    let (block, next_i) = parse_include(lines, i, file_id, base)?;
                    blocks.push(block);
                    i = next_i;
                }
                "bibliography" => {
                    if context != BlockContext::Module {
                        return Err(malformed(file_id, base, line, "declarations require module scope"));
                    }
                    if content.trim_end() != "bibliography" {
                        return Err(malformed(
                            file_id,
                            base,
                            line,
                            "'bibliography' must appear alone on its line, with no attributes or trailing text",
                        ));
                    }
                    let span = mk_span(file_id, base, line.content_byte_start, line.byte_end);
                    blocks.push(TopBlock::Bibliography { span });
                    i += 1;
                }
                "math" => {
                    let (block, next_i) = parse_equation(lines, source, i, file_id, base)?;
                    blocks.push(block);
                    i = next_i;
                }
                "figure" => {
                    let (block, next_i) = parse_figure(lines, i, file_id, base)?;
                    blocks.push(block);
                    i = next_i;
                }
                "table" => {
                    let (block, next_i) = parse_table(lines, i, file_id, base)?;
                    blocks.push(block);
                    i = next_i;
                }
                "theorem" | "proposition" | "lemma" | "definition" | "example" | "remark" => {
                    let (block, next_i) =
                        parse_theorem_like(lines, source, i, word, file_id, base)?;
                    blocks.push(block);
                    i = next_i;
                }
                "proof" => {
                    let (block, next_i) = parse_proof(lines, source, i, file_id, base)?;
                    blocks.push(block);
                    i = next_i;
                }
                "tex" => {
                    let (block, next_i) = parse_raw_tex(lines, source, i, file_id, base)?;
                    blocks.push(block);
                    i = next_i;
                }
                _ => unreachable!("all reserved words are handled above"),
            }
            continue;
        }

        let (text, next_i) = consume_paragraph(lines, i, content, indent);
        let span = mk_span(
            file_id,
            base,
            line.content_byte_start,
            lines[next_i - 1].byte_end,
        );
        blocks.push(TopBlock::Paragraph { text, span });
        i = next_i;
    }

    Ok((blocks, i))
}

fn heading_level(content: &str) -> Option<u8> {
    let hashes = content.chars().take_while(|&c| c == '#').count();
    if hashes == 0 || hashes > 3 {
        return None;
    }
    let rest = &content[hashes..];
    if rest.starts_with(' ') {
        Some(hashes as u8)
    } else {
        None
    }
}

fn starts_with_reserved_word(content: &str) -> Option<&'static str> {
    for &word in RESERVED_WORDS {
        if let Some(rest) = content.strip_prefix(word) {
            let boundary_ok = match rest.chars().next() {
                None => true,
                Some(c) => !(c.is_alphanumeric() || c == '_' || c == '-'),
            };
            if boundary_ok {
                return Some(word);
            }
        }
    }
    None
}

fn detect_list_marker(content: &str) -> Option<(bool, Option<u32>, usize)> {
    if let Some(rest) = content.strip_prefix("- ") {
        return Some((false, None, content.len() - rest.len()));
    }
    let bytes = content.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx > 0 && content[idx..].starts_with(". ") {
        let num: u32 = content[..idx].parse().ok()?;
        return Some((true, Some(num), idx + 2));
    }
    None
}

fn parse_list<'a>(
    lines: &[StructLine<'a>],
    source: &str,
    start: usize,
    file_id: FileId,
    base: u32,
) -> Result<(TopBlock, usize), Diagnostic> {
    let list_indent = lines[start].indent;
    let (first_ordered, _, _) =
        detect_list_marker(lines[start].content).expect("caller matched a list marker");
    let mut items = Vec::new();
    let mut i = start;
    let mut expected_next: Option<u32> = None;
    let mut list_start: Option<u32> = None;
    let mut first = true;

    loop {
        if i >= lines.len() {
            break;
        }
        if lines[i].is_blank {
            let mut j = i + 1;
            while j < lines.len() && lines[j].is_blank {
                j += 1;
            }
            let continues = j < lines.len()
                && lines[j].indent == list_indent
                && detect_list_marker(lines[j].content).map(|m| m.0) == Some(first_ordered);
            if !continues {
                break;
            }
            i = j;
            continue;
        }
        if lines[i].indent != list_indent {
            break;
        }
        let marker = detect_list_marker(lines[i].content);
        let (ordered, num, marker_len) = match marker {
            Some(m) => m,
            None => break,
        };
        if ordered != first_ordered {
            break;
        }
        if ordered {
            let n = num.expect("ordered marker always carries a number");
            if first {
                list_start = Some(n);
            }
            if let Some(expected) = expected_next {
                if n != expected {
                    return Err(malformed(
                        file_id,
                        base,
                        &lines[i],
                        &format!("expected ordered marker {expected}, found {n}"),
                    ));
                }
            }
            expected_next = Some(n + 1);
        }
        first = false;

        let line = &lines[i];
        let item_text = line.content[marker_len..].to_string();
        let item_start_byte = line.content_byte_start;
        let mut item_end_byte = line.byte_end;
        i += 1;

        let continuation_indent = list_indent + 1;
        let (continuation, next_i) = parse_block_sequence(
            lines,
            source,
            i,
            continuation_indent,
            BlockContext::ListItem,
            file_id,
            base,
        )?;
        if next_i > i {
            item_end_byte = lines[next_i - 1].byte_end;
        }
        i = next_i;

        items.push(RawListItem {
            text: item_text,
            continuation,
            span: mk_span(file_id, base, item_start_byte, item_end_byte),
        });
    }

    let span_end = items
        .last()
        .map(|it| it.span.byte_end - base)
        .unwrap_or(lines[start].byte_end);
    let span = mk_span(file_id, base, lines[start].content_byte_start, span_end);
    Ok((
        TopBlock::List(RawList {
            ordered: first_ordered,
            start: list_start,
            items,
            span,
        }),
        i,
    ))
}

fn consume_paragraph<'a>(
    lines: &[StructLine<'a>],
    start: usize,
    first_text: &str,
    indent: u32,
) -> (String, usize) {
    let mut parts = vec![first_text.trim_end().to_string()];
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        if line.is_blank || line.indent != indent {
            break;
        }
        let content = line.content;
        if heading_level(content).is_some() {
            break;
        }
        if detect_list_marker(content).is_some() {
            break;
        }
        if content.starts_with("$$") {
            break;
        }
        if crate::syntax::opaque::fence_len(content).is_some() {
            break;
        }
        // A `|` line by itself does not end a running paragraph (it stays
        // prose, D1's "pipe-looking prose" case): only a genuine opener,
        // confirmed by the delimiter-row lookahead, does.
        if pipe_table_opens(lines, i) {
            break;
        }
        if starts_with_reserved_word(content).is_some() {
            break;
        }
        if let Some(rest) = content.strip_prefix('\\') {
            if starts_with_reserved_word(rest).is_some() {
                parts.push(rest.trim_end().to_string());
                i += 1;
                continue;
            }
        }
        parts.push(content.trim_end().to_string());
        i += 1;
    }
    (parts.join(" "), i)
}

fn parse_document<'a>(
    lines: &[StructLine<'a>],
    start: usize,
    file_id: FileId,
    base: u32,
) -> Result<(TopBlock, usize), Diagnostic> {
    let header = &lines[start];
    if header.content.trim_end() != "document:" {
        return Err(malformed(
            file_id,
            base,
            header,
            "malformed document header, expected 'document:'",
        ));
    }

    let body_indent = header.indent + 1;
    let mut i = start + 1;

    let mut title: Option<(String, SourceSpan)> = None;
    let mut subtitle = None;
    let mut authors = Vec::new();
    let mut affiliations = Vec::new();
    let mut date = None;
    let mut language = None;
    let mut abstract_paragraphs = Vec::new();
    let mut keywords = Vec::new();
    let mut seen: HashSet<&'static str> = HashSet::new();

    while i < lines.len() {
        let line = &lines[i];
        if line.is_blank {
            i += 1;
            continue;
        }
        if line.indent < body_indent {
            break;
        }
        if line.indent > body_indent {
            return Err(malformed(
                file_id,
                base,
                line,
                "unexpected indentation in document metadata",
            ));
        }
        let content = line.content;

        if let Some(rest) = content.strip_prefix("title:") {
            require_unseen(&mut seen, "title", line, file_id, base)?;
            let text = parse_scalar(rest.trim_start()).ok_or_else(|| {
                malformed(file_id, base, line, "expected a scalar value for 'title'")
            })?;
            title = Some((text, mk_span(file_id, base, line.content_byte_start, line.byte_end)));
            i += 1;
        } else if let Some(rest) = content.strip_prefix("subtitle:") {
            require_unseen(&mut seen, "subtitle", line, file_id, base)?;
            subtitle = Some(parse_scalar(rest.trim_start()).ok_or_else(|| {
                malformed(file_id, base, line, "expected a scalar value for 'subtitle'")
            })?);
            i += 1;
        } else if let Some(rest) = content.strip_prefix("date:") {
            require_unseen(&mut seen, "date", line, file_id, base)?;
            date = Some(parse_scalar(rest.trim_start()).ok_or_else(|| {
                malformed(file_id, base, line, "expected a scalar value for 'date'")
            })?);
            i += 1;
        } else if let Some(rest) = content.strip_prefix("language:") {
            require_unseen(&mut seen, "language", line, file_id, base)?;
            let text = parse_scalar(rest.trim_start()).ok_or_else(|| {
                malformed(file_id, base, line, "expected a scalar value for 'language'")
            })?;
            language = Some((text, mk_span(file_id, base, line.content_byte_start, line.byte_end)));
            i += 1;
        } else if let Some(rest) = content.strip_prefix("keywords:") {
            require_unseen(&mut seen, "keywords", line, file_id, base)?;
            keywords = parse_string_list(rest.trim_start()).ok_or_else(|| {
                malformed(file_id, base, line, "expected a list value for 'keywords'")
            })?;
            i += 1;
        } else if let Some(rest) = content.strip_prefix("affiliations:") {
            require_unseen(&mut seen, "affiliations", line, file_id, base)?;
            affiliations = parse_string_list(rest.trim_start()).ok_or_else(|| {
                malformed(file_id, base, line, "expected a list value for 'affiliations'")
            })?;
            i += 1;
        } else if content.trim_end() == "authors:" {
            require_unseen(&mut seen, "authors", line, file_id, base)?;
            let (parsed, next_i) = parse_authors(lines, i, file_id, base)?;
            authors = parsed;
            i = next_i;
        } else if content.trim_end() == "abstract:" {
            require_unseen(&mut seen, "abstract", line, file_id, base)?;
            let (parsed, next_i) = parse_abstract(lines, i, file_id, base)?;
            abstract_paragraphs = parsed;
            i = next_i;
        } else {
            return Err(malformed(
                file_id,
                base,
                line,
                "unsupported document metadata field",
            ));
        }
    }

    let title = title.ok_or_else(|| {
        Diagnostic::error(
            "E-META-001",
            "document metadata requires a title",
            mk_span(file_id, base, header.content_byte_start, header.byte_end),
        )
    })?;

    Ok((
        TopBlock::Document {
            metadata: RawMetadata {
                title,
                subtitle,
                authors,
                affiliations,
                date,
                language,
                abstract_paragraphs,
                keywords,
            },
        },
        i,
    ))
}

fn require_unseen(
    seen: &mut HashSet<&'static str>,
    key: &'static str,
    line: &StructLine<'_>,
    file_id: FileId,
    base: u32,
) -> Result<(), Diagnostic> {
    if seen.insert(key) {
        Ok(())
    } else {
        Err(malformed(
            file_id,
            base,
            line,
            &format!("duplicate '{key}' field"),
        ))
    }
}

fn parse_authors<'a>(
    lines: &[StructLine<'a>],
    start: usize,
    file_id: FileId,
    base: u32,
) -> Result<(Vec<RawAuthor>, usize), Diagnostic> {
    let header = &lines[start];
    let item_indent = header.indent + 1;
    let affiliation_indent = item_indent + 1;
    let mut i = start + 1;
    let mut authors = Vec::new();

    while i < lines.len() {
        let line = &lines[i];
        if line.is_blank {
            i += 1;
            continue;
        }
        if line.indent < item_indent {
            break;
        }
        if line.indent > item_indent {
            return Err(malformed(file_id, base, line, "unexpected indentation in authors"));
        }
        let rest = line.content.strip_prefix("- ").ok_or_else(|| {
            malformed(file_id, base, line, "expected '- name: \"...\"' for each author")
        })?;
        let name_value = rest.strip_prefix("name:").ok_or_else(|| {
            malformed(file_id, base, line, "expected 'name:' as the first author field")
        })?;
        let name = parse_scalar(name_value.trim_start()).ok_or_else(|| {
            malformed(file_id, base, line, "expected a scalar value for author 'name'")
        })?;
        let name_span = mk_span(file_id, base, line.content_byte_start, line.byte_end);
        i += 1;

        let mut affiliation = None;
        if i < lines.len() && !lines[i].is_blank && lines[i].indent == affiliation_indent {
            let aff_line = &lines[i];
            if let Some(value) = aff_line.content.strip_prefix("affiliation:") {
                let text = parse_scalar(value.trim_start()).ok_or_else(|| {
                    malformed(file_id, base, aff_line, "expected a scalar value for 'affiliation'")
                })?;
                affiliation = Some(RawAffiliation::Single(text));
            } else if let Some(value) = aff_line.content.strip_prefix("affiliations:") {
                let list = parse_string_list(value.trim_start()).ok_or_else(|| {
                    malformed(file_id, base, aff_line, "expected a list value for 'affiliations'")
                })?;
                affiliation = Some(RawAffiliation::Multiple(list));
            } else {
                return Err(malformed(
                    file_id,
                    base,
                    aff_line,
                    "expected 'affiliation:' or 'affiliations:' for this author",
                ));
            }
            i += 1;
            if i < lines.len() && !lines[i].is_blank && lines[i].indent == affiliation_indent {
                return Err(malformed(
                    file_id,
                    base,
                    &lines[i],
                    "an author may declare only one affiliation form",
                ));
            }
        }

        authors.push(RawAuthor {
            name: (name, name_span),
            affiliation,
        });
    }

    Ok((authors, i))
}

fn parse_abstract<'a>(
    lines: &[StructLine<'a>],
    start: usize,
    file_id: FileId,
    base: u32,
) -> Result<(Vec<(String, SourceSpan)>, usize), Diagnostic> {
    let header = &lines[start];
    let body_indent = header.indent + 1;
    let mut i = start + 1;
    let mut paragraphs = Vec::new();

    while i < lines.len() {
        let line = &lines[i];
        if line.is_blank {
            i += 1;
            continue;
        }
        if line.indent < body_indent {
            break;
        }
        if line.indent > body_indent {
            return Err(malformed(file_id, base, line, "unexpected indentation in abstract"));
        }
        let content = line.content;
        if heading_level(content).is_some() {
            return Err(malformed(
                file_id,
                base,
                line,
                "headings are not permitted inside an abstract",
            ));
        }
        if starts_with_reserved_word(content).is_some() {
            return Err(malformed(
                file_id,
                base,
                line,
                "declarations are not permitted inside an abstract; move them to module scope",
            ));
        }
        let (text, next_i) = consume_paragraph(lines, i, content, body_indent);
        let span = mk_span(file_id, base, line.content_byte_start, lines[next_i - 1].byte_end);
        paragraphs.push((text, span));
        i = next_i;
    }

    Ok((paragraphs, i))
}

fn parse_refs<'a>(
    lines: &[StructLine<'a>],
    start: usize,
    file_id: FileId,
    base: u32,
) -> Result<(TopBlock, usize), Diagnostic> {
    let header = &lines[start];
    if header.content.trim_end() != "refs:" {
        return Err(malformed(file_id, base, header, "malformed refs header, expected 'refs:'"));
    }
    let body_indent = header.indent + 1;
    let mut i = start + 1;
    let mut entries = Vec::new();
    let mut seen_aliases: HashSet<String> = HashSet::new();

    while i < lines.len() {
        let line = &lines[i];
        if line.is_blank {
            i += 1;
            continue;
        }
        if line.indent < body_indent {
            break;
        }
        if line.indent > body_indent {
            return Err(malformed(file_id, base, line, "unexpected indentation in refs"));
        }
        let (alias, kind, identifier) = parse_ref_entry_line(line.content).ok_or_else(|| {
            malformed(
                file_id,
                base,
                line,
                "expected 'alias: doi:...' (or arxiv:/isbn:/url:)",
            )
        })?;
        if !seen_aliases.insert(alias.clone()) {
            return Err(malformed(file_id, base, line, "duplicate reference alias"));
        }
        let span = mk_span(file_id, base, line.content_byte_start, line.byte_end);
        entries.push(RefEntry {
            alias,
            kind,
            identifier,
            span,
        });
        i += 1;
    }

    let span = mk_span(file_id, base, header.content_byte_start, header.byte_end);
    Ok((TopBlock::Refs { entries, span }, i))
}

fn parse_ref_entry_line(content: &str) -> Option<(String, RefKind, String)> {
    let colon = content.find(':')?;
    let alias = &content[..colon];
    if !is_name(alias) {
        return None;
    }
    let rest = content[colon + 1..].trim_start();
    const PREFIXES: &[(&str, RefKind)] = &[
        ("doi:", RefKind::Doi),
        ("arxiv:", RefKind::Arxiv),
        ("isbn:", RefKind::Isbn),
        ("url:", RefKind::Url),
    ];
    for (prefix, kind) in PREFIXES {
        if let Some(value) = rest.strip_prefix(prefix) {
            let value = value.trim().to_string();
            if value.is_empty() {
                return None;
            }
            return Some((alias.to_string(), *kind, value));
        }
    }
    None
}

fn is_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn parse_include<'a>(
    lines: &[StructLine<'a>],
    start: usize,
    file_id: FileId,
    base: u32,
) -> Result<(TopBlock, usize), Diagnostic> {
    let line = &lines[start];
    let rest = line
        .content
        .strip_prefix("include")
        .expect("caller matched the 'include' keyword");
    let rest = rest.strip_prefix(' ').ok_or_else(|| {
        malformed(
            file_id,
            base,
            line,
            "expected 'include \"path\"' with a quoted path",
        )
    })?;
    let (path, _consumed) = parse_quoted(rest).ok_or_else(|| {
        malformed(
            file_id,
            base,
            line,
            "expected a quoted path after 'include'",
        )
    })?;
    let span = mk_span(file_id, base, line.content_byte_start, line.byte_end);
    Ok((TopBlock::Include { path, span }, start + 1))
}

fn parse_equation<'a>(
    lines: &[StructLine<'a>],
    source: &str,
    start: usize,
    file_id: FileId,
    base: u32,
) -> Result<(TopBlock, usize), Diagnostic> {
    let header = &lines[start];
    let rest = header.content.strip_prefix("math").expect("caller matched 'math'");
    let trimmed = rest.trim_end();
    let (attrs_part, colon_ok) = if trimmed == ":" {
        (String::new(), true)
    } else if rest.starts_with(" [") && trimmed.ends_with(':') {
        (trimmed[..trimmed.len() - 1].trim().to_string(), true)
    } else {
        (String::new(), false)
    };
    if !colon_ok {
        return Err(malformed(
            file_id,
            base,
            header,
            "malformed math header, expected 'math:' or 'math [attrs]:'",
        ));
    }
    let attrs = if attrs_part.is_empty() {
        Vec::new()
    } else {
        parse_attributes(&attrs_part)
            .ok_or_else(|| malformed(file_id, base, header, "malformed math attributes"))?
    };
    validate_attributes(&["id"], &attrs, header, file_id, base)?;
    let id = attrs.iter().find(|(k, _)| k == "id").map(|(_, v)| v.clone());

    let (payload, next_i) = parse_opaque_payload(source, lines, start + 1, header.indent + 1);
    let end = if next_i > start + 1 {
        lines[next_i - 1].byte_end
    } else {
        header.byte_end
    };
    let span = mk_span(file_id, base, header.content_byte_start, end);
    Ok((
        TopBlock::Equation {
            id,
            numbered: true,
            payload,
            span,
        },
        next_i,
    ))
}

/// `$$ ... $$` display math: unnumbered, no attributes. Closes on the
/// same line or on a later line containing `$$`; text between the
/// delimiters is the payload, with line breaks and indentation beyond the
/// block's own structural indent kept byte-for-byte, as `math:` does.
/// TeX ends display math at a paragraph break, so a blank line inside the
/// delimiters, or an empty payload, is rejected here at its line instead
/// of failing later in the engine with no source location.
fn parse_dollar_display<'a>(
    lines: &[StructLine<'a>],
    source: &str,
    start: usize,
    file_id: FileId,
    base: u32,
) -> Result<(TopBlock, usize), Diagnostic> {
    let header = &lines[start];
    let strip = (header.indent * 2) as usize;
    let after = &header.content[2..];

    if let Some(close) = after.find("$$") {
        if !after[close + 2..].trim().is_empty() {
            return Err(malformed(
                file_id,
                base,
                header,
                "unexpected text after the closing '$$'",
            ));
        }
        if after[..close].trim().is_empty() {
            return Err(malformed(file_id, base, header, "empty display math"));
        }
        let span = mk_span(file_id, base, header.content_byte_start, header.byte_end);
        let payload = after[..close].to_string();
        return Ok((
            TopBlock::Equation {
                id: None,
                numbered: false,
                payload,
                span,
            },
            start + 1,
        ));
    }

    let bytes = source.as_bytes();
    let line_end = |l: &StructLine<'_>| {
        if bytes.get(l.byte_end as usize) == Some(&b'\r') {
            "\r\n"
        } else {
            "\n"
        }
    };
    let mut parts: Vec<(&str, &str)> = Vec::new();
    if !after.trim().is_empty() {
        parts.push((after, line_end(header)));
    }
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        if !line.is_blank && line.indent < header.indent {
            break;
        }
        if line.is_blank {
            return Err(malformed(
                file_id,
                base,
                line,
                "blank line inside display math; close it with '$$' before the blank line",
            ));
        }
        let text = &source[line.byte_start as usize + strip..line.byte_end as usize];
        if let Some(close) = text.find("$$") {
            if !text[close + 2..].trim().is_empty() {
                return Err(malformed(file_id, base, line, "unexpected text after the closing '$$'"));
            }
            let before = &text[..close];
            if !before.trim().is_empty() {
                parts.push((before, ""));
            }
            if parts.is_empty() {
                return Err(malformed(file_id, base, header, "empty display math"));
            }
            let mut payload = String::new();
            for (idx, (t, eol)) in parts.iter().enumerate() {
                payload.push_str(t);
                if idx + 1 < parts.len() {
                    payload.push_str(eol);
                }
            }
            let span = mk_span(file_id, base, header.content_byte_start, line.byte_end);
            return Ok((
                TopBlock::Equation {
                    id: None,
                    numbered: false,
                    payload,
                    span,
                },
                i + 1,
            ));
        }
        parts.push((text, line_end(line)));
        i += 1;
    }
    Err(malformed(
        file_id,
        base,
        header,
        "unterminated display math: no closing '$$'",
    ))
}

/// Fenced code block: opens at a line [`crate::syntax::opaque::fence_len`]
/// recognizes (shared with the lexer so the two cannot disagree on what a
/// fence looks like). The first whitespace-delimited word of the trimmed
/// info string, if any, becomes the language tag, kept as written; the
/// rest of the info string is ignored. Closes at the first later line, at
/// the block's own structural indent, whose content -- after removing
/// that indent -- [`crate::syntax::opaque::fence_closes`] recognizes as a
/// closer. Content is never parsed as Terse and is kept byte for byte,
/// like `math:`/`tex:` payloads, including blank lines, trailing
/// whitespace, tabs, internal indentation, and original line endings. A
/// content line containing the rendering environment's end sequence
/// (`\end{TerseCode}`, tolerating spaces inside the braces) is rejected:
/// `listings` would close the block there and run the rest as live LaTeX.
fn parse_code_block<'a>(
    lines: &[StructLine<'a>],
    source: &str,
    start: usize,
    len: usize,
    file_id: FileId,
    base: u32,
) -> Result<(TopBlock, usize), Diagnostic> {
    let header = &lines[start];
    let info = header.content[len..].trim();
    let language = info.split_whitespace().next().map(|s| s.to_string());

    let strip = (header.indent * 2) as usize;
    let bytes = source.as_bytes();
    let line_end = |l: &StructLine<'_>| {
        if bytes.get(l.byte_end as usize) == Some(&b'\r') {
            "\r\n"
        } else {
            "\n"
        }
    };

    let mut parts: Vec<(&str, &str)> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        if !line.is_blank && line.indent < header.indent {
            break;
        }
        let text = if line.is_blank {
            // A whitespace-only line may be shorter than the prefix, or
            // start with a tab; strip only the prefix spaces it has.
            let raw = &source[line.byte_start as usize..line.byte_end as usize];
            let lead = raw.bytes().take_while(|&b| b == b' ').count();
            &raw[lead.min(strip)..]
        } else {
            &source[line.byte_start as usize + strip..line.byte_end as usize]
        };
        if !line.is_blank && crate::syntax::opaque::fence_closes(text, len) {
            let mut code = String::new();
            for (idx, (t, eol)) in parts.iter().enumerate() {
                code.push_str(t);
                if idx + 1 < parts.len() {
                    code.push_str(eol);
                }
            }
            let span = mk_span(file_id, base, header.content_byte_start, line.byte_end);
            return Ok((
                TopBlock::CodeBlock {
                    language,
                    code,
                    span,
                },
                i + 1,
            ));
        }
        if !line.is_blank && contains_forbidden_code_end_sequence(text) {
            return Err(malformed(
                file_id,
                base,
                line,
                "code block content cannot contain '\\end{TerseCode}'",
            ));
        }
        parts.push((text, line_end(line)));
        i += 1;
    }
    Err(malformed(
        file_id,
        base,
        header,
        "unterminated code block: no closing fence",
    ))
}

/// Detects `\end{TerseCode}` (tolerating spaces inside the braces)
/// anywhere in a code line -- not just at the start, since it is
/// dangerous mid-line too: `listings` closes its environment at the exact
/// sequence, and whatever follows on that line, and every line after it,
/// becomes live LaTeX rather than inert code content. Operates on raw
/// bytes: every marker byte is ASCII, and ASCII bytes never appear inside
/// a multi-byte UTF-8 sequence, so byte-level scanning never misaligns
/// with a `str`'s char boundaries.
fn contains_forbidden_code_end_sequence(text: &str) -> bool {
    let b = text.as_bytes();
    let mut i = 0usize;
    while i + 4 <= b.len() {
        if &b[i..i + 4] == b"\\end" {
            let mut j = i + 4;
            while j < b.len() && b[j] == b' ' {
                j += 1;
            }
            if j < b.len() && b[j] == b'{' {
                j += 1;
                while j < b.len() && b[j] == b' ' {
                    j += 1;
                }
                if j + 9 <= b.len() && &b[j..j + 9] == b"TerseCode" {
                    let mut k = j + 9;
                    while k < b.len() && b[k] == b' ' {
                        k += 1;
                    }
                    if k < b.len() && b[k] == b'}' {
                        return true;
                    }
                }
            }
        }
        i += 1;
    }
    false
}

/// `tex:` raw block: an opaque payload with the same extraction rules as
/// `math:`, ending at the next nonblank dedented line or EOF. No
/// attributes are supported.
fn parse_raw_tex<'a>(
    lines: &[StructLine<'a>],
    source: &str,
    start: usize,
    file_id: FileId,
    base: u32,
) -> Result<(TopBlock, usize), Diagnostic> {
    let header = &lines[start];
    if header.content.trim_end() != "tex:" {
        return Err(malformed(
            file_id,
            base,
            header,
            "malformed tex header, expected 'tex:'",
        ));
    }

    let (payload, next_i) = parse_opaque_payload(source, lines, start + 1, header.indent + 1);
    let end = if next_i > start + 1 {
        lines[next_i - 1].byte_end
    } else {
        header.byte_end
    };
    let span = mk_span(file_id, base, header.content_byte_start, end);
    Ok((TopBlock::RawTex { payload, span }, next_i))
}

/// Collects opaque payload lines from `start` while their indentation is at
/// least `indent`, terminating at the next nonblank dedented line or EOF.
/// Unlike ordinary block content, this reads bytes directly from `source`
/// (the decoded, BOM-stripped file text that `lines` was lexed from)
/// instead of the pre-trimmed `StructLine::content`, so each line's
/// original line-ending (LF or CRLF) and each whitespace-only line's exact
/// bytes are preserved rather than normalized. Only the required
/// structural indentation prefix (always literal ASCII spaces, per the
/// lexer's two-space rule) is stripped; deeper internal indentation is
/// real source bytes and needs no reconstruction.
fn parse_opaque_payload<'a>(
    source: &str,
    lines: &[StructLine<'a>],
    start: usize,
    indent: u32,
) -> (String, usize) {
    let bytes = source.as_bytes();
    let strip = (indent * 2) as usize;

    struct Seg {
        start: usize,
        end: usize,
        crlf: bool,
    }
    let mut segs: Vec<Seg> = Vec::new();
    let mut i = start;
    let mut last_nonblank = 0usize;

    while i < lines.len() {
        let line = &lines[i];
        if line.is_blank {
            segs.push(Seg {
                start: line.byte_start as usize,
                end: line.byte_end as usize,
                crlf: bytes.get(line.byte_end as usize) == Some(&b'\r'),
            });
            i += 1;
            continue;
        }
        if line.indent < indent {
            break;
        }
        segs.push(Seg {
            start: line.byte_start as usize + strip,
            end: line.byte_end as usize,
            crlf: bytes.get(line.byte_end as usize) == Some(&b'\r'),
        });
        last_nonblank = segs.len();
        i += 1;
    }

    segs.truncate(last_nonblank);

    let mut out = String::new();
    for (idx, seg) in segs.iter().enumerate() {
        out.push_str(&source[seg.start..seg.end]);
        if idx + 1 < segs.len() {
            out.push_str(if seg.crlf { "\r\n" } else { "\n" });
        }
    }
    (out, start + last_nonblank)
}

fn parse_figure<'a>(
    lines: &[StructLine<'a>],
    start: usize,
    file_id: FileId,
    base: u32,
) -> Result<(TopBlock, usize), Diagnostic> {
    let header = &lines[start];
    let rest = header
        .content
        .strip_prefix("figure")
        .expect("caller matched 'figure'");
    let rest = rest
        .strip_prefix(' ')
        .ok_or_else(|| malformed(file_id, base, header, "malformed figure header, expected a quoted path"))?;
    let (path, consumed) = parse_quoted(rest)
        .ok_or_else(|| malformed(file_id, base, header, "malformed figure header, expected a quoted path"))?;
    let after_path = rest[consumed..].trim_end();
    let (attrs_part, colon_ok) = if after_path == ":" {
        (String::new(), true)
    } else if let Some(stripped) = after_path.strip_suffix(':') {
        (stripped.trim().to_string(), true)
    } else {
        (String::new(), false)
    };
    if !colon_ok {
        return Err(malformed(
            file_id,
            base,
            header,
            "malformed figure header, expected ':' after the path/attributes",
        ));
    }
    let attrs = if attrs_part.is_empty() {
        Vec::new()
    } else {
        parse_attributes(&attrs_part)
            .ok_or_else(|| malformed(file_id, base, header, "malformed figure attributes"))?
    };
    validate_attributes(&["id", "role"], &attrs, header, file_id, base)?;
    let id = attrs.iter().find(|(k, _)| k == "id").map(|(_, v)| v.clone());
    let role = attrs.iter().find(|(k, _)| k == "role").map(|(_, v)| v.clone());
    if let Some(r) = &role {
        if r != "wide" {
            return Err(malformed(
                file_id,
                base,
                header,
                "unknown figure role; only 'wide' is supported",
            ));
        }
    }

    let body_indent = header.indent + 1;
    let mut i = start + 1;
    let mut caption: Option<(String, SourceSpan)> = None;
    let mut alt: Option<String> = None;
    let mut seen = HashSet::new();

    while i < lines.len() {
        let line = &lines[i];
        if line.is_blank {
            i += 1;
            continue;
        }
        if line.indent < body_indent {
            break;
        }
        if line.indent > body_indent {
            return Err(malformed(file_id, base, line, "unexpected indentation in figure fields"));
        }

        if let Some(rest) = line.content.strip_prefix("caption:") {
            require_unseen(&mut seen, "caption", line, file_id, base)?;
            let (text, next_i) = parse_field_text(lines, i, rest, body_indent, file_id, base)?;
            let span = mk_span(file_id, base, line.content_byte_start, lines[next_i - 1].byte_end);
            caption = Some((text, span));
            i = next_i;
        } else if let Some(rest) = line.content.strip_prefix("alt:") {
            require_unseen(&mut seen, "alt", line, file_id, base)?;
            let (text, next_i) = parse_field_text(lines, i, rest, body_indent, file_id, base)?;
            alt = Some(text);
            i = next_i;
        } else {
            return Err(malformed(file_id, base, line, "unsupported figure field"));
        }
    }

    let (caption_text, caption_span) = caption.ok_or_else(|| {
        Diagnostic::error(
            "E-META-010",
            "figure requires a nonempty 'caption'",
            mk_span(file_id, base, header.content_byte_start, header.byte_end),
        )
    })?;
    if caption_text.trim().is_empty() {
        return Err(Diagnostic::error("E-META-010", "figure 'caption' must be nonempty", caption_span));
    }
    let alt = alt.ok_or_else(|| {
        Diagnostic::error(
            "E-META-011",
            "figure requires a nonempty 'alt' text",
            mk_span(file_id, base, header.content_byte_start, header.byte_end),
        )
    })?;
    if alt.trim().is_empty() {
        return Err(Diagnostic::error("E-META-011", "figure 'alt' must be nonempty", caption_span));
    }

    let end = if i > start + 1 { lines[i - 1].byte_end } else { header.byte_end };
    let span = mk_span(file_id, base, header.content_byte_start, end);
    Ok((
        TopBlock::Figure {
            path,
            id,
            role,
            caption: caption_text,
            caption_span,
            alt,
            span,
        },
        i,
    ))
}

/// `field_text = scalar NL | NL INDENT paragraph DEDENT`
fn parse_field_text<'a>(
    lines: &[StructLine<'a>],
    line_idx: usize,
    rest: &str,
    field_indent: u32,
    file_id: FileId,
    base: u32,
) -> Result<(String, usize), Diagnostic> {
    let trimmed = rest.trim_start();
    if !trimmed.is_empty() {
        let text = parse_scalar(trimmed)
            .ok_or_else(|| malformed(file_id, base, &lines[line_idx], "expected a scalar value"))?;
        return Ok((text, line_idx + 1));
    }
    let body_indent = field_indent + 1;
    let mut i = line_idx + 1;
    while i < lines.len() && lines[i].is_blank {
        i += 1;
    }
    if i >= lines.len() || lines[i].indent != body_indent {
        return Err(malformed(
            file_id,
            base,
            &lines[line_idx],
            "expected an indented value on the next line",
        ));
    }
    Ok(consume_paragraph(lines, i, lines[i].content, body_indent))
}

fn parse_table<'a>(
    lines: &[StructLine<'a>],
    start: usize,
    file_id: FileId,
    base: u32,
) -> Result<(TopBlock, usize), Diagnostic> {
    let header = &lines[start];
    let rest = header.content.strip_prefix("table").expect("caller matched 'table'");
    let trimmed = rest.trim_end();
    let (attrs_part, colon_ok) = if trimmed == ":" {
        (String::new(), true)
    } else if rest.starts_with(" [") && trimmed.ends_with(':') {
        (trimmed[..trimmed.len() - 1].trim().to_string(), true)
    } else {
        (String::new(), false)
    };
    if !colon_ok {
        return Err(malformed(
            file_id,
            base,
            header,
            "malformed table header, expected 'table:' or 'table [attrs]:'",
        ));
    }
    let attrs = if attrs_part.is_empty() {
        Vec::new()
    } else {
        parse_attributes(&attrs_part)
            .ok_or_else(|| malformed(file_id, base, header, "malformed table attributes"))?
    };
    validate_attributes(&["id"], &attrs, header, file_id, base)?;
    let id = attrs.iter().find(|(k, _)| k == "id").map(|(_, v)| v.clone());

    let body_indent = header.indent + 1;
    let mut i = start + 1;
    let mut caption: Option<(String, SourceSpan)> = None;
    let mut table_header: Option<TableCells> = None;
    let mut align: Option<Vec<ColumnAlign>> = None;
    let mut rows: Vec<TableCells> = Vec::new();
    let mut seen = HashSet::new();
    // Distinguishes the field form (`header:`/`rows:`) from a pipe body:
    // once one appears, the other is a located mixing error, and a
    // second pipe-table opener (which would silently discard the first)
    // is rejected the same way `header:`/`rows:` already reject a repeat.
    let mut body_kind: Option<&'static str> = None;

    while i < lines.len() {
        let line = &lines[i];
        if line.is_blank {
            i += 1;
            continue;
        }
        if line.indent < body_indent {
            break;
        }
        if line.indent > body_indent {
            return Err(malformed(file_id, base, line, "unexpected indentation in table fields"));
        }

        if let Some(rest) = line.content.strip_prefix("caption:") {
            require_unseen(&mut seen, "caption", line, file_id, base)?;
            let (text, next_i) = parse_field_text(lines, i, rest, body_indent, file_id, base)?;
            let span = mk_span(
                file_id,
                base,
                line.content_byte_start,
                lines[next_i - 1].byte_end,
            );
            caption = Some((text, span));
            i = next_i;
        } else if let Some(rest) = line.content.strip_prefix("header:") {
            if let Some(other) = body_kind {
                if other == "pipe" {
                    return Err(malformed(
                        file_id,
                        base,
                        line,
                        "a table body cannot mix pipe-table lines with 'header:'/'rows:' fields",
                    ));
                }
            }
            body_kind = Some("field");
            require_unseen(&mut seen, "header", line, file_id, base)?;
            let cells = parse_string_list(rest.trim_start()).ok_or_else(|| {
                malformed(file_id, base, line, "expected a list value for 'header'")
            })?;
            let span = mk_span(file_id, base, line.content_byte_start, line.byte_end);
            table_header = Some((cells, span));
            i += 1;
        } else if line.content.trim_end() == "rows:" {
            if let Some(other) = body_kind {
                if other == "pipe" {
                    return Err(malformed(
                        file_id,
                        base,
                        line,
                        "a table body cannot mix pipe-table lines with 'header:'/'rows:' fields",
                    ));
                }
            }
            body_kind = Some("field");
            require_unseen(&mut seen, "rows", line, file_id, base)?;
            let (parsed_rows, next_i) = parse_table_rows(lines, i, file_id, base)?;
            rows = parsed_rows;
            i = next_i;
        } else if line.content.starts_with('|') {
            if body_kind == Some("field") {
                return Err(malformed(
                    file_id,
                    base,
                    line,
                    "a table body cannot mix pipe-table lines with 'header:'/'rows:' fields",
                ));
            }
            if body_kind == Some("pipe") {
                return Err(malformed(file_id, base, line, "a table body can only have one pipe table"));
            }
            body_kind = Some("pipe");
            let (parsed_header, parsed_align, parsed_rows, next_i) = parse_pipe_table_body(lines, i, file_id, base)?;
            table_header = Some(parsed_header);
            align = Some(parsed_align);
            rows = parsed_rows;
            i = next_i;
        } else {
            return Err(malformed(file_id, base, line, "unsupported table field"));
        }
    }

    let table_header = table_header.ok_or_else(|| {
        Diagnostic::error(
            "E-META-013",
            "table requires a 'header'",
            mk_span(file_id, base, header.content_byte_start, header.byte_end),
        )
    })?;
    let align = align.unwrap_or_else(|| vec![ColumnAlign::Default; table_header.0.len()]);
    let error_span = mk_span(file_id, base, header.content_byte_start, header.byte_end);
    let end = if i > start + 1 {
        lines[i - 1].byte_end
    } else {
        header.byte_end
    };
    let span = mk_span(file_id, base, header.content_byte_start, end);
    Ok((finish_table(id, caption, align, table_header, rows, error_span, span)?, i))
}

/// Shared row-shape validation and construction: rejects a table with no
/// data row (`E-META-014`) or an overlong row (`E-META-015`, since
/// dropping a cell to fit would lose authored content, but a short row
/// padded with empty cells does not), and rejects an id with no caption
/// (`E-META-017`). Used by the field form, a bare pipe table, and a
/// `table [id: ...]:` block's pipe body, so all three enforce the same
/// shape. `error_span` locates the table-level errors (`E-META-014`,
/// `E-META-017`); an overlong row's error uses that row's own span.
fn finish_table(
    id: Option<String>,
    caption: Option<(String, SourceSpan)>,
    align: Vec<ColumnAlign>,
    header: TableCells,
    mut rows: Vec<TableCells>,
    error_span: SourceSpan,
    span: SourceSpan,
) -> Result<TopBlock, Diagnostic> {
    if rows.is_empty() {
        return Err(Diagnostic::error(
            "E-META-014",
            "table requires at least one data row",
            error_span,
        ));
    }
    for (row, row_span) in &mut rows {
        if row.len() > header.0.len() {
            return Err(Diagnostic::error(
                "E-META-015",
                format!(
                    "table row has {} cells but the header has {}",
                    row.len(),
                    header.0.len()
                ),
                *row_span,
            ));
        }
        while row.len() < header.0.len() {
            row.push(String::new());
        }
    }
    if id.is_some() && caption.is_none() {
        return Err(Diagnostic::error(
            "E-META-017",
            "a table id needs a caption: an unnumbered table has nothing to reference",
            error_span,
        ));
    }
    Ok(TopBlock::Table {
        id,
        caption,
        align,
        header,
        rows,
        span,
    })
}

/// Whether `lines[i]` opens a pipe table: its content starts with `|`,
/// and the next line, at the same indent, is a valid delimiter row. A
/// `|` line not followed by a delimiter row stays prose (D1); a header/
/// delimiter cell-count mismatch is a hard error reported once parsing
/// actually commits to the table, not a reason to fall back to prose.
fn pipe_table_opens(lines: &[StructLine<'_>], i: usize) -> bool {
    let Some(header) = lines.get(i) else {
        return false;
    };
    if header.is_blank || !header.content.starts_with('|') {
        return false;
    }
    let Some(delim) = lines.get(i + 1) else {
        return false;
    };
    !delim.is_blank && delim.indent == header.indent && parse_delimiter_row(delim.content).is_some()
}

/// Recognizes a pipe-table delimiter row: optional leading and trailing
/// `|`, cells made only of an optional `:`, one or more `-`, and an
/// optional `:`, with surrounding spaces allowed. Returns each cell's
/// alignment in order, or `None` if the line is not a delimiter row at
/// all (as opposed to one with the wrong cell count for its header,
/// which the caller reports as a located error rather than treating as
/// "not a delimiter row").
fn parse_delimiter_row(content: &str) -> Option<Vec<ColumnAlign>> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    if inner.trim().is_empty() {
        return None;
    }
    let mut aligns = Vec::new();
    for cell in inner.split('|') {
        let cell = cell.trim();
        if cell.is_empty() {
            return None;
        }
        let left = cell.starts_with(':');
        let right = cell.ends_with(':');
        let dashes = cell.trim_start_matches(':').trim_end_matches(':');
        if dashes.is_empty() || !dashes.chars().all(|c| c == '-') {
            return None;
        }
        aligns.push(match (left, right) {
            (true, true) => ColumnAlign::Center,
            (true, false) => ColumnAlign::Left,
            (false, true) => ColumnAlign::Right,
            (false, false) => ColumnAlign::Default,
        });
    }
    Some(aligns)
}

/// Splits a pipe-table row's structural content into raw (not yet
/// inline-parsed) cell text, protecting three span kinds shared with the
/// inline parser so the two can never disagree (D2): a backtick-delimited
/// code span, `$...$` math via the exact rule [`crate::syntax::inlines::
/// find_dollar_closer`] uses, and `\(...\)` math. Outside a protected
/// span, `\|` becomes a literal `|` and does not split; inside a code
/// span, `\|` also becomes `|` (GFM); inside math, `\|` is left exactly
/// as written, where it is TeX's double bar. An unmatched protected span
/// (a lone backtick, an unterminated `$` or `\(`) protects nothing past
/// its own opener and is split normally. One leading and one trailing
/// unprotected `|` (the table syntax's own delimiters, not part of any
/// cell) are dropped; an interior empty cell, such as the second of
/// `| a | |`, is kept. Every returned cell is trimmed.
fn split_pipe_row(content: &str) -> Vec<String> {
    let trimmed = content.trim();
    let chars: Vec<char> = trimmed.chars().collect();
    let n = chars.len();
    let mut pieces: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut i = 0usize;
    while i < n {
        match chars[i] {
            '\\' if i + 1 < n && chars[i + 1] == '|' => {
                buf.push('|');
                i += 2;
            }
            '`' => {
                let run_start = i;
                let mut len = 0usize;
                while i < n && chars[i] == '`' {
                    len += 1;
                    i += 1;
                }
                let content_start = i;
                let mut close = None;
                let mut j = i;
                while j < n {
                    if chars[j] == '`' {
                        let run_j = j;
                        let mut rlen = 0usize;
                        while j < n && chars[j] == '`' {
                            rlen += 1;
                            j += 1;
                        }
                        if rlen == len {
                            close = Some((run_j, j));
                            break;
                        }
                    } else {
                        j += 1;
                    }
                }
                match close {
                    Some((close_start, close_end)) => {
                        buf.extend(chars[run_start..content_start].iter().copied());
                        let mut k = content_start;
                        while k < close_start {
                            if chars[k] == '\\' && k + 1 < close_start && chars[k + 1] == '|' {
                                buf.push('|');
                                k += 2;
                            } else {
                                buf.push(chars[k]);
                                k += 1;
                            }
                        }
                        buf.extend(chars[close_start..close_end].iter().copied());
                        i = close_end;
                    }
                    None => {
                        // No matching run: the backtick(s) protect nothing.
                        buf.extend(chars[run_start..content_start].iter().copied());
                        i = content_start;
                    }
                }
            }
            '$' => match crate::syntax::inlines::find_dollar_closer(&chars, i) {
                Some(close) => {
                    buf.extend(chars[i..=close].iter().copied());
                    i = close + 1;
                }
                None => {
                    buf.push('$');
                    i += 1;
                }
            },
            '\\' if i + 1 < n && chars[i + 1] == '(' => {
                let math_start = i;
                let mut j = i + 2;
                let mut close = None;
                while j + 1 < n {
                    if chars[j] == '\\' && chars[j + 1] == ')' {
                        close = Some(j + 1);
                        break;
                    }
                    j += 1;
                }
                match close {
                    Some(close_end) => {
                        buf.extend(chars[math_start..=close_end].iter().copied());
                        i = close_end + 1;
                    }
                    None => {
                        buf.push(chars[i]);
                        i += 1;
                    }
                }
            }
            '|' => {
                pieces.push(std::mem::take(&mut buf));
                i += 1;
            }
            c => {
                buf.push(c);
                i += 1;
            }
        }
    }
    pieces.push(buf);

    if pieces.len() > 1 && trimmed.starts_with('|') && pieces[0].trim().is_empty() {
        pieces.remove(0);
    }
    if pieces.len() > 1
        && trimmed.ends_with('|')
        && pieces.last().is_some_and(|p| p.trim().is_empty())
    {
        pieces.pop();
    }
    pieces.into_iter().map(|p| p.trim().to_string()).collect()
}

/// Parses a pipe table's header, delimiter, and body rows starting at
/// `lines[start]` (the caller has already confirmed, via
/// [`pipe_table_opens`], that this line opens one). Continues while
/// later lines are non-blank, at the same indent as the header, and
/// start with `|`.
fn parse_pipe_table_body<'a>(
    lines: &[StructLine<'a>],
    start: usize,
    file_id: FileId,
    base: u32,
) -> Result<(TableCells, Vec<ColumnAlign>, Vec<TableCells>, usize), Diagnostic> {
    let header_line = &lines[start];
    let indent = header_line.indent;
    let header_cells = split_pipe_row(header_line.content);
    let header_span = mk_span(
        file_id,
        base,
        header_line.content_byte_start,
        header_line.byte_end,
    );

    let delim_line = &lines[start + 1];
    let align =
        parse_delimiter_row(delim_line.content).expect("caller confirmed the next line is a delimiter row");
    if align.len() != header_cells.len() {
        return Err(Diagnostic::error(
            "E-PARSE-002",
            format!(
                "pipe-table delimiter row has {} cells but the header has {}",
                align.len(),
                header_cells.len()
            ),
            mk_span(
                file_id,
                base,
                delim_line.content_byte_start,
                delim_line.byte_end,
            ),
        ));
    }

    let mut i = start + 2;
    let mut rows = Vec::new();
    while i < lines.len() {
        let line = &lines[i];
        if line.is_blank || line.indent != indent || !line.content.starts_with('|') {
            break;
        }
        let cells = split_pipe_row(line.content);
        let span = mk_span(file_id, base, line.content_byte_start, line.byte_end);
        rows.push((cells, span));
        i += 1;
    }

    Ok(((header_cells, header_span), align, rows, i))
}

/// A bare (captionless) pipe table at module/nested scope: `id` and
/// `caption` are always `None`, since a table with no `table [id: ...]:`
/// header has no id to carry.
fn parse_pipe_table<'a>(
    lines: &[StructLine<'a>],
    start: usize,
    file_id: FileId,
    base: u32,
) -> Result<(TopBlock, usize), Diagnostic> {
    let (header, align, rows, next_i) = parse_pipe_table_body(lines, start, file_id, base)?;
    let header_line = &lines[start];
    let error_span = mk_span(
        file_id,
        base,
        header_line.content_byte_start,
        header_line.byte_end,
    );
    let end = lines[next_i - 1].byte_end;
    let span = mk_span(file_id, base, header_line.content_byte_start, end);
    Ok((finish_table(None, None, align, header, rows, error_span, span)?, next_i))
}

fn parse_table_rows<'a>(
    lines: &[StructLine<'a>],
    start: usize,
    file_id: FileId,
    base: u32,
) -> Result<(Vec<TableCells>, usize), Diagnostic> {
    let header_line = &lines[start];
    let body_indent = header_line.indent + 1;
    let mut i = start + 1;
    let mut rows = Vec::new();
    while i < lines.len() {
        let line = &lines[i];
        if line.is_blank {
            i += 1;
            continue;
        }
        if line.indent < body_indent {
            break;
        }
        if line.indent > body_indent {
            return Err(malformed(file_id, base, line, "unexpected indentation in table rows"));
        }
        let rest = line
            .content
            .strip_prefix("- ")
            .ok_or_else(|| malformed(file_id, base, line, "expected '- [cell, cell, ...]' for each row"))?;
        let cells = parse_string_list(rest)
            .ok_or_else(|| malformed(file_id, base, line, "expected a bracketed cell list"))?;
        if cells.is_empty() {
            return Err(malformed(file_id, base, line, "table rows cannot be empty"));
        }
        let span = mk_span(file_id, base, line.content_byte_start, line.byte_end);
        rows.push((cells, span));
        i += 1;
    }
    Ok((rows, i))
}

fn parse_theorem_like<'a>(
    lines: &[StructLine<'a>],
    source: &str,
    start: usize,
    kind_word: &str,
    file_id: FileId,
    base: u32,
) -> Result<(TopBlock, usize), Diagnostic> {
    let header = &lines[start];
    let rest = header
        .content
        .strip_prefix(kind_word)
        .expect("caller matched a theorem-like keyword");
    let trimmed = rest.trim_end();
    let body_after = trimmed.strip_suffix(':').ok_or_else(|| {
        malformed(file_id, base, header, "malformed theorem-like header, expected a trailing ':'")
    })?;
    let (title_part, attrs) = split_trailing_attributes(body_after.trim());
    let attrs = attrs.unwrap_or_default();
    validate_attributes(&["id"], &attrs, header, file_id, base)?;
    let id = attrs.iter().find(|(k, _)| k == "id").map(|(_, v)| v.clone());
    let title = {
        let t = title_part.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    };

    let kind = match kind_word {
        "theorem" => TheoremKind::Theorem,
        "proposition" => TheoremKind::Proposition,
        "lemma" => TheoremKind::Lemma,
        "definition" => TheoremKind::Definition,
        "example" => TheoremKind::Example,
        "remark" => TheoremKind::Remark,
        _ => unreachable!("caller only passes theorem-like keywords"),
    };

    let body_indent = header.indent + 1;
    let (body, next_i) = parse_block_sequence(
        lines,
        source,
        start + 1,
        body_indent,
        BlockContext::Nested,
        file_id,
        base,
    )?;

    let end = if next_i > start + 1 { lines[next_i - 1].byte_end } else { header.byte_end };
    let span = mk_span(file_id, base, header.content_byte_start, end);
    Ok((
        TopBlock::TheoremLike {
            kind,
            title,
            id,
            body,
            span,
        },
        next_i,
    ))
}

fn parse_proof<'a>(
    lines: &[StructLine<'a>],
    source: &str,
    start: usize,
    file_id: FileId,
    base: u32,
) -> Result<(TopBlock, usize), Diagnostic> {
    let header = &lines[start];
    let rest = header.content.strip_prefix("proof").expect("caller matched 'proof'");
    let trimmed = rest.trim_end();
    let body_after = trimmed
        .strip_suffix(':')
        .ok_or_else(|| malformed(file_id, base, header, "malformed proof header, expected a trailing ':'"))?;
    let attrs_str = body_after.trim();
    let attrs = if attrs_str.is_empty() {
        Vec::new()
    } else {
        parse_attributes(attrs_str).ok_or_else(|| malformed(file_id, base, header, "malformed proof attributes"))?
    };
    validate_attributes(&["id", "of"], &attrs, header, file_id, base)?;
    let id = attrs.iter().find(|(k, _)| k == "id").map(|(_, v)| v.clone());
    let of = attrs.iter().find(|(k, _)| k == "of").map(|(_, v)| v.clone());

    let body_indent = header.indent + 1;
    let (body, next_i) = parse_block_sequence(
        lines,
        source,
        start + 1,
        body_indent,
        BlockContext::Nested,
        file_id,
        base,
    )?;

    let end = if next_i > start + 1 { lines[next_i - 1].byte_end } else { header.byte_end };
    let span = mk_span(file_id, base, header.content_byte_start, end);
    Ok((TopBlock::Proof { id, of, body, span }, next_i))
}

/// Splits a trailing `[key: value, ...]` attribute list off the end of an
/// already-collected text span, if the trailing bracketed text
/// unambiguously parses as attributes (not a citation group or link).
fn split_trailing_attributes(content: &str) -> (&str, Option<Vec<(String, String)>>) {
    let trimmed = content.trim_end();
    if !trimmed.ends_with(']') {
        return (content, None);
    }
    if let Some(open) = trimmed.rfind('[') {
        let candidate = &trimmed[open..];
        if !candidate.starts_with("[@") {
            if let Some(attrs) = parse_attributes(candidate) {
                return (trimmed[..open].trim_end(), Some(attrs));
            }
        }
    }
    (content, None)
}

fn validate_attributes(
    allowed: &[&str],
    attrs: &[(String, String)],
    line: &StructLine<'_>,
    file_id: FileId,
    base: u32,
) -> Result<(), Diagnostic> {
    let mut seen = HashSet::new();
    for (key, _) in attrs {
        if !allowed.contains(&key.as_str()) {
            return Err(Diagnostic::error(
                "E-ATTR-001",
                format!("'{key}' is not a valid content attribute here"),
                mk_span(file_id, base, line.content_byte_start, line.byte_end),
            )
            .with_help("presentation settings (width, font, color, margin, placement) belong in a theme file, not content attributes"));
        }
        if !seen.insert(key.clone()) {
            return Err(malformed(file_id, base, line, "duplicate attribute"));
        }
    }
    Ok(())
}

fn parse_attributes(s: &str) -> Option<Vec<(String, String)>> {
    let s = s.trim();
    let inner = s.strip_prefix('[')?.strip_suffix(']')?;
    let inner = inner.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    for part in split_top_level_commas(inner) {
        let part = part.trim();
        let colon = part.find(':')?;
        let key = part[..colon].trim().to_string();
        let value = part[colon + 1..].trim().to_string();
        if key.is_empty() || value.is_empty() {
            return None;
        }
        out.push((key, value));
    }
    Some(out)
}

fn parse_scalar(s: &str) -> Option<String> {
    let s = s.trim_end();
    if s.is_empty() {
        return None;
    }
    if s.starts_with('"') {
        parse_quoted(s).map(|(v, _)| v)
    } else {
        Some(s.to_string())
    }
}

fn parse_string_list(s: &str) -> Option<Vec<String>> {
    let s = s.trim_end();
    let inner = s.strip_prefix('[')?.strip_suffix(']')?;
    let inner = inner.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    for part in split_top_level_commas(inner) {
        out.push(parse_scalar(part.trim())?);
    }
    Some(out)
}

fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut in_quotes = false;
    let mut start = 0usize;
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => in_quotes = !in_quotes,
            b'\\' if in_quotes => i += 1,
            b',' if !in_quotes => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(&s[start..]);
    parts
}

/// Parses a JSON-escaped double-quoted string starting at byte 0 of `s`.
/// Returns the decoded value and the byte length consumed (including both
/// quote characters).
fn parse_quoted(s: &str) -> Option<(String, usize)> {
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    if chars.is_empty() || chars[0].1 != '"' {
        return None;
    }
    let mut out = String::new();
    let mut idx = 1;
    while idx < chars.len() {
        let (byte_pos, c) = chars[idx];
        match c {
            '"' => {
                let end_byte = byte_pos + c.len_utf8();
                return Some((out, end_byte));
            }
            '\\' => {
                idx += 1;
                let (_, esc) = *chars.get(idx)?;
                match esc {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    _ => return None,
                }
            }
            other => out.push(other),
        }
        idx += 1;
    }
    None
}

fn malformed(file_id: FileId, base: u32, line: &StructLine<'_>, msg: &str) -> Diagnostic {
    Diagnostic::error(
        "E-PARSE-002",
        msg,
        mk_span(file_id, base, line.content_byte_start, line.byte_end),
    )
}

fn mk_span(file_id: FileId, base: u32, start: u32, end: u32) -> SourceSpan {
    SourceSpan::new(file_id, base + start, base + end)
}

#[cfg(test)]
mod pipe_row_splitter_tests {
    use super::split_pipe_row;

    #[test]
    fn splits_on_unprotected_pipes_dropping_edge_pipes() {
        assert_eq!(split_pipe_row("| a | b |"), vec!["a", "b"]);
        assert_eq!(split_pipe_row("a | b"), vec!["a", "b"]);
        // An interior empty cell (not the edge markers) is kept.
        assert_eq!(split_pipe_row("| a | |"), vec!["a", ""]);
    }

    #[test]
    fn escaped_pipe_outside_any_span_is_literal() {
        assert_eq!(split_pipe_row(r"| a \| b | c |"), vec!["a | b", "c"]);
    }

    #[test]
    fn code_span_protects_its_pipe_and_unescapes_backslash_pipe() {
        assert_eq!(split_pipe_row("| `a|b` |"), vec!["`a|b`"]);
        assert_eq!(split_pipe_row(r"| `a\|b` |"), vec!["`a|b`"]);
    }

    #[test]
    fn dollar_math_protects_its_pipe_and_keeps_escaped_bar_as_written() {
        assert_eq!(split_pipe_row("| $|x|$ | y |"), vec!["$|x|$", "y"]);
        assert_eq!(split_pipe_row(r"| $\|x\|$ |"), vec![r"$\|x\|$"]);
    }

    #[test]
    fn paren_math_protects_its_pipe() {
        assert_eq!(split_pipe_row(r"| \(a|b\) |"), vec![r"\(a|b\)"]);
    }

    #[test]
    fn unmatched_protected_span_protects_nothing_and_splits_normally() {
        // A lone backtick has no closing run anywhere in the row: it does
        // not swallow the rest of the row as a code span. The `|` after
        // it still splits, giving two cells, not one.
        assert_eq!(split_pipe_row("| ` | x |"), vec!["`", "x"]);
        // Likewise an unterminated `$` (no valid closer) and an
        // unterminated `\(` (no `\)`).
        assert_eq!(split_pipe_row("| $5 | $10 |"), vec!["$5", "$10"]);
        assert_eq!(split_pipe_row(r"| \(a | b |"), vec![r"\(a", "b"]);
    }
}
