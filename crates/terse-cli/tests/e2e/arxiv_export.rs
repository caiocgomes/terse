//! [Acceptance L] arXiv package: a real, self-contained extraction of a
//! full-fixture export compiles conventionally in a genuinely separate,
//! Terse-free, project-free directory with no user TeX tree.

use std::fs;
use std::sync::Mutex;

use crate::common::{full_paper_fixture_dir, tempdir, ENGINE_LOCK};

static PATH_LOCK: Mutex<()> = Mutex::new(());

fn copy_dir_all(src: &std::path::Path, dest: &std::path::Path) {
    fs::create_dir_all(dest).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dest.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir_all(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn copy_fixture_into(dest: &std::path::Path) {
    copy_dir_all(&full_paper_fixture_dir(), dest);
}

#[test]
#[ignore = "requires a local XeLaTeX + Biber distribution"]
fn test_arxiv_archive_compiles_in_clean_environment() {
    let _guard = PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _engine_guard = ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let engine = crate::common::resolved_engine();

    let tmp = tempdir("arxiv-export-e2e");
    copy_fixture_into(&tmp);

    // `--require-compile`: the export command itself must prove the
    // extracted package actually compiles (group 24.3/24.4) before
    // publishing anything, using the real toolchain since one is
    // available here.
    let code = terse_cli::run(["terse", "export", "--target", "arxiv", "--require-compile"], &tmp);
    assert_eq!(code, 0, "a full valid fixture with a locked bibliography must export and compile cleanly");

    let export_dir = tmp.join("build").join("export").join("academic").join("arxiv");
    assert!(export_dir.join("paper.tex").is_file());
    assert!(export_dir.join("references.bib").is_file());
    assert!(export_dir.join("MANIFEST.json").is_file());
    let zip_path = tmp.join("build").join("export").join("academic").join("paper-arxiv.zip");
    assert!(zip_path.is_file());
    let report_path = tmp.join("build").join("export").join("academic").join("paper-arxiv-report.txt");
    let report_text = fs::read_to_string(&report_path).unwrap();
    assert!(
        report_text.contains("compiled-local") || report_text.contains("compiled-profile"),
        "a required, successful compile must report a Compiled* status, not static-only: {report_text}"
    );
    assert!(
        !report_text.to_lowercase().contains("will be accepted"),
        "no report may claim arXiv acceptance"
    );

    // Extract the ZIP into a wholly separate, network-disabled directory
    // with an isolated personal TeX tree, no Terse binary, and no project
    // files -- then compile it with plain, conventional `xelatex`/`biber`
    // invocations, exactly as a human downloading the archive would.
    let zip_bytes = fs::read(&zip_path).unwrap();
    let clean_env = tempdir("arxiv-export-clean-room");
    terse_cli::export::validate::extract_zip(&zip_bytes, &clean_env).unwrap();
    assert!(clean_env.join("paper.tex").is_file());
    assert!(!clean_env.join("paper.pdf").exists(), "the rendered paper itself must never be a package member");

    let empty_texmf = tempdir("arxiv-export-empty-texmf");
    let run_xelatex = || {
        crate::common::engine_command(engine.xelatex.as_deref().expect("xelatex resolvable"), &engine.env)
            .args(["-interaction=nonstopmode", "-halt-on-error", "paper.tex"])
            .current_dir(&clean_env)
            .env("TEXMFHOME", &empty_texmf)
            .env("TEXMFVAR", &empty_texmf)
            .status()
            .expect("xelatex must be runnable directly")
    };
    assert!(run_xelatex().success(), "first xelatex pass must succeed on the extracted package alone");
    let biber_status = crate::common::engine_command(engine.biber.as_deref().expect("biber resolvable"), &engine.env)
        .arg("paper")
        .current_dir(&clean_env)
        .env("TEXMFHOME", &empty_texmf)
        .env("TEXMFVAR", &empty_texmf)
        .status()
        .expect("biber must be runnable directly");
    assert!(biber_status.success(), "biber must resolve the packaged references.bib with no network access");
    assert!(run_xelatex().success());
    assert!(run_xelatex().success());
    assert!(clean_env.join("paper.pdf").is_file(), "the clean-room compile must produce a real PDF");
    let pdf_bytes = fs::read(clean_env.join("paper.pdf")).unwrap();
    assert!(pdf_bytes.len() > 500, "the produced PDF must have real content, not an empty/near-empty shell");
}

#[test]
#[ignore = "requires a local XeLaTeX + Biber distribution"]
fn test_include_bbl_matching_stem_compiles_in_second_isolated_environment() {
    let _guard = PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _engine_guard = ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let engine = crate::common::resolved_engine();

    // Produce a genuine, locally-generated `paper.bbl`: `terse build`
    // itself only ever publishes the final PDF (its scratch compile
    // directory, `.bbl` included, is disposable by design -- see group
    // 20), so a real `.bbl` to feed `--include-bbl` has to come from an
    // actual manual xelatex/biber pass over the tex-only source output,
    // exactly as a human preparing one by hand would.
    let source_tmp = tempdir("include-bbl-source");
    copy_fixture_into(&source_tmp);
    let code = terse_cli::run(["terse", "build", "--tex-only"], &source_tmp);
    assert_eq!(code, 0);
    let generated = source_tmp.join("build").join("academic");
    let manual_compile = tempdir("include-bbl-manual-compile");
    copy_dir_all(&generated, &manual_compile);
    let run_xelatex = |dir: &std::path::Path| {
        crate::common::engine_command(engine.xelatex.as_deref().expect("xelatex resolvable"), &engine.env)
            .args(["-interaction=nonstopmode", "-halt-on-error", "paper.tex"])
            .current_dir(dir)
            .status()
            .expect("xelatex must be runnable directly")
    };
    assert!(run_xelatex(&manual_compile).success());
    let biber_status = crate::common::engine_command(engine.biber.as_deref().expect("biber resolvable"), &engine.env)
        .arg("paper")
        .current_dir(&manual_compile)
        .status()
        .expect("biber must be runnable directly");
    assert!(biber_status.success());
    assert!(run_xelatex(&manual_compile).success());
    let bbl_path = manual_compile.join("paper.bbl");
    assert!(bbl_path.is_file(), "the manual biber pass must produce a real paper.bbl");

    // Second, wholly separate project copy: export it with the manually
    // produced, verified-compatible `.bbl` included.
    let tmp = tempdir("include-bbl-export");
    copy_fixture_into(&tmp);
    let code = terse_cli::run(
        vec![
            "terse".to_string(),
            "export".to_string(),
            "--target".to_string(),
            "arxiv".to_string(),
            "--require-compile".to_string(),
            "--include-bbl".to_string(),
            bbl_path.to_string_lossy().into_owned(),
        ],
        &tmp,
    );
    assert_eq!(code, 0, "a verified matching-stem real bbl must be accepted and the package must still compile");
    let export_dir = tmp.join("build").join("export").join("academic").join("arxiv");
    assert!(export_dir.join("paper.bbl").is_file(), "the verified bbl must actually be included in the export");

    // Third, separately-constructed clean room: extract and compile the
    // include-bbl archive with plain xelatex only (no biber needed, since
    // the bibliography is already pre-resolved).
    let zip_path = tmp.join("build").join("export").join("academic").join("paper-arxiv.zip");
    let zip_bytes = fs::read(&zip_path).unwrap();
    let clean_env = tempdir("include-bbl-clean-room");
    terse_cli::export::validate::extract_zip(&zip_bytes, &clean_env).unwrap();
    let empty_texmf = tempdir("include-bbl-empty-texmf");
    let status = crate::common::engine_command(engine.xelatex.as_deref().expect("xelatex resolvable"), &engine.env)
        .args(["-interaction=nonstopmode", "-halt-on-error", "paper.tex"])
        .current_dir(&clean_env)
        .env("TEXMFHOME", &empty_texmf)
        .env("TEXMFVAR", &empty_texmf)
        .status()
        .expect("xelatex must be runnable directly");
    assert!(status.success(), "the include-bbl export must compile in a second, separately-constructed clean room");
    assert!(clean_env.join("paper.pdf").is_file());
}

#[test]
#[ignore = "requires a local XeLaTeX + Biber distribution"]
fn test_removing_a_required_asset_prevents_compilation_in_clean_environment() {
    // Group 24.5's negative control: deleting a packaged asset the
    // document actually needs must make the clean-room compile fail,
    // proving no ambient host resource (a leftover file, a system font
    // substitution) can silently repair the missing closure.
    let _guard = PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _engine_guard = ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let engine = crate::common::resolved_engine();

    let tmp = tempdir("missing-asset-source");
    copy_fixture_into(&tmp);
    let code = terse_cli::run(["terse", "export", "--target", "arxiv"], &tmp);
    assert_eq!(code, 0);

    let zip_path = tmp.join("build").join("export").join("academic").join("paper-arxiv.zip");
    let zip_bytes = fs::read(&zip_path).unwrap();
    let clean_env = tempdir("missing-asset-clean-room");
    terse_cli::export::validate::extract_zip(&zip_bytes, &clean_env).unwrap();

    let figure = fs::read_dir(clean_env.join("assets"))
        .ok()
        .and_then(|mut d| d.next())
        .map(|e| e.unwrap().path())
        .unwrap_or_else(|| clean_env.join("assets").join("figure.png"));
    if figure.is_file() {
        fs::remove_file(&figure).unwrap();
    }

    let empty_texmf = tempdir("missing-asset-empty-texmf");
    let status = crate::common::engine_command(engine.xelatex.as_deref().expect("xelatex resolvable"), &engine.env)
        .args(["-interaction=nonstopmode", "-halt-on-error", "paper.tex"])
        .current_dir(&clean_env)
        .env("TEXMFHOME", &empty_texmf)
        .env("TEXMFVAR", &empty_texmf)
        .status()
        .expect("xelatex must be runnable directly");
    assert!(!status.success(), "removing a required packaged asset must break the clean-room compile, not repair itself");
}
