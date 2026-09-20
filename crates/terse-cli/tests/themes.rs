//! CLI-level two-theme coverage (task group 10): building the same
//! single-module content under both public example themes must produce
//! byte-identical main TeX/bibliography and visibly different style
//! layers. Full PDF-level visible-content assertions are group 17's
//! `tests/common/pdf.rs` helper, exercised against the complete
//! full-paper fixture; this file stays at the generated-source level.

use std::fs;
use std::path::Path;

fn tempdir(label: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir().join(format!(
        "terse-themes-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&base).unwrap();
    base
}

fn themes_fixture_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/themes")
}

fn write_two_theme_project(tmp: &Path, entry: &str) {
    fs::write(
        tmp.join("terse.toml"),
        concat!(
            "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n\n",
            "[themes]\nacademic = \"themes/academic.theme\"\nmagalu = \"themes/magalu.theme\"\n",
        ),
    )
    .unwrap();
    fs::write(tmp.join("paper.trs"), entry).unwrap();
    fs::create_dir_all(tmp.join("themes/assets")).unwrap();
    for name in ["academic.theme", "magalu.theme"] {
        fs::copy(themes_fixture_dir().join(name), tmp.join("themes").join(name)).unwrap();
    }
    fs::copy(
        themes_fixture_dir().join("assets/logo.png"),
        tmp.join("themes/assets/logo.png"),
    )
    .unwrap();
}

const RICH_ENTRY: &str = concat!(
    "document:\n  title: \"All Elements\"\n\n",
    "# Introduction [id: sec-intro]\n\n",
    "A paragraph with *emphasis*, **strong**, and math \\(x^2\\).\n\n",
    "- one\n- two\n\n",
    "math [id: eq-main]:\n  x = y + 1\n\n",
    "theorem [id: thm-main]:\n  Every paragraph terminates.\n\n",
    "proof [of: thm-main]:\n  By construction.\n\n",
    "tex:\n  % an explicit raw escape hatch\n",
);

#[test]
fn test_two_theme_presentations_differ() {
    let tmp = tempdir("differ");
    write_two_theme_project(&tmp, RICH_ENTRY);

    for theme in ["academic", "magalu"] {
        let code = terse_cli::run(["terse", "build", "--tex-only", "--theme", theme], &tmp);
        assert_eq!(code, 0, "theme '{theme}' must build cleanly");
    }

    let academic_style = fs::read_to_string(tmp.join("build/academic/terse-style.sty")).unwrap();
    let magalu_style = fs::read_to_string(tmp.join("build/magalu/terse-style.sty")).unwrap();
    assert_ne!(academic_style, magalu_style, "presentation must differ between themes");
    assert!(academic_style.contains("libertinus-otf"));
    assert!(magalu_style.contains("tgheros"));
    assert!(magalu_style.contains("AddToShipoutPictureBG"), "magalu carries a watermark");
    assert!(
        !academic_style.contains("AddToShipoutPictureBG"),
        "academic declares no watermark"
    );
}

#[test]
fn test_all_elements_render_under_both_themes() {
    let tmp = tempdir("all-elements");
    write_two_theme_project(&tmp, RICH_ENTRY);

    let mut bodies = Vec::new();
    for theme in ["academic", "magalu"] {
        let code = terse_cli::run(["terse", "build", "--tex-only", "--theme", theme], &tmp);
        assert_eq!(code, 0, "theme '{theme}' must build cleanly with every MVP element present");
        let tex = fs::read_to_string(tmp.join("build").join(theme).join("paper.tex")).unwrap();
        assert!(tex.contains("\\TerseHeadingOne{Introduction}"));
        assert!(tex.contains("\\emph{emphasis}"));
        assert!(tex.contains("\\textbf{strong}"));
        assert!(tex.contains("\\(x^2\\)"));
        assert!(tex.contains("\\begin{itemize}"));
        assert!(tex.contains("\\begin{TerseEquation}\\label{eq-main}"));
        assert!(tex.contains("\\begin{tersetheorem}"));
        assert!(tex.contains("\\begin{proof}"));
        assert!(tex.contains("% an explicit raw escape hatch"));
        bodies.push(tex);
    }
    assert_eq!(bodies[0], bodies[1], "main tex bytes are theme-invariant across every element kind");
}
