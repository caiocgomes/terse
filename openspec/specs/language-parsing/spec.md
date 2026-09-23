# Language Parsing Specification

## Purpose
Defines the `.trs` source language: UTF-8 structural whitespace, explicit reserved headers and scalars, document metadata, inline and block elements, citation and declaration syntax, semantic-only attributes, validated TeX math, and explicit opaque raw TeX.
## Requirements
### Requirement: UTF-8 source and consistent structural whitespace
The parser SHALL accept UTF-8 `.trs` files, an optional initial BOM, LF or CRLF line endings, and EOF without a final newline. Structural indentation MUST use exactly two spaces per level; leading tabs outside opaque payloads, bare CR, invalid UTF-8, and dedents to unopened levels MUST produce source-located errors. Blank lines MUST NOT change the indentation stack. Original bytes and trivia SHALL remain available for formatting and source mapping.

#### Scenario: Equivalent line endings
- **GIVEN** the same valid document encoded with LF or CRLF, optionally with a BOM
- **WHEN** each version is parsed
- **THEN** both produce equivalent semantic content and retain their original source spans

#### Scenario: Invalid indentation
- **WHEN** a structural child line uses a tab, three leading spaces, or an invalid dedent
- **THEN** parsing fails at that line and no successful document is emitted

### Requirement: Explicit structural recognition and scalar syntax
The parser SHALL recognize structural headers only at a logical line's structural start. Malformed reserved headers MUST fail instead of becoming prose. A backslash before a reserved textual start SHALL allow literal prose. Full-line `//` comments SHALL be retained as trivia outside opaque regions; trailing inline comments SHALL NOT be recognized. Quoted strings and paths SHALL use JSON string escapes. Bare scalars SHALL honor their enclosing delimiters. IDs, aliases, roles, and theme names SHALL match `[A-Za-z][A-Za-z0-9_-]*`, case-sensitively. Duplicate fields, duplicate attributes, unknown keys, implicit expression evaluation, and YAML extensions MUST be rejected.

#### Scenario: Reserved text is explicitly escaped
- **WHEN** a paragraph starts with `\include` followed by ordinary text
- **THEN** it becomes prose beginning with `include` and produces no include dependency

#### Scenario: Malformed header is diagnosed
- **WHEN** a source line starts with `figure` but omits its required quoted path
- **THEN** the parser reports a malformed figure header instead of treating it as a paragraph

### Requirement: Complete document metadata
The entry module SHALL contain exactly one `document:` block before content, with a required nonempty title and optional subtitle, ordered authors and affiliations, date, language, abstract, and ordered keywords. Authors SHALL begin with `- name:` and allow either `affiliation` text or an `affiliations` string list. Title/subtitle and name/affiliation fields SHALL support their documented inline subset; title fields MUST reject footnotes. Abstracts SHALL allow paragraphs/lists with inline math, citations, and footnotes, and reject headings/declarations. Missing date MUST remain absent; missing language SHALL use `en`. The initial target SHALL validate `en` and `pt-BR` and diagnose unsupported locale mappings. Included modules MUST NOT introduce document metadata.

#### Scenario: Full metadata survives parsing
- **GIVEN** metadata containing all supported fields, two authors with affiliations, an abstract, and multiple keywords
- **WHEN** it is parsed
- **THEN** all supplied values and ordered sequences are retained without substituting the host date or locale

#### Scenario: Invalid metadata placement or fields
- **WHEN** an included module contains `document:`, or an author specifies both affiliation forms, or a required title is missing
- **THEN** validation fails at the offending declaration with a correction for that condition

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
- **THEN** parsing fails with `E-PARSE-050` on that paragraph, with guidance to put display math on its own line

### Requirement: Headings and ordered or unordered lists
The language SHALL support `#`, `##`, and `###` headings with optional IDs, unordered `- ` lists, and ordered positive-decimal `N. ` lists. An ordered list SHALL retain its first number and require subsequent markers to increase by one. Item continuations and nested lists SHALL indent one structural level relative to the marker line, independently of marker width. Items SHALL contain paragraphs and lists; unsupported block types MUST be diagnosed. A marker-kind change SHALL start a separate list.

#### Scenario: Nested lists and three heading levels
- **WHEN** a document contains three heading levels and an ordered list starting at `9.` with nested unordered items and continued paragraphs
- **THEN** heading levels, starting number, nesting, and item order are preserved

#### Scenario: Nonsequential ordered markers
- **WHEN** consecutive items in the same ordered list are marked `3.` and `5.`
- **THEN** validation reports the second marker and the expected next value

### Requirement: Equations and theorem-like blocks
The language SHALL support `math [id: name]:` display blocks; theorem, proposition, lemma, definition, example, and remark blocks with optional titles/IDs; and proof blocks with optional `id` and `of` attributes. Bodies SHALL support paragraphs, lists, equations, figures, tables, raw TeX, and nested theorem/proof blocks. Headings and bibliography markers MUST remain module-level. A proof's `of` target SHALL be resolved explicitly, without inferring a relationship from adjacency.

`math:` equations SHALL be numbered. The language SHALL also support unnumbered `$$` display equations, which carry no ID. A logical line whose structural content begins with `$$` SHALL open one, and a preceding paragraph running on the lines above SHALL end at that line. The equation MAY close on the same line (`$$ x $$`) or on a later line whose content ends with `$$`. The payload SHALL be the bytes between the delimiters, retaining line breaks and internal indentation after structural dedenting, and SHALL be validated as display math. Non-whitespace after the closing `$$` on its line, and an unterminated `$$`, MUST be rejected at a source location. Because TeX ends display math at a paragraph break, a blank line between the delimiters MUST be rejected at that line, and an empty or whitespace-only payload MUST be rejected at the opening `$$`. `$$` equations SHALL be accepted exactly where `math:` equations are accepted, and diagnosed elsewhere. A line beginning with `\$` SHALL be prose starting with a literal dollar. Unnumbered equations SHALL be generated as `\[` … `\]` and SHALL NOT consume an equation number.

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

#### Scenario: Blank line or empty dollar display
- **WHEN** a `$$` block has a blank line before or after its formula lines, or its payload is empty or whitespace-only (`$$ $$`, `$$$$`, or `$$` directly followed by `$$`)
- **THEN** parsing fails at the blank line, or at the opening `$$` for an empty payload, and no LaTeX is generated

#### Scenario: Malformed dollar display
- **WHEN** a `$$` block reaches end of file without a closing `$$`, or text follows the closing `$$` on its line, or a `$$` equation appears inside a list item
- **THEN** parsing fails at a source location naming the problem, and no LaTeX is generated

### Requirement: Semantic figures and rectangular tables
Figures SHALL use a quoted local source path with optional ID and role, and require nonempty caption and plain-text alt fields. Captions SHALL support inline content except footnotes; caption/alt fields SHALL accept scalar or indented paragraph values. Tables SHALL require a caption, one header row, and at least one rectangular data row, with optional ID. Table cells SHALL be quoted or bare inline-text scalars without footnotes; commas/brackets inside cells MUST be escaped or quoted. Field declaration order MUST NOT alter field meaning. Spanning cells and nested block cells MUST be rejected.

#### Scenario: Figure and table preserve their meaning
- **WHEN** a figure with `role: wide`, caption, alt text, and ID and a two-column table with a quoted comma-containing cell are parsed
- **THEN** all figure fields, the semantic role, table header, row/cell order, and cell text are retained

#### Scenario: Invalid figure or table
- **WHEN** a figure lacks alt text or a table row has fewer cells than its header
- **THEN** validation identifies the missing field or mismatched row and rejects the document

### Requirement: Citation and declaration syntax
The parser SHALL distinguish narrative `@alias`, parenthetical `[@alias]`, groups `[@a; @b]`, and per-item locators `[@b, p. 42]`. Unescaped `[@` SHALL select citation parsing before link parsing. Narrative `@` SHALL require a token boundary so email addresses remain text. Locator parsing SHALL retain item order and escaped delimiters. Module-level `refs:` maps SHALL accept explicit DOI, arXiv, ISBN, and URL prefixes. Includes SHALL use `include "path.trs"`, never implicit globs. `bibliography:` SHALL be a module-level marker. Syntax acceptance of a reserved identifier MUST NOT imply provider support.

#### Scenario: Citation forms are not conflated
- **WHEN** a paragraph contains all four citation forms, an email address, and an escaped `\@literal`
- **THEN** the parser retains narrative versus parenthetical intent, grouping and locators, while the email and escaped alias remain text

#### Scenario: Declarations are explicit
- **WHEN** `refs:` or `include` is nested inside a proof
- **THEN** the parser reports a placement error instead of evaluating it as a preprocessor directive

### Requirement: Content accepts only semantic attributes
Headings, equations, tables, and theorem-like blocks SHALL accept only `id`; figures SHALL accept `id` and the initial `wide` role; proofs SHALL accept `id` and `of`. Other content attributes and unknown roles MUST be rejected. Font, color, margin, absolute width, float placement, and manual spacing MUST NOT be ordinary content attributes.

#### Scenario: Theme isolation in content [Acceptance I]
- **WHEN** a figure sets `width: 80mm` or a heading sets `font: 18pt`
- **THEN** validation fails at that attribute and suggests moving presentation settings to a theme

#### Scenario: Semantic role is valid
- **WHEN** a figure uses `[id: transition, role: wide]`
- **THEN** it parses as semantic data without introducing a dimension or float-placement instruction

### Requirement: Validated TeX math without payload rewriting
The compiler SHALL validate inline/display math against a documented, versioned closed subset of ordinary and AMS TeX math, including balanced braces and permitted environments such as `aligned`, `matrix`, and `cases`. It MUST inspect nested control sequences and reject unknown commands, I/O, process execution, macro definitions, catcode changes, dynamic command construction, out-of-scope environment endings, `^^` input encodings, and control characters. Validation SHALL preserve accepted math bytes after structural dedenting. Additional unsupported macros SHALL require the explicit raw escape hatch.

#### Scenario: Familiar mathematical notation survives
- **WHEN** `\frac{\partial \dot V}{\partial H} > 0.` appears in a display-math block
- **THEN** validation accepts the expression and preserves its payload bytes

#### Scenario: Math cannot execute commands
- **WHEN** math contains `\input`, `\write18`, `\csname`, or an encoded command using `^^`
- **THEN** validation fails before invoking any LaTeX process, including when the command is nested inside another command's argument

### Requirement: Explicit opaque raw TeX
`tex:` SHALL introduce a raw block ending at the next nonblank dedented line or EOF. Raw and display-math payload handling SHALL remove only the required structural prefix and retain internal indentation, whitespace-only lines, and original payload line endings. Ordinary builds SHALL preserve raw TeX and SHALL NOT reject it solely for being raw. Strict checking SHALL emit `W-TEX-001` for each raw block; `--deny-warnings` SHALL fail the check without changing its diagnostic code. Export compatibility SHALL be validated separately.

#### Scenario: Raw TeX escape hatch [Acceptance J]
- **GIVEN** a valid document with an explicit raw drawing block and any required declared local support
- **WHEN** it is built normally and then checked with `--strict`
- **THEN** generated TeX contains the dedented raw bytes and the strict check reports the original block's portability boundary

#### Scenario: Opaque payload includes comments and whitespace
- **WHEN** a raw block contains `%` comments, lines starting `//`, extra indentation, and blank lines
- **THEN** those bytes remain payload rather than becoming Terse comments or structural content

