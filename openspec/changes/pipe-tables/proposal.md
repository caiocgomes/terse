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
- Cells carry inline content, as paragraphs do, in both syntaxes: emphasis, strong, code, links, math (`$...$` and `\(...\)`), citations, and cross-references. Footnotes stay rejected, as today. **BREAKING (narrow):** in the block syntax, a cell containing `*`, `` ` ``, `$`, or `[` is now parsed as inline markup instead of literal text. No fixture in the repository has such a cell.
- The caption becomes optional for every table. A captioned table stays a numbered `table` float with `\caption`. A table without a caption is rendered where it is written, unnumbered and not floating. Such a table cannot carry an id, because there is no number to reference; declaring one is a diagnostic.
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

## Open for design

- **Caption and id on a pipe table in `.trs`.** Recommendation: the existing `table [id: ...]:` header takes an optional `caption:` field and a pipe-table body. A bare pipe block is a captionless table.
- **Rows with the wrong number of cells.** GFM pads short rows and drops extra cells. Terse rejects mismatched rows today. Keeping the rejection is explicit, but real Markdown tables sometimes rely on padding.
- **`|` inside math or code in a cell.** GFM splits on every unescaped `|`, even inside code spans and `$...$`, so a cell with `$|x|$` (absolute value) breaks into three cells unless written `$\|x\|$`. The choice is between following GFM or protecting `$...$` and code spans.
- **`fmt`.** Whether it keeps pipe-table bytes as written or pads columns to align them.
- **Equation numbering contradiction.** `latex-generation/spec.md:62` says "display equations without IDs SHALL be unnumbered", but the generator numbers every `math:` block, with or without an id (`latex/mod.rs:93`). `language-parsing/spec.md:93`, added by `dollar-math-delimiters`, turned that code behavior into a requirement ("`math:` equations SHALL be numbered"). The two main specs now contradict each other. Either id-less `math:` becomes unnumbered (then it matches `$$`), or `latex-generation` is corrected to say `math:` is always numbered and `$$` never is. This is the author's decision.
