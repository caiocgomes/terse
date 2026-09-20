//! `terse export --target arxiv` / `terse check --target arxiv`: offline,
//! current-input validation against the versioned `texlive-2025-xelatex`
//! compatibility profile (group 22). No network transport is ever
//! constructed by any test in this file. Building the full portable
//! ZIP/manifest/allowlist and its determinism guarantees is group 23's
//! job -- these tests only exercise the staged source-directory
//! membership already well-defined at this stage (default `.bib`, no
//! prebuilt `.bbl` unless verified).

use std::fs;
use std::path::PathBuf;

const MANIFEST: &str = "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n";

fn tempdir(label: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "terse-arxiv-export-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &std::path::Path, contents: impl AsRef<[u8]>) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn export_dir(tmp: &std::path::Path) -> PathBuf {
    tmp.join("build").join("export").join("academic").join("arxiv")
}

#[test]
fn test_export_validates_current_sources() {
    let tmp = tempdir("current-sources");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(&tmp.join("paper.trs"), "document:\n  title: \"T\"\n\nA valid paragraph.\n");

    let code = terse_cli::run(["terse", "export", "--target", "arxiv"], &tmp);
    assert_eq!(code, 0, "a valid document must export cleanly");
    let before = fs::read(export_dir(&tmp).join("paper.tex")).unwrap();

    // Break the source with an unresolved cross-reference after a
    // successful export.
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\nSee {ref: nowhere} for details.\n",
    );
    let code = terse_cli::run(["terse", "export", "--target", "arxiv"], &tmp);
    assert_ne!(code, 0, "export must fail on current invalid input rather than archive the old build");

    let after = fs::read(export_dir(&tmp).join("paper.tex")).unwrap();
    assert_eq!(before, after, "a failed export must never replace the previous successful generation");

    // `check --target arxiv` shares the same current-input validation.
    let code = terse_cli::run(["terse", "check", "--target", "arxiv"], &tmp);
    assert_ne!(code, 0, "check --target arxiv must also validate current sources, not a cached plan");
}

#[test]
fn test_external_or_missing_export_dependency_fails() {
    // Missing figure.
    let tmp = tempdir("missing-dependency");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("paper.trs"),
        concat!(
            "document:\n  title: \"T\"\n\n",
            "figure \"figs/missing.png\" [id: fig-1]:\n",
            "  caption: Missing.\n",
            "  alt: Missing.\n",
        ),
    );
    let code = terse_cli::run(["terse", "export", "--target", "arxiv"], &tmp);
    assert_ne!(code, 0, "a missing figure dependency must fail export before any staging");
    assert!(!export_dir(&tmp).exists(), "no export directory must be created on failure");

    // Escaping the project root.
    let tmp2 = tempdir("escaping-dependency");
    write(&tmp2.join("terse.toml"), MANIFEST);
    write(
        &tmp2.join("paper.trs"),
        concat!(
            "document:\n  title: \"T\"\n\n",
            "figure \"../outside.png\" [id: fig-1]:\n",
            "  caption: Outside.\n",
            "  alt: Outside.\n",
        ),
    );
    let code = terse_cli::run(["terse", "export", "--target", "arxiv"], &tmp2);
    assert_ne!(code, 0, "a figure path escaping the project root must fail export");
    assert!(!export_dir(&tmp2).exists());

    // An unknown declared package (never guaranteed by any profile).
    let tmp3 = tempdir("unsupported-package");
    write(
        &tmp3.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n\n[latex]\npackages = [\"does-not-exist\"]\n",
    );
    write(&tmp3.join("paper.trs"), "document:\n  title: \"T\"\n\nA paragraph.\n");
    let code = terse_cli::run(["terse", "export", "--target", "arxiv"], &tmp3);
    assert_ne!(code, 0, "an unknown/unsupported declared package must fail export");
}

#[test]
fn test_raw_export_rejection_preserves_content() {
    let tmp = tempdir("raw-rejection");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("paper.trs"),
        concat!(
            "document:\n  title: \"T\"\n\n",
            "A drawing, compiled normally.\n\n",
            "tex:\n  \\begin{tikzpicture}\n    \\draw (0,0) -- (1,1);\n  \\end{tikzpicture}\n",
        ),
    );

    // A normal (non-export) build of the same raw content succeeds --
    // export's rejection of raw `tex:` is specific to the MVP export
    // target, not a general restriction.
    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 0, "normal-build support for raw TeX must remain unaffected by the export policy");
    let build_tex_before = fs::read(tmp.join("build").join("academic").join("paper.tex")).unwrap();

    let code = terse_cli::run(["terse", "export", "--target", "arxiv"], &tmp);
    assert_ne!(code, 0, "a raw `tex:` block must fail the MVP arXiv export target");
    assert!(!export_dir(&tmp).exists(), "export must never silently drop the raw block to pass");

    // The normal build's own output and the authored source are both
    // untouched by the failed export attempt.
    let build_tex_after = fs::read(tmp.join("build").join("academic").join("paper.tex")).unwrap();
    assert_eq!(build_tex_before, build_tex_after);
    let source_after = fs::read_to_string(tmp.join("paper.trs")).unwrap();
    assert!(source_after.contains("tikzpicture"), "the authored raw block must remain in source, never stripped");
}

#[test]
fn test_default_export_omits_prebuilt_bbl() {
    let tmp = tempdir("default-no-bbl");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("references.lock"),
        "lock-version = 1\nnormalization-version = 1\n\n\
         [entries.turing1936.declared]\nprovider = \"doi\"\nidentifier = \"10.1000/turing\"\n\n\
         [entries.turing1936.resolved]\nprovider = \"doi\"\nidentifier = \"10.1000/turing\"\n\n\
         [entries.turing1936.adapter]\nname = \"doi\"\nversion = \"1\"\n\n\
         [entries.turing1936.provider_data]\ntitle = \"On Computable Numbers\"\nwork_type = \"journal-article\"\nanonymous = true\n\n\
         [entries.turing1936.effective]\ntitle = \"On Computable Numbers\"\nwork_type = \"journal-article\"\nanonymous = true\n",
    );
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\nrefs:\n  turing1936: doi:10.1000/turing\n\nSee @turing1936 for details.\n",
    );

    // A prior normal build with a real local `.bbl` sitting in its output
    // directory must not leak into export by accident.
    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 0);
    write(&tmp.join("build").join("academic").join("paper.bbl"), b"% a locally generated bbl, irrelevant to export");

    let code = terse_cli::run(["terse", "export", "--target", "arxiv"], &tmp);
    assert_eq!(code, 0, "a cited, locked reference must export cleanly by default");

    let dir = export_dir(&tmp);
    assert!(dir.join("references.bib").is_file(), "default export must include the generated .bib");
    assert!(!dir.join("paper.bbl").exists(), "default export must never include a prebuilt .bbl");
}

#[test]
fn test_include_bbl_requires_verified_compatibility() {
    let tmp = tempdir("include-bbl");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("references.lock"),
        "lock-version = 1\nnormalization-version = 1\n\n\
         [entries.turing1936.declared]\nprovider = \"doi\"\nidentifier = \"10.1000/turing\"\n\n\
         [entries.turing1936.resolved]\nprovider = \"doi\"\nidentifier = \"10.1000/turing\"\n\n\
         [entries.turing1936.adapter]\nname = \"doi\"\nversion = \"1\"\n\n\
         [entries.turing1936.provider_data]\ntitle = \"On Computable Numbers\"\nwork_type = \"journal-article\"\nanonymous = true\n\n\
         [entries.turing1936.effective]\ntitle = \"On Computable Numbers\"\nwork_type = \"journal-article\"\nanonymous = true\n",
    );
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\nrefs:\n  turing1936: doi:10.1000/turing\n\nSee @turing1936 for details.\n",
    );

    let run_include_bbl = |tmp: &std::path::Path, bbl: &std::path::Path| -> i32 {
        terse_cli::run(
            vec![
                "terse".to_string(),
                "export".to_string(),
                "--target".to_string(),
                "arxiv".to_string(),
                "--include-bbl".to_string(),
                bbl.to_string_lossy().into_owned(),
            ],
            tmp,
        )
    };

    // Missing file.
    let code = run_include_bbl(&tmp, &tmp.join("does-not-exist.bbl"));
    assert_ne!(code, 0, "a missing --include-bbl file must fail explicitly");

    // Wrong main-stem.
    write(&tmp.join("other.bbl"), b"\\ProvidesFile{other.bbl}\n% biblatex generated bbl\n");
    let code = run_include_bbl(&tmp, &tmp.join("other.bbl"));
    assert_ne!(code, 0, "a .bbl whose stem does not match the main tex stem must be rejected, never renamed");

    // Right stem, unverifiable content (no recognizable biblatex/biber
    // marker).
    write(&tmp.join("paper.bbl"), b"this file has no recognizable bibliography marker at all\n");
    let code = run_include_bbl(&tmp, &tmp.join("paper.bbl"));
    assert_ne!(code, 0, "unverifiable bibliography content must be rejected, not trusted");
    assert!(!export_dir(&tmp).exists(), "no export must be published for an unverifiable requested bbl");

    // Right stem, verified marker: succeeds and is actually included.
    write(&tmp.join("paper.bbl"), b"% biblatex generated bibliography file\n\\ProvidesFile{paper.bbl}\n");
    let code = run_include_bbl(&tmp, &tmp.join("paper.bbl"));
    assert_eq!(code, 0, "a verified matching-stem bbl must be accepted");
    assert!(export_dir(&tmp).join("paper.bbl").is_file(), "the verified bbl must actually be included in the export");
}

#[test]
fn test_export_rejects_unknown_profile_and_target() {
    let tmp = tempdir("unknown-target");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(&tmp.join("paper.trs"), "document:\n  title: \"T\"\n\nA paragraph.\n");

    let code = terse_cli::run(["terse", "export", "--target", "not-a-real-target"], &tmp);
    assert_ne!(code, 0, "an unknown export target must fail cleanly");
}

fn zip_entry_names(bytes: &[u8]) -> Vec<String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
    (0..archive.len()).map(|i| archive.by_index(i).unwrap().name().to_string()).collect()
}

fn zip_path(tmp: &std::path::Path) -> PathBuf {
    tmp.join("build").join("export").join("academic").join("paper-arxiv.zip")
}

#[test]
fn test_archive_paths_are_portable() {
    let tmp = tempdir("portable-paths");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("paper.trs"),
        concat!(
            "document:\n  title: \"T\"\n\n",
            "figure \"figs/plot.png\" [id: fig-1]:\n",
            "  caption: A plot.\n",
            "  alt: A plot.\n",
        ),
    );
    write(&tmp.join("figs/plot.png"), b"not a real png but bytes suffice for this check");

    let code = terse_cli::run(["terse", "export", "--target", "arxiv"], &tmp);
    assert_eq!(code, 0);

    let names = zip_entry_names(&fs::read(zip_path(&tmp)).unwrap());
    for name in &names {
        assert!(!name.starts_with('/'), "no absolute member path: {name}");
        assert!(!name.contains(".."), "no traversal member path: {name}");
        assert!(!name.starts_with('\\') && !name.contains(":\\"), "no drive/UNC-shaped member path: {name}");
    }
    // Case-fold uniqueness: no two members collide on a case-insensitive
    // filesystem even though this one is case-sensitive.
    let mut folded: Vec<String> = names.iter().map(|n| n.to_ascii_lowercase()).collect();
    let before = folded.len();
    folded.sort();
    folded.dedup();
    assert_eq!(before, folded.len(), "no case-fold collisions among archive members");
}

#[test]
fn test_source_map_is_excluded_from_export() {
    // The export target forbids maps and reports. `paper.map.json` is a
    // deliverable of an ordinary build, so it has to be filtered out here
    // explicitly, alongside the instructions and the internal manifest.
    let tmp = tempdir("export-no-map");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(&tmp.join("paper.trs"), "document:\n  title: \"T\"\n\nA paragraph.\n");

    assert_eq!(terse_cli::run(["terse", "export", "--target", "arxiv"], &tmp), 0);

    let dir = export_dir(&tmp);
    assert!(
        !dir.join("paper.map.json").exists(),
        "a generated-to-source map is not part of a portable package"
    );
    let names = zip_entry_names(&fs::read(zip_path(&tmp)).unwrap());
    assert!(!names.contains(&"paper.map.json".to_string()), "archive members: {names:?}");
}

#[test]
fn test_export_allowlist_excludes_cruft() {
    let tmp = tempdir("allowlist-cruft");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(&tmp.join("paper.trs"), "document:\n  title: \"T\"\n\nA paragraph.\n");

    // The scenario names four kinds of cruft, and this test used to plant
    // none of them: it asserted only that two generated files were absent,
    // which the allowlist would satisfy even if it archived the whole
    // project directory alongside them. Plant all four, in the project
    // root and in a subdirectory, so a recursive walk cannot pass.
    write(&tmp.join("unused-photo.png"), b"\x89PNG\r\n\x1a\n not referenced by any figure");
    write(&tmp.join("paper.trs~"), b"editor backup of the entry");
    write(&tmp.join("notes/scratch.log"), b"a build log that must not travel");
    write(&tmp.join("previous-render.pdf"), b"%PDF-1.4 a rendered paper, not a figure");

    let code = terse_cli::run(["terse", "export", "--target", "arxiv"], &tmp);
    assert_eq!(code, 0);

    let dir = export_dir(&tmp);
    assert!(dir.join("paper.tex").is_file(), "generated main source must be included");
    assert!(dir.join("terse-style.sty").is_file(), "generated style file must be included");
    assert!(!dir.join("COMPILE.txt").exists(), "human build instructions are not part of a portable package");
    assert!(!dir.join("build-manifest.json").exists(), "the internal determinism manifest is not the export manifest");
    assert!(dir.join("MANIFEST.json").is_file(), "the export's own MANIFEST.json must be present");

    let names = zip_entry_names(&fs::read(zip_path(&tmp)).unwrap());
    assert!(names.contains(&"paper.tex".to_string()));
    assert!(!names.contains(&"COMPILE.txt".to_string()));
    assert!(!names.contains(&"build-manifest.json".to_string()));
    assert!(names.contains(&"MANIFEST.json".to_string()));

    // Exact membership against an independently written expected set: the
    // archive is what the allowlist admits, never what happens to sit in
    // the project directory.
    let expected: std::collections::BTreeSet<String> = ["MANIFEST.json", "paper.tex", "references.bib", "terse-style.sty"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let actual: std::collections::BTreeSet<String> = names.iter().cloned().collect();
    assert_eq!(actual, expected, "the archive must contain exactly the allowlisted members");

    for planted in ["unused-photo.png", "paper.trs~", "notes/scratch.log", "previous-render.pdf", "paper.trs"] {
        assert!(
            !names.iter().any(|n| n == planted || n.ends_with(planted)),
            "{planted} must never reach the archive; members were {names:?}"
        );
        assert!(
            !dir.join(planted).exists(),
            "{planted} must never reach the staged export directory either"
        );
    }

    // A used PDF figure is content, not the rendered paper output, and
    // must be retained.
    let tmp2 = tempdir("allowlist-pdf-figure");
    write(&tmp2.join("terse.toml"), MANIFEST);
    write(
        &tmp2.join("paper.trs"),
        concat!(
            "document:\n  title: \"T\"\n\n",
            "figure \"figs/plot.pdf\" [id: fig-1]:\n",
            "  caption: A plot.\n",
            "  alt: A plot.\n",
        ),
    );
    write(&tmp2.join("figs/plot.pdf"), b"%PDF-1.4 fake but sufficient for this check");
    let code = terse_cli::run(["terse", "export", "--target", "arxiv"], &tmp2);
    assert_eq!(code, 0);
    let names2 = zip_entry_names(&fs::read(zip_path(&tmp2)).unwrap());
    assert!(
        names2.iter().any(|n| n.ends_with("plot.pdf")),
        "a used PDF figure asset must be retained in the export, unlike the rendered paper PDF"
    );
}

#[test]
fn test_zip_bytes_and_manifest_are_deterministic() {
    let tmp_a = tempdir("determinism-a");
    write(&tmp_a.join("terse.toml"), MANIFEST);
    write(&tmp_a.join("paper.trs"), "document:\n  title: \"T\"\n\nA paragraph.\n");
    let code = terse_cli::run(["terse", "export", "--target", "arxiv"], &tmp_a);
    assert_eq!(code, 0);
    let zip_a = fs::read(zip_path(&tmp_a)).unwrap();
    let manifest_a = fs::read(export_dir(&tmp_a).join("MANIFEST.json")).unwrap();

    // Touch source files to an arbitrary different mtime, then re-export
    // into a fresh independent root with identical content.
    let tmp_b = tempdir("determinism-b");
    write(&tmp_b.join("terse.toml"), MANIFEST);
    write(&tmp_b.join("paper.trs"), "document:\n  title: \"T\"\n\nA paragraph.\n");
    let far_past = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    let far_future = std::time::SystemTime::now() + std::time::Duration::from_secs(1_000_000);
    filetime::set_file_mtime(tmp_b.join("terse.toml"), filetime::FileTime::from_system_time(far_past)).unwrap();
    filetime::set_file_mtime(tmp_b.join("paper.trs"), filetime::FileTime::from_system_time(far_future)).unwrap();

    let code = terse_cli::run(["terse", "export", "--target", "arxiv"], &tmp_b);
    assert_eq!(code, 0);
    let zip_b = fs::read(zip_path(&tmp_b)).unwrap();
    let manifest_b = fs::read(export_dir(&tmp_b).join("MANIFEST.json")).unwrap();

    assert_eq!(zip_a, zip_b, "identical export inputs must produce byte-identical ZIP archives regardless of source mtimes");
    assert_eq!(manifest_a, manifest_b, "MANIFEST.json must be byte-identical for identical export inputs");
    assert!(!String::from_utf8_lossy(&manifest_a).contains("MANIFEST.json"), "the manifest excludes its own hash");

    // Re-exporting the SAME root a second time (no source change at all)
    // must also be byte-identical.
    let code = terse_cli::run(["terse", "export", "--target", "arxiv"], &tmp_a);
    assert_eq!(code, 0);
    let zip_a_again = fs::read(zip_path(&tmp_a)).unwrap();
    assert_eq!(zip_a, zip_a_again);
}

/// process-wide lock: PATH mutation in one test must not race another
/// test's real subprocess spawns (`engine::find_tool` reads live `PATH`).
static PATH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Group 24 scenario: no compatible engine is available. A normal
/// export must still succeed, reporting `static-only`; the same export
/// with `--require-compile` must fail instead, leaving the previous
/// successful generation untouched.
#[test]
fn test_no_engine_reports_static_only_or_fails_required() {
    let _guard = PATH_LOCK.lock().unwrap();
    let tmp = tempdir("no-engine-export");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(&tmp.join("paper.trs"), "document:\n  title: \"T\"\n\nA paragraph.\n");

    let real_path = std::env::var("PATH").unwrap_or_default();
    let empty_path_dir = tempdir("no-engine-empty-path");
    std::env::set_var("PATH", &empty_path_dir);

    let code = terse_cli::run(["terse", "export", "--target", "arxiv"], &tmp);
    assert_eq!(code, 0, "normal export must degrade to static-only, not fail, when no engine is available");
    let report_path = tmp
        .join("build")
        .join("export")
        .join("academic")
        .join("paper-arxiv-report.txt");
    let report = fs::read_to_string(&report_path).unwrap();
    assert!(report.contains("static-only"), "report was: {report}");
    let sentinel = fs::read(export_dir(&tmp).join("paper.tex")).unwrap();

    let code = terse_cli::run(
        ["terse", "export", "--target", "arxiv", "--require-compile", "--toolchain", "system"],
        &tmp,
    );
    std::env::set_var("PATH", &real_path);
    assert_ne!(code, 0, "--require-compile must fail outright without a compatible engine");
    let after = fs::read(export_dir(&tmp).join("paper.tex")).unwrap();
    assert_eq!(sentinel, after, "a failed required-compile export must never replace the previous generation");
}

/// Group 24 scenario: the local toolchain compiles successfully but is
/// not verified to match the target profile's exact assumptions --
/// `compiled-local`, never `compiled-profile`, and never an acceptance
/// claim.
#[test]
fn test_local_compile_is_not_claimed_as_profile_match() {
    let _guard = PATH_LOCK.lock().unwrap();
    let dest = tempdir("profile-mismatch-dest");
    write(&dest.join("paper.tex"), b"content");
    let profile = terse_core::artifact::profile::resolve_profile("texlive-2025-xelatex").unwrap();

    let mut runner = terse_cli::engine::FakeProcessRunner::new(vec![
        terse_cli::engine::ProcessOutcome::success(),
        terse_cli::engine::ProcessOutcome::success_with_log(
            "XeTeX 3.14159265-2.6-0.999995 (TeX Live 2022)\n".as_bytes().to_vec(),
        ),
    ]);
    let (tc, host) = fake_toolchain(&tempdir("profile-mismatch-tools"));
    let report = terse_cli::export::validate::compile_extracted(
        &dest, "paper", false, false, &profile, &tc, &host, &mut runner,
    )
    .expect("a scripted successful compile must not itself be an error");
    assert!(matches!(report, terse_cli::export::validate::CompileReport::CompiledLocal { .. }));
    let rendered = report.render(&profile.name);
    assert!(rendered.contains("compiled-local"));
    assert!(!rendered.contains("compiled-profile"));
    assert!(!rendered.to_lowercase().contains("will be accepted"));
}

/// Group 24 scenario: a clean compilation uncovers a hidden dependency --
/// the recorder log names a path outside both the extracted package and
/// any recognized system TeX distribution resource, which must be a hard
/// validation failure with an actionable explanation.
#[test]
fn test_recorder_detects_hidden_dependency() {
    let _guard = PATH_LOCK.lock().unwrap();
    let dest = tempdir("hidden-dependency-dest");
    write(&dest.join("paper.tex"), b"content");
    let outside = tempdir("hidden-dependency-outside");
    let leaked = outside.join("secret-input.tex");
    write(&leaked, b"leaked content that was never packaged");
    write(
        &dest.join("paper.fls"),
        format!("PWD {}\nINPUT paper.tex\nINPUT {}\n", dest.display(), leaked.display()),
    );

    let profile = terse_core::artifact::profile::resolve_profile("texlive-2025-xelatex").unwrap();
    // One compile pass, then every `kpsewhich -var-value` probe reports an
    // unrelated root, so the leaked path is outside both the package and
    // the distribution.
    let mut responses = vec![terse_cli::engine::ProcessOutcome::success()];
    responses.extend(std::iter::repeat_n(
        terse_cli::engine::ProcessOutcome::success_with_log(b"/opt/texlive/2025\n".to_vec()),
        10,
    ));
    let mut runner = terse_cli::engine::FakeProcessRunner::new(responses);
    let (tc, host) = fake_toolchain(&tempdir("hidden-dependency-tools"));
    let result = terse_cli::export::validate::compile_extracted(
        &dest, "paper", false, false, &profile, &tc, &host, &mut runner,
    );
    match result {
        Err(terse_cli::export::validate::ValidateError::HiddenDependency(paths)) => {
            assert!(paths.iter().any(|p| p.contains("secret-input.tex")));
        }
        other => panic!("expected a HiddenDependency failure, got {other:?}"),
    }
}

/// A resolved toolchain whose executables are sentinel scripts in `dir`
/// (never run: every test here uses the fake runner) plus a host literal.
fn fake_toolchain(dir: &std::path::Path) -> (terse_cli::toolchain::ResolvedToolchain, terse_cli::toolchain::HostEnv) {
    fs::create_dir_all(dir).unwrap();
    for tool in ["xelatex", "biber", "kpsewhich"] {
        let path = dir.join(tool);
        fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    let host = terse_cli::toolchain::HostEnv::minimal(
        terse_cli::toolchain::HostOs::current(),
        dir.as_os_str().to_os_string(),
    );
    let tc = terse_cli::toolchain::resolve(&terse_cli::toolchain::ToolchainSelector::System, "2025", &host)
        .expect("system resolution never fails");
    (tc, host)
}

/// Group 1 of `managed-toolchain`: distribution roots for the hidden-
/// dependency check come from `kpsewhich` through the bounded runner,
/// with the prepared environment, never from a raw `Command`.
#[test]
fn test_kpsewhich_uses_bounded_runner() {
    let dest = tempdir("kpsewhich-runner-dest");
    write(&dest.join("paper.tex"), b"content");
    let outside = tempdir("kpsewhich-runner-outside");
    let leaked = outside.join("dist-input.tex");
    write(&leaked, b"provided by the distribution");
    write(
        &dest.join("paper.fls"),
        format!("PWD {}\nINPUT paper.tex\nINPUT {}\n", dest.display(), leaked.display()),
    );
    let profile = terse_core::artifact::profile::resolve_profile("texlive-2025-xelatex").unwrap();

    // The first probe reports `outside` as TEXMFROOT, so the leaked file
    // is a legitimate distribution resource and the compile passes.
    let mut responses = vec![terse_cli::engine::ProcessOutcome::success()];
    responses.push(terse_cli::engine::ProcessOutcome::success_with_log(
        format!("{}\n", outside.display()).into_bytes(),
    ));
    responses.extend(std::iter::repeat_n(terse_cli::engine::ProcessOutcome::nonzero(1), 9));
    responses.push(terse_cli::engine::ProcessOutcome::success_with_log(b"XeTeX (TeX Live 2025)\n".to_vec()));
    let mut runner = terse_cli::engine::FakeProcessRunner::new(responses);
    let (tc, host) = fake_toolchain(&tempdir("kpsewhich-runner-tools"));
    let report = terse_cli::export::validate::compile_extracted(
        &dest, "paper", false, false, &profile, &tc, &host, &mut runner,
    )
    .expect("a distribution-provided input is not hidden");
    assert!(matches!(report, terse_cli::export::validate::CompileReport::CompiledProfile));

    let probes: Vec<_> = runner
        .invocations
        .iter()
        .filter(|inv| inv.program.file_name().map(|n| n == "kpsewhich").unwrap_or(false))
        .filter(|inv| inv.args.first().map(String::as_str) == Some("-var-value"))
        .collect();
    assert_eq!(probes.len(), 10, "one bounded invocation per distribution variable");
    assert_eq!(probes[0].args, vec!["-var-value".to_string(), "TEXMFROOT".to_string()]);
    for inv in &probes {
        assert!(inv.env.iter().any(|(k, _)| k == "PATH"), "prepared environment carries PATH");
        assert!(inv.env.iter().any(|(k, _)| k == "TEXMFHOME"), "prepared environment carries TEXMFHOME");
    }
}

fn generation_dir(tmp: &std::path::Path) -> PathBuf {
    tmp.join("build").join("export").join("academic")
}

fn hash_file(path: &std::path::Path) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(fs::read(path).unwrap());
    hasher.finalize().into()
}

/// Group 25: the extracted directory, the sibling ZIP, and the
/// compilation report are one managed generation, published atomically
/// together -- never as three independent writes that could leave a
/// mixed (new-zip/old-report, or new-directory/old-zip) result behind.
/// A failure at any later stage (here, an unavailable required engine)
/// must leave every one of the three prior outputs byte-identical, not
/// merely "still present".
#[test]
fn test_export_transaction_preserves_consistent_generation() {
    let _guard = PATH_LOCK.lock().unwrap();
    let tmp = tempdir("transaction-consistent");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(&tmp.join("paper.trs"), "document:\n  title: \"T\"\n\nA paragraph.\n");

    let code = terse_cli::run(["terse", "export", "--target", "arxiv"], &tmp);
    assert_eq!(code, 0, "the first export must succeed");

    let gen_dir = generation_dir(&tmp);
    let tex_hash_before = hash_file(&gen_dir.join("arxiv").join("paper.tex"));
    let zip_hash_before = hash_file(&gen_dir.join("paper-arxiv.zip"));
    let report_hash_before = hash_file(&gen_dir.join("paper-arxiv-report.txt"));
    let manifest_hash_before = hash_file(&gen_dir.join("arxiv").join(
        terse_core::artifact::export::MANIFEST_FILE_NAME,
    ));

    // Force a later-stage failure: `--require-compile` with no engine on
    // PATH fails only after validation/staging has already built the
    // full candidate generation in memory, exactly the point at which a
    // non-transactional implementation could plausibly have written one
    // artifact and not the others.
    let real_path = std::env::var("PATH").unwrap_or_default();
    let empty_path_dir = tempdir("transaction-consistent-empty-path");
    std::env::set_var("PATH", &empty_path_dir);
    let code = terse_cli::run(
        ["terse", "export", "--target", "arxiv", "--require-compile", "--toolchain", "system"],
        &tmp,
    );
    std::env::set_var("PATH", &real_path);
    assert_ne!(code, 0, "a required compile with no engine available must fail, not silently degrade");

    assert_eq!(
        hash_file(&gen_dir.join("arxiv").join("paper.tex")),
        tex_hash_before,
        "a failed later-stage export must not touch the extracted directory at all"
    );
    assert_eq!(
        hash_file(&gen_dir.join("paper-arxiv.zip")),
        zip_hash_before,
        "a failed later-stage export must not touch the previous ZIP"
    );
    assert_eq!(
        hash_file(&gen_dir.join("paper-arxiv-report.txt")),
        report_hash_before,
        "a failed later-stage export must not touch the previous report"
    );
    assert_eq!(
        hash_file(&gen_dir.join("arxiv").join(terse_core::artifact::export::MANIFEST_FILE_NAME)),
        manifest_hash_before,
        "a failed later-stage export must not touch the previous manifest"
    );

    // Simulate a crash between the backup rename and the final install
    // rename of the ONE combined generation (directory + zip + report
    // are all staged under `gen_dir`'s parent as a single tree keyed by
    // `gen_dir`), then recover, following the exact pattern group 5's
    // `publication::recover` already proves for a single directory --
    // this asserts export composes with that mechanism rather than
    // needing its own.
    let staged = terse_cli::publication::stage(
        &gen_dir,
        &[("arxiv/paper.tex".to_string(), b"v2".to_vec())],
    )
    .unwrap();
    let parent = gen_dir.parent().unwrap();
    let backup_dir = parent.join(format!(".terse-backup-crash-sim-{}", std::process::id()));
    fs::rename(&gen_dir, &backup_dir).unwrap();
    fs::write(
        parent.join(".terse-publish-journal.json"),
        format!(
            "{{\"staging\": {:?}, \"final\": {:?}, \"backup\": {:?}, \"stage\": \"backed-up\"}}\n",
            staged.staging_dir, gen_dir, backup_dir
        ),
    )
    .unwrap();
    assert!(!gen_dir.exists(), "mid-crash: no complete generation must be visible yet");

    terse_cli::publication::recover(parent).unwrap();

    assert_eq!(
        fs::read(gen_dir.join("arxiv").join("paper.tex")).unwrap(),
        b"v2",
        "recovery must finish installing the staged generation"
    );
    assert!(!parent.join(".terse-publish-journal.json").exists());
    assert!(!backup_dir.exists(), "recovery must not leave a stale backup of a superseded generation");
}

/// `managed-toolchain` scenario "Banner year alone is not a profile
/// match": with a toolchain whose `xelatex --version` names the profile
/// year but which lacks one profile font, the report is `compiled-local`
/// naming the missing font among the unverified assumptions; with every
/// package, font, and babel definition resolvable, the same compile is
/// `compiled-profile`.
#[test]
fn test_compiled_profile_requires_verified_packages() {
    use terse_cli::engine::{FakeProcessRunner, ProcessInvocation, ProcessOutcome};
    use terse_cli::export::validate::{compile_extracted, CompileReport};

    let _guard = PATH_LOCK.lock().unwrap();
    let profile = terse_core::artifact::profile::resolve_profile("texlive-2025-xelatex").unwrap();
    let (tc, host) = fake_toolchain(&tempdir("verified-profile-tools"));

    // A runner that answers by argument: the compile pass succeeds, the
    // banner names the profile year, every `kpsewhich <file>` lookup
    // succeeds except the one for the pagella font file.
    struct ByArgs {
        inner: FakeProcessRunner,
        missing_file: &'static str,
    }
    impl terse_cli::engine::ProcessRunner for ByArgs {
        fn run(&mut self, inv: &ProcessInvocation, timeout: std::time::Duration) -> ProcessOutcome {
            self.inner.invocations.push(inv.clone());
            self.inner.timeouts.push(timeout);
            if inv.args.first().map(String::as_str) == Some("--version") {
                return ProcessOutcome::success_with_log(b"XeTeX 3.14 (TeX Live 2025)\n".to_vec());
            }
            if inv.program.file_name().map(|n| n == "kpsewhich").unwrap_or(false) {
                let file = inv.args.last().map(String::as_str).unwrap_or("");
                if file == self.missing_file {
                    return ProcessOutcome::nonzero(1);
                }
                return ProcessOutcome::success_with_log(format!("/dist/{file}\n").into_bytes());
            }
            ProcessOutcome::success()
        }
    }

    let dest = tempdir("verified-profile-missing-font");
    write(&dest.join("paper.tex"), b"content");
    let mut runner = ByArgs { inner: FakeProcessRunner::new(vec![]), missing_file: "texgyrepagella-regular.otf" };
    let report = compile_extracted(&dest, "paper", false, false, &profile, &tc, &host, &mut runner)
        .expect("a scripted successful compile is not an error");
    match &report {
        CompileReport::CompiledLocal { tested_assumptions } => {
            assert!(
                tested_assumptions.iter().any(|a| a.contains("tgpagella") || a.contains("texgyrepagella")),
                "the unverified font must be named: {tested_assumptions:?}"
            );
        }
        other => panic!("banner year alone must not yield compiled-profile, got {other:?}"),
    }
    let rendered = report.render(&profile.name);
    assert!(rendered.contains("compiled-local") && !rendered.contains("compiled-profile"));
    let lookups = runner
        .inner
        .invocations
        .iter()
        .filter(|inv| inv.program.file_name().map(|n| n == "kpsewhich").unwrap_or(false))
        .filter(|inv| inv.args.first().map(String::as_str) != Some("-var-value"))
        .count();
    assert!(
        lookups >= profile.packages.len() + profile.fonts.len() + profile.babel_languages.len(),
        "every profile package, font, and babel definition is probed through the runner, got {lookups}"
    );
    assert!(runner.inner.timeouts.iter().all(|t| *t <= std::time::Duration::from_secs(120)));

    let dest = tempdir("verified-profile-complete");
    write(&dest.join("paper.tex"), b"content");
    let mut runner = ByArgs { inner: FakeProcessRunner::new(vec![]), missing_file: "nothing-is-missing" };
    let report = compile_extracted(&dest, "paper", false, false, &profile, &tc, &host, &mut runner)
        .expect("a scripted successful compile is not an error");
    assert!(matches!(report, CompileReport::CompiledProfile), "got {report:?}");
}
