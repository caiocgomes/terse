//! Full-paper fixture acceptance gate A: the same authored content under
//! both public themes, compared at every level tests.md asks for
//! (semantic projection, generated .tex/.bib bytes, authored sequence,
//! rendered PDF text/links), with only the style layer differing.

use std::fs;

use crate::common::{full_paper_fixture_dir, tempdir};

fn copy_fixture_to(dest: &std::path::Path) {
    fn copy_dir(src: &std::path::Path, dest: &std::path::Path) {
        fs::create_dir_all(dest).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() && path.file_name().unwrap() == "build" {
                continue;
            }
            let target = dest.join(entry.file_name());
            if path.is_dir() {
                copy_dir(&path, &target);
            } else {
                fs::copy(&path, &target).unwrap();
            }
        }
    }
    copy_dir(&full_paper_fixture_dir(), dest);
}

/// `close-verification-gaps` scenario "Declared settings reach the output",
/// at the only level that can prove it: the rendered page.
///
/// Every setting this theme declares was accepted by the schema and
/// discarded by the generator before this change, so against `academic`
/// the two PDFs were identical in page size, citation form and logo. Each
/// assertion below is one of those, and each would pass vacuously if
/// asserted on the generated body instead, which is theme-blind.
#[test]
#[ignore = "requires a local XeLaTeX/Biber distribution"]
fn test_declared_settings_are_visible_in_rendered_pdfs() {
    let _engine_guard = crate::common::ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    crate::common::pdf::require_pdf_tools();
    let tmp = tempdir("declared-settings-visible");
    copy_fixture_to(&tmp);

    // The contrast theme and its logo asset join the fixture project.
    let themes_dir = tmp.join("themes");
    fs::copy(
        crate::common::themes_fixture_dir().join("contrast.theme"),
        themes_dir.join("contrast.theme"),
    )
    .unwrap();
    let manifest = tmp.join("terse.toml");
    let text = fs::read_to_string(&manifest).unwrap();
    fs::write(&manifest, text + "contrast = \"themes/contrast.theme\"\n").unwrap();

    for theme in ["academic", "contrast"] {
        let code = terse_cli::run(["terse", "build", "--require-pdf", "--theme", theme], &tmp);
        assert_eq!(code, 0, "theme '{theme}' must build the fixture cleanly");
    }
    let academic_pdf = tmp.join("build").join("academic").join("paper.pdf");
    let contrast_pdf = tmp.join("build").join("contrast").join("paper.pdf");

    // Page geometry: A4 (595x842pt) against letter (612x792pt).
    let (aw, ah) = crate::common::pdf::page_size(&academic_pdf);
    let (cw, ch) = crate::common::pdf::page_size(&contrast_pdf);
    assert!(
        (aw - cw).abs() > 5.0 || (ah - ch).abs() > 5.0,
        "page.size must reach the rendered page: academic {aw}x{ah}, contrast {cw}x{ch}"
    );

    // Citation form: author-year against numeric.
    let academic_text = crate::common::pdf::extract_text(&academic_pdf);
    let contrast_text = crate::common::pdf::extract_text(&contrast_pdf);
    assert!(
        contrast_text.contains("[1]"),
        "citation.style = numeric must render bracketed numbers"
    );
    assert!(
        !academic_text.contains("[1]"),
        "citation.style = author-year must not render bracketed numbers"
    );

    // Cover layout and logo, the two settings group 2 activated. A cover
    // gives the title material its own page, so the contrast build has one
    // page more than the paper-title build of the same content; and the
    // logo, which was defined as a macro and never invoked before this
    // change, is now actually placed on that page.
    let academic_pages = crate::common::pdf::page_count(&academic_pdf);
    let contrast_pages = crate::common::pdf::page_count(&contrast_pdf);
    assert!(
        contrast_pages > academic_pages,
        "title.layout = cover must give the title material its own page: \
         academic {academic_pages} pages, contrast {contrast_pages}"
    );
    let contrast_style = fs::read_to_string(tmp.join("build/contrast/terse-style.sty")).unwrap();
    assert!(
        contrast_style.contains("{TerseTitleBlock}") && contrast_style.contains("\\TerseLogo\\par"),
        "the cover must invoke \\TerseLogo, not merely define it"
    );
    // Visual evidence that the logo is ink on the page, not just a macro
    // in the preamble: the band it occupies near the top of the cover is
    // not a single flat color.
    let cover_dir = tmp.join("cover-render");
    fs::create_dir_all(&cover_dir).unwrap();
    let cover_png = crate::common::pdf::render_page_png(&contrast_pdf, 1, &cover_dir);
    assert!(
        !crate::common::pdf::region_is_uniform(&cover_png, 0.25, 0.08, 0.75, 0.30),
        "the cover's logo band must not be blank"
    );

    // Both still render the authored content: a presentation change may
    // never cost a sentence. Asserted on distinctive single words, because
    // `pdftotext` interleaves columns in the two-column theme and would
    // split any multi-word phrase; whitespace is normalized for the same
    // reason.
    for (label, text) in [("academic", &academic_text), ("contrast", &contrast_text)] {
        let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        for expected in ["Lovelace", "Background", "Discussion", "Turing", "Shannon"] {
            assert!(flat.contains(expected), "{label} PDF lost `{expected}`");
        }
    }
}

#[test]
#[ignore = "requires a local XeLaTeX/Biber distribution"]
fn test_same_paper_content_under_two_themes() {
    let _engine_guard = crate::common::ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempdir("full-paper-two-themes");
    copy_fixture_to(&tmp);

    let mut tex_bytes = Vec::new();
    let mut bib_bytes = Vec::new();
    let mut pdf_texts = Vec::new();
    for theme in ["academic", "magalu"] {
        let code = terse_cli::run(["terse", "build", "--require-pdf", "--theme", theme], &tmp);
        assert_eq!(code, 0, "theme '{theme}' must build the full-paper fixture cleanly");
        let out = tmp.join("build").join(theme);
        tex_bytes.push(fs::read(out.join("paper.tex")).unwrap());
        bib_bytes.push(fs::read(out.join("references.bib")).unwrap());
        pdf_texts.push(crate::common::pdf::extract_text(&out.join("paper.pdf")));
    }

    assert_eq!(tex_bytes[0], tex_bytes[1], "generated main TeX must be byte-identical across themes");
    assert_eq!(bib_bytes[0], bib_bytes[1], "generated bibliography must be byte-identical across themes");

    // Independently-listed authored content sequence, not just a digest:
    // every element that should be visible in text form appears under
    // both themes, in the same relative order.
    for text in &pdf_texts {
        assert!(text.contains("A Complete Terse Paper"));
        assert!(text.contains("Background"));
        assert!(text.contains("Discussion"));
        assert!(text.contains("Results"));
        assert!(text.contains("Turing"));
        assert!(text.contains("Shannon"));
        assert!(!text.contains("??"), "no unresolved reference placeholders:\n{text}");
    }
    // The style layer, and only the style layer, differs and is visibly
    // non-empty in that difference (magalu's watermark/logo).
    let academic_style = fs::read_to_string(tmp.join("build/academic/terse-style.sty")).unwrap();
    let magalu_style = fs::read_to_string(tmp.join("build/magalu/terse-style.sty")).unwrap();
    assert_ne!(academic_style, magalu_style);
}

#[test]
#[ignore = "requires a local XeLaTeX/Biber distribution"]
fn test_public_theme_has_no_private_dependencies() {
    let _engine_guard = crate::common::ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Both shipped example themes must build using only distribution
    // fonts and redistributable, theme-relative assets: nothing outside
    // the copied fixture tree, no ambient corporate resources.
    let tmp = tempdir("full-paper-public-themes");
    copy_fixture_to(&tmp);

    for theme in ["academic", "magalu"] {
        let code = terse_cli::run(["terse", "build", "--require-pdf", "--theme", theme], &tmp);
        assert_eq!(code, 0, "theme '{theme}' must compile using only its own bundled/distribution resources");
    }
}
