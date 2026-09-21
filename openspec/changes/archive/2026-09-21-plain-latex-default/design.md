## Context

`generate_style` (`crates/terse-core/src/latex/mod.rs:750-788`) emits `fontspec`, a font package, `\setmainfont`, and `geometry` unconditionally; headings come from a custom loop (`:681-699`) with private counters `terseheadingone..three` and `\Large\bfseries\selectfont`; the title block is a custom environment (`:713-725`) whose `\TerseAuthor[2]` prints only the name (`:780`); `TerseAbstract` is a bold "Abstract." paragraph (`:783`). `render_title_material` (`:261-326`) puts the abstract and keywords inside `TerseTitleBlock`. `compiler_defaults` (`theme/mod.rs:60`) hard-codes `a4`, 2.5 cm, `libertinus-otf`, author-year, `title_align: left`. The fixture themes `tests/fixtures/themes/{academic,magalu}.theme` (identical to the `full-paper` copies) already declare page, font, and citation style explicitly, so they are unaffected by a baseline change. The scaffold's `academic.theme` (`init.rs:24-25`) is a comment only.

Measured on 2026-09-20/21 with the managed TeX Live 2025 XeLaTeX: plain `article` embeds Latin Modern and renders pt-BR with zero missing characters; `size10.clo` gives `\textwidth=345pt` on any paper and `\textheight` 550pt on letter, 598pt on A4 (`size10.clo:131-138`). `article.cls:302-313` defines the three sectioning commands through `\@startsection` with the font as the last argument (`\normalfont\Large\bfseries`, `\large\bfseries`, `\normalsize\bfseries`).

## Goals / Non-Goals

Goals: with an empty theme the output is what a LaTeX user writes by hand for a paper; theme properties keep working; body bytes stay theme-blind; deterministic, untimestamped output; no change to the pinned package closure.

Non-Goals: renaming the default, changing the scaffold, loading content packages conditionally, new theme properties, new packages. Deferred with the author's words: "depois podemos melhorar pra outros temas."

## Decisions

### D1. Optional font and page in the resolved theme

`ResolvedTheme.body_font` becomes `Option<String>` and `page_size`/`page_margin_cm` become `Option`. `compiler_defaults` sets them to `None`, `citation_style` to `numeric`, `title_align` to `center` (what `\maketitle` does). Everything else keeps its value. `resolve.rs` arms write `Some(..)`. `font_package_for`/`font_files_for` take the token when present. The name stays `academic`; nothing in the CLI changes.

### D2. Conditional font and geometry lines

`generate_style` emits `\RequirePackage{fontspec}`, the font package, and `\setmainfont` only when `body_font` is `Some`. It emits `geometry` only when `page_size` or `page_margin_cm` is `Some`:
- size and margin: `[<paper>,margin=<n>cm]` (as today);
- margin only: `[margin=<n>cm]` on the class paper;
- size only: `[<paper>,textwidth=345pt,textheight=<class value>,centering]` with the class value per paper (letter 550pt, A4 598pt), so only the sheet changes. The heavy test's oracle is `\documentclass[a4paper]{article}` compiled directly: `\textwidth` and `\textheight` must match.

All other `\RequirePackage` lines stay unconditional. `STYLE_PACKAGES` is unchanged because every package can still be emitted.

### D3. Headings delegate to the class

The custom loop and counters go. Per level: default (`weight: bold`, `numbering: decimal`) emits `\newcommand{\TerseHeadingOne}[1]{\section{#1}}` and likewise for the other two. `numbering: none` emits `\section*{#1}`; the body's `\phantomsection\label` (`latex/mod.rs:80`) still gives it an anchor, and `\Terseref` for unnumbered headings keeps using the title reference the body already emits. `numbering: roman` adds `\renewcommand{\thesection}{\Roman{section}}` (and the corresponding `\thesubsection`/`\thesubsubsection` when set at those levels). `weight: regular|italic` re-issues the class's `\@startsection` definition from `article.cls:302-313` with `\bfseries` replaced by `\mdseries`/`\itshape`, wrapped in `\makeatletter ... \makeatother`. No package is added. `heading_weight_command` stays as the token-to-command map.

### D4. Title block through `\maketitle`

Paper layout with `align: center` (the new default):

```latex
\newenvironment{TerseTitleBlock}{\date{}}{\maketitle}
\newcommand{\TerseTitle}[1]{\title{#1}}
\newcommand{\TerseSubtitle}[1]{\g@addto@macro\@title{\\[0.5ex]\large #1}}
\newcommand{\TerseAuthor}[2]{<append "name" or "name\\affil" to \@author, separated by \and after the first>}
\newcommand{\TerseAffiliation}[1]{\g@addto@macro\@author{\\ #1}}
\newcommand{\TerseDate}[1]{\date{#1}}
\newenvironment{TerseAbstract}{\begin{abstract}}{\end{abstract}}
```

`\date{}` at environment start guarantees no `\today`. An empty affiliation argument appends nothing (checked with `\if\relax\detokenize{#2}\relax`). `\g@addto@macro` needs `\makeatletter` in the `.sty`; a `.sty` is already in `@` mode, so nothing extra is needed. `\maketitle` in two-column mode already spans (`article` uses `\twocolumn[\@maketitle]` when `\col@number>1`), so `\AtBeginDocument{\twocolumn}` (`:620`) stays.

`layout: cover`, or `align: left`, keep today's custom block and macros unchanged, including the cover's logo and `\clearpage`.

### D5. Abstract and keywords follow the title block

`render_title_material` closes `TerseTitleBlock` after the date and emits `TerseAbstract` and `\TerseKeywords` after it. This changes body bytes for every theme identically; the theme-blind gate still holds. Under the cover layout the abstract now starts on the page after the cover, which is the conventional cover behavior; the magalu evidence PDF changes accordingly.

## Risks / Trade-offs

- **Default look changes for every untethered project.** Requested.
- **`\g@addto@macro\@title`** with `\\` inside `\title` is standard practice for subtitles; `hyperref`'s `pdftitle` is set by the body's `\hypersetup` from plain text, not from `\@title`, so no `\\` leaks into PDF metadata.
- **Numeric citations by default** change `\autocite` rendering for projects that relied on author-year; the fixture themes declare their style explicitly, so evidence stays.
- **Cover layout moves the abstract** off the cover page. Acceptable and conventional; documented in the evidence.

## Migration Plan

One commit on `main`. Order: D1, D2, D3, D4, D5, then fixture evidence and docs. Archive normally (no conflicting deltas).
