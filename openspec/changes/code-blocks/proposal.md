## Why

Terse has inline code but no code block. A snippet of Python in a paper today has to go through raw `tex:`, which is opaque, is flagged `W-TEX-001` in strict checks, is rejected by arXiv export, and is written in LaTeX rather than in the document's own language. `compile-markdown` also needs somewhere to put fenced code, the construct most likely to appear in a real `.md` file. A native code block gives `.trs` the fence authors already type, and gives Markdown a node to map onto, as `dollar-math-delimiters` and `pipe-tables` do for math and tables.

## What Changes

- `.trs` accepts fenced code blocks: a line starting with three or more backticks, an optional language tag (```` ```python ````), the code lines, and a closing fence of at least the same length. The content is opaque: kept byte for byte after structural dedent, never parsed as Terse, never escaped.
- A code block is emitted with `listings` (`\begin{lstlisting}[language=Python]`), so highlighting happens inside LaTeX. The generated `.tex` keeps the code exactly as written, no Rust dependency is added, and XeLaTeX runs with shell escape disabled as the spec requires. Checked on TeX Live 2026 XeLaTeX with `-no-shell-escape`: a Python block with accented identifiers, comments, docstrings, and strings compiles with no warning and no missing character.
- Language tags map to `listings` dialects (e.g. `python` → `Python`, `sh`/`bash` → `bash`, `r` → `R`, `sql` → `SQL`). A block with no tag, or with a language `listings` does not know (Rust, JavaScript, TypeScript, JSON, and YAML are not in its set), is still rendered in monospace, just without highlighting. An unknown tag is not an error.
- The default presentation (no theme) is plain and monochrome: monospace text, keywords bold, comments italic, no color. It follows the plain-`article` default.
- A code line that would end the environment early (`\end{lstlisting}`) is rejected with a located diagnostic, since `listings` would otherwise close the block there.
- **Opaque payloads accept tabs and any indentation.** Today the lexer rejects the whole file on a leading tab or on indentation that is not a multiple of two spaces, even inside `math:` or `$$` payloads (`E-PARSE-011`; verified during the `dollar-math-delimiters` verification). Real code needs both: Makefile tabs, Python continuation lines aligned under an opening parenthesis. Lines inside a code block, `math:`, `tex:`, or `$$` payload stop being structural lines. This also closes the inherited `math:` limitation recorded in the base spec's own wording ("leading tabs outside opaque payloads", `openspec/specs/language-parsing/spec.md:9`).
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

- `language-parsing`: new fenced code block syntax, and the UTF-8/structural-whitespace requirement exempting opaque payload lines (code, `math:`, `tex:`, `$$`) from structural indentation rules.
- `latex-generation`: code blocks are emitted through `listings` with the language mapping and plain default style, and the escaping requirement covers the one sequence that must be rejected.
- `semantic-ast`: the model represents code blocks (language tag plus opaque content).
- `arxiv-export` and `toolchain`: the verified package set and the pinned closure include `listings`.

## Impact

- `crates/terse-core/src/syntax/lexer.rs`: opaque-region awareness (the lexer, or a pass before it, must know where fences and payload blocks begin and end).
- `crates/terse-core/src/syntax/blocks.rs`: fence parsing; `consume_paragraph` stops at a fence line.
- Semantic model, projection, and `crates/terse-core/src/latex/mod.rs`: new `CodeBlock` node and its emission; `\lstset` in the style layer.
- Toolchain and export profiles, the closure re-derivation, and a code block in `tests/fixtures/full-paper`.
- `docs/language.md`.
- `compile-markdown` depends on this change: fenced and indented Markdown code blocks map onto the same node.

## Open for design

- **Code blocks in list items.** Markdown puts code inside list items often, but `.trs` list items accept only paragraphs and lists (`openspec/specs/language-parsing/spec.md:80`). Allowing code there widens that requirement. Rejecting keeps it, at the cost of some Markdown files.
- **Lexer strategy.** The lexer could learn fences and payload headers itself, or a pre-pass could mark opaque line ranges before structural lexing. Either way, `terse fmt` must keep treating those ranges as byte-exact.
- **Font for code.** Under the plain default the monospace is Latin Modern Typewriter, which rendered all tested characters. A theme's main font does not change it.
