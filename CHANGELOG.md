# Changelog

All notable changes to Terse are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0,
a minor version may change the language, the theme format, or the generated
LaTeX.

## [0.1.0] - Unreleased

The first public version.

### Language

- `.trs` documents with entry metadata (title, subtitle, authors with
  affiliations, date, abstract, keywords, `en` or `pt-BR`), three heading
  levels with ids, nested ordered and unordered lists, emphasis, strong,
  inline code, links, and footnotes.
- Theorem-like blocks with titles and ids, and proofs bound to their
  theorem by id rather than by position.
- Figures with captions, alt text, and a `wide` role; tables written either
  as `table:` blocks or as GFM pipe tables, with inline content in cells and
  an optional caption.
- Restricted math through `math:` blocks, `\(...\)`, `$...$`, and `$$...$$`,
  checked against a closed command allowlist at parse time.
- Fenced code blocks rendered with `listings`, and explicit raw TeX through
  `tex:` blocks, which always raise a warning.
- Multi-file projects through `include`, with one project-wide id namespace
  for cross-references.
- Located diagnostics with stable `E-`/`W-` codes, as text or JSON.

### Output and themes

- Generated LaTeX that is meant to be read and edited: content in
  `paper.tex`, every presentation decision in `terse-style.sty`, and
  `COMPILE.txt` with the equivalent manual `xelatex`/`biber` sequence.
- Without a theme, the plain LaTeX `article` look. A `.theme` file is a
  closed declarative schema (no code, no imports) whose properties change
  only what they declare: page size and margins, columns, fonts, figure
  defaults, citation style, watermark, and logo.
- The same sources render under different themes with byte-identical
  `paper.tex`; only the style layer differs.

### Citations

- `refs:` declarations for DOI and arXiv identifiers, resolved by the
  explicit, transactional `terse refs resolve` into a committed lock file.
  No other build step touches the network.

### Commands

- `init`, `check`, `build`, `fmt`, `watch`, `export --target arxiv`,
  `refs resolve`, `doctor`, and `toolchain install|status|update|uninstall`.
- `export --target arxiv` builds a self-contained, offline package validated
  against the TeX Live 2025 profile arXiv compiles with.
- `terse toolchain install` provisions a private, pinned TeX Live 2025 with
  XeLaTeX and Biber on Linux and macOS, verified against a pinned SHA-512;
  `terse doctor` diagnoses an existing toolchain and suggests per-OS fixes.

[0.1.0]: https://github.com/caiocgomes/terse/commits/main
