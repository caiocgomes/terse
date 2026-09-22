## Why

`$...$` and `$$...$$` are the math delimiters authors already type, both in LaTeX papers and in Markdown. Terse only accepts `\(...\)` inline and the `math:` block, so every pasted formula has to be rewritten by hand. The `compile-markdown` change also needs these delimiters, because a Markdown file with math is written with dollars.

## What Changes

- Inline `$...$` in `.trs` prose means the same thing as `\(...\)`: the same semantic node, the same validation against the math allowlist (`crates/terse-core/src/syntax/math.rs`), the same generated LaTeX.
- `$$...$$` means display math: the same semantic node and validation as an unlabeled `math:` block.
- Dollar recognition follows the Pandoc boundary rule, so currency in prose stays text. An opening `$` must be followed by a non-space character. A closing `$` must be preceded by a non-space character and must not be followed by a digit. A `$` with no valid partner is literal text. `\$` stays a literal dollar; it is already an accepted escape (`crates/terse-core/src/syntax/inlines.rs:9`).
- `\(...\)` and `math:` stay as they are. `fmt` keeps whichever delimiter the author wrote and does not normalize one form into the other.
- **BREAKING (narrow)**: today every `$` in `.trs` prose is literal and is escaped to `\$` on output (`crates/terse-core/src/latex/escape.rs:11`). Prose where two dollars happen to satisfy the boundary rule, as in `between $5 and 10$`, will now parse as math. The fix is to write `\$`. No `.trs` file in the repository contains a `$` today, so no fixture changes meaning.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `language-parsing`: the inline-elements requirement gains `$...$` next to `\(math\)`, and display math gains the `$$...$$` form. The "Validated TeX math without payload rewriting" requirement applies to both new forms unchanged.

## Impact

- `crates/terse-core/src/syntax/inlines.rs`: recognize `$` and `$$` with the boundary rule and reuse the existing math parsing path.
- `crates/terse-core/src/syntax/blocks.rs`: only if `$$` on its own lines is treated as a block rather than an inline; the design decides.
- `docs/language.md`: document both delimiters and the currency rule.
- No change to semantic nodes, LaTeX generation, themes, or the math allowlist.

## Open for design

- Whether `$$...$$` may span source lines. Markdown authors usually write it that way. Today's rule says inline math stays on one logical line, so display dollars need their own rule.
- Whether `$$` in the middle of a paragraph splits it into paragraph, equation, paragraph, or is only accepted as a paragraph by itself.
