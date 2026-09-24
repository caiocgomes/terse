## Context

A table today is only the block form. `parse_table` (`crates/terse-core/src/syntax/blocks.rs:1465`) reads `caption:` (a scalar), `header:` (a string list), and `rows:` (string lists). It requires all three (`E-META-012` caption, `E-META-013` header, `E-META-014` at least one row) and rejects any row whose length differs from the header (`E-META-015`). `TopBlock::Table` and `NodeKind::Table` carry `caption: String`, `header: Vec<String>`, `rows: Vec<Vec<String>>` (`semantic/mod.rs:113`, lowered at `:727` with no inline parsing).

The generator emits every table as a `table` float with `\TerseFigureAlign`, a `tabular` of `l` columns with booktabs rules, header cells through `\TerseTableHeaderCell`, and cells and caption through `escape_text` (`latex/mod.rs:141-224`). The style owns alignment (`\TerseFigureAlign`, from the theme's figure alignment), cell padding (`\arraystretch`), header styling, and rules (`latex/mod.rs:670-680`, `838-842`).

Two existing gaps sit in the requirement this change rewrites:

- "Captions SHALL support inline content except footnotes" (`openspec/specs/language-parsing/spec.md`, "Semantic figures and rectangular tables") is honored by figures (`parse_inline_at` plus a footnote check, `semantic/mod.rs:708-713`) but not by tables, whose caption is escaped plain text.
- `latex-generation`'s "display equations without IDs SHALL be unnumbered" contradicts the generator and `language-parsing`. The author chose option (b): `math:` is always numbered, `$$` never.

Four node walkers skip tables because tables carry no inlines today: `contains_citation` (`semantic/mod.rs:261`, decides whether Biber runs), `validate_cross_refs` (`:426`), `check_citations` (`:504`), and `collect_cited_in_nodes` (`:621`, feeds `references.bib`). Once cells are inline, skipping them would mean a citation in a cell never reaches the bibliography and a `{ref: ...}` in a cell is never validated.

## Goals / Non-Goals

**Goals:**

- Pipe tables in `.trs`, bare (captionless) or inside the `table [id: ...]:` header with an optional `caption:`.
- Inline cells and inline captions in both syntaxes, fully walked by reference, citation, link, and math validation.
- Delimiter-row alignment.
- Optional captions: captioned tables are numbered floats, and captionless tables are unnumbered and in place, with no id.
- The corrected equation-numbering sentence in `latex-generation`.

**Non-Goals:** spanning or multi-line cells; alignment for the block syntax; column widths or long tables; pipe tables in list items (tables are not list-item content today); re-padding columns in `fmt`.

## Decisions

### D1. Recognizing a pipe table

- **Start.** A pipe table starts at a structural line whose content begins with `|`, when the next line, at the same indent, is a delimiter row: optional leading and trailing `|`, cells made only of optional `:`, one or more `-`, and optional `:`, with surrounding spaces.
- **Header/delimiter mismatch.** If the delimiter row's cell count differs from the header's, that is a located error (`E-PARSE-002`). In `.trs` it does not fall back to prose, because a line that looks like a delimiter row and silently becomes paragraph text is exactly the ambiguity Terse rejects elsewhere.
- **Body.** It continues while lines at that indent begin with `|`, and ends at a blank line, a dedent, or any other line.
- **Paragraph lookahead.** A `|` line without a following delimiter row stays prose, so `consume_paragraph` stops only when the lookahead finds a delimiter row.
- **Placement.** A bare pipe table is accepted in `Module` and `Nested` contexts, the same places `table` is today, and rejected in list items as the block form is.

### D2. Splitting cells

A row is split on `|`, ignoring the optional leading and trailing pipe, with three protected span kinds shared with the inline parser so the two cannot disagree:

- **Code spans.** A backtick run up to the next run of the same length.
- **Dollar math.** It uses the inline rule `find_dollar_closer` (`inlines.rs`). It is exposed as a crate-level helper, so `| $5 | $10 |` splits into two cells, while `| $|x|$ |` is one cell.
- **Paren math.** `\(` up to `\)`.

Outside protected spans, `\|` becomes a literal `|` and does not split. Inside a code span, `\|` also becomes `|`, as in GFM. Inside math it is left as written, where it is TeX's double bar. A trailing unmatched protected span, such as a lone backtick, protects nothing and is split normally. Cells are trimmed.

### D3. Row shape

- **Short row.** Padded with empty cells to the header width, as in GFM.
- **Long row.** Rejected with `E-META-015`. The code keeps its meaning, a row whose length the header does not allow; only the short case stops being an error.
- **Block form.** Gets the same rule, so both syntaxes agree.

### D4. Block form: optional caption and pipe body

`table [id: ...]:` accepts, at its body indent, either the existing `header:`/`rows:` fields or pipe-table lines, never both (`E-PARSE-002` naming the mix). `caption:` becomes optional. An `id` with no `caption` is `E-META-017` ("a table id needs a caption: an unnumbered table has nothing to reference"). `E-META-012` is no longer emitted. It stays reserved and is not reused.

### D5. Model

`NodeKind::Table` and `ProjectedNode::Table` become:

```text
{ id: Option<String>, caption: Option<Vec<Inline>>,
  align: Vec<ColumnAlign>, header: Vec<Vec<Inline>>, rows: Vec<Vec<Vec<Inline>>> }
```

`TopBlock::Table` keeps raw cell text instead, with one span per line: `caption: Option<(String, SourceSpan)>`, `header: (Vec<String>, SourceSpan)`, `rows: Vec<(Vec<String>, SourceSpan)>`, plus `align` and `id`. Inline parsing happens at lowering, as it already does for a figure's caption, and every cell on a line reports its diagnostics at that line.

`ColumnAlign` is `Default | Left | Center | Right`. The block form yields `Default` for every column. Lowering parses the caption and every cell with `parse_inline_at` (which already validates links and math), rejecting footnotes with `E-META-018` ("table captions and cells cannot contain footnotes"). The four walkers named in the Context descend into the caption, header, and rows. The projection keeps alignment, because it is authored meaning. Because the existing table fields change shape, every document with a table digests differently for unchanged content, so `PROJECTION_VERSION` goes from 2 to 3.

### D6. Emission

- **Column spec.** From `align`: `Default`/`Left` give `l`, `Center` gives `c`, `Right` gives `r`.
- **Cells.** Header cells are `\TerseTableHeaderCell{<inlines>}` and body cells are rendered inlines, joined by ` & `, as today but through `render_inlines` instead of `escape_text`.
- **Captioned table.** Unchanged float: `\begin{table}`, `\TerseFigureAlign`, the tabular, `\caption{<inlines>}`, the optional `\label`, `\end{table}`. It is numbered.
- **Captionless table.** A new style environment, `\newenvironment{TerseTableHere}{\trivlist\TerseFigureAlign\item\relax}{\endtrivlist}`. That is LaTeX's own `center` definition with the theme's alignment in place of `\centering` (tested with a booktabs `tabular` under both `\centering` and `\raggedright`: no errors, table placed between the surrounding paragraphs, and only the captioned table numbered), so the table appears where it is written, with list spacing, no float, no `\caption`, and no counter. Theme table padding, rules, and header styling apply to both forms, because they live in the style.

### D7. Equation numbering sentence

Rewriting "Numbered and unnumbered references remain valid" replaces "Identified equations SHALL have numbered anchors; display equations without IDs SHALL be unnumbered" with the implemented behavior (option b): every `math:` equation is numbered, with an anchor when it has an ID, and `$$` equations are unnumbered. No code changes for this.

### D8. `fmt`

Pipe rows are ordinary structural lines. The formatter already only trims trailing whitespace and normalizes blank lines and never rewraps. Column padding is kept as written, so nothing changes.

## Risks / Trade-offs

- [**BREAKING (narrow):** a block-form cell containing `*`, a backtick, `$`, `[`, or `{ref:` now parses as inline markup] $\rightarrow$ No `.trs` in the repository has such a cell (checked by grep). Authors escape with `\` as in prose.
- [A cell with a lone `$` price and a later `$` in the same row] $\rightarrow$ The splitter uses the inline `$` rule, so it behaves exactly as prose does.
- [A malformed delimiter row is an error, where GFM would render prose] $\rightarrow$ It is deliberate. `compile-markdown` can choose GFM's lenient behavior for `.md` through its own parser.
- [Walkers that miss cells would drop citations silently] $\rightarrow$ There is a test with a citation and a cross-reference only inside a table cell, asserting the `.bib` entry and reference validation.

## Migration Plan

No existing document changes meaning except the narrow breaking case above (none in the repository). `E-META-012` is retired but not reused. Rollback means reverting the change.
