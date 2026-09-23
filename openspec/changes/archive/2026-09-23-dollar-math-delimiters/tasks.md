## 1. Inline `$...$`

- [x] 1.1 Write tests: `test_dollar_inline_math_equals_paren_math`, `test_currency_dollars_stay_text`, `test_escaped_dollars`, `test_midline_display_dollars_rejected` in `syntax/inlines.rs`; `test_dollar_math_rejects_execution` in `syntax/tests.rs`; `test_dollar_inline_math_generates_like_paren_math`, `test_currency_dollars_are_escaped_in_output` in `latex/mod.rs`. Confirm they fail.
- [x] 1.2 Run `gitnexus_impact` on `parse_until` and `parse_math` in `inlines.rs` and report the blast radius (`gitnexus_impact` returned UNKNOWN with zero callers for these symbols both before and after reindex: the index does not resolve Rust enum uses or method calls. Blast radius taken from grep instead: the only entry point is `parse_inline`, called at `semantic/mod.rs:799` for all inline text).
- [x] 1.3 Add `$` dispatch in the inline loop: Pandoc opener/closer rule, skip `\$` while scanning, fall back to literal text when there is no valid closer, and emit `Inline::Math` [tests: test_dollar_inline_math_equals_paren_math, test_currency_dollars_stay_text, test_escaped_dollars, test_dollar_math_rejects_execution, test_dollar_inline_math_generates_like_paren_math, test_currency_dollars_are_escaped_in_output]
- [x] 1.4 Reject `$$` inside the inline parser with the "own line" diagnostic [tests: test_midline_display_dollars_rejected]
- [x] 1.5 Re-run the existing inline suite (`test_mixed_inline_paragraph`, `test_crossing_and_nested_delimiters_fail`, `test_crossing_delimiters_fail`, `test_triple_asterisk_rejected`, `test_code_and_math_are_literal`) and confirm it still passes

## 2. Numbered flag on equations

- [x] 2.1 Write tests: extend `test_theorem_proof_equation_structure` to assert `numbered: true`. Confirm it fails. (`test_projection_distinguishes_numbered_equations` moved to 3.1, because it needs `$$` to parse.)
- [x] 2.2 Run `gitnexus_impact` on `TopBlock::Equation`, `NodeKind::Equation`, and `ProjectedNode::Equation`, and report d=1 dependents (`gitnexus_impact` returned UNKNOWN with zero callers for these symbols both before and after reindex: the index does not resolve Rust enum uses or method calls. Blast radius taken from grep instead: 4 production sites destructure every field, `blocks.rs`, `semantic/mod.rs`, `projection.rs`, `latex/mod.rs`; the others use `..`).
- [x] 2.3 Add `numbered: bool` to the syntax, semantic, and projection equation nodes; `math:` sets `true`; update every match arm the impact analysis lists [tests: test_theorem_proof_equation_structure]

## 3. Display `$$` blocks

- [x] 3.1 Write tests: `test_multiline_dollar_display_splits_paragraph`, `test_single_line_dollar_display`, `test_malformed_dollar_display_fails` in `syntax/tests.rs`; `test_projection_distinguishes_numbered_equations` in `semantic/tests.rs`; `test_format_preserves_dollar_math` in `syntax/format_tests.rs`. Confirm they fail.
- [x] 3.2 Run `gitnexus_impact` on `consume_paragraph` and `parse_block_sequence`, and report the blast radius (`gitnexus_impact` returned UNKNOWN with zero callers for these symbols both before and after reindex: the index does not resolve Rust enum uses or method calls. Blast radius taken from grep instead: `consume_paragraph` is also called by the abstract parser and field values, where a `$$` line now fails as inline `$$`).
- [x] 3.3 Parse `$$` blocks (single-line and multi-line) into `TopBlock::Equation { id: None, numbered: false, .. }`, reusing the opaque-payload dedent from `parse_equation`; reject trailing text, an unterminated block, and placement outside equation contexts [tests: test_multiline_dollar_display_splits_paragraph, test_single_line_dollar_display, test_malformed_dollar_display_fails, test_projection_distinguishes_numbered_equations]
- [x] 3.4 Make `consume_paragraph` stop at a line starting with `$$` [tests: test_multiline_dollar_display_splits_paragraph]
- [x] 3.5 Make `fmt` treat the `$$` payload as opaque (no code needed: `format.rs:30` already keeps an `Equation` span byte-for-byte) [tests: test_format_preserves_dollar_math]

## 4. Unnumbered emission

- [x] 4.1 Write tests: `test_unnumbered_dollar_display_emits_brackets` in `latex/mod.rs`; `test_dollar_display_does_not_consume_equation_number` in `crates/terse-cli/tests/e2e/latex_generation.rs`. Confirm they fail.
- [x] 4.2 Run `gitnexus_impact` on the equation arm of the LaTeX body emitter and report the blast radius (`gitnexus_impact` returned UNKNOWN with zero callers for these symbols both before and after reindex: the index does not resolve Rust enum uses or method calls. Blast radius taken from grep instead: `render_node` is reached per node by the body and by `source_map.rs:124`, which maps any node kind).
- [x] 4.3 Emit `\[` + payload + `\]` for unnumbered equations; keep `TerseEquation` for numbered ones; confirm the source map still maps payload lines [tests: test_unnumbered_dollar_display_emits_brackets, test_dollar_display_does_not_consume_equation_number]

## 5. Docs and closure

- [x] 5.1 Document `$...$`, `$$...$$`, the currency rule, `\$`, and "use `math [id:]:` when you need a number or reference" in `docs/language.md`
- [x] 5.2 Run the full `cargo test` workspace suite, plus `cargo clippy` and `cargo fmt --check` (clippy: 55 warnings before and after, none new; `fmt --check` already fails on untouched files such as `args.rs`, so it is not a gate here)
- [x] 5.3 Run `gitnexus_detect_changes` and confirm only the expected symbols changed; commit and reindex

## 6. Verification follow-ups

- [x] 6.1 Write test: `test_blank_or_empty_dollar_display_fails`. Confirm it fails.
- [x] 6.2 Reject a blank line inside `$$` at that line, and an empty or whitespace-only payload at the opening `$$` [tests: test_blank_or_empty_dollar_display_fails]
- [x] 6.3 Spec: add the blank/empty rule and scenario; reword "Mid-line display dollars are rejected" to the paragraph-level `E-PARSE-050` it actually produces
- [x] 6.4 Design: correct D2's parity claim with `math:`; add the URL-closes-a-price risk (same output as Pandoc 3.11); docs: document both
