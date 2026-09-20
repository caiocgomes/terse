//! Effect-free multi-file include expansion.
//!
//! This module only operates on already-loaded, already-parsed modules: it
//! never reads a file itself. The application layer discovers which
//! `.trs` files exist (following `include` statements with real filesystem
//! confinement), parses each one exactly once, and hands the resulting map
//! here keyed by root-relative logical path (e.g. `"sections/method.trs"`;
//! the entry uses its manifest-declared logical path, e.g. `"paper.trs"`).
//!
//! Expansion is parse-once/expand-per-occurrence: each module is parsed a
//! single time, but every `include` occurrence re-emits (clones) that
//! module's blocks at its authored position, so repeated inclusion repeats
//! content rather than deduplicating it. Cycle detection uses the active
//! expansion stack of logical keys, independent of host paths or any
//! parse-cache identity.

use std::collections::HashMap;

use crate::diagnostic::Diagnostic;
use crate::source::SourceSpan;
use crate::syntax::blocks::TopBlock;

use super::paths::{self, PathError};

/// One already-loaded, already-parsed module, ready to expand.
#[derive(Debug, Clone)]
pub struct ModuleSource {
    pub blocks: Vec<TopBlock>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IncludePathError {
    /// The declared path names more than one file (`*`/`?`); Terse never
    /// enumerates the filesystem to decide included content.
    Wildcard,
    /// The declared path does not name an explicit `.trs` module.
    NotTrs,
    Path(PathError),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExpandError {
    InvalidIncludePath {
        span: SourceSpan,
        declared: String,
        reason: IncludePathError,
    },
    MissingInclude {
        span: SourceSpan,
        declared: String,
        target: String,
    },
    Cycle {
        span: SourceSpan,
        /// The complete route, starting at the entry and ending with the
        /// target that closes the cycle (which repeats an earlier entry).
        route: Vec<String>,
    },
    /// A non-entry module declared `document:` metadata, which the
    /// language reserves for the entry module only.
    MetadataOutsideEntry { span: SourceSpan },
}

impl ExpandError {
    pub fn into_diagnostic(self) -> Diagnostic {
        match self {
            ExpandError::InvalidIncludePath { span, declared, reason } => {
                let message = match reason {
                    IncludePathError::Wildcard => format!(
                        "include '{declared}' names more than one file; list explicit module paths instead of a wildcard"
                    ),
                    IncludePathError::NotTrs => {
                        format!("include '{declared}' must name an explicit '.trs' module")
                    }
                    IncludePathError::Path(_) => {
                        format!("include '{declared}' is not a valid path within the project")
                    }
                };
                Diagnostic::error("E-INCLUDE-001", message, span)
            }
            ExpandError::MissingInclude { span, declared, target } => Diagnostic::error(
                "E-INCLUDE-002",
                format!("include '{declared}' (resolved to '{target}') does not exist"),
                span,
            ),
            ExpandError::Cycle { span, route } => Diagnostic::error(
                "E-INCLUDE-003",
                format!("include cycle: {}", route.join(" \u{2192} ")),
                span,
            ),
            ExpandError::MetadataOutsideEntry { span } => Diagnostic::error(
                "E-INCLUDE-004",
                "document metadata is only permitted in the entry module",
                span,
            ),
        }
    }
}

/// The root-relative logical directory a module's own key lives in, e.g.
/// `"sections/method.trs"` is in directory `["sections"]`, and the
/// root-level `"paper.trs"` is in directory `[]`.
pub fn dir_of(key: &str) -> Vec<String> {
    let mut parts: Vec<String> = key.split('/').map(str::to_string).collect();
    parts.pop();
    parts
}

fn resolve_target(base_dir: &[String], declared: &str) -> Result<String, IncludePathError> {
    if declared.contains('*') || declared.contains('?') {
        return Err(IncludePathError::Wildcard);
    }
    if !declared.ends_with(".trs") {
        return Err(IncludePathError::NotTrs);
    }
    let components =
        paths::resolve_declared_path(base_dir, declared).map_err(IncludePathError::Path)?;
    Ok(components.join("/"))
}

/// Expands `entry_key`'s module (and everything it transitively includes)
/// into one flat, authored-order block sequence. Each block is paired with
/// its occurrence route: the chain of module keys (starting at the entry)
/// traversed to emit it, so two occurrences of the same included module
/// (e.g. included twice) carry distinct routes even though their content
/// and spans are identical — this is what lets project-wide diagnostics
/// (duplicate IDs, duplicate bibliography markers) report two genuinely
/// different origins for coincident spans.
pub fn expand(
    entry_key: &str,
    modules: &HashMap<String, ModuleSource>,
) -> Result<Vec<(TopBlock, Vec<String>)>, ExpandError> {
    let mut active = vec![entry_key.to_string()];
    let mut route = vec![entry_key.to_string()];
    let mut counter: u32 = 0;
    expand_module(entry_key, modules, &mut active, &mut route, &mut counter, true)
}

#[allow(clippy::too_many_arguments)]
fn expand_module(
    key: &str,
    modules: &HashMap<String, ModuleSource>,
    active: &mut Vec<String>,
    route: &mut Vec<String>,
    counter: &mut u32,
    is_entry: bool,
) -> Result<Vec<(TopBlock, Vec<String>)>, ExpandError> {
    let module = modules
        .get(key)
        .expect("caller resolves and inserts every reachable key before expanding");
    let base_dir = dir_of(key);

    let mut out = Vec::new();
    for block in &module.blocks {
        match block {
            TopBlock::Document { metadata } if !is_entry => {
                return Err(ExpandError::MetadataOutsideEntry {
                    span: metadata.title.1,
                });
            }
            TopBlock::Include { path, span } => {
                let target = resolve_target(&base_dir, path).map_err(|reason| {
                    ExpandError::InvalidIncludePath {
                        span: *span,
                        declared: path.clone(),
                        reason,
                    }
                })?;
                if !modules.contains_key(&target) {
                    return Err(ExpandError::MissingInclude {
                        span: *span,
                        declared: path.clone(),
                        target,
                    });
                }
                if active.contains(&target) {
                    let mut cycle_route = active.clone();
                    cycle_route.push(target);
                    return Err(ExpandError::Cycle {
                        span: *span,
                        route: cycle_route,
                    });
                }
                // Every include *occurrence* gets a distinct, deterministic
                // identity (a traversal-order counter appended to the plain
                // target key), independent of host paths or cache state, so
                // two occurrences of the same target from the same parent
                // still produce distinct routes for their duplicate-origin
                // diagnostics even though their spans coincide.
                *counter += 1;
                let occurrence = format!("{target}#{counter}");
                active.push(target.clone());
                route.push(occurrence);
                let mut nested = expand_module(&target, modules, active, route, counter, false)?;
                route.pop();
                active.pop();
                out.append(&mut nested);
            }
            other => out.push((other.clone(), route.clone())),
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{FileId, SourceSpan};
    use crate::syntax::blocks::{RawAffiliation, RawAuthor, RawMetadata};

    fn span() -> SourceSpan {
        SourceSpan::new(FileId(0), 0, 1)
    }

    fn paragraph(text: &str) -> TopBlock {
        TopBlock::Paragraph {
            text: text.to_string(),
            span: span(),
        }
    }

    fn include(path: &str) -> TopBlock {
        TopBlock::Include {
            path: path.to_string(),
            span: span(),
        }
    }

    fn modules(pairs: &[(&str, Vec<TopBlock>)]) -> HashMap<String, ModuleSource> {
        pairs
            .iter()
            .map(|(k, b)| (k.to_string(), ModuleSource { blocks: b.clone() }))
            .collect()
    }

    #[test]
    fn test_repeated_include_repeats_content() {
        let mods = modules(&[
            ("paper.trs", vec![include("method.trs"), include("method.trs")]),
            ("method.trs", vec![paragraph("shared")]),
        ]);
        let expanded = expand("paper.trs", &mods).expect("expands");
        let blocks: Vec<TopBlock> = expanded.into_iter().map(|(b, _)| b).collect();
        assert_eq!(blocks, vec![paragraph("shared"), paragraph("shared")]);
    }

    #[test]
    fn test_include_order_is_authored_order() {
        let mods = modules(&[
            (
                "paper.trs",
                vec![paragraph("intro"), include("method.trs"), paragraph("outro")],
            ),
            ("method.trs", vec![paragraph("method body")]),
        ]);
        let expanded = expand("paper.trs", &mods).expect("expands");
        let blocks: Vec<TopBlock> = expanded.into_iter().map(|(b, _)| b).collect();
        assert_eq!(
            blocks,
            vec![paragraph("intro"), paragraph("method body"), paragraph("outro")]
        );
    }

    #[test]
    fn test_repeated_include_occurrences_have_distinct_routes() {
        // Two occurrences of the same included module carry distinct
        // routes even though their block content and origin file/line are
        // identical, so a duplicate-ID diagnostic reported per-occurrence
        // can still name two different origins.
        let mods = modules(&[
            ("paper.trs", vec![include("shared.trs"), include("shared.trs")]),
            ("shared.trs", vec![paragraph("x")]),
        ]);
        let expanded = expand("paper.trs", &mods).expect("expands");
        assert_eq!(expanded.len(), 2);
        assert_ne!(expanded[0].1, expanded[1].1);
        assert_eq!(expanded[0].1, vec!["paper.trs".to_string(), "shared.trs#1".to_string()]);
        assert_eq!(expanded[1].1, vec!["paper.trs".to_string(), "shared.trs#2".to_string()]);
    }

    #[test]
    fn test_wildcard_include_is_invalid() {
        let mods = modules(&[("paper.trs", vec![include("sections/*.trs")])]);
        let err = expand("paper.trs", &mods).unwrap_err();
        assert!(matches!(
            err,
            ExpandError::InvalidIncludePath {
                reason: IncludePathError::Wildcard,
                ..
            }
        ));
    }

    #[test]
    fn test_missing_include_is_source_located() {
        let mods = modules(&[("paper.trs", vec![include("missing.trs")])]);
        let err = expand("paper.trs", &mods).unwrap_err();
        match err {
            ExpandError::MissingInclude { target, .. } => assert_eq!(target, "missing.trs"),
            other => panic!("expected MissingInclude, got {other:?}"),
        }
    }

    #[test]
    fn test_indirect_include_cycle_is_complete() {
        let mods = modules(&[
            ("paper.trs", vec![include("method.trs")]),
            ("method.trs", vec![include("appendix.trs")]),
            ("appendix.trs", vec![include("paper.trs")]),
        ]);
        let err = expand("paper.trs", &mods).unwrap_err();
        match err {
            ExpandError::Cycle { route, .. } => assert_eq!(
                route,
                vec![
                    "paper.trs".to_string(),
                    "method.trs".to_string(),
                    "appendix.trs".to_string(),
                    "paper.trs".to_string(),
                ]
            ),
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    #[test]
    fn test_nested_document_metadata_is_rejected() {
        let metadata = RawMetadata {
            title: ("Nested".to_string(), span()),
            subtitle: None,
            authors: vec![RawAuthor {
                name: ("A".to_string(), span()),
                affiliation: None::<RawAffiliation>,
            }],
            affiliations: Vec::new(),
            date: None,
            language: None,
            abstract_paragraphs: Vec::new(),
            keywords: Vec::new(),
        };
        let mods = modules(&[
            ("paper.trs", vec![include("method.trs")]),
            ("method.trs", vec![TopBlock::Document { metadata }]),
        ]);
        let err = expand("paper.trs", &mods).unwrap_err();
        assert!(matches!(err, ExpandError::MetadataOutsideEntry { .. }));
    }

    #[test]
    fn test_include_occurrences_are_distinct() {
        // Two occurrences of the same target are independent expansion
        // events, not a single memoized node: each produces its own block
        // in the flattened output, so the result has two entries even
        // though their content is identical.
        let mods = modules(&[
            ("paper.trs", vec![include("shared.trs"), include("shared.trs")]),
            ("shared.trs", vec![paragraph("x")]),
        ]);
        let expanded = expand("paper.trs", &mods).expect("expands");
        assert_eq!(expanded.len(), 2);
    }
}
