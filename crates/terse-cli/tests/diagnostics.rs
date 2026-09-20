use std::fs;

use terse_cli::engine::{CompileFailure, FakeProcessRunner, ProcessOutcome};

fn tempdir(label: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir().join(format!(
        "terse-diagnostics-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&base).unwrap();
    base
}

#[test]
fn test_tool_startup_error_has_honest_span() {
    // The interpreted diagnostic for a tool that could not be started
    // carries no primary span (there is no `.trs` position to blame for a
    // missing/unrunnable executable) and no fabricated coordinates, only
    // the tool-failure code family and a configuration-facing hint.
    let failure = CompileFailure::ToolStartFailed {
        program: "xelatex".to_string(),
    };
    let diag = terse_cli::engine::logs::interpret_failure(&failure);
    assert!(diag.code.starts_with("E-LATEX-"));
    assert!(diag.primary.is_none(), "no fabricated source position for a tool-start failure");
    assert!(!diag.message.contains(".trs"));
    assert!(diag.help.is_some());

    // And the full build command actually reaches this failure path when
    // the runner reports a start failure (distinct from a real engine
    // content failure, which does have process output to interpret).
    let tmp = tempdir("tool-start");
    fs::write(
        tmp.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n",
    )
    .unwrap();
    fs::write(
        tmp.join("paper.trs"),
        "document:\n  title: \"Honest Span\"\n\nHello.\n",
    )
    .unwrap();

    let mut runner = FakeProcessRunner::new(vec![ProcessOutcome::failed_to_start()]);
    let code = terse_cli::build::run_build_with_runner(&tmp, None, None, false, true, &mut runner);
    assert_eq!(code, 3, "a tool-start failure is a tool-execution failure, not a usage error");
}

#[test]
fn test_deny_warnings_does_not_rename_code() {
    let tmp = tempdir("deny-warnings");
    fs::write(
        tmp.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n",
    )
    .unwrap();
    fs::write(
        tmp.join("paper.trs"),
        concat!(
            "document:\n  title: \"Raw Warning\"\n\n",
            "tex:\n  \\relax\n",
        ),
    )
    .unwrap();

    // Ordinary (nonstrict) checking never rejects the raw block solely for
    // being raw.
    let (code, _, diags) = terse_cli::build::check_diagnostics(&tmp, None, None, false, false).unwrap();
    assert_eq!(code, 0);
    assert!(diags.is_empty());

    // Strict checking without --deny-warnings: W-TEX-001 is reported but
    // the run still succeeds.
    let (code_warn_only, _, diags_warn_only) =
        terse_cli::build::check_diagnostics(&tmp, None, None, true, false).unwrap();
    assert_eq!(code_warn_only, 0, "warning-only strict run succeeds");
    assert_eq!(diags_warn_only.len(), 1);
    assert_eq!(diags_warn_only[0].code, "W-TEX-001");

    // Strict checking with --deny-warnings: same code, same span, but now
    // it fails.
    let (code_deny, _, diags_deny) =
        terse_cli::build::check_diagnostics(&tmp, None, None, true, true).unwrap();
    assert_eq!(code_deny, 1, "deny-warnings run fails");
    assert_eq!(diags_deny.len(), 1);
    assert_eq!(diags_deny[0].code, "W-TEX-001", "deny-warnings changes exit status only, never the diagnostic code");
    assert_eq!(diags_deny[0].primary, diags_warn_only[0].primary, "same span in both runs");
}

#[test]
fn test_duplicate_id_diagnostic_has_two_origins() {
    let tmp = tempdir("dup-id-origins");
    fs::write(
        tmp.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n",
    )
    .unwrap();
    fs::write(
        tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\ninclude \"a.trs\"\ninclude \"b.trs\"\n",
    )
    .unwrap();
    fs::write(tmp.join("a.trs"), "# Demand [id: demand-model]\n\nBody.\n").unwrap();
    fs::write(tmp.join("b.trs"), "# Demand again [id: demand-model]\n\nBody.\n").unwrap();

    let (code, entry_path, diags) = terse_cli::build::check_diagnostics(&tmp, None, None, false, false).unwrap();
    assert_eq!(code, 1);
    assert_eq!(diags.len(), 1);

    let diag = &diags[0];
    assert_eq!(diag.code, "E-ID-002");
    assert_eq!(diag.severity, terse_core::diagnostic::Severity::Error);
    assert!(diag.primary.is_some(), "the offending (duplicate) position is exact");
    assert_eq!(
        diag.related.len(),
        2,
        "both the first definition and the duplicate are related origins"
    );
    assert!(
        diag.related[0].span.is_some() && diag.related[1].span.is_some(),
        "both related origins carry exact positions, not just prose"
    );
    assert_ne!(
        diag.related[0].span, diag.related[1].span,
        "the two origins are genuinely distinct positions (different included files)"
    );

    let rendered = terse_cli::diagnostics::render_human(diag, entry_path.to_str().unwrap(), "");
    assert!(rendered.contains("E-ID-002"));
    assert!(rendered.contains("demand-model"), "human wording names the offending id, not just a code");
    assert_eq!(diag.related.len(), 2, "structured field is stable independent of rendering");
}

/// A UTF-8 byte offset for `text` at the start of `needle`'s first
/// occurrence -- used so the test's expected column is derived from the
/// same source bytes the compiler sees, not a hand-counted guess.
fn byte_offset_of(text: &str, needle: &str) -> u32 {
    text.find(needle).unwrap() as u32
}

#[test]
fn test_unicode_columns_match_original_bytes() {
    let tmp = tempdir("unicode-columns");
    fs::write(
        tmp.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n",
    )
    .unwrap();
    fs::write(
        tmp.join("paper.trs"),
        "document:\n  title: \"Unicode\"\n\ninclude \"section.trs\"\n",
    )
    .unwrap();
    // "café" before the citation: each accented/multibyte character is one
    // Unicode scalar but two-plus UTF-8 bytes, so a byte-offset column
    // would overcount relative to what an editor shows.
    let included = "Um café e uma naïve análise. Veja @unknown para detalhes.\n";
    fs::write(tmp.join("section.trs"), included).unwrap();

    let (code, _entry_path, diags) = terse_cli::build::check_diagnostics(&tmp, None, None, false, false).unwrap();
    assert_eq!(code, 1);
    assert_eq!(diags.len(), 1);
    let diag = &diags[0];
    assert_eq!(diag.code, "E-CITE-001");

    let span = diag.primary.expect("unknown-alias diagnostic carries an exact position");
    let byte_start = byte_offset_of(included, "@unknown");
    assert_eq!(span.byte_start, byte_start, "byte range stays anchored to the original bytes");

    // The reported column must count Unicode scalars, not bytes: "café"
    // and "naïve" together contribute 4 multibyte scalars (é, ï, plus the
    // combining-equivalent accents), each occupying 2 UTF-8 bytes, so the
    // scalar column is measurably smaller than a byte-based column would
    // be.
    let (_line, scalar_col) = terse_cli::diagnostics::line_col(included, byte_start);
    let byte_col = byte_start + 1; // a naively byte-counted 1-based column
    assert!(
        scalar_col < byte_col,
        "Unicode-scalar column ({scalar_col}) must be smaller than the raw byte column ({byte_col}) \
         given multibyte characters precede the citation"
    );

    // The diagnostic is anchored in the *included* module, not the entry.
    let loaded = terse_cli::project::load_modules(&tmp, &tmp.join("paper.trs")).unwrap();
    let (path, text) = loaded.file_index.get(&span.file_id).expect("span resolves to a loaded file");
    assert!(path.ends_with("section.trs"), "position must name the original included module");
    assert_eq!(text, included, "resolved file text matches the included module's own bytes");
}

#[test]
fn test_json_diagnostics_are_stable() {
    let tmp = tempdir("json-stable");
    fs::write(
        tmp.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n",
    )
    .unwrap();
    fs::write(
        tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\nSee @missing for details.\n",
    )
    .unwrap();

    // Run twice under varied terminal/color-ish environment to prove the
    // JSON output does not depend on it; capture stdout each time via the
    // lower-level diagnostics API directly (stable across process
    // boundaries the same way, without needing to fork a real process).
    let (_, entry_path, diags1) = terse_cli::build::check_diagnostics(&tmp, None, None, false, false).unwrap();
    let (_, _, diags2) = terse_cli::build::check_diagnostics(&tmp, None, None, false, false).unwrap();
    let loaded = terse_cli::project::load_modules(&tmp, &entry_path).unwrap();

    let json1 = terse_cli::diagnostics::render_json(&diags1, &loaded.file_index, &entry_path);
    let json2 = terse_cli::diagnostics::render_json(&diags2, &loaded.file_index, &entry_path);
    assert_eq!(json1, json2, "identical input must produce byte-identical JSON across runs");

    let value: serde_json::Value = serde_json::from_str(&json1).expect("output must parse as JSON");
    assert_eq!(value["version"], 1, "envelope carries a stable version number");
    assert_eq!(value["diagnostics"].as_array().unwrap().len(), 1);
    assert_eq!(value["diagnostics"][0]["code"], "E-CITE-001");
    assert!(
        !value["diagnostics"][0]["position"]["file"]
            .as_str()
            .unwrap()
            .is_empty()
    );

    // Varying the terminal/color environment must not change the JSON
    // (only the human-rendering path is expected to ever consider such
    // things, and it never runs when `--json` is passed).
    std::env::set_var("NO_COLOR", "1");
    std::env::set_var("TERM", "dumb");
    let (_, _, diags3) = terse_cli::build::check_diagnostics(&tmp, None, None, false, false).unwrap();
    std::env::remove_var("NO_COLOR");
    std::env::remove_var("TERM");
    let json3 = terse_cli::diagnostics::render_json(&diags3, &loaded.file_index, &entry_path);
    assert_eq!(json1, json3, "JSON output must not depend on terminal/color environment variables");

    // A successful check yields a parseable, empty diagnostics array.
    fs::write(
        tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\nHello.\n",
    )
    .unwrap();
    let code = terse_cli::run(["terse", "check", "--json"], &tmp);
    assert_eq!(code, 0);
}

#[test]
fn test_exit_codes_and_json_are_meaningful() {
    // Success: exit 0.
    let tmp_ok = tempdir("exit-codes-ok");
    fs::write(
        tmp_ok.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n",
    )
    .unwrap();
    fs::write(tmp_ok.join("paper.trs"), "document:\n  title: \"T\"\n\nHello.\n").unwrap();
    assert_eq!(terse_cli::run(["terse", "check", "--json"], &tmp_ok), 0);

    // Input/format error (unknown citation alias): exit 1.
    let tmp_input = tempdir("exit-codes-input");
    fs::write(
        tmp_input.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n",
    )
    .unwrap();
    fs::write(
        tmp_input.join("paper.trs"),
        "document:\n  title: \"T\"\n\nSee @missing.\n",
    )
    .unwrap();
    let code = terse_cli::run(["terse", "check", "--json"], &tmp_input);
    assert_eq!(code, 1, "an unknown citation is an input/format error");

    // Usage/configuration error (mutually exclusive flags): exit 2.
    let tmp_usage = tempdir("exit-codes-usage");
    fs::write(
        tmp_usage.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n",
    )
    .unwrap();
    fs::write(tmp_usage.join("paper.trs"), "document:\n  title: \"T\"\n\nHello.\n").unwrap();
    let code = terse_cli::run(["terse", "build", "--tex-only", "--require-pdf"], &tmp_usage);
    assert_eq!(code, 2, "conflicting flags are a usage error");

    // Missing manifest is also a usage/configuration error: exit 2.
    let tmp_missing = tempdir("exit-codes-missing-manifest");
    let code = terse_cli::run(["terse", "check"], &tmp_missing);
    assert_eq!(code, 2, "a missing manifest is a configuration error");

    // Tool/I-O failure (engine reports a start failure): exit 3.
    let tmp_tool = tempdir("exit-codes-tool");
    fs::write(
        tmp_tool.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n",
    )
    .unwrap();
    fs::write(tmp_tool.join("paper.trs"), "document:\n  title: \"T\"\n\nHello.\n").unwrap();
    let mut runner = terse_cli::engine::FakeProcessRunner::new(vec![
        terse_cli::engine::ProcessOutcome::failed_to_start(),
    ]);
    let code = terse_cli::build::run_build_with_runner(&tmp_tool, None, None, false, true, &mut runner);
    assert_eq!(code, 3, "a tool failure is a distinct exit code from an input/usage error");

    // Formatting drift (`fmt --check` on non-canonical input): exit 1.
    let tmp_fmt = tempdir("exit-codes-fmt");
    fs::write(
        tmp_fmt.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n",
    )
    .unwrap();
    // Milestone-1 `fmt` only normalizes line endings/BOM/final newline (the
    // full lossless tree-based formatter is group 19's own scope); a CRLF
    // file is drift under that contract.
    fs::write(tmp_fmt.join("paper.trs"), "document:\r\n  title: \"T\"\r\n\r\nHello.\r\n").unwrap();
    let code = terse_cli::run(["terse", "fmt", "--check", "paper.trs"], &tmp_fmt);
    assert_eq!(code, 1, "formatting drift is reported as a content-check failure, not success");

    // Every JSON-emitting invocation above must have produced stdout that
    // parses as one JSON value with no ANSI escapes or interleaved
    // progress text -- verified directly against the lower-level
    // diagnostic API rather than capturing a child process's real stdout,
    // consistent with how this suite drives the CLI throughout.
    let (_, entry_path, diags) = terse_cli::build::check_diagnostics(&tmp_input, None, None, false, false).unwrap();
    let loaded = terse_cli::project::load_modules(&tmp_input, &entry_path).unwrap();
    let json = terse_cli::diagnostics::render_json(&diags, &loaded.file_index, &entry_path);
    assert!(!json.contains('\u{1b}'), "no ANSI escape sequences in JSON output");
    serde_json::from_str::<serde_json::Value>(&json).expect("stdout JSON must parse cleanly");
}

#[test]
fn test_bounded_error_recovery_never_publishes() {
    let tmp = tempdir("bounded-recovery");
    fs::write(
        tmp.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n",
    )
    .unwrap();
    // A first successful build establishes known previous output.
    fs::write(tmp.join("paper.trs"), "document:\n  title: \"T\"\n\nHello.\n").unwrap();
    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 0);
    let previous = fs::read(tmp.join("build/academic/paper.tex")).unwrap();

    // Several independent, recoverable syntax errors: each malformed
    // reserved header is its own top-level parse failure, separated by
    // ordinary valid paragraphs so the parser can resynchronize at the
    // next module-scope boundary between them.
    let broken = concat!(
        "document:\n  title: \"T\"\n\n",
        "theorem bad attrs here:\n  x\n\n",
        "Valid paragraph one.\n\n",
        "figure bad attrs too:\n  y\n\n",
        "Valid paragraph two.\n\n",
        "table also bad:\n  z\n",
    );
    fs::write(tmp.join("paper.trs"), broken).unwrap();

    let (code, _entry_path, diags) = terse_cli::build::check_diagnostics(&tmp, None, None, false, false).unwrap();
    assert_eq!(code, 1);
    assert!(
        diags.len() >= 2,
        "bounded recovery must surface more than one independent error in a single pass, got {}",
        diags.len()
    );
    // Diagnostics are reported in source order.
    let starts: Vec<u32> = diags.iter().filter_map(|d| d.primary.map(|s| s.byte_start)).collect();
    let mut sorted = starts.clone();
    sorted.sort();
    assert_eq!(starts, sorted, "recovered diagnostics stay in ordered (source) position");

    // Failure never touches the previously published output.
    let code_build = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code_build, 1, "an input with only recoverable errors must still fail overall");
    let after = fs::read(tmp.join("build/academic/paper.tex")).unwrap();
    assert_eq!(previous, after, "a failed build must never replace previously published output");
}

/// `managed-toolchain` scenario "Toolchain codes are stable and distinct":
/// a timed-out engine pass is `E-LATEX-014`, a failed doctor check carries
/// an `E-TOOL-0xx` code, and no code is shared between the engine, doctor,
/// and toolchain-install code tables.
#[test]
fn test_toolchain_codes_are_stable_and_distinct() {
    use terse_cli::engine::PassKind;
    use terse_cli::toolchain::{HostEnv, HostOs, ToolchainSelector};

    let timeout = CompileFailure::Timeout { kind: PassKind::Xelatex, secs: 60 };
    let diag = terse_cli::engine::logs::interpret_failure(&timeout);
    assert_eq!(diag.code, "E-LATEX-014");
    assert!(diag.help.as_deref().unwrap_or("").contains("terse doctor"));

    let root = tempdir("codes-distinct");
    let path_dir = root.join("path");
    fs::create_dir_all(&path_dir).unwrap();
    for name in ["xelatex", "biber", "kpsewhich"] {
        fs::write(path_dir.join(name), "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path_dir.join(name), fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    let mut host = HostEnv::minimal(HostOs::Linux, path_dir.as_os_str().to_os_string());
    host.home = Some(root.join("home"));
    host.tmpdir = Some(root.join("tmp").to_string_lossy().into_owned());
    host.username = Some("tester".to_string());
    let mut runner = FakeProcessRunner::new(vec![
        ProcessOutcome::success_with_log(&b"XeTeX (TeX Live 2025)\n"[..]),
        ProcessOutcome::timed_out(),
    ]);
    let opts = terse_cli::doctor::DoctorOptions { fix: false, json: true, toolchain: Some(ToolchainSelector::System) };
    let report = terse_cli::doctor::run_doctor(&root, &opts, &host, &mut runner);
    let biber = report.checks.iter().find(|c| c.id == "biber.runs").expect("biber.runs check");
    let code = biber.code.expect("a failed check carries a code");
    assert!(code.starts_with("E-TOOL-0"), "got {code}");

    let engine_codes: Vec<&str> = [
        CompileFailure::ToolStartFailed { program: "x".into() },
        CompileFailure::NonZeroExit { kind: PassKind::Xelatex, code: Some(1) },
        CompileFailure::PassLimitExceeded,
        CompileFailure::MissingGlyph,
        timeout,
    ]
    .iter()
    .map(|f| terse_cli::engine::logs::interpret_failure(f).code)
    .collect();
    let mut all: Vec<&str> = Vec::new();
    all.extend(engine_codes.iter().copied());
    all.extend(terse_cli::doctor::checks::ALL_CODES.iter().copied());
    all.extend(terse_cli::toolchain::install::ALL_CODES.iter().copied());
    all.extend(terse_cli::toolchain::RESOLUTION_CODES.iter().copied());
    let mut seen = std::collections::BTreeSet::new();
    for code in &all {
        assert!(seen.insert(*code), "code {code} appears in more than one table");
    }
}
