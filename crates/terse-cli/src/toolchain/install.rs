//! The managed TeX Live installation flow: prerequisites, pinned download
//! with checksum verification, `install-tl` and `tlmgr` through the
//! bounded runner, lock and ownership marker, atomic publication.
//!
//! Layout: the staging directory IS the future prefix root (`TEXDIR`), so
//! after `install-tl` it holds `bin/<platform>/`, `texmf-dist/`, `tlpkg/`
//! and so on, plus Terse's lock and ownership marker, and a single rename
//! installs it. Downloads and the unpacked installer live in a separate
//! work directory that is always removed.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use terse_core::toolchain::spec::ToolchainSpec;

use super::archive;
use super::download::{ArchiveDownloader, DownloadError};
use super::lock::{LockedPackage, ToolchainLock, LOCK_FILE_NAME, LOCK_VERSION};
use super::paths::TexmfDirs;
use super::tlmgr;
use super::{find_tool_on_path, HostEnv, HostOs};
use crate::engine::{ProcessInvocation, ProcessOutcome, ProcessRunner};
use crate::publication::{self, StagedBuild, OWNERSHIP_MARKER};

pub const INSTALL_TL_TIMEOUT: Duration = Duration::from_secs(600);
pub const TLMGR_TIMEOUT: Duration = Duration::from_secs(1800);
pub const TLMGR_INFO_TIMEOUT: Duration = Duration::from_secs(120);
/// The installer archive is about 6 MB; anything approaching this bound
/// is not the pinned installer.
pub const MAX_INSTALLER_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub prefix: PathBuf,
    pub offline_from: Option<PathBuf>,
    pub update: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallSummary {
    pub prefix: PathBuf,
    pub packages: Vec<LockedPackage>,
    pub platform: String,
    /// Human-readable revision deltas against the previous lock (update).
    pub changes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallError {
    UnsupportedPlatform,
    Prerequisites(Vec<&'static str>),
    Download(String),
    ChecksumMismatch { expected: String, actual: String },
    Extract(String),
    InstallTl { detail: String },
    Tlmgr { detail: String },
    Publish(String),
    Io(String),
    ArchiveVerification(String),
    UnownedPrefix(PathBuf),
    Usage(String),
}

/// Every diagnostic code the toolchain install/update/status/uninstall
/// path can emit, in one place so the code-family test can prove none is
/// shared with the engine, resolution, or doctor tables.
pub const ALL_CODES: &[&str] = &[
    "E-TOOL-020",
    "E-TOOL-021",
    "E-TOOL-022",
    "E-TOOL-023",
    "E-TOOL-024",
    "E-TOOL-025",
    "E-TOOL-026",
    "E-TOOL-027",
    "E-TOOL-028",
    "E-TOOL-029",
];

impl InstallError {
    pub fn code(&self) -> &'static str {
        match self {
            InstallError::Prerequisites(_) => "E-TOOL-020",
            InstallError::ChecksumMismatch { .. } => "E-TOOL-021",
            InstallError::InstallTl { .. } => "E-TOOL-022",
            InstallError::Tlmgr { .. } => "E-TOOL-023",
            InstallError::Publish(_) => "E-TOOL-024",
            InstallError::Download(_) | InstallError::Extract(_) | InstallError::Io(_) => "E-TOOL-025",
            InstallError::UnsupportedPlatform => "E-TOOL-026",
            InstallError::ArchiveVerification(_) => "E-TOOL-027",
            InstallError::UnownedPrefix(_) => "E-TOOL-028",
            InstallError::Usage(_) => "E-TOOL-029",
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            InstallError::UnownedPrefix(_) | InstallError::Usage(_) | InstallError::UnsupportedPlatform => 2,
            _ => 3,
        }
    }
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::UnsupportedPlatform => write!(
                f,
                "managed toolchain installation is not supported on Windows in this version; install TeX Live or MiKTeX and use --toolchain system"
            ),
            InstallError::Prerequisites(missing) => write!(
                f,
                "missing prerequisites for install-tl/tlmgr: {}; nothing was downloaded",
                missing.join(", ")
            ),
            InstallError::Download(msg) => write!(f, "downloading the installer failed: {msg}"),
            InstallError::ChecksumMismatch { expected, actual } => write!(
                f,
                "the downloaded installer does not match the pinned SHA-512 (expected {expected}, got {actual}); nothing from it was executed"
            ),
            InstallError::Extract(msg) => write!(f, "unpacking failed: {msg}"),
            InstallError::InstallTl { detail } => write!(f, "install-tl failed: {detail}"),
            InstallError::Tlmgr { detail } => write!(f, "tlmgr failed: {detail}"),
            InstallError::Publish(msg) => write!(f, "installing the staged prefix failed: {msg}"),
            InstallError::Io(msg) => write!(f, "{msg}"),
            InstallError::ArchiveVerification(msg) => write!(f, "the offline archive was rejected: {msg}"),
            InstallError::UnownedPrefix(p) => write!(
                f,
                "'{}' is populated but carries no Terse ownership marker; refusing to replace it",
                p.display()
            ),
            InstallError::Usage(msg) => write!(f, "{msg}"),
        }
    }
}

/// The `install-tl` profile for a portable, infrastructure-only
/// installation rooted at `texdir` with Terse-owned user trees.
pub fn install_profile_text(texdir: &Path, texmf: &TexmfDirs) -> String {
    let d = texdir.display();
    format!(
        "selected_scheme scheme-infraonly\n\
         TEXDIR {d}\n\
         TEXMFSYSCONFIG {d}/texmf-config\n\
         TEXMFSYSVAR {d}/texmf-var\n\
         TEXMFLOCAL {d}/texmf-local\n\
         TEXMFHOME {}\n\
         TEXMFVAR {}\n\
         TEXMFCONFIG {}\n\
         instopt_portable 1\n\
         instopt_adjustpath 0\n\
         instopt_letter 0\n\
         tlpdbopt_install_docfiles 0\n\
         tlpdbopt_install_srcfiles 0\n\
         tlpdbopt_autobackup 0\n\
         tlpdbopt_desktop_integration 0\n\
         tlpdbopt_file_assocs 0\n\
         tlpdbopt_post_code 1\n",
        texmf.home.display(),
        texmf.var.display(),
        texmf.config.display()
    )
}

/// The environment `install-tl` and `tlmgr` run with: the host `PATH`
/// (with the staged `bin/` first once it exists, so `tlmgr` finds
/// `kpsewhich`), the allowlisted host variables, and `LANG`. No `TEXMF*`
/// variable: both tools are configured through the profile and the
/// tree's own `texmf.cnf`, and an inherited override would leak the
/// user's tree into the managed one.
pub fn provisioning_env(host: &HostEnv, bin_dir: Option<&Path>) -> Vec<(String, String)> {
    let mut env = Vec::new();
    let path_value = match bin_dir {
        Some(bin) => {
            let mut dirs = vec![bin.to_path_buf()];
            dirs.extend(host.path_dirs());
            std::env::join_paths(dirs).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| host.path.to_string_lossy().into_owned())
        }
        None => host.path.to_string_lossy().into_owned(),
    };
    env.push(("PATH".to_string(), path_value));
    if let Some(h) = &host.home {
        env.push(("HOME".to_string(), h.to_string_lossy().into_owned()));
    }
    if let Some(u) = &host.userprofile {
        env.push(("USERPROFILE".to_string(), u.to_string_lossy().into_owned()));
    }
    if let Some(t) = &host.tmpdir {
        env.push(("TMPDIR".to_string(), t.clone()));
    }
    if let Some(t) = &host.temp {
        env.push(("TEMP".to_string(), t.clone()));
    }
    if let Some(t) = &host.tmp {
        env.push(("TMP".to_string(), t.clone()));
    }
    if let Some(s) = &host.systemroot {
        env.push(("SYSTEMROOT".to_string(), s.clone()));
    }
    env.push(("LANG".to_string(), "C.UTF-8".to_string()));
    env
}

pub const OWNERSHIP_MARKER_CONTENT: &str = "{\"owned-files\": [\"terse-toolchain.lock.json\"], \"kind\": \"terse-toolchain\"}\n";

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_sibling(parent: &Path, label: &str) -> PathBuf {
    parent.join(format!(".terse-{label}-{}-{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)))
}

/// Whether a platform directory name from a lock can run on this host.
pub fn platform_compatible(os: HostOs, platform: &str) -> bool {
    match os {
        HostOs::MacOs => platform.contains("darwin"),
        HostOs::Linux => platform.contains("linux"),
        HostOs::Windows => platform.contains("windows"),
        HostOs::Other => true,
    }
}

pub fn run_install(
    opts: &InstallOptions,
    spec: &ToolchainSpec,
    host: &HostEnv,
    runner: &mut dyn ProcessRunner,
    downloader: Option<&mut dyn ArchiveDownloader>,
    progress: &mut dyn FnMut(&str),
) -> Result<InstallSummary, InstallError> {
    if host.os.is_windows() {
        return Err(InstallError::UnsupportedPlatform);
    }
    let prefix = &opts.prefix;
    // XeTeX passes its output driver's path (derived from the engine's own
    // location) through `sh`; a space anywhere in the prefix breaks every
    // PDF with "sh: .../Application: No such file or directory". Refuse
    // such a prefix before downloading anything rather than installing a
    // toolchain that cannot produce a single page.
    if prefix.to_string_lossy().chars().any(char::is_whitespace) {
        return Err(InstallError::Usage(format!(
            "the toolchain prefix '{}' contains whitespace, which XeTeX's output driver cannot handle; choose a prefix without spaces (for example --prefix ~/Library/terse/toolchain/texlive-{})",
            prefix.display(),
            spec.texlive_year
        )));
    }
    let parent = prefix
        .parent()
        .ok_or_else(|| InstallError::Usage(format!("'{}' has no parent directory", prefix.display())))?;
    fs::create_dir_all(parent).map_err(|e| InstallError::Io(format!("cannot create '{}': {e}", parent.display())))?;
    if let Err(publication::PublishError::UnownedDestination) = publication::check_destination_ownership(prefix) {
        return Err(InstallError::UnownedPrefix(prefix.clone()));
    }
    let previous = ToolchainLock::read_from_prefix(prefix).and_then(Result::ok);

    let staging = unique_sibling(parent, "staging");
    let work = unique_sibling(parent, "work");
    let result = (|| -> Result<InstallSummary, InstallError> {
        fs::create_dir_all(&staging).map_err(|e| InstallError::Io(e.to_string()))?;
        fs::create_dir_all(&work).map_err(|e| InstallError::Io(e.to_string()))?;
        let (lock, platform) = match &opts.offline_from {
            Some(archive_path) => stage_offline(archive_path, &staging, spec, host, progress)?,
            None => stage_online(&staging, &work, spec, host, runner, downloader, progress)?,
        };
        fs::write(staging.join(LOCK_FILE_NAME), lock.encode()).map_err(|e| InstallError::Io(e.to_string()))?;
        fs::write(staging.join(OWNERSHIP_MARKER), OWNERSHIP_MARKER_CONTENT).map_err(|e| InstallError::Io(e.to_string()))?;
        progress("installing the staged prefix");
        publication::publish(StagedBuild { staging_dir: staging.clone() }, prefix).map_err(|e| InstallError::Publish(format!("{e:?}")))?;
        let changes = revision_changes(previous.as_ref(), &lock);
        Ok(InstallSummary { prefix: prefix.clone(), packages: lock.packages, platform, changes })
    })();
    let _ = fs::remove_dir_all(&work);
    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn stage_online(
    staging: &Path,
    work: &Path,
    spec: &ToolchainSpec,
    host: &HostEnv,
    runner: &mut dyn ProcessRunner,
    downloader: Option<&mut dyn ArchiveDownloader>,
    progress: &mut dyn FnMut(&str),
) -> Result<(ToolchainLock, String), InstallError> {
    let prereqs = crate::doctor::checks::prerequisites(host);
    if !prereqs.missing.is_empty() {
        return Err(InstallError::Prerequisites(prereqs.missing));
    }
    let downloader = downloader.ok_or_else(|| InstallError::Usage("an online install needs a downloader".to_string()))?;
    let perl = find_tool_on_path("perl", host).ok_or(InstallError::Prerequisites(vec!["perl"]))?;

    // 1. Installer archive, verified against the pin before anything in it runs.
    let archive_path = work.join("install-tl-unx.tar.gz");
    progress(&format!("downloading {}", spec.install_tl.unix.url));
    let download = downloader
        .fetch_to_file(&spec.install_tl.unix.url, &archive_path, MAX_INSTALLER_BYTES)
        .map_err(|e: DownloadError| InstallError::Download(e.to_string()))?;
    let expected = spec.install_tl.unix.sha512.to_ascii_lowercase();
    if download.sha512 != expected {
        return Err(InstallError::ChecksumMismatch { expected, actual: download.sha512 });
    }
    progress(&format!("installer verified ({} bytes, SHA-512 matches the pin)", download.bytes));

    let installer_dir = work.join("installer");
    archive::extract_archive(&archive_path, &installer_dir).map_err(InstallError::Extract)?;
    let install_tl = find_install_tl(&installer_dir)?;

    // 2. install-tl with the portable infrastructure-only profile.
    let profile_path = work.join("install.profile");
    fs::write(&profile_path, install_profile_text(staging, &super::paths::texmf_dirs(host)))
        .map_err(|e| InstallError::Io(e.to_string()))?;
    let base_env = provisioning_env(host, None);
    let mut last_error = None;
    let mut repository = None;
    for repo in &spec.repositories {
        progress(&format!("running install-tl (scheme-infraonly) from {repo}"));
        let invocation = ProcessInvocation {
            program: perl.clone(),
            args: vec![
                install_tl.to_string_lossy().into_owned(),
                "-no-gui".to_string(),
                "-profile".to_string(),
                profile_path.to_string_lossy().into_owned(),
                "-repository".to_string(),
                repo.clone(),
            ],
            working_dir: staging.to_path_buf(),
            env: base_env.clone(),
        };
        let outcome = runner.run_streaming(&invocation, INSTALL_TL_TIMEOUT, progress);
        match describe_failure("install-tl", &outcome, INSTALL_TL_TIMEOUT) {
            None => {
                repository = Some(repo.clone());
                break;
            }
            Some(detail) => {
                progress(&format!("install-tl from {repo} failed: {detail}"));
                last_error = Some(detail);
            }
        }
    }
    let repository = match repository {
        Some(r) => r,
        None => return Err(InstallError::InstallTl { detail: last_error.unwrap_or_else(|| "no repository configured".to_string()) }),
    };

    // 3. The pinned closure through the freshly installed tlmgr.
    let bin_dir = locate_bin_dir(staging).ok_or_else(|| InstallError::InstallTl {
        detail: "install-tl finished but produced no bin/<platform>/ directory".to_string(),
    })?;
    let platform = bin_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tlmgr = bin_dir.join("tlmgr");
    let env = provisioning_env(host, Some(&bin_dir));
    let packages = spec.tlmgr_packages();
    progress(&format!("running tlmgr install for {} packages", packages.len()));
    let mut args = vec!["--repository".to_string(), repository.clone(), "install".to_string()];
    args.extend(packages.iter().cloned());
    let invocation = ProcessInvocation { program: tlmgr.clone(), args, working_dir: staging.to_path_buf(), env: env.clone() };
    let outcome = runner.run_streaming(&invocation, TLMGR_TIMEOUT, progress);
    if let Some(detail) = describe_failure("tlmgr install", &outcome, TLMGR_TIMEOUT) {
        return Err(InstallError::Tlmgr { detail });
    }

    // 4. Exact revisions for the lock.
    let invocation = ProcessInvocation {
        program: tlmgr,
        args: ["info", "--only-installed", "--data", "name,localrev"].iter().map(|s| s.to_string()).collect(),
        working_dir: staging.to_path_buf(),
        env,
    };
    let outcome = runner.run(&invocation, TLMGR_INFO_TIMEOUT);
    if let Some(detail) = describe_failure("tlmgr info", &outcome, TLMGR_INFO_TIMEOUT) {
        return Err(InstallError::Tlmgr { detail });
    }
    let installed = tlmgr::parse_installed(&String::from_utf8_lossy(&outcome.stdout));
    if installed.is_empty() {
        return Err(InstallError::Tlmgr { detail: "tlmgr info reported no installed packages".to_string() });
    }

    let lock = ToolchainLock {
        version: LOCK_VERSION,
        texlive_year: spec.texlive_year.clone(),
        repository,
        install_tl_sha512: expected,
        packages: installed,
        terse_version: env!("CARGO_PKG_VERSION").to_string(),
        platform: platform.clone(),
    };
    Ok((lock, platform))
}

fn stage_offline(
    archive_path: &Path,
    staging: &Path,
    spec: &ToolchainSpec,
    host: &HostEnv,
    progress: &mut dyn FnMut(&str),
) -> Result<(ToolchainLock, String), InstallError> {
    progress(&format!("extracting {}", archive_path.display()));
    archive::extract_archive(archive_path, staging).map_err(InstallError::ArchiveVerification)?;
    let verified = archive::verify_sums(staging).map_err(InstallError::ArchiveVerification)?;
    progress(&format!("verified {verified} files against the archive manifest"));
    let lock_text = archive::read_member(staging, LOCK_FILE_NAME)
        .ok_or_else(|| InstallError::ArchiveVerification("archive has no terse-toolchain.lock.json".to_string()))?;
    let lock = ToolchainLock::decode(&lock_text).map_err(|e| InstallError::ArchiveVerification(format!("invalid lock: {e:?}")))?;
    if lock.texlive_year != spec.texlive_year {
        return Err(InstallError::ArchiveVerification(format!(
            "archive is TeX Live {} but the profile requires {}",
            lock.texlive_year, spec.texlive_year
        )));
    }
    if !platform_compatible(host.os, &lock.platform) {
        return Err(InstallError::ArchiveVerification(format!(
            "archive platform '{}' cannot run on this host",
            lock.platform
        )));
    }
    let bin_dir = staging.join("bin").join(&lock.platform);
    if !bin_dir.is_dir() {
        return Err(InstallError::ArchiveVerification(format!("archive has no bin/{}/ directory", lock.platform)));
    }
    let platform = lock.platform.clone();
    Ok((lock, platform))
}

fn find_install_tl(installer_dir: &Path) -> Result<PathBuf, InstallError> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(installer_dir)
        .map_err(|e| InstallError::Extract(e.to_string()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.file_name().map(|n| n.to_string_lossy().starts_with("install-tl")).unwrap_or(false))
        .collect();
    dirs.sort();
    dirs.into_iter()
        .map(|d| d.join("install-tl"))
        .find(|p| p.is_file())
        .ok_or_else(|| InstallError::Extract("the installer archive contains no install-tl-*/install-tl".to_string()))
}

fn locate_bin_dir(staging: &Path) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(staging.join("bin"))
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs.into_iter().find(|d| d.join("tlmgr").exists() || d.join("xelatex").exists())
}

fn describe_failure(what: &str, outcome: &ProcessOutcome, timeout: Duration) -> Option<String> {
    if !outcome.started {
        return Some(format!("{what} could not be started"));
    }
    if outcome.timed_out {
        return Some(format!("{what} did not finish within {} s and its process tree was terminated", timeout.as_secs()));
    }
    if outcome.status_code != Some(0) {
        let tail = |bytes: &[u8]| -> String {
            let text = String::from_utf8_lossy(bytes);
            let lines: Vec<&str> = text.lines().collect();
            lines[lines.len().saturating_sub(20)..].join("\n")
        };
        return Some(format!(
            "{what} exited with status {}\n{}\n{}",
            outcome.status_code.map(|c| c.to_string()).unwrap_or_else(|| "unknown".to_string()),
            tail(&outcome.stdout),
            tail(&outcome.stderr)
        ));
    }
    None
}

fn revision_changes(previous: Option<&ToolchainLock>, current: &ToolchainLock) -> Vec<String> {
    let Some(prev) = previous else { return Vec::new() };
    let mut changes = Vec::new();
    let before: std::collections::BTreeMap<&str, &str> =
        prev.packages.iter().map(|p| (p.name.as_str(), p.revision.as_str())).collect();
    let after: std::collections::BTreeMap<&str, &str> =
        current.packages.iter().map(|p| (p.name.as_str(), p.revision.as_str())).collect();
    for (name, rev) in &after {
        match before.get(name) {
            None => changes.push(format!("added {name} r{rev}")),
            Some(old) if old != rev => changes.push(format!("{name} r{old} -> r{rev}")),
            Some(_) => {}
        }
    }
    for name in before.keys() {
        if !after.contains_key(name) {
            changes.push(format!("removed {name}"));
        }
    }
    changes
}
