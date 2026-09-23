//! Canonical semantic projection and digest (versioned: `PROJECTION_VERSION`).
//!
//! A [`Projection`] retains every authored/effective bibliography-relevant
//! fact about a compiled document while excluding data that varies for
//! reasons irrelevant to authored meaning: source spans, incidental IDs
//! used only for internal cross-referencing plumbing, theme-furniture
//! output (watermarks, headers/footers, logos), and theme settings
//! themselves. Two documents that differ only in theme, or only in the
//! original byte encoding of otherwise-identical content, project to an
//! equal [`Projection`] and therefore an equal [`digest`].
//!
//! Node IDs (`heading#id`, figure/table/theorem `id`, proof `of`) ARE
//! retained: they are part of authored meaning (cross-reference intent),
//! not incidental. What's excluded is furniture the compiler derives
//! (page numbers, contents, watermarks) and presentation-only settings.

use std::collections::BTreeMap;

use super::{Author, DocumentMetadata, ListItem, Node, NodeKind, ParsedModule};
use crate::references::record::NormalizedRecord;
use crate::syntax::inlines::Inline;
use sha2::{Digest, Sha256};

/// Bumped whenever the projection's shape changes in a way that would
/// change an existing digest for unchanged authored content. Version 2
/// added the effective bibliography records: before it, two documents
/// whose cited works carried different locked metadata digested
/// identically, which called materially different papers equal.
pub const PROJECTION_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectedMetadata {
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
pub enum ProjectedNode {
    Heading { level: u8, id: Option<String>, inlines: Vec<Inline> },
    Paragraph { inlines: Vec<Inline> },
    List { ordered: bool, start: Option<u32>, items: Vec<ProjectedListItem> },
    Equation { id: Option<String>, numbered: bool, payload: String },
    Figure { path: String, id: Option<String>, role: Option<String>, caption: Vec<Inline>, alt: String },
    Table { id: Option<String>, caption: String, header: Vec<String>, rows: Vec<Vec<String>> },
    TheoremLike { kind: crate::semantic::TheoremKind, title: Option<String>, id: Option<String>, body: Vec<ProjectedNode> },
    Proof { id: Option<String>, of: Option<String>, body: Vec<ProjectedNode> },
    RawTex { payload: String },
    CodeBlock { language: Option<String>, code: String },
    Bibliography,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectedListItem {
    pub inlines: Vec<Inline>,
    pub continuation: Vec<ProjectedNode>,
}

/// The canonical, theme-invariant, span-free view of a compiled module.
#[derive(Debug, Clone, PartialEq)]
pub struct Projection {
    pub version: u32,
    pub metadata: Option<ProjectedMetadata>,
    pub blocks: Vec<ProjectedNode>,
    /// Every authorized alias's effective record. The bibliography a
    /// reader sees is authored meaning, not presentation: two documents
    /// with identical prose citing works whose locked metadata differs
    /// are different documents. A `BTreeMap` of already-`Debug` records
    /// keeps [`digest`] deterministic.
    pub references: BTreeMap<String, NormalizedRecord>,
}

fn project_node(node: &Node) -> ProjectedNode {
    match &node.kind {
        NodeKind::Heading { level, id, inlines } => {
            ProjectedNode::Heading { level: *level, id: id.clone(), inlines: inlines.clone() }
        }
        NodeKind::Paragraph { inlines } => ProjectedNode::Paragraph { inlines: inlines.clone() },
        NodeKind::List { ordered, start, items } => ProjectedNode::List {
            ordered: *ordered,
            start: *start,
            items: items.iter().map(project_list_item).collect(),
        },
        NodeKind::Equation { id, numbered, payload } => {
            ProjectedNode::Equation { id: id.clone(), numbered: *numbered, payload: payload.clone() }
        }
        NodeKind::Figure { path, id, role, caption, alt } => ProjectedNode::Figure {
            path: path.clone(),
            id: id.clone(),
            role: role.clone(),
            caption: caption.clone(),
            alt: alt.clone(),
        },
        NodeKind::Table { id, caption, header, rows } => ProjectedNode::Table {
            id: id.clone(),
            caption: caption.clone(),
            header: header.clone(),
            rows: rows.clone(),
        },
        NodeKind::TheoremLike { kind, title, id, body } => ProjectedNode::TheoremLike {
            kind: kind.clone(),
            title: title.clone(),
            id: id.clone(),
            body: body.iter().map(project_node).collect(),
        },
        NodeKind::Proof { id, of, body } => ProjectedNode::Proof {
            id: id.clone(),
            of: of.clone(),
            body: body.iter().map(project_node).collect(),
        },
        NodeKind::RawTex { payload } => ProjectedNode::RawTex { payload: payload.clone() },
        NodeKind::CodeBlock { language, code } => {
            ProjectedNode::CodeBlock { language: language.clone(), code: code.clone() }
        }
        NodeKind::Bibliography => ProjectedNode::Bibliography,
    }
}

fn project_list_item(item: &ListItem) -> ProjectedListItem {
    ProjectedListItem {
        inlines: item.inlines.clone(),
        continuation: item.continuation.iter().map(project_node).collect(),
    }
}

fn project_metadata(metadata: &DocumentMetadata) -> ProjectedMetadata {
    ProjectedMetadata {
        title: metadata.title.clone(),
        subtitle: metadata.subtitle.clone(),
        authors: metadata.authors.clone(),
        affiliations: metadata.affiliations.clone(),
        date: metadata.date.clone(),
        language: metadata.language.clone(),
        abstract_blocks: metadata.abstract_blocks.clone(),
        keywords: metadata.keywords.clone(),
    }
}

/// Projects a lowered module into its canonical, theme-invariant form.
/// Pure and effect-free: no I/O, no spans, no incidental furniture.
pub fn project(
    module: &ParsedModule,
    references: &BTreeMap<String, NormalizedRecord>,
) -> Projection {
    Projection {
        version: PROJECTION_VERSION,
        metadata: module.metadata.as_ref().map(project_metadata),
        blocks: module.blocks.iter().map(project_node).collect(),
        references: references.clone(),
    }
}

/// A stable digest over a [`Projection`]'s canonical debug representation.
/// Two projections that are `==` always produce the same digest; the
/// reverse holds as long as [`ProjectedNode`]/[`ProjectedMetadata`]'s
/// `Debug` output is injective over their `PartialEq` fields, which it is
/// here (no interior mutability, no non-deterministic formatting).
pub fn digest(projection: &Projection) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{projection:?}").as_bytes());
    let bytes = hasher.finalize();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
