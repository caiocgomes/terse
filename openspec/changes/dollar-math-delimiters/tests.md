## Test Strategy

Rust `#[test]` via `cargo test`, following the existing layout:

- inline-parser unit tests in the `tests` module of `crates/terse-core/src/syntax/inlines.rs`;
- parse-and-lower tests in `crates/terse-core/src/syntax/tests.rs`, using its `parse_text` helper (lex, block parse, `semantic::lower`) and asserting on `NodeKind`;
- generation unit tests in the `tests` module of `crates/terse-core/src/latex/mod.rs`, asserting on the generated `.tex` string;
- formatter tests in `crates/terse-core/src/syntax/format_tests.rs`;
- projection tests in `crates/terse-core/src/semantic/tests.rs`;
- one compile test against a real engine in `crates/terse-cli/tests/e2e/latex_generation.rs`, gated the same way the other tests in that file are.

Four scenarios in the modified requirements already exist and keep their current tests. They are re-run unchanged as the regression guard for the `\(...\)` and `math:` paths.

Block-level failures (`$$` blocks) must carry a span that points at the offending line. Inline failures carry the paragraph span with `E-PARSE-050`, because `InlineError` has no position (`crates/terse-core/src/semantic/mod.rs:799`). That is the existing behavior for every inline error and is not changed here.

## Spec-to-Test Mapping

### Capability: language-parsing

#### Scenario: Mixed inline content (existing)
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_mixed_inline_paragraph` (existing, unchanged)
- **Setup (GIVEN)**: existing fixture
- **Action (WHEN)**: existing
- **Assert (THEN)**: existing assertions still pass
- **Edge cases**: none added

#### Scenario: Ambiguous delimiters are rejected (existing)
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`, `crates/terse-core/src/syntax/inlines.rs`
- **Test name**: `test_crossing_and_nested_delimiters_fail`, `test_crossing_delimiters_fail`, `test_triple_asterisk_rejected` (existing, unchanged)
- **Setup (GIVEN)**: existing fixtures
- **Action (WHEN)**: existing
- **Assert (THEN)**: existing assertions still pass
- **Edge cases**: none added

#### Scenario: Dollar inline math matches backslash-paren math
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/inlines.rs`
- **Test name**: `test_dollar_inline_math_equals_paren_math`
- **Setup (GIVEN)**: the strings `x $\frac{a}{b}$ y` and `x \(\frac{a}{b}\) y`
- **Action (WHEN)**: `parse_inline` on each
- **Assert (THEN)**: both return `[Text("x "), Math("\frac{a}{b}"), Text(" y")]`, and the two vectors are equal
- **Edge cases**: `$x$` at the very start and very end of the string; two inline maths in one paragraph (`$a$ and $b$`); math adjacent to punctuation (`($a$).`)

- **Test type**: unit
- **Test file**: `crates/terse-core/src/latex/mod.rs`
- **Test name**: `test_dollar_inline_math_generates_like_paren_math`
- **Setup (GIVEN)**: two documents that differ only in `$\frac{a}{b}$` versus `\(\frac{a}{b}\)`
- **Action (WHEN)**: generate `.tex` for both
- **Assert (THEN)**: the generated bodies are byte-identical

#### Scenario: Currency stays text
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/inlines.rs`
- **Test name**: `test_currency_dollars_stay_text`
- **Setup (GIVEN)**: `It costs $5 to $10 per unit.`, `Pay $ 5 now.`, `A lone $ sign.`
- **Action (WHEN)**: `parse_inline` on each
- **Assert (THEN)**: no `Inline::Math` in any result; the concatenated text equals the input
- **Edge cases**: `$5$10` (closer followed by a digit, so there is no math); `a$b` with no partner; `costs $5 to $10, inline $y^2$.` yields only `Math("y^2")` (scenario "Prices before math do not pair with it")

- **Test type**: unit
- **Test file**: `crates/terse-core/src/latex/mod.rs`
- **Test name**: `test_currency_dollars_are_escaped_in_output`
- **Setup (GIVEN)**: a document whose paragraph is `It costs $5 to $10 per unit.`
- **Action (WHEN)**: generate `.tex`
- **Assert (THEN)**: the body contains `It costs \$5 to \$10 per unit.` and no `\(`

#### Scenario: Escaped dollars
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/inlines.rs`
- **Test name**: `test_escaped_dollars`
- **Setup (GIVEN)**: `price \$5` and `math $a \$ b$ here`
- **Action (WHEN)**: `parse_inline` on each
- **Assert (THEN)**: the first is a single `Text("price $5")` with no math; the second contains `Math("a \$ b")`
- **Edge cases**: `\$5 and $x$` (an escaped dollar before real math does not pair with it)

#### Scenario: Dollar math is validated
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_dollar_math_rejects_execution`
- **Setup (GIVEN)**: a document whose paragraph contains `$\input{evil}$`
- **Action (WHEN)**: lex, block parse, and `semantic::lower` (the steps of `parse_text`, expecting an error)
- **Assert (THEN)**: lowering fails with the same diagnostic code as the existing `\(\input{evil}\)` case in `test_math_rejects_execution`
- **Edge cases**: `$\write18{x}$`; a nested `$\frac{\csname x\endcsname}{2}$`

#### Scenario: Mid-line display dollars are rejected
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/inlines.rs`
- **Test name**: `test_midline_display_dollars_rejected`
- **Setup (GIVEN)**: `where $$x$$ holds`
- **Action (WHEN)**: `parse_inline`
- **Assert (THEN)**: an `InlineError` whose message mentions putting display math on its own line (surfaced as `E-PARSE-050` on the paragraph span)
- **Edge cases**: `\$$x` stays text (escaped first dollar, then a lone `$`)

#### Scenario: Theorem and proof from the authoring model (existing)
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_theorem_proof_equation_structure` (existing; the assertion on `NodeKind::Equation` gains `numbered: true`)
- **Setup (GIVEN)**: existing fixture
- **Action (WHEN)**: existing
- **Assert (THEN)**: existing assertions, plus the `math:` equation reports `numbered: true`
- **Edge cases**: none added

#### Scenario: Unsupported nested structure (existing)
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_nested_module_structures_fail` (existing, unchanged)
- **Setup (GIVEN)**: existing fixture
- **Action (WHEN)**: existing
- **Assert (THEN)**: existing assertions still pass
- **Edge cases**: none added

#### Scenario: Multi-line dollar display splits a paragraph
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_multiline_dollar_display_splits_paragraph`
- **Setup (GIVEN)**: a document body with the lines `where`, `$$`, `a = b +`, `  c`, `$$`, `holds.`, with no blank lines between them
- **Action (WHEN)**: `parse_text`
- **Assert (THEN)**: exactly three blocks: `Paragraph(where)`, `Equation { id: None, numbered: false, payload: "a = b +\n  c" }`, and `Paragraph(holds.)`
- **Edge cases**: CRLF input keeps CRLF inside the payload (mirrors `test_line_endings_preserve_semantics`); a `$$` block inside a theorem body is accepted and nested under the theorem

#### Scenario: Single-line dollar display
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_single_line_dollar_display`
- **Setup (GIVEN)**: a line `$$ E = mc^2 $$`
- **Action (WHEN)**: `parse_text`
- **Assert (THEN)**: one `Equation { id: None, numbered: false, payload: " E = mc^2 " }`
- **Edge cases**: the opening line `$$ a = b +` followed by `c $$` produces the payload `" a = b +\nc "`

- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/format_tests.rs`
- **Test name**: `test_format_preserves_dollar_math`
- **Setup (GIVEN)**: a source with inline `$x$`, a single-line `$$`, and a multi-line `$$` block whose payload has irregular internal indentation
- **Action (WHEN)**: `format_source`, then `format_source` again
- **Assert (THEN)**: the dollar delimiters are not converted to `\(` or `math:`; the payload bytes are unchanged; the second pass is byte-identical to the first

#### Scenario: Dollar display does not consume a number
- **Test type**: unit
- **Test file**: `crates/terse-core/src/latex/mod.rs`
- **Test name**: `test_unnumbered_dollar_display_emits_brackets`
- **Setup (GIVEN)**: `math [id: eq-a]:`, a `$$ x $$` line, and `math [id: eq-b]:`
- **Action (WHEN)**: generate `.tex`
- **Assert (THEN)**: the body contains `\[\n x \n\]` between the two `\begin{TerseEquation}\label{...}` blocks, and no `\begin{TerseEquation}` wraps the `$$` payload

- **Test type**: unit
- **Test file**: `crates/terse-core/src/semantic/tests.rs`
- **Test name**: `test_projection_distinguishes_numbered_equations`
- **Setup (GIVEN)**: two documents with the same payload, one as `math:` and one as `$$`
- **Action (WHEN)**: project both
- **Assert (THEN)**: the projected equations differ only in `numbered`, and the digests differ

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/latex_generation.rs`
- **Test name**: `test_dollar_display_does_not_consume_equation_number`
- **Setup (GIVEN)**: the same three equations plus a paragraph `{ref: eq-a} and {ref: eq-b}`
- **Action (WHEN)**: `terse build` with PDF, then extract the PDF text as the neighboring tests do
- **Assert (THEN)**: the references render as `1` and `2`, and no `(3)` appears

#### Scenario: Blank line or empty dollar display
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_blank_or_empty_dollar_display_fails`
- **Setup (GIVEN)**: `$$`, `x = 1`, blank, `$$` (trailing blank); `$$`, blank, `x = 1`, `$$` (leading blank); `$$ $$`; `$$$$`; `$$` then `$$`
- **Action (WHEN)**: lex and block parse each
- **Assert (THEN)**: each fails; blank-line cases name a blank line and point at its byte offset; empty cases name an empty display and point at the opening `$$`
- **Edge cases**: the trailing-blank case is the one `math:` accepts (it drops trailing blank lines), so it must fail here rather than in the engine

#### Scenario: Malformed dollar display
- **Test type**: unit
- **Test file**: `crates/terse-core/src/syntax/tests.rs`
- **Test name**: `test_malformed_dollar_display_fails`
- **Setup (GIVEN)**: three sources: `$$` then `x` then EOF; `$$ x $$ trailing`; a list item `- item` whose continuation line is `$$ x $$`
- **Action (WHEN)**: lex and block parse each
- **Assert (THEN)**: each fails; the span points at the opening `$$`, the trailing text, and the `$$` inside the item respectively
- **Edge cases**: a paragraph line `\$$ signs are fine` parses as prose starting with a literal `$`

## Coverage Summary

| Capability | Scenario | Test file | Test name | Type |
|------------|----------|-----------|-----------|------|
| language-parsing | Mixed inline content | `syntax/tests.rs` | `test_mixed_inline_paragraph` | unit |
| language-parsing | Ambiguous delimiters are rejected | `syntax/tests.rs`, `syntax/inlines.rs` | `test_crossing_and_nested_delimiters_fail`, `test_crossing_delimiters_fail`, `test_triple_asterisk_rejected` | unit |
| language-parsing | Dollar inline math matches backslash-paren math | `syntax/inlines.rs`, `latex/mod.rs` | `test_dollar_inline_math_equals_paren_math`, `test_dollar_inline_math_generates_like_paren_math` | unit |
| language-parsing | Currency stays text | `syntax/inlines.rs`, `latex/mod.rs` | `test_currency_dollars_stay_text`, `test_currency_dollars_are_escaped_in_output` | unit |
| language-parsing | Prices before math do not pair with it | `syntax/inlines.rs` | `test_currency_dollars_stay_text` | unit |
| language-parsing | Escaped dollars | `syntax/inlines.rs` | `test_escaped_dollars` | unit |
| language-parsing | Dollar math is validated | `syntax/tests.rs` | `test_dollar_math_rejects_execution` | unit |
| language-parsing | Mid-line display dollars are rejected | `syntax/inlines.rs` | `test_midline_display_dollars_rejected` | unit |
| language-parsing | Theorem and proof from the authoring model | `syntax/tests.rs` | `test_theorem_proof_equation_structure` | unit |
| language-parsing | Unsupported nested structure | `syntax/tests.rs` | `test_nested_module_structures_fail` | unit |
| language-parsing | Multi-line dollar display splits a paragraph | `syntax/tests.rs` | `test_multiline_dollar_display_splits_paragraph` | unit |
| language-parsing | Single-line dollar display | `syntax/tests.rs`, `syntax/format_tests.rs` | `test_single_line_dollar_display`, `test_format_preserves_dollar_math` | unit |
| language-parsing | Dollar display does not consume a number | `latex/mod.rs`, `semantic/tests.rs`, `terse-cli/tests/e2e/latex_generation.rs` | `test_unnumbered_dollar_display_emits_brackets`, `test_projection_distinguishes_numbered_equations`, `test_dollar_display_does_not_consume_equation_number` | unit, e2e |
| language-parsing | Blank line or empty dollar display | `syntax/tests.rs` | `test_blank_or_empty_dollar_display_fails` | unit |
| language-parsing | Malformed dollar display | `syntax/tests.rs` | `test_malformed_dollar_display_fails` | unit |

All paths are relative to `crates/terse-core/src/` unless shown otherwise.
