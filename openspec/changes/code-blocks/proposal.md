## Why

Terse has inline code but no code block. A snippet of Python in a paper today has to go through raw `tex:`, which is opaque, is flagged `W-TEX-001` in strict checks, is rejected by arXiv export, and is written in LaTeX rather than in the document's own language. `compile-markdown` also needs somewhere to put fenced code, the construct most likely to appear in a real `.md` file. A native code block gives `.trs` the fence authors already type, and gives Markdown a node to map onto, as `dollar-math-delimiters` and `pipe-tables` do for math and tables.

## What Changes

- `.trs` accepts fenced code blocks: a line starting with three or more backticks, an optional language tag (```` ```python ````), the code lines, and a closing fence of at least the same length. The content is opaque: kept byte for byte after structural dedent, never parsed as Terse, never escaped.
- A code block is emitted through `listings`, inside a semantic environment the style defines with `\lstnewenvironment` (`\begin{TerseCode}[language=Python]`), so highlighting happens inside LaTeX. The generated `.tex` keeps the code exactly as written, no Rust dependency is added, and XeLaTeX runs with shell escape disabled as the spec requires. Checked on TeX Live 2026 XeLaTeX with `-no-shell-escape`: a Python block with accented identifiers, comments, docstrings, and strings compiles with no warning and no missing character.
- Language tags map to `listings` dialects (e.g. `python` → `Python`, `sh`/`bash` → `bash`, `r` → `R`, `sql` → `SQL`). A block with no tag, or with a language `listings` does not know (Rust, JavaScript, TypeScript, JSON, and YAML are not in its set), is still rendered in monospace, just without highlighting. An unknown tag is not an error.
- The default presentation (no theme) is plain and monochrome: monospace text, keywords bold, comments italic, no color. It follows the plain-`article` default.
- A code line containing the environment's end sequence (`\end{TerseCode}`, with or without inner spaces) is rejected with a located diagnostic. Tested: `listings` closes the block at that sequence even mid-line, and whatever follows it becomes live LaTeX, so `\end{TerseCode}\input{...}` would otherwise read a local file. Everything else in the code is inert: `\input`, `\write18`, and `^^5c` inside a block print literally.
- **Opaque payloads accept tabs and any indentation.** Today the lexer rejects the whole file on a leading tab or on indentation that is not a multiple of two spaces, even inside `math:` or `$$` payloads (`E-PARSE-011`; verified during the `dollar-math-delimiters` verification). Real code needs both: Makefile tabs, Python continuation lines aligned under an opening parenthesis. Lines inside a code block, `math:`, `tex:`, or `$$` payload stop being structural lines. This also closes the inherited `math:` limitation recorded in the base spec's own wording ("leading tabs outside opaque payloads", `openspec/specs/language-parsing/spec.md:9`).
- Code blocks are accepted in list items (decided with the author, 2026-09-23), the common Markdown case of installation steps. List items still reject every other block kind (`openspec/specs/language-parsing/spec.md:80`).
- `listings` joins the closed package set: `STYLE_PACKAGES` (`crates/terse-core/src/latex/mod.rs:451`), the export profile `texlive-2025-xelatex`, and the managed toolchain closure (`crates/terse-core/profiles/toolchain-texlive-2025.toml`). The closure is re-derived with `scripts/derive-toolchain-closure.sh` after a code block is added to the `full-paper` fixture. None of these contains `listings` today.

### Non-goals

- Executing code (no notebook or knitr behavior; running document code contradicts Terse's no-executable-content rule).
- Theme styling of code blocks. The theme schema has no code component today (`crates/terse-core/src/theme/resolve.rs:52-68`), so color schemes for code are a later change.
- Captions, ids, line numbers, and line highlighting on code blocks.
- Highlighting for languages `listings` does not know.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `language-parsing`: adds the fenced code block requirement, and modifies "UTF-8 source and consistent structural whitespace" (opaque payload lines stop being structural) and "Headings and ordered or unordered lists" (items may contain code blocks).
- `latex-generation`: adds a requirement for rendering code blocks through `listings`.

`arxiv-export` and `toolchain` need no requirement text change: "Profile packages equal the emitted set" and the pinned closure already cover any package the generator emits. Adding `listings` there is a data change, guarded by `test_profile_packages_equal_emitted_set`.

## Impact

- `crates/terse-core/src/syntax/lexer.rs`: opaque-region awareness (the lexer, or a pass before it, must know where fences and payload blocks begin and end).
- `crates/terse-core/src/syntax/blocks.rs`: fence parsing; `consume_paragraph` stops at a fence line.
- Semantic model, projection, and `crates/terse-core/src/latex/mod.rs`: new `CodeBlock` node and its emission; `\lstset` in the style layer.
- Toolchain and export profiles, the closure re-derivation, and a code block in `tests/fixtures/full-paper`.
- `docs/language.md`.
- `compile-markdown` depends on this change: fenced and indented Markdown code blocks map onto the same node.
