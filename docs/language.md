# The `.trs` language

A `.trs` file is a lossless, indentation-structured document source. Terse
lexes it byte-exactly (original UTF-8 bytes, line endings, and trivia are
retained for diagnostics and formatting) and lowers it into a typed semantic
tree before any LaTeX is generated.

## Entry metadata

Exactly one module per project (the entry, or any module reached only via
`include:`) may carry entry-only metadata: `title`, `authors` (ordered,
with optional affiliations), `abstract`, `keywords`, and `language` (`en`
or `pt-BR`). Declaring these outside the entry, or more than once, is a
diagnostic error — metadata placement is checked, not inferred.

## Structure

- Three heading levels, with optional `{id: ...}` for cross-references.
- Ordered and nested lists, with marker-independent continuation
  indentation.
- Theorem-like blocks (`theorem`, `lemma`, `definition`, `corollary`, and
  others — see `crates/terse-core/src/semantic/mod.rs` for the closed set)
  with optional titles/ids and nested `proof of <id>:` bodies. Proof
  targets are validated against the id namespace; adjacency is never used
  to infer which theorem a proof belongs to.
- Figures (caption, plain-text alt, optional `role: wide`) and tables
  (caption, header row, body rows — rectangular, no spans).
- Restricted math (`math:` blocks and inline `\(...\)`) against a closed,
  versioned command/environment allowlist — see `crates/terse-core/src/
  syntax/math.rs`. Execution primitives (`\input`, `\write18`, `\csname`,
  `\def`, `^^` byte encoding, etc.) are rejected at parse time, not at
  compile time.
- Dollar math, validated the same way. Inline `$...$` is equivalent to
  `\(...\)`. A `$` opens math only when the next character is not a
  space, and it closes at the next unescaped `$`, provided that one does
  not follow a space and is not followed by a digit. Otherwise the `$` is
  literal text, so `It costs $5 to $10` stays prose; write `\$` to force
  a literal dollar. Display `$$ ... $$` must start its own line; it may
  close on the same line or on a later one, and it ends a paragraph
  running above it. It cannot contain a blank line or be empty, since
  TeX ends display math at a paragraph break. `$$` equations are
  unnumbered (`\[...\]`) and take no id. Use `math [id: ...]:` when an
  equation needs a number or a `{ref: ...}`. As in Pandoc, a price and a
  later `$` in the same paragraph pair up even when that `$` sits inside
  a link URL: `Costs $5 [see](https://x.com/a$b)` turns the text between
  them into math. Write the price as `\$5` in that case.
- Explicit raw TeX (`tex:` blocks) for anything outside the restricted
  subset. Raw blocks always emit `W-TEX-001`; `check --deny-warnings`
  turns that into a build failure. Declared support files/packages (e.g.
  `tikz`) are validated and copied deterministically.
- Fenced code blocks: a line of three or more backticks, an optional
  language tag (```` ```python ````), the code, and a closing fence of at
  least the same length. Content is opaque, kept byte for byte, and never
  parsed as Terse: `$`, `*`, `[@alias]`, `\input`, and every other
  metacharacter print literally. A known tag maps to a `listings`
  language for highlighting (`python`/`py`, `r`, `sql`, `bash`/`sh`, `c`,
  `cpp`/`c++`, `java`, `matlab`, `octave`, `html`, `xml`, `go`, `haskell`,
  `ruby`, `perl`, `php`, `scala`, `swift`, `lua`, `fortran`,
  `tex`/`latex`, `make`/`makefile`); an unrecognized or absent tag still
  renders, just without highlighting — never an error. A content line
  containing `\end{TerseCode}` (spaces inside the braces tolerated) is
  rejected, since that sequence would close the rendering environment
  early and run the rest of the block as live LaTeX. Code blocks are
  accepted at module scope, in theorem/proof bodies, and inside list
  items — the one non-paragraph, non-list construct a list item accepts.
  No shell escape is required or used; the code is never executed.

Inside any opaque payload (`math:`/`tex:` bodies, `$$` displays, and code
blocks), only the block's own required structural prefix must be plain
spaces; everything past it — tabs, odd indentation, a Python continuation
aligned under a parenthesis, a Makefile recipe line — is kept as literal
content, never checked against the two-space structural rule.

## Multi-file projects

`include: path/to/file.trs` expands per occurrence (not per parse) in
authored order. Cycles and missing targets fail with the full route from
the entry. IDs and reference aliases live in one project-wide, case-
sensitive namespace; a duplicate reports both origins, including the
include route to each.

## Inline syntax

Emphasis/strong (delimiter-aware, no silent crossing), inline code (backtick
runs), links, footnotes, and citations (`@alias`, narrative or
parenthetical, with per-work locators like `@alias[p. 12]`).

## Resource and diagnostic limits

Versioned, documented bounds prevent runaway parses/resolutions from
producing a partial or ambiguous plan:

- Maximum structural indentation depth: 64 (`E-LIMIT-001`,
  `crates/terse-core/src/syntax/blocks.rs`).
- Maximum diagnostics per `check`/`build` before truncation: 20
  (`E-LIMIT-002`, `crates/terse-core/src/syntax/blocks.rs`).

Hitting a limit is always a diagnostic, never a partial or silently
truncated artifact plan.

## Formatting

`terse fmt` is lossless and semantic-preserving: it canonicalizes
structural spacing/indentation and blank-line runs, but never rewraps
prose, reorders metadata/citations, or touches opaque `math:`/`tex:`
payload bytes. `fmt --check` never writes; `fmt` writes atomically per file
and only after every selected file parses successfully.
