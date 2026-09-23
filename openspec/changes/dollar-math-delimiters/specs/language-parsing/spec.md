## MODIFIED Requirements

### Requirement: Readable paragraphs and inline elements
The parser SHALL support paragraphs, `*emphasis*`, `**strong**`, links `[label](destination)`, backtick-delimited inline code, `^[footnote]`, `\(math\)`, `$math$`, and `{ref: id}`. Paragraph source newlines SHALL mean one semantic space without forcing source reflow. Inline delimiters MUST balance without crossing. Code delimiters SHALL close with a matching backtick-run length; code and inline math MUST stay on one logical line. Nested footnotes, nested links, footnotes in link labels, and unsupported unescaped triple-asterisk runs MUST be rejected. Word-internal asterisks SHALL remain text. Unknown prose backslash escapes MUST fail; escaped punctuation SHALL remain literal. Link destinations SHALL allow balanced/escaped parentheses and reject unescaped whitespace.

`$math$` SHALL produce the same inline math element as `\(math\)`, with the same validation and the same generated LaTeX. An unescaped `$` SHALL open inline math only when it is not immediately followed by another `$` or by whitespace, and the next unescaped `$` in the same paragraph can close it. That next `$` closes only if it is not preceded by whitespace and not followed by an ASCII digit; otherwise the opener is literal text. A `$` further along never closes an earlier opener. An escaped `\$` inside the math SHALL NOT close it. A `$` that does not satisfy these conditions SHALL remain literal text. A `$$` that does not begin a logical line MUST be rejected with guidance to place display math on its own line.

#### Scenario: Mixed inline content
- **WHEN** a wrapped paragraph containing emphasis, strong text, a link, literal code, a footnote, math, and a cross-reference is parsed
- **THEN** their types and order are retained, wrapped prose lines join with semantic spaces, and code/math contents are not parsed as citations or formatting

#### Scenario: Ambiguous delimiters are rejected
- **WHEN** delimiters cross, a footnote nests another footnote, or an unescaped `***` run occurs
- **THEN** parsing fails at the offending delimiter with guidance to use supported nesting or escaping

#### Scenario: Dollar inline math matches backslash-paren math
- **GIVEN** one paragraph containing `$\frac{a}{b}$` and another containing `\(\frac{a}{b}\)`
- **WHEN** both are parsed and generated
- **THEN** each yields one inline math element with payload `\frac{a}{b}` and identical generated LaTeX

#### Scenario: Currency stays text
- **WHEN** a paragraph reads `It costs $5 to $10 per unit.`, or `Pay $ 5 now.`, or contains a single `$`
- **THEN** it contains no inline math and every `$` is literal text, emitted as `\$`

#### Scenario: Prices before math do not pair with it
- **WHEN** a paragraph reads `costs $5 to $10, inline $y^2$.`
- **THEN** its only inline math is `y^2`, and `$5` and `$10` are literal text

#### Scenario: Escaped dollars
- **WHEN** a paragraph contains `\$5` or the math `$a \$ b$`
- **THEN** `\$5` is literal text, and the math payload is `a \$ b`

#### Scenario: Dollar math is validated
- **WHEN** a paragraph contains `$\input{evil}$`
- **THEN** validation fails at that math exactly as it does for `\(\input{evil}\)`

#### Scenario: Mid-line display dollars are rejected
- **WHEN** a paragraph line contains `where $$x$$ holds`
- **THEN** parsing fails at the `$$` with guidance to put display math on its own line

### Requirement: Equations and theorem-like blocks
The language SHALL support `math [id: name]:` display blocks; theorem, proposition, lemma, definition, example, and remark blocks with optional titles/IDs; and proof blocks with optional `id` and `of` attributes. Bodies SHALL support paragraphs, lists, equations, figures, tables, raw TeX, and nested theorem/proof blocks. Headings and bibliography markers MUST remain module-level. A proof's `of` target SHALL be resolved explicitly, without inferring a relationship from adjacency.

`math:` equations SHALL be numbered. The language SHALL also support unnumbered `$$` display equations, which carry no ID. A logical line whose structural content begins with `$$` SHALL open one, and a preceding paragraph running on the lines above SHALL end at that line. The equation MAY close on the same line (`$$ x $$`) or on a later line whose content ends with `$$`. The payload SHALL be the bytes between the delimiters, retaining line breaks and internal indentation after structural dedenting, and SHALL be validated as display math. Non-whitespace after the closing `$$` on its line, and an unterminated `$$`, MUST be rejected at a source location. `$$` equations SHALL be accepted exactly where `math:` equations are accepted, and diagnosed elsewhere. A line beginning with `\$` SHALL be prose starting with a literal dollar. Unnumbered equations SHALL be generated as `\[` … `\]` and SHALL NOT consume an equation number.

#### Scenario: Theorem and proof from the authoring model
- **WHEN** a titled theorem followed by a proof containing an identified display equation is parsed
- **THEN** the theorem, proof, equation, identifiers, and math payload remain distinct semantic nodes in source order

#### Scenario: Unsupported nested structure
- **WHEN** a theorem body contains a heading or an include declaration
- **THEN** validation reports the invalid placement rather than moving the element to module scope

#### Scenario: Multi-line dollar display splits a paragraph
- **GIVEN** the lines `where`, `$$`, `a = b +`, `  c`, `$$`, `holds.` with no blank lines between them
- **WHEN** the module is parsed
- **THEN** it yields a paragraph `where`, an unnumbered equation whose payload keeps both formula lines, their line break, and the two-space indentation, and a paragraph `holds.`

#### Scenario: Single-line dollar display
- **WHEN** a line reads `$$ E = mc^2 $$`
- **THEN** it yields one unnumbered equation with payload ` E = mc^2 `

#### Scenario: Dollar display does not consume a number
- **GIVEN** a module with `math [id: eq-a]:`, then a `$$` equation, then `math [id: eq-b]:`, and a paragraph referencing both ids
- **WHEN** the document is generated and compiled
- **THEN** the `$$` payload is emitted between `\[` and `\]`, and the references render as equation numbers 1 and 2

#### Scenario: Malformed dollar display
- **WHEN** a `$$` block reaches end of file without a closing `$$`, or text follows the closing `$$` on its line, or a `$$` equation appears inside a list item
- **THEN** parsing fails at a source location naming the problem, and no LaTeX is generated
