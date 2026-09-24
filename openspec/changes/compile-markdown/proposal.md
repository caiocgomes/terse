## Why

Markdown was meant to be a simpler way to write what TeX typesets, and Terse takes that idea to its end. For Terse to be practical, it also has to compile any Markdown text, which makes Markdown a restricted Terse. Both compile to TeX.

Today Terse reads only `.trs`, so a Markdown document has to be rewritten by hand. The changes this builds on are archived (`dollar-math-delimiters`, `code-blocks`, `pipe-tables`), and `.trs` already spells most of Markdown the same way: `#` headings, `-` lists, `*emphasis*`, backtick code, links, `$` math, fenced code, and pipe tables.

## Direction

The destination is a syntactic subset: every `.md` file is a valid `.trs` file with Terse's extensions turned off. Getting there means relaxing `.trs` rules that the specs currently state as MUST, one by one:

- indentation of exactly two spaces per level;
- delimiters that must balance without crossing;
- unknown backslash escapes as errors;
- `_x_` as emphasis;
- a malformed pipe-table delimiter as an error rather than prose;
- setext headings.

That convergence is its own later change. This change gets there through translation: a `.md` file is translated into `.trs` text, which the existing pipeline then compiles. The relation is enforced by construction, because anything the translator cannot write in `.trs` cannot exist in `.md`. The paired `.md`/`.trs` fixtures this change adds become the conformance suite that tells the convergence change when it is done. At that point the translator reduces to reading `.md` with the `.trs` parser, with extensions off.

## What Changes

- **Entry and include.** `terse check`, `build`, `watch`, and `refs resolve` accept a project whose manifest `entry` is a `.md` file, and a `.trs` module may `include "chapter.md"`. The manifest stays required. A `.md` file has no includes of its own.
- **Translation to `.trs`.** A `.md` file is parsed as CommonMark, translated into `.trs` text, and compiled by the unchanged pipeline. So every node passes the existing validators (the math allowlist, permitted link schemes, figure-path root confinement, and `\end{TerseCode}` rejection), and a `.md` file produces the same TeX as its translation. Diagnostics report the original `.md` file, line, and column, never positions in the generated text.
- **Literal text stays literal.** Anything that is plain text in Markdown but syntax in `.trs` is escaped during translation, so Markdown text never becomes a construct its author did not write. This covers a reserved word at the start of a paragraph (`theorem:`, `table`, `include`), `{ref: x}`, `^[`, a `@` with no declared `refs:`, and backslash sequences. The set is closed and tested as a property: arbitrary Markdown text translates to `.trs` that renders the same characters.
- **Construct mapping.**
  - Headings `#` to `#####`, paragraphs, emphasis (`*` and `_`, written as `*`), strong, links (inline, reference-style, autolinks), inline code, and nested lists map directly.
  - Math through `$...$` and `$$...$$`, with the `.trs` rule. It is recognized when Markdown is tokenized, so the `*b*` in `$a*b*c$` never becomes emphasis.
  - GFM pipe tables become captionless tables: in place, unnumbered, aligned, with inline cells.
  - Fenced and indented code blocks become code blocks.
  - Footnotes `[^n]` become `^[...]`. A footnote with more than one block is an error.
  - Block quotes, thematic breaks, and hard line breaks use the `.trs` syntax from the prerequisite change. `######` translates to level 5 with a warning.
  - An image alone in its paragraph with a local path becomes a figure whose caption and alt text are both the image's alt text, as in Pandoc. An image inside running text, or one with a remote URL, is a warning and translates to its alt text.
  - HTML comments are dropped. Any other raw HTML is a warning and is dropped.
  - Strikethrough and task-list markers stay literal text, as plain CommonMark reads them.
- **Front matter.** An optional YAML front matter block becomes a `document:` block and a `refs:` block. It uses Pandoc's keys: `title`, `subtitle`, `author` (a string, a list of strings, or a list of `name`/`affiliation` maps), `date`, `abstract`, `keywords`, and `lang` (the locales `.trs` accepts), plus `refs:` for reference declarations. Unknown keys, such as `bibliography`, `geometry`, or `output`, are warnings and are ignored. The YAML is restricted: anchors, aliases, tags, and multiple documents are located errors.
- **Citations, opt-in.** `[@alias, p. 12]` and `@alias` become Terse citations only when the front matter declares `refs:`. Otherwise they translate to escaped text, so `@someone` in a note never fails a build.
- **Title and metadata become optional for every entry, `.trs` included.** An entry with no `document:` block or front matter, or one with no title, compiles. Each field is independent, as in a LaTeX document: a declared field renders, and an absent one leaves no trace (no empty heading, no placeholder, no invented date). The title block runs only when at least one of title, subtitle, authors, or date is declared. `pdftitle` and `pdfauthor` are emitted only for fields that exist. Documents that declare a title keep today's output byte for byte.

### Non-goals

- The grammar convergence itself. `.trs` keeps its current strictness in this change.
- A `.md` including anything, and `.md` spellings of Terse-only constructs: theorems and proofs, `{ref: id}`, element ids, figure roles, numbered equations, and captioned tables.
- Theme properties for block quotes, rules, and heading levels 4 and 5.
- Markdown beyond CommonMark plus front matter, dollar math, GFM pipe tables, and footnotes: strikethrough, task lists, definition lists, `{#id}` attributes, Pandoc `Table:` captions, and `bibliography:` `.bib` files.
- A command that exposes the translation (for example `terse convert`), `terse fmt` for `.md`, compiling without a manifest, and executing code.

## Capabilities

### New Capabilities

- `markdown-source`: the `.md` source, its translation to `.trs` and the escaping that keeps literal text literal, the supported CommonMark subset and each construct's translation, front matter as metadata and reference declarations, opt-in citations, the warnings and errors for everything else, and the paired `.md`/`.trs` conformance fixtures.

### Modified Capabilities

- `multi-file-projects`: the manifest entry may be `.md`; a `.trs` include may name a `.md` file; image paths resolve relative to the declaring `.md`.
- `language-parsing`: "Complete document metadata" no longer requires a `document:` block or a title. The other rules stay: at most one `document:` block, entry-only, nothing in included modules.
- `latex-generation`: title material and PDF metadata render per declared field.
- `diagnostics`: positions in a translated `.md` source are reported against the original `.md` bytes.

## Impact

- **Prerequisite:** a change that gives `.trs` the CommonMark spellings of block quotes, thematic breaks, heading levels 4 and 5, and hard line breaks, together with their semantic nodes and LaTeX rendering. The translator needs somewhere to write them.
- **`crates/terse-core`:** a Markdown-to-`.trs` translator with a position map back to the `.md` bytes. It needs two new dependencies, chosen in the design: a CommonMark parser that can recognize dollar math during tokenization (for example `pulldown-cmark` or `comrak`), and a YAML parser restricted to the supported subset. `DocumentMetadata.title` becomes optional. That changes the digest of every existing document, so `PROJECTION_VERSION` goes to 4.
- **Include expansion** translates a `.md` target before parsing it.
- **`crates/terse-cli`:** entry dispatch by extension for `check`, `build`, `watch`, and `refs resolve`, plus `.md` include targets in project discovery and watch dependencies.
- **Diagnostics:** new codes. Warnings cover dropped HTML, inline or remote images, unknown front matter keys, and `######`. Errors cover restricted-YAML violations and footnotes with more than one block.
- **`docs/`:** a page on the supported Markdown and how each construct translates.

## Decisions (settled with the author)

- **2026-09-23:**
  - Title and metadata are optional for every entry, and an absent field leaves no trace.
  - `$$` in the middle of a paragraph is rejected in `.md` as in `.trs`.
  - An image alone in its paragraph becomes a figure, Pandoc style.
- **2026-09-24:**
  - Markdown is a restricted Terse, and both compile to TeX.
  - The destination is a syntactic subset, reached by translating `.md` to `.trs` now and converging the grammar in a later change.
  - The `.trs` syntax for the new block constructs lands first, in its own change.
  - A `.trs` module may include a `.md` file.
  - Citations are opt-in through `refs:` in the front matter.
  - Optional metadata stays inside this change.
  - Front matter uses Pandoc's keys, and unknown keys warn.
- **Defaults proposed with this text (the author can change them before the design):** the outcomes for `######`, footnotes, HTML comments, other HTML, inline and remote images, strikethrough, and task lists.

## Open for design

- Which CommonMark library, and how it recognizes `$` with the `.trs` rule during tokenization.
- Which YAML library, and how the restricted subset is enforced.
- The shape of the position map from generated `.trs` to `.md` bytes.
- Task order: the metadata change comes first, because it changes `.trs` behavior and can be verified on its own.
