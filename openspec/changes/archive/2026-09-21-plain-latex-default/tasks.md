## 1. Baseline

- [x] 1.1 Make `body_font`, `page_size`, `page_margin_cm` optional in `ResolvedTheme`; `compiler_defaults` sets them `None`, `citation_style` numeric, `title_align` center; update `resolve.rs` arms and `font_package_for`/`font_files_for` callers. [tests: test_academic_defaults_are_the_plain_article, test_empty_theme_equals_defaults, test_typed_theme_values_rejected]
- [x] 1.2 Fix `theme/tests.rs` and `crates/terse-core/tests/themes.rs` pins that assumed Libertinus/A4/author-year/left. [tests: existing theme suite]

## 2. Style emission

- [x] 2.1 Emit `fontspec`, font package, `\setmainfont` only with a font token; emit `geometry` only with page tokens, size-only reproducing the class layout (345pt; 550pt letter, 598pt A4). [tests: test_default_style_has_no_font_or_geometry, test_font_token_pulls_fontspec, test_page_size_alone_reproduces_class_layout, test_every_accepted_property_reaches_the_style]
- [x] 2.2 Replace the heading loop with `\section`/`\subsection`/`\subsubsection` delegation; `none` starred, `roman` counter redefinition, `regular`/`italic` via the class's `\@startsection` definition. [tests: test_headings_delegate_to_sectioning_commands, test_numbering_none_uses_starred_form, test_numbering_roman_redefines_thesection, test_heading_weight_redefines_startsection, test_heading_numbering_disabled_keeps_anchor, test_theme_cannot_inject_raw_tex]
- [x] 2.3 Paper title block through `\title`/`\author`/`\date{}`/`\maketitle` with affiliations and subtitle; `TerseAbstract` as `abstract`; cover and left-aligned layouts keep the custom block. [tests: test_paper_title_block_uses_maketitle, test_missing_date_emits_no_today, test_cover_layout_keeps_custom_block, test_generate_document_is_deterministic_and_untimestamped]
- [x] 2.4 `render_title_material` emits abstract and keywords after `\end{TerseTitleBlock}`. [tests: test_abstract_follows_title_block, test_body_bytes_identical_across_themes]

## 3. Evidence and docs

- [x] 3.1 Heavy suite in the container: plain default compiles offline with Latin Modern and zero missing characters; A4-only theme matches the class oracle; existing theme, two-column, and wide-figure tests pass; recapture evidence PDFs. [tests: test_plain_default_compiles_full_fixture, test_page_size_only_changes_paper, heavy suite]
- [x] 3.2 README and docs: "no theme gives the plain article; add `page: margin` to use the page better; fonts and layouts through a theme"; refresh `~/terse-teste`. [tests: none, doc review]
- [x] 3.3 `cargo test --workspace --locked` green, `npx gitnexus analyze`, commit, archive. [tests: whole suite]
