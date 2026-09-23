use crate::references::lock::{AdapterIdentity, DeclaredIdentity, LockEntry, LockFile, ProviderKind, ResolvedIdentity};
use crate::references::record::{NormalizedRecord, WorkType};
use crate::semantic::{Affiliation, Citation, LocatorKind, NodeKind, TheoremKind};
use crate::source::{FileId, SourceFile};
use crate::syntax::inlines::Inline;
use crate::{compile, InputSnapshot};

/// A minimal already-resolved lock entry for the `robins1986` alias used
/// across these single-module node-coverage tests; citation binding
/// (group 13) requires a matching lock entry for a cited alias, which
/// these tests do not otherwise exercise.
fn locked_robins1986() -> LockFile {
    let record = NormalizedRecord {
        title: Some("A Study".to_string()),
        work_type: Some(WorkType::JournalArticle),
        anonymous: true,
        ..Default::default()
    };
    let mut lock = LockFile::default();
    lock.entries.insert(
        "robins1986".to_string(),
        LockEntry {
            declared: DeclaredIdentity { provider: ProviderKind::Doi, identifier: "10.1000/abc".to_string() },
            resolved: ResolvedIdentity { provider: ProviderKind::Doi, identifier: "10.1000/abc".to_string(), version: None },
            adapter: AdapterIdentity { name: "doi".to_string(), version: "1".to_string() },
            provider_data: record.clone(),
            override_patch: None,
            effective: record,
        },
    );
    lock
}

#[test]
fn test_compiler_core_has_no_effects() {
    let bytes = b"document:\n  title: \"Snapshot Paper\"\n\n# Introduction\n\nHello world.\n".to_vec();
    let file = SourceFile::new(FileId(0), "entry.trs", bytes).expect("valid source");
    let snapshot = InputSnapshot::single(file);

    // compile() takes only in-memory data and returns only in-memory data:
    // it has no filesystem, process, network, or clock handle to use.
    let (diags1, plan1) = compile(&snapshot);
    let (diags2, plan2) = compile(&snapshot);

    assert!(diags1.is_empty(), "unexpected diagnostics: {diags1:?}");
    assert!(diags2.is_empty(), "unexpected diagnostics: {diags2:?}");

    let plan1 = plan1.expect("valid artifact plan");
    let plan2 = plan2.expect("valid artifact plan");

    // Same input snapshot compiled twice produces identical planned content.
    assert_eq!(plan1, plan2);
    assert_eq!(
        plan1.module.metadata.as_ref().map(|m| m.title.as_str()),
        Some("Snapshot Paper")
    );
}

#[test]
fn test_all_mvp_nodes_are_typed() {
    let src = concat!(
        "document:\n",
        "  title: \"All Elements\"\n",
        "  authors:\n",
        "    - name: \"Org Author\"\n",
        "      affiliations: [\"Terse Foundation\", \"Open Source Collective\"]\n",
        "  abstract:\n",
        "    An abstract citing @robins1986.\n",
        "\n",
        "refs:\n",
        "  robins1986: doi:10.1000/abc\n",
        "\n",
        "# Heading\n",
        "\n",
        "A paragraph with a footnote^[note] and {ref: eq-main}.\n",
        "\n",
        "- unordered item\n",
        "\n",
        "1. ordered item\n",
        "\n",
        "math [id: eq-main]:\n",
        "  E = m c^2\n",
        "\n",
        "figure \"a.pdf\" [id: fig-1, role: wide]:\n",
        "  caption: \"A figure\"\n",
        "  alt: \"Descriptive alt text\"\n",
        "\n",
        "table [id: tbl-1]:\n",
        "  caption: \"A table\"\n",
        "  header: [\"A\", \"B\"]\n",
        "  rows:\n",
        "    - [\"1\", \"2\"]\n",
        "\n",
        "theorem [id: thm-1]:\n",
        "  A statement.\n",
        "\n",
        "proof [of: thm-1]:\n",
        "  See [@robins1986, p. 1].\n",
    );

    let bytes = src.as_bytes().to_vec();
    let file = SourceFile::new(FileId(0), "entry.trs", bytes).unwrap();
    let mut snapshot = InputSnapshot::single(file);
    snapshot.lock = Some(locked_robins1986());
    let (diags, plan) = compile(&snapshot);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    let module = plan.unwrap().module;

    // Metadata: organizational (multi-form) affiliations retained.
    let metadata = module.metadata.expect("document metadata");
    assert_eq!(
        metadata.authors[0].affiliation,
        Some(Affiliation::Multiple(vec![
            "Terse Foundation".to_string(),
            "Open Source Collective".to_string()
        ]))
    );
    assert!(matches!(&metadata.abstract_blocks[0][1], Inline::Citation(Citation::Narrative(_))));

    assert_eq!(module.references.len(), 1);
    assert_eq!(module.references[0].alias, "robins1986");

    // Every MVP block kind has a typed representation.
    let kinds: Vec<&str> = module
        .blocks
        .iter()
        .map(|n| match &n.kind {
            NodeKind::Heading { .. } => "heading",
            NodeKind::Paragraph { .. } => "paragraph",
            NodeKind::List { .. } => "list",
            NodeKind::Equation { .. } => "equation",
            NodeKind::Figure { .. } => "figure",
            NodeKind::Table { .. } => "table",
            NodeKind::TheoremLike { .. } => "theorem",
            NodeKind::Proof { .. } => "proof",
            NodeKind::RawTex { .. } => "raw_tex",
            NodeKind::Bibliography => "bibliography",
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            "heading", "paragraph", "list", "list", "equation", "figure", "table", "theorem", "proof",
            "bibliography",
        ],
        "the module's own citation (asserted below) has no explicit 'bibliography' marker, so \
         one is derived and appended at the end (task 12.5)"
    );

    // Paragraph footnote and cross-reference are distinct typed inlines.
    match &module.blocks[1].kind {
        NodeKind::Paragraph { inlines } => {
            assert!(inlines.iter().any(|i| matches!(i, Inline::Footnote(_))));
            assert!(inlines.iter().any(|i| matches!(i, Inline::CrossRef(id) if id == "eq-main")));
        }
        other => panic!("expected paragraph, got {other:?}"),
    }

    // Figure alt text is retained even though it has no visual rendering.
    match &module.blocks[5].kind {
        NodeKind::Figure { alt, role, .. } => {
            assert_eq!(alt, "Descriptive alt text");
            assert_eq!(role.as_deref(), Some("wide"));
        }
        other => panic!("expected figure, got {other:?}"),
    }

    // Theorem kind and proof relationship, with a located citation inside
    // the proof body.
    match &module.blocks[7].kind {
        NodeKind::TheoremLike { kind, .. } => assert_eq!(*kind, TheoremKind::Theorem),
        other => panic!("expected theorem, got {other:?}"),
    }
    match &module.blocks[8].kind {
        NodeKind::Proof { of, body, .. } => {
            assert_eq!(of.as_deref(), Some("thm-1"));
            match &body[0].kind {
                NodeKind::Paragraph { inlines } => {
                    let group = inlines
                        .iter()
                        .find_map(|i| match i {
                            Inline::Citation(Citation::Group(items)) => Some(items),
                            _ => None,
                        })
                        .expect("a citation group");
                    assert_eq!(group[0].locator.as_ref().unwrap().kind, Some(LocatorKind::Page));
                }
                other => panic!("expected paragraph, got {other:?}"),
            }
        }
        other => panic!("expected proof, got {other:?}"),
    }

    // Only math/raw nodes carry TeX payload bytes; everything else is
    // typed structured data (spot-check the equation).
    match &module.blocks[4].kind {
        NodeKind::Equation { id, payload, .. } => {
            assert_eq!(id.as_deref(), Some("eq-main"));
            assert!(payload.contains("E = m c^2"));
        }
        other => panic!("expected equation, got {other:?}"),
    }
}

#[test]
fn test_furniture_excluded_from_authored_ast() {
    // Theme resolution never receives or returns a `ParsedModule`; it only
    // produces a `ResolvedTheme` of typed presentation values. Furniture
    // (watermark, logo, header/footer, contents) has no representation in
    // `NodeKind` at all, so it structurally cannot enter the authored node
    // sequence regardless of which theme is active.
    let bytes = b"document:\n  title: \"Furniture Test\"\n\nBody paragraph.\n".to_vec();
    let file = SourceFile::new(FileId(0), "entry.trs", bytes).expect("valid source");
    let snapshot = crate::InputSnapshot::single(file);
    let (_diags_a, plan_a) = crate::compile(&snapshot);
    let (_diags_b, plan_b) = crate::compile(&snapshot);
    let module_a = plan_a.expect("compiles").module;
    let module_b = plan_b.expect("compiles").module;
    // Resolving two different themes in between changes nothing about the
    // already-lowered module: it is immutable input to generation.
    let _academic = crate::theme::academic();
    let magalu_src = SourceFile::new(
        FileId(1),
        "magalu.theme",
        b"watermark:\n  kind: internal-use\n  opacity: 0.1\n".to_vec(),
    )
    .unwrap();
    let _magalu = crate::theme::resolve_theme("magalu", &magalu_src).expect("resolves");
    assert_eq!(module_a, module_b);
    assert_eq!(module_a.blocks.len(), 1);
}

#[test]
fn test_invalid_binding_blocks_artifact_plan() {
    // An unknown cross-reference id never produces a successful plan.
    let unknown_id = b"document:\n  title: \"T\"\n\nSee {ref: nowhere}.\n".to_vec();
    let file = SourceFile::new(FileId(0), "entry.trs", unknown_id).expect("valid source");
    let (diags, plan) = compile(&InputSnapshot::single(file));
    assert!(plan.is_none(), "an unresolved cross-reference must never emit a plan");
    assert!(!diags.is_empty());
    assert_eq!(diags[0].code, "E-XREF-001");

    // An unknown citation alias (no matching `refs:` declaration) also
    // blocks the plan.
    let unknown_citation = b"document:\n  title: \"T\"\n\nSee @never-declared for details.\n".to_vec();
    let file = SourceFile::new(FileId(0), "entry.trs", unknown_citation).expect("valid source");
    let (diags, plan) = compile(&InputSnapshot::single(file));
    assert!(plan.is_none(), "an unresolved citation alias must never emit a plan");
    assert!(!diags.is_empty());
    assert_eq!(diags[0].code, "E-CITE-001");

    // A proof `of` naming an unknown id also blocks the plan, without
    // emitting a partial successful document alongside the diagnostic.
    let unknown_proof_target =
        b"document:\n  title: \"T\"\n\nproof [of: nowhere]:\n  Body.\n".to_vec();
    let file = SourceFile::new(FileId(0), "entry.trs", unknown_proof_target).expect("valid source");
    let (diags, plan) = compile(&InputSnapshot::single(file));
    assert!(plan.is_none());
    assert!(!diags.is_empty());
    assert_eq!(diags[0].code, "E-PROOF-001");
}

#[test]
fn test_theme_invariant_projection() {
    // Setup: a module compiled once, its projection is theme-agnostic by
    // construction (themes aren't part of `compile()`'s inputs at all).
    // We simulate "the same content compiled twice" by compiling the same
    // bytes twice and asserting the projections (and digests) are equal,
    // which is the property that makes it safe for `terse-cli` to build
    // under two themes and still expect identical projections.
    let bytes = b"document:\n  title: \"T\"\n\n# Intro {id: intro}\n\nHello *world*.\n".to_vec();
    let file_a = SourceFile::new(FileId(0), "entry.trs", bytes.clone()).expect("valid source");
    let file_b = SourceFile::new(FileId(1), "entry.trs", bytes).expect("valid source");
    let (_, plan_a) = compile(&InputSnapshot::single(file_a));
    let (_, plan_b) = compile(&InputSnapshot::single(file_b));
    let proj_a = { let p = plan_a.expect("plan"); crate::semantic::projection::project(&p.module, &p.bindings.authorized) };
    let proj_b = { let p = plan_b.expect("plan"); crate::semantic::projection::project(&p.module, &p.bindings.authorized) };

    // Assertion: equal authored content projects identically even though
    // the two modules came from distinct FileIds (excluded as incidental).
    assert_eq!(proj_a, proj_b);
    assert_eq!(
        crate::semantic::projection::digest(&proj_a),
        crate::semantic::projection::digest(&proj_b)
    );

    // Edge case: differing authored content must project differently.
    let other = b"document:\n  title: \"T\"\n\n# Intro {id: intro}\n\nGoodbye *world*.\n".to_vec();
    let file_c = SourceFile::new(FileId(2), "entry.trs", other).expect("valid source");
    let (_, plan_c) = compile(&InputSnapshot::single(file_c));
    let proj_c = { let p = plan_c.expect("plan"); crate::semantic::projection::project(&p.module, &p.bindings.authorized) };
    assert_ne!(proj_a, proj_c);
    assert_ne!(
        crate::semantic::projection::digest(&proj_a),
        crate::semantic::projection::digest(&proj_c)
    );
}

#[test]
fn test_resource_limits_fail_boundedly() {
    // Setup: a heading nesting depth far beyond any real document, built
    // via deeply nested list items (the only authored construct that can
    // recurse arbitrarily in a single module without includes).
    let mut src = String::from("document:\n  title: \"T\"\n\n");
    src.push_str("- top\n");
    for depth in 0..100 {
        src.push_str(&format!("{}- item{}\n", "  ".repeat(depth + 1), depth));
    }
    let file = SourceFile::new(FileId(0), "entry.trs", src.into_bytes()).expect("valid source");

    // Assertion: compilation fails with an explicit resource-limit
    // diagnostic rather than a stack overflow, an accepted-but-huge plan,
    // or a silent truncation that still reports success.
    let (diags, plan) = compile(&InputSnapshot::single(file));
    assert!(plan.is_none(), "a document exceeding the nesting limit must never produce a plan");
    assert!(!diags.is_empty());
    assert_eq!(diags[0].code, "E-LIMIT-001");

    // The requirement names four limit categories; nesting above is one.
    // Each of the remaining three must fail with its own code and never
    // yield a plan, so that a pathological input is a bounded, explained
    // refusal rather than an unbounded allocation or a stack overflow.

    // Source bytes: rejected at the single chokepoint where bytes enter
    // the core, before any parsing work is attempted.
    let oversized = vec![b'x'; crate::source::MAX_SOURCE_BYTES + 1];
    let err = SourceFile::new(FileId(0), "huge.trs", oversized)
        .expect_err("a source past the byte cap must not load");
    assert_eq!(err.into_diagnostic(FileId(0)).code, "E-LIMIT-003");

    // Node count: a module whose block count exceeds the cap.
    let mut many = String::from("document:\n  title: \"T\"\n\n");
    for i in 0..(crate::semantic::MAX_NODES + 1) {
        many.push_str(&format!("p{i}\n\n"));
    }
    let file = SourceFile::new(FileId(0), "many.trs", many.into_bytes()).expect("valid source");
    let (diags, plan) = compile(&InputSnapshot::single(file));
    assert!(plan.is_none(), "a document exceeding the node limit must never produce a plan");
    assert_eq!(diags[0].code, "E-LIMIT-004");

    // Include depth: a chain one level past the cap fails, and a chain one
    // level below it succeeds, so the boundary is exercised from both
    // sides rather than only from the failing one.
    assert_eq!(include_chain_codes(crate::project::expand::MAX_INCLUDE_DEPTH + 1), vec!["E-LIMIT-005"]);
    assert!(
        include_chain_codes(crate::project::expand::MAX_INCLUDE_DEPTH - 1).is_empty(),
        "a chain just below the include-depth cap must still compile"
    );
}

/// Builds an entry that includes `depth` chained modules and returns the
/// diagnostic codes compiling it produces (empty on success).
fn include_chain_codes(depth: usize) -> Vec<String> {
    use std::collections::HashMap;

    let mut modules: HashMap<String, SourceFile> = HashMap::new();
    let entry_src = "document:\n  title: \"T\"\n\ninclude \"m0.trs\"\n".to_string();
    let entry = SourceFile::new(FileId(0), "paper.trs", entry_src.into_bytes()).expect("valid entry");
    for i in 0..depth {
        let body = if i + 1 < depth {
            format!("include \"m{}.trs\"\n", i + 1)
        } else {
            "leaf paragraph\n".to_string()
        };
        let file = SourceFile::new(FileId((i + 1) as u32), format!("m{i}.trs"), body.into_bytes())
            .expect("valid module");
        modules.insert(format!("m{i}.trs"), file);
    }
    let snapshot = InputSnapshot {
        entry_key: "paper.trs".to_string(),
        entry,
        modules,
        lock: None,
        overrides: HashMap::new(),
    };
    let (diags, _plan) = compile(&snapshot);
    diags.into_iter().map(|d| d.code.to_string()).collect()
}

#[test]
fn test_projection_retains_effective_bibliography() {
    // Two documents identical in authored content whose cited work has
    // different effective (locked) metadata must project differently and
    // digest differently: the bibliography a reader sees is part of the
    // document's meaning, so a projection that omits it would call two
    // materially different papers equal.
    let src = "document:\n  title: \"T\"\n\nrefs:\n  robins1986: doi:10.1000/abc\n\nCited @robins1986 here.\n\nbibliography\n";

    let lock_a = locked_robins1986();
    let mut lock_b = locked_robins1986();
    if let Some(entry) = lock_b.entries.get_mut("robins1986") {
        // Both the provider data and the sealed effective record move
        // together: binding recomputes `effective` from `provider_data`
        // plus any patch, so editing only one of them changes nothing.
        let title = Some("A Completely Different Title".to_string());
        entry.provider_data.title = title.clone();
        entry.effective.title = title;
    }

    let project_with = |lock: LockFile| {
        let file = SourceFile::new(FileId(0), "p.trs", src.as_bytes().to_vec()).expect("valid");
        let mut snapshot = InputSnapshot::single(file);
        snapshot.lock = Some(lock);
        let (diags, plan) = compile(&snapshot);
        assert!(diags.is_empty(), "{diags:?}");
        let plan = plan.expect("plan");
        let projection = crate::semantic::projection::project(&plan.module, &plan.bindings.authorized);
        let digest = crate::semantic::projection::digest(&projection);
        (projection, digest)
    };

    let (proj_a, digest_a) = project_with(lock_a);
    let (proj_b, digest_b) = project_with(lock_b);

    assert_ne!(proj_a, proj_b, "differing effective bibliography must project differently");
    assert_ne!(digest_a, digest_b, "differing effective bibliography must digest differently");
}

#[test]
fn test_projection_distinguishes_numbered_equations() {
    let project_src = |text: &str| {
        let file = SourceFile::new(FileId(0), "entry.trs", text.as_bytes().to_vec())
            .expect("valid source");
        let (_, plan) = compile(&InputSnapshot::single(file));
        let p = plan.expect("plan");
        crate::semantic::projection::project(&p.module, &p.bindings.authorized)
    };
    let numbered = project_src("document:\n  title: \"T\"\n\nmath:\n  x = y\n");
    let unnumbered = project_src("document:\n  title: \"T\"\n\n$$\nx = y\n$$\n");
    assert_ne!(numbered, unnumbered);
    assert_ne!(
        crate::semantic::projection::digest(&numbered),
        crate::semantic::projection::digest(&unnumbered)
    );
    let eqs = |p: &crate::semantic::projection::Projection| format!("{p:?}");
    assert!(
        eqs(&numbered).contains("numbered: true, payload: \"x = y\""),
        "{}",
        eqs(&numbered)
    );
    assert!(
        eqs(&unnumbered).contains("numbered: false, payload: \"x = y\""),
        "{}",
        eqs(&unnumbered)
    );
}
