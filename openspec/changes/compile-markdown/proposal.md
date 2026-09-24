## Why

Authors already write Markdown, but Terse compiles only `.trs`, so a Markdown document has to be rewritten before it can become LaTeX. A `.md` file should compile to the same readable LaTeX project and PDF that an equivalent `.trs` produces. The three changes this builds on are archived (`dollar-math-delimiters`, `code-blocks`, `pipe-tables`), so math, code, and tables already have semantic nodes for Markdown to map onto.

## What Changes

- **Entry.** `terse check`, `build`, `watch`, and `refs resolve` accept a project whose manifest `entry` is a `.md` file. The manifest stays required. The file is parsed as CommonMark and lowered into the existing semantic tree. Everything after lowering (validation, themes including the plain-article default, LaTeX generation, source maps, PDF) is shared with `.trs`.
- **Target: general Markdown, not only papers.** A typical README or notes file should build. Constructs map as follows:
  - **Direct mappings:** headings `#` to `###`, paragraphs, emphasis (`*` and `_`), strong, links (inline, reference-style, autolinks), inline code, nested ordered and unordered lists, math through `$...$` and `$$...$$`, GFM pipe tables (captionless, so in place and unnumbered, with alignment and inline cells), fenced and indented code blocks (the `code-blocks` node), and footnotes `[^n]` (the existing footnote node).
  - **Math uses the `.trs` rule.** `$` recognition and the `$$`-mid-paragraph error are the same as in `.trs`, proven by the same test cases. Math has to be recognized when Markdown is tokenized: otherwise the `*b*` in `$a*b*c$` becomes emphasis before anything else sees it.
  - **Images:** an image alone in its paragraph with a local path becomes a figure whose caption and alt text are both the image's alt text, as in Pandoc. An image inside running text, or one with a remote URL, is a warning and renders as its alt text: Terse has no inline image and never uses the network.
  - **New semantic nodes:** a block quote renders as `quote`. A thematic break renders as a centered rule. `####` becomes heading level 4 (`\paragraph`) and `#####` level 5 (`\subparagraph`); `######` renders as level 5 with a warning. A hard line break becomes a line-break inline (`\\`). These nodes are reachable only from `.md` in this change.
  - **Dropped with a trace:** HTML comments are ignored, since they are comments by intent. Any other raw HTML is a warning and is not rendered.
  - **Left as CommonMark text:** strikethrough (`~~x~~`) and task-list markers stay literal, as plain CommonMark reads them. Rendering them would add a LaTeX package to the pinned closure.
- **Same validation as `.trs`.** Every node from Markdown passes the existing validators: the math allowlist, the permitted link schemes, figure-path root confinement, and the `\end{TerseCode}` rejection inside code. A Markdown front end that built nodes without them would reopen what `.trs` closes.
- **Front matter.** An optional YAML front matter block supplies metadata with Pandoc's keys: `title`, `subtitle`, `author` (a string, a list of strings, or a list of `name`/`affiliation` maps), `date`, `abstract`, `keywords`, and `lang` (the locales `.trs` accepts). Unknown keys, such as `bibliography`, `geometry`, or `output`, produce a warning and are ignored. The YAML is restricted: anchors, aliases, tags, and multiple documents are located errors.
- **Citations, opt-in.** `[@alias, p. 12]` and `@alias` keep Terse's syntax and semantics once the front matter declares `refs:` (for example `robins1986: doi:10.1000/abc`). Without `refs:`, they are plain text, so `@someone` in a note never fails a build. Declared references go through the same lock and `terse refs resolve` flow as `.trs`.
- **Title and metadata become optional for every entry, `.trs` included.** An entry with no `document:` block or front matter, or one with no title, compiles. Each field is independent, as in a LaTeX document: a declared field renders, and an absent one leaves no trace (no empty heading, no placeholder, no invented date). The title block runs only when at least one of title, subtitle, authors, or date is declared. `pdftitle` and `pdfauthor` are emitted only for fields that exist. Documents that declare a title keep today's output byte for byte.

### Non-goals

- `include` between `.md` and `.trs` in either direction. A `.md` entry has no includes.
- Terse-only constructs in `.md`: theorems and proofs, `{ref: id}`, element ids, figure roles, numbered equations, and captioned or numbered tables. `{ref: x}` is literal text there, as CommonMark reads it.
- `.trs` syntax for block quotes, thematic breaks, heading levels 4 and 5, and line breaks.
- Theme properties for the new nodes. Levels 4 and 5 use the class's own commands.
- Markdown beyond CommonMark plus front matter, dollar math, GFM pipe tables, and footnotes: strikethrough, task lists, definition lists, `{#id}` attributes, Pandoc `Table:` captions, and `bibliography:` `.bib` files.
- `terse fmt` for `.md`, compiling without a manifest, and executing code.

## Capabilities

### New Capabilities

- `markdown-source`: the `.md` entry, the supported CommonMark subset and its mapping onto semantic nodes, front matter as metadata and reference declarations, opt-in citations, and the warnings and errors for everything else.

### Modified Capabilities

- `multi-file-projects`: the manifest entry may be a `.md` file; image paths resolve relative to the declaring `.md`; a `.md` entry has no includes, and `.trs` cannot include `.md`.
- `language-parsing`: "Complete document metadata" no longer requires a `document:` block or a title. The other rules stay: at most one `document:` block, entry-only, nothing in included modules.
- `semantic-ast`: the document model gains block quotes, thematic breaks, heading levels 4 and 5, and line breaks.
- `latex-generation`: title material and PDF metadata render per declared field, and the new nodes have defined renderings.

## Impact

- `crates/terse-core`: a Markdown front end that produces the same semantic tree the `.trs` lowering produces. New dependencies are a CommonMark parser that can recognize dollar math during tokenization (for example `pulldown-cmark` or `comrak`) and a YAML parser restricted to the supported subset. The design chooses both.
- The semantic model, the projection, the four citation and reference walkers, the source map, and the LaTeX backend gain the new nodes. `DocumentMetadata.title` becomes optional. That changes the digest of every existing document, so `PROJECTION_VERSION` goes to 4.
- `crates/terse-cli`: entry dispatch by extension for `check`, `build`, `watch`, and `refs resolve`, and project discovery for an entry with no includes.
- New diagnostic codes: warnings for dropped HTML, inline or remote images, unknown front matter keys, and `######`; errors for restricted-YAML violations and multi-block footnotes.
- `docs/`: a page on the supported Markdown and how each construct renders.

## Decisions (settled with the author)

- **2026-09-23:** Title and metadata are optional for every entry, and an absent field leaves no trace. `$$` in the middle of a paragraph is rejected in `.md` as in `.trs`. An image alone in its paragraph becomes a figure, Pandoc style.
- **2026-09-24:** The audience is general Markdown. Citations are supported, opt-in through `refs:` in the front matter. Optional metadata stays inside this change rather than a separate one. Front matter uses Pandoc's keys, and unknown keys warn.
- **2026-09-24, defaults proposed with this text:** the per-construct outcomes above (quote, rule, levels 4 and 5, line break, footnotes, HTML comments, other HTML, inline and remote images, strikethrough and task lists as text). The author can change any of them before the design.

## Open for design

- Which CommonMark library, and how it recognizes `$` with the `.trs` rule during tokenization.
- Which YAML library, and how the restricted subset is enforced.
- How the metadata change is ordered in the tasks. It comes first, because it changes `.trs` behavior and can be verified on its own.
