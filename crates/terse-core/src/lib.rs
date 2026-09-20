//! Terse compiler core.
//!
//! This crate has no side effects: it never touches the filesystem, spawns
//! a process, makes a network call, or reads the clock. [`compile`] accepts
//! an explicit in-memory [`InputSnapshot`] and returns diagnostics plus an
//! optional [`ArtifactPlan`]. The application layer (`terse-cli`) owns every
//! effectful operation: reading files, running the LaTeX engine, and
//! publishing output.

pub mod artifact;
pub mod diagnostic;
pub mod latex;
pub mod project;
pub mod references;
pub mod semantic;
pub mod source;
pub mod syntax;
pub mod theme;
pub mod toolchain;

use std::collections::HashMap;

use diagnostic::Diagnostic;
use project::expand::{self, ModuleSource};
use semantic::ParsedModule;
use source::SourceFile;
use syntax::blocks::TopBlock;

/// The complete in-memory input to a compile: the entry module plus every
/// other module reachable through `include`, keyed by root-relative
/// logical path (e.g. `"sections/method.trs"`). The entry's own key is
/// `entry_key`, e.g. `"paper.trs"`; the root-level default entry uses an
/// empty key (no directory component, so relative includes resolve as if
/// declared at the project root).
#[derive(Debug, Clone)]
pub struct InputSnapshot {
    pub entry_key: String,
    pub entry: SourceFile,
    pub modules: HashMap<String, SourceFile>,
    /// The decoded `references.lock`, if the project has one. Absent is
    /// equivalent to an empty lock (no aliases resolved yet).
    pub lock: Option<references::lock::LockFile>,
    /// The current `references.overrides.toml` content, if any, keyed by
    /// alias.
    pub overrides: HashMap<String, references::overrides::OverridePatch>,
}

impl InputSnapshot {
    /// A single-module snapshot with no includes and no reference data.
    pub fn single(entry: SourceFile) -> Self {
        InputSnapshot {
            entry_key: String::new(),
            entry,
            modules: HashMap::new(),
            lock: None,
            overrides: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactPlan {
    pub module: ParsedModule,
    /// Every authorized alias's effective metadata, regardless of whether
    /// it is actually cited. Callers wanting cited-only records (e.g.
    /// `.bib` generation) intersect this with
    /// [`semantic::collect_cited_aliases`].
    pub bindings: references::bind::Bindings,
    /// Each compiled module's root-relative logical path, keyed by the
    /// [`source::FileId`] its spans carry. Carried on the plan so the
    /// artifact layer can name source-map origins without the core ever
    /// touching a filesystem.
    pub file_paths: std::collections::BTreeMap<source::FileId, String>,
}

/// Parses every module in the snapshot and expands `include`s starting
/// from the entry, returning the flattened, routed block sequence plus
/// the project-wide symbol/reference-alias/bibliography namespaces built
/// from it. This stops short of citation binding, so it succeeds even
/// when a declared alias has no matching lock entry yet — `refs resolve`
/// (in `terse-cli`) uses this to discover what to resolve *before*
/// requiring valid citation bindings, exactly as ordinary `compile` does
/// require them.
pub fn collect_declarations(
    snapshot: &InputSnapshot,
) -> Result<(Vec<(TopBlock, Vec<String>)>, project::symbols::ProjectSymbols), Diagnostic> {
    let entry_blocks = parse_source(&snapshot.entry)?;

    let mut modules: HashMap<String, ModuleSource> = HashMap::new();
    modules.insert(snapshot.entry_key.clone(), ModuleSource { blocks: entry_blocks });
    // Sorted for deterministic traversal: a `HashMap`'s own iteration
    // order is not stable across runs, and this order feeds diagnostic
    // ordering downstream.
    let mut keys: Vec<&String> = snapshot.modules.keys().collect();
    keys.sort();
    for key in keys {
        let file = &snapshot.modules[key];
        let blocks = parse_source(file)?;
        modules.insert(key.clone(), ModuleSource { blocks });
    }

    let expanded = expand::expand(&snapshot.entry_key, &modules).map_err(|e| e.into_diagnostic())?;
    let project_symbols = project::symbols::build(&expanded)?;
    Ok((expanded, project_symbols))
}

/// Parses every module in the snapshot with bounded error recovery
/// (`parse_module_with_recovery`), returning every diagnostic found
/// across all modules in deterministic (sorted-key) order. A non-empty
/// result means the input can never produce a publishable artifact plan,
/// but multiple independent errors are surfaced in one pass instead of
/// stopping at the first.
fn parse_all_with_recovery(snapshot: &InputSnapshot) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let (_blocks, entry_diags) = parse_source_with_recovery(&snapshot.entry);
    diags.extend(entry_diags);

    let mut keys: Vec<&String> = snapshot.modules.keys().collect();
    keys.sort();
    for key in keys {
        let (_blocks, module_diags) = parse_source_with_recovery(&snapshot.modules[key]);
        diags.extend(module_diags);
    }
    diags
}

/// Parses every module in the snapshot, expands `include`s starting from
/// the entry, and lowers the flattened result. Pure function: same input
/// always produces the same output, with no observable side effects.
pub fn compile(snapshot: &InputSnapshot) -> (Vec<Diagnostic>, Option<ArtifactPlan>) {
    let entry_file_id = snapshot.entry.id;

    let recovered = parse_all_with_recovery(snapshot);
    if !recovered.is_empty() {
        return (recovered, None);
    }

    let (expanded, project_symbols) = match collect_declarations(snapshot) {
        Ok(v) => v,
        Err(diag) => return (vec![diag], None),
    };

    let lock = snapshot.lock.clone().unwrap_or_default();
    let bindings = match references::bind::bind(&project_symbols.reference_aliases, &lock, &snapshot.overrides) {
        Ok(b) => b,
        Err(diag) => return (vec![diag], None),
    };
    let known_aliases: std::collections::HashSet<String> = bindings.authorized.keys().cloned().collect();

    let flat_blocks: Vec<TopBlock> = expanded.into_iter().map(|(b, _)| b).collect();

    let module = match semantic::lower(flat_blocks, entry_file_id) {
        Ok(module) => module,
        Err(diag) => return (vec![diag], None),
    };

    let mut sources: HashMap<source::FileId, &str> = HashMap::new();
    sources.insert(snapshot.entry.id, snapshot.entry.text());
    for file in snapshot.modules.values() {
        sources.insert(file.id, file.text());
    }
    if let Err(diag) = semantic::validate_citations_with_sources(&module, &known_aliases, &sources) {
        return (vec![diag], None);
    }

    let mut file_paths = std::collections::BTreeMap::new();
    file_paths.insert(snapshot.entry.id, snapshot.entry_key.clone());
    for (key, file) in &snapshot.modules {
        file_paths.insert(file.id, key.clone());
    }

    (
        Vec::new(),
        Some(ArtifactPlan {
            module,
            bindings,
            file_paths,
        }),
    )
}

fn parse_source(file: &SourceFile) -> Result<Vec<TopBlock>, Diagnostic> {
    let file_id = file.id;
    let base = file.base_offset();
    let lines = syntax::lexer::lex_lines(file.text())
        .map_err(|err| syntax::lexer::indent_error_to_diagnostic(err, file_id, base))?;
    syntax::blocks::parse_module(&lines, file.text(), file_id, base)
}

/// As [`parse_source`], but with bounded error recovery. A lexer-level
/// failure (invalid indentation bytes) still stops at one diagnostic --
/// recovery is a parser-level structural-boundary concept, not lexical.
fn parse_source_with_recovery(file: &SourceFile) -> (Vec<TopBlock>, Vec<Diagnostic>) {
    let file_id = file.id;
    let base = file.base_offset();
    let lines = match syntax::lexer::lex_lines(file.text()) {
        Ok(lines) => lines,
        Err(err) => {
            return (
                Vec::new(),
                vec![syntax::lexer::indent_error_to_diagnostic(err, file_id, base)],
            )
        }
    };
    syntax::blocks::parse_module_with_recovery(&lines, file.text(), file_id, base)
}
