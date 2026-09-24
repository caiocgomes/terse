## MODIFIED Requirements

### Requirement: Numbered and unnumbered references remain valid
Every `math:` equation SHALL be numbered, with a valid anchor when it has an ID; `$$` display equations SHALL be unnumbered. Figures, captioned tables, and theorem-like statements SHALL receive deterministic counters and valid anchors. A table without a caption SHALL be rendered in place, not as a float, with no caption, counter, or anchor, using the same theme alignment, padding, rules, and header styling as captioned tables. Table columns SHALL follow the parsed alignment (`l` for default and left, `c` for center, `r` for right), and table captions and cells SHALL render their inline content. Cross-references SHALL use resolved kinds rather than infer meaning from label names. Proofs SHALL retain proof anchors and explicit `of` relationships without inventing theorem content. Unnumbered heading/proof references SHALL have meaningful labels.

#### Scenario: Cross-file numbering converges [Acceptance E]
- **GIVEN** a forward reference to an identified equation in another included module
- **WHEN** the supported multi-pass compilation completes
- **THEN** the equation number/link is resolved and no unresolved-reference warning remains

#### Scenario: Captioned and captionless tables number correctly
- **GIVEN** a captioned table with ID `tbl-a`, then a captionless pipe table, then a captioned table with ID `tbl-b`, and a paragraph referencing both IDs
- **WHEN** the document is generated and compiled
- **THEN** the captionless table is emitted in place without `\caption` or float, the references render as table numbers 1 and 2, and no "Table 3" appears

#### Scenario: Aligned columns and inline cells render
- **WHEN** a pipe table with alignments left, center, and right and a cell containing `$\alpha$` is generated
- **THEN** its `tabular` column specification is `lcr`, and the cell's math is emitted as math rather than escaped text

#### Scenario: Equation numbering follows the delimiter
- **GIVEN** an unlabeled `math:` equation, a `$$` equation, and a labeled `math:` equation
- **WHEN** the document is compiled
- **THEN** the two `math:` equations are numbered 1 and 2, the `$$` equation carries no number, and a reference to the labeled one renders 2
