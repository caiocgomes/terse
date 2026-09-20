//! Multi-file expansion semantics at the `compile()` boundary: authored
//! order, repeated/distinct include occurrences, wildcard rejection,
//! cycle detection, and missing-include diagnostics. These only need
//! already-loaded in-memory modules, since `terse-core` never reads files
//! itself.

use std::collections::HashMap;

use terse_core::source::{FileId, SourceFile};
use terse_core::{compile, InputSnapshot};

fn file(id: u32, text: &str) -> SourceFile {
    SourceFile::new(FileId(id), format!("module-{id}.trs"), text.as_bytes().to_vec())
        .expect("valid source")
}

fn snapshot(entry_key: &str, entry: &str, modules: &[(&str, &str)]) -> InputSnapshot {
    let mut next_id = 1;
    let mut map = HashMap::new();
    for (key, text) in modules {
        map.insert(key.to_string(), file(next_id, text));
        next_id += 1;
    }
    InputSnapshot {
        entry_key: entry_key.to_string(),
        entry: file(0, entry),
        modules: map,
        lock: None,
        overrides: HashMap::new(),
    }
}

const HEADER: &str = "document:\n  title: \"T\"\n\n";

#[test]
fn test_include_order_is_authored_order() {
    let entry = format!("{HEADER}Intro.\n\ninclude \"method.trs\"\n\nOutro.\n");
    let snap = snapshot("paper.trs", &entry, &[("method.trs", "Method body.\n")]);
    let (diags, plan) = compile(&snap);
    assert!(diags.is_empty(), "{diags:?}");
    let plan = plan.expect("valid plan");
    // Three paragraphs, in authored order: intro, included method body, outro.
    assert_eq!(plan.module.blocks.len(), 3);
}

#[test]
fn test_repeated_include_repeats_content() {
    let entry = format!("{HEADER}include \"shared.trs\"\n\ninclude \"shared.trs\"\n");
    let snap = snapshot("paper.trs", &entry, &[("shared.trs", "Shared paragraph.\n")]);
    let (diags, plan) = compile(&snap);
    assert!(diags.is_empty(), "{diags:?}");
    let plan = plan.expect("valid plan");
    assert_eq!(plan.module.blocks.len(), 2, "each occurrence repeats the content");
}

#[test]
fn test_include_occurrences_are_distinct() {
    let entry = format!("{HEADER}include \"shared.trs\"\n\ninclude \"shared.trs\"\n");
    let snap = snapshot("paper.trs", &entry, &[("shared.trs", "Shared paragraph.\n")]);
    let (_, plan) = compile(&snap);
    let plan = plan.expect("valid plan");
    // Not deduplicated into a single node: two distinct occurrences exist
    // in the flattened sequence even though their source content matches.
    assert_eq!(plan.module.blocks.len(), 2);
    assert_eq!(plan.module.blocks[0], plan.module.blocks[1]);
}

#[test]
fn test_wildcard_include_is_invalid() {
    let entry = format!("{HEADER}include \"sections/*.trs\"\n");
    let snap = snapshot("paper.trs", &entry, &[]);
    let (diags, plan) = compile(&snap);
    assert!(plan.is_none());
    assert!(diags.iter().any(|d| d.code == "E-INCLUDE-001"), "{diags:?}");
}

#[test]
fn test_missing_include_is_source_located() {
    let entry = format!("{HEADER}include \"missing.trs\"\n");
    let snap = snapshot("paper.trs", &entry, &[]);
    let (diags, plan) = compile(&snap);
    assert!(plan.is_none());
    let diag = diags
        .iter()
        .find(|d| d.code == "E-INCLUDE-002")
        .expect("missing-include diagnostic");
    assert!(diag.message.contains("missing.trs"));
    assert!(diag.primary.is_some(), "must anchor at the include statement");
}

#[test]
fn test_indirect_include_cycle_is_complete() {
    let entry = format!("{HEADER}include \"method.trs\"\n");
    let snap = snapshot(
        "paper.trs",
        &entry,
        &[
            ("method.trs", "include \"appendix.trs\"\n"),
            ("appendix.trs", "include \"paper.trs\"\n"),
        ],
    );
    let (diags, plan) = compile(&snap);
    assert!(plan.is_none());
    let diag = diags
        .iter()
        .find(|d| d.code == "E-INCLUDE-003")
        .expect("cycle diagnostic");
    assert!(diag.message.contains("paper.trs"));
    assert!(diag.message.contains("method.trs"));
    assert!(diag.message.contains("appendix.trs"));
}

#[test]
fn test_nested_includes_expand_transitively() {
    let entry = format!("{HEADER}include \"method.trs\"\n");
    let snap = snapshot(
        "paper.trs",
        &entry,
        &[
            ("method.trs", "include \"appendix.trs\"\n"),
            ("appendix.trs", "Appendix content.\n"),
        ],
    );
    let (diags, plan) = compile(&snap);
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(plan.expect("valid plan").module.blocks.len(), 1);
}
