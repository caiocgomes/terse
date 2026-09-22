## Test Strategy

This is the planned test contract for the personal open-source Terse project, written before implementation. It maps all 146 scenarios in the ten capability specs to concrete named tests. None of the test files, harnesses, fixtures, or executable results below is claimed to exist yet. The completed artifacts are the [design](design.md) and the [capability specs](specs/); implementation will create these tests before the corresponding behavior.

### Framework and conventions

There is no existing Cargo workspace, test suite, project.md, or openspec/project.md in the repository. Follow the Rust workspace selected in the design: the standard Rust test runner with table-driven #[test] cases, a terse-core library, and a terse-cli binary/application layer. Use small shared fixture/process/clock helpers where multiple tests need the same boundary; do not introduce another testing framework without a concrete need.

Unit tests exercise isolated parsing, lowering, normalization, serialization, or scheduler behavior; they live in the indicated module test files. Integration tests exercise multiple real components, temporary project directories, CLI argument handling, and filesystem/process/provider contracts. E2E tests execute the installed/built CLI or the documented public workflow and real compatible LaTeX tools. Where an E2E case needs controlled provider resolution, inject the recorded transport into the same CLI application entrypoint used by the binary, then run the actual binary for the subsequent offline build. Do not add a production network-URL override solely to make tests convenient.

Declare module test files from their owning module under cfg(test). A single planned crates/terse-cli/tests/e2e.rs integration-test entrypoint must include every e2e/*.rs module; nested files alone are not discoverable test targets. Each name below denotes a concrete test function. Edge cases are required table entries or assertions within that test, or separately named tests retaining the same traceability entry. Assert public behavior and independently prepared expected values, not private function shape or a second copy of the production algorithm.

The initial distribution implementation remains Rust/XeLaTeX/BibLaTeX/Biber. No test may silently choose another engine, fetch missing packages, replace document content for a theme, or change the user's license selection.

### Fixtures and observable assertions

Create shared, committed, redistributable inputs under tests/fixtures/ during implementation:

| Fixture group | Required contents and purpose |
| --- | --- |
| minimal | Valid entry, academic theme, manifest, empty versioned lock, and engine-free init/check/build baseline. |
| full-paper | Multi-file paper with every MVP element, all metadata, three heading levels, forward/back typed references, DOI/arXiv locked data, figure/table, and academic plus unofficial magalu themes. |
| raw-tex | Separate raw math/drawing/support examples for normal builds, strict diagnostics, and explicit export rejection. The main export fixture remains semantic-only. |
| invalid | Small focused malformed syntax, metadata, IDs, include cycles, paths, themes, locks, and override cases with independent expected positions/codes. |
| providers | Recorded CSL JSON and Atom responses, requested identifiers, normalized expected records, mismatch/error/DTD/redirect cases, and provenance notes. No live requests in required tests. |
| expected | Small readable TeX/Bib/style/map/diagnostic/formatter byte goldens, explicit semantic/text/link expectations, and pinned-render visual baselines. |
| engine-logs | Known engine/Biber errors, warnings, missing glyphs, unresolved references, recorder files, and generated-to-source mapping expectations. |

Use unique sentinel content for authored elements and separate expectations for derived repeated furniture. A compiler-produced semantic digest is useful evidence but is insufficient alone: compare structures and independently listed authored sequences/text. Review small golden updates as behavioral changes; never accept all regenerated snapshots simply because output changed.

Capture before/after bytes or content hashes for every authoritative file and previously published artifact in no-write/rollback tests. Independently enumerate expected artifact membership; do not use the production artifact plan as the only oracle for ZIP membership. Verify archive extraction paths, byte sizes, hashes, and forbidden extra members. Compare the full published text set for determinism; externally produced BBL comparisons require the pinned toolchain, and PDF byte identity is excluded.

For real PDFs, the pinned test environment supplies text extraction, hyperlink/annotation inspection, rasterization, and region-comparison helpers. Check meaningful numbers/destinations and visible content regions, not only file existence or a successful exit. Compare reviewed expected text and stable visual regions under the pinned fonts/toolchain; distinguish float/page coordinates and derived headers from authored order. Empty/all-white content regions must fail even if invisible text can be extracted. The maintainer also visually inspects both theme PDFs before release; this is supplementary evidence, not a replacement for automated cases or a requirement for a separate reviewer.

### Controlled side effects and failure testing

- Resolve bibliographic metadata through recorded/injected HTTP transport and a fake monotonic clock. Assert request count, identity, allowed redirect targets, response limits, sequential arXiv scheduling, retry bounds, and zero calls from ordinary compilation. Live provider smoke tests are optional and excluded from required acceptance.
- Engine-free tests supply missing/rejecting process capabilities or an isolated tool search path. Runner-based tests retain real application validation/publication while returning controlled process outcomes and logs. Real-engine tests independently verify the conventional command sequence and resulting PDF/bibliography.
- Engine/package isolation tests run in the pinned Linux environment with network disabled, an empty user TeX tree, controlled search paths, and only generated deliverables mounted as document inputs. Terse is absent from the standalone recompilation environment. Recorder paths must belong to the bundle or verified distribution resources.
- Watch scheduler tests use fake time; filesystem/process tests wait on explicit attempt/readiness/publication events with bounded deadlines. Do not use arbitrary sleeps as correctness assertions. On timeout retain attempt records, child state, and logs for diagnosis.
- Transaction tests inject failures at stage/rename/publication boundaries and simulate restart recovery; supplement with real directory/file operations on supported operating systems. Assertions concern consistent generations and untouched unrelated files, not a specific private journaling implementation.
- Use temporary test roots and harmless sentinel files. Never run hostile raw TeX to test math validation: rejected math is validated before a runner is available. Any normal raw-TeX compilation uses trusted fixture content.
- Actual platform-sensitive symlink/junction, path, watcher, and process tests run on capable OS jobs; portable fake-path tests supplement them. An unsupported runner feature is reported explicitly and cannot be counted as passing platform coverage.

### Execution tiers and red-green workflow

Once the workspace and tests are implemented, the planned commands are:

```text
cargo test --workspace --locked
cargo test --locked -p terse-cli --test e2e -- --ignored
```

The first command runs the required engine-free unit/integration suite across Linux, macOS, and Windows. Heavy E2E tests have an explicit ignore reason in the default lane and are all run by the second command inside the single pinned Linux LaTeX environment. That lane preflights its tools and fails if they are missing; it never silently skips cases or reports static-only export as successful compiled acceptance. The release harness checks expected test discovery and executed counts so an unregistered module, ignored required case, or empty test run fails validation.

Network is disabled for compiler/toolchain acceptance after dependency/image provisioning. The pinned environment definition, dependency/tool versions, local reproduction scripts, and public fixture inputs are checked in during implementation. The maintainer can run the same scripts locally as CI. Ordinary contributors can begin with the engine-free lane; supported executable packaging/smoke tests still run on the documented OS matrix.

For each end-to-end implementation milestone, first add the mapped tests and fixtures, run them to demonstrate the intended behavioral failure (Red), implement the feature, then rerun the relevant tests (Green). A compile error from a missing test harness or fixture is not adequate behavioral Red evidence. Preserve the original baseline expectations during fixes. Run the broader required suite at milestone/release boundaries, not repeatedly after unrelated documentation-only changes. Supplementary fuzzing can grow later; all deterministic regression cases and A–L acceptance gates in this plan remain required.

Milestones use the design's sequence: first portable page; one paper/two themes; multi-file scholarly paper; Git/editing workflow; export/release. Partial milestone completion does not satisfy first-release acceptance. The license remains the author's decision; release metadata tests verify the eventual choice and bundled notices without selecting a license automatically.

## Spec-to-Test Mapping

All file paths below are future implementation targets relative to the repository root. Scenario headings match their source spec exactly. Each scenario is mapped to one primary named test plus its required edge cases; shared fixtures do not replace distinct behavioral assertions.

### Capability: language-parsing

Source: [language-parsing spec](specs/language-parsing/spec.md).

#### Scenario: Equivalent line endings

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_line_endings_preserve_semantics`
- **Setup (GIVEN)**: The minimal document in LF/CRLF, with/without BOM and final newline.
- **Action (WHEN)**: Parse each byte fixture.
- **Assert (THEN)**: Equivalent nodes and exact original byte spans; no BOM text node.
- **Edge cases**: Reject invalid UTF-8 and bare CR; CRLF inside raw blocks remains opaque.

#### Scenario: Invalid indentation

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_invalid_structural_indentation`
- **Setup (GIVEN)**: Nested proof fixtures using tabs, three spaces, and an unopened dedent.
- **Action (WHEN)**: Parse each invalid fixture.
- **Assert (THEN)**: Failure at the offending line, no successful module.
- **Edge cases**: Valid two-space nesting and blank indented lines remain accepted.

#### Scenario: Reserved text is explicitly escaped

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_escaped_reserved_start_is_prose`
- **Setup (GIVEN)**: A paragraph beginning with literal \\include and another beginning with escaped figure.
- **Action (WHEN)**: Parse and lower the paragraphs.
- **Assert (THEN)**: Text starts with the reserved word; no include/figure node or dependency.
- **Edge cases**: Unescaped valid headers still create structural nodes.

#### Scenario: Malformed header is diagnosed

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_malformed_reserved_header_fails`
- **Setup (GIVEN)**: A figure header without a quoted path, plus malformed math/include headers.
- **Action (WHEN)**: Parse each module.
- **Assert (THEN)**: Malformed-header diagnostic at the header; no prose fallback.
- **Edge cases**: Quoted paths with spaces and JSON escapes parse correctly.

#### Scenario: Full metadata survives parsing

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_all_metadata_fields_survive`
- **Setup (GIVEN)**: Metadata with subtitle, two authors, both supported affiliation forms on different authors, explicit date, pt-BR, abstract, and keywords.
- **Action (WHEN)**: Parse and inspect typed field values.
- **Assert (THEN)**: Exact values/order retained, including Unicode names and abstract inlines.
- **Edge cases**: Absent date stays absent; absent language is en; unsupported locale diagnosed during validation.

#### Scenario: Invalid metadata placement or fields

- **Test type**: integration
- **Test file**: `crates/terse-core/tests/language_parsing.rs`
- **Test name**: `test_invalid_metadata_context`
- **Setup (GIVEN)**: Entry/include projects with included document metadata, missing title, duplicate fields, and both affiliation forms on one author.
- **Action (WHEN)**: Resolve and validate each project.
- **Assert (THEN)**: Each invalid declaration fails at its original location with an applicable correction.
- **Edge cases**: Reject metadata after content and unsupported abstract headings/declarations.

#### Scenario: Mixed inline content

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_mixed_inline_paragraph`
- **Setup (GIVEN)**: A wrapped paragraph with emphasis, strong text, balanced-parenthesis URL, multi-backtick code, footnote, math, and reference.
- **Action (WHEN)**: Parse and lower the paragraph.
- **Assert (THEN)**: Expected inline kinds/order and semantic spaces; code/math preserve literal @ and delimiter text.
- **Edge cases**: Test escaped punctuation, word-internal asterisks, and allowed nested emphasis.

#### Scenario: Ambiguous delimiters are rejected

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_crossing_and_nested_delimiters_fail`
- **Setup (GIVEN)**: Cases with crossing delimiters, nested footnotes/links, unclosed code, unknown backslash escape, and unescaped triple asterisks.
- **Action (WHEN)**: Parse each case.
- **Assert (THEN)**: Localized delimiter/escape error; no successful lossy parse.
- **Edge cases**: Whitespace-only span contents rejected; escaped delimiter text accepted.

#### Scenario: Nested lists and three heading levels

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_headings_and_nested_lists`
- **Setup (GIVEN)**: Three heading levels and ordered items 9, 10, 11 containing nested unordered lists and wrapped/continued paragraphs.
- **Action (WHEN)**: Parse and inspect block trees.
- **Assert (THEN)**: Heading levels, ordered start, item order, and nesting match an independently written expected tree.
- **Edge cases**: Continuation indentation is independent of marker width; a marker-kind change starts a new list.

#### Scenario: Nonsequential ordered markers

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_ordered_markers_are_sequential`
- **Setup (GIVEN)**: One list with markers 3 then 5; valid counterpart 3 then 4.
- **Action (WHEN)**: Parse/validate both fixtures.
- **Assert (THEN)**: Invalid fixture identifies second marker and expected 4; valid start 3 retained.
- **Edge cases**: Zero/negative starts and unsupported blocks inside items fail.

#### Scenario: Theorem and proof from the authoring model

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_theorem_proof_equation_structure`
- **Setup (GIVEN)**: The brief's titled Transfer theorem, proof, and identified derivative equation; variants for every theorem-like kind.
- **Action (WHEN)**: Parse and lower each fixture.
- **Assert (THEN)**: Distinct ordered nodes with exact title/ID/math payload.
- **Edge cases**: Proof of attribute retained; no relationship inferred for an adjacent proof lacking of.

#### Scenario: Unsupported nested structure

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_nested_module_structures_fail`
- **Setup (GIVEN)**: A theorem containing a heading, include, refs declaration, or bibliography marker.
- **Action (WHEN)**: Validate each nested structure.
- **Assert (THEN)**: Placement diagnostic at nested header; no relocation to module scope.
- **Edge cases**: Nested proof/theorem and supported list/equation bodies remain valid.

#### Scenario: Figure and table preserve their meaning

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_figure_table_fields_are_semantic`
- **Setup (GIVEN)**: Wide figure and two-column table, with quoted comma/bracket cell text and reordered field declarations.
- **Action (WHEN)**: Parse and inspect fields.
- **Assert (THEN)**: Caption, plain alt, role, IDs and cell/header/row order match expected values.
- **Edge cases**: Multiline figure caption/alt accepted; numeric-looking cells stay text.

#### Scenario: Invalid figure or table

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_missing_figure_fields_and_ragged_tables`
- **Setup (GIVEN)**: Figures missing caption/alt and tables with empty rows, missing header/caption, ragged rows, or nested block cells.
- **Action (WHEN)**: Parse and validate the cases.
- **Assert (THEN)**: Precise missing-field/row error, with no valid semantic document.
- **Edge cases**: Footnotes in captions/cells and unsupported cell spans rejected.

#### Scenario: Citation forms are not conflated

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_citation_lexing_retains_intent`
- **Setup (GIVEN)**: Narrative, parenthetical, grouped/located citations, an email, and escaped @literal.
- **Action (WHEN)**: Parse the paragraph.
- **Assert (THEN)**: Expected citation types/aliases/locators/order; email and escaped alias stay text.
- **Edge cases**: Quoted/escaped locator delimiters and citation-before-link precedence tested.

#### Scenario: Declarations are explicit

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_declarations_require_module_scope`
- **Setup (GIVEN)**: Proof bodies containing refs or include, plus valid equivalent module-level declarations.
- **Action (WHEN)**: Parse/validate both forms.
- **Assert (THEN)**: Nested declarations fail and create no executable/preprocessor action; module declarations parse.
- **Edge cases**: Reserved DOI/arXiv/ISBN/URL syntax parses independently of provider availability.

#### Scenario: Theme isolation in content [Acceptance I]

- **Test type**: integration
- **Test file**: `crates/terse-core/tests/language_parsing.rs`
- **Test name**: `test_visual_attributes_rejected`
- **Setup (GIVEN)**: Parameter cases: figure width:80mm, heading font:18pt, content margin/color/placement, unknown ID attribute, invalid role.
- **Action (WHEN)**: Run project validation without engine services.
- **Assert (THEN)**: Each case fails at its attribute and names the theme-file correction; zero engine calls.
- **Edge cases**: Apply closed attribute sets to every identified node kind, not only the two examples.

#### Scenario: Semantic role is valid

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_wide_role_has_no_dimensions`
- **Setup (GIVEN)**: Figure with [id: transition, role: wide].
- **Action (WHEN)**: Parse/lower the figure.
- **Assert (THEN)**: ID and semantic role retained, with no content width or placement field.
- **Edge cases**: Unknown/duplicate roles and case variants fail according to case-sensitive grammar.

#### Scenario: Familiar mathematical notation survives

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_tex_math_bytes_are_preserved`
- **Setup (GIVEN)**: Inline and display math containing \\frac{\\partial \\dot V}{\\partial H} > 0., plus aligned/matrix/cases.
- **Action (WHEN)**: Validate then emit the math payload representation.
- **Assert (THEN)**: Accepted payload bytes equal structural-dedented input, without text escaping.
- **Edge cases**: Reject unbalanced braces and unmatched/forbidden environment endings.

#### Scenario: Math cannot execute commands

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_math_rejects_execution`
- **Setup (GIVEN)**: Direct and nested math containing input, write18, csname, macro definitions, catcode changes, unknown commands, control bytes, and ^^ encodings.
- **Action (WHEN)**: Validate without providing a process runner.
- **Assert (THEN)**: Every prohibited case fails before emission; no execution hook is available.
- **Edge cases**: Allowed commands around an unsafe nested argument do not hide it; TeX comments retain correct lexical behavior.

#### Scenario: Raw TeX escape hatch [Acceptance J]

- **Test type**: integration
- **Test file**: `crates/terse-core/tests/language_parsing.rs`
- **Test name**: `test_raw_tex_build_and_strict_warning`
- **Setup (GIVEN)**: Valid raw drawing fixture with declared support and payload byte oracle.
- **Action (WHEN)**: Generate source-only output; check with strict, then strict plus deny-warnings.
- **Assert (THEN)**: Output contains the exact dedented raw bytes; W-TEX-001 points to the block; warning policy changes exit status only.
- **Edge cases**: Normal validation accepts raw; export rejection is covered separately; EOF without newline preserved.

#### Scenario: Opaque payload includes comments and whitespace

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_raw_payload_trivia_is_opaque`
- **Setup (GIVEN)**: Raw payload containing %, //, blank lines, extra spaces, CRLF, and a following dedented paragraph.
- **Action (WHEN)**: Parse and format in memory.
- **Assert (THEN)**: Payload byte equality after structural dedenting; following paragraph is outside the raw node.
- **Edge cases**: Whitespace-only lines and leading payload tabs survive without changing the structural stack.

### Capability: semantic-ast

Source: [semantic-ast spec](specs/semantic-ast/spec.md).

#### Scenario: Every MVP element has a typed representation

- **Test type**: unit
- **Test file**: `crates/terse-core/src/semantic/tests.rs`
- **Test name**: `test_all_mvp_nodes_are_typed`
- **Setup (GIVEN)**: All-elements module plus explicit expected metadata/block/inline model.
- **Action (WHEN)**: Lower syntax to semantic nodes.
- **Assert (THEN)**: Every MVP kind and semantic field exists; only math/raw carry TeX payloads.
- **Edge cases**: Verify alt text, organizational affiliations, proof relationships, and locator kinds.

#### Scenario: Order survives include resolution

- **Test type**: integration
- **Test file**: `crates/terse-core/tests/semantic_ast.rs`
- **Test name**: `test_include_order_is_authored_order`
- **Setup (GIVEN)**: Entry has before/after paragraphs and two modules with unique sentinel sequences.
- **Action (WHEN)**: Resolve includes.
- **Assert (THEN)**: Flattened authored sequence equals an independently listed expected sequence.
- **Edge cases**: Forward references and repeated includes do not sort or deduplicate content.

#### Scenario: Included equation keeps its source location

- **Test type**: integration
- **Test file**: `crates/terse-core/tests/semantic_ast.rs`
- **Test name**: `test_included_spans_survive_generation`
- **Setup (GIVEN)**: An identified equation at a fixed byte/line in sections/method.trs, included by paper.trs.
- **Action (WHEN)**: Resolve and emit source maps.
- **Assert (THEN)**: Original equation span remains the primary origin; entry appears only in include context.
- **Edge cases**: Escaped Unicode text and line-level math mappings keep original byte boundaries.

#### Scenario: Repeated inclusion has distinct occurrence context

- **Test type**: integration
- **Test file**: `crates/terse-core/tests/semantic_ast.rs`
- **Test name**: `test_include_occurrences_are_distinct`
- **Setup (GIVEN)**: Same labelled module included twice through different routes.
- **Action (WHEN)**: Expand occurrences and inspect duplicate diagnostics.
- **Assert (THEN)**: Original spans match but occurrence IDs/routes differ deterministically.
- **Edge cases**: Repeated runs and different absolute project roots do not randomize occurrence identity.

#### Scenario: Same document under two themes [Acceptance A]

- **Test type**: unit
- **Test file**: `crates/terse-core/src/semantic/tests.rs`
- **Test name**: `test_theme_invariant_projection`
- **Setup (GIVEN)**: One resolved full paper and distinct academic/magalu settings.
- **Action (WHEN)**: Produce both style-specific plans and canonical projections.
- **Assert (THEN)**: Projection structures and digests match, including raw/math/alt/citation fields.
- **Edge cases**: A controlled authored text edit changes projection/digest; a theme-only edit does not.

#### Scenario: Presentation-only data stays outside authored content

- **Test type**: unit
- **Test file**: `crates/terse-core/src/semantic/tests.rs`
- **Test name**: `test_furniture_excluded_from_authored_ast`
- **Setup (GIVEN)**: Resolved paper and theme enabling contents, running title, numbering, watermark.
- **Action (WHEN)**: Resolve theme and emit its presentation plan.
- **Assert (THEN)**: AST is unchanged; furniture is represented only as derived presentation.
- **Edge cases**: Repeat title in a running header does not duplicate the authored title node.

#### Scenario: Compilation does not need services or effects

- **Test type**: unit
- **Test file**: `crates/terse-core/src/semantic/tests.rs`
- **Test name**: `test_compiler_core_has_no_effects`
- **Setup (GIVEN)**: Complete in-memory input snapshot with deterministic assets/lock data and no network/process/write services.
- **Action (WHEN)**: Invoke core compilation.
- **Assert (THEN)**: A valid artifact plan is returned without reading clock/environment or opening service handles.
- **Edge cases**: Run twice and compare planned byte buffers.

#### Scenario: Invalid references prevent emission

- **Test type**: unit
- **Test file**: `crates/terse-core/src/semantic/tests.rs`
- **Test name**: `test_invalid_binding_blocks_artifact_plan`
- **Setup (GIVEN)**: Snapshots containing unknown IDs and unknown/stale citations separately.
- **Action (WHEN)**: Compile each snapshot.
- **Assert (THEN)**: Relevant errors and no successful publishable plan.
- **Edge cases**: Diagnostics can accumulate without emitting a partial successful document.

#### Scenario: Excessive nesting is bounded

- **Test type**: unit
- **Test file**: `crates/terse-core/src/semantic/tests.rs`
- **Test name**: `test_resource_limits_fail_boundedly`
- **Setup (GIVEN)**: Inputs just below/above versioned nesting, byte, node, include-depth, and diagnostic-count limits.
- **Action (WHEN)**: Compile through the relevant core boundary.
- **Assert (THEN)**: Boundary input succeeds where valid; excess returns resource-limit failure without panic or publication.
- **Edge cases**: Diagnostic truncation is explicit; test limit values are read from documented configuration, not timing guesses.

### Capability: themes

Source: [themes spec](specs/themes/spec.md).

#### Scenario: Supported semantic selector

- **Test type**: unit
- **Test file**: `crates/terse-core/src/theme/tests.rs`
- **Test name**: `test_base_and_wide_selectors`
- **Setup (GIVEN)**: Valid theme defining figure and figure[role=wide].
- **Action (WHEN)**: Parse and resolve settings.
- **Assert (THEN)**: Only the matching semantic component/role receives the override.
- **Edge cases**: Reject unknown role selectors and malformed selector syntax.

#### Scenario: Theme attempts to select authored content

- **Test type**: unit
- **Test file**: `crates/terse-core/src/theme/tests.rs`
- **Test name**: `test_themes_cannot_select_content`
- **Setup (GIVEN)**: Themes with per-ID selectors, conditions, imports, prose replacement, omit/reorder rules, raw TeX, and hooks.
- **Action (WHEN)**: Validate each theme.
- **Assert (THEN)**: Each unsupported construct produces a property/selector location and no transformed content.
- **Edge cases**: Duplicate selectors/properties fail instead of introducing source-order cascade behavior.

#### Scenario: Corporate and academic presentations

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/themes.rs`
- **Test name**: `test_two_theme_presentations_differ`
- **Setup (GIVEN)**: Full paper with academic title versus unofficial magalu cover/logo/header/watermark; pinned fonts/render tools.
- **Action (WHEN)**: Compile and render both outputs.
- **Assert (THEN)**: Expected cover/furniture regions differ and both have visible authored content against reviewed visual/text oracles.
- **Edge cases**: Comparison alone is insufficient: an all-white/empty rendering must fail content-region checks.

#### Scenario: Invalid typed value

- **Test type**: unit
- **Test file**: `crates/terse-core/src/theme/tests.rs`
- **Test name**: `test_typed_theme_values_rejected`
- **Setup (GIVEN)**: Invalid figure width, citation style, page size, enum, unit, and nonfinite numeric cases.
- **Action (WHEN)**: Validate without engine access.
- **Assert (THEN)**: Property-level errors identify selector and accepted value type.
- **Edge cases**: Exercise all component schemas, including heading/title/theorem/table/header/footer.

#### Scenario: Wide figure override

- **Test type**: unit
- **Test file**: `crates/terse-core/src/theme/tests.rs`
- **Test name**: `test_role_overrides_base_width`
- **Setup (GIVEN)**: Base figure width 75%, wide width 100%, two figure roles.
- **Action (WHEN)**: Resolve style values.
- **Assert (THEN)**: Ordinary width is 75% and wide width 100%; source nodes contain neither dimension.
- **Edge cases**: Absent role override inherits base; kind-specific theorem settings inherit base theorem defaults.

#### Scenario: Reordered selectors

- **Test type**: unit
- **Test file**: `crates/terse-core/src/theme/tests.rs`
- **Test name**: `test_theme_selector_order_irrelevant`
- **Setup (GIVEN)**: Two themes with equal distinct settings in different selector orders.
- **Action (WHEN)**: Resolve both.
- **Assert (THEN)**: Equal resolved settings and canonical style output.
- **Edge cases**: Duplicate rules are rejected rather than made order-dependent.

#### Scenario: Same content under different themes [Acceptance A]

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/themes.rs`
- **Test name**: `test_same_paper_content_under_two_themes`
- **Setup (GIVEN)**: Full paper including all metadata, semantic elements, locked citations, and explicit expected content sequence.
- **Action (WHEN)**: Build/compile academic and magalu.
- **Assert (THEN)**: Equal semantic projections, .tex/.bib bytes, and authored sequences; different .sty/rendered presentation.
- **Edge cases**: Check content text and PDF links as well as hashes; float coordinates may differ.

#### Scenario: Title cover retains metadata

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/themes.rs`
- **Test name**: `test_cover_keeps_all_metadata`
- **Setup (GIVEN)**: Paper with unique title/subtitle/author/affiliation/date/abstract/keyword sentinels and cover theme.
- **Action (WHEN)**: Compile and inspect content plus rendered cover/body regions.
- **Assert (THEN)**: Every metadata field is emitted once as authored content and visible in required semantic order.
- **Edge cases**: Derived running-title repetition is classified separately; absent date stays absent.

#### Scenario: Invalid geometry or visibility

- **Test type**: unit
- **Test file**: `crates/terse-core/src/theme/tests.rs`
- **Test name**: `test_theme_visibility_and_geometry_bounds`
- **Setup (GIVEN)**: Zero body size, nonfinite values, impossible margins, oversized width, foreground watermark, matching colors and clipping attempts.
- **Action (WHEN)**: Validate parameterized themes.
- **Assert (THEN)**: Offending properties fail before generation; valid values at documented bounds pass.
- **Edge cases**: Negative spacing and unsupported off-page transforms fail.

#### Scenario: Watermark remains furniture

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/themes.rs`
- **Test name**: `test_watermark_is_visible_background_furniture`
- **Setup (GIVEN)**: Same fixture with watermark off/on and expected body text regions.
- **Action (WHEN)**: Compile, compare AST/projections and rendered regions.
- **Assert (THEN)**: Authored AST equal; watermark is behind text and body regions remain readable/nonempty.
- **Edge cases**: Validate opacity bounds separately; do not require PDF byte identity.

#### Scenario: Theme needs no private corporate resources

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/themes.rs`
- **Test name**: `test_public_theme_has_no_private_dependencies`
- **Setup (GIVEN)**: Public fixture only, empty user font/TeX trees, pinned distribution fonts, external network blocked.
- **Action (WHEN)**: Compile the unofficial magalu theme.
- **Assert (THEN)**: Logo/style work and dependency recorder contains only fixture assets or approved distribution files.
- **Edge cases**: No corporate credentials or host-only fonts; provenance files cover bundled demonstration assets.

#### Scenario: Undeclared external font or logo

- **Test type**: integration
- **Test file**: `crates/terse-core/tests/themes.rs`
- **Test name**: `test_external_theme_resources_fail`
- **Setup (GIVEN)**: Themes selecting unknown font family or logo path escaping root, including symlink case.
- **Action (WHEN)**: Check selected theme.
- **Assert (THEN)**: Field-located diagnostic with supported local-resource guidance; no outside bytes copied.
- **Edge cases**: Missing local logo also fails; in-root relative logo remains valid.

#### Scenario: Only presentation changes between builds

- **Test type**: integration
- **Test file**: `crates/terse-core/tests/themes.rs`
- **Test name**: `test_theme_switch_keeps_body_bytes`
- **Setup (GIVEN)**: Same immutable input snapshot and two validated themes.
- **Action (WHEN)**: Generate source-only projects.
- **Assert (THEN)**: Main TeX and .bib byte equality, style inequality, valid semantic anchors.
- **Edge cases**: Real compilation/link checks are also required by same_paper_content_under_two_themes.

#### Scenario: Heading numbering is disabled

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/themes.rs`
- **Test name**: `test_unnumbered_heading_reference_works`
- **Setup (GIVEN)**: Cross-referenced heading whose selected theme numbering is none.
- **Action (WHEN)**: Compile and inspect reference text and hyperlink destination.
- **Assert (THEN)**: Reference uses the heading title and points to its anchor without unresolved warnings.
- **Edge cases**: Numbered counterpart still works; unnumbered proof labels with explicit of relationships also tested.

### Capability: latex-generation

Source: [latex-generation spec](specs/latex-generation/spec.md).

#### Scenario: Portable LaTeX project [Acceptance B]

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/latex_generation.rs`
- **Test name**: `test_portable_output_compiles_without_terse`
- **Setup (GIVEN)**: Successful full-paper build; copy only published deliverables into clean pinned toolchain container with network disabled, empty user tree, and no Terse executable/mount.
- **Action (WHEN)**: Run the conventional XeLaTeX/Biber commands documented in COMPILE.txt.
- **Assert (THEN)**: PDF and bibliography compile, references resolve, recorder shows only copied assets and distribution resources.
- **Edge cases**: Run from a different absolute directory; maps/build manifest are not runtime prerequisites. Also edit a copied paragraph/style setting and conventionally recompile, verifying the authored edit and changed presentation without Terse.

#### Scenario: Output remains editable

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/latex_generation.rs`
- **Test name**: `test_generated_sources_are_human_editable`
- **Setup (GIVEN)**: Source-only generated minimal and full papers with literal prose and style macros.
- **Action (WHEN)**: Read sources, edit a copied paragraph and style setting, and inspect changed text.
- **Assert (THEN)**: Conventional readable preamble/body and local macro definitions; edits need no encoded payload decoder or Terse runtime.
- **Edge cases**: This integration test performs text/file assertions only; compilation of the original and edited copies is required by `test_portable_output_compiles_without_terse` in the E2E lane.

#### Scenario: All-element fixture renders

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/latex_generation.rs`
- **Test name**: `test_all_elements_render_under_both_themes`
- **Setup (GIVEN)**: Full fixture with every MVP node and independent expected semantic sequence/text/link list.
- **Action (WHEN)**: Generate and compile both themes.
- **Assert (THEN)**: All authored nodes appear once in semantic emission; expected visible prose/math/figure/table/theorem content and links are present.
- **Edge cases**: Check abstract/keywords/footnotes and all three heading levels; derived furniture counted separately.

#### Scenario: Literal metacharacters and math coexist

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/latex_generation.rs`
- **Test name**: `test_escaping_preserves_literal_text_and_math`
- **Setup (GIVEN)**: Fixture containing every TeX text metacharacter, URL fragments, code with braces, Unicode metadata, safe math, and raw payload oracle.
- **Action (WHEN)**: Generate then compile in the pinned toolchain.
- **Assert (THEN)**: Literal text matches text oracle, URL destinations stay valid, no unintended commands run, math/raw output bytes remain unchanged.
- **Edge cases**: Exercise optional arguments/PDF metadata/bibliography separately rather than relying on one global escape test.

#### Scenario: Dangerous link scheme

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/latex_generation.rs`
- **Test name**: `test_unsafe_link_schemes_fail`
- **Setup (GIVEN)**: Parameterized javascript/data/file/other unsupported destinations plus valid HTTP/HTTPS/mailto controls.
- **Action (WHEN)**: Check and generate without processes.
- **Assert (THEN)**: Unsafe schemes fail at the destination, zero engine/network calls, no output publication.
- **Edge cases**: Reject control characters and unescaped whitespace; supported links are never fetched.

#### Scenario: Same basenames from different directories

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/latex_generation.rs`
- **Test name**: `test_colliding_asset_basenames_are_disambiguated`
- **Setup (GIVEN)**: Two different plot.pdf assets in different directories with distinct known bytes.
- **Action (WHEN)**: Generate twice from clean output directories.
- **Assert (THEN)**: Stable distinct relative filenames and correct per-node copied bytes/paths.
- **Edge cases**: Equal-content assets do not swap source provenance; generated filename collisions fail.

#### Scenario: Unused file stays out of output

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/latex_generation.rs`
- **Test name**: `test_only_used_assets_copied`
- **Setup (GIVEN)**: Project with one referenced PDF and adjacent unrelated image, backup, and file.
- **Action (WHEN)**: Generate source artifacts.
- **Assert (THEN)**: Output contains the referenced asset and none of the unrelated files.
- **Edge cases**: Both document and theme dependencies obey allowlists.

#### Scenario: Unicode paper compiles

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/latex_generation.rs`
- **Test name**: `test_unicode_text_and_bibliography_compile`
- **Setup (GIVEN)**: en and pt-BR papers with accented prose, personal/organization names and locked Unicode metadata.
- **Action (WHEN)**: Compile using selected distribution fonts.
- **Assert (THEN)**: Expected Unicode text retained without transliteration; bibliography renders and logs have no missing glyphs.
- **Edge cases**: Absent date stays undated; no host locale/font fallback. A separate parameter case uses a fixture-verified missing glyph in the pinned font and must produce the documented build failure, exercising the real log path.

#### Scenario: Missing glyph is reported

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/latex_generation.rs`
- **Test name**: `test_missing_glyph_is_build_failure`
- **Setup (GIVEN)**: Existing successful output and recorded engine output with a missing-glyph diagnostic tied to an authored field.
- **Action (WHEN)**: Run build through the process interface.
- **Assert (THEN)**: Failure is actionable with source/context and previous directory bytes unchanged.
- **Edge cases**: Real missing-glyph log recognition is additionally required as a parameter case of `test_unicode_text_and_bibliography_compile`; this integration case uses the recorded runner only.

#### Scenario: Cross-file numbering converges [Acceptance E]

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/latex_generation.rs`
- **Test name**: `test_cross_file_equation_number_converges`
- **Setup (GIVEN)**: Forward equation reference in results to a labelled equation in method; additional unlabelled equation.
- **Action (WHEN)**: Compile through the normal bounded multipass runner.
- **Assert (THEN)**: Labelled number and PDF link resolve; unlabelled display remains unnumbered; no unresolved-reference log warnings.
- **Edge cases**: Repeat with figure/table/theorem/proof references and nonnumeric label names.

#### Scenario: Source generation without installed TeX

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/latex_generation.rs`
- **Test name**: `test_tex_only_starts_no_processes`
- **Setup (GIVEN)**: Valid project and process/network services that fail on any use.
- **Action (WHEN)**: Run build --tex-only.
- **Assert (THEN)**: All required text/assets emitted; process and network invocation counts zero; report states rendering/glyph checks were not run.
- **Edge cases**: Same behavior if tools happen to exist; conflicting PDF flags produce usage failure.

#### Scenario: Auto mode and required PDF differ

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/latex_generation.rs`
- **Test name**: `test_auto_vs_required_pdf_without_engine`
- **Setup (GIVEN)**: Project with old output and isolated PATH lacking the configured engine.
- **Action (WHEN)**: Run default build and --require-pdf in separate copies.
- **Assert (THEN)**: Auto emits source-only success plus warning; require-PDF fails and preserves prior bytes.
- **Edge cases**: Present engine with absent required Biber fails rather than silently reporting source-only success.

#### Scenario: Path is not a shell command

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/latex_generation.rs`
- **Test name**: `test_subprocess_arguments_are_not_shell_text`
- **Setup (GIVEN)**: Project paths with spaces and supported shell-sensitive characters, recording process runner, and harmless execution sentinel.
- **Action (WHEN)**: Invoke PDF build.
- **Assert (THEN)**: Executable and argument vector preserve exact path; no shell invocation; sentinel untouched.
- **Edge cases**: Use OS-valid metacharacter cases per host; sanitized TeX/Biber search paths and no-shell-escape flags asserted.

#### Scenario: Compilation does not converge

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/latex_generation.rs`
- **Test name**: `test_engine_pass_limit_is_enforced`
- **Setup (GIVEN)**: Engine runner repeatedly reports unresolved references; staged build and prior valid output.
- **Action (WHEN)**: Run PDF build to completion.
- **Assert (THEN)**: At most five total engine passes, explicit unresolved failure, unchanged prior output.
- **Edge cases**: Biber runs only when needed; timeout variant terminates owned descendants and fails.

#### Scenario: Explicit drawing support

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/latex_generation.rs`
- **Test name**: `test_declared_tikz_support_compiles`
- **Setup (GIVEN)**: Explicit raw drawing, declared built-in tikz package, installed pinned package, payload oracle.
- **Action (WHEN)**: Build normally and recompile copied output without Terse.
- **Assert (THEN)**: Drawing compiles, required setup is local/conventional, raw bytes are preserved.
- **Edge cases**: Strict mode warns; undeclared support and generated-file collisions fail in validation.

#### Scenario: Engine error preserves prior artifacts

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/latex_generation.rs`
- **Test name**: `test_failed_engine_keeps_previous_generation`
- **Setup (GIVEN)**: Successful output tree with known hashes; runner fails during XeLaTeX or Biber after partial stage files.
- **Action (WHEN)**: Attempt a rebuild for each failure point.
- **Assert (THEN)**: Original TeX/style/bib/assets/PDF hashes unchanged; diagnostic maps to original source where available.
- **Edge cases**: Auxiliaries/logs stay in disposable diagnostic storage; missing assets and nonzero tools also fail.

### Capability: citations

Source: [citations spec](specs/citations/spec.md).

#### Scenario: Multiple independent locators

- **Test type**: unit
- **Test file**: `crates/terse-core/src/references/tests.rs`
- **Test name**: `test_per_work_locators_preserved`
- **Setup (GIVEN)**: Citation group [@robins1986, pp. 10–12; @pearl2009, p. 42] and two locked records.
- **Action (WHEN)**: Bind and emit semantic citation commands.
- **Assert (THEN)**: Work order and each independent locator preserved, with expected typed/literal values.
- **Edge cases**: Include escaped semicolon/bracket locator content and unknown literal locator labels.

#### Scenario: Narrative versus parenthetical form

- **Test type**: unit
- **Test file**: `crates/terse-core/src/references/tests.rs`
- **Test name**: `test_narrative_and_parenthetical_distinct`
- **Setup (GIVEN)**: @paper and [@paper] referring to the same locked work.
- **Action (WHEN)**: Resolve and emit each citation form.
- **Assert (THEN)**: Distinct semantic kinds and corresponding narrative/parenthetical commands; same target.
- **Edge cases**: Grouped citations keep style-independent intent.

#### Scenario: Unknown citation alias [Acceptance D]

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/citations.rs`
- **Test name**: `test_unknown_alias_has_actionable_source`
- **Setup (GIVEN)**: Unknown @acemoglu2027 in an included file with fixed line/column.
- **Action (WHEN)**: Run check in human and JSON modes.
- **Assert (THEN)**: Exit 1, E-CITE-001, exact original position/alias, declaration and refs resolve guidance.
- **Edge cases**: No network/process calls and no writes.

#### Scenario: Leftover lock entry does not authorize a citation

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/citations.rs`
- **Test name**: `test_orphan_lock_cannot_authorize_alias`
- **Setup (GIVEN)**: A cited alias exists in lock but not any source refs declaration.
- **Action (WHEN)**: Check the project.
- **Assert (THEN)**: Undeclared-alias error, no successful binding or publication.
- **Edge cases**: A valid source declaration restores binding; duplicate declarations still fail.

#### Scenario: Declared identifier changed

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/citations.rs`
- **Test name**: `test_changed_identifier_requires_resolution`
- **Setup (GIVEN)**: Locked DOI under an alias; source changes that alias to another DOI.
- **Action (WHEN)**: Check and build without network access.
- **Assert (THEN)**: Stale-binding error and explicit resolve guidance; lock and output hashes unchanged.
- **Edge cases**: Equivalent normalized prefixes/case do not falsely invalidate identity.

#### Scenario: DOI resolution enables offline builds [Acceptance C]

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/citations.rs`
- **Test name**: `test_doi_resolution_then_offline_build`
- **Setup (GIVEN)**: Source with supported DOI and no record; recorded CSL response injected into the resolver used by the CLI application.
- **Action (WHEN)**: Run refs resolve, then actual binary build with network disabled, then validate bibliography in pinned tools.
- **Assert (THEN)**: Versioned normalized lock written; build uses it unchanged and produces valid .bib/PDF citations.
- **Edge cases**: No hand-written .bib; assert one expected metadata request and zero requests from the later build.

#### Scenario: Ordinary resolution retains an unchanged record

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/citations.rs`
- **Test name**: `test_unchanged_reference_not_refetched`
- **Setup (GIVEN)**: Resolved record with unchanged declarations/overrides and transport that rejects all calls.
- **Action (WHEN)**: Run refs resolve without refresh.
- **Assert (THEN)**: Zero provider requests and byte-identical entry metadata/lock.
- **Edge cases**: Explicit refresh uses the provider; unchanged record survives unrelated alias updates.

#### Scenario: Normalized DOI identity

- **Test type**: unit
- **Test file**: `crates/terse-core/src/references/tests.rs`
- **Test name**: `test_doi_normalization_is_canonical`
- **Setup (GIVEN)**: Provider fixtures and equivalent bare/prefixed/case/whitespace DOI inputs.
- **Action (WHEN)**: Normalize and resolve through recorded transport.
- **Assert (THEN)**: Equal canonical identities and normalized metadata.
- **Edge cases**: Test non-Crossref registration-agency response mapping; malformed identifiers fail.

#### Scenario: Provider returns the wrong work

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/citations.rs`
- **Test name**: `test_provider_identity_mismatch_rolls_back`
- **Setup (GIVEN)**: Existing lock; requested DOI and successful CSL response naming another DOI.
- **Action (WHEN)**: Resolve the changed alias.
- **Assert (THEN)**: Resolution failure, identity-mismatch diagnostic and identical old lock.
- **Edge cases**: Malformed/executable provider markup and incomplete effective fields fail without guessed metadata.

#### Scenario: Latest-at-resolution becomes fixed metadata

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/citations.rs`
- **Test name**: `test_versionless_arxiv_stays_pinned`
- **Setup (GIVEN)**: Versionless declaration initially resolved to v2; provider fixture later offers v3.
- **Action (WHEN)**: Build and resolve normally, then explicitly refresh.
- **Assert (THEN)**: Normal operations retain v2 without fetching; refresh alone switches the recorded exact version to v3.
- **Edge cases**: Use fake monotonic time to assert sequential requests spaced at least three seconds, without wall-clock sleeps.

#### Scenario: Explicit arXiv version is honored

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/citations.rs`
- **Test name**: `test_explicit_arxiv_version_verified`
- **Setup (GIVEN)**: Modern and legacy versioned identifiers with matching/mismatching Atom entries.
- **Action (WHEN)**: Resolve each requested version.
- **Assert (THEN)**: Matching exact version retained; mismatch aborts transaction.
- **Edge cases**: DTD/external-entity payload rejected without secondary requests; no PDF/page scraping fallback.

#### Scenario: Reserved ISBN declaration

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/citations.rs`
- **Test name**: `test_reserved_reference_providers_fail`
- **Setup (GIVEN)**: ISBN-backed alias and a generic URL-backed alias in separate fixtures.
- **Action (WHEN)**: Run refs resolve and check.
- **Assert (THEN)**: Unsupported-provider diagnostics at declarations; lock unchanged and no fabricated BibTeX/network fetch.
- **Edge cases**: Uncited unsupported declarations still fail; ordinary hyperlinks remain valid.

#### Scenario: Metadata order does not affect serialization

- **Test type**: unit
- **Test file**: `crates/terse-core/src/references/tests.rs`
- **Test name**: `test_lock_serialization_has_fixed_order`
- **Setup (GIVEN)**: Equivalent provider maps with shuffled field/alias order and intentionally ordered authors.
- **Action (WHEN)**: Normalize and serialize locks.
- **Assert (THEN)**: Byte-identical TOML with version/normalization fields and sorted entries; author order unchanged.
- **Edge cases**: No timestamps/absolute paths/cache data; unsupported lock version rejected.

#### Scenario: Unparsed personal name is retained

- **Test type**: unit
- **Test file**: `crates/terse-core/src/references/tests.rs`
- **Test name**: `test_unparsed_person_name_not_guessed`
- **Setup (GIVEN)**: Atom/metadata fixture providing a full personal name without components, plus structured and organizational name controls.
- **Action (WHEN)**: Normalize reference records.
- **Assert (THEN)**: Unparsed person remains distinct from structured person and organization; no surname inference.
- **Edge cases**: Missing date remains absent; effective anonymous marker is explicit.

#### Scenario: Corrected author data works offline

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/citations.rs`
- **Test name**: `test_offline_override_reseals_effective_record`
- **Setup (GIVEN)**: Existing normalized provider record plus corrections in references.overrides.toml; transport rejects requests.
- **Action (WHEN)**: Run refs resolve --offline, then source-only build.
- **Assert (THEN)**: Corrected title/authors in effective lock and .bib; provider data preserved and zero requests.
- **Edge cases**: Whole-array replacement and optional-field remove work; identity/provider or required-field removal fails.

#### Scenario: Unsealed override is diagnosed

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/citations.rs`
- **Test name**: `test_unsealed_override_requires_resolution`
- **Setup (GIVEN)**: Resolved project; change semantic override without resolving.
- **Action (WHEN)**: Run build/check, then repeat with formatting-only equivalent override.
- **Assert (THEN)**: Semantic change fails with reseal guidance and no writes; formatting-only change remains valid.
- **Edge cases**: Assignment/removal overlap fails; offline resolution cannot invent a missing/changed identifier.

#### Scenario: Mixed resolution fails atomically

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/citations.rs`
- **Test name**: `test_resolution_is_all_or_nothing`
- **Setup (GIVEN)**: Existing lock and two new identifiers; one valid response and one timeout/error/oversized response.
- **Action (WHEN)**: Resolve both with deterministic transport/fake clock.
- **Assert (THEN)**: Failure leaves prior lock byte-identical despite first success.
- **Edge cases**: Cover bounded retry/backoff, disallowed private-address redirects at each hop, no partial replace, and conflicting refresh/offline flags.

#### Scenario: Explicit pruning

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/citations.rs`
- **Test name**: `test_prune_is_explicit`
- **Setup (GIVEN)**: Lock contains one declared record and one orphan record.
- **Action (WHEN)**: Resolve normally and then with --prune.
- **Assert (THEN)**: Normal resolution retains orphan; prune removes only the orphan and preserves retained effective metadata.
- **Edge cases**: Pruning cannot authorize a citation with no source declaration.

#### Scenario: Serialization and presentation ordering are independent

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/citations.rs`
- **Test name**: `test_bib_order_vs_display_order`
- **Setup (GIVEN)**: zeta cited before alpha, duplicate citations, and an unused locked work.
- **Action (WHEN)**: Generate .bib and compile.
- **Assert (THEN)**: Serialized entries alpha then zeta; unused entry absent; visible bibliography zeta then alpha once each.
- **Edge cases**: Per-group sort disabled and locators survive author-year/numeric presentation.

### Capability: multi-file-projects

Source: [multi-file-projects spec](specs/multi-file-projects/spec.md).

#### Scenario: Entry discovery from a subdirectory

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/multi_file_projects.rs`
- **Test name**: `test_manifest_discovery_from_descendant`
- **Setup (GIVEN)**: Manifest/entry at root and nested invocation directory.
- **Action (WHEN)**: Run check without entry from the nested directory.
- **Assert (THEN)**: Root manifest and its root-relative entry selected.
- **Edge cases**: Explicit relative entry starts discovery from its directory, not another working-directory project.

#### Scenario: Missing or invalid manifest

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/multi_file_projects.rs`
- **Test name**: `test_missing_or_invalid_manifest_fails`
- **Setup (GIVEN)**: Directories with no ancestor manifest, unknown field, and unsupported format version.
- **Action (WHEN)**: Invoke check in each project.
- **Assert (THEN)**: Configuration exit 2 and relevant manifest/location; missing manifest suggests init.
- **Edge cases**: No files written; malformed TOML also returns configuration failure.

#### Scenario: Explicit settings win without environment drift

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/multi_file_projects.rs`
- **Test name**: `test_cli_precedence_is_environment_independent`
- **Setup (GIVEN)**: Manifest PDF auto, two themes, and copies with changed locale/TeX search paths.
- **Action (WHEN)**: Run build --theme magalu --tex-only.
- **Assert (THEN)**: Explicit flags win, zero processes, identical text artifact bytes.
- **Edge cases**: Missing academic default requires explicit theme; unknown explicit theme fails.

#### Scenario: Asset in a sibling directory

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/multi_file_projects.rs`
- **Test name**: `test_relative_sibling_asset_resolves`
- **Setup (GIVEN)**: sections/method.trs references ../figures/model.pdf inside root.
- **Action (WHEN)**: Check and source-build from two invocation directories.
- **Assert (THEN)**: Same dependency and copied figure bytes from both directories.
- **Edge cases**: Internal .. is allowed when canonical target stays in root; external escape rejected.

#### Scenario: Theme-relative logo

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/multi_file_projects.rs`
- **Test name**: `test_logo_resolves_from_theme_directory`
- **Setup (GIVEN)**: Theme references assets/logo.pdf and same-named distractor under project/cwd.
- **Action (WHEN)**: Resolve theme and build.
- **Assert (THEN)**: themes/assets/logo.pdf is used and included as dependency.
- **Edge cases**: Distractor bytes never appear in output.

#### Scenario: Repeated module without IDs

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/multi_file_projects.rs`
- **Test name**: `test_repeated_include_repeats_content`
- **Setup (GIVEN)**: Unlabelled paragraph module included twice around other content.
- **Action (WHEN)**: Resolve/build the entry.
- **Assert (THEN)**: Two authored occurrences at explicit positions, no deduplication.
- **Edge cases**: Warm parse cache does not change sequence.

#### Scenario: Wildcard inclusion is rejected

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/multi_file_projects.rs`
- **Test name**: `test_wildcard_include_is_invalid`
- **Setup (GIVEN)**: include sections/*.trs in a directory containing differently named modules.
- **Action (WHEN)**: Check with randomized enumeration.
- **Assert (THEN)**: Wildcard diagnostic with explicit-path guidance; no expanded content.
- **Edge cases**: Quoted literal wildcard remains unsupported, not treated as a glob.

#### Scenario: Indirect include cycle [Acceptance F]

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/multi_file_projects.rs`
- **Test name**: `test_indirect_include_cycle_is_complete`
- **Setup (GIVEN)**: paper -> method -> appendix -> paper with known include spans.
- **Action (WHEN)**: Run check with recording backend/process interfaces.
- **Assert (THEN)**: E-INCLUDE-003 includes the complete ordered cycle/edge locations; zero backend/engine invocations.
- **Edge cases**: Direct cycle, cycle reached through a prefix, and repeated acyclic include controls.

#### Scenario: Include target is absent

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/multi_file_projects.rs`
- **Test name**: `test_missing_include_is_source_located`
- **Setup (GIVEN)**: Entry includes a nonexistent sections/new.trs.
- **Action (WHEN)**: Run check.
- **Assert (THEN)**: Error anchored to include statement and unresolved path reported.
- **Edge cases**: Empty existing module succeeds; missing target is retained in attempted dependencies.

#### Scenario: Multi-file equation reference [Acceptance E]

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/multi_file_projects.rs`
- **Test name**: `test_multi_file_equation_link_valid`
- **Setup (GIVEN)**: Equation in method and forward/back references in results with locked fixture.
- **Action (WHEN)**: Check and compile.
- **Assert (THEN)**: Target resolves to the equation, correct number and valid PDF hyperlink, no unresolved warnings.
- **Edge cases**: Other typed references exercised; changing module filename preserves source provenance.

#### Scenario: Duplicate definitions are actionable

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/multi_file_projects.rs`
- **Test name**: `test_duplicate_ids_show_both_routes`
- **Setup (GIVEN)**: Two modules define demand-model; second variant includes the same module twice.
- **Action (WHEN)**: Check both projects.
- **Assert (THEN)**: E-ID-002 contains both source definitions and distinct include chains for coincident spans.
- **Edge cases**: Case-distinct valid IDs remain distinct; errors precede generation.

#### Scenario: Wrong proof target

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/multi_file_projects.rs`
- **Test name**: `test_proof_of_requires_theorem_target`
- **Setup (GIVEN)**: Proof of attribute names a figure, plus valid lemma target control.
- **Action (WHEN)**: Validate the project.
- **Assert (THEN)**: Wrong-kind diagnostic links proof attribute and figure definition; valid lemma binds.
- **Edge cases**: Unknown target fails; missing of does not infer adjacency.

#### Scenario: Declaration and citation in different modules

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/multi_file_projects.rs`
- **Test name**: `test_citations_bind_across_modules`
- **Setup (GIVEN)**: One included module declares supported alias, another cites it, and lock matches.
- **Action (WHEN)**: Resolve and generate bibliography.
- **Assert (THEN)**: Citation binds and global bibliography contains the work once.
- **Edge cases**: Duplicate declarations fail even when identifiers agree; uncited supported unlocked alias only warns.

#### Scenario: Multiple bibliography markers

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/multi_file_projects.rs`
- **Test name**: `test_bibliography_markers_are_global`
- **Setup (GIVEN)**: Two included modules each contain bibliography marker.
- **Action (WHEN)**: Resolve project.
- **Assert (THEN)**: Failure with both marker spans, no silent removal.
- **Edge cases**: Zero marker with citations appends derived bibliography; one explicit marker retains its position.

#### Scenario: Symlink escapes the project

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/multi_file_projects.rs`
- **Test name**: `test_symlink_escape_never_read`
- **Setup (GIVEN)**: In-root include/asset symlink to outside sentinel file and observable filesystem read/copy boundary.
- **Action (WHEN)**: Check/build.
- **Assert (THEN)**: Reference rejected before outside bytes are consumed/copied; sentinel unchanged.
- **Edge cases**: Run actual symlink/junction cases on capable OS runners; mocked path tests are not the sole evidence.

#### Scenario: Unsafe output setting

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/multi_file_projects.rs`
- **Test name**: `test_output_scope_cannot_overlap_inputs`
- **Setup (GIVEN)**: Manifest output configured to root, source folder, outside directory, or symlink escape; hash all sentinels.
- **Action (WHEN)**: Attempt builds.
- **Assert (THEN)**: Configuration/path failure before any sentinel or previous-output mutation.
- **Edge cases**: Nonexistent output checks canonical existing ancestor; drive/UNC variants tested on applicable OS.

#### Scenario: Missing figure and unsupported format

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/multi_file_projects.rs`
- **Test name**: `test_invalid_figure_dependencies_fail`
- **Setup (GIVEN)**: Missing PDF, existing SVG, remote image URL, and valid PDF/PNG/JPEG controls.
- **Action (WHEN)**: Check projects.
- **Assert (THEN)**: Invalid cases identify source path and accepted local formats; valid assets accepted.
- **Edge cases**: No implicit conversion/download; special files/dangling links rejected.

#### Scenario: Newly referenced missing file can be repaired

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/multi_file_projects.rs`
- **Test name**: `test_failed_lookup_retained_as_dependency`
- **Setup (GIVEN)**: Previously valid entry changes to include/reference an absent file.
- **Action (WHEN)**: Run build planning and inspect dependency result.
- **Assert (THEN)**: Failed plan records missing path and parent, alongside last successful dependencies at watcher handoff.
- **Edge cases**: Lock/overrides, manifest, theme assets, and support files included in complete dependency set.

### Capability: git-oriented-tooling

Source: [git-oriented-tooling spec](specs/git-oriented-tooling/spec.md).

#### Scenario: Initialize a usable project

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/git_oriented_tooling.rs`
- **Test name**: `test_init_scaffold_is_usable`
- **Setup (GIVEN)**: Empty temporary directory, no engine/network access, isolated user configuration.
- **Action (WHEN)**: Run init, check, fmt --check, and build --tex-only.
- **Assert (THEN)**: Versioned manifest/lock, entry, academic theme, ignore file present; commands succeed without resolution or TeX.
- **Edge cases**: Verify no current-date/random content in scaffold and stable repeated fresh initialization.

#### Scenario: Existing source is protected

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/git_oriented_tooling.rs`
- **Test name**: `test_init_conflict_is_preflighted`
- **Setup (GIVEN)**: Preexisting paper.trs and unrelated .gitignore rules; before-tree hash.
- **Action (WHEN)**: Run init without force.
- **Assert (THEN)**: Conflict diagnostic and unchanged whole tree, including .gitignore.
- **Edge cases**: Collision discovered late in scaffold enumeration must still prevent all writes.

#### Scenario: Ignore entries are additive

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/git_oriented_tooling.rs`
- **Test name**: `test_init_ignore_merge_is_additive`
- **Setup (GIVEN)**: Existing unrelated ignore entries and no scaffold conflict.
- **Action (WHEN)**: Initialize, then repeat with explicit force.
- **Assert (THEN)**: Original rules preserved and each required build/cache/auxiliary rule occurs once.
- **Edge cases**: Source figure PDFs remain eligible for Git; unrelated files survive force.

#### Scenario: Check without a TeX installation

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/git_oriented_tooling.rs`
- **Test name**: `test_check_requires_no_engine`
- **Setup (GIVEN)**: Valid locked multi-file project and missing engine, with denied network/process capabilities.
- **Action (WHEN)**: Run check.
- **Assert (THEN)**: Success, no subprocess/network requests, no source/lock/output changes.
- **Edge cases**: Verify check reports static validation limits honestly.

#### Scenario: Theme selection controls validation scope

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/git_oriented_tooling.rs`
- **Test name**: `test_check_theme_scope_is_explicit`
- **Setup (GIVEN)**: One valid and one invalid declared theme, each with separate assets.
- **Action (WHEN)**: Check all themes, then explicitly select the valid one.
- **Assert (THEN)**: First fails on invalid theme; selected check succeeds and touches only its presentation dependencies.
- **Edge cases**: Common authored asset/citation errors fail both checks.

#### Scenario: Formatting stability [Acceptance H]

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/git_oriented_tooling.rs`
- **Test name**: `test_format_is_semantic_and_idempotent`
- **Setup (GIVEN)**: Valid unformatted project with source/themes, deliberate paragraph wrapping and ordered metadata/citations.
- **Action (WHEN)**: Run fmt twice, reparse, then fmt --check.
- **Assert (THEN)**: Second pass byte-identical, check exit 0, pre/post semantic projections equal.
- **Edge cases**: Syntactically valid unresolved citations can be formatted without provider access.

#### Scenario: Opaque payload and paragraph wrapping are preserved

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/format_tests.rs`
- **Test name**: `test_formatter_preserves_opaque_bytes`
- **Setup (GIVEN)**: Golden inputs with wrapped prose, BOM, CRLF outside/inside payloads, extra payload spaces and EOF without newline.
- **Action (WHEN)**: Format in memory and compare to independent expected bytes.
- **Assert (THEN)**: Only structural/nonopaque canonicalization changes; prose wrapping and code/math/raw payload bytes match oracles.
- **Edge cases**: Payload comments and whitespace-only lines remain untouched; initial BOM removed.

#### Scenario: Malformed file prevents batch writes

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/git_oriented_tooling.rs`
- **Test name**: `test_batch_format_parse_failure_writes_nothing`
- **Setup (GIVEN)**: Multiple files needing formatting and one malformed file positioned late in the argument list.
- **Action (WHEN)**: Run fmt for all paths.
- **Assert (THEN)**: Invalid-file diagnostic and unchanged bytes for every selected file.
- **Edge cases**: Repeat with invalid file first/last and duplicate path arguments.

#### Scenario: CI detects formatting drift

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/git_oriented_tooling.rs`
- **Test name**: `test_format_check_is_read_only`
- **Setup (GIVEN)**: Valid source/theme with noncanonical separators and preserved before-hashes.
- **Action (WHEN)**: Run fmt --check in human and JSON modes.
- **Assert (THEN)**: Exit 1, affected filenames present, no input writes.
- **Edge cases**: Formatted control returns 0 and valid empty diagnostics.

#### Scenario: Deterministic output [Acceptance G]

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/git_oriented_tooling.rs`
- **Test name**: `test_text_artifacts_are_reproducible`
- **Setup (GIVEN)**: Two clean copies at different roots; same compiler/lock/config, varied locale/enumeration/source mtimes.
- **Action (WHEN)**: Run identical source-only builds and compare manifests of all generated text.
- **Assert (THEN)**: Every corresponding Terse-generated text byte matches; no absolute paths, timestamps, or random IDs.
- **Edge cases**: Repeat cached/uncached; .bbl comparison belongs to pinned-toolchain lane, PDFs excluded.

#### Scenario: Clean and cached builds agree

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/git_oriented_tooling.rs`
- **Test name**: `test_cache_is_disposable`
- **Setup (GIVEN)**: Same project with valid caches, no caches, and corrupt parse/provider cache files.
- **Action (WHEN)**: Build each with network disabled.
- **Assert (THEN)**: Identical generated text and zero provider calls; corrupt entries recomputed.
- **Edge cases**: Current changed source/lock always invalidates stale cache-derived results.

#### Scenario: CI observes failure categories

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/git_oriented_tooling.rs`
- **Test name**: `test_exit_codes_and_json_are_meaningful`
- **Setup (GIVEN)**: Cases for unknown citation, invalid flag/manifest, provider/tool/I/O failure, formatting drift, and success.
- **Action (WHEN)**: Invoke each noninteractively with JSON diagnostics.
- **Assert (THEN)**: Codes 1, 2, 3, 1, and 0 follow contract; stdout parses without ANSI/progress contamination.
- **Edge cases**: Absent stdin never causes prompts; deny-warnings returns validation failure.

#### Scenario: Publication failure rolls back

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/git_oriented_tooling.rs`
- **Test name**: `test_publication_rollback_and_restart_recovery`
- **Setup (GIVEN)**: Known valid managed output and complete next stage; deterministic failure points before/after backup and replacement renames.
- **Action (WHEN)**: Attempt publication; simulate process restart where journal remains.
- **Assert (THEN)**: After rollback/recovery exactly one complete valid generation exists, without mixed files.
- **Edge cases**: Repeat concurrent-writer lock case; atomic-replacement platforms also exercise portable fallback contract.

#### Scenario: Unowned destination is protected

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/git_oriented_tooling.rs`
- **Test name**: `test_unowned_output_is_never_replaced`
- **Setup (GIVEN)**: Populated output directory without valid ownership manifest, with known sentinel bytes.
- **Action (WHEN)**: Attempt build publication.
- **Assert (THEN)**: Failure with all destination/source sentinel bytes unchanged.
- **Edge cases**: Forged/malformed ownership manifest does not authorize destructive cleanup.

#### Scenario: A contributor reproduces the demonstration

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/git_oriented_tooling.rs`
- **Test name**: `test_public_checkout_full_user_workflow`
- **Setup (GIVEN)**: Clean source checkout, documented Rust/TeX prerequisites and redistributable fixture assets; no private credentials.
- **Action (WHEN)**: Install using documented command; run init/check/fmt/two-theme build, conventional compilation, and export.
- **Assert (THEN)**: Workflow succeeds using locked references and unedited source across themes; no hidden files or manual .bib/TeX repairs.
- **Edge cases**: License/package metadata checked against maintainer's actual selected license at release; the test does not choose one.

#### Scenario: One maintainer can run release validation

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/git_oriented_tooling.rs`
- **Test name**: `test_maintainer_can_reproduce_release_checks`
- **Setup (GIVEN)**: Checked-in local validation scripts, pinned Linux TeX environment and supported OS compiler jobs.
- **Action (WHEN)**: Run the documented local workflow and equivalent CI entrypoints.
- **Assert (THEN)**: All A–L cases executed with results; supported executable smoke checks present; no separate reviewer/service prerequisite.
- **Edge cases**: CLI/core tests remain runnable without TeX; full release run fails if required heavy cases were skipped.

### Capability: diagnostics

Source: [diagnostics spec](specs/diagnostics/spec.md).

#### Scenario: Duplicate identifier reports both definitions

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/diagnostics.rs`
- **Test name**: `test_duplicate_id_diagnostic_has_two_origins`
- **Setup (GIVEN)**: Two definitions with fixed spans and optional repeated-include route variant.
- **Action (WHEN)**: Check in human and JSON modes.
- **Assert (THEN)**: E-ID-002, error severity, exact offending/first positions, related routes and reliable correction.
- **Edge cases**: Exact stable structured fields asserted; human wording checked for meaningful content, not incidental wrapping.

#### Scenario: Tool startup has no invented source line

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/diagnostics.rs`
- **Test name**: `test_tool_startup_error_has_honest_span`
- **Setup (GIVEN)**: Engine configuration and runner returning executable-start failure.
- **Action (WHEN)**: Attempt --require-pdf build.
- **Assert (THEN)**: Tool failure code/family, absent primary span where unmappable and config related location; no fabricated .trs coordinates.
- **Edge cases**: Missing executable distinguished from engine content failure.

#### Scenario: Unicode before a cited alias

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/diagnostics.rs`
- **Test name**: `test_unicode_columns_match_original_bytes`
- **Setup (GIVEN)**: Included paragraph with multibyte accented text before an unknown alias; known UTF-8 byte indices.
- **Action (WHEN)**: Run check and compare human/JSON positions.
- **Assert (THEN)**: Original module path, one-based Unicode-scalar column, exact original byte range and include context.
- **Edge cases**: CRLF and combining-codepoint variant distinguish scalar counts from bytes/graphemes.

#### Scenario: Strict raw warning remains recognizable

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/diagnostics.rs`
- **Test name**: `test_deny_warnings_does_not_rename_code`
- **Setup (GIVEN)**: One raw block in a valid project.
- **Action (WHEN)**: Run check --strict with/without --deny-warnings.
- **Assert (THEN)**: Both contain W-TEX-001 at the same span; warning-only run succeeds and deny-warnings run fails.
- **Edge cases**: Ordinary nonstrict checking does not reject the raw block solely for being raw.

#### Scenario: CI parses repeated validation failures

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/diagnostics.rs`
- **Test name**: `test_json_diagnostics_are_stable`
- **Setup (GIVEN)**: Project with several deterministic invalid sources and Unicode paths.
- **Action (WHEN)**: Run two JSON checks with terminal/color env varied.
- **Assert (THEN)**: Equal ordered diagnostic arrays, versioned envelope, stdout JSON only; progress restricted to stderr.
- **Edge cases**: Success yields parseable empty diagnostics; no absolute host prefix.

#### Scenario: Watch distinguishes failed and published attempts

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/diagnostics.rs`
- **Test name**: `test_watch_json_reports_each_attempt`
- **Setup (GIVEN)**: Watch in JSON/source-only mode; first error then repaired source.
- **Action (WHEN)**: Collect completed-attempt records using bounded readiness handshakes.
- **Assert (THEN)**: One parseable record per completion, increasing attempt IDs, correct status and publication result.
- **Edge cases**: Superseded attempts distinct from published ones; no interleaved prose.

#### Scenario: Engine error in included raw block

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/diagnostics.rs`
- **Test name**: `test_engine_error_maps_to_included_raw_source`
- **Setup (GIVEN)**: Generated map for an included raw block and recorded engine log referencing a known emitted line.
- **Action (WHEN)**: Interpret the failure through the normal build diagnostic path.
- **Assert (THEN)**: Primary original module/block line, generated file/line as related context.
- **Edge cases**: Also test escaped-text fallback, theme parameter origin, and bibliography alias/override provenance.

#### Scenario: Unmappable package failure

- **Test type**: unit
- **Test file**: `crates/terse-core/src/diagnostic/tests.rs`
- **Test name**: `test_unmappable_error_stays_generated`
- **Setup (GIVEN)**: Recorded failure in a distribution package with no trustworthy source-map interval.
- **Action (WHEN)**: Map diagnostic.
- **Assert (THEN)**: Generated/tool context retained, no invented .trs location.
- **Edge cases**: Unknown log formats remain explicit tool errors; no path content guessed as source spans.

#### Scenario: Several independent source errors

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/diagnostics.rs`
- **Test name**: `test_bounded_error_recovery_never_publishes`
- **Setup (GIVEN)**: Several recoverable syntax errors, a documented diagnostic cap, and known previous output.
- **Action (WHEN)**: Run validation/build.
- **Assert (THEN)**: Ordered errors up to limit, explicit truncation notice, failure and no replacement plan/output.
- **Edge cases**: The parser stops at safe structural boundaries; exceeding limits never panics.

### Capability: watch-mode

Source: [watch-mode spec](specs/watch-mode/spec.md).

#### Scenario: Initial syntax error can be repaired

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/watch_mode.rs`
- **Test name**: `test_watch_recovers_from_initial_error`
- **Setup (GIVEN)**: Valid manifest, broken entry, no old output; source-only watcher and readiness observer.
- **Action (WHEN)**: Start watch, await first failure, fix source, await completed successor.
- **Assert (THEN)**: Original source error followed by first valid publication without process restart.
- **Edge cases**: Invalid startup configuration exits 2; valid project with missing include stays repairable.

#### Scenario: Atomic replacement and asset changes rebuild

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/watch_mode.rs`
- **Test name**: `test_atomic_saves_and_asset_edits_trigger_rebuild`
- **Setup (GIVEN)**: Successful watch with included source, theme/logo/figure, lock/overrides and support dependencies.
- **Action (WHEN)**: Atomically replace the module; separately change each dependency's bytes.
- **Assert (THEN)**: Each change causes a successful build with new relevant content/dependency hashes.
- **Edge cases**: Exercise actual filesystem events on each supported OS and polling fallback; no timing-only success assertions.

#### Scenario: Creating a missing include repairs the build

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/watch_mode.rs`
- **Test name**: `test_missing_dependency_creation_is_detected`
- **Setup (GIVEN)**: Watch fails after source introduces missing module/asset.
- **Action (WHEN)**: Create the missing file without touching the entry.
- **Assert (THEN)**: A new attempt succeeds and publishes using the created file.
- **Edge cases**: Deletion/recreation and missing overrides-parent observation covered.

#### Scenario: Generated output does not cause a loop

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/watch_mode.rs`
- **Test name**: `test_output_and_cache_events_are_ignored`
- **Setup (GIVEN)**: Watcher with deterministic event source; real successful build writes output/cache.
- **Action (WHEN)**: Deliver output/cache events and observe the scheduler.
- **Assert (THEN)**: No new attempt is scheduled solely by generated writes.
- **Edge cases**: Adjacent actual source event still triggers; ignore rules do not hide source directories.

#### Scenario: Burst of filesystem events

- **Test type**: unit
- **Test file**: `crates/terse-cli/src/watch/tests.rs`
- **Test name**: `test_events_use_trailing_debounce`
- **Setup (GIVEN)**: Fake clock and events at 0, 50, and 100 ms, followed by quiescence.
- **Action (WHEN)**: Advance to 249 then 250 ms and drive scheduler.
- **Assert (THEN)**: Zero starts before trailing 150 ms expires, exactly one afterward, no overlap.
- **Edge cases**: Events during a running build create at most one pending successor.

#### Scenario: Input changes while compilation runs

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/watch_mode.rs`
- **Test name**: `test_stale_snapshot_never_published`
- **Setup (GIVEN)**: Successful old output; process runner pauses current build at a barrier.
- **Action (WHEN)**: Change input while paused, then release runner and await successor.
- **Assert (THEN)**: Paused generation reported superseded and not published; successor uses latest snapshot.
- **Edge cases**: Change an asset or manifest, not only main source; old output remains valid until successful successor.

#### Scenario: Watch after error [Acceptance K]

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/watch_mode.rs`
- **Test name**: `test_watch_keeps_last_good_pdf_after_syntax_error`
- **Setup (GIVEN)**: Pinned-toolchain watch with successful PDF and all artifact hashes.
- **Action (WHEN)**: Introduce syntax error, await diagnostic, compare outputs, repair, await successful rebuild.
- **Assert (THEN)**: Error points to original source; old hashes persist through failure; corrected source automatically produces next valid PDF.
- **Edge cases**: Handshake/deadline-based waits, no arbitrary sleeps; retain diagnostic evidence on timeout.

#### Scenario: Lock and engine failures also preserve output

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/watch_mode.rs`
- **Test name**: `test_watch_retains_output_on_lock_or_engine_failure`
- **Setup (GIVEN)**: Successful generation, recording runner, and current reference lock.
- **Action (WHEN)**: Make lock stale or inject engine failure, then repair dependency/runner for next input event.
- **Assert (THEN)**: Failure leaves prior output intact; watcher stays alive and repair yields successful publication.
- **Edge cases**: Theme/asset and publication failures follow same preservation contract.

#### Scenario: New selected theme becomes active

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/watch_mode.rs`
- **Test name**: `test_watch_updates_theme_dependency_graph`
- **Setup (GIVEN)**: Manifest selects a valid theme file and then remaps its name to another valid theme with distinct assets.
- **Action (WHEN)**: Edit manifest, await build, then edit new theme asset.
- **Assert (THEN)**: Build uses new theme and its later asset change triggers another attempt.
- **Edge cases**: Failed remap retains old and attempted dependencies; prune obsolete dependency only after success.

#### Scenario: Interrupt during an attempt

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/watch_mode.rs`
- **Test name**: `test_interrupt_stops_owned_processes`
- **Setup (GIVEN)**: Successful output and current staged compile with observable child/descendant processes.
- **Action (WHEN)**: Send supported interruption and await bounded termination.
- **Assert (THEN)**: No partial publication, owned children stopped, locks released, last output hashes unchanged.
- **Edge cases**: Interrupt during debounce and publication rollback; unrelated processes are never terminated.

### Capability: arxiv-export

Source: [arxiv-export spec](specs/arxiv-export/spec.md).

#### Scenario: Current source controls export

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/arxiv_export.rs`
- **Test name**: `test_export_validates_current_sources`
- **Setup (GIVEN)**: Old successful build/export and current source with unresolved cross-reference.
- **Action (WHEN)**: Run export with recorded backend/process boundary.
- **Assert (THEN)**: Current-source validation failure, old build not packaged, existing export hashes unchanged.
- **Edge cases**: Changed lock/theme also invalidates stale prior build.

#### Scenario: arXiv package [Acceptance L]

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/arxiv_export.rs`
- **Test name**: `test_arxiv_archive_compiles_in_clean_environment`
- **Setup (GIVEN)**: Full semantic-only academic fixture with locked DOI/arXiv entries and used assets.
- **Action (WHEN)**: Export with required compilation; extract ZIP in separate network-disabled environment lacking Terse and project files; compile conventionally.
- **Assert (THEN)**: Directory/ZIP/manifest exist, bibliography/links resolve, recorder closure valid, no upload/network attempt.
- **Edge cases**: Both clean compilation and manifest hash checks are mandatory; static-only success does not satisfy this case. Include a verified compatible `--include-bbl` parameter case and conventionally compile the packaged matching-stem BBL.

#### Scenario: Unused files and rendered paper are excluded

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/arxiv_export.rs`
- **Test name**: `test_export_allowlist_excludes_cruft`
- **Setup (GIVEN)**: Valid project plus used PDF figure, unused image, editor backup, logs, maps, .trs source and rendered paper PDF.
- **Action (WHEN)**: Export and list exact ZIP members.
- **Assert (THEN)**: Used figure and required .tex/.sty/.bib/manifest included; unrelated/auxiliary/source/rendered-paper files and reports excluded.
- **Edge cases**: Basename similarity does not remove a required figure PDF.

#### Scenario: Missing or external dependency [Acceptance L]

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/arxiv_export.rs`
- **Test name**: `test_external_or_missing_export_dependency_fails`
- **Setup (GIVEN)**: Valid prior export; mutate asset to missing path, external path, traversal or escaping link.
- **Action (WHEN)**: Run export for each case.
- **Assert (THEN)**: Actionable source-located error, no outside bytes included, old export intact.
- **Edge cases**: Reject external package input even if local environment could supply it; no fetching/conversion.

#### Scenario: Portable filename collision

- **Test type**: unit
- **Test file**: `crates/terse-core/src/artifact/tests.rs`
- **Test name**: `test_archive_paths_are_portable`
- **Setup (GIVEN)**: Artifact plan with paths colliding under case-folding and with absolute/drive/UNC/traversal members.
- **Action (WHEN)**: Validate archive plan.
- **Assert (THEN)**: Unsafe/colliding paths rejected before archive writing.
- **Edge cases**: Safe nested paths retained, separators normalized; no recursive directory walk admitted.

#### Scenario: Default export avoids local BBL version coupling

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/arxiv_export.rs`
- **Test name**: `test_default_export_omits_prebuilt_bbl`
- **Setup (GIVEN)**: Normal build includes local .bbl and generated .bib.
- **Action (WHEN)**: Export without include-bbl.
- **Assert (THEN)**: Archive contains .bib and no .bbl; bibliography source/processor agree.
- **Edge cases**: Source-only export works without a local BBL; compiled archive validated separately.

#### Scenario: Requested incompatible BBL is rejected

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/arxiv_export.rs`
- **Test name**: `test_include_bbl_requires_verified_compatibility`
- **Setup (GIVEN)**: Recorded matching/mismatching bibliography profiles and main-stem BBL names.
- **Action (WHEN)**: Export with include-bbl in each condition.
- **Assert (THEN)**: Only verified compatible matching-stem BBL included; others fail explicitly without replacement.
- **Edge cases**: Unverifiable profile is not treated as compatible. This integration case uses recorded tool/profile responses; matching-control compilation is required by `test_arxiv_archive_compiles_in_clean_environment` in the E2E lane.

#### Scenario: Normally valid raw drawing is not silently omitted

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/arxiv_export.rs`
- **Test name**: `test_raw_export_rejection_preserves_content`
- **Setup (GIVEN)**: Raw drawing accepted by normal build; custom executable support variants and previous valid export.
- **Action (WHEN)**: Run arXiv export.
- **Assert (THEN)**: E-EXPORT-004 identifies raw block/support config; source/prior output unchanged; no silent omission.
- **Edge cases**: Normal raw generation/compile remains valid in its separate test; successful local raw compile does not waive rejection.

#### Scenario: Repeated export is deterministic

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/arxiv_export.rs`
- **Test name**: `test_zip_bytes_and_manifest_are_deterministic`
- **Setup (GIVEN)**: Same inputs/compiler/profile with changed source mtimes/root paths and clean outputs.
- **Action (WHEN)**: Export twice under controlled equal validation conditions.
- **Assert (THEN)**: Identical manifest/ZIP bytes; sorted root-relative membership; sizes/hashes match extracted bytes; manifest excludes its own hash.
- **Edge cases**: Fixed timestamps/modes/platform attributes; no host owner or staging path in archive.

#### Scenario: Local toolchain differs from target profile

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/arxiv_export.rs`
- **Test name**: `test_local_compile_is_not_claimed_as_profile_match`
- **Setup (GIVEN)**: Compatible runner reports successful compilation but unverified full package/font profile.
- **Action (WHEN)**: Export and inspect separate validation report.
- **Assert (THEN)**: Status compiled-local, actual tested assumptions described, no exact target/acceptance guarantee.
- **Edge cases**: Verified control alone reports compiled-profile; reports remain outside source archive.

#### Scenario: No compatible engine is available

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/arxiv_export.rs`
- **Test name**: `test_no_engine_reports_static_only_or_fails_required`
- **Setup (GIVEN)**: Statically closed semantic-only paper, no compatible engine, prior export sentinel.
- **Action (WHEN)**: Export normally and with require-compile in separate copies.
- **Assert (THEN)**: Normal export reports static-only and missing checks; required version fails preserving previous generation.
- **Edge cases**: Missing/risky dependencies still fail even without an engine; no silent tool download.

#### Scenario: Clean compilation uncovers a hidden dependency

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/arxiv_export.rs`
- **Test name**: `test_recorder_detects_hidden_dependency`
- **Setup (GIVEN)**: Supported artifact bundle and controlled runner/recorder listing a file outside package/distribution roots.
- **Action (WHEN)**: Validate extracted package.
- **Assert (THEN)**: Dependency failure, actionable explanation, no replacement export.
- **Edge cases**: Run real clean environment test with an intentionally removed packaged asset; empty user TeX tree prevents ambient repair.

#### Scenario: Archive or validation fails after staging

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/arxiv_export.rs`
- **Test name**: `test_export_transaction_preserves_consistent_generation`
- **Setup (GIVEN)**: Known prior directory/ZIP/reports and failure injection during archive creation, extraction, compilation, or publication.
- **Action (WHEN)**: Attempt replacement and simulate restart at transaction boundaries.
- **Assert (THEN)**: All prior published outputs remain or recover as one consistent generation; source/unrelated files untouched.
- **Edge cases**: Verify no mixed new ZIP/old manifest or report; unsafe unowned destination refused.

## Coverage Summary

All **146 spec scenarios** across **83 requirements** and **10 capabilities** are mapped. The primary tests comprise **40 unit**, **87 integration**, and **19 E2E** cases. These counts describe the plan, not implemented or passing tests.

### Acceptance gates A–L

| Gate | Primary mapped test(s) | Required evidence |
| --- | --- | --- |
| A | `test_theme_invariant_projection`; `test_same_paper_content_under_two_themes` | Equal authored projections/body bytes plus visibly different, nonempty presentations. |
| B | `test_portable_output_compiles_without_terse` | Generated bundle compiles offline in a clean compatible environment without Terse. |
| C | `test_doi_resolution_then_offline_build` | Explicit recorded DOI resolution writes the lock; subsequent offline build preserves it and compiles bibliography. |
| D | `test_unknown_alias_has_actionable_source` | Unknown alias fails with original location, stable code, alias, and resolution guidance. |
| E | `test_cross_file_equation_number_converges`; `test_multi_file_equation_link_valid` | Cross-file equation target has a valid number and PDF hyperlink after compilation. |
| F | `test_indirect_include_cycle_is_complete` | Complete include cycle and edge locations reported before generation. |
| G | `test_text_artifacts_are_reproducible` | All generated text artifacts match across clean roots and environment-independent variations. |
| H | `test_format_is_semantic_and_idempotent` | Formatting preserves meaning; second run is unchanged and check succeeds. |
| I | `test_visual_attributes_rejected` | Presentation attributes in content fail at the attribute with a theme-file correction. |
| J | `test_raw_tex_build_and_strict_warning` | Raw bytes preserved in normal output and strict source-located portability warning emitted. |
| K | `test_watch_keeps_last_good_pdf_after_syntax_error` | Watch keeps the prior successful output through an error and automatically publishes the repair. |
| L | `test_arxiv_archive_compiles_in_clean_environment`; `test_external_or_missing_export_dependency_fails` | Safe complete ZIP/manifest, clean offline compilation, dependency failures, and zero uploads. |

### Every spec scenario

| Capability | Scenario | Test file | Test name | Type |
| --- | --- | --- | --- | --- |
| language-parsing | Equivalent line endings | `crates/terse-core/src/syntax/tests.rs` | `test_line_endings_preserve_semantics` | unit |
| language-parsing | Invalid indentation | `crates/terse-core/src/syntax/tests.rs` | `test_invalid_structural_indentation` | unit |
| language-parsing | Reserved text is explicitly escaped | `crates/terse-core/src/syntax/tests.rs` | `test_escaped_reserved_start_is_prose` | unit |
| language-parsing | Malformed header is diagnosed | `crates/terse-core/src/syntax/tests.rs` | `test_malformed_reserved_header_fails` | unit |
| language-parsing | Full metadata survives parsing | `crates/terse-core/src/syntax/tests.rs` | `test_all_metadata_fields_survive` | unit |
| language-parsing | Invalid metadata placement or fields | `crates/terse-core/tests/language_parsing.rs` | `test_invalid_metadata_context` | integration |
| language-parsing | Mixed inline content | `crates/terse-core/src/syntax/tests.rs` | `test_mixed_inline_paragraph` | unit |
| language-parsing | Ambiguous delimiters are rejected | `crates/terse-core/src/syntax/tests.rs` | `test_crossing_and_nested_delimiters_fail` | unit |
| language-parsing | Nested lists and three heading levels | `crates/terse-core/src/syntax/tests.rs` | `test_headings_and_nested_lists` | unit |
| language-parsing | Nonsequential ordered markers | `crates/terse-core/src/syntax/tests.rs` | `test_ordered_markers_are_sequential` | unit |
| language-parsing | Theorem and proof from the authoring model | `crates/terse-core/src/syntax/tests.rs` | `test_theorem_proof_equation_structure` | unit |
| language-parsing | Unsupported nested structure | `crates/terse-core/src/syntax/tests.rs` | `test_nested_module_structures_fail` | unit |
| language-parsing | Figure and table preserve their meaning | `crates/terse-core/src/syntax/tests.rs` | `test_figure_table_fields_are_semantic` | unit |
| language-parsing | Invalid figure or table | `crates/terse-core/src/syntax/tests.rs` | `test_missing_figure_fields_and_ragged_tables` | unit |
| language-parsing | Citation forms are not conflated | `crates/terse-core/src/syntax/tests.rs` | `test_citation_lexing_retains_intent` | unit |
| language-parsing | Declarations are explicit | `crates/terse-core/src/syntax/tests.rs` | `test_declarations_require_module_scope` | unit |
| language-parsing | Theme isolation in content [Acceptance I] | `crates/terse-core/tests/language_parsing.rs` | `test_visual_attributes_rejected` | integration |
| language-parsing | Semantic role is valid | `crates/terse-core/src/syntax/tests.rs` | `test_wide_role_has_no_dimensions` | unit |
| language-parsing | Familiar mathematical notation survives | `crates/terse-core/src/syntax/tests.rs` | `test_tex_math_bytes_are_preserved` | unit |
| language-parsing | Math cannot execute commands | `crates/terse-core/src/syntax/tests.rs` | `test_math_rejects_execution` | unit |
| language-parsing | Raw TeX escape hatch [Acceptance J] | `crates/terse-core/tests/language_parsing.rs` | `test_raw_tex_build_and_strict_warning` | integration |
| language-parsing | Opaque payload includes comments and whitespace | `crates/terse-core/src/syntax/tests.rs` | `test_raw_payload_trivia_is_opaque` | unit |
| semantic-ast | Every MVP element has a typed representation | `crates/terse-core/src/semantic/tests.rs` | `test_all_mvp_nodes_are_typed` | unit |
| semantic-ast | Order survives include resolution | `crates/terse-core/tests/semantic_ast.rs` | `test_include_order_is_authored_order` | integration |
| semantic-ast | Included equation keeps its source location | `crates/terse-core/tests/semantic_ast.rs` | `test_included_spans_survive_generation` | integration |
| semantic-ast | Repeated inclusion has distinct occurrence context | `crates/terse-core/tests/semantic_ast.rs` | `test_include_occurrences_are_distinct` | integration |
| semantic-ast | Same document under two themes [Acceptance A] | `crates/terse-core/src/semantic/tests.rs` | `test_theme_invariant_projection` | unit |
| semantic-ast | Presentation-only data stays outside authored content | `crates/terse-core/src/semantic/tests.rs` | `test_furniture_excluded_from_authored_ast` | unit |
| semantic-ast | Compilation does not need services or effects | `crates/terse-core/src/semantic/tests.rs` | `test_compiler_core_has_no_effects` | unit |
| semantic-ast | Invalid references prevent emission | `crates/terse-core/src/semantic/tests.rs` | `test_invalid_binding_blocks_artifact_plan` | unit |
| semantic-ast | Excessive nesting is bounded | `crates/terse-core/src/semantic/tests.rs` | `test_resource_limits_fail_boundedly` | unit |
| themes | Supported semantic selector | `crates/terse-core/src/theme/tests.rs` | `test_base_and_wide_selectors` | unit |
| themes | Theme attempts to select authored content | `crates/terse-core/src/theme/tests.rs` | `test_themes_cannot_select_content` | unit |
| themes | Corporate and academic presentations | `crates/terse-cli/tests/e2e/themes.rs` | `test_two_theme_presentations_differ` | e2e |
| themes | Invalid typed value | `crates/terse-core/src/theme/tests.rs` | `test_typed_theme_values_rejected` | unit |
| themes | Wide figure override | `crates/terse-core/src/theme/tests.rs` | `test_role_overrides_base_width` | unit |
| themes | Reordered selectors | `crates/terse-core/src/theme/tests.rs` | `test_theme_selector_order_irrelevant` | unit |
| themes | Same content under different themes [Acceptance A] | `crates/terse-cli/tests/e2e/themes.rs` | `test_same_paper_content_under_two_themes` | e2e |
| themes | Title cover retains metadata | `crates/terse-cli/tests/e2e/themes.rs` | `test_cover_keeps_all_metadata` | e2e |
| themes | Invalid geometry or visibility | `crates/terse-core/src/theme/tests.rs` | `test_theme_visibility_and_geometry_bounds` | unit |
| themes | Watermark remains furniture | `crates/terse-cli/tests/e2e/themes.rs` | `test_watermark_is_visible_background_furniture` | e2e |
| themes | Theme needs no private corporate resources | `crates/terse-cli/tests/e2e/themes.rs` | `test_public_theme_has_no_private_dependencies` | e2e |
| themes | Undeclared external font or logo | `crates/terse-core/tests/themes.rs` | `test_external_theme_resources_fail` | integration |
| themes | Only presentation changes between builds | `crates/terse-core/tests/themes.rs` | `test_theme_switch_keeps_body_bytes` | integration |
| themes | Heading numbering is disabled | `crates/terse-cli/tests/e2e/themes.rs` | `test_unnumbered_heading_reference_works` | e2e |
| latex-generation | Portable LaTeX project [Acceptance B] | `crates/terse-cli/tests/e2e/latex_generation.rs` | `test_portable_output_compiles_without_terse` | e2e |
| latex-generation | Output remains editable | `crates/terse-cli/tests/latex_generation.rs` | `test_generated_sources_are_human_editable` | integration |
| latex-generation | All-element fixture renders | `crates/terse-cli/tests/e2e/latex_generation.rs` | `test_all_elements_render_under_both_themes` | e2e |
| latex-generation | Literal metacharacters and math coexist | `crates/terse-cli/tests/e2e/latex_generation.rs` | `test_escaping_preserves_literal_text_and_math` | e2e |
| latex-generation | Dangerous link scheme | `crates/terse-cli/tests/latex_generation.rs` | `test_unsafe_link_schemes_fail` | integration |
| latex-generation | Same basenames from different directories | `crates/terse-cli/tests/latex_generation.rs` | `test_colliding_asset_basenames_are_disambiguated` | integration |
| latex-generation | Unused file stays out of output | `crates/terse-cli/tests/latex_generation.rs` | `test_only_used_assets_copied` | integration |
| latex-generation | Unicode paper compiles | `crates/terse-cli/tests/e2e/latex_generation.rs` | `test_unicode_text_and_bibliography_compile` | e2e |
| latex-generation | Missing glyph is reported | `crates/terse-cli/tests/latex_generation.rs` | `test_missing_glyph_is_build_failure` | integration |
| latex-generation | Cross-file numbering converges [Acceptance E] | `crates/terse-cli/tests/e2e/latex_generation.rs` | `test_cross_file_equation_number_converges` | e2e |
| latex-generation | Source generation without installed TeX | `crates/terse-cli/tests/latex_generation.rs` | `test_tex_only_starts_no_processes` | integration |
| latex-generation | Auto mode and required PDF differ | `crates/terse-cli/tests/latex_generation.rs` | `test_auto_vs_required_pdf_without_engine` | integration |
| latex-generation | Path is not a shell command | `crates/terse-cli/tests/latex_generation.rs` | `test_subprocess_arguments_are_not_shell_text` | integration |
| latex-generation | Compilation does not converge | `crates/terse-cli/tests/latex_generation.rs` | `test_engine_pass_limit_is_enforced` | integration |
| latex-generation | Explicit drawing support | `crates/terse-cli/tests/e2e/latex_generation.rs` | `test_declared_tikz_support_compiles` | e2e |
| latex-generation | Engine error preserves prior artifacts | `crates/terse-cli/tests/latex_generation.rs` | `test_failed_engine_keeps_previous_generation` | integration |
| citations | Multiple independent locators | `crates/terse-core/src/references/tests.rs` | `test_per_work_locators_preserved` | unit |
| citations | Narrative versus parenthetical form | `crates/terse-core/src/references/tests.rs` | `test_narrative_and_parenthetical_distinct` | unit |
| citations | Unknown citation alias [Acceptance D] | `crates/terse-cli/tests/citations.rs` | `test_unknown_alias_has_actionable_source` | integration |
| citations | Leftover lock entry does not authorize a citation | `crates/terse-cli/tests/citations.rs` | `test_orphan_lock_cannot_authorize_alias` | integration |
| citations | Declared identifier changed | `crates/terse-cli/tests/citations.rs` | `test_changed_identifier_requires_resolution` | integration |
| citations | DOI resolution enables offline builds [Acceptance C] | `crates/terse-cli/tests/e2e/citations.rs` | `test_doi_resolution_then_offline_build` | e2e |
| citations | Ordinary resolution retains an unchanged record | `crates/terse-cli/tests/citations.rs` | `test_unchanged_reference_not_refetched` | integration |
| citations | Normalized DOI identity | `crates/terse-core/src/references/tests.rs` | `test_doi_normalization_is_canonical` | unit |
| citations | Provider returns the wrong work | `crates/terse-cli/tests/citations.rs` | `test_provider_identity_mismatch_rolls_back` | integration |
| citations | Latest-at-resolution becomes fixed metadata | `crates/terse-cli/tests/citations.rs` | `test_versionless_arxiv_stays_pinned` | integration |
| citations | Explicit arXiv version is honored | `crates/terse-cli/tests/citations.rs` | `test_explicit_arxiv_version_verified` | integration |
| citations | Reserved ISBN declaration | `crates/terse-cli/tests/citations.rs` | `test_reserved_reference_providers_fail` | integration |
| citations | Metadata order does not affect serialization | `crates/terse-core/src/references/tests.rs` | `test_lock_serialization_has_fixed_order` | unit |
| citations | Unparsed personal name is retained | `crates/terse-core/src/references/tests.rs` | `test_unparsed_person_name_not_guessed` | unit |
| citations | Corrected author data works offline | `crates/terse-cli/tests/citations.rs` | `test_offline_override_reseals_effective_record` | integration |
| citations | Unsealed override is diagnosed | `crates/terse-cli/tests/citations.rs` | `test_unsealed_override_requires_resolution` | integration |
| citations | Mixed resolution fails atomically | `crates/terse-cli/tests/citations.rs` | `test_resolution_is_all_or_nothing` | integration |
| citations | Explicit pruning | `crates/terse-cli/tests/citations.rs` | `test_prune_is_explicit` | integration |
| citations | Serialization and presentation ordering are independent | `crates/terse-cli/tests/e2e/citations.rs` | `test_bib_order_vs_display_order` | e2e |
| multi-file-projects | Entry discovery from a subdirectory | `crates/terse-cli/tests/multi_file_projects.rs` | `test_manifest_discovery_from_descendant` | integration |
| multi-file-projects | Missing or invalid manifest | `crates/terse-cli/tests/multi_file_projects.rs` | `test_missing_or_invalid_manifest_fails` | integration |
| multi-file-projects | Explicit settings win without environment drift | `crates/terse-cli/tests/multi_file_projects.rs` | `test_cli_precedence_is_environment_independent` | integration |
| multi-file-projects | Asset in a sibling directory | `crates/terse-cli/tests/multi_file_projects.rs` | `test_relative_sibling_asset_resolves` | integration |
| multi-file-projects | Theme-relative logo | `crates/terse-cli/tests/multi_file_projects.rs` | `test_logo_resolves_from_theme_directory` | integration |
| multi-file-projects | Repeated module without IDs | `crates/terse-cli/tests/multi_file_projects.rs` | `test_repeated_include_repeats_content` | integration |
| multi-file-projects | Wildcard inclusion is rejected | `crates/terse-cli/tests/multi_file_projects.rs` | `test_wildcard_include_is_invalid` | integration |
| multi-file-projects | Indirect include cycle [Acceptance F] | `crates/terse-cli/tests/multi_file_projects.rs` | `test_indirect_include_cycle_is_complete` | integration |
| multi-file-projects | Include target is absent | `crates/terse-cli/tests/multi_file_projects.rs` | `test_missing_include_is_source_located` | integration |
| multi-file-projects | Multi-file equation reference [Acceptance E] | `crates/terse-cli/tests/e2e/multi_file_projects.rs` | `test_multi_file_equation_link_valid` | e2e |
| multi-file-projects | Duplicate definitions are actionable | `crates/terse-cli/tests/multi_file_projects.rs` | `test_duplicate_ids_show_both_routes` | integration |
| multi-file-projects | Wrong proof target | `crates/terse-cli/tests/multi_file_projects.rs` | `test_proof_of_requires_theorem_target` | integration |
| multi-file-projects | Declaration and citation in different modules | `crates/terse-cli/tests/multi_file_projects.rs` | `test_citations_bind_across_modules` | integration |
| multi-file-projects | Multiple bibliography markers | `crates/terse-cli/tests/multi_file_projects.rs` | `test_bibliography_markers_are_global` | integration |
| multi-file-projects | Symlink escapes the project | `crates/terse-cli/tests/multi_file_projects.rs` | `test_symlink_escape_never_read` | integration |
| multi-file-projects | Unsafe output setting | `crates/terse-cli/tests/multi_file_projects.rs` | `test_output_scope_cannot_overlap_inputs` | integration |
| multi-file-projects | Missing figure and unsupported format | `crates/terse-cli/tests/multi_file_projects.rs` | `test_invalid_figure_dependencies_fail` | integration |
| multi-file-projects | Newly referenced missing file can be repaired | `crates/terse-cli/tests/multi_file_projects.rs` | `test_failed_lookup_retained_as_dependency` | integration |
| git-oriented-tooling | Initialize a usable project | `crates/terse-cli/tests/git_oriented_tooling.rs` | `test_init_scaffold_is_usable` | integration |
| git-oriented-tooling | Existing source is protected | `crates/terse-cli/tests/git_oriented_tooling.rs` | `test_init_conflict_is_preflighted` | integration |
| git-oriented-tooling | Ignore entries are additive | `crates/terse-cli/tests/git_oriented_tooling.rs` | `test_init_ignore_merge_is_additive` | integration |
| git-oriented-tooling | Check without a TeX installation | `crates/terse-cli/tests/git_oriented_tooling.rs` | `test_check_requires_no_engine` | integration |
| git-oriented-tooling | Theme selection controls validation scope | `crates/terse-cli/tests/git_oriented_tooling.rs` | `test_check_theme_scope_is_explicit` | integration |
| git-oriented-tooling | Formatting stability [Acceptance H] | `crates/terse-cli/tests/git_oriented_tooling.rs` | `test_format_is_semantic_and_idempotent` | integration |
| git-oriented-tooling | Opaque payload and paragraph wrapping are preserved | `crates/terse-core/src/syntax/format_tests.rs` | `test_formatter_preserves_opaque_bytes` | unit |
| git-oriented-tooling | Malformed file prevents batch writes | `crates/terse-cli/tests/git_oriented_tooling.rs` | `test_batch_format_parse_failure_writes_nothing` | integration |
| git-oriented-tooling | CI detects formatting drift | `crates/terse-cli/tests/git_oriented_tooling.rs` | `test_format_check_is_read_only` | integration |
| git-oriented-tooling | Deterministic output [Acceptance G] | `crates/terse-cli/tests/git_oriented_tooling.rs` | `test_text_artifacts_are_reproducible` | integration |
| git-oriented-tooling | Clean and cached builds agree | `crates/terse-cli/tests/git_oriented_tooling.rs` | `test_cache_is_disposable` | integration |
| git-oriented-tooling | CI observes failure categories | `crates/terse-cli/tests/git_oriented_tooling.rs` | `test_exit_codes_and_json_are_meaningful` | integration |
| git-oriented-tooling | Publication failure rolls back | `crates/terse-cli/tests/git_oriented_tooling.rs` | `test_publication_rollback_and_restart_recovery` | integration |
| git-oriented-tooling | Unowned destination is protected | `crates/terse-cli/tests/git_oriented_tooling.rs` | `test_unowned_output_is_never_replaced` | integration |
| git-oriented-tooling | A contributor reproduces the demonstration | `crates/terse-cli/tests/e2e/git_oriented_tooling.rs` | `test_public_checkout_full_user_workflow` | e2e |
| git-oriented-tooling | One maintainer can run release validation | `crates/terse-cli/tests/e2e/git_oriented_tooling.rs` | `test_maintainer_can_reproduce_release_checks` | e2e |
| diagnostics | Duplicate identifier reports both definitions | `crates/terse-cli/tests/diagnostics.rs` | `test_duplicate_id_diagnostic_has_two_origins` | integration |
| diagnostics | Tool startup has no invented source line | `crates/terse-cli/tests/diagnostics.rs` | `test_tool_startup_error_has_honest_span` | integration |
| diagnostics | Unicode before a cited alias | `crates/terse-cli/tests/diagnostics.rs` | `test_unicode_columns_match_original_bytes` | integration |
| diagnostics | Strict raw warning remains recognizable | `crates/terse-cli/tests/diagnostics.rs` | `test_deny_warnings_does_not_rename_code` | integration |
| diagnostics | CI parses repeated validation failures | `crates/terse-cli/tests/diagnostics.rs` | `test_json_diagnostics_are_stable` | integration |
| diagnostics | Watch distinguishes failed and published attempts | `crates/terse-cli/tests/diagnostics.rs` | `test_watch_json_reports_each_attempt` | integration |
| diagnostics | Engine error in included raw block | `crates/terse-cli/tests/diagnostics.rs` | `test_engine_error_maps_to_included_raw_source` | integration |
| diagnostics | Unmappable package failure | `crates/terse-core/src/diagnostic/tests.rs` | `test_unmappable_error_stays_generated` | unit |
| diagnostics | Several independent source errors | `crates/terse-cli/tests/diagnostics.rs` | `test_bounded_error_recovery_never_publishes` | integration |
| watch-mode | Initial syntax error can be repaired | `crates/terse-cli/tests/watch_mode.rs` | `test_watch_recovers_from_initial_error` | integration |
| watch-mode | Atomic replacement and asset changes rebuild | `crates/terse-cli/tests/watch_mode.rs` | `test_atomic_saves_and_asset_edits_trigger_rebuild` | integration |
| watch-mode | Creating a missing include repairs the build | `crates/terse-cli/tests/watch_mode.rs` | `test_missing_dependency_creation_is_detected` | integration |
| watch-mode | Generated output does not cause a loop | `crates/terse-cli/tests/watch_mode.rs` | `test_output_and_cache_events_are_ignored` | integration |
| watch-mode | Burst of filesystem events | `crates/terse-cli/src/watch/tests.rs` | `test_events_use_trailing_debounce` | unit |
| watch-mode | Input changes while compilation runs | `crates/terse-cli/tests/watch_mode.rs` | `test_stale_snapshot_never_published` | integration |
| watch-mode | Watch after error [Acceptance K] | `crates/terse-cli/tests/e2e/watch_mode.rs` | `test_watch_keeps_last_good_pdf_after_syntax_error` | e2e |
| watch-mode | Lock and engine failures also preserve output | `crates/terse-cli/tests/watch_mode.rs` | `test_watch_retains_output_on_lock_or_engine_failure` | integration |
| watch-mode | New selected theme becomes active | `crates/terse-cli/tests/watch_mode.rs` | `test_watch_updates_theme_dependency_graph` | integration |
| watch-mode | Interrupt during an attempt | `crates/terse-cli/tests/watch_mode.rs` | `test_interrupt_stops_owned_processes` | integration |
| arxiv-export | Current source controls export | `crates/terse-cli/tests/arxiv_export.rs` | `test_export_validates_current_sources` | integration |
| arxiv-export | arXiv package [Acceptance L] | `crates/terse-cli/tests/e2e/arxiv_export.rs` | `test_arxiv_archive_compiles_in_clean_environment` | e2e |
| arxiv-export | Unused files and rendered paper are excluded | `crates/terse-cli/tests/arxiv_export.rs` | `test_export_allowlist_excludes_cruft` | integration |
| arxiv-export | Missing or external dependency [Acceptance L] | `crates/terse-cli/tests/arxiv_export.rs` | `test_external_or_missing_export_dependency_fails` | integration |
| arxiv-export | Portable filename collision | `crates/terse-core/src/artifact/tests.rs` | `test_archive_paths_are_portable` | unit |
| arxiv-export | Default export avoids local BBL version coupling | `crates/terse-cli/tests/arxiv_export.rs` | `test_default_export_omits_prebuilt_bbl` | integration |
| arxiv-export | Requested incompatible BBL is rejected | `crates/terse-cli/tests/arxiv_export.rs` | `test_include_bbl_requires_verified_compatibility` | integration |
| arxiv-export | Normally valid raw drawing is not silently omitted | `crates/terse-cli/tests/arxiv_export.rs` | `test_raw_export_rejection_preserves_content` | integration |
| arxiv-export | Repeated export is deterministic | `crates/terse-cli/tests/arxiv_export.rs` | `test_zip_bytes_and_manifest_are_deterministic` | integration |
| arxiv-export | Local toolchain differs from target profile | `crates/terse-cli/tests/arxiv_export.rs` | `test_local_compile_is_not_claimed_as_profile_match` | integration |
| arxiv-export | No compatible engine is available | `crates/terse-cli/tests/arxiv_export.rs` | `test_no_engine_reports_static_only_or_fails_required` | integration |
| arxiv-export | Clean compilation uncovers a hidden dependency | `crates/terse-cli/tests/arxiv_export.rs` | `test_recorder_detects_hidden_dependency` | integration |
| arxiv-export | Archive or validation fails after staging | `crates/terse-cli/tests/arxiv_export.rs` | `test_export_transaction_preserves_consistent_generation` | integration |

### Coverage maintenance

Before considering this artifact complete, compare the set of (capability, scenario heading) pairs with specs/*/spec.md, require an exact match in both the detailed mapping and summary table, and verify each mapping has a type, file, unique name, setup, action, assertions, and edge cases. Confirm that every requirement owns a mapped scenario and that every A–L marker remains represented. Reject placeholders, duplicate mappings, missing cases, stale summary rows, and nonexistent links to planning artifacts; future test paths are intentionally not checked for existence before implementation.

During implementation, extend this audit to verify that each named test is registered/discovered and that required cases actually execute in their assigned lane. Additional boundary cases can be added without rewriting existing scenario names. Update the spec, mapping, and acceptance evidence together if a behavior is intentionally changed; do not silently drop tests to make a release green.
