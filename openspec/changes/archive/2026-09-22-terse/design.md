## Context

Terse is a new compiler and CLI; this repository currently contains OpenSpec configuration and the [proposal](proposal.md), with no compiler implementation or existing capability specifications. The first release must support a real multi-file paper, two presentations of the same content, explicit DOI/arXiv metadata resolution, inspectable LaTeX, PDF compilation, and a self-contained arXiv export.

Terse is the author's personal project and will be released as open source. Plan implementation and maintenance for one primary maintainer, with public contributions possible. Magalu is an example presentation used to demonstrate theme separation; it does not establish corporate ownership, sponsorship, or an internal deployment requirement. The compiler, examples, documentation, and validation workflow must be maintainable from the public repository.

The primary users are authors, reviewers editing generated LaTeX, and maintainers running checks in CI. Committed plain-text inputs and explicitly referenced assets are authoritative. Disposable caches, provider availability, machine-specific fonts, and previous successful builds must not be necessary to regenerate text artifacts.

The language and theme examples in the brief are the starting authoring model. This design refines their grammar and tooling contracts without introducing editorial views or another output backend. The default workflow is `sdd-tdd`; capability specs, the detailed test plan, and implementation tasks follow this artifact.

## Goals / Non-Goals

**Goals:**

- Keep every authored semantic element and its order invariant when changing themes.
- Produce useful, human-readable LaTeX projects that work without Terse or network access.
- Specify enough syntax, data structures, and failure behavior to implement all MVP elements without backend-driven parsing decisions.
- Make formatting, citation locking, generated text, and packaging deterministic and reviewable in Git.
- Preserve original source identity through includes, validation, emission, and engine diagnostics.
- Deliver a single compiler executable for Linux, macOS, and Windows, with optional external LaTeX tooling.
- Keep architecture, contribution instructions, and release automation practical for one maintainer, while preserving all MVP acceptance scenarios.

**Non-Goals:**

No new math notation, editor/UI, prose rewriting, conditional content, CSS engine, arbitrary package/class compatibility, TeX import, multiplayer features, reference-manager replacement, title search, Zotero integration, uploads, secondary backends, or document plugins. Raw TeX is a deliberate trust boundary, not an extensible semantic language. Byte-identical PDFs across different TeX installations and comprehensive PDF accessibility certification are not MVP guarantees.

## Decisions

### 1. Rust executable with a reusable compiler core

Use a Rust Cargo workspace with a small CLI crate and a compiler library split into focused modules: `source`, `syntax`, `semantic`, `project`, `references`, `theme`, `latex`, `diagnostic`, `artifact`, and `watch`. Keep network providers and process execution behind interfaces so offline compilation and tests cannot accidentally use them. Do not create one crate per module until independent compilation or reuse warrants it.

Use maintained libraries for argument parsing, TOML/JSON, HTTPS with a bundled TLS implementation, XML parsing, filesystem events, hashing, and ZIP writing. Select concrete versions during implementation, commit `Cargo.lock` and a Rust toolchain pin, and build releases with `--locked`. Parser behavior, canonical serialization, and theme policy remain owned by Terse, rather than inherited accidentally from dependency defaults.

Keep one public repository and one compiler release. The author initially maintains the compiler, reference adapters, and arXiv profile together; additional contributors can work through ordinary issues and pull requests. Introduce interfaces for concrete test seams and the existing compiler stages, without requiring a service deployment, independent subsystem teams, or a public plugin architecture. New abstractions should serve an implemented milestone or a required acceptance test.

The public repository must contain the instructions and inputs needed to build, test, and reproduce the demonstration. Add a concise `README` and `CONTRIBUTING` guide covering setup, test commands, fixture updates, and issue/patch submission. CI and local development use the same checked-in scripts; hosted automation is a convenience, not the only way to validate a change. Contributors can run the compiler tests and `--tex-only` workflow before installing the documented optional PDF toolchain.

The author has not selected a license yet. Record that as a release decision rather than assuming MIT, Apache, GPL, or another license. Once selected, add the license text and package metadata, and document provenance/notices for bundled third-party assets and templates. The licensing arrangement for generated style/template material must support distributing and manually editing the resulting LaTeX projects, including journal/arXiv packages; the compiler must not insert a license choice for the author's document. This design records distribution requirements without applying a license to the repository now.

Alternatives: Python or TypeScript would shorten initial prototyping but add runtime/distribution requirements; a general Markdown converter would make structural ambiguity and theme/content guarantees harder to control. Rust matches the requested distribution model. An embedded TeX distribution is excluded: LaTeX remains an ordinary external toolchain.

### 2. Explicit compiler stages and immutable semantic content

```text
project discovery and configuration
  -> source registry and lossless syntax trees
  -> semantic modules
  -> include expansion and global symbol resolution
  -> citation binding against the lockfile
  -> semantic and target validation
  -> theme validation and resolution
  -> LaTeX artifact plan and source maps
  -> optional isolated working-directory compilation
  -> transactional publication or export packaging
```

The compiler core accepts an explicit input snapshot and returns diagnostics plus an artifact plan. It does not write output, fetch metadata, run processes, or read the clock. The application layer supplies filesystem reads and publishes results. Reference resolution is a separate operation that shares declaration parsing but can run before unresolved citations become valid.

`ParsedModule` contains local metadata/declarations, ordered blocks, include nodes, and a source identity. `ResolvedDocument` contains metadata, an expanded block tree, global symbols, resolved reference entries, and dependency records. Blocks include heading, paragraph, list, equation, figure, table, theorem-like block, proof, bibliography marker, and raw TeX. Inlines include text, emphasis, strong, link, code, footnote, math, citation group, and cross-reference. Figure alt text and semantic role are retained even when a target cannot expose them accessibly. Metadata and bibliography records are typed data, never prebuilt TeX strings.

Every semantic node and field that can produce an error carries a `SourceSpan { file_id, byte_start, byte_end }`. Node identity is assigned deterministically from module occurrence and syntax position, not randomness. Include occurrences retain their include stack; source identity always identifies the original file. Diagnostic rendering calculates line/column from the original bytes.

Themes receive a read-only document view plus semantic kinds/roles and produce a separate `ResolvedTheme`. They cannot return a transformed AST. The backend traverses the document in source order once; styles do not decide which nodes are emitted. A versioned semantic-content projection includes metadata, ordered blocks/inlines, math/raw payloads, IDs, targets, captions, alt text, citation groups/locators, and effective bibliography data. It excludes spans, theme choices, derived furniture, and incidental node IDs. Its canonical digest must match across themes; structural equality and emitted-content tests are the primary evidence, not the digest alone.

Alternative: a mutable AST passed to template callbacks is simpler but permits the exact content rewriting this product excludes. Preserve a lossless syntax tree separately for formatting instead of trying to reconstruct authoring trivia from the semantic AST.

### 3. Project root, configuration, and command behavior

Find `terse.toml` by walking upward from an explicit entry's directory, or from the working directory if no entry is supplied. A manifest is required; a missing one produces a diagnostic recommending `terse init`. Its directory is the project root. The explicit entry is relative to the invocation directory; manifest paths are relative to the root. Source include/asset paths are relative to their declaring `.trs` file; theme assets are relative to the declaring `.theme` file. These rules are the same in every command.

```toml
format-version = 1

[project]
entry = "paper.trs"
output = "build"

[themes]
academic = "themes/academic.theme"
magalu = "themes/magalu.theme"

[latex]
engine = "xelatex"
pdf = "auto"

[references]
lockfile = "references.lock"
overrides = "references.overrides.toml"

[export.arxiv]
profile = "texlive-2025-xelatex"
```

Unknown keys and unsupported format versions are errors. The optional overrides file may be absent. `academic` is the default theme when declared; otherwise theme-dependent commands require `--theme`. Effective settings follow explicit CLI flags, then manifest, then documented compiler defaults. No environment variable changes document semantics, formatting, theme selection, or text artifacts in v1. `PATH` only locates optional tools; terminal/color variables only affect human diagnostics. Tool environment is explicitly prepared before execution.

Normalize paths lexically, then verify canonical existing ancestors and symlink targets stay within the root. Reject drive/UNC escapes, traversal, dangling symlinks, special files, and portable-path collisions. Project-relative logical paths use `/` in artifacts. Generated filenames use an ASCII-safe basename and a stable digest of the logical path when needed; never embed host absolute paths. Detect case-insensitive output collisions even on case-sensitive hosts. Do not normalize authored prose's Unicode code points.

Output must be a proper descendant of the root and must not overlap authoritative files/directories; neither the root itself nor a source directory is a valid output directory. The default build destination is `build/<theme>/`, with a safe stem derived from the entry (`paper.trs` becomes `paper.tex`). Internal temporary names never enter published artifacts.

| Command | Contract |
| --- | --- |
| `init [directory] [--force]` | Preflight conflicts, then create manifest, entry, academic theme, empty versioned lockfile, and additive ignore entries. Without force, existing scaffold files cause failure before writes. Never discard unrelated `.gitignore` entries, even with force. |
| `check [entry] [--theme name] [--strict] [--target arxiv]` | Validate complete reachable content, all declared themes by default or the selected theme, reference bindings, assets, and static target compatibility. Requires no TeX engine. |
| `fmt [paths] [--check]` | Format `.trs` and `.theme` files. Without paths, format the reachable project modules and declared themes, in sorted path order. Never follow a glob-defined include order. |
| `refs resolve [entry] [--refresh] [--offline]` | Resolve missing/changed declarations and overrides; refresh deliberately refetches unchanged identifiers. Reject incompatible `--refresh --offline`. Commit the lock transaction only if all requested resolutions succeed. |
| `build [entry] [--theme name] [--tex-only | --require-pdf]` | Generate/validate all sources; default `auto` compiles if the configured engine exists. Missing engine yields sources and a clear warning; an available engine that fails causes build failure. `--require-pdf` fails on missing tooling; `--tex-only` invokes none. |
| `watch [entry] [--theme name]` | Initial build, then rebuild dependency changes. Build flags also apply. A failed attempt keeps the last successful artifacts. |
| `export [entry] [--theme name] --target arxiv` | Rebuild from authoritative inputs, validate, then publish a source directory, ZIP, file manifest, and separate validation report. Never package an unchecked previous build or upload anything. |

All commands accept `--diagnostics human|json`; validation commands accept `--deny-warnings`. Exit codes: `0` success, `1` input/validation failure or formatting differences, `2` usage/configuration error, `3` I/O/provider/tool execution failure. Watch reports per-attempt results and remains alive after recoverable failures; explicit interruption terminates it. Malformed project configuration can prevent startup with code `2`.

### 4. Language v1: explicit blocks with a lossless parser

Use a handwritten, line-oriented block parser with a small delimiter-aware inline parser. First tokenize indentation and retain trivia, then parse structure and lower to semantic nodes. This makes exact source spans, readable recovery, and low-noise formatting easier than treating the whole document as YAML or CommonMark. The following productions and lexical rules define the initial grammar; capability specs must carry these choices into normative requirements.

#### Whitespace, names, and recognition

- Input is UTF-8, with an optional initial BOM accepted and removed by formatting. Accept LF and CRLF; reject bare CR. Canonical formatting uses LF and a final newline outside opaque payloads.
- Structural indentation is exactly two spaces per level. Leading tabs are errors outside opaque math/raw payloads. Blank lines do not alter the indentation stack. A dedent must reach a previously opened level.
- Blocks are recognized only at the beginning of a logical line after structural indentation. Reserved block keywords at that position must form valid headers; malformed headers are errors, not silent paragraphs. Prefix a reserved textual start with a backslash to make it prose.
- IDs, aliases, roles, and theme names match `[A-Za-z][A-Za-z0-9_-]*` and are case-sensitive. Bibliography identifier values have their own DOI/arXiv validation and are not restricted to this pattern.
- Quoted paths use double quotes and JSON string escaping. Bare text scalars extend to the line end; quote scalar values when delimiter characters would be ambiguous. Duplicate/unknown keys and attributes are errors. Declaration maps allow no aliases, implicit typing, expression evaluation, or YAML extensions.
- A full line beginning `//` outside an opaque payload is a comment. There are no trailing inline comments. Comments remain in the syntax tree but are not authored document content.
- A paragraph consists of consecutive nonblank, nonstructural lines at the same indentation. A source newline inside it means one semantic space; formatting retains the author's line breaks. There is no implicit manual line-break or vertical-spacing syntax.

#### Structural productions

```ebnf
module       = { blank | comment | metadata | refs | include | block } ;
include      = 'include' SP quoted_path NL ;
metadata     = 'document:' NL INDENT metadata_fields DEDENT ;
refs         = 'refs:' NL INDENT { alias ':' SP reference_identifier NL } DEDENT ;
block        = heading | paragraph | list | math | figure | table
             | theorem_like | proof | bibliography | raw_tex ;
heading      = ('#' | '##' | '###') SP inline_text [SP attributes] NL ;
math         = 'math' [SP attributes] ':' NL opaque_payload ;
figure       = 'figure' SP quoted_path [SP attributes] ':' NL
               INDENT figure_fields DEDENT ;
table        = 'table' [SP attributes] ':' NL INDENT table_fields DEDENT ;
theorem_like = theorem_kind [SP inline_title] [SP attributes] ':' NL
               INDENT { block | blank | comment } DEDENT ;
theorem_kind = 'theorem' | 'proposition' | 'lemma' | 'definition'
             | 'example' | 'remark' ;
proof        = 'proof' [SP attributes] ':' NL
               INDENT { block | blank | comment } DEDENT ;
bibliography = 'bibliography:' NL ;
raw_tex      = 'tex:' NL opaque_payload ;
attributes   = '[' attribute {',' SP attribute} ']' ;
attribute    = name ':' SP scalar ;
```

`SP` is one or more spaces before canonical formatting, `NL` is a recognized line ending, and indentation tokens come from the lexical pass. Optional separators tolerate extra horizontal spaces; the formatter emits one. The inline parser and delimiter scanner distinguish a terminal attribute list from escaped brackets, inline code, math, or link syntax. The token families `metadata_fields`, `figure_fields`, `table_fields`, `list`, and `opaque_payload` are defined below; they are not general-purpose mappings.

The remaining grammar is defined over the lexer tokens below. `TEXT` is a maximal nonempty run that does not begin an unescaped inline delimiter or a recognized block header. `BARE` is a nonempty scalar scanned to the enclosing line/list/attribute delimiter while honoring escapes. `QUOTED` is a double-quoted string with JSON escapes and no literal newline. `OPAQUE_LINE` retains the original physical line bytes after the required structural prefix; a blank payload line is also an `OPAQUE_LINE`. Context validation below restricts inline/block kinds and required/unique fields; it does not change parsing precedence.

```ebnf
name              = ASCII_LETTER { ASCII_LETTER | DIGIT | '_' | '-' } ;
id                = name ;
alias             = name ;
quoted_path       = QUOTED ;
scalar            = QUOTED | BARE ;
string_list       = '[' [scalar {',' SP scalar}] ']' ;
reference_identifier = ('doi:' | 'arxiv:' | 'isbn:' | 'url:') IDENTIFIER_TEXT ;
paragraph         = inline_text NL { inline_text NL } ;
inline_text       = inline { inline } ;
inline            = TEXT | ESCAPED | emphasis | strong | code | link
                  | footnote | inline_math | narrative | parenthetical | reference ;
emphasis          = '*' inline_text '*' ;
strong            = '**' inline_text '**' ;
code              = BACKTICK_RUN CODE_TEXT MATCHING_BACKTICK_RUN ;
link              = '[' inline_text '](' DESTINATION ')' ;
footnote          = '^[' inline_text ']' ;
inline_math       = '\\(' MATH_TEXT '\\)' ;
narrative         = '@' alias ;
parenthetical     = '[' cite_item {';' SP cite_item} ']' ;
cite_item         = '@' alias [',' SP LOCATOR_TEXT] ;
reference         = '{ref:' SP id '}' ;
opaque_payload    = INDENT OPAQUE_LINE {OPAQUE_LINE} DEDENT ;
field_text        = scalar NL | NL INDENT paragraph DEDENT ;
figure_fields     = { ('caption:' SP_OR_BLOCK field_text)
                  | ('alt:' SP_OR_BLOCK field_text) } ;
table_fields      = { ('caption:' SP scalar NL)
                  | ('header:' SP string_list NL)
                  | ('rows:' NL INDENT table_row {table_row} DEDENT) } ;
table_row         = '-' SP string_list NL ;
metadata_fields   = { text_metadata | authors | affiliations | keywords | abstract } ;
text_metadata     = ('title:' | 'subtitle:' | 'date:' | 'language:') SP scalar NL ;
authors           = 'authors:' NL INDENT author {author} DEDENT ;
author            = '-' SP 'name:' SP scalar NL
                    [INDENT author_affiliation DEDENT] ;
author_affiliation = 'affiliation:' SP scalar NL
                   | 'affiliations:' SP string_list NL ;
affiliations      = 'affiliations:' SP string_list NL ;
keywords          = 'keywords:' SP string_list NL ;
abstract          = 'abstract:' NL INDENT {paragraph | list | blank | comment} DEDENT ;
list              = unordered_list | ordered_list ;
unordered_list    = unordered_item {unordered_item} ;
ordered_list      = ordered_item {ordered_item} ;
unordered_item    = '-' SP inline_text NL [item_continuation] ;
ordered_item      = POSITIVE_INTEGER '.' SP inline_text NL [item_continuation] ;
item_continuation = INDENT {paragraph | list | blank | comment} DEDENT ;
```

`SP_OR_BLOCK` means spaces before an inline scalar, or no spaces before a newline-led block value; it consumes no newline itself. End-of-file supplies a virtual final `NL`/dedents for parsing without adding bytes to opaque regions. Blank/comment tokens may occur between structured field records and list items without becoming content. Ordered/unordered marker changes terminate one list and start another. Continuation lines belonging to an item's first paragraph follow the item-continuation indentation rule; a blank line distinguishes an additional paragraph.

Inline precedence is code/math, escaped punctuation, footnote/reference/citation, link, strong, emphasis, then text. Closers are recognized against the current delimiter stack; delimiters may not cross. Unescaped `***` runs are rejected in v1 with guidance to use explicit nested spans; asterisks immediately between word characters remain text. A delimiter opener must be followed by nonspace content and a closer preceded by nonspace content. `MATH_TEXT`, `CODE_TEXT`, `LOCATOR_TEXT`, and `DESTINATION` are delimiter-aware lexical modes, not arbitrary recursive expressions. Destinations permit balanced parentheses or escaped parentheses and reject unescaped whitespace. These rules and the field/context restrictions are part of the grammar contract, with examples and invalid cases to become parser fixtures.

Exactly one `document:` block belongs to the entry module, before content. Required metadata is `title`; optional fields are `subtitle`, `authors`, `affiliations`, `date`, `language`, `abstract`, and `keywords`. Absent date remains absent, never today's date. Absent language uses the explicit compiler default `en`. Language tags parse as text and must have a supported backend locale mapping (initial fixture coverage: `en`, `pt-BR`); unsupported locales fail precisely rather than falling back silently.

Titles/subtitles and author names/affiliations are text with the supported inline subset; title fields cannot contain footnotes. Authors are an ordered sequence of records beginning `- name:`, with optional `affiliation` text or `affiliations` string list (mutually exclusive). Document `affiliations` is an ordered list of additional affiliation text. Keywords are an ordered string list. `abstract:` introduces indented paragraphs/lists, with inline math/citations/footnotes allowed, but no headings or declarations. Quoted ISO dates remain literal authored text; date is not interpreted using the host locale.

`refs:` may occur at module level in any module; aliases enter one project namespace. Two declarations of the same alias are errors, even if their identifiers match. Declarations are not rendered where they occur. `include` is module-level only. A nested include/metadata/reference declaration gets a placement diagnostic instead of textual substitution. Exactly one explicit `bibliography:` marker may occur at module level; absent a marker, append a derived bibliography after the body if citations exist. It lists all cited works once, in first-citation order. Themes cannot filter works or reorder citations/bibliography entries.

Allowed content attributes are enumerated, never open-ended: heading/equation/table/theorem-like blocks accept `id`; figures accept `id` and `role`; proofs accept `id` and `of` (an existing theorem-like ID). For now `wide` is the only figure role, and unknown roles fail. Every node kind accepts only its listed attributes. `font`, `color`, `margin`, `width`, `placement`, and similar visual attributes fail with a theme-file suggestion.

Figures require nonempty `caption` and `alt`, in either declared order; each is a single-line scalar or an indented multiline paragraph field. Caption supports inlines except footnotes; alt is plain text. Accept PDF, PNG, and JPEG assets initially, without automatic conversion or fetching. The figure node emits caption/alt in its fixed semantic field order regardless of field order in source. Source is the quoted path in the header; it cannot be a network URL.

Tables use an explicit schema with rectangular cells, one header row, and one or more data rows; no spans, nested blocks, or visual alignment syntax:

```text
table [id: costs]:
  caption: Inventory costs by policy
  header: [Policy, Cost]
  rows:
    - [Baseline, 12]
    - [Forecast, 9]
```

Bracket-list cells are quoted or bare scalars; commas/brackets inside bare cells must be escaped or quoted. Parse cells as inline content (without footnotes), not numeric expressions. Header, rows, and caption are required; field order does not change their semantic meaning. The theme chooses widths and typography.

Unordered items begin `- `; ordered items begin a positive decimal number followed by `. `. The first ordered number is the authored start value, subsequent markers must increase by one. Continuation paragraphs and nested lists indent exactly one level relative to the item marker's line, independently of marker width. Each item begins with a paragraph and may contain further paragraphs/lists; other block kinds inside list items are initially rejected explicitly. Theorems and proofs can contain paragraphs, lists, equations, figures, tables, raw TeX, and nested theorem/proof blocks; headings and bibliography markers remain module-level.

#### Inline syntax and TeX boundaries

| Syntax | Meaning |
| --- | --- |
| `*text*`, `**text**` | Emphasis and strong; properly nested, nonempty delimiters. |
| `[label](https://example.org)` | Link; label contains inlines except links/footnotes, destination is validated and escaped separately. |
| Backtick-delimited text | Inline code; one line, literal contents. A run of backticks closes with a run of the same length. |
| `^[note text]` | Inline footnote; inlines allowed, nested footnotes and block content rejected. |
| `\(TeX math\)` | Inline math, on one logical line; no `$` shorthand in v1. |
| `@alias` | Narrative citation. |
| `[@alias]`, `[@a; @b, p. 42]` | Parenthetical citation group in authored order; each item has an optional locator. |
| `{ref: id}` | Typed cross-reference, resolved after includes. |

Outside code/math, a backslash escapes punctuation including `@`, brackets, braces, asterisks, and a leading reserved keyword start; it does not introduce a TeX command. An unknown backslash escape is an error, preventing accidental raw TeX. Narrative `@` requires a token boundary, so an email address is ordinary text. Aliases end before punctuation not permitted by their grammar. Parenthetical citations take precedence over ordinary link syntax when an unescaped `[@` opens the span. Delimiters must balance; ambiguous input receives a diagnostic rather than lossy recovery. Plain text that needs delimiter characters can escape them.

Locator text follows `, ` up to the group's `;` or `]`; escaped delimiters are allowed. Normalize recognized `p.`, `pp.`, `ch.`, `sec.`, `vol.` labels into a locator kind plus the authored value, retaining the original text. Other locator text remains a literal locator. Do not infer missing works or reorder group items.

Math/raw block payload begins one structural level deeper than its header. Strip only that common structural prefix; preserve payload bytes, including further indentation and line endings. Whitespace-only lines within the payload remain payload. Payload terminates at the next nonblank dedented line or EOF. The formatter treats payload regions as opaque: it can adjust only their enclosing structural indentation, and does not reflow, trim, or normalize their internal bytes. Inline math/code contents likewise remain opaque to formatting.

TeX math requires a vetted lexical subset before emission: ordinary math symbols, braces, scripts, standard/AMS math commands, and explicitly supported math environments such as `aligned`, `matrix`, and `cases`. Validate balanced braces/environments and every control sequence, including commands nested inside text arguments. Reject I/O, shell/process operations, macro definitions, catcode changes, dynamic command construction, environment termination outside the permitted math environments, and unknown commands. Reject TeX `^^` input encodings and control characters before command scanning so encoded commands cannot bypass validation. Comments follow TeX lexical rules and are retained. Package-defined user macros require an explicit raw `tex:` block in v1. The vetted math payload is emitted unchanged; the validator must never claim arbitrary TeX is safe from a denylist scan alone.

Raw `tex:` bypasses this math subset and is emitted verbatim after structural dedenting. `check --strict` emits `W-TEX-001` at every raw block; `--deny-warnings` promotes failure policy without changing the diagnostic code. Ordinary checking documents the trust boundary but does not reject a raw block solely for being raw. arXiv export has a stricter policy below.

Alternative: allowing arbitrary math macros verbatim would expose execution through ordinary source and contradict the declarative-source promise. Treating every backslash as raw TeX would also make escaping impossible to reason about.

### 5. Module resolution, symbols, and dependency tracking

Parse each canonical file once, but expand each explicit include occurrence at its source position; repeated inclusion repeats content, not silently deduplicated content. Detect cycles using the active expansion stack, not the global parsed-file cache. Report the full root-to-cycle context and complete cycle edges with include locations. Reject missing files and root escapes before parsing their contents.

Build the global ID table after expansion. Duplicate IDs report both definitions and, for repeated inclusion, both include routes. References can point forward or across files. Heading, equation, figure, table, and theorem-like references use their kind for labels; proofs receive an anchor and, when `of` is supplied, a relationship to the referenced theorem. Proofs without `of` are independent; no adjacency-based inference.

A dependency graph records all reachable modules, the manifest, lockfile, optional override file, themes selected for the operation, their assets, and document assets. It records attempted missing paths as well as successful reads so creating a missing file can recover watch mode. Include declarations never depend on directory iteration order. Resource limits on recursion, nodes, bytes, and diagnostics produce errors rather than partial successful output; their defaults are versioned implementation constants.

Alternative: textual preprocessing loses source identity and makes declaration scope accidental; include-once semantics silently removes authored content. Explicit module expansion avoids both.

### 6. Bounded themes compiled to trusted semantic styles

Theme files use the same indentation/scalar lexical rules, but a distinct schema. Top-level keys are component selectors, with one optional exact semantic role selector (`figure[role=wide]`). No selector combinators, per-ID selection, conditions, variables with expressions, imports, templates, raw TeX, or executable hooks exist in v1. Specific role rules override the base component; base rules override compiler defaults. Duplicate selectors/properties are errors, so file order is not hidden cascade state.

| Component | Initial typed properties |
| --- | --- |
| `page` | `size` (`a4`/`letter`), uniform or four named margins, single/two `columns`. |
| `body` | Font token, size, line-height, paragraph spacing and indentation, text color. |
| `heading.1`–`heading.3` | Size, weight, spacing-before/after, numbering (`decimal`/`roman`/`none`), color. |
| `title` | `layout` (`paper`/`cover`), size, subtitle-size, spacing; every provided metadata field is rendered. |
| `theorem` and each theorem-like kind, `proof` | Title weight, body font style, spacing, border-left thickness/color. |
| `equation` | Spacing, number position (`left`/`right`). |
| `figure`, `figure[role=wide]` | Align, default-width/width as percentage, placement (`here`/`top`/`bottom`), caption style. |
| `table` | Cell padding, rule thickness, header weight, caption style; no cell/row filtering. |
| `citation` | Style token (`author-year`/`numeric`), delimiter token, link color. |
| `bibliography` | Font size, line spacing, entry spacing, heading style; no filtering/sorting controls. |
| `header`, `footer` | Left/center/right slots from `none`, `title`, `section`, `page-number`, `logo`. |
| `logo` | Local source path, bounded width, title/header placement. |
| `watermark` | `kind` (`none`/`internal-use`/`draft`), bounded opacity and angle, drawn behind content. |
| `contents` | Whether to insert a derived table of contents after title material. |

These are closed typed schemas, not pass-through LaTeX options. Require finite positive sizes, usable page content boxes, nonnegative spacing, widths within the container, and sensible versioned bounds. Do not permit zero-size text, transparent text, clipping, off-page transforms, a foreground watermark, or text color matching its background. Logo and furniture layouts cannot cover the body. Reject configurations that can suppress content or create impossible layouts instead of accepting any syntactically valid dimension. PDF regression checks cover remaining overflow/visibility failures.

Font tokens initially select TeX-distributed Libertinus, Latin Modern, or TeX Gyre families by font filenames, never host-installed family lookup. The initial theme schema does not load arbitrary custom fonts. Logos are ordinary explicitly referenced assets and copied into the output. Watermark text is a compiler-owned localized label selected by an enum, not arbitrary document prose in a theme. Metadata is rendered once in fixed semantic order; an optional running title or table of contents is clearly derived furniture.

The backend owns trusted macro/environment templates, counter creation, anchors, and node traversal. Theme compilation fills validated presentation parameters in those templates; it never interpolates user text into executable definitions. Both theme outputs use the same `paper.tex` and `.bib` bytes for the same target configuration; `terse-style.sty` and theme assets supply visual differences. Numbering display may vary, but every referenced element retains a valid anchor. Unnumbered headings are referenced by their title; unnumbered proofs by proof label and their `of` relationship where present. Authored figure/table/equation ordering is the ordered sequence of emitted nodes, not their page coordinates after legal float placement.

Alternative: general CSS or templates would introduce a much larger parser and allow omission or reordering. A static style ABI limits that risk and keeps generated projects editable.

### 7. Citation resolution, normalized records, and overrides

Use identifier adapters that produce a Terse-owned `ReferenceRecord` with type, title, ordered author/editor names, issued date parts, container title, volume, issue, pages, publisher, edition, DOI, arXiv identity/version, ISBN, and URL when applicable. Model structured personal names (`given`, `family`, optional particles/suffix), unparsed personal names, and organization names distinctly. Preserve a provider's unsplit name as an unparsed personal name instead of guessing surname boundaries; an override can supply structured parts. Never flatten an already structured name and attempt to split it later. Missing essential metadata is an error with an override suggestion, not a guessed title/author. Version 1 requires title, a supported work type, and at least one author/editor/organization or an explicit anonymous marker; publication date may be absent and render as undated.

DOI resolution uses HTTPS content negotiation via `doi.org` requesting CSL JSON, avoiding the assumption that every DOI is registered with Crossref. This capability is documented by [Crossref's content-negotiation guide](https://www.crossref.org/documentation/retrieve-metadata/content-negotiation/). Normalize identifier prefixes, surrounding whitespace, and DOI case; confirm the returned DOI matches. Strip provider markup through a limited parser into supported text/inlines, reject malformed or executable markup, and never treat metadata as trusted TeX.

arXiv resolution uses the official Atom API with `id_list`, supporting modern and legacy identifiers plus explicit `vN` versions. A versionless declaration resolves to an exact version recorded in the lock; it stays fixed until explicit refresh. Use one sequential worker, at least three seconds between requests, response-size/time limits, and bounded retries respecting server backoff. These choices follow the [arXiv API manual's identifier and rate guidance](https://info.arxiv.org/help/api/user-manual.html). XML parsing disables external entities and DTD retrieval. Never scrape a PDF or article page as a metadata fallback.

Provider HTTP uses HTTPS, constrained redirects, timeouts, capped response sizes, and a Terse user agent. Do not follow redirects to local/private addresses or arbitrary URL declarations. DOI service redirects are validated at each hop. Fetch only metadata, not cited full text or linked assets. Resolution stages all changes and atomically replaces the lock only after success; a partial outage leaves it byte-for-byte unchanged.

ISBN and generic URL identifiers parse and receive a precise unsupported-provider diagnostic during resolution in the MVP. Their syntax remains reserved. They are not silently replaced with incomplete manually generated BibTeX; the real paper fixture uses supported DOI/arXiv entries. Ordinary hyperlinks remain supported independently.

`references.lock` uses TOML with `lock-version = 1`, an explicit normalization version, and an `entries.<alias>` table for each alias, lexicographically sorted. Each entry contains the canonical declared identifier, exact resolved identifier/version, provider name, provider-adapter version, normalized provider record, explicit override patch, and resulting effective record used by builds. Record field order is fixed; author order is preserved. No fetch timestamps, machine paths, cache identifiers, or HTTP response ordering enter the lock. Preserve existing entries until `refs resolve --prune` explicitly removes aliases no longer declared.

Authors edit an optional `references.overrides.toml`:

```toml
overrides-version = 1

[entries.dml]
title = "Corrected article title"
authors = [{ given = "Jane", family = "Doe" }]
```

Overrides use the normalized record schema, replace arrays as a whole, and reject identity/provider changes. An explicit `remove = ["field"]` list removes optional fields; it cannot remove required fields or overlap fields being assigned. `refs resolve` seals the patch and effective data into the lock. `refs resolve --offline` can reseal changed overrides from existing normalized provider records but cannot resolve an unseen/changed identifier. Builds compare current declarations and overrides with the lock and report stale bindings with a resolution command; they never apply unrecorded metadata changes implicitly. Formatting of the overrides file does not invalidate an equivalent semantic patch.

Every used alias must exist in source declarations and have a matching resolved lock entry. A leftover lock entry cannot authorize an undeclared citation. Declarations with malformed or unsupported identifiers fail checking even when uncited; an unused, supported but not-yet-resolved declaration may warn without blocking until cited. Emit `.bib` for cited works only, in sorted alias order and fixed field order, with deterministic context-aware escaping. Citation order and bibliography order are maintained separately from serialization order.

Alternative: fetching during `build`, locking provider-generated BibTeX, or editing generated `.bib` would make content depend on network availability and prevent stable normalization/overrides.

### 8. LaTeX backend, Unicode, and source mapping

Select XeLaTeX as the initial engine, with `article`, `fontspec`, a bounded math/theorem/graphics/table package set, `hyperref`, and BibLaTeX/Biber. A single engine keeps validation and package coverage tractable. This refines the brief's provisional LuaLaTeX manifest: arXiv currently lists XeLaTeX among supported processors, but does not list LuaLaTeX, and advises filename-based font loading. See [arXiv's toolchain documentation](https://info.arxiv.org/help/faq/texlive.html). Compiler releases ship an offline compatibility profile; builds never query this website.

BibLaTeX/Biber supplies Unicode-aware bibliography processing and distinct narrative/parenthetical multicite commands, as documented by [the BibLaTeX package](https://ctan.org/pkg/biblatex). Map semantic citations through Terse macros to `textcite`, `parencite`, or multicite equivalents, preserving per-item locators. Use `sorting=none` and disable citation reordering. Citation style affects presentation, not membership or ordering. An absent date produces the style's undated rendering, never the current year.

Alternatives: pdfLaTeX provides broad submission compatibility but complicates Unicode/font coverage; LuaLaTeX is attractive for normal output but would require a second engine path for the chosen export profile. Natbib/BibTeX is simpler to package but makes Unicode metadata and independent per-item locators harder. Defer additional engine/backend support until the primary workflow works end to end.

Default output:

```text
build/academic/
  paper.tex
  terse-style.sty
  references.bib
  paper.map.json
  build-manifest.json
  COMPILE.txt
  paper.pdf                 # when requested compilation succeeds
  paper.bbl                 # when bibliography compilation succeeds
  figures/...
  theme-assets/...
```

`paper.tex` has a short conventional preamble, readable source comments with root-relative paths, and semantic macros/environments such as `TerseTitle`, `TerseHeading`, `TerseEquation`, `TerseTheorem`, `TerseFigure`, and `TerseReference`. Use environment bodies for long/block content rather than giant macro arguments; raw blocks are written in their original block position. The generated `.sty` includes all custom macro definitions and appearance. Standard dependencies are documented in `COMPILE.txt`; no Terse binary or macro registry is required. Standard math environments must remain nested within the appropriate generated equation environment.

Metadata renders as title, subtitle, authors/affiliations, explicit date, abstract, keywords, then body; themes may distribute title material over pages while preserving that order. An equation with an ID receives a numbered anchor; one without an ID is displayed unnumbered. Figures, tables, and theorem-like statements use deterministic counters; proof labels remain references without inventing a theorem statement. Implement cross-reference text using resolved kinds and trusted macros, rather than guessing types from TeX labels. The first release need not depend on a general cross-reference inference package.

Ordinary text escaping covers `\\`, `{`, `}`, `%`, `$`, `&`, `_`, `#`, `~`, and `^` by output context. URLs, code spans, metadata/PDF strings, bibliography values, macro optional arguments, and file paths have distinct encoders; do not run one global replacement pass. Supported link schemes are HTTPS, HTTP, and mailto; link destinations are never fetched. Code/math are not parsed for citations. Reject control characters and dangerous link schemes. Preserve all Unicode text supported by the selected fonts; missing glyphs during compilation are actionable failures. `--tex-only` reports that glyph coverage and rendering were not engine-validated.

Map every generated text segment to its originating source span where possible. `paper.map.json` has a schema version, relative source names, and sorted generated line/column intervals. Verbatim math/raw blocks have line-level mappings; escaped text and macro expansions fall back to the nearest originating node/field. Map generated style parameters to `.theme` locations. Compiler-owned scaffolding has an explicit generated origin; do not invent a `.trs` position for it. BibLaTeX/provider failures map through the alias to its declaration and relevant override field. Comments aid manual inspection but are not the only source map.

Run the engine with an argument vector, controlled working directory, no shell interpolation, and flags equivalent to `-no-shell-escape -interaction=nonstopmode -halt-on-error -file-line-error -recorder`. Start with a clean staging directory. Run XeLaTeX, then Biber when citations require it, then XeLaTeX until reference state converges (bounded to five total engine passes). Nonzero exit, unresolved references/citations after the limit, missing assets, or missing glyphs fail the attempt. Parse file/line diagnostics and retain the generated location as related information. Timeouts terminate the process tree.

No automatic package installation, remote bibliography sources, or engine downloads occur. Clear inherited TeX/Biber search-path overrides; allow only the staging tree and documented distribution resources. Prefer restrictive TeX input/output policies and close unrelated handles. Shell escape being disabled is not a complete OS sandbox: raw TeX and user-supplied style files remain trusted code and can exploit engine capabilities. Document external sandboxing for untrusted projects rather than promising that command-line flags make arbitrary TeX safe.

Allow explicit `[latex].support-files` and `[latex].packages` string arrays in the manifest for normal raw-TeX builds. Paths resolve from the project root; copy support files to deterministic package-local paths, preserving declared relative dependencies, and reject collisions with compiler-generated files. Package names are validated names, not arbitrary preamble fragments; names outside the built-in supported set require corresponding declared support files. A built-in `tikz` option enables the brief's drawing example without fetching dependencies. All additional support files/packages are part of the raw trust boundary, and `check --strict` warns on them. The ordinary semantic path uses only compiler-owned styles and the fixed package set. Export rejects unverifiable extensions rather than quietly omitting them.

### 9. Determinism, formatting, and disposable caches

Terse-generated `.tex`, `.sty`, `.bib`, maps, manifests, and instructions use stable ordering, fixed serialization versions, root-relative paths, and no timestamps or random identifiers. Explicit document dates are preserved; no implicit `today` macro is emitted. Asset bytes are copied unchanged. Empty `.bib` output has a fixed representation. Rebuilding after deleting caches must produce identical text bytes.

PDFs and externally generated `.bbl` depend on a compatible TeX/Biber installation. Include `.bbl` in the reproducibility check when toolchain versions/profile are pinned; reject or remove incidental volatile data by documented serialization rules, never by silently changing bibliographic meaning. Logs and auxiliary files are transient staging diagnostics and are not published text artifacts. Runtime details, tool executable paths, durations, and validation results belong to a separate local report under `.terse-cache/`, outside reproducible artifacts.

The formatter edits the lossless tree: indentation, attribute separators, known declaration spacing, and non-payload line endings. Preserve paragraph wrapping, comment placement, metadata order, citation ordering, escaped text, and opaque payload bytes. Never sort authored nodes or silently correct invalid grammar. Parse all requested files before writes; malformed input leaves files untouched. `fmt --check` runs the same transform in memory and returns code `1` with file differences if any exist. Successful formatting must reparse to the same semantic content and be idempotent.

Caches under `.terse-cache/` may hold parsed modules, normalized provider responses, and compilation diagnostics. Keys include compiler/schema versions and content digests of all relevant inputs. Cache reads never bypass lock validation or dependency checks; corrupted/missing cache entries cause recomputation. Provider caches are usable only by explicit reference-resolution commands. A refresh bypasses cached provider data. `.gitignore` entries cover `build/`, `.terse-cache/`, the configured output path, and standard LaTeX auxiliaries without ignoring authored figures or source PDFs.

Alternative: pretty-printing the AST would erase paragraph choices and raw bytes; using unordered serializer output or provider timestamps would create unstable diffs. Treat canonical serialization as a versioned contract with golden tests.

### 10. Atomic publication and watch recovery

Each build computes an immutable input snapshot and stages all generated files in a sibling temporary directory on the same filesystem as the destination. Compile there. Before publishing, check whether dependencies changed during the attempt; if so, mark it superseded and schedule a fresh build instead of publishing stale output.

Serialize publications with an output lock. Replace files only in a Terse-owned output directory identified by its manifest; refuse an unowned populated destination. Use an atomic directory replacement where supported, otherwise rename the previous generation to a backup, rename the completed stage into place, and roll back on failure. A small journal supports recovery after a crash between renames. This fallback may have a brief path-availability gap but must not leave partially mixed artifacts. Never remove the previous valid generation until the new one is installed. Generated output is editable by users; the documentation warns that the next successful build replaces that managed output and advises copying it for independent editing.

Watch uses filesystem events with a polling fallback and a 150 ms trailing debounce window. Watch parents of dependency files to handle atomic editor saves, deletion/recreation, and missing dependencies. Include the manifest, selected theme, lock/overrides, transitive sources, and all used assets. Ignore output/cache events. Keep the union of last successful dependencies and newly discovered/attempted dependencies after failures; prune obsolete dependencies only after a successful rebuild. One build runs at a time, with at most one pending successor.

On a syntax, reference, asset, or engine error, publish diagnostics only. The previous directory and PDF remain intact. Correcting the input or restoring a missing file triggers a new attempt. A theme switch or manifest update reconstructs the graph. This model also prevents a failed `refs resolve` or `export` from leaving partially updated authoritative/output artifacts.

### 11. arXiv export uses an explicit offline target profile

Ship a versioned `texlive-2025-xelatex` profile with the compiler, based on the currently documented target; later profile updates require a compiler/configuration change, never a network lookup during export. It lists engine, standard package/font assumptions, supported asset types, and bibliography strategy. A different local toolchain may validate ordinary compilability but cannot establish an exact target match.

The default export includes generated `.bib` and omits a prebuilt `.bbl`, letting a compatible target rebuild it. Current arXiv documentation describes bibliography processing and warns about version-sensitive prebuilt BibLaTeX `.bbl` files. See [arXiv's submission instructions](https://info.arxiv.org/help/submit_tex.html). An optional `--include-bbl` requires a verified compatible bibliography toolchain and matching main stem; an incompatible or unverifiable `.bbl` is an error, not silently shipped. This is a design choice to reduce unnecessary version coupling.

Export reconstructs an artifact plan from validated inputs and selects only `.tex`, required `.sty`/support files, `.bib`, permitted `.bbl`, used figures/theme assets, and `MANIFEST.json`. Standard TeX-distribution packages are documented toolchain dependencies, not copied machine-wide. Custom generated style files are always included. Do not bundle the rendered paper PDF, engine auxiliaries/logs, source maps, cache, editor files, unused figures, original `.trs` sources, or host metadata. A figure PDF is a required asset and must remain included.

The normal export destination is `build/export/<theme>/arxiv/`, with a sibling `<entry>-arxiv.zip` and separate `validation.json`/human report. The archive contains files at its root, not nested under the staging directory. Manifest entries are sorted relative paths with sizes and content hashes; the manifest describes the payload and explicitly excludes its own hash to avoid recursion. ZIP ordering, timestamps (a fixed representable epoch), permissions, compression settings, and platform attributes are fixed. Do not preserve source file modification times or filesystem owner information.

Copy from the artifact allowlist only; never recursively archive a project directory. Reject absolute paths, symlink escapes, traversal, missing assets, case-fold collisions, unsupported dependencies, and references to files outside the package/distribution closure. Extract the finished ZIP into a clean temporary directory, verify its manifest, and compile there when a compatible engine is available. Use an empty user TeX tree and controlled search paths. Inspect recorder output to ensure inputs are packaged files or known distribution dependencies, normalizing paths before reporting. Export never fetches packages, bibliographic metadata, fonts, or other dependencies.

Raw TeX or custom executable support files prevent a reliable static dependency/security closure in the MVP. Normal builds support them, but arXiv export fails with `E-EXPORT-004` identifying each block/file and explaining the unsupported boundary. Successful local compilation alone is not proof that arbitrary raw TeX has no conditional external dependencies. The author can keep using the ordinary LaTeX output independently; the exporter never removes the raw content to force success.

Validation states are explicit: `static-only` when no compatible local engine is available; `compiled-local` when the extracted package compiles using a compatible local toolchain; `compiled-profile` only when engine, packages, and font profile are actually matched in the pinned validation environment. Missing or risky dependencies always fail. With no engine, a statically closed semantic-only package may be produced with an explicit uncompiled report; `--require-compile` makes absent validation tooling an error. Even `compiled-profile` does not claim guaranteed acceptance by arXiv or perform an upload. CI's release gate requires compilation, not a static-only report.

Alternative: archiving the build directory risks stale data and accidental files; prebuilding a `.bbl` unconditionally couples output to local package versions; automatic conversion to another engine risks changing content or math support. The explicit profile and clean extraction test make the limits inspectable.

### 12. Diagnostics as a shared data model

All stages return `Diagnostic { severity, code, message, primary, related, help }`. Source-backed diagnostics always name the original relative file; line/column are one-based Unicode scalar positions, while JSON also provides byte ranges. File-wide errors (invalid UTF-8, missing include target, invalid manifest) anchor to the referencing source/configuration where available. A process-start failure without a specific source location has a null span and the relevant configuration location as related context.

Reserve stable families: `E-PARSE-*`, `E-META-*`, `E-INCLUDE-*`, `E-ID-*`, `E-REF-*`, `E-CITE-*`, `E-THEME-*`, `E-ASSET-*`, `E-CONFIG-*`, `E-LATEX-*`, `E-EXPORT-*`, and warning families. Preserve the brief's codes: `E-CITE-001` unknown alias, `E-ID-002` duplicate ID, `E-INCLUDE-003` cycle, and `W-TEX-001` raw portability warning. Codes are never repurposed within a supported diagnostic schema version.

Human output includes severity/code, file/line/column, an excerpt, related definitions/include edges, and reliable remediation. JSON emits a versioned object with diagnostics in deterministic source-traversal/position/code order; all progress text goes to stderr. Watch JSON emits one JSON object per build attempt with a monotonic attempt number, status, diagnostics, and publication result. No ANSI codes or prose contaminate JSON stdout. Errors can recover at structural boundaries for additional diagnostics, but an invalid AST is never emitted as a successful document.

### 13. Acceptance evidence, milestones, and documentation

The next `tests.md` will specify test cases, but the architecture must support these test seams now:

| Scenario | Required validation approach |
| --- | --- |
| A | Same multi-file fixture, equal semantic projection and `.tex` body, differing `.sty`; compile both in a pinned environment and compare rendered pages for the intended corporate/academic differences and visible content. |
| B | Copy only generated deliverables to an isolated directory/container with Terse absent and network disabled; run documented conventional commands. |
| C | Recorded DOI provider response through the real normalization/lock pipeline; subsequent build with HTTP disabled; validate `.bib` via Biber/LaTeX. Add equivalent versioned/unversioned arXiv cases. |
| D | Unknown and unresolved aliases, stale identifiers/overrides, and misleading leftover lock entries assert stable codes and original positions. |
| E | Cross-module equation/heading/figure/table/theorem/proof references compile with valid anchors and no unresolved-reference diagnostics. |
| F | Direct/indirect/repeated-include fixtures assert complete cycle paths, related edges, and duplicate-ID behavior before any backend call. |
| G | Two clean directories and cache/no-cache builds compare every generated text artifact; change host paths/order/locale and keep explicit configuration constant. Pin tools when comparing optional `.bbl`. |
| H | Golden formatter cases plus parse-format semantic equivalence and second-pass identity, including comments, multiline prose, math/raw bytes, themes, and CRLF inputs. |
| I | Reject visual attributes in every content context, unknown properties, content-selecting theme rules, and configurations that hide nodes. |
| J | Raw bytes survive normal generation; strict warnings point to their source; restricted math cannot smuggle execution. |
| K | Watch integration modifies, deletes, atomically replaces, and repairs transitive files/assets/themes/lock data; failed builds preserve the previous output bytes. |
| L | Verify exact ZIP membership/path safety and manifest hashes, clean extraction compilation, optional `.bbl` rules, raw-extension rejection, honest unavailable-tool reports, and absence of upload/network calls. |

Unit/golden tests cover syntax and serialization; provider tests use committed response fixtures and an injected HTTP client; process tests use a fake runner for errors and a pinned real toolchain for acceptance. Network integration smoke tests are opt-in and never required for ordinary CI. Regression tests cover malformed input, path escapes, math-validation bypasses, and resource limits; targeted fuzzing can expand coverage as those components mature. Visual tests accompany semantic assertions rather than replacing them. The maintainer can perform the release fixture's visual inspection of both PDFs; an independent reviewer is welcome but not a release prerequisite. Machine snapshots alone are insufficient evidence that all prose is visible.

Use one pinned Linux LaTeX/Biber environment for the complete PDF, export, and visual acceptance suite, plus a small Linux/macOS/Windows matrix for compiler tests and executable smoke tests. Exercise platform-sensitive paths, process arguments, watching, and publication through focused tests on each supported OS. Do not require the full TeX distribution/version/font matrix for every change. All A–L acceptance scenarios remain release requirements; the smaller environment matrix controls maintenance cost without dropping them. Publish the pinned environment definition and local reproduction commands in the repository. The author initially owns updates to this environment and its arXiv profile.

Implement as end-to-end milestones:

1. **First portable page:** executable, init, minimal metadata/paragraph/heading grammar, academic theme, check, deterministic `.tex`/`.sty`, source diagnostics, optional XeLaTeX, and install/build documentation.
2. **One paper, two themes:** complete document element and inline syntax, restricted math/raw boundary, figure/table assets, theorem/proof/reference macros, Magalu theme, content-equivalence assertions, and normal portable compilation.
3. **Multi-file scholarly paper:** module resolution, global symbols, DOI/arXiv resolution, lock/overrides, bibliography commands, offline builds, and the committed realistic two-theme paper fixture.
4. **Git and editing workflow:** complete formatting, JSON diagnostics, idempotence/determinism tests, safe output publication, watch dependencies and error recovery, and CI/Git documentation.
5. **Export and release:** target profile, safe ZIP/manifest generation, extracted-package compilation, dependency rejection, cross-platform packaging, all A–L acceptance gates, installation/language/theme/citation/export guides, contribution instructions, and the author's chosen license/package metadata.

Each milestone ships a runnable user path and its tests; none is solely an infrastructure layer. The release fixture includes title/subtitle, authors/affiliations, abstract/keywords, all required blocks/inlines, at least three heading levels, cross-file references, supported DOI/arXiv identifiers with locked metadata, a figure, a table, and both themes. Put raw-TeX cases in a separate fixture so the primary arXiv demonstration stays within the verified export subset. Use original or redistributable checked-in example prose/assets with documented provenance. Keep `magalu` and `academic` as the two theme names for the acceptance demonstration, with `magalu` clearly identified as an unofficial example. Use a generic redistributable demonstration graphic for logo behavior; company logos, private documents, and internal fonts are not prerequisites for public tests or releases. An author can supply personal theme assets in their own document project.

From a clone with the documented Rust prerequisites, the supported source installation command will be `cargo install --locked --path crates/terse-cli`. Release automation also produces standalone executables for the supported OS/architecture matrix. Installation of a compatible TeX distribution/Biber is a separately documented prerequisite for PDFs, never a hidden step in `terse build`. Generated `COMPILE.txt` states the XeLaTeX/Biber sequence a user runs without Terse.

## Risks / Trade-offs

- **Language ambiguity and formatter drift** -> Keep the grammar small, recognize explicit reserved starts, retain lossless trivia, and test parse/format semantic equivalence before adding convenience syntax.
- **Theme settings hide or overflow authored content** -> Limit tokens/layouts, validate geometry and visibility, centralize emission, and combine equality tests with compiled visual/content inspection.
- **Raw TeX and custom macros are not safely analyzable** -> Validate ordinary math with a closed grammar, keep raw source explicit, disable shell escape, document trusted execution, and reject unverifiable raw extensions in arXiv export.
- **Font/Unicode coverage varies** -> Use filename-resolved distribution fonts, validate known language mappings, fail missing glyphs, and document that arbitrary scripts/font shaping may need future target support.
- **arXiv/toolchain support changes** -> Ship versioned offline profiles, cite their basis, revalidate profiles for releases, report actual local validation, and avoid incompatible prebuilt `.bbl` files.
- **Metadata providers return incomplete or changing records** -> Lock normalized/effective metadata, provide explicit overrides, validate provider identity, use bounded retries, and keep builds offline.
- **Atomic directory replacement differs by platform** -> Stage on the same filesystem, lock outputs, use a recovery journal/rollback fallback, and test interrupted publication on each supported OS.
- **Watch events are lost or arrive during a build** -> Watch parent directories, retain missing dependencies, support polling, and verify input digests before publication.
- **A single full-featured MVP is substantial** -> Deliver runnable milestones, keep unsupported syntax/providers/targets explicit, and require the whole release workflow before claiming completion.
- **Maintenance exceeds one person's capacity** -> Keep one compiler/backend/toolchain profile, automate reproducible checks, use a focused cross-platform matrix, and expand tooling or support commitments when actual usage warrants it.

## Migration Plan

There is no existing Terse source or API to migrate. Introduce `format-version`, lock/normalization versions, diagnostic schema versions, and the style ABI from the first milestone. Before 1.0, syntax changes require release notes and fixture migrations; never rewrite source or lockfiles automatically during a build. `init` and reference resolution are the only commands that create or update their respective authoritative scaffolding/data, and `fmt` rewrites only explicitly selected/reachable source/theme files.

Release gates include all A–L tests, offline output compilation with Terse absent, export extraction tests, the two-theme demonstration, and smoke tests for every supported executable package. Use the focused validation matrix defined above, with publicly reproducible fixture/toolchain inputs. The author can run and review these checks as the sole maintainer. The first open-source release also includes the chosen license, package metadata, contribution instructions, and bundled-asset notices. A failed release can roll back the executable/version while keeping source intact; incompatible newer lock/schema versions fail with a clear version diagnostic instead of being reinterpreted. Failed builds/exports/resolutions roll back through their transactional writes.

## Open Questions

- **License selection:** the author will choose the project's open-source license before its public release. Include generated style/template distribution in that decision; no particular license has been assumed or applied.
- **Additional language/script coverage:** `en` and `pt-BR` are the initial validated locale mappings. Expand only with font, quotation, hyphenation, and bibliography fixtures; unsupported mappings remain explicit errors.

These questions do not block capability-spec authoring. The following specs and test plan must preserve the decisions and acceptance gates in this document rather than silently expanding the raw-TeX export guarantee or narrowing the core document model.
