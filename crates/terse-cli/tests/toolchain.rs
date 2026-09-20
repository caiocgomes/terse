//! Toolchain resolution: precedence, platform-correct discovery, and the
//! explicit-selection failure modes. Engine-free: tool presence is
//! simulated with sentinel executables, execution with the fake runner.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use terse_cli::engine::{FakeProcessRunner, ProcessOutcome};
use terse_cli::toolchain::{self, HostEnv, HostOs, ToolchainSelector, ToolchainSource};

use common::{executable_dir, install_fake_prefix, tempdir};

const ENTRY: &str = "document:\n  title: \"Toolchain\"\n\n# Intro\n\nHello.\n";

fn manifest(toolchain_line: Option<&str>) -> String {
    let mut m = String::from("format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n");
    if let Some(line) = toolchain_line {
        m.push_str("\n[latex]\n");
        m.push_str(line);
        m.push('\n');
    }
    m
}

fn linux_host(path_dir: &Path, xdg: &Path) -> HostEnv {
    let mut host = HostEnv::minimal(HostOs::Linux, path_dir.as_os_str().to_os_string());
    host.xdg_data_home = Some(xdg.to_path_buf());
    host.home = Some(xdg.join("home"));
    host
}

fn managed_prefix_for(host: &HostEnv) -> PathBuf {
    toolchain::paths::managed_prefix(host, "2025").expect("linux host with XDG has a data dir")
}

fn text_artifacts(root: &Path) -> Vec<(String, Vec<u8>)> {
    let dir = root.join("build").join("academic");
    let mut out = Vec::new();
    for name in ["paper.tex", "terse-style.sty", "paper.bib"] {
        if let Ok(bytes) = fs::read(dir.join(name)) {
            out.push((name.to_string(), bytes));
        }
    }
    assert!(!out.is_empty(), "a build must publish text artifacts");
    out
}

#[test]
fn test_toolchain_precedence_is_documented() {
    let root = tempdir("precedence");
    let xdg = root.join("xdg");
    let path_dir = executable_dir(&root.join("path"), &["xelatex", "biber"]);
    let host = linux_host(&path_dir, &xdg);
    let prefix = managed_prefix_for(&host);
    install_fake_prefix(&prefix, "2025");

    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("paper.trs"), ENTRY).unwrap();

    // A required-PDF build through the fake runner records which engine
    // was resolved (the fake writes no PDF, so its exit code is not the
    // subject here); a `--tex-only` build publishes the text artifacts
    // whose bytes must not depend on the resolution.
    let engine_used = |flag: Option<&ToolchainSelector>| -> PathBuf {
        let mut runner = FakeProcessRunner::new(vec![ProcessOutcome::success(); 3]);
        let _ = terse_cli::build::run_build_with_host(
            &project, None, None, false, true, false, flag, &host, &mut runner,
        );
        assert!(!runner.invocations.is_empty(), "the resolved engine was invoked");
        runner.invocations[0].program.clone()
    };
    let published_text = |flag: Option<&ToolchainSelector>| -> Vec<(String, Vec<u8>)> {
        let mut runner = FakeProcessRunner::new(vec![]);
        let code = terse_cli::build::run_build_with_host(
            &project, None, None, true, false, false, flag, &host, &mut runner,
        );
        assert_eq!(code, 0);
        text_artifacts(&project)
    };
    let managed_xelatex = prefix.join("bin").join("fake-platform").join("xelatex");

    // 1. Flag beats a manifest that says `system`.
    fs::write(project.join("terse.toml"), manifest(Some("toolchain = \"system\""))).unwrap();
    let flag = ToolchainSelector::parse_flag("managed", &project);
    assert_eq!(engine_used(Some(&flag)), managed_xelatex);
    let first = published_text(Some(&flag));
    let selector = toolchain::select(Some(&flag), Some("system"), &project).unwrap();
    let tc = toolchain::resolve(&selector, "2025", &host).unwrap();
    assert!(matches!(tc.source, ToolchainSource::Managed { .. }));
    assert!(tc.reason.contains("managed"), "reason names its source: {}", tc.reason);

    // 2. No flag: the manifest's `system` wins over the installed prefix.
    assert_eq!(engine_used(None), path_dir.join("xelatex"));
    let second = published_text(None);
    let selector = toolchain::select(None, Some("system"), &project).unwrap();
    let tc = toolchain::resolve(&selector, "2025", &host).unwrap();
    assert_eq!(tc.source, ToolchainSource::System);
    assert!(tc.reason.contains("system"), "{}", tc.reason);

    // 3. No flag, no manifest line: auto prefers the matching managed prefix.
    fs::write(project.join("terse.toml"), manifest(None)).unwrap();
    assert_eq!(engine_used(None), managed_xelatex);
    let third = published_text(None);
    let selector = toolchain::select(None, None, &project).unwrap();
    let tc = toolchain::resolve(&selector, "2025", &host).unwrap();
    assert!(matches!(tc.source, ToolchainSource::Managed { .. }));
    assert!(tc.reason.contains("matches the profile year"), "{}", tc.reason);

    assert_eq!(first, second, "tool location never changes generated text");
    assert_eq!(second, third, "tool location never changes generated text");

    // Edge: `--toolchain DIR` beats a manifest `managed`.
    let explicit = root.join("explicit-prefix");
    install_fake_prefix(&explicit, "2025");
    fs::write(project.join("terse.toml"), manifest(Some("toolchain = \"managed\""))).unwrap();
    let flag = ToolchainSelector::parse_flag(explicit.to_str().unwrap(), &project);
    assert_eq!(
        engine_used(Some(&flag)),
        explicit.join("bin").join("fake-platform").join("xelatex")
    );

    // Edge: a manifest value outside the keywords/explicit paths is a
    // configuration error.
    fs::write(project.join("terse.toml"), manifest(Some("toolchain = \"bogus\""))).unwrap();
    let mut runner = FakeProcessRunner::new(vec![]);
    let code = terse_cli::build::run_build_with_host(
        &project, None, None, false, true, false, None, &host, &mut runner,
    );
    assert_eq!(code, 2);
    assert!(runner.invocations.is_empty());
}

#[test]
fn test_explicit_toolchain_without_engine_is_config_error() {
    let root = tempdir("explicit-missing");
    let xdg = root.join("xdg");
    let path_dir = executable_dir(&root.join("path"), &["xelatex"]);
    let host = linux_host(&path_dir, &xdg);

    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("terse.toml"), manifest(None)).unwrap();
    fs::write(project.join("paper.trs"), ENTRY).unwrap();

    // A previous successful build whose bytes must survive.
    let mut runner = FakeProcessRunner::new(vec![]);
    assert_eq!(
        terse_cli::build::run_build_with_host(&project, None, None, true, false, false, None, &host, &mut runner),
        0
    );
    let before = text_artifacts(&project);

    let flag = ToolchainSelector::parse_flag("managed", &project);
    let mut runner = FakeProcessRunner::new(vec![]);
    let code = terse_cli::build::run_build_with_host(
        &project, None, None, false, false, false, Some(&flag), &host, &mut runner,
    );
    assert_eq!(code, 2, "managed requested but not installed is a configuration error");
    assert!(runner.invocations.is_empty());

    let empty = root.join("empty-dir");
    fs::create_dir_all(&empty).unwrap();
    let flag = ToolchainSelector::parse_flag(empty.to_str().unwrap(), &project);
    let mut runner = FakeProcessRunner::new(vec![]);
    let code = terse_cli::build::run_build_with_host(
        &project, None, None, false, false, false, Some(&flag), &host, &mut runner,
    );
    assert_eq!(code, 2, "an explicit directory without xelatex is a configuration error");
    assert!(runner.invocations.is_empty());

    assert_eq!(text_artifacts(&project), before, "previous output untouched");

    // The diagnostic itself names the remedy.
    let selector = toolchain::select(Some(&ToolchainSelector::Managed), None, &project).unwrap();
    let err = toolchain::resolve(&selector, "2025", &host).unwrap_err();
    assert_eq!(err.code(), "E-TOOL-015");
    assert!(err.message().contains("terse toolchain install"));
}

#[cfg(unix)]
#[test]
fn test_unix_discovery_requires_executable_bit() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempdir("exec-bit");
    let plain = root.join("plain");
    fs::create_dir_all(&plain).unwrap();
    fs::write(plain.join("xelatex"), "not executable").unwrap();
    fs::set_permissions(plain.join("xelatex"), fs::Permissions::from_mode(0o644)).unwrap();
    let real = executable_dir(&root.join("real"), &["xelatex"]);

    let host = HostEnv::minimal(HostOs::current(), "");
    assert_eq!(
        toolchain::find_tool_in("xelatex", &[plain.clone(), real.clone()], &host),
        Some(real.join("xelatex")),
        "the executable one is returned, the plain file skipped"
    );
    assert_eq!(toolchain::find_tool_in("xelatex", &[plain], &host), None);
}

// ---------------------------------------------------------------------------
// Group 3: managed toolchain install / update / status / uninstall.
// ---------------------------------------------------------------------------

use std::cell::Cell;
use std::io::Write;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha512};
use terse_cli::engine::{ProcessInvocation, ProcessRunner};
use terse_cli::toolchain::archive;
use terse_cli::toolchain::download::{ArchiveDownloader, FakeArchiveDownloader};
use terse_cli::toolchain::install::{run_install, InstallError, InstallOptions};
use terse_cli::toolchain::lock::{ToolchainLock, LOCK_FILE_NAME};
use terse_core::toolchain::spec::{Closure, InstallTl, InstallTlArtifact, ToolchainSpec};

fn sha512_hex(bytes: &[u8]) -> String {
    let mut h = Sha512::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// A gzip tarball shaped like the real installer archive: one directory
/// with `install-tl` inside it.
fn fake_installer_archive() -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let enc = flate2::write::GzEncoder::new(&mut buf, flate2::Compression::default());
        let mut tar = tar::Builder::new(enc);
        let script = b"#!/usr/bin/env perl\nexit 0;\n";
        let mut header = tar::Header::new_gnu();
        header.set_size(script.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, "install-tl-20990101/install-tl", &script[..]).unwrap();
        tar.finish().unwrap();
    }
    buf
}

fn spec_for(archive_bytes: &[u8]) -> ToolchainSpec {
    ToolchainSpec {
        schema_version: 1,
        texlive_year: "2025".to_string(),
        export_profile: "texlive-2025-xelatex".to_string(),
        repositories: vec!["https://mirror.example.test/tlnet-final/".to_string()],
        install_tl: InstallTl {
            unix: InstallTlArtifact {
                url: "https://mirror.example.test/tlnet-final/install-tl-unx.tar.gz".to_string(),
                sha512: sha512_hex(archive_bytes),
            },
        },
        closure: Closure {
            derived_from: "test".to_string(),
            derived: vec!["booktabs".to_string(), "xetex".to_string()],
            manual: vec!["scheme-infraonly".to_string(), "biber".to_string()],
        },
    }
}

/// A host with every install prerequisite present as a sentinel and a
/// data directory under `root`.
fn provisioning_host(root: &Path) -> HostEnv {
    let path_dir = executable_dir(&root.join("path"), &["perl", "tar", "xz", "curl"]);
    let mut host = HostEnv::minimal(HostOs::Other, path_dir.as_os_str().to_os_string());
    host.xdg_data_home = Some(root.join("xdg"));
    host.home = Some(root.join("home"));
    host.tmpdir = Some(root.join("tmp").to_string_lossy().into_owned());
    fs::create_dir_all(root.join("tmp")).unwrap();
    host
}

/// Scripted outcomes like `FakeProcessRunner`, plus the one side effect a
/// real `install-tl` has that later steps depend on: it populates
/// `bin/<platform>/` inside its working directory (the staging root).
struct SideEffectRunner {
    inner: FakeProcessRunner,
}

impl ProcessRunner for SideEffectRunner {
    fn run(&mut self, invocation: &ProcessInvocation, timeout: std::time::Duration) -> ProcessOutcome {
        if invocation.args.iter().any(|a| a == "-profile") {
            executable_dir(
                &invocation.working_dir.join("bin").join("fake-platform"),
                &["xelatex", "biber", "kpsewhich", "tlmgr"],
            );
            fs::create_dir_all(invocation.working_dir.join("tlpkg")).unwrap();
            fs::write(invocation.working_dir.join("tlpkg").join("texlive.tlpdb"), "name booktabs\n\n").unwrap();
        }
        self.inner.run(invocation, timeout)
    }
}

fn hash_tree(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
        let mut entries: Vec<_> = fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                let rel = p.strip_prefix(base).unwrap().to_string_lossy().into_owned();
                out.push((rel, sha512_hex(&fs::read(&p).unwrap())));
            }
        }
    }
    walk(dir, dir, &mut out);
    out
}

fn staging_siblings(parent: &Path) -> Vec<PathBuf> {
    fs::read_dir(parent)
        .map(|it| {
            it.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.file_name().map(|n| n.to_string_lossy().starts_with(".terse-")).unwrap_or(false))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn test_install_rejects_checksum_mismatch_before_running_anything() {
    let root = tempdir("install-checksum");
    let host = provisioning_host(&root);
    let good = fake_installer_archive();
    let spec = spec_for(&good);
    let mut tampered = good.clone();
    tampered.push(0);
    let mut downloader = FakeArchiveDownloader::new();
    downloader.serve(&spec.install_tl.unix.url, tampered);
    let mut runner = FakeProcessRunner::new(vec![]);
    let prefix = root.join("xdg").join("terse").join("toolchain").join("texlive-2025");
    let opts = InstallOptions { prefix: prefix.clone(), offline_from: None, update: false };

    let err = run_install(&opts, &spec, &host, &mut runner, Some(&mut downloader), &mut |_| {})
        .expect_err("a checksum mismatch must fail the install");
    assert!(matches!(err, InstallError::ChecksumMismatch { .. }), "{err:?}");
    assert_eq!(err.code(), "E-TOOL-021");
    assert_eq!(err.exit_code(), 3);
    assert!(runner.invocations.is_empty(), "nothing from the archive is ever executed");
    assert_eq!(downloader.requests().len(), 1, "exactly the installer download");
    assert!(!prefix.exists());
    assert!(staging_siblings(prefix.parent().unwrap()).is_empty(), "no staging directory remains");
}

#[test]
fn test_install_fails_before_download_when_prerequisites_are_missing() {
    let root = tempdir("install-prereqs");
    let mut host = provisioning_host(&root);
    host.path = executable_dir(&root.join("path-no-perl"), &["tar", "xz", "curl"]).into_os_string();
    let good = fake_installer_archive();
    let spec = spec_for(&good);
    let mut downloader = FakeArchiveDownloader::new();
    downloader.serve(&spec.install_tl.unix.url, good);
    let mut runner = FakeProcessRunner::new(vec![]);
    let prefix = root.join("prefix");
    let opts = InstallOptions { prefix: prefix.clone(), offline_from: None, update: false };
    let err = run_install(&opts, &spec, &host, &mut runner, Some(&mut downloader), &mut |_| {}).unwrap_err();
    assert!(matches!(err, InstallError::Prerequisites(ref m) if m.contains(&"perl")), "{err:?}");
    assert_eq!(err.code(), "E-TOOL-020");
    assert!(downloader.requests().is_empty(), "no byte is downloaded without prerequisites");
    assert!(!prefix.exists());
}

#[test]
fn test_toolchain_install_publishes_lock_and_marker() {
    let root = tempdir("install-ok");
    let host = provisioning_host(&root);
    let good = fake_installer_archive();
    let spec = spec_for(&good);
    let mut downloader = FakeArchiveDownloader::new();
    downloader.serve(&spec.install_tl.unix.url, good);
    let mut runner = SideEffectRunner {
        inner: FakeProcessRunner::new(vec![
            ProcessOutcome::success(),
            ProcessOutcome::success(),
            ProcessOutcome::success_with_log(&b"xetex,73850\nbooktabs,77677\nbiber,75738\n"[..]),
        ]),
    };
    let prefix = root.join("xdg").join("terse").join("toolchain").join("texlive-2025");
    let opts = InstallOptions { prefix: prefix.clone(), offline_from: None, update: false };
    let mut progress = Vec::new();
    let summary = run_install(&opts, &spec, &host, &mut runner, Some(&mut downloader), &mut |line| progress.push(line.to_string()))
        .expect("scripted install succeeds");
    assert_eq!(summary.prefix, prefix);
    assert_eq!(summary.packages.len(), 3);

    let inv = &runner.inner.invocations;
    assert_eq!(inv.len(), 3, "install-tl, tlmgr install, tlmgr info");
    assert!(inv[0].program.ends_with("perl"), "install-tl runs through perl: {:?}", inv[0].program);
    assert!(inv[0].args.iter().any(|a| a.ends_with("install-tl")));
    assert!(inv[0].args.contains(&"-no-gui".to_string()));
    assert!(inv[0].args.contains(&"-repository".to_string()));
    assert!(inv[0].args.contains(&spec.repositories[0]));
    assert!(inv[1].program.ends_with("tlmgr"));
    assert_eq!(&inv[1].args[..3], &["--repository", spec.repositories[0].as_str(), "install"]);
    let installed: Vec<&str> = inv[1].args[3..].iter().map(String::as_str).collect();
    assert_eq!(installed, ["biber", "booktabs", "xetex"], "derived + manual, sorted, no scheme");
    assert_eq!(inv[2].args, ["info", "--only-installed", "--data", "name,localrev"]);
    for i in inv {
        let path = i.env.iter().find(|(k, _)| k == "PATH").map(|(_, v)| v.clone()).unwrap();
        assert!(!i.env.iter().any(|(k, _)| k.starts_with("TEXMF")), "provisioning env never carries TEXMF*: {:?}", i.env);
        assert!(path.contains("path"), "host PATH is present: {path}");
    }
    let tlmgr_path: String = inv[1].env.iter().find(|(k, _)| k == "PATH").map(|(_, v)| v.clone()).unwrap();
    assert!(tlmgr_path.starts_with(&*inv[1].program.parent().unwrap().to_string_lossy()), "managed bin first for tlmgr");
    assert_eq!(runner.inner.timeouts[0], std::time::Duration::from_secs(600));
    assert_eq!(runner.inner.timeouts[1], std::time::Duration::from_secs(1800));

    let lock = ToolchainLock::read_from_prefix(&prefix).unwrap().unwrap();
    assert_eq!(lock.texlive_year, "2025");
    assert_eq!(lock.repository, spec.repositories[0]);
    assert_eq!(lock.install_tl_sha512, spec.install_tl.unix.sha512);
    assert_eq!(lock.platform, "fake-platform");
    assert_eq!(lock.terse_version, env!("CARGO_PKG_VERSION"));
    let names: Vec<&str> = lock.packages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["biber", "booktabs", "xetex"]);
    assert!(prefix.join(".terse-owned.json").exists(), "ownership marker");
    assert!(prefix.join("bin").join("fake-platform").join("xelatex").exists());
    assert!(staging_siblings(prefix.parent().unwrap()).is_empty());
    assert!(!progress.is_empty(), "progress is reported");
}

#[test]
fn test_toolchain_install_is_transactional() {
    let root = tempdir("install-transactional");
    let host = provisioning_host(&root);
    let prefix = root.join("xdg").join("terse").join("toolchain").join("texlive-2025");
    install_fake_prefix(&prefix, "2025");
    fs::write(prefix.join("marker-file.txt"), b"previous generation").unwrap();
    let before = hash_tree(&prefix);

    let good = fake_installer_archive();
    let spec = spec_for(&good);
    let mut downloader = FakeArchiveDownloader::new();
    downloader.serve(&spec.install_tl.unix.url, good);
    let mut runner = SideEffectRunner {
        inner: FakeProcessRunner::new(vec![ProcessOutcome::success(), ProcessOutcome::nonzero(1)]),
    };
    let opts = InstallOptions { prefix: prefix.clone(), offline_from: None, update: true };
    let err = run_install(&opts, &spec, &host, &mut runner, Some(&mut downloader), &mut |_| {})
        .expect_err("tlmgr failure fails the install");
    assert!(matches!(err, InstallError::Tlmgr { .. }), "{err:?}");
    assert_eq!(err.code(), "E-TOOL-023");
    assert_eq!(err.exit_code(), 3);
    assert_eq!(hash_tree(&prefix), before, "the previous prefix is byte-identical");
    assert!(staging_siblings(prefix.parent().unwrap()).is_empty(), "no staging sibling remains");
}

#[test]
fn test_toolchain_offline_verifies_checksums() {
    let root = tempdir("offline-tamper");
    let host = provisioning_host(&root);
    let source = root.join("source-prefix");
    install_fake_prefix(&source, "2025");
    let archive_path = root.join("prefix.tar.gz");
    archive::create_archive(&source, &archive_path).expect("archive is produced");

    // Tamper with the payload after the manifest inside was written:
    // re-pack the same tree with one byte flipped and the manifest untouched.
    let unpacked = root.join("unpacked");
    archive::extract_archive(&archive_path, &unpacked).expect("our own archive extracts");
    let victim = unpacked.join("bin").join("fake-platform").join("xelatex");
    let mut bytes = fs::read(&victim).unwrap();
    bytes[0] ^= 0xff;
    fs::write(&victim, bytes).unwrap();
    let tampered = root.join("tampered.tar.gz");
    {
        let file = fs::File::create(&tampered).unwrap();
        let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut tar = tar::Builder::new(enc);
        tar.follow_symlinks(false);
        tar.append_dir_all(".", &unpacked).unwrap();
        tar.into_inner().unwrap().finish().unwrap().flush().unwrap();
    }

    let spec = spec_for(&fake_installer_archive());
    let prefix = root.join("xdg").join("terse").join("toolchain").join("texlive-2025");
    let opts = InstallOptions { prefix: prefix.clone(), offline_from: Some(tampered), update: false };
    let mut runner = FakeProcessRunner::new(vec![]);
    let err = run_install(&opts, &spec, &host, &mut runner, None, &mut |_| {}).expect_err("tampered archive is rejected");
    assert!(matches!(err, InstallError::ArchiveVerification(_)), "{err:?}");
    assert_eq!(err.code(), "E-TOOL-027");
    assert_eq!(err.exit_code(), 3);
    assert!(runner.invocations.is_empty());
    assert!(!prefix.exists());
    assert!(staging_siblings(prefix.parent().unwrap()).is_empty());

    // The untampered archive installs atomically with no downloader at all.
    let opts = InstallOptions { prefix: prefix.clone(), offline_from: Some(archive_path), update: false };
    let summary = run_install(&opts, &spec, &host, &mut runner, None, &mut |_| {}).expect("offline install succeeds");
    assert_eq!(summary.prefix, prefix);
    assert!(prefix.join(LOCK_FILE_NAME).exists());
    assert!(prefix.join("bin").join("fake-platform").join("xelatex").exists());
    assert!(runner.invocations.is_empty(), "offline install runs no process");
    assert!(staging_siblings(prefix.parent().unwrap()).is_empty());

    // A lock year that differs from the profile is refused before anything is installed.
    let other = root.join("other-prefix");
    install_fake_prefix(&other, "2026");
    let other_archive = root.join("other.tar.gz");
    archive::create_archive(&other, &other_archive).unwrap();
    let target = root.join("target-2026");
    let opts = InstallOptions { prefix: target.clone(), offline_from: Some(other_archive), update: false };
    let err = run_install(&opts, &spec, &host, &mut runner, None, &mut |_| {}).unwrap_err();
    assert!(matches!(err, InstallError::ArchiveVerification(_)), "{err:?}");
    assert!(!target.exists());
}

#[test]
fn test_toolchain_is_only_new_network_path() {
    let root = tempdir("network-boundary");
    let path_dir = executable_dir(&root.join("path"), &["xelatex", "biber", "kpsewhich", "perl", "tar", "xz", "curl"]);
    let mut host = HostEnv::minimal(HostOs::Linux, path_dir.as_os_str().to_os_string());
    host.xdg_data_home = Some(root.join("xdg"));
    host.home = Some(root.join("home"));
    host.tmpdir = Some(root.join("tmp").to_string_lossy().into_owned());
    fs::create_dir_all(root.join("tmp")).unwrap();
    install_fake_prefix(&managed_prefix_for(&host), "2025");

    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("terse.toml"), manifest(None)).unwrap();
    fs::write(project.join("paper.trs"), ENTRY).unwrap();

    let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let factory_calls = Rc::new(Cell::new(0u32));
    let commands: Vec<Vec<&str>> = vec![
        vec!["terse", "check"],
        vec!["terse", "build", "--tex-only", "--toolchain", "managed"],
        vec!["terse", "build", "--require-pdf", "--toolchain", "managed"],
        vec!["terse", "fmt", "--check"],
        vec!["terse", "export", "--target", "arxiv", "--toolchain", "managed"],
        vec!["terse", "doctor", "--toolchain", "managed"],
        vec!["terse", "toolchain", "status"],
    ];
    for argv in &commands {
        let calls = factory_calls.clone();
        let log = requests.clone();
        let mut factory = move || -> Box<dyn ArchiveDownloader> {
            calls.set(calls.get() + 1);
            Box::new(FakeArchiveDownloader::denied_with_log(log.clone()))
        };
        let code = terse_cli::run_with_host_and_downloader(argv.clone(), &project, &host, &mut factory);
        assert!(code == 0 || code == 1 || code == 3, "{argv:?} exit {code}");
    }
    assert_eq!(factory_calls.get(), 0, "no command other than toolchain install/update asks for a downloader");
    assert!(requests.lock().unwrap().is_empty(), "zero network requests");

    // The install path is the one that asks, and a denied downloader
    // records exactly the attempt it refused.
    let calls = factory_calls.clone();
    let log = requests.clone();
    let mut factory = move || -> Box<dyn ArchiveDownloader> {
        calls.set(calls.get() + 1);
        Box::new(FakeArchiveDownloader::denied_with_log(log.clone()))
    };
    let code = terse_cli::run_with_host_and_downloader(
        ["terse", "toolchain", "install", "--prefix", root.join("fresh").to_str().unwrap()],
        &project,
        &host,
        &mut factory,
    );
    assert_eq!(code, 3);
    assert_eq!(factory_calls.get(), 1);
    assert_eq!(requests.lock().unwrap().len(), 1);

    // Structurally: the real downloader is constructed in exactly one
    // place, and no compilation/check/watch module knows the type.
    let lib = include_str!("../src/lib.rs");
    assert_eq!(lib.matches("RealArchiveDownloader::new").count(), 1);
    for src in [
        include_str!("../src/build.rs"),
        include_str!("../src/watch/mod.rs"),
        include_str!("../src/format.rs"),
        include_str!("../src/export/mod.rs"),
        include_str!("../src/export/validate.rs"),
        include_str!("../src/doctor/checks.rs"),
    ] {
        assert!(!src.contains("ArchiveDownloader"), "compilation paths never see the downloader");
    }
}

#[test]
fn test_toolchain_status_reports_year_match() {
    let root = tempdir("status");
    let path_dir = executable_dir(&root.join("path"), &["xelatex"]);
    let host = linux_host(&path_dir, &root.join("xdg"));
    let prefix = managed_prefix_for(&host);
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();

    // No prefix at all: still exit 0, says so.
    let code = terse_cli::run_with_host(["terse", "toolchain", "status"], &project, &host);
    assert_eq!(code, 0);

    install_fake_prefix(&prefix, "2026");
    let out = run_capturing(["terse", "toolchain", "status", "--json", "--prefix", prefix.to_str().unwrap()], &project, &host);
    assert_eq!(out.code, 0);
    let value: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap();
    assert_eq!(value["installed"], true);
    assert_eq!(value["lock_year"], "2026");
    assert_eq!(value["profile_year"], "2025");
    assert_eq!(value["matches_profile"], false);
    assert_eq!(value["prefix"], prefix.to_string_lossy().as_ref());

    // Auto resolution skips the mismatched prefix; a build still works from PATH.
    fs::write(project.join("terse.toml"), manifest(None)).unwrap();
    fs::write(project.join("paper.trs"), ENTRY).unwrap();
    let mut runner = FakeProcessRunner::new(vec![]);
    let code = terse_cli::build::run_build_with_host(&project, None, None, true, false, false, None, &host, &mut runner);
    assert_eq!(code, 0);

    // `update` refuses the mismatched year without touching it.
    let before = hash_tree(&prefix);
    let code = terse_cli::run_with_host(["terse", "toolchain", "update"], &project, &host);
    assert_eq!(code, 2);
    assert_eq!(hash_tree(&prefix), before);
}

#[test]
fn test_toolchain_uninstall_removes_only_prefix() {
    let root = tempdir("uninstall");
    let host = provisioning_host(&root);
    let prefix = root.join("toolchain").join("texlive-2025");
    install_fake_prefix(&prefix, "2025");
    let sibling = root.join("toolchain").join("keep.txt");
    fs::write(&sibling, b"sentinel").unwrap();
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();

    let argv = ["terse", "toolchain", "uninstall", "--prefix", prefix.to_str().unwrap()];
    assert_eq!(terse_cli::run_with_host(argv, &project, &host), 0);
    assert!(!prefix.exists());
    assert_eq!(fs::read(&sibling).unwrap(), b"sentinel");
    assert_eq!(terse_cli::run_with_host(argv, &project, &host), 0, "nothing to remove is not an error");
}

#[test]
fn test_toolchain_uninstall_refuses_unowned_prefix() {
    let root = tempdir("uninstall-unowned");
    let host = provisioning_host(&root);
    let dir = root.join("not-ours");
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::write(dir.join("bin").join("xelatex"), b"precious").unwrap();
    let before = hash_tree(&dir);
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    let code = terse_cli::run_with_host(["terse", "toolchain", "uninstall", "--prefix", dir.to_str().unwrap()], &project, &host);
    assert_eq!(code, 2);
    assert_eq!(hash_tree(&dir), before);
}

#[test]
fn test_managed_prefix_absent_from_artifacts() {
    let root = tempdir("prefix-absent");
    let path_dir = executable_dir(&root.join("path"), &["xelatex", "biber", "kpsewhich"]);
    let mut host = linux_host(&path_dir, &root.join("xdg-DISTINCTIVE-9f3a"));
    host.tmpdir = Some(root.join("tmp").to_string_lossy().into_owned());
    fs::create_dir_all(root.join("tmp")).unwrap();
    let prefix = managed_prefix_for(&host);
    install_fake_prefix(&prefix, "2025");
    let needle = prefix.to_string_lossy().into_owned();

    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("terse.toml"), manifest(Some("toolchain = \"managed\""))).unwrap();
    fs::write(project.join("paper.trs"), ENTRY).unwrap();

    assert_eq!(terse_cli::run_with_host(["terse", "build", "--tex-only"], &project, &host), 0);
    assert_eq!(terse_cli::run_with_host(["terse", "export", "--target", "arxiv"], &project, &host), 0);

    let mut checked = 0;
    fn walk(dir: &Path, needle: &str, checked: &mut usize) {
        for entry in fs::read_dir(dir).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                walk(&p, needle, checked);
            } else if p.extension().map(|e| e != "zip" && e != "pdf" && e != "png").unwrap_or(true) {
                let text = String::from_utf8_lossy(&fs::read(&p).unwrap()).into_owned();
                assert!(!text.contains(needle), "{} contains the managed prefix path", p.display());
                *checked += 1;
            }
        }
    }
    walk(&project.join("build"), &needle, &mut checked);
    assert!(checked >= 4, "checked {checked} published text artifacts");

    let zip_path = project.join("build").join("export").join("academic").join("paper-arxiv.zip");
    let mut archive = zip::ZipArchive::new(fs::File::open(&zip_path).unwrap()).unwrap();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut bytes).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains(&needle), "zip member {} leaks the prefix", file.name());
    }
}

struct Captured {
    code: i32,
    stdout: String,
}

/// Runs the built binary so stdout can be captured separately from stderr.
fn run_capturing<const N: usize>(argv: [&str; N], cwd: &Path, host: &HostEnv) -> Captured {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_terse"));
    cmd.args(&argv[1..]).current_dir(cwd).env_clear();
    cmd.env("PATH", &host.path);
    if let Some(x) = &host.xdg_data_home {
        cmd.env("XDG_DATA_HOME", x);
    }
    if let Some(h) = &host.home {
        cmd.env("HOME", h);
    }
    let out = cmd.output().expect("binary runs");
    Captured { code: out.status.code().unwrap_or(-1), stdout: String::from_utf8_lossy(&out.stdout).into_owned() }
}
