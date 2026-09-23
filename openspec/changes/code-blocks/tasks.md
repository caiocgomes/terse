## 1. Opaque-aware lexing

- [ ] 1.1 Write tests: `test_opaque_payload_lines_are_not_structural` in `syntax/tests.rs`. Confirm it fails (today `E-PARSE-011`/`E-PARSE-010`).
- [ ] 1.2 Assess the blast radius of `lex_lines`, `parse_opaque_payload`, `parse_dollar_display`, and `consume_paragraph` (GitNexus impact, falling back to grep for Rust symbols it cannot resolve) and report it.
- [ ] 1.3 Add `syntax/opaque.rs` with the shared `opener` recognizer (`Indented` for exact `math:`/`math [..]:`/`tex:`, `Dollar`, `Fence { len }`) and use it from the block parser where `$$` and headers are detected today [tests: test_opaque_payload_lines_are_not_structural]
- [ ] 1.4 Make `lex_lines` a single-pass state machine: payload lines get the region's fixed indent and bytes after the prefix, never touch the stack, and end by closer, shorter prefix, or EOF [tests: test_opaque_payload_lines_are_not_structural, test_invalid_structural_indentation, test_line_endings_preserve_semantics]
- [ ] 1.5 Add `opaque: bool` to `StructLine`, and make `parse_module_with_recovery` skip opaque lines when searching for its next boundary [tests: test_malformed_code_blocks_fail]
- [ ] 1.6 Run the full existing suite unchanged (every `math:`/`tex:`/`$$` test) and confirm green

## 2. Fenced code blocks in the syntax and model

- [ ] 2.1 Write tests: `test_fenced_code_block_is_byte_exact`, `test_fence_ends_running_paragraph`, `test_code_block_in_list_item`, `test_long_fences_and_untagged_blocks`, `test_inline_triple_backticks_stay_prose`, `test_malformed_code_blocks_fail`. Confirm they fail.
- [ ] 2.2 Add `TopBlock::CodeBlock`, `NodeKind::CodeBlock`, and `ProjectedNode::CodeBlock`; update every exhaustive match the compiler reports (semantic, assets, projection, tests) [tests: test_fenced_code_block_is_byte_exact]
- [ ] 2.3 Parse fences in `parse_block_sequence` for Module, Nested, and ListItem contexts; stop `consume_paragraph` at a fence opener; take the language tag from the info string [tests: test_fenced_code_block_is_byte_exact, test_fence_ends_running_paragraph, test_code_block_in_list_item, test_long_fences_and_untagged_blocks, test_inline_triple_backticks_stay_prose]
- [ ] 2.4 Reject unterminated fences at the opener and `\end{TerseCode}` (whitespace variants) at the content line, both `E-PARSE-002` [tests: test_malformed_code_blocks_fail]
- [ ] 2.5 Add `CodeBlock` spans to `collect_opaque_ranges` [tests: test_format_preserves_code_blocks]

## 3. Emission through listings

- [ ] 3.1 Write tests: `test_python_code_block_emits_terse_code`, `test_unknown_code_tags_emit_plain_environment` in `latex/mod.rs`; `test_code_block_is_inert_in_pdf` in `crates/terse-cli/tests/e2e/latex_generation.rs`. Confirm they fail.
- [ ] 3.2 Add `listings` to the always-loaded core set with the tested `\lstset` and `\lstnewenvironment{TerseCode}[1][]{\lstset{#1}}{}` [tests: test_python_code_block_emits_terse_code]
- [ ] 3.3 Emit `TerseCode` with the closed language map; unknown or missing tags emit no option [tests: test_python_code_block_emits_terse_code, test_unknown_code_tags_emit_plain_environment]
- [ ] 3.4 Run the e2e test against a local XeLaTeX [tests: test_code_block_is_inert_in_pdf]

## 4. Package closure

- [ ] 4.1 Add `listings` to `STYLE_PACKAGES` and to the export profile's `packages` [tests: test_profile_packages_equal_emitted_set]
- [ ] 4.2 Add a Python code block to `tests/fixtures/full-paper`
- [ ] 4.3 Run `scripts/derive-toolchain-closure.sh` (needs network access to the pinned repository) and commit the regenerated `[closure].derived`
- [ ] 4.4 Run the pinned closure e2e test [tests: test_pinned_closure_covers_full_paper_inputs]

## 5. Docs and closure

- [ ] 5.1 Document fenced code blocks, the language map, inert content, the forbidden end sequence, list-item placement, and opaque payload indentation in `docs/language.md`
- [ ] 5.2 Run the full workspace suite, clippy (no new warnings), and rustfmt limited to new hunks
- [ ] 5.3 Run `gitnexus_detect_changes`, commit, and reindex
