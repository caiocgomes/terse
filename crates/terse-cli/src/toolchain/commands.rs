//! `terse toolchain <install|update|status|uninstall>`: the CLI surface
//! over [`super::install`]. `install` and `update` are the only callers of
//! the downloader factory; `status` and `uninstall` never ask for it.

use std::fs;
use std::path::{Path, PathBuf};

use terse_core::toolchain::spec::{toolchain_spec_for, ToolchainSpec};

use super::download::ArchiveDownloader;
use super::install::{run_install, InstallOptions, InstallSummary};
use super::lock::{ToolchainLock, LOCK_FILE_NAME};
use super::{archive, paths, resolve, HostEnv, ToolchainSelector};
use crate::args::ToolchainAction;
use crate::engine::ProcessRunner;
use crate::publication::OWNERSHIP_MARKER;

pub fn run_toolchain(
    cwd: &Path,
    action: &ToolchainAction,
    host: &HostEnv,
    runner: &mut dyn ProcessRunner,
    make_downloader: &mut dyn FnMut() -> Box<dyn ArchiveDownloader>,
) -> i32 {
    let profile_year = profile_year_for(cwd);
    match action {
        ToolchainAction::Install { year, prefix, offline, from, json } => {
            let year = year.clone().unwrap_or_else(|| profile_year.clone());
            let spec = match spec_for(&year) {
                Ok(s) => s,
                Err(code) => return code,
            };
            let Some(prefix) = prefix_for(prefix.as_deref(), host, &spec) else { return 2 };
            let opts = InstallOptions { prefix, offline_from: from.clone(), update: false };
            let _ = offline;
            install(&opts, &spec, host, runner, make_downloader, *json)
        }
        ToolchainAction::Update { prefix, json } => {
            let spec = match spec_for(&profile_year) {
                Ok(s) => s,
                Err(code) => return code,
            };
            let Some(prefix) = prefix_for(prefix.as_deref(), host, &spec) else { return 2 };
            let lock = match ToolchainLock::read_from_prefix(&prefix) {
                Some(Ok(lock)) => lock,
                _ => {
                    eprintln!(
                        "error[E-TOOL-015]: no managed toolchain is installed at '{}'; run `terse toolchain install`",
                        prefix.display()
                    );
                    return 2;
                }
            };
            if lock.texlive_year != spec.texlive_year {
                eprintln!(
                    "error[E-TOOL-016]: the managed toolchain at '{}' is TeX Live {} but the profile requires {}; run `terse toolchain install`",
                    prefix.display(),
                    lock.texlive_year,
                    spec.texlive_year
                );
                return 2;
            }
            let opts = InstallOptions { prefix, offline_from: None, update: true };
            install(&opts, &spec, host, runner, make_downloader, *json)
        }
        ToolchainAction::Status { prefix, archive: archive_out, json } => {
            let spec = match spec_for(&profile_year) {
                Ok(s) => s,
                Err(code) => return code,
            };
            status(prefix.as_deref(), archive_out.as_deref(), host, &spec, *json)
        }
        ToolchainAction::Uninstall { prefix } => {
            let spec = match spec_for(&profile_year) {
                Ok(s) => s,
                Err(code) => return code,
            };
            let Some(prefix) = prefix_for(prefix.as_deref(), host, &spec) else { return 2 };
            uninstall(&prefix)
        }
    }
}

/// The export profile's year from the nearest project manifest, or the
/// compiler default when no project is in scope.
fn profile_year_for(cwd: &Path) -> String {
    match crate::project::resolve_project(None, cwd) {
        Ok((ctx, _)) => crate::project::profile_year(&ctx.manifest),
        Err(_) => terse_core::artifact::profile::resolve_profile(crate::project::DEFAULT_PROFILE_NAME)
            .map(|p| p.texlive_year)
            .unwrap_or_else(|_| "2025".to_string()),
    }
}

fn spec_for(year: &str) -> Result<ToolchainSpec, i32> {
    toolchain_spec_for(year).map_err(|e| {
        eprintln!("error[E-TOOL-029]: no managed toolchain profile exists for TeX Live {}", e.0);
        2
    })
}

fn prefix_for(explicit: Option<&Path>, host: &HostEnv, spec: &ToolchainSpec) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return Some(p.to_path_buf());
    }
    match paths::managed_prefix(host, &spec.texlive_year) {
        Some(p) => Some(p),
        None => {
            eprintln!("error[E-TOOL-015]: no user data directory is available for a managed toolchain; pass --prefix");
            None
        }
    }
}

fn install(
    opts: &InstallOptions,
    spec: &ToolchainSpec,
    host: &HostEnv,
    runner: &mut dyn ProcessRunner,
    make_downloader: &mut dyn FnMut() -> Box<dyn ArchiveDownloader>,
    json: bool,
) -> i32 {
    let mut progress = |line: &str| eprintln!("toolchain: {line}");
    let result = if opts.offline_from.is_some() {
        run_install(opts, spec, host, runner, None, &mut progress)
    } else {
        let mut downloader = make_downloader();
        run_install(opts, spec, host, runner, Some(downloader.as_mut()), &mut progress)
    };
    match result {
        Ok(summary) => {
            report_summary(&summary, spec, json);
            0
        }
        Err(e) => {
            eprintln!("error[{}]: {e}", e.code());
            e.exit_code()
        }
    }
}

fn report_summary(summary: &InstallSummary, spec: &ToolchainSpec, json: bool) {
    for change in &summary.changes {
        eprintln!("toolchain: {change}");
    }
    if json {
        println!(
            "{{\"prefix\": {}, \"texlive_year\": {}, \"platform\": {}, \"packages\": {}, \"changes\": {}}}",
            json_str(&summary.prefix.to_string_lossy()),
            json_str(&spec.texlive_year),
            json_str(&summary.platform),
            summary.packages.len(),
            summary.changes.len()
        );
    } else {
        println!(
            "toolchain: installed TeX Live {} ({} packages, {}) at {}",
            spec.texlive_year,
            summary.packages.len(),
            summary.platform,
            summary.prefix.display()
        );
    }
}

fn status(explicit: Option<&Path>, archive_out: Option<&Path>, host: &HostEnv, spec: &ToolchainSpec, json: bool) -> i32 {
    let prefix = match explicit {
        Some(p) => p.to_path_buf(),
        None => match paths::managed_prefix(host, &spec.texlive_year) {
            Some(p) => p,
            None => {
                if json {
                    println!("{{\"installed\": false, \"prefix\": null, \"profile_year\": {}}}", json_str(&spec.texlive_year));
                } else {
                    println!("toolchain: no user data directory; no managed prefix can exist");
                }
                return 0;
            }
        },
    };
    let lock = ToolchainLock::read_from_prefix(&prefix);
    let resolved = resolve(&ToolchainSelector::Auto, &spec.texlive_year, host).ok();
    let (source, reason) = resolved
        .as_ref()
        .map(|tc| (tc.source_label().to_string(), tc.reason.clone()))
        .unwrap_or_else(|| ("unresolved".to_string(), String::new()));

    let installed = matches!(lock, Some(Ok(_)));
    let lock = match lock {
        Some(Ok(lock)) => Some(lock),
        Some(Err(e)) => {
            eprintln!("warning: the lock at '{}' is invalid: {e:?}", prefix.join(LOCK_FILE_NAME).display());
            None
        }
        None => None,
    };
    let lock_year = lock.as_ref().map(|l| l.texlive_year.clone());
    let matches_profile = lock_year.as_deref() == Some(spec.texlive_year.as_str());

    let mut archive_path = None;
    if let Some(out) = archive_out {
        if !installed {
            eprintln!(
                "error[E-TOOL-015]: no managed toolchain is installed at '{}'; nothing to archive",
                prefix.display()
            );
            return 2;
        }
        if let Err(e) = archive::create_archive(&prefix, out) {
            eprintln!("error[E-TOOL-025]: writing the archive failed: {e}");
            return 3;
        }
        eprintln!("toolchain: wrote {}", out.display());
        archive_path = Some(out.to_path_buf());
    }

    if json {
        println!(
            "{{\"installed\": {installed}, \"prefix\": {}, \"lock_year\": {}, \"profile_year\": {}, \"matches_profile\": {matches_profile}, \"platform\": {}, \"packages\": {}, \"source\": {}, \"reason\": {}, \"archive\": {}}}",
            json_str(&prefix.to_string_lossy()),
            lock_year.as_deref().map(json_str).unwrap_or_else(|| "null".to_string()),
            json_str(&spec.texlive_year),
            lock.as_ref().map(|l| json_str(&l.platform)).unwrap_or_else(|| "null".to_string()),
            lock.as_ref().map(|l| l.packages.len()).unwrap_or(0),
            json_str(&source),
            json_str(&reason),
            archive_path.as_ref().map(|p| json_str(&p.to_string_lossy())).unwrap_or_else(|| "null".to_string()),
        );
    } else {
        println!("toolchain: source {source} ({reason})");
        println!("toolchain: managed prefix {}", prefix.display());
        match &lock {
            Some(l) => println!(
                "toolchain: installed TeX Live {} ({} packages, {}); profile requires {}: {}",
                l.texlive_year,
                l.packages.len(),
                l.platform,
                spec.texlive_year,
                if matches_profile { "matches" } else { "MISMATCH, run `terse toolchain install`" }
            ),
            None => println!("toolchain: not installed (run `terse toolchain install`)"),
        }
    }
    0
}

fn uninstall(prefix: &Path) -> i32 {
    if !prefix.exists() {
        println!("toolchain: nothing to remove at {}", prefix.display());
        return 0;
    }
    let owned = fs::read_to_string(prefix.join(OWNERSHIP_MARKER))
        .map(|c| c.contains("\"owned-files\""))
        .unwrap_or(false);
    if !owned {
        eprintln!(
            "error[E-TOOL-028]: '{}' carries no Terse ownership marker; refusing to remove it",
            prefix.display()
        );
        return 2;
    }
    match fs::remove_dir_all(prefix) {
        Ok(()) => {
            println!("toolchain: removed {}", prefix.display());
            0
        }
        Err(e) => {
            eprintln!("error[E-TOOL-025]: removing '{}' failed: {e}", prefix.display());
            3
        }
    }
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}
