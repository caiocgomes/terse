## Why

A table in `.trs` today is a verbose block: `table [id: ...]:` with `caption:`, `header: [...]`, and `rows:` as lists of quoted strings (`tests/fixtures/full-paper/paper.trs:35`). Four limits follow from how it is modeled and generated:

- Cells are plain text, only escaped (`crates/terse-core/src/latex/mod.rs:206-212`). A cell with `$\alpha$` or `**b**` prints those characters literally, with no error.
- The caption is mandatory, and every table becomes a numbered `table` float (`latex/mod.rs:141-161`).
- Every column is left-aligned (`"l".repeat(...)`).
- Markdown's pipe table, the form authors already write, is not accepted.

`compile-markdown` needs a table node that can carry what a Markdown table carries. Making the pipe table native to `.trs` gives both formats one table, as `dollar-math-delimiters` did for math.

## What Changes

- `.trs` accepts pipe tables, meaning a header row, a delimiter row, and body rows in the GFM shape:

  ```
  | Metric   |  Value |
  |:---------|-------:|
  | Accuracy | $0.97$ |
  ```

  A line starting with `|` becomes a table only when the next line is a valid delimiter row. Otherwise it stays prose, so existing paragraphs that begin with `|` keep their meaning.
- The delimiter row sets column alignment: `:---` left, `:---:` center, `---:` right, `---` default (left). Alignment becomes part of the semantic table and maps to `l`/`c`/`r` in `tabular`.
- Cells carry inline content, as paragraphs do, in both syntaxes: emphasis, strong, code, links, math (`$...$` and `\(...\)`), citations, and cross-references. The table caption becomes inline too. The spec already says "Captions SHALL support inline content", and figures honor it, but table captions were escaped plain text. Footnotes stay rejected in cells and captions (`E-META-018`). **BREAKING (narrow):** in the block syntax, a cell containing `*`, `` ` ``, `$`, or `[` is now parsed as inline markup instead of literal text. No fixture in the repository has such a cell.
- The caption becomes optional for every table. A captioned table stays a numbered `table` float with `\caption`. A table without a caption is rendered where it is written, unnumbered and not floating. Such a table cannot carry an id, because there is no number to reference; declaring one is `E-META-017`. `E-META-012` (missing caption) is retired, not reused.
- The existing block syntax keeps working unchanged for any document that does not hit the breaking case above.

### Non-goals

- Spanning cells, multi-line cells, and nested blocks in cells (still rejected).
- Alignment for the block syntax (it keeps left-aligned columns).
- Column width control, `longtable`, and page-breaking tables.
- Pandoc's `Table:` caption line or other non-GFM table extensions.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `language-parsing`: "Semantic figures and rectangular tables" gains the pipe syntax, optional caption (with id only when captioned), inline cells, and delimiter-row alignment.
- `latex-generation`: "Numbered and unnumbered references remain valid" gives counters only to captioned tables. This requirement also carries the equation-numbering contradiction below, which rewriting the block forces us to settle.

## Impact

- `crates/terse-core/src/syntax/blocks.rs`: pipe-table recognition (header plus delimiter lookahead) in the block sequence and in `consume_paragraph`, plus inline cells and an optional caption in `parse_table`.
- Semantic `Table` node (`crates/terse-core/src/semantic/mod.rs:113`) and its projection: cells become inline content, the caption becomes optional, and alignments are added. Inline math in cells goes through the same validation as paragraphs.
- `crates/terse-core/src/latex/mod.rs`: `tabular` column spec from alignments, inline cell rendering, and a captionless in-place table without a float. The existing `\TerseTableHeaderCell` style macro keeps styling header cells.
- `docs/language.md`: pipe tables, alignment, inline cells, and optional caption.
- `compile-markdown` depends on this change: it maps GFM tables onto the same node.

## Decisions (settled with the author, 2026-09-23)

- **Caption and id on a pipe table in `.trs`.** The existing `table [id: ...]:` header takes an optional `caption:` field and a pipe-table body. A bare pipe block is a captionless table.
- **Rows with the wrong number of cells.** A short row is padded with empty cells, as in GFM. A row with more cells than the header is a diagnostic, because dropping a cell would lose content silently.
- **`|` inside math or code.** A `|` inside `$...$`, `\(...\)`, or a code span does not split cells, so `$|x|$` works. Outside those spans, `\|` is a literal pipe. Inside a code span, `\|` also reads as `|` for GFM compatibility. Inside math it is left alone, where `\|` is TeX's double bar.
- **`fmt`.** Pipe-table lines are kept as written; columns are not re-padded.
- **Equation numbering.** Option (b): `math:` equations are always numbered, with an anchor when identified, and `$$` equations never are. Rewriting "Numbered and unnumbered references remain valid" corrects its sentence "display equations without IDs SHALL be unnumbered" to match `language-parsing` and the generator. No PDF changes.
