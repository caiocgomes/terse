## Why

With no theme, Terse must produce the LaTeX academics already like: `\documentclass{article}` with Computer Modern (Latin Modern under XeLaTeX), `\maketitle`, `\section`, the `abstract` environment, and the class's own page layout. The author stated it on 2026-09-20 and again on 2026-09-21: "eu queria só que ele também fosse o padrão do terse ... neste momento se ele funcionar com o padrão, já está bom. depois podemos melhorar pra outros temas."

Today the compiler default is Terse's own look: Libertinus through `fontspec`, A4 with 2.5 cm margins through `geometry`, headings as custom `\Large\bfseries` blocks with private counters, a custom title block whose `\TerseAuthor` discards the affiliation argument, and a hand-made "Abstract." paragraph. The scaffold's `academic.theme` is an empty file, so every new project gets this look with no way to reach the plain one.

One measured fact makes the change small: under the pinned XeLaTeX, a bare `\documentclass{article}` without `fontspec` embeds Latin Modern and typesets pt-BR accents with zero missing characters. The plain default needs no font machinery at all.

## What Changes

- **Compiler default becomes the unmodified `article`.** No font token: no `fontspec`, no font package, no `\setmainfont`. No page token: no `geometry`, so the class's letter layout applies. Headings delegate to `\section`, `\subsection`, `\subsubsection`. The title block collects `\title`, `\author` (with affiliations), `\date` and runs `\maketitle`, emitting `\date{}` when the document has no date so `\today` never appears. The abstract uses the `abstract` environment. Citations default to numeric, which is what a plain article with a bibliography gives.
- **Theme properties keep their meaning; only what happens when they are unset changes.** `body: font` still loads `fontspec` and the font; `page: size`/`margin` still load `geometry`; `heading weight` maps onto the class's own `\@startsection` font argument (no new package); `numbering: none` uses `\section*`, `roman` redefines `\thesection`. `title: layout: cover` keeps the custom cover block.
- **`page: size` alone reproduces the class layout on that paper**: text block 345pt wide and the line count `article` computes for the paper (measured: letter 550pt, A4 598pt), so a theme that only enlarges the paper changes the paper and nothing else visible.
- **Body order**: abstract and keywords move out of `TerseTitleBlock` and follow it, because `\maketitle` must precede `abstract`. The body stays byte-identical across themes.

Unchanged and out of scope: the theme name `academic` remains the default name and output directory (an empty `academic.theme` now equals the plain default, which is what the name should mean); the scaffold; the unconditional loading of `amsmath`, `amsthm`, `graphicx`, `booktabs`, `enumitem`, `hyperref`, `xcolor` (they do not alter the plain look and keep the body theme-blind); the pinned package closure; the `[themes]` manifest rules.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `themes`: "Bounded presentation components" (the compiler default is the plain `article`), "Portable font and asset selection" (kernel font when no token), "Style compilation uses a stable semantic interface" (title and headings delegate to the class by default).
- `latex-generation`: "Documented Unicode and bibliography toolchain" (the default needs no `fontspec`).

## Impact

- Code: `crates/terse-core/src/theme/mod.rs` (`compiler_defaults`: font and page become optional, citation numeric, title align center), `crates/terse-core/src/theme/resolve.rs` (writes into the optional fields), `crates/terse-core/src/latex/mod.rs` (`generate_style` package lines conditional on font/page; heading, title and abstract macros; `render_title_material` emits abstract and keywords after the title block).
- Tests pinned to the old default change on purpose: `test_academic_defaults_are_stable`, `test_every_accepted_property_reaches_the_style`, heading and title assertions in `latex/mod.rs` tests, `theme/tests.rs` pins, gate evidence PDFs.
- Every project without an explicit theme changes appearance. Deliberate; there is no released user base.
