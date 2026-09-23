## Why

Authors already write in Markdown. Today Terse only accepts `.trs` sources (`crates/terse-cli/src/format.rs:42`), so a paper written in `.md` has to be rewritten before it can become LaTeX. A plain Markdown file should compile to the same readable LaTeX project, and the same PDF, that the equivalent `.trs` produces.

## What Changes

- `terse check`, `terse build`, and `terse watch` accept a `.md` entry file. It is parsed as CommonMark and lowered into the existing semantic tree. Everything after that (semantic validation, themes including the plain-article default, LaTeX generation, source maps, PDF) is shared with `.trs` and does not change.
- Only Markdown constructs with a Terse equivalent are accepted: headings `#` to `###`, paragraphs, emphasis, strong, links, inline code, ordered and unordered nested lists, math through `$...$` / `$$...$$` (from `dollar-math-delimiters`), and GFM pipe tables, mapped onto the table node from `pipe-tables` (captionless, so rendered in place and unnumbered, with delimiter-row alignment and inline cells).
- Fenced code blocks (with or without a language tag) and indented code blocks map onto the code block node from `code-blocks`, rendered through `listings`.
- An image alone in its paragraph (`![alt](path)`) becomes a Terse figure whose caption and alt text are both the image's alt text, as in Pandoc. An image inside running text is a diagnostic, because Terse has no inline image.
- Document metadata (title, authors, date, language, abstract, keywords) comes from an optional YAML front matter block that holds the same fields as the `.trs` `document:` block.
- **Title and metadata become optional for every entry, `.trs` included.** Terse stops assuming every document is a paper with a title. An entry with no `document:` block (or no front matter), or with a `document:` block that has no title, compiles. Each metadata field is independent, as in a LaTeX document: a declared field is rendered, and an absent one leaves no trace, meaning no empty heading, no placeholder, and no date the author did not write. The title block (`\maketitle`) runs only when at least one of title, subtitle, authors, or date is declared, so authors without a title still appear; `pdftitle` and `pdfauthor` are emitted only for fields that exist. The model already allows it: `ParsedModule.metadata` is an `Option` (`crates/terse-core/src/semantic/mod.rs:51`), and only the required `title: String` (`:31`) and the entry-level rule stand in the way. Documents that declare a title keep today's output byte-for-byte.
- Markdown constructs with no Terse equivalent produce a source-located diagnostic error. They are never silently dropped. This covers headings deeper than `###`, raw HTML, block quotes, and thematic breaks. The semantic tree has no quote or rule node (`crates/terse-core/src/semantic/mod.rs:84`).

### Non-goals

- `include` between `.md` and `.trs` in either direction. A project is either one or the other.
- Terse-only constructs inside `.md`: theorems and proofs, `{ref: id}`, element ids, figure roles, citations and `refs:`.
- `terse fmt` for `.md` files.
- Markdown dialects beyond CommonMark plus front matter, dollar math, and GFM pipe tables.
- Executing code blocks.

## Capabilities

### New Capabilities

- `markdown-source`: accepting a `.md` entry, the CommonMark subset and its mapping onto semantic nodes, front matter as document metadata, and diagnostics for unsupported constructs.

### Modified Capabilities

- `multi-file-projects`: entry discovery and explicit CLI entry paths accept `.md` as well as `.trs`.
- `language-parsing`: "Complete document metadata" no longer requires the entry to carry a `document:` block or a title. The remaining rules stay: at most one `document:` block, entry-only, and nothing in included modules.
- `latex-generation`: title material and PDF metadata render per declared field; absent fields leave no trace, and the title block runs only if title, subtitle, authors, or date is declared.

## Impact

- New Markdown front end in `crates/terse-core` that produces the same semantic tree the `.trs` lowering produces. The parser dependency (for example `pulldown-cmark` or `comrak`) is a design decision.
- `crates/terse-cli`: entry-extension dispatch for `check`, `build`, and `watch`.
- `docs/`: a page on the supported Markdown subset.
- Depends on `dollar-math-delimiters` (archived 2026-09-23) for math on `pipe-tables` for tables, and on `code-blocks` for code. Both must land first.

## Open for design

- **Markdown parser library and `$`.** Decided: `$$` in the middle of a paragraph is rejected in `.md` as in `.trs` (author, 2026-09-23). If the chosen library (`pulldown-cmark` or `comrak`) brings its own `$` handling, the design decides whether to use it or apply the `.trs` rule, which today matches Pandoc 3.11 byte for byte.
