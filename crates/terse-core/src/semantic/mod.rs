//! Lowering from the syntax layer's blocks into typed semantic nodes:
//! inline parsing, document metadata validation (locale support, unique
//! fields), reference declaration collection, and the complete MVP block
//! model (lists, equations, figures, tables, theorem-like/proof bodies).

use crate::diagnostic::Diagnostic;
use crate::source::{FileId, SourceSpan};
use crate::syntax::blocks::{RawAffiliation, RawListItem, RefEntry, TopBlock};
use crate::syntax::inlines::{self, Inline};

pub use crate::syntax::blocks::{ColumnAlign, RefKind, TheoremKind};
pub use crate::syntax::inlines::{Citation, CiteItem, Locator, LocatorKind};

const SUPPORTED_LOCALES: &[&str] = &["en", "pt-BR"];
const DEFAULT_LOCALE: &str = "en";

#[derive(Debug, Clone, PartialEq)]
pub enum Affiliation {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Author {
    pub name: String,
    pub affiliation: Option<Affiliation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentMetadata {
    pub title: String,
    pub subtitle: Option<String>,
    pub authors: Vec<Author>,
    pub affiliations: Vec<String>,
    pub date: Option<String>,
    pub language: String,
    pub abstract_blocks: Vec<Vec<Inline>>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceDeclaration {
    pub alias: String,
    pub kind: RefKind,
    pub identifier: String,
}

#[derive(Debug, Clone)]
pub struct ParsedModule {
    pub file_id: FileId,
    pub metadata: Option<DocumentMetadata>,
    pub references: Vec<ReferenceDeclaration>,
    pub blocks: Vec<Node>,
}

impl PartialEq for ParsedModule {
    /// Semantic-content equality: two modules with the same authored
    /// content and order are equal regardless of source encoding (line
    /// endings, BOM) or the resulting byte spans.
    fn eq(&self, other: &Self) -> bool {
        self.metadata == other.metadata && self.references == other.references && self.blocks == other.blocks
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub span: SourceSpan,
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    pub inlines: Vec<Inline>,
    pub continuation: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    Heading {
        level: u8,
        id: Option<String>,
        inlines: Vec<Inline>,
    },
    Paragraph {
        inlines: Vec<Inline>,
    },
    List {
        ordered: bool,
        start: Option<u32>,
        items: Vec<ListItem>,
    },
    /// A display equation. The payload is emitted unchanged; validation
    /// against the closed TeX math subset is added in task group 8.
    /// `math:` blocks are numbered; `$$` displays are not.
    Equation {
        id: Option<String>,
        numbered: bool,
        payload: String,
    },
    Figure {
        path: String,
        id: Option<String>,
        role: Option<String>,
        caption: Vec<Inline>,
        alt: String,
    },
    Table {
        id: Option<String>,
        caption: Option<Vec<Inline>>,
        align: Vec<ColumnAlign>,
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    TheoremLike {
        kind: TheoremKind,
        title: Option<String>,
        id: Option<String>,
        body: Vec<Node>,
    },
    Proof {
        id: Option<String>,
        /// The theorem-like `id` this proof is explicitly of, if declared.
        /// Never inferred from adjacency.
        of: Option<String>,
        body: Vec<Node>,
    },
    /// An explicit `tex:` raw escape hatch. The payload is opaque to Terse
    /// (never validated against the math subset or any other schema) and
    /// is emitted byte-for-byte. Strict checking reports `W-TEX-001` for
    /// every occurrence via [`collect_warnings`].
    RawTex {
        payload: String,
    },
    /// A fenced code block. `language` is the fence's info-string tag as
    /// written (rendering maps it to a `listings` language, or omits the
    /// option for an unknown or absent tag); `code` is opaque, emitted
    /// byte-for-byte, never parsed as Terse or escaped. Carries no ID.
    CodeBlock {
        language: Option<String>,
        code: String,
    },
    /// The rendered bibliography: either the author's explicit `bibliography`
    /// marker (preserving its authored position) or a derived one appended
    /// when citations exist but no marker was declared. At most one ever
    /// exists per module (enforced project-wide before lowering).
    Bibliography,
}

/// Lowers a block sequence (already expanded/flattened for a multi-file
/// project, or a single module's own blocks). Validates cross-references
/// and proof targets, and appends a derived bibliography marker when
/// citations exist but no explicit marker was authored. Does not validate
/// citation aliases against declared references — that project-wide check
/// is [`validate_citations`], run separately by the application entrypoint
/// once the project-wide alias namespace is known.
pub fn lower(blocks: Vec<TopBlock>, file_id: FileId) -> Result<ParsedModule, Diagnostic> {
    let module = lower_impl(blocks, file_id, true)?;
    check_node_budget(&module)?;
    Ok(module)
}

/// The largest number of semantic nodes one compiled document may contain,
/// counting nested list continuations and theorem/proof bodies. The
/// complete acceptance fixture is well under a hundred; a long book with
/// every section included is thousands. 100_000 leaves that headroom
/// intact while bounding the work every later stage (projection, LaTeX
/// emission, source mapping) performs per node, each of which walks this
/// tree at least once.
pub const MAX_NODES: usize = 100_000;

fn count_nodes(nodes: &[Node]) -> usize {
    nodes
        .iter()
        .map(|node| {
            1 + match &node.kind {
                NodeKind::List { items, .. } => items
                    .iter()
                    .map(|item| count_nodes(&item.continuation))
                    .sum::<usize>(),
                NodeKind::TheoremLike { body, .. } | NodeKind::Proof { body, .. } => count_nodes(body),
                _ => 0,
            }
        })
        .sum()
}

/// Refuses a document whose node count exceeds [`MAX_NODES`], before any
/// artifact plan exists. Counted after lowering rather than during it so
/// the limit applies to the expanded, project-wide tree an include graph
/// can multiply, not to any single module in isolation.
fn check_node_budget(module: &ParsedModule) -> Result<(), Diagnostic> {
    let total = count_nodes(&module.blocks);
    if total > MAX_NODES {
        return Err(Diagnostic::error(
            "E-LIMIT-004",
            format!("document has {total} semantic nodes, exceeding the {MAX_NODES}-node limit"),
            SourceSpan::new(module.file_id, 0, 0),
        ));
    }
    Ok(())
}

/// Lowers a *single, unexpanded* module without validating cross-reference
/// ids against its own symbol table. A standalone file's cross-references
/// may legitimately target ids defined in a sibling module that only
/// becomes visible after project-wide include expansion
/// (`project::expand`) — validating them here, before that expansion, was
/// a latent bug (`fmt` on any file with a valid cross-file forward
/// reference failed with a spurious `E-XREF-001`). Math/raw-execution and
/// proof-target checks remain single-file-safe and still run.
pub fn lower_single_file_unchecked_refs(
    blocks: Vec<TopBlock>,
    file_id: FileId,
) -> Result<ParsedModule, Diagnostic> {
    lower_impl(blocks, file_id, false)
}

fn lower_impl(blocks: Vec<TopBlock>, file_id: FileId, check_xrefs: bool) -> Result<ParsedModule, Diagnostic> {
    let mut metadata = None;
    let mut references = Vec::new();
    let mut body_blocks = Vec::new();

    for block in blocks {
        match block {
            TopBlock::Document { metadata: raw } => {
                metadata = Some(lower_metadata(raw)?);
            }
            TopBlock::Refs { entries, .. } => {
                references.extend(lower_refs(entries));
            }
            // Dependency tracking for includes is introduced with module
            // resolution (task group 11).
            TopBlock::Include { .. } => {}
            other => body_blocks.push(other),
        }
    }

    let mut nodes = lower_blocks(body_blocks)?;

    if check_xrefs {
        let mut symbols = SymbolTable::new();
        collect_symbols(&nodes, &mut symbols);
        validate_cross_refs(&nodes, &symbols)?;
    }
    validate_proof_targets(&nodes)?;

    if !nodes.iter().any(|n| matches!(n.kind, NodeKind::Bibliography))
        && contains_citation(&nodes)
    {
        nodes.push(Node {
            kind: NodeKind::Bibliography,
            span: SourceSpan::new(file_id, 0, 0),
        });
    }

    Ok(ParsedModule {
        file_id,
        metadata,
        references,
        blocks: nodes,
    })
}

fn contains_citation(nodes: &[Node]) -> bool {
    nodes.iter().any(|node| match &node.kind {
        NodeKind::Heading { inlines, .. } | NodeKind::Paragraph { inlines } => inlines_contain_citation(inlines),
        NodeKind::Figure { caption, .. } => inlines_contain_citation(caption),
        NodeKind::List { items, .. } => items
            .iter()
            .any(|item| inlines_contain_citation(&item.inlines) || contains_citation(&item.continuation)),
        NodeKind::TheoremLike { body, .. } | NodeKind::Proof { body, .. } => contains_citation(body),
        NodeKind::Table { caption, header, rows, .. } => {
            caption.as_deref().is_some_and(inlines_contain_citation)
                || header.iter().any(|cell| inlines_contain_citation(cell))
                || rows.iter().any(|row| row.iter().any(|cell| inlines_contain_citation(cell)))
        }
        NodeKind::Equation { .. } | NodeKind::RawTex { .. } | NodeKind::CodeBlock { .. } | NodeKind::Bibliography => {
            false
        }
    })
}

fn inlines_contain_citation(inlines: &[Inline]) -> bool {
    inlines.iter().any(|inline| match inline {
        Inline::Citation(_) => true,
        Inline::Emphasis(v) | Inline::Strong(v) | Inline::Footnote(v) => inlines_contain_citation(v),
        Inline::Link { label, .. } => inlines_contain_citation(label),
        Inline::Text(_) | Inline::Code(_) | Inline::Math(_) | Inline::CrossRef(_) => false,
    })
}

/// Target-kind map for `proof [of: ...]` validation: every declared id
/// paired with whether it names a theorem-like block (the only kind a
/// proof may explicitly target).
fn collect_theorem_targets(nodes: &[Node], out: &mut std::collections::HashMap<String, bool>) {
    for node in nodes {
        match &node.kind {
            NodeKind::TheoremLike { id, body, .. } => {
                if let Some(id) = id {
                    out.insert(id.clone(), true);
                }
                collect_theorem_targets(body, out);
            }
            NodeKind::Proof { id, body, .. } => {
                if let Some(id) = id {
                    out.insert(id.clone(), false);
                }
                collect_theorem_targets(body, out);
            }
            NodeKind::Heading { id, .. } => {
                if let Some(id) = id {
                    out.entry(id.clone()).or_insert(false);
                }
            }
            NodeKind::Equation { id, .. } | NodeKind::Figure { id, .. } | NodeKind::Table { id, .. } => {
                if let Some(id) = id {
                    out.entry(id.clone()).or_insert(false);
                }
            }
            NodeKind::List { items, .. } => {
                for item in items {
                    collect_theorem_targets(&item.continuation, out);
                }
            }
            NodeKind::Paragraph { .. }
            | NodeKind::RawTex { .. }
            | NodeKind::CodeBlock { .. }
            | NodeKind::Bibliography => {}
        }
    }
}

fn validate_proof_targets(nodes: &[Node]) -> Result<(), Diagnostic> {
    let mut targets = std::collections::HashMap::new();
    collect_theorem_targets(nodes, &mut targets);
    walk_proofs(nodes, &targets)
}

fn walk_proofs(nodes: &[Node], targets: &std::collections::HashMap<String, bool>) -> Result<(), Diagnostic> {
    for node in nodes {
        match &node.kind {
            NodeKind::Proof { of, body, .. } => {
                if let Some(of) = of {
                    match targets.get(of) {
                        Some(true) => {}
                        Some(false) => {
                            return Err(Diagnostic::error(
                                "E-PROOF-001",
                                format!("proof 'of: {of}' does not name a theorem-like block"),
                                node.span,
                            ));
                        }
                        None => {
                            return Err(Diagnostic::error(
                                "E-PROOF-001",
                                format!("proof 'of: {of}' names an unknown id"),
                                node.span,
                            ));
                        }
                    }
                }
                walk_proofs(body, targets)?;
            }
            NodeKind::TheoremLike { body, .. } => walk_proofs(body, targets)?,
            NodeKind::List { items, .. } => {
                for item in items {
                    walk_proofs(&item.continuation, targets)?;
                }
            }
            NodeKind::Heading { .. }
            | NodeKind::Paragraph { .. }
            | NodeKind::Equation { .. }
            | NodeKind::Figure { .. }
            | NodeKind::Table { .. }
            | NodeKind::RawTex { .. }
            | NodeKind::CodeBlock { .. }
            | NodeKind::Bibliography => {}
        }
    }
    Ok(())
}

/// A single module's typed symbol table: every declared `id` (headings,
/// equations, figures, tables, theorem-like blocks, proofs) mapped to what
/// a cross-reference to it should display. Headings resolve to their own
/// title text (there is no visible heading number to point at); every
/// other kind resolves to an ordinary numbered LaTeX `\ref`. Cross-file
/// resolution is task group 12's extension of this same model.
#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Heading(Vec<Inline>),
    Numbered,
}

pub type SymbolTable = std::collections::HashMap<String, SymbolKind>;

/// Builds the single-module symbol table for an already-lowered module,
/// for use by the LaTeX backend when rendering cross-references.
pub fn build_symbol_table(module: &ParsedModule) -> SymbolTable {
    let mut out = SymbolTable::new();
    collect_symbols(&module.blocks, &mut out);
    out
}

fn collect_symbols(nodes: &[Node], out: &mut SymbolTable) {
    for node in nodes {
        match &node.kind {
            NodeKind::Heading { id, inlines, .. } => {
                if let Some(id) = id {
                    out.insert(id.clone(), SymbolKind::Heading(inlines.clone()));
                }
            }
            NodeKind::Equation { id, .. } | NodeKind::Figure { id, .. } | NodeKind::Table { id, .. } => {
                if let Some(id) = id {
                    out.insert(id.clone(), SymbolKind::Numbered);
                }
            }
            NodeKind::TheoremLike { id, body, .. } => {
                if let Some(id) = id {
                    out.insert(id.clone(), SymbolKind::Numbered);
                }
                collect_symbols(body, out);
            }
            NodeKind::Proof { id, body, .. } => {
                if let Some(id) = id {
                    out.insert(id.clone(), SymbolKind::Numbered);
                }
                collect_symbols(body, out);
            }
            NodeKind::List { items, .. } => {
                for item in items {
                    collect_symbols(&item.continuation, out);
                }
            }
            NodeKind::Paragraph { .. }
            | NodeKind::RawTex { .. }
            | NodeKind::CodeBlock { .. }
            | NodeKind::Bibliography => {}
        }
    }
}

fn validate_cross_refs(nodes: &[Node], symbols: &SymbolTable) -> Result<(), Diagnostic> {
    for node in nodes {
        match &node.kind {
            NodeKind::Heading { inlines, .. } | NodeKind::Paragraph { inlines } => {
                check_inline_refs(inlines, node.span, symbols)?;
            }
            NodeKind::Figure { caption, .. } => {
                check_inline_refs(caption, node.span, symbols)?;
            }
            NodeKind::List { items, .. } => {
                for item in items {
                    check_inline_refs(&item.inlines, node.span, symbols)?;
                    validate_cross_refs(&item.continuation, symbols)?;
                }
            }
            NodeKind::TheoremLike { body, .. } | NodeKind::Proof { body, .. } => {
                validate_cross_refs(body, symbols)?;
            }
            NodeKind::Table {
                caption,
                header,
                rows,
                ..
            } => {
                if let Some(caption) = caption {
                    check_inline_refs(caption, node.span, symbols)?;
                }
                for cell in header {
                    check_inline_refs(cell, node.span, symbols)?;
                }
                for row in rows {
                    for cell in row {
                        check_inline_refs(cell, node.span, symbols)?;
                    }
                }
            }
            NodeKind::Equation { .. }
            | NodeKind::RawTex { .. }
            | NodeKind::CodeBlock { .. }
            | NodeKind::Bibliography => {}
        }
    }
    Ok(())
}

fn check_inline_refs(inlines: &[Inline], span: SourceSpan, symbols: &SymbolTable) -> Result<(), Diagnostic> {
    for inline in inlines {
        match inline {
            Inline::CrossRef(id) => {
                if !symbols.contains_key(id) {
                    return Err(Diagnostic::error(
                        "E-XREF-001",
                        format!("undefined cross-reference id '{id}'"),
                        span,
                    ));
                }
            }
            Inline::Emphasis(v) | Inline::Strong(v) => check_inline_refs(v, span, symbols)?,
            Inline::Link { label, .. } => check_inline_refs(label, span, symbols)?,
            Inline::Footnote(v) => check_inline_refs(v, span, symbols)?,
            Inline::Text(_) | Inline::Code(_) | Inline::Math(_) | Inline::Citation(_) => {}
        }
    }
    Ok(())
}

/// Project-wide citation binding: every cited alias, anywhere in the
/// module, must have a matching `refs:` declaration in `known_aliases`
/// (the project-wide reference-alias namespace built by
/// [`crate::project::symbols`]). Run by the application entrypoint after
/// [`lower`] succeeds, not by `lower` itself, so single-module syntax/unit
/// tests that lower citation syntax in isolation (without also declaring
/// `refs:`) are unaffected.
pub fn validate_citations(
    module: &ParsedModule,
    known_aliases: &std::collections::HashSet<String>,
) -> Result<(), Diagnostic> {
    validate_citations_with_sources(module, known_aliases, &std::collections::HashMap::new())
}

/// As [`validate_citations`], but given each module's own source text
/// (keyed by `FileId`), an unauthorized citation's diagnostic is narrowed
/// from its whole containing node to the exact `@alias` occurrence within
/// that node's original bytes -- a textual (not formally tracked-through-
/// the-lexer) refinement, since the inline lexer joins physical lines into
/// one logical string before parsing and does not itself retain a
/// per-inline byte offset. This is exact whenever the citation's
/// containing node is a single physical line (the overwhelmingly common
/// case); a citation that itself sits across a wrapped line boundary
/// falls back to the containing node's own span, same as
/// [`validate_citations`].
pub fn validate_citations_with_sources(
    module: &ParsedModule,
    known_aliases: &std::collections::HashSet<String>,
    sources: &std::collections::HashMap<FileId, &str>,
) -> Result<(), Diagnostic> {
    check_citations(&module.blocks, known_aliases, sources)
}

fn check_citations(
    nodes: &[Node],
    known_aliases: &std::collections::HashSet<String>,
    sources: &std::collections::HashMap<FileId, &str>,
) -> Result<(), Diagnostic> {
    for node in nodes {
        match &node.kind {
            NodeKind::Heading { inlines, .. } | NodeKind::Paragraph { inlines } => {
                check_inline_citations(inlines, node.span, known_aliases, sources)?;
            }
            NodeKind::Figure { caption, .. } => {
                check_inline_citations(caption, node.span, known_aliases, sources)?;
            }
            NodeKind::List { items, .. } => {
                for item in items {
                    check_inline_citations(&item.inlines, node.span, known_aliases, sources)?;
                    check_citations(&item.continuation, known_aliases, sources)?;
                }
            }
            NodeKind::TheoremLike { body, .. } | NodeKind::Proof { body, .. } => {
                check_citations(body, known_aliases, sources)?;
            }
            NodeKind::Table {
                caption,
                header,
                rows,
                ..
            } => {
                if let Some(caption) = caption {
                    check_inline_citations(caption, node.span, known_aliases, sources)?;
                }
                for cell in header {
                    check_inline_citations(cell, node.span, known_aliases, sources)?;
                }
                for row in rows {
                    for cell in row {
                        check_inline_citations(cell, node.span, known_aliases, sources)?;
                    }
                }
            }
            NodeKind::Equation { .. }
            | NodeKind::RawTex { .. }
            | NodeKind::CodeBlock { .. }
            | NodeKind::Bibliography => {}
        }
    }
    Ok(())
}

fn check_inline_citations(
    inlines: &[Inline],
    span: SourceSpan,
    known_aliases: &std::collections::HashSet<String>,
    sources: &std::collections::HashMap<FileId, &str>,
) -> Result<(), Diagnostic> {
    for inline in inlines {
        match inline {
            Inline::Citation(citation) => {
                for alias in citation_aliases(citation) {
                    if !known_aliases.contains(alias) {
                        let refined = sources
                            .get(&span.file_id)
                            .map(|text| refine_citation_span(span, alias, text))
                            .unwrap_or(span);
                        return Err(Diagnostic::error(
                            "E-CITE-001",
                            format!("citation '{alias}' has no matching 'refs:' declaration"),
                            refined,
                        ));
                    }
                }
            }
            Inline::Emphasis(v) | Inline::Strong(v) | Inline::Footnote(v) => {
                check_inline_citations(v, span, known_aliases, sources)?;
            }
            Inline::Link { label, .. } => check_inline_citations(label, span, known_aliases, sources)?,
            Inline::Text(_) | Inline::Code(_) | Inline::Math(_) | Inline::CrossRef(_) => {}
        }
    }
    Ok(())
}

/// Narrows `node_span` to the exact `@alias` occurrence within it, by a
/// literal search over the node's own original bytes. See
/// [`validate_citations_with_sources`] for why this is a heuristic rather
/// than a formally tracked position.
fn refine_citation_span(node_span: SourceSpan, alias: &str, source_text: &str) -> SourceSpan {
    let start = node_span.byte_start as usize;
    let end = (node_span.byte_end as usize).min(source_text.len());
    if start > end || end > source_text.len() || !source_text.is_char_boundary(start) {
        return node_span;
    }
    let slice = &source_text[start..end];
    let needle = format!("@{alias}");
    match slice.find(needle.as_str()) {
        Some(pos) => {
            let byte_start = node_span.byte_start + pos as u32;
            let byte_end = byte_start + needle.len() as u32;
            SourceSpan::new(node_span.file_id, byte_start, byte_end)
        }
        None => node_span,
    }
}

fn citation_aliases(citation: &Citation) -> Vec<&str> {
    match citation {
        Citation::Narrative(alias) => vec![alias.as_str()],
        Citation::Group(items) => items.iter().map(|i| i.alias.as_str()).collect(),
    }
}

/// True if the module contains an explicit or derived bibliography
/// position. Used to decide whether generated LaTeX needs to load
/// BibLaTeX/Biber at all: a module with no bibliography never pays for
/// that dependency.
pub fn has_bibliography(module: &ParsedModule) -> bool {
    fn any_bib(nodes: &[Node]) -> bool {
        nodes.iter().any(|n| match &n.kind {
            NodeKind::Bibliography => true,
            NodeKind::List { items, .. } => items.iter().any(|i| any_bib(&i.continuation)),
            NodeKind::TheoremLike { body, .. } | NodeKind::Proof { body, .. } => any_bib(body),
            _ => false,
        })
    }
    any_bib(&module.blocks)
}

/// True if the module contains at least one fenced code block. Used to
/// decide whether generated LaTeX needs to load `listings` at all: a
/// module with no code block never pays for that dependency, the same
/// reasoning [`has_bibliography`] applies to `biblatex`/`biber`. This
/// matters beyond tidiness: `listings` is not yet part of every deployed
/// managed TeX Live prefix's package closure (it is added by this same
/// change), so a document that never uses a code block must keep
/// compiling on a prefix that has not been updated to include it.
pub fn has_code_block(module: &ParsedModule) -> bool {
    fn any_code(nodes: &[Node]) -> bool {
        nodes.iter().any(|n| match &n.kind {
            NodeKind::CodeBlock { .. } => true,
            NodeKind::List { items, .. } => items.iter().any(|i| any_code(&i.continuation)),
            NodeKind::TheoremLike { body, .. } | NodeKind::Proof { body, .. } => any_code(body),
            _ => false,
        })
    }
    any_code(&module.blocks)
}

/// Every alias actually cited anywhere in the module, in no particular
/// order (a `BTreeSet` for deterministic iteration). The caller
/// intersects this with the project's authorized/bound aliases to decide
/// exactly which works belong in the generated `.bib` — a declared but
/// never-cited reference must never appear there.
pub fn collect_cited_aliases(module: &ParsedModule) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    collect_cited_in_nodes(&module.blocks, &mut out);
    out
}

fn collect_cited_in_nodes(nodes: &[Node], out: &mut std::collections::BTreeSet<String>) {
    for node in nodes {
        match &node.kind {
            NodeKind::Heading { inlines, .. } | NodeKind::Paragraph { inlines } => {
                collect_cited_in_inlines(inlines, out);
            }
            NodeKind::Figure { caption, .. } => collect_cited_in_inlines(caption, out),
            NodeKind::List { items, .. } => {
                for item in items {
                    collect_cited_in_inlines(&item.inlines, out);
                    collect_cited_in_nodes(&item.continuation, out);
                }
            }
            NodeKind::TheoremLike { body, .. } | NodeKind::Proof { body, .. } => {
                collect_cited_in_nodes(body, out);
            }
            NodeKind::Table {
                caption,
                header,
                rows,
                ..
            } => {
                if let Some(caption) = caption {
                    collect_cited_in_inlines(caption, out);
                }
                for cell in header {
                    collect_cited_in_inlines(cell, out);
                }
                for row in rows {
                    for cell in row {
                        collect_cited_in_inlines(cell, out);
                    }
                }
            }
            NodeKind::Equation { .. }
            | NodeKind::RawTex { .. }
            | NodeKind::CodeBlock { .. }
            | NodeKind::Bibliography => {}
        }
    }
}

fn collect_cited_in_inlines(inlines: &[Inline], out: &mut std::collections::BTreeSet<String>) {
    for inline in inlines {
        match inline {
            Inline::Citation(citation) => {
                for alias in citation_aliases(citation) {
                    out.insert(alias.to_string());
                }
            }
            Inline::Emphasis(v) | Inline::Strong(v) | Inline::Footnote(v) => collect_cited_in_inlines(v, out),
            Inline::Link { label, .. } => collect_cited_in_inlines(label, out),
            Inline::Text(_) | Inline::Code(_) | Inline::Math(_) | Inline::CrossRef(_) => {}
        }
    }
}

fn lower_blocks(blocks: Vec<TopBlock>) -> Result<Vec<Node>, Diagnostic> {
    blocks.into_iter().map(lower_block).collect()
}

fn lower_block(block: TopBlock) -> Result<Node, Diagnostic> {
    match block {
        TopBlock::Heading { level, text, id, span } => {
            let inlines = parse_inline_at(&text, span)?;
            Ok(Node {
                kind: NodeKind::Heading { level, id, inlines },
                span,
            })
        }
        TopBlock::Paragraph { text, span } => {
            let inlines = parse_inline_at(&text, span)?;
            Ok(Node {
                kind: NodeKind::Paragraph { inlines },
                span,
            })
        }
        TopBlock::List(raw) => {
            let span = raw.span;
            let mut items = Vec::new();
            for item in raw.items {
                items.push(lower_list_item(item)?);
            }
            Ok(Node {
                kind: NodeKind::List {
                    ordered: raw.ordered,
                    start: raw.start,
                    items,
                },
                span,
            })
        }
        TopBlock::Equation { id, numbered, payload, span } => {
            crate::syntax::math::validate(&payload).map_err(|e| math_error_to_diagnostic(e, span))?;
            Ok(Node {
                kind: NodeKind::Equation { id, numbered, payload },
                span,
            })
        }
        TopBlock::Figure {
            path,
            id,
            role,
            caption,
            caption_span,
            alt,
            span,
        } => {
            let caption_inlines = parse_inline_at(&caption, caption_span)?;
            if contains_footnote(&caption_inlines) {
                return Err(Diagnostic::error(
                    "E-META-016",
                    "figure captions cannot contain footnotes",
                    caption_span,
                ));
            }
            Ok(Node {
                kind: NodeKind::Figure {
                    path,
                    id,
                    role,
                    caption: caption_inlines,
                    alt,
                },
                span,
            })
        }
        TopBlock::Table {
            id,
            caption,
            align,
            header,
            rows,
            span,
        } => {
            let caption_inlines = caption
                .map(|(text, caption_span)| {
                    let inlines = parse_inline_at(&text, caption_span)?;
                    if contains_footnote(&inlines) {
                        return Err(Diagnostic::error(
                            "E-META-018",
                            "table captions and cells cannot contain footnotes",
                            caption_span,
                        ));
                    }
                    Ok(inlines)
                })
                .transpose()?;
            let lower_cells =
                |(cells, span): (Vec<String>, SourceSpan)| -> Result<Vec<Vec<Inline>>, Diagnostic> {
                    cells
                        .into_iter()
                        .map(|cell| {
                            let inlines = parse_inline_at(&cell, span)?;
                            if contains_footnote(&inlines) {
                                return Err(Diagnostic::error(
                                    "E-META-018",
                                    "table captions and cells cannot contain footnotes",
                                    span,
                                ));
                            }
                            Ok(inlines)
                        })
                        .collect()
                };
            let header_inlines = lower_cells(header)?;
            let rows_inlines = rows.into_iter().map(lower_cells).collect::<Result<Vec<_>, _>>()?;
            Ok(Node {
                kind: NodeKind::Table {
                    id,
                    caption: caption_inlines,
                    align,
                    header: header_inlines,
                    rows: rows_inlines,
                },
                span,
            })
        }
        TopBlock::TheoremLike {
            kind,
            title,
            id,
            body,
            span,
        } => {
            let body_nodes = lower_blocks(body)?;
            Ok(Node {
                kind: NodeKind::TheoremLike {
                    kind,
                    title,
                    id,
                    body: body_nodes,
                },
                span,
            })
        }
        TopBlock::Proof { id, of, body, span } => {
            let body_nodes = lower_blocks(body)?;
            Ok(Node {
                kind: NodeKind::Proof {
                    id,
                    of,
                    body: body_nodes,
                },
                span,
            })
        }
        TopBlock::RawTex { payload, span } => Ok(Node {
            kind: NodeKind::RawTex { payload },
            span,
        }),
        TopBlock::CodeBlock {
            language,
            code,
            span,
        } => Ok(Node {
            kind: NodeKind::CodeBlock { language, code },
            span,
        }),
        TopBlock::Bibliography { span } => Ok(Node {
            kind: NodeKind::Bibliography,
            span,
        }),
        TopBlock::Document { .. } | TopBlock::Refs { .. } | TopBlock::Include { .. } => {
            unreachable!("document/refs/include are extracted before body lowering")
        }
    }
}

fn lower_list_item(item: RawListItem) -> Result<ListItem, Diagnostic> {
    let inlines = parse_inline_at(&item.text, item.span)?;
    let continuation = lower_blocks(item.continuation)?;
    Ok(ListItem { inlines, continuation })
}

fn contains_footnote(inlines: &[Inline]) -> bool {
    inlines.iter().any(|i| match i {
        Inline::Footnote(_) => true,
        Inline::Emphasis(v) | Inline::Strong(v) => contains_footnote(v),
        Inline::Link { label, .. } => contains_footnote(label),
        Inline::Text(_) | Inline::Code(_) | Inline::Math(_) | Inline::CrossRef(_) | Inline::Citation(_) => false,
    })
}

fn parse_inline_at(text: &str, span: SourceSpan) -> Result<Vec<Inline>, Diagnostic> {
    let parsed = inlines::parse_inline(text).map_err(|e| Diagnostic::error("E-PARSE-050", e.0, span))?;
    validate_inline_math(&parsed, span)?;
    validate_inline_links(&parsed, span)?;
    Ok(parsed)
}

fn validate_inline_links(inlines: &[Inline], span: SourceSpan) -> Result<(), Diagnostic> {
    for inline in inlines {
        match inline {
            Inline::Link { label, destination } => {
                crate::latex::escape::validate_link_scheme(destination).map_err(
                    |crate::latex::escape::UnsafeLinkScheme(scheme)| {
                        Diagnostic::error(
                            "E-LINK-001",
                            format!("link scheme '{scheme}' is not permitted (only http, https, and mailto)"),
                            span,
                        )
                    },
                )?;
                validate_inline_links(label, span)?;
            }
            Inline::Emphasis(v) | Inline::Strong(v) | Inline::Footnote(v) => validate_inline_links(v, span)?,
            Inline::Text(_) | Inline::Code(_) | Inline::Math(_) | Inline::CrossRef(_) | Inline::Citation(_) => {}
        }
    }
    Ok(())
}

fn validate_inline_math(inlines: &[Inline], span: SourceSpan) -> Result<(), Diagnostic> {
    for inline in inlines {
        match inline {
            Inline::Math(payload) => {
                crate::syntax::math::validate(payload).map_err(|e| math_error_to_diagnostic(e, span))?;
            }
            Inline::Emphasis(v) | Inline::Strong(v) => validate_inline_math(v, span)?,
            Inline::Link { label, .. } => validate_inline_math(label, span)?,
            Inline::Footnote(v) => validate_inline_math(v, span)?,
            Inline::Text(_) | Inline::Code(_) | Inline::CrossRef(_) | Inline::Citation(_) => {}
        }
    }
    Ok(())
}

fn math_error_to_diagnostic(err: crate::syntax::math::MathError, span: SourceSpan) -> Diagnostic {
    use crate::syntax::math::MathError;
    let message = match &err {
        MathError::UnbalancedBraces { .. } => "math payload has unbalanced braces".to_string(),
        MathError::UnknownCommand { name, .. } => {
            format!("'\\{name}' is not part of the supported math subset; use the raw TeX escape hatch for unsupported macros")
        }
        MathError::ForbiddenCommand { name, .. } => {
            format!("'\\{name}' can execute code or perform I/O and is never permitted in math")
        }
        MathError::UnknownEnvironment { name, .. } => {
            format!("math environment '{name}' is not part of the supported subset")
        }
        MathError::MismatchedEnvironmentEnd { expected, found, .. } => match expected {
            Some(open) => format!("math environment '\\end{{{found}}}' does not match the open '\\begin{{{open}}}'"),
            None => format!("math environment '\\end{{{found}}}' has no matching '\\begin'"),
        },
        MathError::ControlByte { .. } => "math payload contains a raw control byte".to_string(),
        MathError::EncodedInput { .. } => "math payload uses a '^^' input encoding, which is never permitted".to_string(),
    };
    Diagnostic::error("E-MATH-001", message, span)
}

/// Collects strict-mode warnings (currently `W-TEX-001` for every explicit
/// raw `tex:` escape hatch) from an already-lowered module. Never fails a
/// build on its own; the application layer decides whether warnings block
/// `check --deny-warnings`.
pub fn collect_warnings(module: &ParsedModule) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    collect_node_warnings(&module.blocks, &mut out);
    out
}

/// Collects the source span of every explicit `tex:` raw escape hatch in
/// an already-lowered module. Raw TeX is fully supported by a normal
/// build (group 8) but is unconditionally unsupported by the MVP arXiv
/// export target (group 22), which needs every occurrence's location to
/// report `E-EXPORT-004` without inventing or approximating a span.
pub fn collect_raw_tex_spans(module: &ParsedModule) -> Vec<SourceSpan> {
    let mut out = Vec::new();
    collect_raw_tex_node_spans(&module.blocks, &mut out);
    out
}

fn collect_raw_tex_node_spans(nodes: &[Node], out: &mut Vec<SourceSpan>) {
    for node in nodes {
        match &node.kind {
            NodeKind::RawTex { .. } => out.push(node.span),
            NodeKind::List { items, .. } => {
                for item in items {
                    collect_raw_tex_node_spans(&item.continuation, out);
                }
            }
            NodeKind::TheoremLike { body, .. } | NodeKind::Proof { body, .. } => {
                collect_raw_tex_node_spans(body, out)
            }
            NodeKind::Heading { .. }
            | NodeKind::Paragraph { .. }
            | NodeKind::Equation { .. }
            | NodeKind::Figure { .. }
            | NodeKind::Table { .. }
            | NodeKind::CodeBlock { .. }
            | NodeKind::Bibliography => {}
        }
    }
}

fn collect_node_warnings(nodes: &[Node], out: &mut Vec<Diagnostic>) {
    for node in nodes {
        match &node.kind {
            NodeKind::RawTex { .. } => out.push(
                Diagnostic::warning(
                    "W-TEX-001",
                    "explicit raw TeX is opaque to Terse and is not guaranteed portable across engines or export targets",
                    node.span,
                ),
            ),
            NodeKind::List { items, .. } => {
                for item in items {
                    collect_node_warnings(&item.continuation, out);
                }
            }
            NodeKind::TheoremLike { body, .. } | NodeKind::Proof { body, .. } => {
                collect_node_warnings(body, out)
            }
            NodeKind::Heading { .. }
            | NodeKind::Paragraph { .. }
            | NodeKind::Equation { .. }
            | NodeKind::Figure { .. }
            | NodeKind::Table { .. }
            | NodeKind::CodeBlock { .. }
            | NodeKind::Bibliography => {}
        }
    }
}

fn lower_refs(entries: Vec<RefEntry>) -> Vec<ReferenceDeclaration> {
    entries
        .into_iter()
        .map(|e| ReferenceDeclaration {
            alias: e.alias,
            kind: e.kind,
            identifier: e.identifier,
        })
        .collect()
}

fn lower_metadata(raw: crate::syntax::blocks::RawMetadata) -> Result<DocumentMetadata, Diagnostic> {
    let (title_text, _title_span) = raw.title;

    let language = match raw.language {
        Some((lang, span)) => {
            if !SUPPORTED_LOCALES.contains(&lang.as_str()) {
                return Err(Diagnostic::error(
                    "E-META-002",
                    format!("unsupported language '{lang}'; supported locales are en, pt-BR"),
                    span,
                ));
            }
            lang
        }
        None => DEFAULT_LOCALE.to_string(),
    };

    let mut authors = Vec::new();
    for raw_author in raw.authors {
        let (name, _span) = raw_author.name;
        let affiliation = raw_author.affiliation.map(|a| match a {
            RawAffiliation::Single(s) => Affiliation::Single(s),
            RawAffiliation::Multiple(v) => Affiliation::Multiple(v),
        });
        authors.push(Author { name, affiliation });
    }

    let mut abstract_blocks = Vec::new();
    for (text, span) in raw.abstract_paragraphs {
        abstract_blocks.push(parse_inline_at(&text, span)?);
    }

    Ok(DocumentMetadata {
        title: title_text,
        subtitle: raw.subtitle,
        authors,
        affiliations: raw.affiliations,
        date: raw.date,
        language,
        abstract_blocks,
        keywords: raw.keywords,
    })
}

pub mod projection;

#[cfg(test)]
mod tests;
