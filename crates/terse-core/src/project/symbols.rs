//! Project-wide (cross-file) validation over the flattened, routed block
//! sequence produced by [`super::expand`], before semantic lowering:
//!
//! - one case-sensitive ID namespace across every included module, with
//!   both origins (span + occurrence route) reported on a duplicate;
//! - one project-wide reference-alias namespace, rejecting a duplicate
//!   alias declaration even when the identifier agrees;
//! - at most one explicit `bibliography` marker, with both origins
//!   reported if more than one is declared.
//!
//! This module is effect-free: it only reads already-parsed [`TopBlock`]
//! trees paired with their occurrence routes.

use std::collections::HashMap;

use crate::diagnostic::Diagnostic;
use crate::source::SourceSpan;
use crate::syntax::blocks::{RefEntry, TheoremKind, TopBlock};

/// Where a declaration came from: its original span plus the include
/// chain (occurrence route) that reached it.
#[derive(Debug, Clone)]
pub struct Occurrence {
    pub span: SourceSpan,
    pub route: Vec<String>,
}

fn route_text(route: &[String]) -> String {
    route.join(" \u{2192} ")
}

/// What kind of target an `id` refers to, for `proof [of: ...]` validation
/// (task 12.3 uses this to check the target is a theorem-like kind without
/// inferring anything from the label name).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Heading,
    TheoremLike(TheoremKind),
    Proof,
    Other,
}

/// The project-wide symbol/reference/bibliography namespaces collected
/// from the flattened, routed block sequence. Building this never mutates
/// or reorders the input; it only observes it.
#[derive(Debug, Clone, Default)]
pub struct ProjectSymbols {
    pub ids: HashMap<String, TargetKind>,
    pub reference_aliases: HashMap<String, RefEntry>,
    pub bibliography_marker: Option<Occurrence>,
}

/// Walks the flattened, routed block sequence once, collecting the
/// project-wide namespaces and failing on the first duplicate ID,
/// duplicate reference alias, or duplicate bibliography marker.
pub fn build(blocks: &[(TopBlock, Vec<String>)]) -> Result<ProjectSymbols, Diagnostic> {
    let mut out = ProjectSymbols::default();
    let mut id_origins: HashMap<String, Occurrence> = HashMap::new();
    let mut alias_origins: HashMap<String, Occurrence> = HashMap::new();

    for (block, route) in blocks {
        walk_block(block, route, &mut out, &mut id_origins, &mut alias_origins)?;
    }

    Ok(out)
}

fn declare_id(
    id: &str,
    kind: TargetKind,
    span: SourceSpan,
    route: &[String],
    out: &mut ProjectSymbols,
    origins: &mut HashMap<String, Occurrence>,
) -> Result<(), Diagnostic> {
    if let Some(first) = origins.get(id) {
        return Err(Diagnostic::error(
            "E-ID-002",
            format!("duplicate id '{id}' is declared more than once in this project"),
            span,
        )
        .with_related(
            format!("first declared here (route: {})", route_text(&first.route)),
            Some(first.span),
        )
        .with_related(
            format!("duplicate declared here (route: {})", route_text(route)),
            Some(span),
        ));
    }
    origins.insert(
        id.to_string(),
        Occurrence {
            span,
            route: route.to_vec(),
        },
    );
    out.ids.insert(id.to_string(), kind);
    Ok(())
}

fn declare_alias(
    entry: &RefEntry,
    route: &[String],
    out: &mut ProjectSymbols,
    origins: &mut HashMap<String, Occurrence>,
) -> Result<(), Diagnostic> {
    if let Some(first) = origins.get(&entry.alias) {
        return Err(Diagnostic::error(
            "E-REF-002",
            format!(
                "duplicate reference alias '{}' is declared more than once in this project",
                entry.alias
            ),
            entry.span,
        )
        .with_related(
            format!("first declared here (route: {})", route_text(&first.route)),
            Some(first.span),
        )
        .with_related(
            format!("duplicate declared here (route: {})", route_text(route)),
            Some(entry.span),
        ));
    }
    origins.insert(
        entry.alias.clone(),
        Occurrence {
            span: entry.span,
            route: route.to_vec(),
        },
    );
    out.reference_aliases.insert(entry.alias.clone(), entry.clone());
    Ok(())
}

fn declare_bibliography_marker(
    span: SourceSpan,
    route: &[String],
    out: &mut ProjectSymbols,
) -> Result<(), Diagnostic> {
    if let Some(first) = &out.bibliography_marker {
        return Err(Diagnostic::error(
            "E-BIB-001",
            "at most one 'bibliography' marker is permitted per project",
            span,
        )
        .with_related(
            format!("first marker here (route: {})", route_text(&first.route)),
            Some(first.span),
        )
        .with_related(
            format!("second marker here (route: {})", route_text(route)),
            Some(span),
        ));
    }
    out.bibliography_marker = Some(Occurrence {
        span,
        route: route.to_vec(),
    });
    Ok(())
}

fn walk_block(
    block: &TopBlock,
    route: &[String],
    out: &mut ProjectSymbols,
    id_origins: &mut HashMap<String, Occurrence>,
    alias_origins: &mut HashMap<String, Occurrence>,
) -> Result<(), Diagnostic> {
    match block {
        TopBlock::Heading { id, span, .. } => {
            if let Some(id) = id {
                declare_id(id, TargetKind::Heading, *span, route, out, id_origins)?;
            }
        }
        TopBlock::Equation { id, span, .. } | TopBlock::Figure { id, span, .. } | TopBlock::Table { id, span, .. } => {
            if let Some(id) = id {
                declare_id(id, TargetKind::Other, *span, route, out, id_origins)?;
            }
        }
        TopBlock::TheoremLike { kind, id, body, span, .. } => {
            if let Some(id) = id {
                declare_id(id, TargetKind::TheoremLike(*kind), *span, route, out, id_origins)?;
            }
            for nested in body {
                walk_block(nested, route, out, id_origins, alias_origins)?;
            }
        }
        TopBlock::Proof { id, body, span, .. } => {
            if let Some(id) = id {
                declare_id(id, TargetKind::Proof, *span, route, out, id_origins)?;
            }
            for nested in body {
                walk_block(nested, route, out, id_origins, alias_origins)?;
            }
        }
        TopBlock::List(list) => {
            for item in &list.items {
                for nested in &item.continuation {
                    walk_block(nested, route, out, id_origins, alias_origins)?;
                }
            }
        }
        TopBlock::Refs { entries, .. } => {
            for entry in entries {
                declare_alias(entry, route, out, alias_origins)?;
            }
        }
        TopBlock::Bibliography { span } => {
            declare_bibliography_marker(*span, route, out)?;
        }
        TopBlock::Document { .. } | TopBlock::Paragraph { .. } | TopBlock::RawTex { .. } | TopBlock::Include { .. } => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FileId;

    fn span(file: u32, start: u32) -> SourceSpan {
        SourceSpan::new(FileId(file), start, start + 1)
    }

    fn heading(id: &str, file: u32, start: u32) -> TopBlock {
        TopBlock::Heading {
            level: 1,
            text: "T".to_string(),
            id: Some(id.to_string()),
            span: span(file, start),
        }
    }

    #[test]
    fn test_duplicate_ids_show_both_routes() {
        let blocks = vec![
            (heading("demand-model", 0, 0), vec!["paper.trs".to_string(), "a.trs#1".to_string()]),
            (heading("demand-model", 1, 0), vec!["paper.trs".to_string(), "b.trs#2".to_string()]),
        ];
        let err = build(&blocks).unwrap_err();
        assert_eq!(err.code, "E-ID-002");
        assert_eq!(err.related.len(), 2);
        assert!(err.related[0].message.contains("a.trs#1"));
        assert!(err.related[1].message.contains("b.trs#2"));
    }

    #[test]
    fn test_case_distinct_ids_are_not_duplicates() {
        let blocks = vec![
            (heading("Demand", 0, 0), vec!["paper.trs".to_string()]),
            (heading("demand", 0, 10), vec!["paper.trs".to_string()]),
        ];
        assert!(build(&blocks).is_ok());
    }

    #[test]
    fn test_duplicate_reference_alias_fails_even_with_same_identifier() {
        let entry = |alias: &str, start: u32| RefEntry {
            alias: alias.to_string(),
            kind: crate::syntax::blocks::RefKind::Doi,
            identifier: "10.1/x".to_string(),
            span: span(0, start),
        };
        let blocks = vec![
            (
                TopBlock::Refs {
                    entries: vec![entry("robins1986", 0)],
                    span: span(0, 0),
                },
                vec!["paper.trs".to_string()],
            ),
            (
                TopBlock::Refs {
                    entries: vec![entry("robins1986", 10)],
                    span: span(0, 10),
                },
                vec!["paper.trs".to_string()],
            ),
        ];
        let err = build(&blocks).unwrap_err();
        assert_eq!(err.code, "E-REF-002");
    }

    #[test]
    fn test_bibliography_markers_are_global() {
        let blocks = vec![
            (TopBlock::Bibliography { span: span(0, 0) }, vec!["paper.trs".to_string(), "a.trs#1".to_string()]),
            (TopBlock::Bibliography { span: span(1, 0) }, vec!["paper.trs".to_string(), "b.trs#2".to_string()]),
        ];
        let err = build(&blocks).unwrap_err();
        assert_eq!(err.code, "E-BIB-001");
        assert_eq!(err.related.len(), 2);
    }

    #[test]
    fn test_single_bibliography_marker_is_accepted() {
        let blocks = vec![(TopBlock::Bibliography { span: span(0, 0) }, vec!["paper.trs".to_string()])];
        let symbols = build(&blocks).expect("single marker is fine");
        assert!(symbols.bibliography_marker.is_some());
    }
}
