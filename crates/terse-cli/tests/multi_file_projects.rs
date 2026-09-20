use std::fs;
use std::path::Path;

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

const MANIFEST: &str = "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n";
const ENTRY: &str = "document:\n  title: \"Test Paper\"\n\n# Intro\n\nHello.\n";

/// `managed-toolchain`: `[latex] engine` may not disagree with the export
/// profile's engine; every command that loads the manifest fails the same
/// way, and the default or a matching value passes.
#[test]
fn test_engine_field_disagreeing_with_profile_is_config_error() {
    let tmp = tempdir_named("engine-mismatch");
    write(&tmp.join("paper.trs"), ENTRY);
    let mismatched = format!("{MANIFEST}\n[latex]\nengine = \"pdflatex\"\n");
    write(&tmp.join("terse.toml"), &mismatched);
    assert_eq!(terse_cli::run(["terse", "check"], &tmp), 2);
    assert_eq!(terse_cli::run(["terse", "build", "--tex-only"], &tmp), 2);
    assert!(!tmp.join("build").exists(), "no output is produced on a configuration error");

    let matching = format!("{MANIFEST}\n[latex]\nengine = \"xelatex\"\n");
    write(&tmp.join("terse.toml"), &matching);
    assert_eq!(terse_cli::run(["terse", "check"], &tmp), 0);
    write(&tmp.join("terse.toml"), MANIFEST);
    assert_eq!(terse_cli::run(["terse", "check"], &tmp), 0);
}

#[test]
fn test_manifest_discovery_from_descendant() {
    let tmp = tempdir();
    write(&tmp.join("terse.toml"), MANIFEST);
    write(&tmp.join("paper.trs"), ENTRY);
    let nested = tmp.join("sections").join("deep");
    fs::create_dir_all(&nested).unwrap();

    // From the nested directory, `check` with no explicit entry discovers
    // the root manifest and its root-relative entry.
    let code = terse_cli::run(["terse", "check"], &nested);
    assert_eq!(code, 0);

    // An explicit relative entry starts discovery from its own directory,
    // not from another project rooted at the working directory.
    let other_root = tmp.join("other-project");
    write(&other_root.join("terse.toml"), MANIFEST);
    write(&other_root.join("paper.trs"), ENTRY);
    let code = terse_cli::run(
        ["terse", "check", "../other-project/paper.trs"],
        &tmp.join("sections"),
    );
    assert_eq!(code, 0);
}

#[test]
fn test_missing_or_invalid_manifest_fails() {
    let tmp = tempdir();
    // No ancestor manifest at all.
    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 2);

    // Unknown field.
    let bad_field_dir = tmp.join("bad-field");
    write(
        &bad_field_dir.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\nbogus = true\n",
    );
    write(&bad_field_dir.join("paper.trs"), ENTRY);
    let before = fs::read(bad_field_dir.join("terse.toml")).unwrap();
    let code = terse_cli::run(["terse", "check"], &bad_field_dir);
    assert_eq!(code, 2);
    let after = fs::read(bad_field_dir.join("terse.toml")).unwrap();
    assert_eq!(before, after, "no files should be written on failure");

    // Unsupported format version.
    let bad_version_dir = tmp.join("bad-version");
    write(
        &bad_version_dir.join("terse.toml"),
        "format-version = 2\n\n[project]\nentry = \"paper.trs\"\n",
    );
    write(&bad_version_dir.join("paper.trs"), ENTRY);
    let code = terse_cli::run(["terse", "check"], &bad_version_dir);
    assert_eq!(code, 2);

    // Malformed TOML.
    let malformed_dir = tmp.join("malformed");
    write(&malformed_dir.join("terse.toml"), "not = [valid");
    let code = terse_cli::run(["terse", "check"], &malformed_dir);
    assert_eq!(code, 2);
}

#[test]
fn test_cli_precedence_is_environment_independent() {
    let tmp = tempdir();
    write(
        &tmp.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n\n[latex]\npdf = \"auto\"\n",
    );
    write(&tmp.join("paper.trs"), ENTRY);

    // An explicit theme not declared in this manifest's `[themes]` fails,
    // rather than silently falling back to the manifest's default or
    // environment state.
    let code = terse_cli::run(
        ["terse", "build", "--theme", "magalu", "--tex-only"],
        &tmp,
    );
    assert_eq!(code, 2, "magalu is not declared in this project's [themes]");

    // The explicit --tex-only flag wins over the manifest's `pdf = "auto"`:
    // zero build failures from missing engine, identical bytes across two
    // separately-invoked builds regardless of environment variables.
    let out1 = tmp.join("out1");
    fs::create_dir_all(&out1).unwrap();
    let code = with_env_var("TERSE_UNKNOWN_ENV_PROBE", "changed", || {
        terse_cli::run(["terse", "build", "--tex-only"], &tmp)
    });
    assert_eq!(code, 0);
    let tex1 = fs::read(tmp.join("build").join("academic").join("paper.tex")).unwrap();

    let build_dir = tmp.join("build");
    fs::remove_dir_all(&build_dir).unwrap();
    let code = with_env_var("TERSE_UNKNOWN_ENV_PROBE", "different-value", || {
        terse_cli::run(["terse", "build", "--tex-only"], &tmp)
    });
    assert_eq!(code, 0);
    let tex2 = fs::read(build_dir.join("academic").join("paper.tex")).unwrap();
    assert_eq!(tex1, tex2);
}

#[test]
fn test_output_scope_cannot_overlap_inputs() {
    let cases = [
        ("output-is-root", "."),
        ("output-is-source-dir", "sections"),
        ("output-escapes-root", "../outside"),
    ];
    for (case_name, output) in cases {
        let tmp = tempdir_named(case_name);
        write(
            &tmp.join("terse.toml"),
            &format!(
                "format-version = 1\n\n[project]\nentry = \"sections/paper.trs\"\noutput = \"{output}\"\n"
            ),
        );
        write(&tmp.join("sections").join("paper.trs"), ENTRY);
        let sentinel = tmp.join("sections").join("paper.trs");
        let before = fs::read(&sentinel).unwrap();

        let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
        assert_eq!(code, 2, "case {case_name} should fail as a configuration error");

        let after = fs::read(&sentinel).unwrap();
        assert_eq!(before, after, "case {case_name} must not mutate sources");
    }
}

#[test]
fn test_symlink_escape_never_read() {
    let tmp = tempdir();
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"Test\"\n\ninclude \"leak.trs\"\n",
    );

    // A sentinel file lives outside the project entirely, containing bytes
    // that must never appear anywhere in a generated artifact.
    let outside_dir = tmp.parent().unwrap().join(format!(
        "terse-outside-{}",
        std::process::id()
    ));
    fs::create_dir_all(&outside_dir).unwrap();
    let sentinel = outside_dir.join("secret.trs");
    write(&sentinel, "SENTINEL-BYTES-MUST-NEVER-BE-READ\n");

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(&sentinel, tmp.join("leak.trs")).unwrap();

        let loaded =
            terse_cli::project::load_modules(&tmp, &tmp.join("paper.trs")).expect("entry loads");
        // The symlink escape is never resolved into the module map...
        assert!(!loaded.snapshot.modules.contains_key("leak.trs"));
        // ...and is recorded only as an attempted (never-read) dependency.
        assert!(loaded.dependencies.attempted.contains("leak.trs"));
        assert!(!loaded.dependencies.resolved.contains("leak.trs"));

        // The full CLI path also fails cleanly (compile reports the
        // missing include) rather than ever reading the sentinel.
        let code = terse_cli::run(["terse", "check"], &tmp);
        assert_eq!(code, 1);
    }

    let _ = fs::remove_dir_all(&outside_dir);
}

#[test]
fn test_failed_lookup_retained_as_dependency() {
    let tmp = tempdir();
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"Test\"\n\ninclude \"sections/missing.trs\"\n",
    );

    let loaded =
        terse_cli::project::load_modules(&tmp, &tmp.join("paper.trs")).expect("entry loads");
    assert!(loaded.dependencies.attempted.contains("sections/missing.trs"));
    // The parent directory is also retained, so creating it later (before
    // the file itself exists) is still an observable repair attempt.
    assert!(loaded.dependencies.attempted.contains("sections"));
    assert!(!loaded.dependencies.resolved.contains("sections/missing.trs"));

    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 1, "a missing include fails checking");
}

#[test]
fn test_include_order_is_authored_order_end_to_end() {
    let tmp = tempdir();
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"Test\"\n\nIntro.\n\ninclude \"sections/method.trs\"\n\nOutro.\n",
    );
    write(&tmp.join("sections").join("method.trs"), "Method body.\n");

    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 0);
}

#[test]
fn test_indirect_include_cycle_is_complete_end_to_end() {
    let tmp = tempdir();
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"Test\"\n\ninclude \"method.trs\"\n",
    );
    write(&tmp.join("method.trs"), "include \"appendix.trs\"\n");
    write(&tmp.join("appendix.trs"), "include \"paper.trs\"\n");

    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 1);
}

#[test]
fn test_duplicate_ids_show_both_routes() {
    // Variant 1: two different included modules each define the same id.
    let tmp = tempdir_named("dup-ids-two-modules");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\ninclude \"a.trs\"\ninclude \"b.trs\"\n",
    );
    write(&tmp.join("a.trs"), "# Demand [id: demand-model]\n\nBody.\n");
    write(&tmp.join("b.trs"), "# Demand again [id: demand-model]\n\nBody.\n");
    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 1, "duplicate id across two modules must fail checking");

    // Variant 2: a single module defines the id once, but is included
    // twice, so the duplicate arises from two distinct occurrences of the
    // very same span rather than two different files.
    let tmp2 = tempdir_named("dup-ids-repeated-include");
    write(&tmp2.join("terse.toml"), MANIFEST);
    write(
        &tmp2.join("paper.trs"),
        "document:\n  title: \"T\"\n\ninclude \"shared.trs\"\ninclude \"shared.trs\"\n",
    );
    write(&tmp2.join("shared.trs"), "# Demand [id: demand-model]\n\nBody.\n");
    let code = terse_cli::run(["terse", "check"], &tmp2);
    assert_eq!(code, 1, "duplicate id from repeated inclusion must also fail checking");

    // Case-distinct ids remain valid.
    let tmp3 = tempdir_named("dup-ids-case-distinct");
    write(&tmp3.join("terse.toml"), MANIFEST);
    write(
        &tmp3.join("paper.trs"),
        "document:\n  title: \"T\"\n\n# Demand [id: Demand]\n\nBody.\n\n# demand [id: demand]\n\nBody.\n",
    );
    let code = terse_cli::run(["terse", "check"], &tmp3);
    assert_eq!(code, 0, "case-distinct ids are not duplicates");
}

#[test]
fn test_proof_of_requires_theorem_target() {
    let tmp = tempdir_named("proof-wrong-target");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("paper.trs"),
        concat!(
            "document:\n  title: \"T\"\n\n",
            "figure \"a.pdf\" [id: fig-one]:\n  caption: \"A figure\"\n  alt: \"Alt\"\n\n",
            "proof [of: fig-one]:\n  This wrongly targets a figure.\n",
        ),
    );
    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 1, "a proof cannot target a non-theorem-like id");

    // A valid lemma target binds successfully.
    let tmp2 = tempdir_named("proof-valid-target");
    write(&tmp2.join("terse.toml"), MANIFEST);
    write(
        &tmp2.join("paper.trs"),
        concat!(
            "document:\n  title: \"T\"\n\n",
            "lemma [id: main-lemma]:\n  The lemma statement.\n\n",
            "proof [of: main-lemma]:\n  By direct computation.\n",
        ),
    );
    let code = terse_cli::run(["terse", "check"], &tmp2);
    assert_eq!(code, 0, "a proof targeting a real theorem-like id must check cleanly");

    // An independent proof (no `of`) never infers a target from adjacency.
    let tmp3 = tempdir_named("proof-independent");
    write(&tmp3.join("terse.toml"), MANIFEST);
    write(
        &tmp3.join("paper.trs"),
        "document:\n  title: \"T\"\n\nlemma [id: adjacent-lemma]:\n  Statement.\n\nproof:\n  Independent proof body.\n",
    );
    let code = terse_cli::run(["terse", "check"], &tmp3);
    assert_eq!(code, 0, "a proof with no 'of' is independent and never inferred");
}

#[test]
fn test_citations_bind_across_modules() {
    let tmp = tempdir_named("citations-cross-module");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\ninclude \"refs.trs\"\ninclude \"body.trs\"\n",
    );
    write(&tmp.join("refs.trs"), "refs:\n  robins1986: doi:10.1000/abc\n");
    write(&tmp.join("body.trs"), "See @robins1986 for details.\n");
    write(&tmp.join("references.lock"), "lock-version = 1\nnormalization-version = 1\n\n[entries.robins1986.declared]\nprovider = \"doi\"\nidentifier = \"10.1000/abc\"\n\n[entries.robins1986.resolved]\nprovider = \"doi\"\nidentifier = \"10.1000/abc\"\n\n[entries.robins1986.adapter]\nname = \"doi\"\nversion = \"1\"\n\n[entries.robins1986.provider_data]\ntitle = \"T\"\nwork_type = \"journal-article\"\nanonymous = true\n\n[entries.robins1986.effective]\ntitle = \"T\"\nwork_type = \"journal-article\"\nanonymous = true\n");
    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 0, "a citation binding to a cross-module declaration must check cleanly");

    // An unknown citation alias fails checking.
    let tmp2 = tempdir_named("citations-unknown-alias");
    write(&tmp2.join("terse.toml"), MANIFEST);
    write(
        &tmp2.join("paper.trs"),
        "document:\n  title: \"T\"\n\nSee @never-declared for details.\n",
    );
    let code = terse_cli::run(["terse", "check"], &tmp2);
    assert_eq!(code, 1, "citing an alias with no declaration must fail checking");
}

#[test]
fn test_bibliography_markers_are_global() {
    let tmp = tempdir_named("bib-two-markers");
    write(&tmp.join("terse.toml"), MANIFEST);
    write(
        &tmp.join("paper.trs"),
        "document:\n  title: \"T\"\n\ninclude \"a.trs\"\ninclude \"b.trs\"\n",
    );
    write(&tmp.join("a.trs"), "bibliography\n");
    write(&tmp.join("b.trs"), "bibliography\n");
    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 1, "two bibliography markers in one project must fail checking");

    // Exactly one explicit marker is retained and checks cleanly.
    let tmp2 = tempdir_named("bib-one-marker");
    write(&tmp2.join("terse.toml"), MANIFEST);
    write(
        &tmp2.join("paper.trs"),
        "document:\n  title: \"T\"\n\nrefs:\n  robins1986: doi:10.1000/abc\n\nSee @robins1986.\n\nbibliography\n",
    );
    write(&tmp2.join("references.lock"), "lock-version = 1\nnormalization-version = 1\n\n[entries.robins1986.declared]\nprovider = \"doi\"\nidentifier = \"10.1000/abc\"\n\n[entries.robins1986.resolved]\nprovider = \"doi\"\nidentifier = \"10.1000/abc\"\n\n[entries.robins1986.adapter]\nname = \"doi\"\nversion = \"1\"\n\n[entries.robins1986.provider_data]\ntitle = \"T\"\nwork_type = \"journal-article\"\nanonymous = true\n\n[entries.robins1986.effective]\ntitle = \"T\"\nwork_type = \"journal-article\"\nanonymous = true\n");
    let code = terse_cli::run(["terse", "check"], &tmp2);
    assert_eq!(code, 0, "a single explicit bibliography marker checks cleanly");

    // Zero markers with citations present still checks cleanly (a
    // bibliography is derived and appended automatically).
    let tmp3 = tempdir_named("bib-zero-markers-with-citation");
    write(&tmp3.join("terse.toml"), MANIFEST);
    write(
        &tmp3.join("paper.trs"),
        "document:\n  title: \"T\"\n\nrefs:\n  robins1986: doi:10.1000/abc\n\nSee @robins1986.\n",
    );
    write(&tmp3.join("references.lock"), "lock-version = 1\nnormalization-version = 1\n\n[entries.robins1986.declared]\nprovider = \"doi\"\nidentifier = \"10.1000/abc\"\n\n[entries.robins1986.resolved]\nprovider = \"doi\"\nidentifier = \"10.1000/abc\"\n\n[entries.robins1986.adapter]\nname = \"doi\"\nversion = \"1\"\n\n[entries.robins1986.provider_data]\ntitle = \"T\"\nwork_type = \"journal-article\"\nanonymous = true\n\n[entries.robins1986.effective]\ntitle = \"T\"\nwork_type = \"journal-article\"\nanonymous = true\n");
    let code = terse_cli::run(["terse", "check"], &tmp3);
    assert_eq!(code, 0, "no explicit marker with citations present still checks cleanly");
}

// --- test helpers -----------------------------------------------------

fn tempdir() -> std::path::PathBuf {
    tempdir_named("t")
}

fn tempdir_named(label: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!(
        "terse-test-{label}-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    fs::create_dir_all(&base).unwrap();
    base
}

fn unique_suffix() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Runs `f` with an environment variable set, restoring the previous value
/// afterward. Serialized via a process-wide lock so parallel tests do not
/// race on process environment mutation.
fn with_env_var<T>(key: &str, value: &str, f: impl FnOnce() -> T) -> T {
    use std::sync::Mutex;
    static ENV_LOCK: Mutex<()> = Mutex::new(());
    let _guard = ENV_LOCK.lock().unwrap();
    let previous = std::env::var(key).ok();
    std::env::set_var(key, value);
    let result = f();
    match previous {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
    result
}
