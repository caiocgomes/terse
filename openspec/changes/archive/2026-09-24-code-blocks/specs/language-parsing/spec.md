## ADDED Requirements

### Requirement: Fenced code blocks
The language SHALL support fenced code blocks. A logical line whose structural content is a run of three or more backticks followed by an info string containing no backtick SHALL open one. The first whitespace-delimited word of the trimmed info string, if any, SHALL be the block's language tag, kept as written; the rest of the info string SHALL be ignored. The block SHALL close at the first later line, at the block's structural indent, whose content is a run of at least as many backticks followed only by whitespace. The content SHALL be the bytes of the lines between opener and closer after removing the block's structural prefix, retaining blank lines, trailing whitespace, tabs, internal indentation, and original line endings. Content SHALL NOT be parsed as Terse: it carries no inline elements, citations, references, or math, and is never escaped or validated as TeX. A preceding paragraph running on the lines above SHALL end at the opener. A line starting with backticks whose info string contains a backtick SHALL remain prose. A block that reaches end of file, or a non-blank line indented less than the block, before its closer MUST be rejected at the opener. A content line containing the rendering environment's end sequence (`\end{TerseCode}`, with or without spaces inside) MUST be rejected at that line. Code blocks SHALL be accepted at module scope, in theorem-like and proof bodies, and in list item continuation lines, and SHALL carry no ID. The semantic model SHALL represent a code block with its optional language tag and exact content.

#### Scenario: Code is kept byte for byte
- **GIVEN** a module containing a fence opened with ```` ```python ````, whose lines include a tab-indented line, a line indented by three spaces, a line `// not a comment`, a line with `$x$ and *y* and [@ref]`, a blank line, and a closing ```` ``` ````
- **WHEN** the module is parsed
- **THEN** it yields one code block with language tag `python` and content equal to those lines' exact bytes joined by their original line endings, and no citation, math, or emphasis is produced

#### Scenario: A fence ends a running paragraph
- **GIVEN** the lines `Run this:`, ```` ```bash ````, `make all`, ```` ``` ````, `Then continue.` with no blank lines between them
- **WHEN** the module is parsed
- **THEN** it yields the paragraph `Run this:`, a code block tagged `bash` with content `make all`, and the paragraph `Then continue.`

#### Scenario: Code block inside a list item
- **GIVEN** an ordered list item `1. Install:` followed by an indented fence with content `pip install terse` and its indented closer
- **WHEN** the module is parsed
- **THEN** the item contains the paragraph `Install:` and the code block, and a `math:` block at the same place is still rejected

#### Scenario: Longer fences and untagged blocks
- **WHEN** a block is opened with four backticks and contains a line of three backticks, or is opened with three backticks and no info string
- **THEN** the three-backtick line is content of the four-backtick block, and the untagged block has no language tag

#### Scenario: Backticks in running prose stay prose
- **WHEN** a paragraph line is ```` ```x``` is inline code ````
- **THEN** it is a paragraph containing inline code, not a code block

#### Scenario: Malformed code blocks
- **WHEN** a fence reaches end of file without a closer, or a code line contains `\end{TerseCode}` or `\end {TerseCode}`
- **THEN** parsing fails at the opener or at that line respectively, naming the problem, and no LaTeX is generated

## MODIFIED Requirements

### Requirement: UTF-8 source and consistent structural whitespace
The parser SHALL accept UTF-8 `.trs` files, an optional initial BOM, LF or CRLF line endings, and EOF without a final newline. Structural indentation MUST use exactly two spaces per level; leading tabs outside opaque payloads, bare CR, invalid UTF-8, and dedents to unopened levels MUST produce source-located errors. Blank lines MUST NOT change the indentation stack. Original bytes and trivia SHALL remain available for formatting and source mapping.

Lines inside an opaque payload SHALL NOT be structural lines. The payloads are a `math:` or `tex:` block (lines indented deeper than the header), a multi-line `$$` display, and a fenced code block (lines up to the closer). Beyond the prefix that places such a line in its block, the line MAY contain tabs and any number of spaces. It SHALL NOT open, close, or validate an indentation level. The parser SHALL recognize payload openers with the same rules the block grammar uses, so a malformed header is never treated as a payload opener.

#### Scenario: Equivalent line endings
- **GIVEN** the same valid document encoded with LF or CRLF, optionally with a BOM
- **WHEN** each version is parsed
- **THEN** both produce equivalent semantic content and retain their original source spans

#### Scenario: Invalid indentation
- **WHEN** a structural child line uses a tab, three leading spaces, or an invalid dedent
- **THEN** parsing fails at that line and no successful document is emitted

#### Scenario: Opaque payload lines are not structural
- **GIVEN** a `math:` block whose second payload line is indented by five spaces, a `tex:` block with a tab after its two-space prefix, a multi-line `$$` display whose middle line starts with three spaces, and a fenced code block containing a tab-indented line
- **WHEN** the module is parsed
- **THEN** parsing succeeds, and each payload keeps those bytes exactly; the structural line after each block is still checked for indentation

### Requirement: Headings and ordered or unordered lists
The language SHALL support `#`, `##`, and `###` headings with optional IDs, unordered `- ` lists, and ordered positive-decimal `N. ` lists. An ordered list SHALL retain its first number and require subsequent markers to increase by one. Item continuations and nested lists SHALL indent one structural level relative to the marker line, independently of marker width. Items SHALL contain paragraphs, lists, and fenced code blocks; other block types MUST be diagnosed. A marker-kind change SHALL start a separate list.

#### Scenario: Nested lists and three heading levels
- **WHEN** a document contains three heading levels and an ordered list starting at `9.` with nested unordered items and continued paragraphs
- **THEN** heading levels, starting number, nesting, and item order are preserved

#### Scenario: Nonsequential ordered markers
- **WHEN** consecutive items in the same ordered list are marked `3.` and `5.`
- **THEN** validation reports the second marker and the expected next value
