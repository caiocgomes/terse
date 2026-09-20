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
- Explicit raw TeX (`tex:` blocks) for anything outside the restricted
  subset. Raw blocks always emit `W-TEX-001`; `check --deny-warnings`
  turns that into a build failure. Declared support files/packages (e.g.
  `tikz`) are validated and copied deterministically.

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
