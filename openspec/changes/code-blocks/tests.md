## Test Strategy

Tests use Rust `#[test]` through `cargo test`, placed as elsewhere in the repository:

- **Parse and lower:** `crates/terse-core/src/syntax/tests.rs`, through `parse_text` / `try_parse_diagnostics`, asserting on `NodeKind` and on diagnostic spans.
- **Generation:** unit tests in `crates/terse-core/src/latex/mod.rs`, through its `body_of` / `parse_src` helpers and `generate_style`.
- **Formatter:** `crates/terse-core/src/syntax/format_tests.rs`.
- **Profile guard:** `crates/terse-core/src/artifact/profile.rs` (existing `test_profile_packages_equal_emitted_set`).
- **Real engine:** `crates/terse-cli/tests/e2e/latex_generation.rs`, `#[ignore]`d like its neighbors, run with `-- --ignored` against a local XeLaTeX.
- **Pinned closure:** `crates/terse-cli/tests/e2e/toolchain.rs` (existing `test_pinned_closure_covers_full_paper_inputs`), which becomes meaningful once `full-paper` contains a code block.

Four inherited scenarios keep their existing tests, re-run as the regression guard. The lexer change sits on every parse, so the full existing suite, including every `math:`/`tex:`/`$$` byte-preservation test, must stay green unchanged.

Every failure assertion checks the diagnostic code and that its span starts at the offending line (the opener or the content line).

## Spec-to-Test Mapping

### Capability: language-parsing

#### Scenario: Code is kept byte for byte
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_fenced_code_block_is_byte_exact`
- **Setup (GIVEN)**: a document with ```` ```python ````, then lines `\tif x:`, `   aligned = (1,`, `// not a comment`, `$x$ and *y* and [@ref]`, an empty line, `end`, then ```` ``` ````
- **Action (WHEN)**: `parse_text`
- **Assert (THEN)**: exactly one block, `CodeBlock { language: Some("python"), code }`, where `code` equals those six lines joined by `\n`, byte for byte (tab and three spaces intact)
- **Edge cases**: a CRLF variant keeps `\r\n` inside `code`; a whitespace-only content line keeps its spaces; parsing `[@ref]` inside code produces no citation diagnostic even though `ref` is undeclared

#### Scenario: A fence ends a running paragraph
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_fence_ends_running_paragraph`
- **Setup (GIVEN)**: lines `Run this:`, ```` ```bash ````, `make all`, ```` ``` ````, `Then continue.` with no blank lines between them
- **Action (WHEN)**: `parse_text`
- **Assert (THEN)**: three blocks: `Paragraph("Run this:")`, `CodeBlock { language: Some("bash"), code: "make all" }`, `Paragraph("Then continue.")`

#### Scenario: Code block inside a list item
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_code_block_in_list_item`
- **Setup (GIVEN)**: the item line `1. Install:` followed by three continuation lines indented one level (two spaces): ```` ```bash ````, `pip install terse`, ```` ``` ````
- **Action (WHEN)**: `parse_text`
- **Assert (THEN)**: the list item's continuation holds `CodeBlock { code: "pip install terse" }` after the item paragraph
- **Edge cases**: the same position holding `math:` still fails with the existing list-item diagnostic; a fence inside a theorem body is accepted and nested under the theorem

#### Scenario: Longer fences and untagged blocks
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_long_fences_and_untagged_blocks`
- **Setup (GIVEN)**: a four-backtick fence containing a ```` ``` ```` line; a three-backtick fence with no info string; a fence with info string `python extra words`
- **Action (WHEN)**: `parse_text`
- **Assert (THEN)**: the first block's code contains the ```` ``` ```` line; the second has `language: None`; the third has `language: Some("python")`

#### Scenario: Backticks in running prose stay prose
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_inline_triple_backticks_stay_prose`
- **Setup (GIVEN)**: a paragraph line ```` ```x``` is inline code ````
- **Action (WHEN)**: `parse_text`
- **Assert (THEN)**: one `Paragraph` whose inlines contain `Code("x")`, and no `CodeBlock`

#### Scenario: Malformed code blocks
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_malformed_code_blocks_fail`
- **Setup (GIVEN)**: (a) ```` ```python ```` then `x = 1` then EOF; (b) a block whose content line is `print("\end{TerseCode}")`; (c) the same with `\end {TerseCode}`
- **Action (WHEN)**: `try_parse_diagnostics`
- **Assert (THEN)**: each fails with `E-PARSE-002`; the span starts at the opener for (a) and at the content line for (b) and (c); messages name an unterminated block or the forbidden sequence
- **Edge cases**: a content line `\end{lstlisting}` is accepted (harmless under `TerseCode`)

#### Scenario: Equivalent line endings (existing)
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_line_endings_preserve_semantics` (existing, unchanged)
- **Setup (GIVEN)**: existing
- **Action (WHEN)**: existing
- **Assert (THEN)**: existing assertions still pass

#### Scenario: Invalid indentation (existing)
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_invalid_structural_indentation` (existing, unchanged)
- **Setup (GIVEN)**: existing
- **Action (WHEN)**: existing
- **Assert (THEN)**: tabs, three-space indents, and bad dedents on structural lines still fail at that line

#### Scenario: Opaque payload lines are not structural
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_opaque_payload_lines_are_not_structural`
- **Setup (GIVEN)**: `math:` with payload lines `  a = b` and `     + c` (five spaces); `tex:` with a line `  \t\draw;`; `$$`, `x =`, `   y`, `$$`; a fence containing `\tindented`; each followed by a paragraph
- **Action (WHEN)**: `parse_text`
- **Assert (THEN)**: parsing succeeds; the `math:` payload is `a = b\n   + c`, the `tex:` payload is `\t\draw;`, the `$$` payload is `x =\n   y`, the code is `\tindented`, and each following paragraph is intact
- **Edge cases**: a paragraph line after a block indented by three spaces still fails with `E-PARSE-011`; a malformed header `math: nope` followed by a three-space line still fails (it opens no region); `test_tex_math_bytes_are_preserved` and the `$$` tests pass unchanged

#### Scenario: Nested lists and three heading levels (existing)
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_headings_and_nested_lists` (existing, unchanged)
- **Setup (GIVEN)**: existing
- **Action (WHEN)**: existing
- **Assert (THEN)**: existing assertions still pass

#### Scenario: Nonsequential ordered markers (existing)
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_ordered_markers_are_sequential` (existing, unchanged)
- **Setup (GIVEN)**: existing
- **Action (WHEN)**: existing
- **Assert (THEN)**: existing assertions still pass

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/format_tests.rs`
- **Test name**: `test_format_preserves_code_blocks`
- **Setup (GIVEN)**: a source whose code block contains trailing spaces, a tab, two consecutive blank lines, and CRLF line endings
- **Action (WHEN)**: `format_source`, then again on its output
- **Assert (THEN)**: the code block bytes are unchanged (trailing spaces, tab, both blank lines, CRLF), while structural whitespace outside the block is canonicalized; the second pass is byte-identical

### Capability: latex-generation

#### Scenario: Tagged Python block
- **Test type**: unit
- **Test file**: `crates/terse-core/src/latex/mod.rs`
- **Test name**: `test_python_code_block_emits_terse_code`
- **Setup (GIVEN)**: a document with a ```` ```python ```` block containing `def f(x):` and `    return x`
- **Action (WHEN)**: generate the body and `generate_style` for the plain default theme
- **Assert (THEN)**: the body contains `\begin{TerseCode}[language=Python]\ndef f(x):\n    return x\n\end{TerseCode}`; the style contains `\RequirePackage{listings}`, the `\lstset` with `keywordstyle=\bfseries` and `commentstyle=\itshape` and no `\color`, and `\lstnewenvironment{TerseCode}`
- **Edge cases**: `py` maps to `Python`; `C++` maps to `C++`; the style is identical for documents with and without code blocks

#### Scenario: Unknown or missing tag
- **Test type**: unit
- **Test file**: `crates/terse-core/src/latex/mod.rs`
- **Test name**: `test_unknown_code_tags_emit_plain_environment`
- **Setup (GIVEN)**: three blocks tagged `rust`, `x]{evil}`, and untagged
- **Action (WHEN)**: generate the body
- **Assert (THEN)**: each is emitted as `\begin{TerseCode}\n` with no `[`; the string `evil` does not appear outside the code content

#### Scenario: Code is inert in the PDF
- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/latex_generation.rs`
- **Test name**: `test_code_block_is_inert_in_pdf`
- **Setup (GIVEN)**: a project whose `paper.trs` has a Python block with `\input{secret.txt}`, `\write18{touch PWNED}`, `^^5cinput`, and `café = "ação"`, and a `secret.txt` containing `SECRET-MARKER` next to it
- **Action (WHEN)**: `terse build --require-pdf`, then extract the PDF text
- **Assert (THEN)**: the build succeeds; the PDF text contains the three sequences literally and `café`/`ação`; it does not contain `SECRET-MARKER`; no `PWNED` file exists in the project or build directory

#### Scenario: Package set stays closed
- **Test type**: unit
- **Test file**: `crates/terse-core/src/artifact/profile.rs`
- **Test name**: `test_profile_packages_equal_emitted_set` (existing; fails until `listings` is in both lists)
- **Setup (GIVEN)**: `STYLE_PACKAGES` and the embedded export profile
- **Action (WHEN)**: compare the sets
- **Assert (THEN)**: they are equal and include `listings`

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/toolchain.rs`
- **Test name**: `test_pinned_closure_covers_full_paper_inputs` (existing)
- **Setup (GIVEN)**: `tests/fixtures/full-paper` containing a Python code block, and the re-derived closure
- **Action (WHEN)**: compile the fixture with the recorder under the managed prefix
- **Assert (THEN)**: every recorded input is owned by a closure package, including `listings.sty` and the Python language file

## Coverage Summary

| Capability | Scenario | Test file | Test name | Type |
|------------|----------|-----------|-----------|------|
| language-parsing | Code is kept byte for byte | `syntax/tests.rs` | `test_fenced_code_block_is_byte_exact` | unit |
| language-parsing | A fence ends a running paragraph | `syntax/tests.rs` | `test_fence_ends_running_paragraph` | unit |
| language-parsing | Code block inside a list item | `syntax/tests.rs` | `test_code_block_in_list_item` | unit |
| language-parsing | Longer fences and untagged blocks | `syntax/tests.rs` | `test_long_fences_and_untagged_blocks` | unit |
| language-parsing | Backticks in running prose stay prose | `syntax/tests.rs` | `test_inline_triple_backticks_stay_prose` | unit |
| language-parsing | Malformed code blocks | `syntax/tests.rs` | `test_malformed_code_blocks_fail` | unit |
| language-parsing | Equivalent line endings | `syntax/tests.rs` | `test_line_endings_preserve_semantics` | unit |
| language-parsing | Invalid indentation | `syntax/tests.rs` | `test_invalid_structural_indentation` | unit |
| language-parsing | Opaque payload lines are not structural | `syntax/tests.rs`, `syntax/format_tests.rs` | `test_opaque_payload_lines_are_not_structural`, `test_format_preserves_code_blocks` | unit |
| language-parsing | Nested lists and three heading levels | `syntax/tests.rs` | `test_headings_and_nested_lists` | unit |
| language-parsing | Nonsequential ordered markers | `syntax/tests.rs` | `test_ordered_markers_are_sequential` | unit |
| latex-generation | Tagged Python block | `latex/mod.rs` | `test_python_code_block_emits_terse_code` | unit |
| latex-generation | Unknown or missing tag | `latex/mod.rs` | `test_unknown_code_tags_emit_plain_environment` | unit |
| latex-generation | Code is inert in the PDF | `terse-cli/tests/e2e/latex_generation.rs` | `test_code_block_is_inert_in_pdf` | e2e |
| latex-generation | Package set stays closed | `artifact/profile.rs`, `terse-cli/tests/e2e/toolchain.rs` | `test_profile_packages_equal_emitted_set`, `test_pinned_closure_covers_full_paper_inputs` | unit, e2e |

Paths without a crate prefix are under `crates/terse-core/src/`.
