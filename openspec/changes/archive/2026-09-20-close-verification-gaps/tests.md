## Test Strategy

This plan follows the conventions the `terse` and `managed-toolchain` changes established: the standard Rust runner, `terse-core` unit tests in module `tests.rs` files, `terse-cli` integration tests under `crates/terse-cli/tests/`, heavy cases declared from `crates/terse-cli/tests/e2e.rs` with an explicit `#[ignore]` reason. `cargo` is `~/.cargo/bin/cargo`. Baseline before this change: 346 passed, 0 failed, 18 ignored.

One rule governs every case below, because ignoring it is what produced this change: **a test must be able to fail.** The verification found assertions on theme-blind functions, on `is_ok()`, and on names, none of which can distinguish a working implementation from an inert one. Concretely, for theme work that means asserting on the emitted `terse-style.sty` text or on the rendered PDF, never on `generate_document` output, which is invariant by construction. For wiring work it means asserting on the published artifact set, never on the generating function called directly.

Existing tests that this change must strengthen rather than duplicate are named in group 4 of `tasks.md`; each keeps its name and gains assertions.

### Fixtures

`tests/fixtures/themes/` gains a third theme exercising the settings that are inert today (letter paper, wide margin, two columns, a distinct body color, numeric-versus-author-year citation, a logo, a cover title). The two existing themes stay as they are, because their current declarations are precisely the evidence of the defect: they already ask for A4, margins, citation styles and a logo that never arrive.

### The committed fixture layout, and why it is not the one `terse` promised

`terse`'s own `tests.md` promised seven fixture groups: `minimal`, `invalid`, `expected`, `engine-logs`, `full-paper`, `providers`, `raw-tex`. Six exist, and only three of the promised names are among them:

| Group | Files | Purpose |
|---|---|---|
| `assets` | 1 | A tiny valid PNG, shared by every test that needs a real image. |
| `full-paper` | 9 | The multi-file acceptance paper: three `.trs` modules, manifest, committed lock, figure, both themes. |
| `providers` | 7 | Recorded CSL-JSON and Atom responses, including malformed and DTD cases. |
| `raw-tex` | 3 | The trusted drawing example kept separate from the semantic-only export fixture. |
| `themes` | 5 | The example themes and their logo asset. |
| `toolchain` | 9 | The fake managed prefix, the `tlpdb` excerpt, the `install-tl` profile, the doctor golden. |

The four promised-but-absent groups are not an oversight to be corrected by creating empty directories; each names a fixture *style* this codebase deliberately did not adopt, and committing to it now would make the contract harder to keep true rather than easier.

`minimal` would duplicate what `terse init` scaffolds. The scaffold is the minimal project, `test_init_scaffold_is_usable` exercises it directly, and a committed copy would drift from the generator silently — the same class of defect this whole change exists to remove.

`invalid` and `engine-logs` would move malformed sources and engine logs out of the tests that assert on them. Those cases are table-driven and read better inline, where the bad input sits beside the expectation it violates; the strengthened tests in group 4 are a direct demonstration, since each row now pairs an input with its own code, message and position.

`expected` would hold byte goldens for generated text, and the project has none by choice: assertions are structural, as `tests/doctor.rs` does against `fixtures/toolchain/doctor-report.json`. Group 3 reached the same conclusion for `paper.map.json`.

So `terse` task 1.3 and the `tests/fixtures/expected/` half of task 17.5 are closed by recording the real layout, not by building the promised one. Both are annotated accordingly when this change is archived.

## Spec-to-Test Mapping

### Capability: themes

#### Scenario: Declared settings reach the output

- **Test type**: unit
- **Test file**: `crates/terse-core/src/theme/tests.rs`
- **Test name**: `test_every_accepted_property_reaches_the_style`
- **Setup (GIVEN)**: A theme declaring a non-default value for every property the schema accepts.
- **Action (WHEN)**: `generate_style` on the resolved theme.
- **Assert (THEN)**: Table-driven, one row per property, each asserting the specific substring the property must produce (`geometry` options, `\setmainfont` filename, `\color`, `style=authoryear`, `\TerseFigurePlacement`, `\TerseLogo` invocation, theorem/table/bibliography/title macros). A property with no row fails the test, so the table cannot silently lag the schema.
- **Edge cases**: The same theme under `generate_document` produces bytes identical to the default theme's, proving the body stayed theme-blind.

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/themes.rs`
- **Test name**: `test_declared_settings_are_visible_in_rendered_pdfs`
- **Setup (GIVEN)**: The full-paper fixture and two themes differing in page size, margin, columns, body font, color, citation style, and logo.
- **Action (WHEN)**: `build --require-pdf` under each.
- **Assert (THEN)**: Page geometry differs (`pdfinfo` page size or measured text width), citation form differs (`[1]` versus author-year in extracted text), the logo is present in one and absent in the other, and both render every fixture element.

#### Scenario: Deferred component is not silently accepted

- **Test type**: unit
- **Test file**: `crates/terse-core/src/theme/tests.rs`
- **Test name**: `test_deferred_components_are_unknown`
- **Setup (GIVEN)**: Themes declaring `header`, `footer`, `contents`, `equation`, `proof`.
- **Action (WHEN)**: Resolve each.
- **Assert (THEN)**: Each fails naming the selector as an unknown component, with its span; not a generic error, and not acceptance followed by an unknown-property error.

#### Scenario: Variant attribute key is fixed per component

- **Test type**: unit
- **Test file**: `crates/terse-core/src/theme/tests.rs`
- **Test name**: `test_variant_attribute_key_is_fixed_per_component`
- **Setup (GIVEN)**: Themes declaring `theorem[kind=lemma]`, `theorem[role=lemma]`, and `figure[kind=wide]`.
- **Action (WHEN)**: Resolve each.
- **Assert (THEN)**: The first resolves; the other two fail as invalid selectors naming the offending key.
- **Edge cases**: A theme declaring both `theorem[kind=lemma]` and `theorem[role=lemma]` fails on the invalid key, not as a duplicate selector, proving the two forms are not conflated.

#### Scenario: Corporate and academic presentations

- Strengthened existing test `test_two_theme_presentations_differ`: extend beyond the watermark to the settings this change activates.

#### Scenario: Invalid typed value

- Existing `test_typed_theme_values_rejected`, extended to the new components' value types.

#### Scenario: Theorem kind inherits its base

- **Test type**: unit
- **Test file**: `crates/terse-core/src/theme/tests.rs`
- **Test name**: `test_theorem_kind_overrides_base`
- **Setup (GIVEN)**: A `theorem` base value and a differing `theorem[kind=lemma]` value.
- **Action (WHEN)**: Resolve and generate the style.
- **Assert (THEN)**: The lemma environment carries the kind value, the theorem environment the base value, the remaining theorem-like kinds the base value; and the resolution is unchanged when the two selectors are declared in the opposite order.

#### Scenarios: Wide figure override, Reordered selectors

- Existing `test_role_overrides_base_width` and `test_theme_selector_order_irrelevant`, unchanged.

### Capability: latex-generation

#### Scenario: Complete readable LaTeX deliverables (`paper.map.json`)

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/latex_generation.rs`
- **Test name**: `test_source_map_is_a_published_deliverable`
- **Setup (GIVEN)**: The multi-file full-paper fixture.
- **Action (WHEN)**: `build --tex-only`.
- **Assert (THEN)**: `paper.map.json` is in the published set and in `build-manifest.json`'s hashed entries; it parses; it carries a schema version; every path in it is root-relative with `/` separators and no numeric file id or absolute path; and an interval resolves to the included module, not the entry.
- **Edge cases**: Asserted structurally, following `tests/doctor.rs`'s golden convention, not as a byte golden, since no byte goldens exist for generated text today.

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/arxiv_export.rs`
- **Test name**: `test_source_map_is_excluded_from_export`
- **Assert (THEN)**: The archive's independently enumerated member list contains no map, alongside the existing report and manifest exclusions.

#### Scenario: Compilation does not converge (unresolved references)

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/latex_generation.rs`
- **Test name**: `test_undefined_references_fail_the_build`
- **Setup (GIVEN)**: A fake runner whose final XeLaTeX pass log carries `There were undefined references`, and a previous published generation hashed beforehand.
- **Action (WHEN)**: `build --require-pdf`.
- **Assert (THEN)**: Exit 3 with a distinct code, and the previous generation is byte-identical afterwards.
- **Edge cases**: A log where only an intermediate pass reports undefined references and the final pass is clean must succeed, which is the regression the `.any()`-over-all-passes shape would cause.

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/latex_generation.rs`
- **Test name**: `test_raw_tex_undefined_reference_fails_against_real_engine`
- **Assert (THEN)**: A `tex:` block containing `\ref{nope}` fails the build with a real XeLaTeX, rather than publishing a PDF containing `??`.

### Capability: semantic-ast

#### Scenario: Resource limits fail boundedly

- Existing `test_resource_limits_fail_boundedly`, extended from nesting alone to all four categories: a source exceeding the byte cap, a module exceeding the node cap, an include chain exceeding the depth cap, and the existing nesting and diagnostic caps. Each asserts its own `E-LIMIT-*` code and that no artifact plan is produced.
- **Edge case**: A chain one level below the include-depth cap succeeds, so the boundary is exercised from both sides.

#### Scenario: Theme-invariant projection

- Existing `test_theme_invariant_projection`, extended: two documents whose only difference is the effective locked metadata of a cited work must now produce different projections and different digests, which is impossible today.

### Capability: git-oriented-tooling

#### Scenario: Reproducible generated text

- Existing `test_text_artifacts_are_reproducible`, extended to cover `paper.map.json` across clean roots and differing mtimes.

## Coverage Summary

| Capability | Scenario | Test file | Test name | Type |
|---|---|---|---|---|
| themes | Declared settings reach the output | `crates/terse-core/src/theme/tests.rs` | `test_every_accepted_property_reaches_the_style` | unit |
| themes | Declared settings reach the output | `crates/terse-cli/tests/e2e/themes.rs` | `test_declared_settings_are_visible_in_rendered_pdfs` | e2e |
| themes | Deferred component is not silently accepted | `crates/terse-core/src/theme/tests.rs` | `test_deferred_components_are_unknown` | unit |
| themes | Variant attribute key is fixed per component | `crates/terse-core/src/theme/tests.rs` | `test_variant_attribute_key_is_fixed_per_component` | unit |
| themes | Theorem kind inherits its base | `crates/terse-core/src/theme/tests.rs` | `test_theorem_kind_overrides_base` | unit |
| themes | Corporate and academic presentations | `crates/terse-cli/tests/e2e/themes.rs` | `test_two_theme_presentations_differ` | e2e |
| themes | Invalid typed value | `crates/terse-core/src/theme/tests.rs` | `test_typed_theme_values_rejected` | unit |
| latex-generation | Complete readable LaTeX deliverables | `crates/terse-cli/tests/latex_generation.rs` | `test_source_map_is_a_published_deliverable` | integration |
| latex-generation | Package membership is minimal and explicit | `crates/terse-cli/tests/arxiv_export.rs` | `test_source_map_is_excluded_from_export` | integration |
| latex-generation | Compilation does not converge | `crates/terse-cli/tests/latex_generation.rs` | `test_undefined_references_fail_the_build` | integration |
| latex-generation | Compilation does not converge | `crates/terse-cli/tests/e2e/latex_generation.rs` | `test_raw_tex_undefined_reference_fails_against_real_engine` | e2e |
| semantic-ast | Bounded processing | `crates/terse-core/src/semantic/tests.rs` | `test_resource_limits_fail_boundedly` | unit |
| semantic-ast | Theme-invariant projection | `crates/terse-core/tests/themes.rs` | `test_theme_invariant_projection` | integration |
| git-oriented-tooling | Reproducible generated text | `crates/terse-cli/tests/git_oriented_tooling.rs` | `test_text_artifacts_are_reproducible` | integration |

Group 4 of `tasks.md` strengthens existing tests without renaming them; they keep their current mapping in `terse`'s own `tests.md` and are not re-listed here. New tests: 8 primary (4 unit, 3 integration, 2 e2e, counting the two e2e separately), plus assertions added to 6 existing tests. The discovery floor in `test_maintainer_can_reproduce_release_checks` rises accordingly.
