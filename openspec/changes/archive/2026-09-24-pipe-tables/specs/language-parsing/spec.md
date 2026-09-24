## MODIFIED Requirements

### Requirement: Semantic figures and rectangular tables
Figures SHALL use a quoted local source path with optional ID and role, and require nonempty caption and plain-text alt fields. Captions SHALL support inline content except footnotes; caption/alt fields SHALL accept scalar or indented paragraph values. Field declaration order MUST NOT alter field meaning. Spanning cells and nested block cells MUST be rejected.

A table SHALL have one header row and at least one data row. It SHALL be written either as a `table [id: ...]:` block or as a bare pipe table. The block's body SHALL hold either `header:`/`rows:` fields or pipe-table lines, never both, plus an optional `caption:`.

In the field form, cells SHALL be quoted or bare scalars, and commas or brackets inside cells MUST be escaped or quoted.

A pipe table SHALL begin at a line starting with `|` whose next line at the same indent is a delimiter row. A delimiter row's cells consist of an optional `:`, one or more `-`, and an optional `:`. The table SHALL continue while lines at that indent start with `|`. A `|` line not followed by a delimiter row SHALL remain prose. A delimiter row whose cell count differs from the header's MUST be rejected. A delimiter cell SHALL set its column's alignment: `:-` left, `:-:` center, `-:` right, and `-` default. Field-form columns SHALL use the default alignment.

Cells SHALL split on `|` except inside code spans, `$...$` math recognized by the inline dollar rule, and `\(...\)` math. Outside math, `\|` SHALL be a literal pipe; inside math it SHALL be kept as written.

Table captions and cells SHALL support inline content except footnotes. A data row shorter than the header SHALL be padded with empty cells, and a longer row MUST be rejected. The caption SHALL be optional. A table with an ID MUST have a caption.

#### Scenario: Figure and table preserve their meaning
- **WHEN** a figure with `role: wide`, caption, alt text, and ID and a two-column table with a quoted comma-containing cell are parsed
- **THEN** all figure fields, the semantic role, table header, row/cell order, and cell text are retained

#### Scenario: Invalid figure or table
- **WHEN** a figure lacks alt text, a table row has more cells than its header, or a table declares an ID without a caption
- **THEN** validation identifies the missing field, the overlong row, or the uncaptioned ID and rejects the document

#### Scenario: Bare pipe table with alignment and inline cells
- **GIVEN** the lines `| Metric | Value |`, `|:--|--:|`, `| *Accuracy* | $0.97$ |`, `| Latency |`
- **WHEN** they are parsed
- **THEN** one captionless table results, with alignments left and right, a header `Metric`/`Value`, a first row holding emphasis and inline math, and a second row padded to `Latency` plus an empty cell

#### Scenario: Pipes inside math and code do not split cells
- **WHEN** a row is ``| $|x|$ | `a|b` | a \| b | $5 | $10 |``
- **THEN** its cells are the math `|x|`, the code `a|b`, the text `a | b`, the text `$5`, and the text `$10`

#### Scenario: Captioned pipe table in the block form
- **GIVEN** `table [id: tbl-results]:` with `caption: "Summary of *results*"` followed by pipe-table lines at the body indent
- **WHEN** it is parsed
- **THEN** it is one table with that ID, an inline caption containing emphasis, and the pipe rows

#### Scenario: Pipe-looking prose and malformed pipe tables
- **WHEN** a paragraph line starts with `|` and is followed by ordinary text, or a header of two cells is followed by a delimiter row of three, or a `table:` block mixes `rows:` with pipe lines
- **THEN** the first stays a paragraph, and the second and third are rejected at their lines

#### Scenario: Citations and references inside cells are live
- **GIVEN** a table whose only citation `[@smith2020]` and only cross-reference `{ref: sec-intro}` appear inside cells
- **WHEN** the project is checked and built
- **THEN** the citation is included in the generated bibliography, and an unknown reference ID in a cell is reported as it would be in a paragraph
