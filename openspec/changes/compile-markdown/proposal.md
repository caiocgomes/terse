## Why

Authors already write in Markdown. Today Terse only accepts `.trs` sources (`crates/terse-cli/src/format.rs:42`), so a paper written in `.md` has to be rewritten before it can become LaTeX. A plain Markdown file should compile to the same readable LaTeX project, and the same PDF, that the equivalent `.trs` produces.

## What Changes

- `terse check`, `terse build`, and `terse watch` accept a `.md` entry file. It is parsed as CommonMark and lowered into the existing semantic tree. Everything after that (semantic validation, themes including the plain-article default, LaTeX generation, source maps, PDF) is shared with `.trs` and does not change.
- Only Markdown constructs with a Terse equivalent are accepted: headings `#` to `###`, paragraphs, emphasis, strong, links, inline code, ordered and unordered nested lists, and math through `$...$` / `$$...$$` (from `dollar-math-delimiters`).
- Document metadata (title, authors, date, language, abstract, keywords) comes from an optional YAML front matter block that holds the same fields as the `.trs` `document:` block.
- **Title and metadata become optional for every entry, `.trs` included.** Terse stops assuming every document is a paper with a title. An entry with no `document:` block (or no front matter), or with a `document:` block that has no title, compiles; the generated LaTeX then has no title block (`\maketitle` is not run) and no `pdftitle`. The model already allows it: `ParsedModule.metadata` is an `Option` (`crates/terse-core/src/semantic/mod.rs:51`), and only the required `title: String` (`:31`) and the entry-level rule stand in the way. Documents that declare a title keep today's output byte-for-byte.
- Markdown constructs with no Terse equivalent produce a source-located diagnostic error. They are never silently dropped. This covers headings deeper than `###`, raw HTML, block quotes, thematic breaks, and code blocks. The semantic tree has no code-block, quote, or rule node; its block kinds are Heading, Paragraph, List, Equation, Figure, Table, TheoremLike, and Proof (`crates/terse-core/src/semantic/mod.rs:84`).

### Non-goals

- `include` between `.md` and `.trs` in either direction. A project is either one or the other.
- Terse-only constructs inside `.md`: theorems and proofs, `{ref: id}`, element ids, figure roles, citations and `refs:`.
- `terse fmt` for `.md` files.
- Markdown dialects beyond CommonMark plus front matter and dollar math.

## Capabilities

### New Capabilities

- `markdown-source`: accepting a `.md` entry, the CommonMark subset and its mapping onto semantic nodes, front matter as document metadata, and diagnostics for unsupported constructs.

### Modified Capabilities

- `multi-file-projects`: entry discovery and explicit CLI entry paths accept `.md` as well as `.trs`.
- `language-parsing`: "Complete document metadata" no longer requires the entry to carry a `document:` block or a title. The remaining rules stay: at most one `document:` block, entry-only, and nothing in included modules.
- `latex-generation`: the title block and PDF title are emitted only when a title exists.

## Impact

- New Markdown front end in `crates/terse-core` that produces the same semantic tree the `.trs` lowering produces. The parser dependency (for example `pulldown-cmark` or `comrak`) is a design decision.
- `crates/terse-cli`: entry-extension dispatch for `check`, `build`, and `watch`.
- `docs/`: a page on the supported Markdown subset.
- Depends on `dollar-math-delimiters` for math. That change should land first.

## Open for design

- **Metadata without a title.** Authors, date, or an abstract may be declared without a title. The design decides whether to render them anyway (for example authors and date above the body) or to reject them with a diagnostic.
- **Standalone images.** `![alt](path)` alone in a paragraph is the natural equivalent of a Terse figure, but a Terse figure requires both a caption and alt text. Either map it (caption from the image title, for example) or reject it.
- **Code blocks.** Common in real Markdown, but they have no semantic node. Rejecting them keeps this change small. Supporting them would be a separate change.
- **GFM pipe tables.** Terse has tables, but CommonMark does not, and Terse tables require a caption.
