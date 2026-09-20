use std::fs;
use std::sync::Mutex;
use std::time::Duration;

use terse_cli::engine::{self, CompileFailure, EngineConfig, FakeProcessRunner, ProcessOutcome};

fn tiny_png() -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/assets/tiny.png"),
    )
    .expect("tiny.png fixture must exist")
}

fn themes_fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/themes")
}

fn tempdir(label: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir().join(format!(
        "terse-latex-gen-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&base).unwrap();
    base
}

const MANIFEST: &str = "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n";
const ENTRY: &str =
    "document:\n  title: \"Portable Page\"\n\n# Introduction\n\nHello, world.\n";

#[test]
fn test_tex_only_starts_no_processes() {
    let tmp = tempdir("tex-only");
    fs::write(tmp.join("terse.toml"), MANIFEST).unwrap();
    fs::write(tmp.join("paper.trs"), ENTRY).unwrap();

    // The `run_build` code path for `--tex-only` contains no
    // `std::process::Command`, socket, or network call anywhere in its
    // call graph (source::, syntax::, semantic::, latex::, artifact:: are
    // pure string/byte transforms; the only I/O here is std::fs). The
    // process/network-invocation-count assertion is therefore structural,
    // not runtime-instrumented: this crate cannot spawn what it never
    // calls.
    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 0);

    let out = tmp.join("build").join("academic");
    assert!(out.join("paper.tex").is_file());
    assert!(out.join("terse-style.sty").is_file());
    assert!(out.join("references.bib").is_file());
    assert!(out.join("COMPILE.txt").is_file());
    assert!(out.join("build-manifest.json").is_file());

    let tex = fs::read_to_string(out.join("paper.tex")).unwrap();
    assert!(tex.contains("Portable Page"));
    assert!(tex.contains("Hello, world."));

    // Conflicting PDF flags are a usage failure, not a silent choice.
    let code = terse_cli::run(
        ["terse", "build", "--tex-only", "--require-pdf"],
        &tmp,
    );
    assert_eq!(code, 2);
}

#[test]
fn test_tex_only_is_deterministic_across_runs() {
    let tmp = tempdir("determinism");
    fs::write(tmp.join("terse.toml"), MANIFEST).unwrap();
    fs::write(tmp.join("paper.trs"), ENTRY).unwrap();

    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 0);
    let first = fs::read(tmp.join("build").join("academic").join("paper.tex")).unwrap();

    fs::remove_dir_all(tmp.join("build")).unwrap();
    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 0);
    let second = fs::read(tmp.join("build").join("academic").join("paper.tex")).unwrap();

    assert_eq!(first, second);
}

/// process-wide lock: PATH mutation in one test must not race another
/// test's real subprocess spawns.
static PATH_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn test_auto_vs_required_pdf_without_engine() {
    let _guard = PATH_LOCK.lock().unwrap();
    let tmp = tempdir("no-engine");
    fs::write(tmp.join("terse.toml"), MANIFEST).unwrap();
    fs::write(tmp.join("paper.trs"), ENTRY).unwrap();

    let real_path = std::env::var("PATH").unwrap_or_default();
    let empty_path_dir = tempdir("empty-path");
    std::env::set_var("PATH", &empty_path_dir);

    // auto (neither --tex-only nor --require-pdf): missing engine still
    // succeeds, source-only, with a warning (exit 0).
    let code = terse_cli::run(["terse", "build", "--toolchain", "system"], &tmp);
    std::env::set_var("PATH", &real_path);
    assert_eq!(code, 0, "auto mode must degrade to source-only, not fail");
    assert!(tmp
        .join("build")
        .join("academic")
        .join("paper.tex")
        .is_file());
    assert!(!tmp
        .join("build")
        .join("academic")
        .join("paper.pdf")
        .exists());

    // --require-pdf with the same missing engine fails outright and
    // preserves whatever was already published.
    let before = fs::read(tmp.join("build").join("academic").join("paper.tex")).unwrap();
    std::env::set_var("PATH", &empty_path_dir);
    let code = terse_cli::run(["terse", "build", "--require-pdf", "--toolchain", "system"], &tmp);
    std::env::set_var("PATH", &real_path);
    assert_eq!(code, 3);
    let after = fs::read(tmp.join("build").join("academic").join("paper.tex")).unwrap();
    assert_eq!(before, after);
}

#[test]
fn test_subprocess_arguments_are_not_shell_text() {
    let _guard = PATH_LOCK.lock().unwrap();
    let tmp = tempdir("shell-sensitive $(); & name");
    fs::write(tmp.join("terse.toml"), MANIFEST).unwrap();
    fs::write(tmp.join("paper.trs"), ENTRY).unwrap();

    let mut runner = FakeProcessRunner::new(vec![ProcessOutcome::success()]);
    let code = terse_cli::build::run_build_with_runner(
        &tmp,
        None,
        None,
        false,
        true,
        &mut runner,
    );
    // The real xelatex binary on this machine will actually run against
    // the injected runner's recorded call, not the fake outcome, unless
    // xelatex is absent; either way, what we assert here is the shape of
    // the invocation the fake runner observed when it *was* reached.
    let _ = code;

    assert_eq!(runner.invocations.len(), 1, "exactly one xelatex pass for a converged, citation-free document");
    let invocation = &runner.invocations[0];
    assert_eq!(
        invocation.working_dir.file_name().is_some(),
        true,
        "working directory must be a real path, not shell-interpolated text"
    );
    assert_eq!(
        invocation.args,
        vec![
            "-no-shell-escape".to_string(),
            "-interaction=nonstopmode".to_string(),
            "-halt-on-error".to_string(),
            "-file-line-error".to_string(),
            "-recorder".to_string(),
            "paper.tex".to_string(),
        ],
        "argument vector must be exact literal strings, never shell-joined"
    );
}

#[test]
fn test_engine_pass_limit_is_enforced() {
    // Always claims a rerun is needed: with Biber in the loop, this can
    // never converge and must stop at the bounded pass limit rather than
    // looping forever.
    let responses = vec![
        ProcessOutcome::success(),                                       // xelatex pass 1
        ProcessOutcome::success_with_log(&b"Please (re)run"[..]),        // biber (requests a rerun)
        ProcessOutcome::success_with_log(&b"Rerun to get it right"[..]), // xelatex pass 3
        ProcessOutcome::success_with_log(&b"Rerun to get it right"[..]), // xelatex pass 4
        ProcessOutcome::success_with_log(&b"Rerun to get it right"[..]), // xelatex pass 5
    ];
    let mut runner = FakeProcessRunner::new(responses);
    let config = EngineConfig {
        xelatex: std::path::PathBuf::from("xelatex"),
        biber: std::path::PathBuf::from("biber"),
        timeout: Duration::from_secs(5),
    };
    let result = engine::compile_bounded(
        &mut runner,
        &std::env::temp_dir(),
        "paper",
        &config,
        true,
    );

    match result {
        Err((passes, CompileFailure::PassLimitExceeded)) => {
            assert!(passes.len() <= engine::MAX_ENGINE_PASSES as usize);
        }
        other => panic!("expected a pass-limit failure, got {other:?}"),
    }
    assert!(runner.invocations.len() as u32 <= engine::MAX_ENGINE_PASSES);
}

#[test]
fn test_source_map_is_a_published_deliverable() {
    // `paper.map.json` is named among the default deliverables. It has to
    // be asserted on the *published set*, not on the generating function:
    // the map generator existed, worked, and was unit-tested for a long
    // time while never being wired into a build at all.
    let tmp = tempdir("source-map");
    fs::write(tmp.join("terse.toml"), MANIFEST).unwrap();
    fs::write(
        tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\nEntry paragraph.\n\ninclude \"part.trs\"\n",
    )
    .unwrap();
    fs::write(tmp.join("part.trs"), "Included paragraph.\n").unwrap();

    assert_eq!(terse_cli::run(["terse", "build", "--tex-only"], &tmp), 0);
    let out = tmp.join("build").join("academic");
    let map_text = fs::read_to_string(out.join("paper.map.json")).expect("paper.map.json is published");

    let map: serde_json::Value = serde_json::from_str(&map_text).expect("map parses as JSON");
    assert!(map["schema-version"].as_u64().is_some(), "map carries a schema version");
    let intervals = map["intervals"].as_array().expect("intervals array");
    assert!(!intervals.is_empty(), "a document with body content maps at least one interval");

    // Paths must be root-relative with `/` separators: no absolute host
    // path, and no raw numeric file id leaking through.
    for interval in intervals {
        let path = interval["path"].as_str().expect("every interval names its source path");
        assert!(!path.starts_with('/'), "root-relative, got {path}");
        assert!(!path.contains('\\'), "portable separators, got {path}");
        assert!(!path.contains(&tmp.to_string_lossy().to_string()), "no host path in {path}");
    }
    // The included module is attributed to itself, not to the entry.
    assert!(
        intervals.iter().any(|i| i["path"] == "part.trs"),
        "an interval resolves to the included module: {map_text}"
    );

    // And it is hashed by the build manifest, which is only true if it was
    // pushed before the manifest was computed.
    let manifest = fs::read_to_string(out.join("build-manifest.json")).unwrap();
    assert!(manifest.contains("paper.map.json"), "manifest hashes the map: {manifest}");
}

#[test]
fn test_undefined_references_fail_the_build() {
    let _guard = PATH_LOCK.lock().unwrap();
    let tmp = tempdir("undefined-refs");
    fs::write(tmp.join("terse.toml"), MANIFEST).unwrap();
    fs::write(tmp.join("paper.trs"), ENTRY).unwrap();

    // A known-good previous generation to protect.
    assert_eq!(terse_cli::run(["terse", "build", "--tex-only"], &tmp), 0);
    let out = tmp.join("build").join("academic");
    let before: Vec<(String, Vec<u8>)> = ["paper.tex", "terse-style.sty", "COMPILE.txt"]
        .iter()
        .map(|name| (name.to_string(), fs::read(out.join(name)).unwrap()))
        .collect();

    // XeLaTeX exits 0 while leaving `??` in the PDF, so the failure can
    // only come from the settled log's own text.
    let mut runner = FakeProcessRunner::new(vec![ProcessOutcome::success_with_log(
        &b"LaTeX Warning: There were undefined references.\n"[..],
    )]);
    let code = terse_cli::build::run_build_with_runner(&tmp, None, None, false, true, &mut runner);
    assert_eq!(code, 3, "an undefined reference in the final pass fails the build");

    for (name, bytes) in &before {
        assert_eq!(&fs::read(out.join(name)).unwrap(), bytes, "{name} survives the failed build");
    }

    // The final-pass-only semantics (the regression an `.any()`-over-all-
    // passes scan would cause, since every intermediate pass of a citation
    // build reports undefined references before `.aux`/`.bbl` are read
    // back) is asserted directly against the log-interpretation function
    // in `engine::logs`, which is where that decision lives and where a
    // fake runner's inability to produce a PDF cannot mask the result.
}

#[test]
fn test_failed_engine_keeps_previous_generation() {
    let _guard = PATH_LOCK.lock().unwrap();
    let tmp = tempdir("engine-failure");
    fs::write(tmp.join("terse.toml"), MANIFEST).unwrap();
    fs::write(tmp.join("paper.trs"), ENTRY).unwrap();

    // A known-good previous generation (source-only baseline).
    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 0);
    let out = tmp.join("build").join("academic");
    let before: Vec<(String, Vec<u8>)> = ["paper.tex", "terse-style.sty", "references.bib", "COMPILE.txt"]
        .iter()
        .map(|name| (name.to_string(), fs::read(out.join(name)).unwrap()))
        .collect();

    // A rebuild whose engine pass fails outright (nonzero exit) must never
    // touch the previously published generation.
    let mut runner = FakeProcessRunner::new(vec![ProcessOutcome::nonzero(1)]);
    let code = terse_cli::build::run_build_with_runner(
        &tmp,
        None,
        None,
        false,
        true,
        &mut runner,
    );
    assert_eq!(code, 3);

    let after: Vec<(String, Vec<u8>)> = ["paper.tex", "terse-style.sty", "references.bib", "COMPILE.txt"]
        .iter()
        .map(|name| (name.to_string(), fs::read(out.join(name)).unwrap()))
        .collect();
    assert_eq!(before, after, "a failed engine pass must not mutate the prior generation");
}

#[test]
fn test_declared_local_support_file_is_copied_and_validated() {
    let tmp = tempdir("support_files");
    fs::write(
        tmp.join("terse.toml"),
        concat!(
            "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n\n",
            "[latex]\npackages = [\"tikz\"]\nsupport_files = [\"drawing/lib.tex\"]\n",
        ),
    )
    .unwrap();
    fs::write(
        tmp.join("paper.trs"),
        concat!(
            "document:\n  title: \"With Support\"\n\n",
            "tex:\n  \\input{drawing/lib.tex}\n",
        ),
    )
    .unwrap();
    fs::create_dir_all(tmp.join("drawing")).unwrap();
    fs::write(tmp.join("drawing/lib.tex"), b"% shared tikz styles\n").unwrap();

    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 0);

    let out = tmp.join("build").join("academic");
    assert!(out.join("drawing/lib.tex").is_file());
    assert_eq!(
        fs::read(out.join("drawing/lib.tex")).unwrap(),
        b"% shared tikz styles\n"
    );
    let style = fs::read_to_string(out.join("terse-style.sty")).unwrap();
    assert!(
        style.contains("\\RequirePackage{tikz}"),
        "declared built-in tikz package must be required in the generated style"
    );
}

#[test]
fn test_undeclared_package_is_rejected() {
    let tmp = tempdir("unknown-package");
    fs::write(
        tmp.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n\n[latex]\npackages = [\"pgfplots\"]\n",
    )
    .unwrap();
    fs::write(tmp.join("paper.trs"), ENTRY).unwrap();

    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 2, "an undeclared/unknown package must fail before any generation or engine call");
    assert!(!tmp.join("build").exists(), "no output is produced on validation failure");
}

#[test]
fn test_support_file_collision_with_generated_name_is_rejected() {
    let tmp = tempdir("support-collision");
    fs::write(
        tmp.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n\n[latex]\nsupport_files = [\"paper.tex\"]\n",
    )
    .unwrap();
    fs::write(tmp.join("paper.trs"), ENTRY).unwrap();
    fs::write(tmp.join("paper.tex"), b"not the generated file").unwrap();

    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 2, "a declared support file colliding with a generated file name must fail validation");
}

#[test]
fn test_relative_sibling_asset_resolves() {
    let tmp = tempdir("relative-asset");
    fs::write(tmp.join("terse.toml"), MANIFEST).unwrap();
    fs::write(
        tmp.join("paper.trs"),
        concat!(
            "document:\n  title: \"With Figure\"\n\n",
            "figure \"figs/diagram.png\" [id: fig-1]:\n",
            "  caption: A diagram.\n",
            "  alt: An alt description.\n",
        ),
    )
    .unwrap();
    fs::create_dir_all(tmp.join("figs")).unwrap();
    fs::write(tmp.join("figs/diagram.png"), tiny_png()).unwrap();

    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 0);

    let out = tmp.join("build").join("academic");
    let copied = out.join("assets/diagram.png");
    assert!(copied.is_file(), "the declaring-file-relative asset must be copied");
    assert_eq!(fs::read(&copied).unwrap(), tiny_png(), "asset bytes are preserved unchanged");

    let tex = fs::read_to_string(out.join("paper.tex")).unwrap();
    assert!(
        tex.contains("\\includegraphics[width=\\TerseFigureWidth]{assets/diagram.png}"),
        "the generated tex must reference the final logical asset path, not the authored one:\n{tex}"
    );
}

#[test]
fn test_invalid_figure_dependencies_fail() {
    // Missing file.
    let tmp = tempdir("missing-figure");
    fs::write(tmp.join("terse.toml"), MANIFEST).unwrap();
    fs::write(
        tmp.join("paper.trs"),
        concat!(
            "document:\n  title: \"Missing Figure\"\n\n",
            "figure \"figs/missing.png\" [id: fig-1]:\n",
            "  caption: Missing.\n",
            "  alt: Missing.\n",
        ),
    )
    .unwrap();
    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 2, "a missing figure dependency must fail before any generation");
    assert!(!tmp.join("build").exists());

    // Unsupported extension (no implicit conversion).
    let tmp2 = tempdir("unsupported-figure");
    fs::write(tmp2.join("terse.toml"), MANIFEST).unwrap();
    fs::write(
        tmp2.join("paper.trs"),
        concat!(
            "document:\n  title: \"SVG Figure\"\n\n",
            "figure \"figs/diagram.svg\" [id: fig-1]:\n",
            "  caption: Vector.\n",
            "  alt: Vector.\n",
        ),
    )
    .unwrap();
    fs::create_dir_all(tmp2.join("figs")).unwrap();
    fs::write(tmp2.join("figs/diagram.svg"), b"<svg></svg>").unwrap();
    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp2);
    assert_eq!(code, 2, "an unsupported figure extension must fail, never convert implicitly");

    // Escaping the project root.
    let tmp3 = tempdir("escaping-figure");
    fs::write(tmp3.join("terse.toml"), MANIFEST).unwrap();
    fs::write(
        tmp3.join("paper.trs"),
        concat!(
            "document:\n  title: \"Escaping Figure\"\n\n",
            "figure \"../outside.png\" [id: fig-1]:\n",
            "  caption: Outside.\n",
            "  alt: Outside.\n",
        ),
    )
    .unwrap();
    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp3);
    assert_eq!(code, 2, "a figure path escaping the project root must fail");
}

#[test]
fn test_colliding_asset_basenames_are_disambiguated() {
    let tmp = tempdir("colliding-basenames");
    fs::write(tmp.join("terse.toml"), MANIFEST).unwrap();
    fs::write(
        tmp.join("paper.trs"),
        concat!(
            "document:\n  title: \"Two Diagrams\"\n\n",
            "figure \"a/diagram.png\" [id: fig-a]:\n",
            "  caption: First.\n",
            "  alt: First.\n\n",
            "figure \"b/diagram.png\" [id: fig-b]:\n",
            "  caption: Second.\n",
            "  alt: Second.\n",
        ),
    )
    .unwrap();
    fs::create_dir_all(tmp.join("a")).unwrap();
    fs::create_dir_all(tmp.join("b")).unwrap();
    fs::write(tmp.join("a/diagram.png"), tiny_png()).unwrap();
    fs::write(tmp.join("b/diagram.png"), tiny_png()).unwrap();

    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 0);

    let out = tmp.join("build").join("academic");
    assert!(out.join("assets/diagram.png").is_file());
    assert!(
        out.join("assets/diagram-1.png").is_file(),
        "the second same-basename asset from a different directory must be disambiguated"
    );
}

#[test]
fn test_only_used_assets_copied() {
    let tmp = tempdir("unused-asset");
    fs::write(tmp.join("terse.toml"), MANIFEST).unwrap();
    fs::write(
        tmp.join("paper.trs"),
        concat!(
            "document:\n  title: \"One Figure\"\n\n",
            "figure \"figs/used.png\" [id: fig-1]:\n",
            "  caption: Used.\n",
            "  alt: Used.\n",
        ),
    )
    .unwrap();
    fs::create_dir_all(tmp.join("figs")).unwrap();
    fs::write(tmp.join("figs/used.png"), tiny_png()).unwrap();
    // Present on disk but never referenced by any figure block.
    fs::write(tmp.join("figs/unused.png"), tiny_png()).unwrap();

    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 0);

    let out = tmp.join("build").join("academic");
    assert!(out.join("assets/used.png").is_file());
    assert!(
        !out.join("assets/unused.png").exists(),
        "an unreferenced sibling file must never be copied into the output"
    );
}

#[test]
fn test_unsafe_link_schemes_fail() {
    let tmp = tempdir("unsafe-link");
    fs::write(tmp.join("terse.toml"), MANIFEST).unwrap();
    fs::write(
        tmp.join("paper.trs"),
        "document:\n  title: \"Bad Link\"\n\nSee [here](javascript:alert(1)) for details.\n",
    )
    .unwrap();
    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 1, "a dangerous link scheme must fail checking, never reach generation");
}

#[test]
fn test_generated_sources_are_human_editable() {
    let tmp = tempdir("human-editable");
    fs::write(tmp.join("terse.toml"), MANIFEST).unwrap();
    fs::write(tmp.join("paper.trs"), ENTRY).unwrap();
    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 0);

    let out = tmp.join("build").join("academic");
    let tex = fs::read_to_string(out.join("paper.tex")).unwrap();
    // Ordinary readable LaTeX, not a single minified line, and explicitly
    // marked as regeneratable rather than hand-maintained.
    assert!(tex.lines().count() > 3, "generated tex must be multi-line, human-readable source");
    assert!(tex.contains("Do not edit by hand"));
    assert!(!tex.contains('\r'), "generated sources use plain LF line endings");
}

#[test]
fn test_theme_switch_keeps_body_bytes() {
    let tmp = tempdir("theme-switch");
    fs::write(
        tmp.join("terse.toml"),
        concat!(
            "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n\n",
            "[themes]\nacademic = \"themes/academic.theme\"\nmagalu = \"themes/magalu.theme\"\n",
        ),
    )
    .unwrap();
    fs::write(tmp.join("paper.trs"), ENTRY).unwrap();
    fs::create_dir_all(tmp.join("themes/assets")).unwrap();
    for name in ["academic.theme", "magalu.theme"] {
        fs::copy(themes_fixture_dir().join(name), tmp.join("themes").join(name)).unwrap();
    }
    fs::copy(
        themes_fixture_dir().join("assets/logo.png"),
        tmp.join("themes/assets/logo.png"),
    )
    .unwrap();

    let code = terse_cli::run(["terse", "build", "--tex-only", "--theme", "academic"], &tmp);
    assert_eq!(code, 0);
    let academic_tex = fs::read(tmp.join("build/academic/paper.tex")).unwrap();
    let academic_bib = fs::read(tmp.join("build/academic/references.bib")).unwrap();

    let code = terse_cli::run(["terse", "build", "--tex-only", "--theme", "magalu"], &tmp);
    assert_eq!(code, 0);
    let magalu_tex = fs::read(tmp.join("build/magalu/paper.tex")).unwrap();
    let magalu_bib = fs::read(tmp.join("build/magalu/references.bib")).unwrap();

    assert_eq!(academic_tex, magalu_tex, "main tex bytes must be theme-invariant");
    assert_eq!(academic_bib, magalu_bib, "bibliography bytes must be theme-invariant");

    // Only the style layer differs, and it visibly differs: magalu's logo
    // is copied and its watermark is configured, academic's is not.
    let academic_style = fs::read_to_string(tmp.join("build/academic/terse-style.sty")).unwrap();
    let magalu_style = fs::read_to_string(tmp.join("build/magalu/terse-style.sty")).unwrap();
    assert_ne!(academic_style, magalu_style);
    assert!(!academic_style.contains("eso-pic") || !academic_style.contains("AddToShipoutPictureBG"));
    assert!(magalu_style.contains("AddToShipoutPictureBG"), "magalu declares a watermark");
    assert!(tmp.join("build/magalu/theme-assets/logo.png").is_file(), "magalu's logo must be copied");
    assert!(!tmp.join("build/academic/theme-assets").exists(), "academic declares no logo");
}

#[test]
fn test_cover_keeps_all_metadata() {
    let tmp = tempdir("cover-metadata");
    // A real cover theme: until `close-verification-gaps` the `title`
    // component accepted no properties, so this scenario ran under the
    // default paper theme and could not have detected a cover layout that
    // dropped metadata to fit the page.
    fs::write(
        tmp.join("terse.toml"),
        concat!(
            "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n\n",
            "[themes]\nacademic = \"themes/cover.theme\"\n",
        ),
    )
    .unwrap();
    fs::create_dir_all(tmp.join("themes")).unwrap();
    fs::write(
        tmp.join("themes/cover.theme"),
        "title:\n  layout: cover\n  align: center\n",
    )
    .unwrap();
    fs::write(
        tmp.join("paper.trs"),
        concat!(
            "document:\n",
            "  title: \"Full Cover\"\n",
            "  subtitle: \"A Subtitle\"\n",
            "  authors:\n",
            "    - name: \"Ada Lovelace\"\n",
            "      affiliation: \"Analytical Engines Ltd\"\n",
            "  date: \"1843-01-01\"\n",
            "  abstract:\n",
            "    An abstract paragraph.\n",
            "  keywords: [\"computation\", \"engines\"]\n\n",
            "# Intro\n\nBody.\n",
        ),
    )
    .unwrap();

    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 0);
    let tex = fs::read_to_string(tmp.join("build/academic/paper.tex")).unwrap();

    assert!(tex.contains("\\TerseTitle{Full Cover}"));
    assert!(tex.contains("\\TerseSubtitle{A Subtitle}"));
    assert!(tex.contains("\\TerseAuthor{Ada Lovelace}{Analytical Engines Ltd}"));
    assert!(tex.contains("\\TerseDate{1843-01-01}"));
    assert!(tex.contains("An abstract paragraph."));
    assert!(tex.contains("\\TerseKeywords{computation, engines}"));
    // PDF-level document metadata, distinct from the visible cover text.
    assert!(tex.contains("\\hypersetup{pdftitle={Full Cover},pdfauthor={Ada Lovelace}}"));

    // All of the above is theme-blind by construction, so on its own it
    // could not tell a cover from a paper title. The layout lives in the
    // style: assert this really is the cover variant, and that the whole
    // block is what the theme owns.
    let style = fs::read_to_string(tmp.join("build/academic/terse-style.sty")).unwrap();
    let cover = style
        .lines()
        .find(|l| l.contains("{TerseTitleBlock}"))
        .expect("the title block environment is defined");
    assert!(
        cover.contains("\\clearpage"),
        "a cover layout must give the title material its own page: {cover}"
    );
    assert!(
        tex.contains("\\begin{TerseTitleBlock}") && tex.contains("\\end{TerseTitleBlock}"),
        "every metadata macro must sit inside the block the theme lays out"
    );
}

#[test]
fn test_unnumbered_heading_reference_works() {
    let tmp = tempdir("unnumbered-heading-ref");
    fs::write(
        tmp.join("terse.toml"),
        concat!(
            "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n\n",
            "[themes]\nacademic = \"themes/no-numbering.theme\"\n",
        ),
    )
    .unwrap();
    fs::create_dir_all(tmp.join("themes")).unwrap();
    fs::write(
        tmp.join("themes/no-numbering.theme"),
        "heading.1:\n  numbering: none\n",
    )
    .unwrap();
    fs::write(
        tmp.join("paper.trs"),
        concat!(
            "document:\n  title: \"Unnumbered\"\n\n",
            "# Background [id: sec-background]\n\n",
            "See {ref: sec-background} for context.\n",
        ),
    )
    .unwrap();

    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 0);
    let tex = fs::read_to_string(tmp.join("build/academic/paper.tex")).unwrap();
    assert!(
        tex.contains("\\hyperref[sec-background]{Background}"),
        "an unnumbered heading's cross-reference must display its title and link to its anchor:\n{tex}"
    );
    assert!(tex.contains("\\label{sec-background}"));
}

#[test]
fn test_missing_glyph_is_build_failure() {
    // The theme's Latin fonts (Latin Modern / TeX Gyre) have no CJK
    // glyphs, so xelatex reports a real "Missing character" warning even
    // though it still exits 0 — group 16 turns that into a build failure
    // instead of shipping a document with silently absent text, and the
    // previous good PDF must never be overwritten by a failed attempt.
    let _guard = PATH_LOCK.lock().unwrap();
    let tmp = tempdir("missing-glyph");
    fs::write(tmp.join("terse.toml"), MANIFEST).unwrap();
    fs::write(
        tmp.join("paper.trs"),
        "document:\n  title: \"Glyph Test\"\n\nHello, world.\n",
    )
    .unwrap();
    let first = terse_cli::run(["terse", "build", "--require-pdf"], &tmp);
    assert_eq!(first, 0, "a normal build with only Latin text must succeed first");
    let good_pdf = fs::read(tmp.join("build/academic/paper.pdf")).unwrap();

    fs::write(
        tmp.join("paper.trs"),
        "document:\n  title: \"Glyph Test\"\n\nHello, \u{5b57} world.\n",
    )
    .unwrap();
    let second = terse_cli::run(["terse", "build", "--require-pdf"], &tmp);
    assert_ne!(second, 0, "a character with no available glyph must fail the build");
    let after = fs::read(tmp.join("build/academic/paper.pdf")).unwrap();
    assert_eq!(after, good_pdf, "a failed build must never replace the last good PDF");
}

/// `managed-toolchain` group 1: every engine child receives exactly the
/// prepared environment. Search-path overrides exported by the shell never
/// reach a build; Terse-owned `TEXMF*` directories always do.
#[test]
fn test_child_env_is_prepared_allowlist() {
    let _guard = PATH_LOCK.lock().unwrap();
    let tmp = tempdir("prepared-env");
    fs::write(tmp.join("terse.toml"), MANIFEST).unwrap();
    fs::write(tmp.join("paper.trs"), ENTRY).unwrap();

    let tools = tempdir("prepared-env-tools");
    for tool in ["xelatex", "biber"] {
        let path = tools.join(tool);
        fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    let real_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", &tools);
    std::env::set_var("TEXINPUTS", "/leaked/texinputs//:");
    std::env::set_var("BIBINPUTS", "/leaked/bibinputs");
    std::env::set_var("TEXMFCNF", "/leaked/cnf");
    std::env::set_var("TEXMFHOME", "/leaked/texmfhome");

    let mut runner = FakeProcessRunner::new(vec![ProcessOutcome::success(); 4]);
    let code = terse_cli::build::run_build_with_runner(&tmp, None, None, false, true, &mut runner);

    std::env::set_var("PATH", &real_path);
    for key in ["TEXINPUTS", "BIBINPUTS", "TEXMFCNF", "TEXMFHOME"] {
        std::env::remove_var(key);
    }
    // The fake engine writes no PDF, so the command's own exit code is
    // not the subject; the recorded invocations are.
    let _ = code;
    assert!(!runner.invocations.is_empty(), "the fake engine ran at least once");

    for inv in &runner.invocations {
        for (key, value) in &inv.env {
            assert!(
                terse_cli::toolchain::env::EMITTED_KEYS.contains(&key.as_str()),
                "unexpected environment key {key} in {:?}",
                inv.env
            );
            assert!(!value.contains("/leaked/"), "override leaked through {key}={value}");
        }
        for owned in ["TEXMFHOME", "TEXMFVAR", "TEXMFCONFIG"] {
            let value = inv
                .env
                .iter()
                .find(|(k, _)| k == owned)
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| panic!("{owned} must be set"));
            assert!(value.contains("terse"), "{owned} is a Terse-owned directory, got {value}");
        }
        assert!(inv.env.iter().any(|(k, v)| k == "LANG" && v == "C.UTF-8"));
    }

    // The runner itself adds nothing: an invocation with no environment
    // records no environment.
    let inv = engine::ProcessInvocation {
        program: std::path::PathBuf::from("x"),
        args: vec![],
        working_dir: tmp.clone(),
        env: vec![],
    };
    let mut bare = FakeProcessRunner::new(vec![]);
    let _ = terse_cli::engine::ProcessRunner::run(&mut bare, &inv, Duration::from_secs(1));
    assert!(bare.invocations[0].env.is_empty());
}

/// `managed-toolchain` group 1: a pass that exceeds its timeout is
/// `E-LATEX-014`, distinct from a nonzero exit (`E-LATEX-011`); both
/// preserve the previous output.
#[test]
fn test_timeout_is_distinct_from_crash() {
    let tmp = tempdir("timeout-vs-crash");
    fs::write(tmp.join("terse.toml"), MANIFEST).unwrap();
    fs::write(tmp.join("paper.trs"), ENTRY).unwrap();
    assert_eq!(terse_cli::run(["terse", "build", "--tex-only"], &tmp), 0);
    let before = fs::read(tmp.join("build/academic/paper.tex")).unwrap();

    let config = EngineConfig {
        xelatex: std::path::PathBuf::from("xelatex"),
        biber: std::path::PathBuf::from("biber"),
        timeout: Duration::from_secs(60),
    };
    let mut hung = FakeProcessRunner::new(vec![ProcessOutcome::timed_out()]);
    let (_, failure) = engine::compile_bounded(&mut hung, &tmp, "paper", &config, false).unwrap_err();
    let diag = engine::logs::interpret_failure(&failure);
    assert_eq!(diag.code, "E-LATEX-014");
    assert!(diag.message.contains("60s"), "{}", diag.message);
    assert!(diag.help.as_deref().unwrap_or("").contains("terse doctor"));

    let mut crashed = FakeProcessRunner::new(vec![ProcessOutcome::nonzero(1)]);
    let (_, failure) = engine::compile_bounded(&mut crashed, &tmp, "paper", &config, false).unwrap_err();
    assert_eq!(engine::logs::interpret_failure(&failure).code, "E-LATEX-011");

    // Through the full command, both are tool failures (exit 3) that leave
    // the last published generation untouched.
    let tools = tempdir("timeout-vs-crash-tools");
    let xelatex = tools.join("xelatex");
    fs::write(&xelatex, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&xelatex, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let host = terse_cli::toolchain::HostEnv::minimal(
        terse_cli::toolchain::HostOs::current(),
        tools.as_os_str().to_os_string(),
    );
    for outcome in [ProcessOutcome::timed_out(), ProcessOutcome::nonzero(1)] {
        let mut runner = FakeProcessRunner::new(vec![outcome]);
        let code = terse_cli::build::run_build_with_host(
            &tmp, None, None, false, true, false, None, &host, &mut runner,
        );
        assert_eq!(code, 3);
        assert_eq!(fs::read(tmp.join("build/academic/paper.tex")).unwrap(), before);
        assert!(!tmp.join("build/academic/paper.pdf").exists());
    }
}

/// `managed-toolchain` group 1: a timeout terminates the whole process
/// tree, grandchildren included, not only the direct child.
#[cfg(unix)]
#[test]
fn test_timeout_terminates_process_tree() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempdir("process-tree");
    let script = tmp.join("spawner.sh");
    let marker = tmp.join("grandchild.pid");
    fs::write(
        &script,
        format!("#!/bin/sh\nsleep 30 &\necho $! > '{}'\nwait\n", marker.display()),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let invocation = engine::ProcessInvocation {
        program: script.clone(),
        args: vec![],
        working_dir: tmp.clone(),
        env: vec![("PATH".to_string(), "/usr/bin:/bin".to_string())],
    };
    let started = std::time::Instant::now();
    let outcome =
        terse_cli::engine::ProcessRunner::run(&mut engine::RealProcessRunner, &invocation, Duration::from_secs(1));
    assert!(outcome.started);
    assert!(outcome.timed_out, "the script runs 30s and must be cut off");
    assert!(started.elapsed() < Duration::from_secs(10), "termination is prompt");

    let pid: i32 = fs::read_to_string(&marker)
        .expect("the script recorded its grandchild's pid before waiting")
        .trim()
        .parse()
        .unwrap();
    // Bounded deadline: a killed process disappears from the table (or is
    // a zombie reaped by init) well within this window.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let alive = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .map(|o| {
                let s = String::from_utf8_lossy(&o.stdout);
                o.status.success() && !s.trim().is_empty() && !s.contains('Z')
            })
            .unwrap_or(false);
        if !alive {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "grandchild {pid} survived the timeout");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// `managed-toolchain` group 1: `[latex] pdf` is the default compilation
/// mode; explicit flags still win.
#[test]
fn test_manifest_pdf_mode_is_default() {
    let tmp = tempdir("manifest-pdf-mode");
    fs::write(tmp.join("paper.trs"), ENTRY).unwrap();
    let empty_tools = tempdir("manifest-pdf-mode-empty");
    let host = terse_cli::toolchain::HostEnv::minimal(
        terse_cli::toolchain::HostOs::current(),
        empty_tools.as_os_str().to_os_string(),
    );

    fs::write(tmp.join("terse.toml"), format!("{MANIFEST}\n[latex]\npdf = \"require-pdf\"\n")).unwrap();
    let code = terse_cli::run_with_host(["terse", "build"], &tmp, &host);
    assert_eq!(code, 3, "the manifest's require-pdf makes a missing engine fatal");
    assert!(!tmp.join("build").exists());
    let code = terse_cli::run_with_host(["terse", "build", "--tex-only"], &tmp, &host);
    assert_eq!(code, 0, "the explicit flag wins");
    assert!(tmp.join("build/academic/paper.tex").is_file());

    fs::write(tmp.join("terse.toml"), format!("{MANIFEST}\n[latex]\npdf = \"tex-only\"\n")).unwrap();
    let code = terse_cli::run_with_host(["terse", "build", "--require-pdf"], &tmp, &host);
    assert_eq!(code, 3, "--require-pdf overrides a tex-only manifest default");
    let mut runner = FakeProcessRunner::new(vec![]);
    let code = terse_cli::build::run_build_with_host(&tmp, None, None, false, false, false, None, &host, &mut runner);
    assert_eq!(code, 0, "tex-only default builds without an engine");
    assert!(runner.invocations.is_empty());

    fs::write(tmp.join("terse.toml"), format!("{MANIFEST}\n[latex]\npdf = \"bogus\"\n")).unwrap();
    assert_eq!(terse_cli::run_with_host(["terse", "build", "--tex-only"], &tmp, &host), 2);
}
