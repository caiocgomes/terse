use std::fs;
use std::path::Path;
use std::sync::Mutex;

/// Process-wide lock: PATH mutation in one test must not race another
/// test's real subprocess spawns or PATH reads.
static PATH_LOCK: Mutex<()> = Mutex::new(());

fn tempdir(label: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir().join(format!(
        "terse-git-tooling-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&base).unwrap();
    base
}

fn sha256_hex(bytes: &[u8]) -> String {
    // Cheap stand-in content hash for tree-comparison assertions: a real
    // sha2 dependency is already used in terse-core's artifact manifest,
    // but tests only need a stable fingerprint, not that specific
    // algorithm.
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn hash_tree(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(dir).unwrap().to_string_lossy().to_string();
                out.push((rel, sha256_hex(&fs::read(&path).unwrap())));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn test_init_scaffold_is_usable() {
    let tmp = tempdir("scaffold");

    let code = terse_cli::run(["terse", "init"], &tmp);
    assert_eq!(code, 0);

    assert!(tmp.join("terse.toml").is_file());
    assert!(tmp.join("paper.trs").is_file());
    assert!(tmp.join("themes").join("academic.theme").is_file());
    assert!(tmp.join("references.lock").is_file());
    assert!(tmp.join(".gitignore").is_file());

    // No current-date or random content: two fresh scaffolds are
    // byte-identical.
    let tmp2 = tempdir("scaffold2");
    let code2 = terse_cli::run(["terse", "init"], &tmp2);
    assert_eq!(code2, 0);
    assert_eq!(
        fs::read(tmp.join("paper.trs")).unwrap(),
        fs::read(tmp2.join("paper.trs")).unwrap()
    );
    assert_eq!(
        fs::read(tmp.join("terse.toml")).unwrap(),
        fs::read(tmp2.join("terse.toml")).unwrap()
    );

    // check, fmt --check, and build --tex-only all succeed without a TeX
    // engine or network access (this process never spawns one).
    let code = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code, 0);
    let code = terse_cli::run(["terse", "fmt", "--check"], &tmp);
    assert_eq!(code, 0, "the generated entry is already canonical");
    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 0);
    assert!(tmp.join("build").join("academic").join("paper.tex").is_file());
}

#[test]
fn test_init_conflict_is_preflighted() {
    let tmp = tempdir("conflict");
    fs::write(tmp.join("paper.trs"), "preexisting content\n").unwrap();
    fs::write(tmp.join(".gitignore"), "/node_modules\n").unwrap();

    let before = hash_tree(&tmp);

    let code = terse_cli::run(["terse", "init"], &tmp);
    assert_eq!(code, 2, "existing paper.trs must block init without --force");

    let after = hash_tree(&tmp);
    assert_eq!(before, after, "no writes on a preflighted conflict, including .gitignore");
}

#[test]
fn test_init_ignore_merge_is_additive() {
    let tmp = tempdir("ignore-merge");
    fs::write(tmp.join(".gitignore"), "/node_modules\ncustom-rule\n").unwrap();
    // A source figure PDF must remain eligible for Git (never ignored).
    fs::create_dir_all(tmp.join("figures")).unwrap();
    fs::write(tmp.join("figures").join("model.pdf"), b"%PDF-1.4 fake\n").unwrap();

    let code = terse_cli::run(["terse", "init"], &tmp);
    assert_eq!(code, 0);

    let ignore_after_first = fs::read_to_string(tmp.join(".gitignore")).unwrap();
    assert!(ignore_after_first.contains("/node_modules"));
    assert!(ignore_after_first.contains("custom-rule"));
    assert!(ignore_after_first.contains("/build"));
    assert_eq!(ignore_after_first.matches("/build").count(), 1);

    let unrelated_before = fs::read(tmp.join("figures").join("model.pdf")).unwrap();

    // Repeating with --force preserves unrelated entries and files, and
    // still adds each required rule exactly once.
    let code = terse_cli::run(["terse", "init", "--force"], &tmp);
    assert_eq!(code, 0);
    let ignore_after_second = fs::read_to_string(tmp.join(".gitignore")).unwrap();
    assert!(ignore_after_second.contains("/node_modules"));
    assert!(ignore_after_second.contains("custom-rule"));
    assert_eq!(ignore_after_second.matches("/build").count(), 1);
    assert_eq!(ignore_after_second.matches("/.terse-cache").count(), 1);

    let unrelated_after = fs::read(tmp.join("figures").join("model.pdf")).unwrap();
    assert_eq!(unrelated_before, unrelated_after);
}

#[test]
fn test_unowned_output_is_never_replaced() {
    let tmp = tempdir("unowned-output");
    fs::write(
        tmp.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n",
    )
    .unwrap();
    fs::write(
        tmp.join("paper.trs"),
        "document:\n  title: \"Unowned Test\"\n\nHello.\n",
    )
    .unwrap();

    // A populated destination with no Terse ownership marker: something
    // else put files there (a prior manual copy, a different tool).
    let output = tmp.join("build").join("academic");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("sentinel.txt"), b"do not touch").unwrap();
    let sentinel_before = fs::read(output.join("sentinel.txt")).unwrap();

    let source_before = fs::read(tmp.join("paper.trs")).unwrap();

    let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
    assert_eq!(code, 3, "an unowned populated destination must refuse publication");

    assert_eq!(fs::read(output.join("sentinel.txt")).unwrap(), sentinel_before);
    assert_eq!(fs::read(tmp.join("paper.trs")).unwrap(), source_before);
    assert!(
        !output.join("paper.tex").exists(),
        "nothing should have been published into the unowned destination"
    );
}

#[test]
fn test_publication_rollback_and_restart_recovery() {
    use terse_cli::publication;

    let tmp = tempdir("publish-recovery");
    let output = tmp.join("build").join("academic");

    // A known-valid managed output from a prior successful publish.
    let staged1 = publication::stage(&output, &[("paper.tex".to_string(), b"v1".to_vec())]).unwrap();
    publication::publish(staged1, &output).unwrap();
    assert_eq!(fs::read(output.join("paper.tex")).unwrap(), b"v1");

    // A complete next stage exists, ready to publish.
    let staged2 = publication::stage(&output, &[("paper.tex".to_string(), b"v2".to_vec())]).unwrap();

    // Simulate a crash exactly between the backup rename and the install
    // rename (as `publish` performs them): the old generation is already
    // moved aside, the new one is staged but not yet installed, and a
    // journal records that in-progress state, as if the process had been
    // killed mid-publish.
    let staging_dir = staged2.staging_dir.clone();
    let backup_dir = tmp.join("build").join(".simulated-backup");
    fs::rename(&output, &backup_dir).unwrap();
    fs::write(
        tmp.join("build").join(".terse-publish-journal.json"),
        format!(
            "{{\"staging\": {:?}, \"final\": {:?}, \"backup\": {:?}, \"stage\": \"backed-up\"}}\n",
            staging_dir, output, backup_dir
        ),
    )
    .unwrap();

    // Before recovery: no mixed state (no `final` generation at all yet).
    assert!(!output.exists());

    // Restart recovery finishes the interrupted publication.
    publication::recover(output.parent().unwrap()).unwrap();

    // After rollback/recovery: exactly one complete, valid generation,
    // no leftover backup or journal, no mixed files.
    assert_eq!(fs::read(output.join("paper.tex")).unwrap(), b"v2");
    assert!(!backup_dir.exists());
    assert!(!tmp
        .join("build")
        .join(".terse-publish-journal.json")
        .exists());
    assert!(!staging_dir.exists());
}

#[test]
fn test_check_requires_no_engine() {
    let _guard = PATH_LOCK.lock().unwrap();
    let tmp = tempdir("check-no-engine");
    fs::write(
        tmp.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n",
    )
    .unwrap();
    fs::write(
        tmp.join("paper.trs"),
        "document:\n  title: \"No Engine\"\n\nHello, world.\n",
    )
    .unwrap();

    let real_path = std::env::var("PATH").unwrap_or_default();
    let empty_path_dir = tempdir("check-no-engine-empty-path");
    std::env::set_var("PATH", &empty_path_dir);
    let code = terse_cli::run(["terse", "check"], &tmp);
    std::env::set_var("PATH", &real_path);

    assert_eq!(code, 0, "check must succeed without xelatex/biber on PATH at all");
    assert!(!tmp.join("build").exists(), "check is read-only: it must never create output");
    assert!(
        !tmp.join("references.lock").exists(),
        "check must never write a lock file"
    );
}

#[test]
fn test_check_theme_scope_is_explicit() {
    let tmp = tempdir("check-theme-scope");
    fs::write(
        tmp.join("terse.toml"),
        concat!(
            "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n\n",
            "[themes]\nvalid = \"themes/valid.theme\"\ninvalid = \"themes/invalid.theme\"\n",
        ),
    )
    .unwrap();
    fs::write(
        tmp.join("paper.trs"),
        "document:\n  title: \"Theme Scope\"\n\nHello.\n",
    )
    .unwrap();
    fs::create_dir_all(tmp.join("themes")).unwrap();
    fs::write(
        tmp.join("themes/valid.theme"),
        "page:\n  size: a4\n  margin: 2.5cm\n\nbody:\n  font: libertinus-otf\n",
    )
    .unwrap();
    // `invalid.theme` is declared but never written to disk: any check
    // that reaches it must fail on the missing file, not silently ignore
    // it.

    // Checking every declared theme (no --theme) fails because of the
    // invalid one.
    let code_all = terse_cli::run(["terse", "check"], &tmp);
    assert_eq!(code_all, 2, "checking all declared themes must fail on the invalid one");

    // Explicitly selecting only the valid theme succeeds and never
    // touches the invalid theme's (nonexistent) file.
    let code_valid = terse_cli::run(["terse", "check", "--theme", "valid"], &tmp);
    assert_eq!(code_valid, 0, "an explicitly selected valid theme must check independently of others");
}

#[test]
fn test_format_check_is_read_only() {
    let tmp = tempdir("fmt-check-read-only");
    assert_eq!(terse_cli::run(["terse", "init"], &tmp), 0);

    let entry = tmp.join("paper.trs");
    // Introduce drift: a run of blank lines the formatter must collapse.
    let drifted = "document:\n  title: \"Untitled Paper\"\n\n\n\n# Introduction\n\nStart writing here.\n";
    fs::write(&entry, drifted).unwrap();
    let before = fs::read(&entry).unwrap();

    let code = terse_cli::run(["terse", "fmt", "--check"], &tmp);
    assert_eq!(code, 1, "drifted input must report exit code 1");
    assert_eq!(
        fs::read(&entry).unwrap(),
        before,
        "fmt --check must never modify a selected file"
    );

    // Actually formatting writes the change...
    let code_write = terse_cli::run(["terse", "fmt"], &tmp);
    assert_eq!(code_write, 0);
    let formatted = fs::read(&entry).unwrap();
    assert_ne!(formatted, before, "fmt without --check must write the canonical form");

    // ...after which --check reports canonical input with exit code 0
    // and still makes no further change.
    let code_now_clean = terse_cli::run(["terse", "fmt", "--check"], &tmp);
    assert_eq!(code_now_clean, 0, "already-canonical input must report exit code 0");
    assert_eq!(fs::read(&entry).unwrap(), formatted);
}

#[test]
fn test_batch_format_parse_failure_writes_nothing() {
    let tmp = tempdir("fmt-batch-atomicity");
    assert_eq!(terse_cli::run(["terse", "init"], &tmp), 0);

    let entry = tmp.join("paper.trs");
    let drifted = "document:\n  title: \"Untitled Paper\"\n\n\n\n# Introduction\n\nStart writing here.\n";
    fs::write(&entry, drifted).unwrap();
    let entry_before = fs::read(&entry).unwrap();

    // A second, unrelated file with a real parse/validation failure
    // (restricted math rejects execution attempts, task group 8).
    let bad = tmp.join("bad.trs");
    fs::write(&bad, "math:\n  \\input{evil}\n").unwrap();

    let code = terse_cli::run(["terse", "fmt", "paper.trs", "bad.trs"], &tmp);
    assert_ne!(code, 0, "a batch containing an invalid file must fail");
    assert_eq!(
        fs::read(&entry).unwrap(),
        entry_before,
        "a failing batch must write none of its files, not even the valid, drifted one"
    );
}

/// Every generated *text* artifact (main.tex, style, .bib, source map,
/// COMPILE.txt/MANIFEST) is byte-identical across two independent clean
/// builds run under deliberately varied environments (locale, timezone,
/// working directory name, and file mtimes) with no shared cache. PDF
/// bytes are explicitly excluded from this contract (they embed
/// tool/timestamp metadata) — only the generated source set is compared.
#[test]
fn test_text_artifacts_are_reproducible() {
    let _guard = PATH_LOCK.lock().unwrap();

    let build_once = |label: &str, lang: &str, tz: &str| -> Vec<(String, String)> {
        let tmp = tempdir(label);
        assert_eq!(terse_cli::run(["terse", "init"], &tmp), 0);

        // Touch every source file so mtimes differ across the two runs
        // without changing content.
        std::thread::sleep(std::time::Duration::from_millis(5));
        let entry = tmp.join("paper.trs");
        let content = fs::read(&entry).unwrap();
        fs::write(&entry, &content).unwrap();

        let old_lang = std::env::var("LANG").ok();
        let old_tz = std::env::var("TZ").ok();
        unsafe {
            std::env::set_var("LANG", lang);
            std::env::set_var("TZ", tz);
        }
        let code = terse_cli::run(["terse", "build", "--tex-only"], &tmp);
        unsafe {
            match old_lang {
                Some(v) => std::env::set_var("LANG", v),
                None => std::env::remove_var("LANG"),
            }
            match old_tz {
                Some(v) => std::env::set_var("TZ", v),
                None => std::env::remove_var("TZ"),
            }
        }
        assert_eq!(code, 0);

        let output_dir = tmp.join("build").join("academic");
        hash_tree(&output_dir)
            .into_iter()
            .filter(|(name, _)| !name.ends_with(".pdf"))
            .collect()
    };

    let first = build_once("determinism-a", "en_US.UTF-8", "America/Sao_Paulo");
    let second = build_once("determinism-b", "pt_BR.UTF-8", "UTC");

    assert!(!first.is_empty(), "a build must produce at least one text artifact");
    assert_eq!(
        first, second,
        "generated text artifacts must be byte-identical across clean builds regardless of locale/timezone/mtime"
    );
}

/// A disposable build cache never changes program output: deleting it
/// (or corrupting an entry) must only ever cost a recompute, never a
/// behavior change or crash.
#[test]
fn test_cache_is_disposable() {
    let tmp = tempdir("cache-disposable");
    assert_eq!(terse_cli::run(["terse", "init"], &tmp), 0);

    // First build: cold cache (none exists yet).
    assert_eq!(terse_cli::run(["terse", "build", "--tex-only"], &tmp), 0);
    let output_dir = tmp.join("build").join("academic");
    let warm_baseline = hash_tree(&output_dir);

    let cache_dir = tmp.join(".terse-cache");
    assert!(cache_dir.is_dir(), "a disposable cache directory must exist after a build");

    // Second build: warm cache must reproduce identical output.
    assert_eq!(terse_cli::run(["terse", "build", "--tex-only"], &tmp), 0);
    assert_eq!(hash_tree(&output_dir), warm_baseline, "a warm cache must not change output");

    // Corrupt every cache entry: must be treated as a silent miss, never
    // an error or a crash.
    for entry in fs::read_dir(&cache_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            fs::write(&path, b"not a valid cache entry").unwrap();
        }
    }
    assert_eq!(
        terse_cli::run(["terse", "build", "--tex-only"], &tmp),
        0,
        "a corrupted cache entry must be treated as a miss, not fail the build"
    );
    assert_eq!(hash_tree(&output_dir), warm_baseline, "a corrupted cache must not change output");

    // Deleting the whole cache directory must only cost a recompute.
    fs::remove_dir_all(&cache_dir).unwrap();
    assert_eq!(
        terse_cli::run(["terse", "build", "--tex-only"], &tmp),
        0,
        "a missing cache directory must never fail a build"
    );
    assert_eq!(hash_tree(&output_dir), warm_baseline, "output must be identical with no cache at all");
}

/// Toolchain commands classify their failures like every other command:
/// a failed doctor check is a validation failure (`1`), an unusable
/// explicit toolchain selection is a configuration error (`2`), and a
/// provisioning failure such as a checksum mismatch is a tool failure
/// (`3`; that part is exercised once `terse toolchain install` exists in
/// the managed-toolchain change's group 3).
#[test]
fn test_toolchain_commands_classify_exit_codes() {
    use terse_cli::engine::{FakeProcessRunner, ProcessOutcome};
    use terse_cli::toolchain::{HostEnv, HostOs, ToolchainSelector};

    let root = tempdir("toolchain-exit-codes");
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
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n",
    )
    .unwrap();
    fs::write(project.join("paper.trs"), "document:\n  title: \"T\"\n\nHello.\n").unwrap();
    let mut host = HostEnv::minimal(HostOs::Linux, path_dir.as_os_str().to_os_string());
    host.home = Some(root.join("home"));
    host.tmpdir = Some(root.join("tmp").to_string_lossy().into_owned());
    host.username = Some("tester".to_string());

    // 1: doctor against a hung biber.
    let mut runner = FakeProcessRunner::new(vec![
        ProcessOutcome::success_with_log(&b"XeTeX (TeX Live 2025)\n"[..]),
        ProcessOutcome::timed_out(),
    ]);
    let opts = terse_cli::doctor::DoctorOptions { fix: false, json: true, toolchain: Some(ToolchainSelector::System) };
    let report = terse_cli::doctor::run_doctor(&project, &opts, &host, &mut runner);
    assert_eq!(terse_cli::doctor::exit_code(&report), 1);
    let parsed: serde_json::Value = serde_json::from_str(&terse_cli::doctor::render_json(&report)).unwrap();
    assert!(parsed["checks"].is_array());

    // 2: build with an explicit managed selection and no installed prefix.
    let mut runner = FakeProcessRunner::new(vec![]);
    let code = terse_cli::build::run_build_with_host(
        &project, None, None, false, false, true, Some(&ToolchainSelector::Managed), &host, &mut runner,
    );
    assert_eq!(code, 2);
    assert!(runner.invocations.is_empty());

    // 3: toolchain install against a downloader whose bytes do not match
    // the pinned checksum (provisioning/download failures are tool failures).
    for name in ["perl", "tar", "xz", "curl"] {
        fs::write(path_dir.join(name), "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path_dir.join(name), fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    let mut factory = || -> Box<dyn terse_cli::toolchain::download::ArchiveDownloader> {
        let mut fake = terse_cli::toolchain::download::FakeArchiveDownloader::new();
        let spec = terse_core::toolchain::spec::toolchain_spec_for("2025").unwrap();
        fake.serve(&spec.install_tl.unix.url, b"not the pinned installer".to_vec());
        Box::new(fake)
    };
    let prefix = root.join("managed-prefix");
    let code = terse_cli::run_with_host_and_downloader(
        ["terse", "toolchain", "install", "--prefix", prefix.to_str().unwrap()],
        &project,
        &host,
        &mut factory,
    );
    assert_eq!(code, 3);
    assert!(!prefix.exists());
}

/// `managed-toolchain` scenario "Version flag": the real binary prints the
/// workspace package version on stdout and exits `0`.
#[test]
fn test_version_flag_prints_cargo_version() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_terse"))
        .arg("--version")
        .output()
        .expect("the terse binary runs");
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("terse {}\n", env!("CARGO_PKG_VERSION"))
    );
}
