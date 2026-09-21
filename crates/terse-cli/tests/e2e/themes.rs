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
    // gives the title material its own page: the first body heading is
    // absent from the cover's page and present on the paper-title build's
    // first page. (Asserted on page content, not page count: since the
    // paper title became `\maketitle`, both builds can total the same
    // number of pages.) And the logo, which was defined as a macro and
    // never invoked before this change, is now actually placed on that page.
    let academic_first = crate::common::pdf::extract_text_page(&academic_pdf, 1);
    let contrast_first = crate::common::pdf::extract_text_page(&contrast_pdf, 1);
    assert!(
        academic_first.contains("Background"),
        "title.layout = paper keeps the body on the first page: {academic_first}"
    );
    assert!(
        !contrast_first.contains("Background"),
        "title.layout = cover must give the title material its own page: {contrast_first}"
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

/// Adds a theme file to the copied fixture and declares it in the manifest.
fn add_theme(tmp: &std::path::Path, name: &str, source: &str) {
    fs::write(tmp.join("themes").join(format!("{name}.theme")), source).unwrap();
    let manifest = tmp.join("terse.toml");
    let text = fs::read_to_string(&manifest).unwrap();
    fs::write(&manifest, format!("{text}{name} = \"themes/{name}.theme\"\n")).unwrap();
}

/// `plain-latex-default` scenarios "No theme is the plain article" and
/// "Default font renders accents without fontspec": a theme that sets
/// nothing must produce the unmodified `article`, proven at the only levels
/// that cannot lie: the packages the style loads, the fonts the PDF embeds,
/// and the sheet size. A successful `--require-pdf` build already implies
/// zero `Missing character` lines (build.rs fails the build on them).
#[test]
#[ignore = "requires a local XeLaTeX/Biber distribution"]
fn test_plain_default_compiles_full_fixture() {
    let _engine_guard = crate::common::ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    crate::common::pdf::require_pdf_tools();
    let tmp = tempdir("plain-default");
    copy_fixture_to(&tmp);
    add_theme(&tmp, "plain", "// Sets nothing: the compiler default is the plain article.\n");

    let code = terse_cli::run(["terse", "build", "--require-pdf", "--theme", "plain"], &tmp);
    assert_eq!(code, 0, "the plain default must build the full-paper fixture cleanly");
    let out = tmp.join("build").join("plain");

    let style = fs::read_to_string(out.join("terse-style.sty")).unwrap();
    for absent in ["fontspec", "setmainfont", "geometry", "libertinus"] {
        assert!(!style.contains(absent), "the plain default must not load `{absent}`");
    }
    assert!(style.contains("\\maketitle"), "the plain title block must delegate to \\maketitle");
    assert!(style.contains("\\section{#1}"), "plain headings must delegate to \\section");
    assert!(style.contains("\\begin{abstract}"), "the plain abstract must be the abstract environment");

    let pdf = out.join("paper.pdf");
    let fonts = crate::common::pdf::fonts(&pdf);
    assert!(fonts.contains("LMRoman"), "the plain default must embed Latin Modern:\n{fonts}");
    assert!(!fonts.contains("Libertinus"), "the plain default must not embed Libertinus:\n{fonts}");

    let (w, h) = crate::common::pdf::page_size(&pdf);
    assert!((w - 612.0).abs() < 2.0 && (h - 792.0).abs() < 2.0, "plain default must be letter: {w}x{h}");

    let text: String = crate::common::pdf::extract_text(&pdf).split_whitespace().collect::<Vec<_>>().join(" ");
    for expected in ["Lovelace", "Abstract", "Background", "Discussion"] {
        assert!(text.contains(expected), "plain PDF lost `{expected}`");
    }
}

/// `plain-latex-default` scenario "Page size alone changes only the paper":
/// a theme declaring only `page: size: a4` must change the sheet and keep
/// the text block `article` computes for that sheet. The oracle is the
/// class itself, compiled directly with the same engine.
#[test]
#[ignore = "requires a local XeLaTeX/Biber distribution"]
fn test_page_size_only_changes_paper() {
    let _engine_guard = crate::common::ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    crate::common::pdf::require_pdf_tools();
    let tmp = tempdir("page-size-only");
    copy_fixture_to(&tmp);
    add_theme(&tmp, "plain", "// Sets nothing.\n");
    add_theme(&tmp, "a4only", "page:\n  size: a4\n");

    for theme in ["plain", "a4only"] {
        let code = terse_cli::run(["terse", "build", "--require-pdf", "--theme", theme], &tmp);
        assert_eq!(code, 0, "theme '{theme}' must build cleanly");
    }
    let plain_style = fs::read_to_string(tmp.join("build/plain/terse-style.sty")).unwrap();
    let a4_style = fs::read_to_string(tmp.join("build/a4only/terse-style.sty")).unwrap();

    // The only difference between the two styles is the geometry line
    // (and the theme name in the header comment).
    let strip = |s: &str| -> Vec<String> {
        s.lines()
            .filter(|l| !l.contains("geometry") && !l.starts_with("% Generated by terse"))
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(strip(&plain_style), strip(&a4_style), "page.size alone must change only the geometry line");
    let geometry = a4_style
        .lines()
        .find(|l| l.contains("geometry"))
        .expect("the A4-only style must load geometry");
    assert!(geometry.contains("a4paper"), "geometry must select A4: {geometry}");

    // Class oracle: compile `\documentclass[a4paper]{article}` directly and
    // read the block it computes; the theme's geometry line must carry the
    // same numbers.
    let engine = crate::common::resolved_engine();
    let xelatex = engine.xelatex.expect("a resolved xelatex");
    let oracle_dir = tmp.join("oracle");
    fs::create_dir_all(&oracle_dir).unwrap();
    fs::write(
        oracle_dir.join("oracle.tex"),
        "\\documentclass[a4paper]{article}\\begin{document}\\typeout{TERSE-ORACLE TW=\\the\\textwidth TH=\\the\\textheight}x\\end{document}\n",
    )
    .unwrap();
    let status = crate::common::engine_command(&xelatex, &engine.env)
        .current_dir(&oracle_dir)
        .args(["-interaction=batchmode", "oracle.tex"])
        .status()
        .expect("xelatex must run");
    assert!(status.success(), "the oracle article must compile");
    let log = fs::read_to_string(oracle_dir.join("oracle.log")).unwrap();
    let line = log.lines().find(|l| l.contains("TERSE-ORACLE")).expect("oracle typeout in log");
    let dim = |key: &str| -> f64 {
        let start = line.find(key).unwrap() + key.len();
        line[start..].split("pt").next().unwrap().trim().parse::<f64>().unwrap()
    };
    let (tw, th) = (dim("TW="), dim("TH="));
    let style_dim = |key: &str| -> f64 {
        let start = geometry.find(key).unwrap() + key.len();
        geometry[start..].split("pt").next().unwrap().parse::<f64>().unwrap()
    };
    assert!((style_dim("textwidth=") - tw).abs() < 0.01, "textwidth must match the class: {geometry} vs {tw}");
    assert!((style_dim("textheight=") - th).abs() < 0.01, "textheight must match the class: {geometry} vs {th}");

    let (pw, ph) = crate::common::pdf::page_size(&tmp.join("build/plain/paper.pdf"));
    let (aw, ah) = crate::common::pdf::page_size(&tmp.join("build/a4only/paper.pdf"));
    assert!((pw - 612.0).abs() < 2.0 && (ph - 792.0).abs() < 2.0, "plain must be letter: {pw}x{ph}");
    assert!((aw - 595.0).abs() < 2.0 && (ah - 842.0).abs() < 2.0, "a4only must be A4: {aw}x{ah}");
}
