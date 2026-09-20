//! `terse doctor`: bounded checks with causes, safe fixes, and the
//! definitive micro-compile. Engine-free: tool presence is simulated with
//! sentinel executables, execution with the fake runner.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use terse_cli::doctor::{self, CheckStatus, DoctorOptions, DoctorReport};
use terse_cli::engine::{FakeProcessRunner, ProcessOutcome};
use terse_cli::toolchain::{HostEnv, HostOs, ToolchainSelector};

use common::{executable_dir, tempdir};

const ENTRY: &str = "document:\n  title: \"Doctor\"\n\nHello.\n";
const MANIFEST: &str = "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n";

fn linux_host(path_dir: &Path, tmpdir: &Path) -> HostEnv {
    let mut host = HostEnv::minimal(HostOs::Linux, path_dir.as_os_str().to_os_string());
    host.tmpdir = Some(tmpdir.to_string_lossy().into_owned());
    host.username = Some("cgomes".to_string());
    host.home = Some(tmpdir.join("home"));
    host
}

fn options(fix: bool) -> DoctorOptions {
    DoctorOptions { fix, json: false, toolchain: Some(ToolchainSelector::System) }
}

fn project(root: &Path) -> PathBuf {
    let p = root.join("project");
    fs::create_dir_all(&p).unwrap();
    fs::write(p.join("terse.toml"), MANIFEST).unwrap();
    fs::write(p.join("paper.trs"), ENTRY).unwrap();
    p
}

/// A recursive `path -> sha256` map, so a whole tree can be proven
/// unchanged by a read-only command.
fn tree_hash(root: &Path) -> BTreeMap<String, String> {
    use sha2::{Digest, Sha256};
    let mut out = BTreeMap::new();
    fn walk(dir: &Path, base: &Path, out: &mut BTreeMap<String, String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                walk(&p, base, out);
            } else {
                let mut h = Sha256::new();
                h.update(fs::read(&p).unwrap());
                out.insert(p.strip_prefix(base).unwrap().to_string_lossy().into_owned(), format!("{:x}", h.finalize()));
            }
        }
    }
    walk(root, root, &mut out);
    out
}

fn check<'a>(report: &'a DoctorReport, id: &str) -> &'a doctor::Check {
    report
        .checks
        .iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("report has no check '{id}': {:?}", report.checks.iter().map(|c| &c.id).collect::<Vec<_>>()))
}

#[test]
fn test_doctor_checks_have_timeouts_and_causes() {
    let root = tempdir("doctor-causes");
    let tmp = root.join("tmp");
    fs::create_dir_all(&tmp).unwrap();
    let path_dir = executable_dir(&root.join("path"), &["xelatex", "biber", "kpsewhich", "perl", "curl", "tar", "xz"]);
    let host = linux_host(&path_dir, &tmp);
    let proj = project(&root);
    let before = tree_hash(&root);

    // xelatex answers with a 2025 banner; biber never exits; everything
    // after that succeeds (kpsewhich lookups, df, micro-compile passes).
    let mut runner = FakeProcessRunner::new(vec![
        ProcessOutcome::success_with_log(&b"XeTeX 3.14 (TeX Live 2025)\n"[..]),
        ProcessOutcome::timed_out(),
    ]);
    let report = doctor::run_doctor(&proj, &options(false), &host, &mut runner);

    let biber = check(&report, "biber.runs");
    assert_eq!(biber.status, CheckStatus::Fail);
    let expected_dir = terse_core::toolchain::par::par_cache_dir(&tmp, "cgomes");
    let cause = biber.probable_cause.as_deref().expect("a hung biber names its probable cause");
    assert!(cause.contains(&expected_dir.to_string_lossy().into_owned()), "cause names the PAR cache dir: {cause}");
    let fix = biber.fix_command.as_deref().expect("a fix command is offered");
    assert!(fix.contains("rm -rf") && fix.contains("par-63676f6d6573"), "{fix}");
    assert!(biber.auto_fixable);
    assert!(biber.code.is_some_and(|c| c.starts_with("E-TOOL-")), "{:?}", biber.code);

    assert_eq!(check(&report, "xelatex.runs").status, CheckStatus::Ok);
    assert!(check(&report, "xelatex.runs").evidence.iter().any(|e| e.contains("2025")));

    assert!(!runner.invocations.is_empty());
    assert_eq!(runner.timeouts.len(), runner.invocations.len());
    assert!(
        runner.timeouts.iter().all(|t| *t > Duration::ZERO && *t <= Duration::from_secs(120)),
        "every probe is bounded: {:?}",
        runner.timeouts
    );
    // The first two invocations are the two version probes, each with the
    // probe timeout, not the engine's compile timeout.
    assert!(runner.invocations[0].program.ends_with("xelatex"));
    assert_eq!(runner.invocations[0].args, vec!["--version".to_string()]);
    assert!(runner.invocations[1].program.ends_with("biber"));
    assert_eq!(runner.timeouts[0], terse_cli::toolchain::probe::PROBE_TIMEOUT);
    assert_eq!(runner.timeouts[1], terse_cli::toolchain::probe::PROBE_TIMEOUT);

    assert_eq!(doctor::exit_code(&report), 1);
    assert_eq!(tree_hash(&root), before, "doctor without --fix writes nothing");

    // Edge: a missing xelatex fails presence and skips the micro-compile
    // with an informational check, never a spurious failure.
    let path_dir = executable_dir(&root.join("path-no-xelatex"), &["perl", "curl", "tar", "xz"]);
    let host = linux_host(&path_dir, &tmp);
    let mut runner = FakeProcessRunner::new(vec![]);
    let report = doctor::run_doctor(&proj, &options(false), &host, &mut runner);
    assert_eq!(check(&report, "xelatex.present").status, CheckStatus::Fail);
    assert_eq!(check(&report, "compile.micro").status, CheckStatus::Info);
    assert!(runner.invocations.iter().all(|i| !i.program.ends_with("xelatex")));
    assert_eq!(doctor::exit_code(&report), 1);
}

#[test]
fn test_doctor_json_envelope_is_versioned() {
    let root = tempdir("doctor-json");
    let tmp = root.join("tmp");
    fs::create_dir_all(&tmp).unwrap();
    let path_dir = executable_dir(&root.join("path"), &["xelatex", "biber", "kpsewhich", "perl", "curl", "tar", "xz"]);
    let proj = project(&root);

    let run = |lang: &str| -> String {
        let mut host = linux_host(&path_dir, &tmp);
        // Locale/terminal-ish differences must not reach the report.
        host.systemroot = Some(lang.to_string());
        let mut runner = FakeProcessRunner::new(vec![
            ProcessOutcome::success_with_log(&b"XeTeX 3.14 (TeX Live 2025)\n"[..]),
            ProcessOutcome::timed_out(),
        ]);
        let report = doctor::run_doctor(&proj, &options(false), &host, &mut runner);
        doctor::render_json(&report)
    };
    let json1 = run("C");
    let json2 = run("pt_BR.UTF-8");

    let value: serde_json::Value = serde_json::from_str(&json1).expect("stdout payload parses as JSON");
    assert_eq!(value["version"], serde_json::json!(terse_cli::diagnostics::JSON_ENVELOPE_VERSION));
    assert!(value["terse_version"].is_string());
    assert_eq!(value["toolchain"]["source"], "system");
    let ids1: Vec<String> = value["checks"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap().to_string()).collect();
    let statuses1: Vec<String> =
        value["checks"].as_array().unwrap().iter().map(|c| c["status"].as_str().unwrap().to_string()).collect();
    let value2: serde_json::Value = serde_json::from_str(&json2).unwrap();
    let ids2: Vec<String> = value2["checks"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap().to_string()).collect();
    let statuses2: Vec<String> =
        value2["checks"].as_array().unwrap().iter().map(|c| c["status"].as_str().unwrap().to_string()).collect();
    assert_eq!(ids1, ids2);
    assert_eq!(statuses1, statuses2);

    // The reviewed golden pins the check identifiers, their order, and
    // their statuses for this scenario.
    let golden: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(common::fixtures_dir().join("toolchain/doctor-report.json")).unwrap())
            .unwrap();
    assert_eq!(golden["version"], value["version"]);
    let golden_pairs: Vec<(String, String)> = golden["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["id"].as_str().unwrap().to_string(), c["status"].as_str().unwrap().to_string()))
        .collect();
    let actual_pairs: Vec<(String, String)> = ids1.into_iter().zip(statuses1).collect();
    assert_eq!(actual_pairs, golden_pairs, "check ids/order/statuses match the reviewed golden");
}

#[test]
fn test_doctor_json_goes_to_stdout_only() {
    // Process-level: with no tools on PATH and no project, `doctor --json`
    // must still emit exactly one JSON document on stdout and keep every
    // line of prose on stderr.
    let root = tempdir("doctor-stdout");
    let empty_path = root.join("empty-path");
    fs::create_dir_all(&empty_path).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_terse"))
        .args(["doctor", "--json", "--toolchain", "system"])
        .current_dir(&root)
        .env_clear()
        .env("PATH", &empty_path)
        .env("HOME", root.join("home"))
        .env("TMPDIR", root.join("tmp"))
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("stdout is JSON: {e}\n{stdout}"));
    assert_eq!(value["version"], serde_json::json!(terse_cli::diagnostics::JSON_ENVELOPE_VERSION));
    assert_eq!(out.status.code(), Some(1), "no xelatex is a failed check");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(serde_json::from_str::<serde_json::Value>(stderr.trim()).is_err(), "stderr is prose, not JSON: {stderr}");
}

#[test]
fn test_doctor_fix_removes_only_stale_par_cache() {
    let root = tempdir("doctor-fix");
    let tmp = root.join("tmp");
    let par = terse_core::toolchain::par::par_cache_dir(&tmp, "cgomes");
    fs::create_dir_all(par.join("cache-x")).unwrap();
    fs::write(par.join("cache-x").join("file"), "stale").unwrap();
    fs::write(tmp.join("sibling.txt"), "keep me").unwrap();
    let sibling_before = tree_hash(&tmp).get("sibling.txt").cloned().unwrap();
    let path_dir = executable_dir(&root.join("path"), &["xelatex", "biber", "kpsewhich", "perl", "curl", "tar", "xz"]);
    let host = linux_host(&path_dir, &tmp);
    let proj = project(&root);

    // First biber probe hangs; after the fix it answers.
    let mut runner = FakeProcessRunner::new(vec![
        ProcessOutcome::success_with_log(&b"XeTeX 3.14 (TeX Live 2025)\n"[..]),
        ProcessOutcome::timed_out(),
        ProcessOutcome::success_with_log(&b"biber version: 2.21\n"[..]),
    ]);
    let report = doctor::run_doctor(&proj, &options(true), &host, &mut runner);

    assert!(!par.exists(), "the stale PAR cache directory is removed");
    assert_eq!(tree_hash(&tmp).get("sibling.txt").cloned().unwrap(), sibling_before, "the sibling is byte-identical");
    assert!(
        report.actions.iter().any(|a| a.contains(&par.to_string_lossy().into_owned())),
        "the report lists the removal: {:?}",
        report.actions
    );
    let biber_probes = runner.invocations.iter().filter(|i| i.program.ends_with("biber") && i.args == ["--version"]).count();
    assert_eq!(biber_probes, 2, "biber is probed again after the fix");
    assert_eq!(check(&report, "biber.runs").status, CheckStatus::Ok);
}

#[test]
fn test_doctor_fix_never_writes_without_flag() {
    let root = tempdir("doctor-nofix");
    let tmp = root.join("tmp");
    let par = terse_core::toolchain::par::par_cache_dir(&tmp, "cgomes");
    fs::create_dir_all(par.join("cache-x")).unwrap();
    fs::write(par.join("cache-x").join("file"), "stale").unwrap();
    fs::write(tmp.join("sibling.txt"), "keep me").unwrap();
    let path_dir = executable_dir(&root.join("path"), &["xelatex", "biber", "kpsewhich", "perl", "curl", "tar", "xz"]);
    let host = linux_host(&path_dir, &tmp);
    let proj = project(&root);
    let before = tree_hash(&root);

    let mut runner = FakeProcessRunner::new(vec![
        ProcessOutcome::success_with_log(&b"XeTeX 3.14 (TeX Live 2025)\n"[..]),
        ProcessOutcome::timed_out(),
    ]);
    let report = doctor::run_doctor(&proj, &options(false), &host, &mut runner);

    assert_eq!(tree_hash(&root), before, "nothing under the temp tree or the project changed");
    assert!(par.exists());
    let biber = check(&report, "biber.runs");
    assert!(biber.auto_fixable, "the repair is marked auto-fixable");
    assert!(report.actions.is_empty(), "no action applied without --fix");
}

#[test]
fn test_doctor_microcompile_reports_failure_distinctly() {
    let root = tempdir("doctor-micro");
    let tmp = root.join("tmp");
    fs::create_dir_all(&tmp).unwrap();
    let path_dir = executable_dir(&root.join("path"), &["xelatex", "biber", "kpsewhich", "perl", "curl", "tar", "xz"]);
    let host = linux_host(&path_dir, &tmp);
    let proj = project(&root);

    // Version and lookup probes succeed; the micro-compile's biber pass
    // exits nonzero with a recognizable log line.
    let profile = terse_core::artifact::profile::resolve_profile("texlive-2025-xelatex").unwrap();
    let lookups = 1 /* TEXMFDIST */ + profile.packages.len() + 4 /* fonts */ + 2 /* babel */;
    let mut responses = vec![
        ProcessOutcome::success_with_log(&b"XeTeX 3.14 (TeX Live 2025)\n"[..]),
        ProcessOutcome::success_with_log(&b"biber version: 2.21\n"[..]),
    ];
    responses.extend(std::iter::repeat(ProcessOutcome::success_with_log(&b"/texmf/found\n"[..])).take(lookups));
    responses.push(ProcessOutcome::success()); // df
    responses.push(ProcessOutcome::success()); // micro xelatex pass 1
    let mut failing_biber = ProcessOutcome::nonzero(2);
    failing_biber.stdout = b"ERROR - Cannot find 'paper.bcf'!\n".to_vec();
    responses.push(failing_biber);
    let mut runner = FakeProcessRunner::new(responses);
    let report = doctor::run_doctor(&proj, &options(false), &host, &mut runner);

    let micro = check(&report, "compile.micro");
    assert_eq!(micro.status, CheckStatus::Fail);
    assert!(
        micro.evidence.iter().any(|e| e.contains("Cannot find 'paper.bcf'")),
        "the biber log excerpt is evidence: {:?}",
        micro.evidence
    );
    assert!(micro.code.is_some_and(|c| c.starts_with("E-TOOL-")));
    for id in ["xelatex.present", "xelatex.runs", "biber.present", "biber.runs", "packages.resolvable", "fonts.resolvable"] {
        assert_eq!(check(&report, id).status, CheckStatus::Ok, "{id}");
    }
    // The micro-compile used the fixed stem and the prepared environment.
    let micro_invocations: Vec<_> = runner
        .invocations
        .iter()
        .filter(|i| i.args.iter().any(|a| a == "paper.tex" || a == "paper"))
        .collect();
    assert!(micro_invocations.len() >= 2, "{:?}", runner.invocations);
    for inv in micro_invocations {
        assert!(inv.env.iter().any(|(k, _)| k == "TEXMFHOME"), "prepared env: {:?}", inv.env);
        assert!(inv.env.iter().all(|(k, _)| terse_cli::toolchain::env::EMITTED_KEYS.contains(&k.as_str())));
    }
    assert_eq!(doctor::exit_code(&report), 1);
}
