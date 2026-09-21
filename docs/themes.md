# Themes

A `.theme` file is a closed, declarative style schema — never code. It has
no expressions, no content-selection, no imports, and no executable hooks
(`crates/terse-core/src/theme/parse.rs` rejects all of these at parse
time). Themes can only set typed properties on a fixed set of components;
they can never mutate the authored document tree.

## Selectors

```
page:
  margin: 2.5cm

figure:
  width: 80%

figure[role=wide]:
  width: 100%
```

Bare `component:` selectors set a base value; `component[role=value]:`
overrides it for that role only. Selector order in the file never matters
— duplicates are rejected outright rather than resolved by "last one
wins."

## The default is plain LaTeX

With no theme, or with a theme that sets nothing (the empty `academic.theme`
that `terse init` writes), the output is what a LaTeX user would write by
hand for a paper: `\documentclass{article}` with the class's own font
(Computer Modern, embedded as Latin Modern under XeLaTeX), the class's own
page layout, `\maketitle` for the title block (authors with their
affiliations), the `abstract` environment, `\section`/`\subsection`/
`\subsubsection` headings, and numeric citations. The generated
`terse-style.sty` loads no font package and no `geometry` in that case.

Every theme property is a change over that baseline and pulls in only what
it needs. The most common wish, using more of the page, is one line:

```
page:
  margin: 2.5cm
```

`page: size: a4` alone changes the sheet and keeps the text-block width
and margin rule the class computes for that paper; add `margin` to
override it. `body: font` switches to a TeX-distributed OpenType family
through `fontspec`.

## Bundled themes

- `academic` — the name of the default. The scaffold's file is empty, so it
  is the plain LaTeX look above; the fixture copy under `tests/fixtures/`
  shows a fuller academic delta (A4, 2.5 cm margins, Libertinus,
  author-year citations).
- `magalu` — a clearly-unofficial example theme (visible watermark and
  logo) demonstrating furniture and branding properties. It has no private
  dependencies: its logo is a redistributable, hand-authored placeholder
  under the theme's own directory (`test_public_theme_has_no_private_dependencies`).

Select a theme with `--theme <name>` or in `terse.toml`'s `[themes]` table.

## Assets

Logos resolve relative to their own `.theme` file's directory. Absolute
paths, URL schemes, and `..` escapes are rejected before any file is read.

## Bounds

Geometry and visibility settings are checked before generation: margins
must leave a usable page area, figure widths are 1–100%, watermark opacity
is capped at 0.3, and body text color may never equal the background.
Violating any of these is a diagnostic, not a silently clamped value.

## What themes cannot do

Themes cannot select or filter content, add or remove document nodes, run
code, or fetch external resources. The same authored `.trs` source
produces byte-identical main TeX/bibliography content under every theme —
only the style layer (`terse-style.sty`) differs
(`test_theme_switch_keeps_body_bytes`, `test_theme_invariant_projection`).
