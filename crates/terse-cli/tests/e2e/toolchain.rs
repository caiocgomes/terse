//! Managed toolchain and doctor cases that need a real XeLaTeX/Biber.

use std::fs;

use crate::common::{tempdir, ENGINE_LOCK};

/// Runs `terse doctor` against whichever real toolchain this environment
/// has: the managed prefix when one is installed for the profile year,
/// otherwise the system TeX on `PATH` (`auto` resolution). Every check
/// must be `ok` or `info` and the embedded micro-compile must produce a
/// PDF.
#[test]
#[ignore = "needs a real XeLaTeX/Biber toolchain (managed prefix or system PATH)"]
fn test_doctor_microcompile_succeeds_on_managed_prefix() {
    let _engine_guard = ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = tempdir("doctor-e2e");
    fs::create_dir_all(&root).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_terse"))
        .args(["doctor", "--json", "--toolchain", "auto"])
        .current_dir(&root)
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("stdout is JSON: {e}\n{stdout}"));
    let checks = value["checks"].as_array().unwrap();
    let failing: Vec<String> = checks
        .iter()
        .filter(|c| c["status"] == "fail")
        .map(|c| format!("{}: {}", c["id"], c["summary"]))
        .collect();
    assert!(failing.is_empty(), "no failing check on a working toolchain: {failing:?}\n{}", String::from_utf8_lossy(&out.stderr));
    let micro = checks.iter().find(|c| c["id"] == "compile.micro").expect("micro-compile check present");
    assert_eq!(micro["status"], "ok", "{micro}");
    assert_eq!(out.status.code(), Some(0));
}

fn managed_prefix() -> Option<std::path::PathBuf> {
    use terse_cli::toolchain::{paths, HostEnv};
    let host = HostEnv::capture();
    let prefix = paths::managed_prefix(&host, "2025")?;
    prefix.join(terse_cli::toolchain::lock::LOCK_FILE_NAME).exists().then_some(prefix)
}

fn terse(args: &[&str], cwd: &std::path::Path) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_terse"))
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("binary runs")
}

/// `toolchain status --archive` on the installed managed prefix, then a
/// fresh offline install from that archive and a required-PDF build of
/// the full-paper fixture under both themes through the new prefix. No
/// network: the install path receives no downloader when `--offline`.
#[test]
#[ignore = "needs an installed managed TeX Live 2025 prefix (terse toolchain install)"]
fn test_offline_install_then_build_require_pdf() {
    let _engine_guard = ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let source = managed_prefix().expect("run `terse toolchain install` first");
    let root = tempdir("offline-e2e");
    let archive = root.join("managed-prefix.tar.gz");
    let out = terse(&["toolchain", "status", "--archive", archive.to_str().unwrap(), "--prefix", source.to_str().unwrap()], &root);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(archive.exists());

    let prefix = root.join("fresh-prefix");
    let out = terse(
        &["toolchain", "install", "--offline", "--from", archive.to_str().unwrap(), "--prefix", prefix.to_str().unwrap()],
        &root,
    );
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(prefix.join(terse_cli::toolchain::lock::LOCK_FILE_NAME).exists());
    assert!(prefix.join(".terse-owned.json").exists());

    let project = root.join("paper");
    copy_dir(&crate::common::full_paper_fixture_dir(), &project);
    for theme in ["academic", "magalu"] {
        let out = terse(&["build", "--require-pdf", "--theme", theme, "--toolchain", prefix.to_str().unwrap()], &project);
        assert!(out.status.success(), "{theme}: {}", String::from_utf8_lossy(&out.stderr));
        let pdf = project.join("build").join(theme).join("paper.pdf");
        assert!(pdf.exists(), "{theme} PDF published");
        let text = crate::common::pdf::extract_text(&pdf);
        assert!(text.contains("Terse"), "{theme}: PDF text contains fixture prose");
    }
}

/// Drift guard for the pinned closure: compiles the fixture under both
/// themes with `-recorder` using the managed prefix, maps every `INPUT`
/// under `texmf-dist` (and `bin/`) to its owning package through the
/// prefix's own `texlive.tlpdb`, and asserts every owner is in the
/// profile's derived-or-manual closure. A missing package is named.
#[test]
#[ignore = "needs an installed managed TeX Live 2025 prefix (terse toolchain install)"]
fn test_pinned_closure_covers_full_paper_inputs() {
    let _engine_guard = ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prefix = managed_prefix().expect("run `terse toolchain install` first");
    let spec = terse_core::toolchain::spec::toolchain_spec_for("2025").unwrap();
    let tlpdb = fs::read_to_string(prefix.join("tlpkg").join("texlive.tlpdb")).expect("installed catalog");
    let host = terse_cli::toolchain::HostEnv::capture();
    let tc = terse_cli::toolchain::resolve(&terse_cli::toolchain::ToolchainSelector::Dir(prefix.clone()), "2025", &host)
        .expect("managed prefix resolves");
    let bin = tc.bin_dir.clone().unwrap();
    let env = terse_cli::toolchain::prepare_child_env(&tc, &host, &terse_cli::toolchain::paths::texmf_dirs(&host));

    let root = tempdir("closure-e2e");
    let project = root.join("paper");
    copy_dir(&crate::common::full_paper_fixture_dir(), &project);
    let mut inputs = std::collections::BTreeSet::new();
    for theme in ["academic", "magalu"] {
        let out = terse(&["build", "--tex-only", "--theme", theme], &project);
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let dir = project.join("build").join(theme);
        let run = |program: &str, args: &[&str]| {
            let mut cmd = std::process::Command::new(bin.join(program));
            cmd.args(args).current_dir(&dir).env_clear();
            for (k, v) in &env {
                cmd.env(k, v);
            }
            let out = cmd.output().expect("tool runs");
            assert!(out.status.success(), "{program} {args:?} failed:\n{}", String::from_utf8_lossy(&out.stdout));
        };
        let xelatex = ["-no-shell-escape", "-interaction=nonstopmode", "-halt-on-error", "-file-line-error", "-recorder", "paper.tex"];
        run("xelatex", &xelatex);
        run("biber", &["paper"]);
        run("xelatex", &xelatex);
        run("xelatex", &xelatex);
        let fls = fs::read_to_string(dir.join("paper.fls")).unwrap();
        for line in fls.lines().filter_map(|l| l.strip_prefix("INPUT ")) {
            let path = std::path::Path::new(line.trim());
            let abs = if path.is_absolute() { path.to_path_buf() } else { dir.join(path) };
            let abs = abs.canonicalize().unwrap_or(abs);
            let canon_prefix = prefix.canonicalize().unwrap();
            if let Ok(rel) = abs.strip_prefix(&canon_prefix) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                if rel.starts_with("texmf-dist/") || rel.starts_with("bin/") {
                    inputs.insert(rel);
                }
            }
        }
    }
    assert!(inputs.len() > 50, "recorder saw {} distribution inputs", inputs.len());
    let files: Vec<&str> = inputs.iter().map(String::as_str).collect();
    let owners = terse_core::toolchain::tlpdb::owning_packages(&tlpdb, &files);
    let unmapped: Vec<&&str> = files.iter().filter(|f| !owners.contains_key(**f)).collect();
    assert!(unmapped.is_empty(), "inputs with no owning package: {unmapped:?}");
    let closure: std::collections::BTreeSet<String> =
        spec.closure.derived.iter().chain(spec.closure.manual.iter()).cloned().collect();
    let missing: std::collections::BTreeSet<&str> = owners
        .values()
        .map(|p| p.split('.').next().unwrap_or(p))
        .filter(|p| !closure.contains(*p))
        .collect();
    assert!(missing.is_empty(), "packages used by the fixture but absent from the pinned closure: {missing:?}");
}

/// Gate M primary case. The provisioning itself (`terse toolchain install`,
/// the only networked step) happens before this test runs: in CI it is the
/// `RUN` line of `tests/toolchain/Dockerfile`, on a workstation it is the
/// documented command. This test then proves the provisioned prefix is the
/// one the committed profile describes and that it compiles a real paper,
/// without any network: the lock names the profile year, one of the pinned
/// repositories, the pinned installer checksum, and every package of the
/// derived-plus-manual closure; `doctor` reports no failing check; and a
/// required-PDF build of the fixture succeeds through the managed prefix.
#[test]
#[ignore = "needs an installed managed TeX Live 2025 prefix (terse toolchain install)"]
fn test_managed_toolchain_provisions_ci_image() {
    let _engine_guard = ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prefix = managed_prefix().expect("run `terse toolchain install` first");
    let spec = terse_core::toolchain::spec::toolchain_spec_for("2025").unwrap();
    let lock = terse_cli::toolchain::lock::ToolchainLock::read_from_prefix(&prefix)
        .expect("lock file present")
        .expect("lock decodes");
    assert_eq!(lock.texlive_year, spec.texlive_year, "lock year matches the profile");
    assert!(
        spec.repositories.iter().any(|r| r.trim_end_matches('/') == lock.repository.trim_end_matches('/')),
        "lock repository {} is one of the pinned repositories {:?}",
        lock.repository,
        spec.repositories
    );
    assert_eq!(lock.install_tl_sha512, spec.install_tl.unix.sha512, "lock records the pinned installer checksum");
    let locked: std::collections::BTreeSet<&str> = lock.packages.iter().map(|p| p.name.as_str()).collect();
    let missing: Vec<&String> = spec
        .closure
        .derived
        .iter()
        .chain(spec.closure.manual.iter())
        .filter(|p| p.as_str() != "scheme-infraonly" && !locked.contains(p.as_str()))
        .collect();
    assert!(missing.is_empty(), "closure packages absent from the provisioned lock: {missing:?}");
    assert!(lock.packages.iter().all(|p| !p.revision.is_empty()), "every locked package has a revision");

    let root = tempdir("provision-e2e");
    fs::create_dir_all(&root).unwrap();
    let out = terse(&["doctor", "--json", "--toolchain", "managed"], &root);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("stdout is JSON: {e}\n{stdout}"));
    let failing: Vec<String> = value["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["status"] == "fail")
        .map(|c| format!("{}: {}", c["id"], c["summary"]))
        .collect();
    assert!(failing.is_empty(), "doctor on the provisioned prefix: {failing:?}\n{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(value["toolchain"]["source"], "managed", "{}", value["toolchain"]);
    assert_eq!(out.status.code(), Some(0));

    let project = root.join("paper");
    copy_dir(&crate::common::full_paper_fixture_dir(), &project);
    let out = terse(&["build", "--require-pdf", "--theme", "academic", "--toolchain", "managed"], &project);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let pdf = project.join("build").join("academic").join("paper.pdf");
    let text = crate::common::pdf::extract_text(&pdf);
    assert!(text.contains("Terse") && text.contains("Turing"), "PDF carries fixture prose and a resolved citation");
}

/// Real-process counterpart of the fake-runner fix test: `xelatex` and
/// `kpsewhich` are the managed prefix's real binaries, `biber` is a script
/// that never exits, and a stale-looking `par-<hex(user)>` directory is
/// planted in a private `TMPDIR` next to an unrelated sentinel. `doctor
/// --fix` must let the real probe time out (process group terminated),
/// name that directory as the probable cause, delete exactly it, report
/// the action, re-probe, and still exit `1` because the stand-in keeps
/// hanging.
#[cfg(unix)]
#[test]
#[ignore = "needs an installed managed TeX Live 2025 prefix; two real 10 s probe timeouts"]
fn test_doctor_detects_corrupt_par_cache_and_fixes_it() {
    use std::os::unix::fs::PermissionsExt;
    let _engine_guard = ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prefix = managed_prefix().expect("run `terse toolchain install` first");
    let host = terse_cli::toolchain::HostEnv::capture();
    let real = terse_cli::toolchain::resolve(&terse_cli::toolchain::ToolchainSelector::Dir(prefix.clone()), "2025", &host)
        .expect("managed prefix resolves");
    let root = tempdir("par-fix-e2e");
    let bin = root.join("tc").join("bin").join("fake-platform");
    fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(real.xelatex.as_ref().unwrap(), bin.join("xelatex")).unwrap();
    std::os::unix::fs::symlink(real.kpsewhich.as_ref().unwrap(), bin.join("kpsewhich")).unwrap();
    fs::write(bin.join("biber"), "#!/bin/sh\nsleep 600\n").unwrap();
    fs::set_permissions(bin.join("biber"), fs::Permissions::from_mode(0o755)).unwrap();

    let tmp = root.join("tmp");
    // The username is pinned rather than read from the ambient environment
    // and then handed to the child below, so the test computes the same
    // PAR path the child will. Reading `USER` here made this case fail in
    // the pinned container, where none of `USER`/`USERNAME`/`LOGNAME` is
    // set: a test that cannot run in the acceptance environment cannot
    // protect it.
    let username = "terse-test-user";
    let par = terse_core::toolchain::par::par_cache_dir(&tmp, username);
    fs::create_dir_all(par.join("cache-x")).unwrap();
    fs::write(par.join("cache-x").join("file"), b"stale").unwrap();
    fs::write(tmp.join("sibling.txt"), b"keep me").unwrap();

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_terse"))
        .args(["doctor", "--fix", "--json", "--toolchain", root.join("tc").to_str().unwrap()])
        .env("TMPDIR", &tmp)
        .env("USER", username)
        .current_dir(&root)
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("stdout is JSON: {e}\n{stdout}"));
    let biber = value["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "biber.runs")
        .expect("biber.runs check present");
    assert_eq!(biber["status"], "fail", "{biber}");
    let cause = biber["probable_cause"].as_str().unwrap_or("");
    assert!(cause.contains(par.to_str().unwrap()), "probable cause names the planted directory: {cause}");
    let actions = value["actions"].to_string();
    assert!(actions.contains(par.to_str().unwrap()), "the removal is reported as an action: {actions}");
    assert!(!par.exists(), "stale PAR cache removed");
    assert_eq!(fs::read(tmp.join("sibling.txt")).unwrap(), b"keep me", "unrelated sibling untouched");
    assert_eq!(out.status.code(), Some(1), "the stand-in keeps hanging, so doctor still fails");
    assert!(
        std::process::Command::new("pgrep").args(["-f", "sleep 600"]).output().map(|o| o.stdout.is_empty()).unwrap_or(true),
        "the hung stand-in's process group was terminated"
    );
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dst.join(entry.file_name());
        if entry.path().is_dir() {
            if entry.file_name() == "build" || entry.file_name() == ".terse-cache" {
                continue;
            }
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}
