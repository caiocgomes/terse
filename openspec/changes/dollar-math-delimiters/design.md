## Context

Today math enters `.trs` two ways:

- Inline `\(...\)`. The inline parser dispatches on `\(` (`crates/terse-core/src/syntax/inlines.rs:139`) and `parse_math` (`inlines.rs:283`) scans to `\)` and emits `Inline::Math(content)`.
- The `math:` block. The block parser handles it as a reserved word (`crates/terse-core/src/syntax/blocks.rs:405`) and `parse_equation` (`blocks.rs:1036`) returns `TopBlock::Equation { id, payload, span }`, keeping the payload lines after structural dedent.

Validation against the allowlist (`crates/terse-core/src/syntax/math.rs:68`) runs at lowering: `lower_block` for equations and `validate_inline_math` for inline math. Producing the same syntax nodes therefore reuses validation, semantic lowering, and source mapping. Only the unnumbered emission in D3 is new.

Two facts constrain the design:

- Paragraph text is assembled by joining source lines with a single space (`consume_paragraph`, `blocks.rs:607-640`). Anything recognized inside the inline parser has already lost its line breaks. That is harmless for one-line `$...$`, but it would violate the "preserve accepted math bytes" requirement for a display formula written across several lines.
- `TopBlock::Equation` is emitted as `\begin{TerseEquation}`, which is defined as `equation` (`crates/terse-core/src/latex/mod.rs:88-96`, `:871`). Every equation is numbered today, whether or not it has an id.

## Goals / Non-Goals

**Goals:**

- `$...$` in prose behaves exactly like `\(...\)`.
- `$$...$$` behaves as display math and preserves payload bytes, including multi-line formulas.
- Currency in prose stays text without the author having to escape it in the common cases.

**Non-Goals:**

- Changing `\(...\)`, `math:`, the allowlist, or themes. The only change to the semantic model and to emission is the numbered flag in D3.
- Labels on `$$` equations. An author who needs `{ref: eq-x}` keeps using `math [id: eq-x]:`.
- Markdown. `compile-markdown` reuses these rules; this change only touches `.trs`.

## Decisions

### D1. Inline `$` is recognized in the inline parser with the Pandoc boundary rule

In the inline loop, a `$` becomes a math opener when all of the following hold:

- it is not preceded by `\`;
- it is not immediately followed by another `$`;
- the next character is not whitespace;
- the next unescaped `$` in the paragraph text can close it: it is not preceded by whitespace and not followed by an ASCII digit.

Only the next unescaped `$` is a candidate. A later one never closes an earlier opener. TeX itself forbids a bare `$` inside inline math, and a wider search pairs a price with the next real formula: `costs $5 to $10, inline $y^2$` would become one math span from `5` to `y^2`. That bug was caught while building a real PDF during implementation. If all of that holds, the content between the two dollars becomes `Inline::Math` and follows the same path as `\(...\)`. If the candidate does not qualify, the `$` is pushed as literal text, which keeps today's behavior (escaped to `\$` on output). During the scan for a closer, `\$` inside the formula is skipped, so `$a \$ b$` works.

*Alternative considered:* any `$` opens math, and literal dollars must be `\$`. Rejected because it breaks every sentence that contains a price, and it is not what "standard" means for anyone coming from Pandoc or Markdown.

*Alternative considered:* a `$` with no closer is an error. Rejected because a single price in prose is common and legitimate.

### D2. `$$` is a block construct, not an inline one

A line whose content starts with `$$` at a position where a paragraph could start opens display math. Two shapes are accepted:

- single line: `$$ x^2 $$`;
- multi-line: an opening line that starts with `$$`, then payload lines, then a line whose content ends with `$$`. Text after `$$` on the opening line and text before `$$` on the closing line belong to the payload.

The payload keeps its line breaks and internal indentation after structural dedent, exactly as `parse_equation` does. The result is `TopBlock::Equation { id: None, numbered: false, payload, span }`. `$$` is accepted wherever the language already accepts equations (module scope and theorem/proof bodies), and diagnosed where it does not, such as inside list items, exactly like `math:`.

`consume_paragraph` treats a line starting with `$$` as a paragraph terminator, the same way it already treats reserved words and list markers. Markdown-style writing therefore works: `where\n$$\nE = mc^2\n$$\nholds` produces paragraph, equation, paragraph.

A `$$` found by the inline parser, meaning inside a line that does not start with it, is an error: "display math `$$` must start its own line." Non-whitespace text after the closing `$$` on the same line is also an error. An unterminated `$$` block is an error at the opening line.

*Alternative considered:* handling `$$` inside the inline parser, the way Pandoc does. Rejected because paragraph lines have already been joined with spaces by that point, so a multi-line formula would lose its bytes. It would also need a new inline-level display node to split a paragraph.

### D3. `$$` equations are unnumbered and emitted as `\[...\]`

In LaTeX and in Markdown, `$$` means an unnumbered display. Every `math:` block is numbered today, so `$$` cannot simply reuse the node as it stands. `TopBlock::Equation`, `NodeKind::Equation`, and `ProjectedNode::Equation` gain a `numbered: bool`. `math:` sets it to `true` and `$$` sets it to `false`. The generator emits unnumbered equations as `\[` + payload + `\]` on their own lines. That is plain LaTeX, needs no package, and is what a reader of the generated `.tex` expects. The flag belongs in the projection because numbering is authored meaning: it changes the numbers every later equation gets.

*Alternative considered:* numbering `$$` like `math:` so that no node changes. Rejected because an author pasting `$$` would see a "(1)" they did not ask for, which defeats the point of the change.

*Alternative considered:* a `TerseEquation*` environment. Rejected because themes do not style equations today (`crates/terse-core/src/theme/resolve.rs:40`), so a wrapper adds indirection with no hook to justify it.

### D4. `fmt` treats the `$$` payload as opaque

`fmt` canonicalizes structural whitespace only. The `$$` payload gets the same treatment as a `math:` payload: internal lines are not reindented or reflowed. Inline `$...$` sits inside paragraph text, which `fmt` already leaves alone apart from structural whitespace.

## Risks / Trade-offs

- [Prose where two dollars satisfy the rule, e.g. `between $5 and 10$`, becomes math] $\rightarrow$ `5 and 10` contains no forbidden command, so it passes validation and silently renders as italic math. The author has to write `\$`. No `.trs` in the repository contains `$`.
- [`$$` mid-line is an error where Pandoc would accept it] $\rightarrow$ The diagnostic names the fix (move `$$` to its own line). This is a deliberate price for byte preservation.
- [A paragraph line that legitimately starts with `$$` as text, e.g. `$$ signs are...`] $\rightarrow$ It is parsed as display math and fails validation or termination with a located error. The author writes `\$$`, the same backslash escape that already turns a reserved start into literal prose.

