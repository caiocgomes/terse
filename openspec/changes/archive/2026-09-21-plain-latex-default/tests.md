# Tests

Written before the code. Unit tests next to the code (`theme/tests.rs`, `latex/mod.rs`), integration tests under `crates/terse-core/tests/` and `crates/terse-cli/tests/`, heavy tests behind the existing `heavy` feature in the container.

## Baseline (themes: Bounded presentation components)

- `test_academic_defaults_are_the_plain_article` (`theme/tests.rs`, replaces `test_academic_defaults_are_stable`): `compiler_defaults` has `body_font == None`, `page_size == None`, `page_margin_cm == None`, `citation_style == "numeric"`, `title_align == "center"`, `heading_weight == bold x3`, `heading_numbering == decimal x3`. Scenario: "No theme is the plain article".
- `test_empty_theme_equals_defaults` (`crates/terse-core/tests/themes.rs`): a comments-only theme resolves equal to `compiler_defaults` and its style contains neither `fontspec` nor `geometry`.

## Style emission (themes; latex-generation: Documented Unicode)

- `test_default_style_has_no_font_or_geometry` (`latex/mod.rs`): style for the defaults contains no `fontspec`, no `\setmainfont`, no font package, no `geometry`; still contains `hyperref`, `amsthm`, `graphicx`, `booktabs`.
- `test_font_token_pulls_fontspec` (`latex/mod.rs`): `body: font: libertinus-otf` emits `fontspec`, `libertinus-otf`, `\setmainfont`. Existing `test_every_accepted_property_reaches_the_style` keeps its coverage diff and updates its expectations to the new baseline.
- `test_page_size_alone_reproduces_class_layout` (`latex/mod.rs`): `page: size: a4` only emits one `geometry` line with `a4paper`, `textwidth=345pt`, `textheight=598pt`; `letter` gives `textheight=550pt`; `margin` set emits `margin=<n>cm` and no `textwidth`. Scenario: "Page size alone changes only the paper".

## Headings (themes: stable semantic interface)

- `test_headings_delegate_to_sectioning_commands` (`latex/mod.rs`): default style defines `\TerseHeadingOne` as `\section{#1}`, Two as `\subsection`, Three as `\subsubsection`; contains no `\newcounter{terseheading` and no `\selectfont`.
- `test_numbering_none_uses_starred_form` (`latex/mod.rs`): `heading.1 numbering: none` emits `\section*{#1}`; existing `test_heading_numbering_disabled_keeps_anchor` keeps passing (anchor from the body's `\phantomsection\label`).
- `test_numbering_roman_redefines_thesection` (`latex/mod.rs`): `\renewcommand{\thesection}{\Roman{section}}` only for roman.
- `test_heading_weight_redefines_startsection` (`latex/mod.rs`): `weight: italic` emits `\@startsection{section}` with `\itshape`; `bold` emits no redefinition; injection regression `test_theme_cannot_inject_raw_tex` stays.

## Title block (themes: stable semantic interface)

- `test_paper_title_block_uses_maketitle` (`latex/mod.rs`): default style's `TerseTitleBlock` begins with `\date{}` and ends with `\maketitle`; `\TerseTitle` maps to `\title`; `\TerseAuthor` appends `name\\affil` with `\and` between authors; `TerseAbstract` wraps `abstract`. Scenario: "Default title block is maketitle".
- `test_missing_date_emits_no_today` (`latex/mod.rs`, extends `test_generate_document_is_deterministic_and_untimestamped`): no `\today` in body or style for a dateless document.
- `test_cover_layout_keeps_custom_block` (`latex/mod.rs`): `title: layout: cover` still emits the `\vspace*...\TerseLogo...\clearpage` block and the size-based `\TerseTitle` macros; `align: left` under `paper` also keeps the custom block.
- `test_abstract_follows_title_block` (`latex/mod.rs`): body emits `\end{TerseTitleBlock}` before `\begin{TerseAbstract}` and `\TerseKeywords`; theme-blind gate (`test_body_bytes_identical_across_themes`) keeps passing.

## Heavy (container)

- `test_plain_default_compiles_full_fixture` (heavy): full-paper fixture with an empty theme compiles with XeLaTeX and Biber; zero `Missing character`; `pdffonts` lists `LMRoman*` and no Libertinus; PDF page size is letter. Scenarios: "No theme is the plain article", "Default font renders accents without fontspec".
- `test_page_size_only_changes_paper` (heavy): A4-only theme vs empty theme: page sizes A4 vs letter; `\textwidth`/`\textheight` logged by the A4 build equal those of a directly compiled `\documentclass[a4paper]{article}`.
- Existing heavy tests for `academic`, `magalu`, two-column, wide figures keep passing; evidence PDFs recaptured to `~/terse-gate-evidence-<date>/` including the plain default.
