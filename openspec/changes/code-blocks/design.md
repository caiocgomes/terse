## Context

Terse parses a `.trs` file in two passes: an indentation lexer (`crates/terse-core/src/syntax/lexer.rs`, `lex_lines`) and a block parser (`crates/terse-core/src/syntax/blocks.rs`). The lexer is content-agnostic today. It rejects the whole file on a leading tab (`E-PARSE-010`), on indentation that is not a multiple of two spaces (`E-PARSE-011`), or on a dedent to an unopened level (`E-PARSE-012`), and it does so for every line, including lines the block parser later treats as opaque payload (`math:`, `tex:`, `$$`). The spec already says tabs are only an error "outside opaque payloads" (`openspec/specs/language-parsing/spec.md:9`), so the lexer is stricter than the spec. The `dollar-math-delimiters` verification confirmed it: a `$$` or `math:` payload with three-space alignment fails with `E-PARSE-011`.

Opaque payloads are cut from source bytes by the block parser. `parse_opaque_payload` strips `indent * 2` bytes from each line (`blocks.rs:1257`), and `parse_dollar_display` slices `source[line.byte_start + strip .. line.byte_end]`. Both decide where a payload ends from `line.indent` and `line.is_blank`. The formatter keeps opaque blocks byte-exact by collecting the spans of `Equation` and `RawTex` nodes (`crates/terse-core/src/syntax/format.rs:27`).

The style layer always loads a core package set (`xcolor`, `amsmath`, `amsthm`, `graphicx`, `booktabs`, `enumitem`, `hyperref`; `latex/mod.rs:858-864`) and adds `biblatex` only for citation builds. `STYLE_PACKAGES` (`latex/mod.rs:451`) must equal the export profile's `packages` list (`crates/terse-core/profiles/texlive-2025-xelatex.toml`), enforced by `test_profile_packages_equal_emitted_set`. The managed toolchain's closure (`toolchain-texlive-2025.toml`, `[closure].derived`) is derived by `scripts/derive-toolchain-closure.sh` from recorder output of compiling `tests/fixtures/full-paper`. `listings` is in none of them.

Behavior of `listings` under XeLaTeX with `-no-shell-escape`, measured on TeX Live 2026 in scratch compiles:

- UTF-8 accents in identifiers, comments, docstrings, and strings render correctly with no missing characters and no `extendedchars` option. Tabs expand.
- `\input{...}`, `\write18{...}`, and `^^5cinput` inside a block print literally.
- The environment's end sequence closes the block even mid-line, and the rest of that line and every following line become live LaTeX ("Undefined control sequence", "Extra \endlstlisting"). With a `\lstnewenvironment{TerseCode}` wrapper, the end sequence becomes `\end{TerseCode}`, and a literal `\end{lstlisting}` inside the code is harmless.

## Goals / Non-Goals

**Goals:**

- Fenced code blocks in `.trs`, byte-exact, rendered through `listings` with language-aware highlighting and a monochrome plain default.
- Lines inside any opaque payload (code, `math:`, `tex:`, `$$`) may contain tabs and any indentation beyond their block's prefix.
- Code blocks at module scope, in theorem-like and proof bodies, and in list item continuations.
- `listings` in the closed package set, export profile, and managed closure.

**Non-Goals:**

- Executing code; theme styling of code (the theme schema has no code component); captions, ids, line numbers; highlighting for languages `listings` lacks.
- Tilde fences (`~~~`) in `.trs`. `compile-markdown` handles them through its Markdown parser.
- A fence on a list item's marker line (`- ```python`). In `.trs` the fence starts a continuation line.
- Implementing `//` comment trivia, which the spec mentions but `.trs` does not implement today (a `// ...` line currently becomes prose). Out of scope; it does not affect code content, which is opaque either way.

## Decisions

### D1. The lexer learns opaque regions through a shared recognizer

A new module `crates/terse-core/src/syntax/opaque.rs` exposes one recognizer, used by both the lexer and the block parser so the two cannot disagree:

- `opener(content) -> Option<Opener>`, applied to the structural content of a structural line:
  - `Indented`: the content, trimmed at the end, is exactly `math:`, or matches `math [` … `]:`, or is exactly `tex:`. These are the same shapes `parse_equation` and `parse_raw_tex` accept; a malformed header is not an opener and stays a structural line, so the block parser still reports it as today.
  - `Dollar`: the content starts with `$$` and does not contain a second `$$` after the first two characters (a single-line `$$ x $$` opens nothing).
  - `Fence { len }`: the content starts with a run of at least three backticks, and the rest of the line (the info string) contains no backtick.

`lex_lines` becomes a single pass with a small state machine:

- Outside a region, a line is lexed exactly as today (tab, odd-indent, and dedent checks, stack updates). If its content is an opener, the region starts after it.
- Inside an `Indented` region opened at level `k`, a line is payload if it is blank or begins with at least `2(k+1)` spaces. It is emitted with `indent = k+1` and `content` = the bytes after that prefix, whatever they contain (tabs, odd spaces). It never touches the indentation stack. The first non-blank line with a shorter prefix ends the region and is lexed structurally.
- Inside a `Dollar` or `Fence` region opened at level `k`, a line is payload if it is blank or begins with at least `2k` spaces, and is emitted with `indent = k` and the bytes after that prefix. The line that closes the region ends it: for `Dollar`, the first line containing `$$`; for `Fence`, a line whose content after the prefix is a run of at least `len` backticks followed only by whitespace. A non-blank line with a shorter prefix ends the region without a closer and is lexed structurally, and the block parser then reports the block as unterminated, as it does for `$$` today.

The existing consumers keep working unchanged: `parse_opaque_payload` still strips `indent * 2` from `byte_start`, and `parse_dollar_display` still slices from `byte_start + 2k`.

`StructLine` gains `opaque: bool`, true for payload lines. `parse_module_with_recovery` resumes after an error at "the next line at zero indentation". At module scope every payload line of a fence or `$$` now has indent 0, so without the marker an unterminated fence would restart parsing on each code line and print one spurious diagnostic per line. `$$` already cascades this way today (seen in the `dollar-math-delimiters` verification). Recovery skips opaque lines when looking for its next boundary.

*Alternative considered:* a separate pre-pass that marks opaque line ranges before a still context-free lexer. It is equivalent in behavior, but it adds a second walk and a second place that must agree on region ends. Rejected in favor of one pass using the shared recognizer.

*Alternative considered:* keep the lexer strict and require payload lines to use only two-space multiples. Rejected: Makefile tabs and Python continuation lines aligned under a parenthesis are ordinary code.

### D2. Fence syntax

- **Opener.** A structural line whose content is `Fence { len }` (D1). The info string is trimmed. Its first whitespace-delimited word, if any, is the language tag, kept as written. The rest of the info string is ignored.
- **Closer.** A line at the block's structural indent whose content is at least `len` backticks followed only by whitespace. Content lines keep their bytes exactly after the block's `2k` prefix, including blank lines, trailing whitespace, and CRLF.
- **Prose that starts with backticks.** A line such as ```` ```x``` is code ```` has a backtick in its info string, so it is not a fence and stays prose (inline code), as in CommonMark.
- **Running paragraph.** `consume_paragraph` stops at a fence opener, as it does for `$$`.
- **Unterminated block.** One that reaches EOF, or a shorter prefix, before its closer is `E-PARSE-002` at the opener.
- **Forbidden sequence.** A content line matching `\\end\s*\{\s*TerseCode\s*\}` is `E-PARSE-002` at that line, with a message naming the sequence. No other content is inspected: code is never parsed as Terse (no citations, references, math validation, or escapes).

### D3. Model and placement

`TopBlock::CodeBlock { language: Option<String>, code: String, span }` lowers to `NodeKind::CodeBlock { language, code }` and `ProjectedNode::CodeBlock { language, code }`. The projection keeps the tag as written, because it is authored meaning; which highlighter handles it is a rendering detail. A code block has no id. It is accepted in `BlockContext::Module`, `Nested`, and `ListItem`. It is the only non-paragraph, non-list block a list item accepts.

### D4. Emission through a semantic `TerseCode` environment

- **Style layer, loaded conditionally.** `listings` was going to join the always-loaded core set (like `booktabs`), matching how the style stays independent of content everywhere except the bibliography -- but `listings` is not yet part of any already-deployed managed TeX Live prefix's package closure (this change adds it to the closed set and the export profile, D5, but an already-provisioned prefix only gets the package after a maintainer re-derives and reinstalls that closure, which needs network access, task 4.3). Loading it unconditionally was tried first and reverted after it broke eight pre-existing tests that compile a real PDF and never touch a code block (`test_unicode_text_and_bibliography_compile`, `test_watch_keeps_last_good_pdf_after_syntax_error`, and five more in `latex_generation.rs`) against this machine's already-installed, not-yet-re-derived prefix -- confirmed by `git stash` showing them green on the pre-change commit and red on this one. `generate_style` gains a `has_code: bool` parameter (mirroring `bibliography_language: Option<&str>`), and `crate::semantic::has_code_block` (mirroring `has_bibliography`) supplies it from the module at the one production call site (`artifact/mod.rs`). The style adds, only when the document has a code block:

  ```latex
  \RequirePackage{listings}
  \lstset{basicstyle=\ttfamily, keywordstyle=\bfseries, commentstyle=\itshape,
    columns=fullflexible, keepspaces=true, showstringspaces=false,
    upquote=true, breaklines=true}
  \lstnewenvironment{TerseCode}[1][]{\lstset{#1}}{}
  ```

  This is the tested setting set. It uses no color and no size change (like `verbatim`), and long lines wrap instead of running into the margin. `check_requirements` (`artifact/profile.rs`, the arXiv export gate) already only validates explicitly-declared `[latex] packages` plus font/babel, never the always-vs-conditional core set at runtime, so conditional loading needed no change there; `STYLE_PACKAGES` already lists conditionally-emitted members (`babel`, `biblatex`), so `listings` sitting beside them is the established pattern, not a new one.
- **Body.** `\begin{TerseCode}[language=<Name>]`, the code bytes, then `\end{TerseCode}`. It is plain `\begin{TerseCode}` when the tag is absent or unknown.
- **Language map.** Closed: a lower-cased tag → a `listings` name, containing only names present in `lstlang1/2/3.sty`. For example: `python`/`py` → `Python`, `r` → `R`, `sql` → `SQL`, `bash`/`sh`/`shell`/`zsh` → `bash`, `c` → `C`, `cpp`/`c++` → `C++`, `java` → `Java`, `matlab` → `Matlab`, `octave` → `Octave`, `html` → `HTML`, `xml` → `XML`, `go` → `Go`, `haskell` → `Haskell`, `ruby` → `Ruby`, `perl` → `Perl`, `php` → `PHP`, `scala` → `Scala`, `swift` → `Swift`, `lua` → `Lua`, `fortran` → `Fortran`, `tex`/`latex` → `TeX`, `make`/`makefile` → `make`. `listings` fails hard on an undefined language (tested: `[language=Rust]` stops the compile with "Couldn't load requested language"), so only map values reach LaTeX. The author's tag never does, which also closes an injection path through the option list.

### D5. Package closure

`"listings"` is added to `STYLE_PACKAGES` and to the export profile's `packages`. `tests/fixtures/full-paper` gains a Python code block, so the recorder sees `listings.sty` and its language files. `scripts/derive-toolchain-closure.sh` then regenerates `[closure].derived`. The script needs network access to the pinned repository, and its output is committed as data. The managed prefix installs the new closure the next time the toolchain is installed or updated. The fixture block must land together with the new closure: added alone, against a prefix provisioned from the old closure, it broke 16 tests that reuse the fixture (seen during the implementation).

### D6. `fmt`

`collect_opaque_ranges` (`format.rs:27`) adds `TopBlock::CodeBlock` spans, so code bytes are never touched, including trailing whitespace and CRLF inside the block.

## Risks / Trade-offs

- [The lexer change sits on the path of every parse] $\rightarrow$ The recognizer is shared with the block parser. Tests cover each opener kind at module scope and nested, plus region end by closer, dedent, and EOF. The existing suite (including the `math:`/`tex:`/`$$` byte-preservation tests) is the regression guard.
- [A line like `tex:` inside another block's field text now opens an opaque region] $\rightarrow$ Only exact header shapes open one, and the following lines must be deeper to count as payload. The block parser rejects a stray `tex:` or `math:` header in a non-block context exactly as before.
- [`listings` highlights lexically: builtins such as `list`, `sum`, and `print` are bold keywords] $\rightarrow$ This is the accepted cost of rendering inside LaTeX without a Rust highlighter.
- [`breaklines` wraps long code lines in the PDF, so copied text differs from the source] $\rightarrow$ The source and the `.tex` keep the exact lines, and the alternative is text overflowing the margin.
- [The closure re-derivation needs network access] $\rightarrow$ It is a maintainer step run once. Until it runs, a prefix provisioned from the pinned closure lacks `listings`, while the export profile already declares it, so `terse doctor` fails `packages.resolvable`. The CI `heavy-tex` lane runs `doctor` as preflight and aborts before any e2e test. The tasks were meant to run the re-derivation before the e2e run; the implementation inverted that order, so the change cannot be archived until 4.3 runs.
- [Glyphs missing from Latin Modern Typewriter (CJK, emoji)] $\rightarrow$ The same missing-glyph behavior as prose, not specific to code.

## Migration Plan

No document changes meaning, with one exception: a line whose content is exactly a triple-backtick fence opener now starts a code block. A `grep` over the repository's `.trs` files finds no such line. The closure update is data. Rollback means reverting the change.
