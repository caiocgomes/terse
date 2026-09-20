//! Public-checkout workflow and reproducible-release checks. These exercise
//! the *documented* commands (README/docs/scripts) rather than calling
//! internal APIs directly, so they double as a check that the docs
//! haven't drifted from the actual CLI surface.

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::common::tempdir;

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf()
}

/// Simulates a "clean public checkout": copies the whole project (minus
/// build/target/cache noise) to a fresh temp directory, since this
/// repository has no git history to actually clone from.
fn copy_checkout_to(dest: &Path) {
    fn copy_dir(src: &Path, dest: &Path) {
        fs::create_dir_all(dest).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if matches!(
                name_str.as_ref(),
                "target" | ".terse-cache" | "build" | "dist" | ".git"
            ) {
                continue;
            }
            let path = entry.path();
            let target = dest.join(&name);
            if path.is_dir() {
                copy_dir(&path, &target);
            } else {
                fs::copy(&path, &target).unwrap();
            }
        }
    }
    copy_dir(&workspace_root(), dest);
}

#[test]
#[ignore = "builds the whole workspace from a copied checkout; slow"]
fn test_public_checkout_full_user_workflow() {
    let checkout = tempdir("public-checkout");
    copy_checkout_to(&checkout);

    // An explicit target directory inside the copied checkout, so the
    // binary under test is the one built from that checkout regardless of
    // any CARGO_TARGET_DIR the caller (e.g. the pinned container) sets.
    let target_dir = checkout.join("target");
    let status = Command::new("cargo")
        .args(["build", "--locked", "-p", "terse-cli", "--bin", "terse", "--target-dir"])
        .arg(&target_dir)
        .current_dir(&checkout)
        .status()
        .expect("cargo build must run");
    assert!(status.success(), "cargo build --locked must succeed from a clean checkout");

    let bin = target_dir.join("debug/terse");
    assert!(bin.exists(), "the checkout build must produce {}", bin.display());
    let project = checkout.join("demo-paper");
    fs::create_dir_all(&project).unwrap();

    let run = |args: &[&str]| -> std::process::Output {
        Command::new(&bin)
            .args(args)
            .current_dir(&project)
            .output()
            .expect("terse invocation must run")
    };

    assert!(run(&["init"]).status.success(), "init must succeed");
    assert!(run(&["check"]).status.success(), "check must succeed on the freshly scaffolded project");
    assert!(run(&["fmt", "--check"]).status.success(), "scaffold must already be canonically formatted");
    assert!(run(&["build", "--tex-only"]).status.success(), "source-only build must succeed without an engine");

    let _engine_guard = crate::common::ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let engine = crate::common::resolved_engine();
    let has_engine = engine.xelatex.is_some();
    if has_engine {
        assert!(
            run(&["build", "--require-pdf"]).status.success(),
            "PDF build must succeed when xelatex/biber are available"
        );
        assert!(project.join("build/academic/paper.pdf").exists());
    }

    assert!(
        run(&["export", "--target", "arxiv"]).status.success(),
        "offline arxiv export must succeed from the freshly scaffolded project"
    );

    // Two-theme build, LaTeX inspection, PDF, and export against the full
    // scholarly-paper fixture (locked DOI/arXiv data, redistributable
    // assets), run from inside the copied checkout — proving the just-
    // built binary works end to end on real multi-file/reference content
    // without any manual bibliography/TeX repair or per-theme source edit.
    let paper = checkout.join("full-paper-demo");
    fn copy_dir(src: &Path, dest: &Path) {
        fs::create_dir_all(dest).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.file_name().unwrap() == "build" {
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
    copy_dir(&crate::common::full_paper_fixture_dir(), &paper);

    let run_paper = |args: &[&str]| -> std::process::Output {
        Command::new(&bin).args(args).current_dir(&paper).output().expect("terse invocation must run")
    };
    assert!(run_paper(&["check"]).status.success(), "full-paper fixture must check cleanly");
    assert!(run_paper(&["fmt", "--check"]).status.success(), "full-paper fixture must already be canonical");

    if has_engine {
        for theme in ["academic", "magalu"] {
            let out = run_paper(&["build", "--require-pdf", "--theme", theme]);
            assert!(
                out.status.success(),
                "two-theme build must succeed for {theme}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            let tex = paper.join(format!("build/{theme}/paper.tex"));
            assert!(tex.exists(), "{theme} must generate inspectable LaTeX source");
            let pdf = paper.join(format!("build/{theme}/paper.pdf"));
            assert!(pdf.exists(), "{theme} must produce a PDF");
            let text = crate::common::pdf::extract_text(&pdf);
            assert!(!text.trim().is_empty(), "{theme} PDF must have real visible text, not blank output");
        }
    }

    let has_biber = engine.biber.is_some();
    let export_args: &[&str] = if has_engine && has_biber {
        &["export", "--target", "arxiv", "--require-compile"]
    } else {
        &["export", "--target", "arxiv"]
    };
    let export_out = run_paper(export_args);
    assert!(
        export_out.status.success(),
        "full-paper export must succeed from the copied checkout: {}",
        String::from_utf8_lossy(&export_out.stderr)
    );
}

#[test]
fn test_maintainer_can_reproduce_release_checks() {
    // Real test discovery: the documented commands must be genuinely
    // executable, and the count they report must reflect the actual
    // codebase, not a hardcoded number that could silently drift.
    let root = workspace_root();
    let list_output = Command::new("cargo")
        .args(["test", "--workspace", "--locked", "--", "--list"])
        .current_dir(&root)
        .output()
        .expect("cargo test --list must run");
    assert!(list_output.status.success(), "test discovery must succeed");
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let discovered = stdout.lines().filter(|l| l.trim_end().ends_with(": test")).count();

    // tests.md documents an initial floor of 146 primary tests (40 unit,
    // 87 integration, 19 E2E); the real count has grown since as edge
    // cases were added group by group. The requirement is a floor, not an
    // exact match — a shrinking count would indicate lost coverage.
    assert!(
        discovered >= 183,
        "discovered {discovered} tests, expected at least the documented floor of 183 (146 named in terse/tests.md plus 37 named in managed-toolchain/tests.md)"
    );

    for script in [
        "scripts/test-engine-free.sh",
        "scripts/test-tex.sh",
        "scripts/release-checks.sh",
        "scripts/package-release.sh",
    ] {
        let path = root.join(script);
        assert!(path.exists(), "documented script {script} must exist");
        let meta = fs::metadata(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert!(
                meta.permissions().mode() & 0o111 != 0,
                "{script} must be executable"
            );
        }
    }

    for doc in [
        "README.md",
        "CONTRIBUTING.md",
        "docs/installation.md",
        "docs/language.md",
        "docs/themes.md",
        "docs/citations.md",
        "docs/git-workflow.md",
        "docs/export.md",
    ] {
        assert!(root.join(doc).exists(), "documented file {doc} must exist");
    }

    for workflow in [".github/workflows/ci.yml", ".github/workflows/release.yml"] {
        assert!(root.join(workflow).exists(), "{workflow} must exist");
    }
}
