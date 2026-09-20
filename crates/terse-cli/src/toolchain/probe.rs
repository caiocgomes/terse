//! Tool probes that RUN a tool through the bounded runner: the TeX Live
//! year from `xelatex --version`, and the distribution roots from
//! `kpsewhich -var-value`. Every probe carries a timeout and the prepared
//! environment; nothing here spawns a raw `Command`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::engine::{ProcessInvocation, ProcessRunner};

pub const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

const ROOT_VARIABLES: &[&str] = &[
    "TEXMFROOT",
    "TEXMFDIST",
    "TEXMFSYSVAR",
    "TEXMFSYSCONFIG",
    "TEXMFVAR",
    "TEXMFCONFIG",
    "TEXMFLOCAL",
    "SELFAUTOPARENT",
    "SELFAUTOGRANDPARENT",
    "SELFAUTOLOC",
];

/// Runs `<xelatex> --version` and extracts the `TeX Live <year>` banner
/// year. `None` on any failure to start, time out, or parse; an ambiguous
/// banner is never a match.
pub fn detect_toolchain_year(
    runner: &mut dyn ProcessRunner,
    xelatex: &Path,
    working_dir: &Path,
    env: &[(String, String)],
) -> Option<String> {
    let invocation = ProcessInvocation {
        program: xelatex.to_path_buf(),
        args: vec!["--version".to_string()],
        working_dir: working_dir.to_path_buf(),
        env: env.to_vec(),
    };
    let outcome = runner.run(&invocation, PROBE_TIMEOUT);
    if !outcome.started || outcome.timed_out || outcome.status_code != Some(0) {
        return None;
    }
    let text = String::from_utf8_lossy(&outcome.stdout);
    let after = text.split("TeX Live ").nth(1)?;
    let year: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if year.len() == 4 {
        Some(year)
    } else {
        None
    }
}

/// Verifies that a toolchain actually provides everything an export
/// profile guarantees: every profile package as a resolvable `.sty`, every
/// profile font through its representative font file, and every babel
/// language through its `.ldf`, each looked up with one bounded
/// `kpsewhich` invocation. Returns the list of unverified items (never
/// empty on `Err`), phrased so a report can record them as untested
/// assumptions. A toolchain without `kpsewhich` verifies nothing.
pub fn verify_profile(
    runner: &mut dyn ProcessRunner,
    kpsewhich: Option<&Path>,
    working_dir: &Path,
    env: &[(String, String)],
    profile: &terse_core::artifact::profile::ExportProfile,
) -> Result<(), Vec<String>> {
    use crate::doctor::checks::{BABEL_FILES, FONT_FILES};

    let Some(kpsewhich) = kpsewhich else {
        return Err(vec!["kpsewhich: not available, no profile package or font was verified".to_string()]);
    };
    let mut lookups: Vec<(String, String)> = Vec::new();
    for pkg in &profile.packages {
        lookups.push((format!("package {pkg}"), format!("{pkg}.sty")));
    }
    for font in &profile.fonts {
        match FONT_FILES.iter().find(|(tok, _)| tok == font) {
            Some((_, file)) => lookups.push((format!("font {font}"), (*file).to_string())),
            None => lookups.push((format!("font {font}"), String::new())),
        }
    }
    for lang in &profile.babel_languages {
        match BABEL_FILES.iter().find(|(tok, _)| tok == lang) {
            Some((_, file)) => lookups.push((format!("babel {lang}"), (*file).to_string())),
            None => lookups.push((format!("babel {lang}"), String::new())),
        }
    }
    let mut unverified = Vec::new();
    for (label, file) in lookups {
        if file.is_empty() {
            unverified.push(format!("{label}: no representative file to look up"));
            continue;
        }
        let invocation = ProcessInvocation {
            program: kpsewhich.to_path_buf(),
            args: vec![file.clone()],
            working_dir: working_dir.to_path_buf(),
            env: env.to_vec(),
        };
        let outcome = runner.run(&invocation, PROBE_TIMEOUT);
        if !(outcome.started && !outcome.timed_out && outcome.status_code == Some(0)) {
            unverified.push(format!("{label} ({file}) not resolvable by kpsewhich"));
        }
    }
    if unverified.is_empty() {
        Ok(())
    } else {
        Err(unverified)
    }
}

/// The real TeX distribution roots as `kpsewhich` reports them, one
/// bounded invocation per variable, computed once per validation.
pub fn distribution_roots(
    runner: &mut dyn ProcessRunner,
    kpsewhich: &Path,
    working_dir: &Path,
    env: &[(String, String)],
) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for var in ROOT_VARIABLES {
        let invocation = ProcessInvocation {
            program: kpsewhich.to_path_buf(),
            args: vec!["-var-value".to_string(), (*var).to_string()],
            working_dir: working_dir.to_path_buf(),
            env: env.to_vec(),
        };
        let outcome = runner.run(&invocation, PROBE_TIMEOUT);
        if !outcome.started || outcome.timed_out || outcome.status_code != Some(0) {
            continue;
        }
        let text = String::from_utf8_lossy(&outcome.stdout);
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        roots.push(std::fs::canonicalize(trimmed).unwrap_or_else(|_| PathBuf::from(trimmed)));
    }
    roots
}
