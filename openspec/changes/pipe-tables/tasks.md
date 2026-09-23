## 1. Table model with inline cells, optional caption, and alignment

- [ ] 1.1 Write tests: update `test_figure_table_fields_are_semantic` for inline cells and default alignment; `test_invalid_tables_fail`. Confirm they fail.
- [ ] 1.2 Assess the blast radius of `TopBlock::Table`, `NodeKind::Table`, `ProjectedNode::Table`, `parse_table`, and the table arm of `render_node` (GitNexus impact, falling back to grep for Rust symbols it cannot resolve) and report it.
- [ ] 1.3 Change the three `Table` variants to `caption: Option<…>`, `align: Vec<ColumnAlign>`, and inline header and rows; lower caption and cells with `parse_inline_at`, rejecting footnotes with `E-META-018` [tests: test_figure_table_fields_are_semantic]
- [ ] 1.4 Make `caption:` optional, `id` without a caption `E-META-017`, short rows padded, long rows `E-META-015`; stop emitting `E-META-012` [tests: test_invalid_tables_fail]

## 2. Walkers descend into tables

- [ ] 2.1 Write test: `test_table_cell_citations_and_refs_are_live` in `semantic/tests.rs`. Confirm it fails.
- [ ] 2.2 Make `contains_citation`, `validate_cross_refs`, `check_citations`, and `collect_cited_in_nodes` walk the table caption, header, and rows [tests: test_table_cell_citations_and_refs_are_live]

## 3. Pipe-table syntax

- [ ] 3.1 Write tests: `test_bare_pipe_table`, `test_pipe_cell_splitting_protects_math_and_code`, `test_captioned_pipe_table_block`, `test_pipe_prose_and_malformed_pipe_tables`. Confirm they fail.
- [ ] 3.2 Expose the inline dollar-closer rule as a crate-level helper and write the cell splitter (code spans, `$...$`, `\(...\)` protected; `\|` handling) [tests: test_pipe_cell_splitting_protects_math_and_code]
- [ ] 3.3 Recognize bare pipe tables in `parse_block_sequence` (Module and Nested) with delimiter-row lookahead; stop `consume_paragraph` at a pipe table; parse alignments [tests: test_bare_pipe_table, test_pipe_prose_and_malformed_pipe_tables]
- [ ] 3.4 Accept a pipe body in the `table [id: ...]:` block, rejecting a mix with `header:`/`rows:` [tests: test_captioned_pipe_table_block, test_pipe_prose_and_malformed_pipe_tables]

## 4. Emission and numbering

- [ ] 4.1 Write tests: `test_captionless_table_is_in_place`, `test_pipe_table_alignment_and_inline_cells` in `latex/mod.rs`; `test_captionless_table_does_not_consume_a_number`, `test_equation_numbering_follows_delimiter` in `crates/terse-cli/tests/e2e/latex_generation.rs`. Confirm the unit tests fail.
- [ ] 4.2 Emit the column spec from alignments and render caption and cells as inlines [tests: test_pipe_table_alignment_and_inline_cells]
- [ ] 4.3 Add `TerseTableHere` to the style and emit captionless tables in it, without float, caption, or label [tests: test_captionless_table_is_in_place]
- [ ] 4.4 Run the two e2e tests against a local XeLaTeX [tests: test_captionless_table_does_not_consume_a_number, test_equation_numbering_follows_delimiter]

## 5. Docs and closure

- [ ] 5.1 Document pipe tables, alignment, inline cells and captions, optional captions, the in-place captionless table, and the splitting rules in `docs/language.md`
- [ ] 5.2 Run the full workspace suite (including `test_all_elements_render_under_both_themes` and `test_cross_file_equation_number_converges`), clippy (no new warnings), and rustfmt limited to new hunks
- [ ] 5.3 Run `gitnexus_detect_changes`, commit, and reindex
