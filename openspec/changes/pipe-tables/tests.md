## Test Strategy

Tests use Rust `#[test]` through `cargo test`, in the existing layout:

- **Parse and lower:** `crates/terse-core/src/syntax/tests.rs` (`parse_text`, `try_parse_diagnostics`).
- **Cell splitter:** unit tests beside it in `crates/terse-core/src/syntax/blocks.rs`'s test module, or in `syntax/tests.rs` if the splitter is not exposed.
- **Semantic walkers:** `crates/terse-core/src/semantic/tests.rs`, using `compile()` and the existing `locked_robins1986()` lock helper.
- **Generation:** `crates/terse-core/src/latex/mod.rs` tests (`body_of`, `parse_src`).
- **Real engine:** `crates/terse-cli/tests/e2e/latex_generation.rs`, `#[ignore]`d like its neighbors.

Existing table and numbering tests are regression guards. `test_figure_table_fields_are_semantic` must keep passing with the field form; `test_all_elements_render_under_both_themes` and `test_cross_file_equation_number_converges` must stay green.

Every failure assertion checks the diagnostic code and that the span starts at the offending line.

## Spec-to-Test Mapping

### Capability: language-parsing

#### Scenario: Figure and table preserve their meaning (existing)
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_figure_table_fields_are_semantic` (existing; cell assertions updated from `String` to inline `Text`)
- **Setup (GIVEN)**: existing field-form table with a quoted comma-containing cell
- **Action (WHEN)**: existing
- **Assert (THEN)**: header, row and cell order, and cell text are retained; the cell is a single `Text` inline containing the comma; every column has `Default` alignment

#### Scenario: Invalid figure or table
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_invalid_tables_fail`
- **Setup (GIVEN)**: (a) a field-form row with three cells under a two-cell header; (b) the same in pipe form; (c) `table [id: t1]:` with a header and rows but no caption; (d) a figure without `alt`
- **Action (WHEN)**: `try_parse_diagnostics`
- **Assert (THEN)**: (a) and (b) fail with `E-META-015` at the table; (c) fails with `E-META-017`; (d) fails as it does today
- **Edge cases**: a field-form row with one cell under a two-cell header now succeeds, padded with an empty cell; a caption or cell with `^[note]` fails with `E-META-018`

#### Scenario: Bare pipe table with alignment and inline cells
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_bare_pipe_table`
- **Setup (GIVEN)**: `| Metric | Value |`, `|:--|--:|`, `| *Accuracy* | $0.97$ |`, `| Latency |`, then a blank line and a paragraph
- **Action (WHEN)**: `parse_text`
- **Assert (THEN)**: one `Table { id: None, caption: None, align: [Left, Right] }`; header cells `Text("Metric")` and `Text("Value")`; row one holds `Emphasis([Text("Accuracy")])` and `Math("0.97")`; row two is `[Text("Latency")]` plus an empty cell; the paragraph follows
- **Edge cases**: `|---|:-:|` gives `[Default, Center]`; a table inside a theorem body is accepted; the same lines inside a list item are rejected as the field form is

#### Scenario: Pipes inside math and code do not split cells
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_pipe_cell_splitting_protects_math_and_code`
- **Setup (GIVEN)**: a header of five cells, a delimiter, and the row ``| $|x|$ | `a|b` | a \| b | $5 | $10 |``
- **Action (WHEN)**: `parse_text`
- **Assert (THEN)**: five cells: `Math("|x|")`, `Code("a|b")`, `Text("a | b")`, `Text("$5")`, `Text("$10")`
- **Edge cases**: ``| `a\|b` |`` yields `Code("a|b")`; `| $\|x\|$ |` yields `Math("\|x\|")` (kept as written); `| \(a|b\) |` is one math cell; a lone backtick protects nothing

#### Scenario: Captioned pipe table in the block form
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_captioned_pipe_table_block`
- **Setup (GIVEN)**: `table [id: tbl-results]:`, `  caption: "Summary of *results*"`, then two-space-indented pipe lines
- **Action (WHEN)**: `parse_text`
- **Assert (THEN)**: `Table { id: Some("tbl-results"), caption: Some([Text("Summary of "), Emphasis([Text("results")])]) }` with the pipe rows and their alignments
- **Edge cases**: the field-form caption becomes inline too (`caption: "A *b*"` yields emphasis)

#### Scenario: Pipe-looking prose and malformed pipe tables
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_pipe_prose_and_malformed_pipe_tables`
- **Setup (GIVEN)**: (a) `| not a table`, `just prose`; (b) `| a | b |`, `|---|---|---|`; (c) `table:` whose body has `rows:` and a pipe line
- **Action (WHEN)**: `parse_text` for (a), `try_parse_diagnostics` for (b) and (c)
- **Assert (THEN)**: (a) is one paragraph `| not a table just prose`; (b) fails with `E-PARSE-002` at the delimiter line; (c) fails with `E-PARSE-002` naming the mix
- **Edge cases**: a paragraph running into a pipe table (no blank line, next line is a delimiter row) ends before the table

#### Scenario: Citations and references inside cells are live
- **Test type**: unit
- **Test file**: `crates/terse-core/src/semantic/tests.rs`
- **Test name**: `test_table_cell_citations_and_refs_are_live`
- **Setup (GIVEN)**: a module whose only citation `[@robins1986]` is inside a pipe-table cell, with the `locked_robins1986()` lock; a second module whose only `{ref: nope}` is inside a cell
- **Action (WHEN)**: `compile()` each; `collect_cited_aliases` on the first
- **Assert (THEN)**: the first compiles and `collect_cited_aliases` contains `robins1986`, and its plan requests a bibliography build; the second fails with the same diagnostic code a paragraph `{ref: nope}` produces
- **Edge cases**: a citation only in a table caption behaves the same

### Capability: latex-generation

#### Scenario: Cross-file numbering converges [Acceptance E] (existing)
- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/multi_file_projects.rs`
- **Test name**: `test_cross_file_equation_number_converges` (existing, unchanged)
- **Setup (GIVEN)**: existing
- **Action (WHEN)**: existing
- **Assert (THEN)**: existing assertions still pass

#### Scenario: Captioned and captionless tables number correctly
- **Test type**: unit
- **Test file**: `crates/terse-core/src/latex/mod.rs`
- **Test name**: `test_captionless_table_is_in_place`
- **Setup (GIVEN)**: captioned `tbl-a`, a bare pipe table, captioned `tbl-b`
- **Action (WHEN)**: generate the body and style
- **Assert (THEN)**: the bare table is `\begin{TerseTableHere}` … `\end{TerseTableHere}` with no `\caption` and no `\label`; the two captioned tables are `\begin{table}` floats with `\caption` and `\label`; the style defines `TerseTableHere` with `\trivlist\TerseFigureAlign\item\relax`

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/latex_generation.rs`
- **Test name**: `test_captionless_table_does_not_consume_a_number`
- **Setup (GIVEN)**: the same three tables and a paragraph `REFA{ref: tbl-a} REFB{ref: tbl-b}`
- **Action (WHEN)**: `terse build --require-pdf`, then extract the PDF text
- **Assert (THEN)**: `REFA1` and `REFB2` appear; `Table 3` does not

#### Scenario: Aligned columns and inline cells render
- **Test type**: unit
- **Test file**: `crates/terse-core/src/latex/mod.rs`
- **Test name**: `test_pipe_table_alignment_and_inline_cells`
- **Setup (GIVEN)**: a pipe table with delimiter `|:--|:-:|--:|` and a cell `$\alpha$` and a header cell `**Name**`
- **Action (WHEN)**: generate the body
- **Assert (THEN)**: the tabular spec is `{lcr}`; the cell is emitted as `\(\alpha\)`, not `\$\textbackslash{}alpha\$`; the header cell is `\TerseTableHeaderCell{\textbf{Name}}` or the strong macro the generator uses for prose

#### Scenario: Equation numbering follows the delimiter
- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/latex_generation.rs`
- **Test name**: `test_equation_numbering_follows_delimiter`
- **Setup (GIVEN)**: an unlabeled `math:` equation `a = 1`, a `$$ b = 2 $$`, a labeled `math [id: eq-c]:` equation `c = 3`, and a paragraph `REFC{ref: eq-c}`
- **Action (WHEN)**: `terse build --require-pdf`, then extract the PDF text
- **Assert (THEN)**: `(1)` and `(2)` appear, `(3)` does not, and `REFC2` appears

## Coverage Summary

| Capability | Scenario | Test file | Test name | Type |
|------------|----------|-----------|-----------|------|
| language-parsing | Figure and table preserve their meaning | `syntax/tests.rs` | `test_figure_table_fields_are_semantic` | unit |
| language-parsing | Invalid figure or table | `syntax/tests.rs` | `test_invalid_tables_fail` | unit |
| language-parsing | Bare pipe table with alignment and inline cells | `syntax/tests.rs` | `test_bare_pipe_table` | unit |
| language-parsing | Pipes inside math and code do not split cells | `syntax/tests.rs` | `test_pipe_cell_splitting_protects_math_and_code` | unit |
| language-parsing | Captioned pipe table in the block form | `syntax/tests.rs` | `test_captioned_pipe_table_block` | unit |
| language-parsing | Pipe-looking prose and malformed pipe tables | `syntax/tests.rs` | `test_pipe_prose_and_malformed_pipe_tables` | unit |
| language-parsing | Citations and references inside cells are live | `semantic/tests.rs` | `test_table_cell_citations_and_refs_are_live` | unit |
| latex-generation | Cross-file numbering converges | `terse-cli/tests/e2e/multi_file_projects.rs` | `test_cross_file_equation_number_converges` | e2e |
| latex-generation | Captioned and captionless tables number correctly | `latex/mod.rs`, `terse-cli/tests/e2e/latex_generation.rs` | `test_captionless_table_is_in_place`, `test_captionless_table_does_not_consume_a_number` | unit, e2e |
| latex-generation | Aligned columns and inline cells render | `latex/mod.rs` | `test_pipe_table_alignment_and_inline_cells` | unit |
| latex-generation | Equation numbering follows the delimiter | `terse-cli/tests/e2e/latex_generation.rs` | `test_equation_numbering_follows_delimiter` | e2e |

Paths without a crate prefix are under `crates/terse-core/src/`.
