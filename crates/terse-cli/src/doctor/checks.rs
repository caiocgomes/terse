//! The ordered check list. Every tool execution goes through the injected
//! runner with [`PROBE_TIMEOUT`] (the micro-compile uses the engine's own
//! per-pass timeout); presence checks discover without running. Nothing
//! here writes outside the doctor's own temporary working directory.

use std::path::{Path, PathBuf};
use std::time::Duration;

use terse_core::artifact::profile::{self, ExportProfile};
use terse_core::toolchain::par::par_cache_dir;

use crate::engine::{self, EngineConfig, ProcessInvocation, ProcessOutcome, ProcessRunner};
use crate::project::{self, ProjectError};
use crate::toolchain::probe::PROBE_TIMEOUT;
use crate::toolchain::{self, HostEnv, HostOs, ResolvedToolchain, TexmfDirs, ToolchainSource};

use super::fix;
use super::report::{Check, CheckStatus, DoctorReport, ToolchainSummary};
use super::DoctorOptions;

/// Doctor check codes: one family, stable meanings.
pub const CODE_XELATEX_MISSING: &str = "E-TOOL-031";
pub const CODE_XELATEX_RUN: &str = "E-TOOL-032";
pub const CODE_XELATEX_YEAR: &str = "E-TOOL-033";
pub const CODE_BIBER_MISSING: &str = "E-TOOL-034";
pub const CODE_BIBER_RUN: &str = "E-TOOL-035";
pub const CODE_KPSEWHICH: &str = "E-TOOL-036";
pub const CODE_PACKAGES: &str = "E-TOOL-037";
pub const CODE_FONTS: &str = "E-TOOL-038";
pub const CODE_BABEL: &str = "E-TOOL-039";
pub const CODE_PREREQUISITES: &str = "E-TOOL-040";
pub const CODE_QUARANTINE: &str = "E-TOOL-041";
pub const CODE_DISK: &str = "E-TOOL-042";
pub const CODE_MICRO_COMPILE: &str = "E-TOOL-043";
pub const CODE_PROJECT_LOAD: &str = "E-TOOL-044";
pub const CODE_ENGINE_MISMATCH: &str = "E-CONFIG-008";

pub const ALL_CODES: &[&str] = &[
    CODE_XELATEX_MISSING,
    CODE_XELATEX_RUN,
    CODE_XELATEX_YEAR,
    CODE_BIBER_MISSING,
    CODE_BIBER_RUN,
    CODE_KPSEWHICH,
    CODE_PACKAGES,
    CODE_FONTS,
    CODE_BABEL,
    CODE_PREREQUISITES,
    CODE_QUARANTINE,
    CODE_DISK,
    CODE_MICRO_COMPILE,
    CODE_PROJECT_LOAD,
];

/// Representative font file per profile font package, looked up through
/// `kpsewhich` because the generated style loads these fonts by filename
/// through kpathsea, not through fontconfig.
/// Every font file a theme's `body.font` can make `\setmainfont` load, not
/// just the regular weight: `generate_style` names all four variants by
/// filename, so a distribution missing only the bold-italic would pass a
/// regular-weight-only probe and then fail at compile time.
pub const FONT_FILES: &[(&str, &str)] = &[
    ("tgheros", "texgyreheros-regular.otf"),
    ("tgheros", "texgyreheros-bold.otf"),
    ("tgheros", "texgyreheros-italic.otf"),
    ("tgheros", "texgyreheros-bolditalic.otf"),
    ("tgpagella", "texgyrepagella-regular.otf"),
    ("tgpagella", "texgyrepagella-bold.otf"),
    ("tgpagella", "texgyrepagella-italic.otf"),
    ("tgpagella", "texgyrepagella-bolditalic.otf"),
    ("lmodern", "lmroman10-regular.otf"),
    ("lmodern", "lmroman10-bold.otf"),
    ("lmodern", "lmroman10-italic.otf"),
    ("lmodern", "lmroman10-bolditalic.otf"),
    ("libertinus-otf", "LibertinusSerif-Regular.otf"),
    ("libertinus-otf", "LibertinusSerif-Bold.otf"),
    ("libertinus-otf", "LibertinusSerif-Italic.otf"),
    ("libertinus-otf", "LibertinusSerif-BoldItalic.otf"),
];

/// Babel language definition file per supported document language.
pub const BABEL_FILES: &[(&str, &str)] = &[("en", "english.ldf"), ("pt-BR", "portuges.ldf")];

const MICRO_TEX: &str = "\\documentclass{article}\n\\usepackage{fontspec}\n\\usepackage[backend=biber,sorting=none]{biblatex}\n\\addbibresource{paper.bib}\n\\begin{document}\nTerse doctor micro-compile \\cite{doctor2026}.\n\\printbibliography\n\\end{document}\n";
const MICRO_BIB: &str = "@article{doctor2026,\n  title = {Doctor},\n  author = {Terse, Doctor},\n  date = {2026}\n}\n";
const MICRO_COMPILE_TIMEOUT: Duration = Duration::from_secs(60);

pub struct Prerequisites {
    pub missing: Vec<&'static str>,
    pub found: Vec<String>,
}

/// Presence of what `install-tl`/`tlmgr` need to run and download:
/// `perl`, one of `curl`/`wget`, `tar`, and `xz`. Discovery only.
/// `toolchain install` runs this same check before downloading anything.
pub fn prerequisites(host: &HostEnv) -> Prerequisites {
    let mut missing = Vec::new();
    let mut found = Vec::new();
    for tool in ["perl", "tar", "xz"] {
        match toolchain::find_tool_on_path(tool, host) {
            Some(p) => found.push(p.display().to_string()),
            None => missing.push(tool),
        }
    }
    match toolchain::find_tool_on_path("curl", host).or_else(|| toolchain::find_tool_on_path("wget", host)) {
        Some(p) => found.push(p.display().to_string()),
        None => missing.push("curl or wget"),
    }
    Prerequisites { missing, found }
}

pub fn prerequisites_fix_command(host: &HostEnv) -> &'static str {
    match host.os {
        HostOs::MacOs => "brew install perl wget xz",
        HostOs::Linux | HostOs::Other => "apt-get install perl wget tar xz-utils (or your distribution's equivalent)",
        HostOs::Windows => "managed installation is not supported on Windows in this version",
    }
}

pub fn prerequisites_check(host: &HostEnv) -> Check {
    if host.os.is_windows() {
        return Check::info("prerequisites", "managed installation prerequisites are not applicable on Windows");
    }
    let p = prerequisites(host);
    if p.missing.is_empty() {
        let mut c = Check::ok("prerequisites", "perl, a downloader, tar, and xz are available for `terse toolchain install`");
        c.evidence = p.found;
        c
    } else {
        let mut c = Check::warn(
            "prerequisites",
            format!("missing for `terse toolchain install`: {}", p.missing.join(", ")),
            CODE_PREREQUISITES,
        )
        .with_cause("install-tl and tlmgr are Perl programs that download with curl or wget and unpack with tar/xz")
        .with_fix(prerequisites_fix_command(host), false);
        c.evidence = p.found;
        c
    }
}

fn probe(
    runner: &mut dyn ProcessRunner,
    program: &Path,
    args: &[&str],
    working_dir: &Path,
    env: &[(String, String)],
) -> ProcessOutcome {
    let invocation = ProcessInvocation {
        program: program.to_path_buf(),
        args: args.iter().map(|s| s.to_string()).collect(),
        working_dir: working_dir.to_path_buf(),
        env: env.to_vec(),
    };
    runner.run(&invocation, PROBE_TIMEOUT)
}

fn first_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes).lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string)
}

fn banner_year(stdout: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(stdout);
    let after = text.split("TeX Live ").nth(1)?;
    let year: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    (year.len() == 4).then_some(year)
}

fn describe_outcome(o: &ProcessOutcome) -> &'static str {
    if !o.started {
        "could not be started"
    } else if o.timed_out {
        "did not answer within the timeout and its process tree was terminated"
    } else if o.status_code != Some(0) {
        "exited with a nonzero status"
    } else {
        "ran"
    }
}

fn install_hint(tc: &ResolvedToolchain, host: &HostEnv, what: &str) -> String {
    match tc.source {
        ToolchainSource::Managed { .. } | ToolchainSource::Explicit { .. } => {
            "terse toolchain install (reinstalls the pinned managed TeX Live)".to_string()
        }
        ToolchainSource::System => match host.os {
            HostOs::MacOs => format!("tlmgr install {what} (MacTeX/TeX Live), or `terse toolchain install`"),
            HostOs::Windows => format!("tlmgr install {what} (TeX Live) or let MiKTeX install {what} on demand"),
            HostOs::Linux | HostOs::Other => format!("tlmgr install {what} (TeX Live), or `terse toolchain install`"),
        },
    }
}

fn engine_absent_hint(host: &HostEnv) -> &'static str {
    match host.os {
        HostOs::Windows => "install TeX Live (tug.org/texlive) or MiKTeX and make sure xelatex.exe is on PATH",
        HostOs::MacOs => "terse toolchain install, or install MacTeX/TeX Live 2025 and put /Library/TeX/texbin on PATH",
        HostOs::Linux | HostOs::Other => "terse toolchain install, or install TeX Live 2025 (texlive-xetex + biber) and put its bin/ on PATH",
    }
}

pub fn run_all(cwd: &Path, opts: &DoctorOptions, host: &HostEnv, runner: &mut dyn ProcessRunner) -> DoctorReport {
    let work_dir = crate::build::unique_temp_dir("doctor");
    let report = run_all_in(cwd, opts, host, runner, &work_dir);
    let _ = std::fs::remove_dir_all(&work_dir);
    report
}

fn run_all_in(
    cwd: &Path,
    opts: &DoctorOptions,
    host: &HostEnv,
    runner: &mut dyn ProcessRunner,
    work_dir: &Path,
) -> DoctorReport {
    // Project and profile (a project is optional for doctor).
    let (config_check, manifest_toolchain, root, profile_name) = match project::load_project(cwd) {
        Ok(ctx) => {
            let name = project::profile_name(&ctx.manifest);
            let engine = ctx.manifest.latex.engine.clone();
            (
                Check::ok("config.engine", format!("[latex] engine = \"{engine}\" agrees with profile {name}")),
                ctx.manifest.latex.toolchain.clone(),
                ctx.root.clone(),
                name,
            )
        }
        Err(ProjectError::ManifestNotFound { .. }) => (
            Check::info("config.engine", format!("no terse.toml found from {}; checking the default profile", cwd.display())),
            None,
            cwd.to_path_buf(),
            project::DEFAULT_PROFILE_NAME.to_string(),
        ),
        Err(ProjectError::EngineMismatch { configured, profile, profile_name }) => (
            Check::fail(
                "config.engine",
                format!("[latex] engine = \"{configured}\" but profile {profile_name} requires \"{profile}\""),
                CODE_ENGINE_MISMATCH,
            )
            .with_fix(format!("set [latex] engine = \"{profile}\" in terse.toml, or remove the field"), false),
            None,
            cwd.to_path_buf(),
            project::DEFAULT_PROFILE_NAME.to_string(),
        ),
        Err(e) => (
            Check::fail("config.engine", format!("the project manifest could not be loaded: {e:?}"), CODE_PROJECT_LOAD),
            None,
            cwd.to_path_buf(),
            project::DEFAULT_PROFILE_NAME.to_string(),
        ),
    };
    let profile: ExportProfile = profile::resolve_profile(&profile_name)
        .or_else(|_| profile::resolve_profile(project::DEFAULT_PROFILE_NAME))
        .expect("the default profile is always embedded");

    // Toolchain resolution.
    let selection = toolchain::select(opts.toolchain.as_ref(), manifest_toolchain.as_deref(), &root);
    let resolved = selection.and_then(|sel| toolchain::resolve(&sel, &profile.texlive_year, host));
    let (summary, source_check, tc) = match resolved {
        Ok(tc) => {
            let prefix = match &tc.source {
                ToolchainSource::Managed { prefix } => Some(prefix.display().to_string()),
                ToolchainSource::Explicit { dir } => Some(dir.display().to_string()),
                ToolchainSource::System => None,
            };
            let summary = ToolchainSummary { source: tc.source_label(), reason: tc.reason.clone(), prefix };
            let check = Check::ok("toolchain.source", format!("{}: {}", tc.source_label(), tc.reason));
            (summary, check, Some(tc))
        }
        Err(e) => {
            let summary = ToolchainSummary { source: "none", reason: e.message(), prefix: None };
            let check = Check::fail("toolchain.source", e.message(), e.code())
                .with_fix("terse toolchain install, or --toolchain system", false);
            (summary, check, None)
        }
    };

    let mut report = DoctorReport::new(summary);
    report.checks.push(Check::info("terse.version", format!("terse {}", env!("CARGO_PKG_VERSION"))));
    report.checks.push(source_check);
    report.checks.push(config_check);

    let Some(tc) = tc else {
        report.checks.push(prerequisites_check(host));
        report.checks.push(Check::info("macos.quarantine", "not checked: no toolchain resolved"));
        report.checks.push(disk_space_check(runner, host, None, work_dir));
        report.checks.push(poppler_check(host));
        report.checks.push(Check::info("compile.micro", "skipped: no toolchain resolved"));
        return report;
    };

    // The doctor's own scratch TeX user tree: every probe and the
    // micro-compile write only under `work_dir`, never into the user's
    // data directory.
    let _ = std::fs::create_dir_all(work_dir);
    let texmf = TexmfDirs::under(&work_dir.join("texmf"));
    let env = toolchain::prepare_child_env(&tc, host, &texmf);

    // xelatex
    let xelatex_ok = match &tc.xelatex {
        None => {
            report.checks.push(
                Check::fail("xelatex.present", "xelatex was not found", CODE_XELATEX_MISSING)
                    .with_cause(format!("toolchain {}: {}", tc.source_label(), tc.reason))
                    .with_fix(engine_absent_hint(host), false),
            );
            report.checks.push(Check::info("xelatex.runs", "skipped: xelatex is absent"));
            false
        }
        Some(path) => {
            report.checks.push(Check::ok("xelatex.present", path.display().to_string()));
            let out = probe(runner, path, &["--version"], work_dir, &env);
            if out.started && !out.timed_out && out.status_code == Some(0) {
                let banner = first_line(&out.stdout).unwrap_or_else(|| "(no banner)".to_string());
                match banner_year(&out.stdout) {
                    Some(year) if year == profile.texlive_year => {
                        report.checks.push(Check::ok("xelatex.runs", format!("TeX Live {year}")).with_evidence(banner));
                    }
                    Some(year) => report.checks.push(
                        Check::warn(
                            "xelatex.runs",
                            format!("TeX Live {year} but profile {} describes {}", profile.name, profile.texlive_year),
                            CODE_XELATEX_YEAR,
                        )
                        .with_evidence(banner)
                        .with_cause("export `compiled-profile` needs the profile year; builds still work")
                        .with_fix("terse toolchain install (pinned to the profile year)", false),
                    ),
                    None => report.checks.push(
                        Check::warn("xelatex.runs", "ran, but its banner names no TeX Live year", CODE_XELATEX_YEAR)
                            .with_evidence(banner),
                    ),
                }
                true
            } else {
                report.checks.push(
                    Check::fail("xelatex.runs", format!("xelatex --version {}", describe_outcome(&out)), CODE_XELATEX_RUN)
                        .with_evidence(first_line(&out.stderr).or_else(|| first_line(&out.stdout)).unwrap_or_default())
                        .with_cause("a broken or foreign xelatex executable, or a missing format file (xelatex.fmt)")
                        .with_fix(install_hint(&tc, host, "xetex latex-bin"), false),
                );
                false
            }
        }
    };

    // biber
    let mut biber_ok = false;
    match &tc.biber {
        None => {
            report.checks.push(
                Check::fail("biber.present", "biber was not found (required for citations)", CODE_BIBER_MISSING)
                    .with_fix(install_hint(&tc, host, "biber"), false),
            );
            report.checks.push(Check::info("biber.runs", "skipped: biber is absent"));
        }
        Some(path) => {
            report.checks.push(Check::ok("biber.present", path.display().to_string()));
            let mut check = biber_runs_check(runner, path, work_dir, &env, host);
            if check.status == CheckStatus::Fail && check.auto_fixable && opts.fix {
                if let Some(action) = fix::remove_stale_par_cache(host) {
                    report.actions.push(action);
                    check = biber_runs_check(runner, path, work_dir, &env, host);
                }
            }
            biber_ok = check.status == CheckStatus::Ok;
            report.checks.push(check);
        }
    }

    // kpsewhich and lookups
    let kpse_ok = match &tc.kpsewhich {
        None => {
            report.checks.push(
                Check::fail("kpsewhich.runs", "kpsewhich was not found", CODE_KPSEWHICH)
                    .with_fix(install_hint(&tc, host, "kpathsea"), false),
            );
            None
        }
        Some(path) => {
            let out = probe(runner, path, &["-var-value", "TEXMFDIST"], work_dir, &env);
            if out.started && !out.timed_out && out.status_code == Some(0) {
                let mut c = Check::ok("kpsewhich.runs", "kpsewhich -var-value TEXMFDIST");
                if let Some(l) = first_line(&out.stdout) {
                    c = c.with_evidence(format!("TEXMFDIST = {l}"));
                }
                report.checks.push(c);
                Some(path.clone())
            } else {
                report.checks.push(
                    Check::fail("kpsewhich.runs", format!("kpsewhich {}", describe_outcome(&out)), CODE_KPSEWHICH)
                        .with_fix(install_hint(&tc, host, "kpathsea"), false),
                );
                None
            }
        }
    };

    match &kpse_ok {
        None => {
            report.checks.push(Check::info("packages.resolvable", "skipped: kpsewhich unavailable"));
            report.checks.push(Check::info("fonts.resolvable", "skipped: kpsewhich unavailable"));
            report.checks.push(Check::info("babel.languages", "skipped: kpsewhich unavailable"));
        }
        Some(kpse) => {
            let files: Vec<(String, String)> = profile.packages.iter().map(|p| (p.clone(), format!("{p}.sty"))).collect();
            report.checks.push(lookup_check(
                runner,
                kpse,
                work_dir,
                &env,
                "packages.resolvable",
                "profile package",
                &files,
                CODE_PACKAGES,
                &|missing| install_hint(&tc, host, &missing.join(" ")),
            ));
            // Every variant of every profile font, not the first match per
            // token: `\setmainfont` names all four by filename.
            let fonts: Vec<(String, String)> = profile
                .fonts
                .iter()
                .flat_map(|f| {
                    FONT_FILES
                        .iter()
                        .filter(move |(tok, _)| tok == f)
                        .map(move |(_, file)| (f.clone(), file.to_string()))
                })
                .collect();
            report.checks.push(lookup_check(
                runner,
                kpse,
                work_dir,
                &env,
                "fonts.resolvable",
                "profile font",
                &fonts,
                CODE_FONTS,
                &|missing| install_hint(&tc, host, &font_packages(missing).join(" ")),
            ));
            let langs: Vec<(String, String)> = profile
                .babel_languages
                .iter()
                .filter_map(|l| BABEL_FILES.iter().find(|(tok, _)| tok == l).map(|(_, file)| (l.clone(), file.to_string())))
                .collect();
            report.checks.push(lookup_check(
                runner,
                kpse,
                work_dir,
                &env,
                "babel.languages",
                "babel language definition",
                &langs,
                CODE_BABEL,
                &|_| install_hint(&tc, host, "babel babel-english babel-portuges hyphen-english hyphen-portuguese"),
            ));
        }
    }

    report.checks.push(prerequisites_check(host));

    // macOS quarantine on managed binaries.
    report.checks.push(quarantine_check(runner, host, &tc, work_dir, &env, opts.fix, &mut report.actions));

    let space_dir = match &tc.source {
        ToolchainSource::Managed { prefix } => prefix.parent().map(Path::to_path_buf),
        _ => toolchain::paths::data_dir(host),
    };
    report.checks.push(disk_space_check(runner, host, space_dir.as_deref(), work_dir));
    report.checks.push(poppler_check(host));

    // The definitive check.
    if xelatex_ok && biber_ok {
        report.checks.push(micro_compile_check(runner, &tc, work_dir, &env));
    } else {
        report.checks.push(Check::info("compile.micro", "skipped: xelatex or biber did not pass its own check"));
    }

    report
}

fn biber_runs_check(
    runner: &mut dyn ProcessRunner,
    biber: &Path,
    work_dir: &Path,
    env: &[(String, String)],
    host: &HostEnv,
) -> Check {
    let out = probe(runner, biber, &["--version"], work_dir, env);
    if out.started && !out.timed_out && out.status_code == Some(0) {
        let banner = first_line(&out.stdout).unwrap_or_else(|| "(no banner)".to_string());
        return Check::ok("biber.runs", banner.clone()).with_evidence(banner);
    }
    if out.timed_out {
        let mut check = Check::fail(
            "biber.runs",
            format!("biber --version {}", describe_outcome(&out)),
            CODE_BIBER_RUN,
        );
        match host.username.as_deref() {
            Some(user) => {
                let dir = par_cache_dir(&host.temp_dir(), user);
                check = check
                    .with_cause(format!(
                        "biber is a PAR-packed Perl program; a stale unpack cache at {} (left by a previous biber build, typically after a TeX Live upgrade) makes it hang without output",
                        dir.display()
                    ))
                    .with_fix(format!("rm -rf {}", dir.display()), true);
            }
            None => {
                check = check.with_cause(
                    "biber is a PAR-packed Perl program; a stale unpack cache (par-<hex(username)> under the temp directory) makes it hang without output",
                );
            }
        }
        return check;
    }
    Check::fail("biber.runs", format!("biber --version {}", describe_outcome(&out)), CODE_BIBER_RUN)
        .with_evidence(first_line(&out.stderr).or_else(|| first_line(&out.stdout)).unwrap_or_default())
        .with_cause("a broken biber binary or a missing Perl runtime")
        .with_fix("terse toolchain install, or reinstall biber from your TeX distribution", false)
}

#[allow(clippy::too_many_arguments)]
fn lookup_check(
    runner: &mut dyn ProcessRunner,
    kpsewhich: &Path,
    work_dir: &Path,
    env: &[(String, String)],
    id: &'static str,
    what: &str,
    files: &[(String, String)],
    code: &'static str,
    fix_for: &dyn Fn(&[String]) -> String,
) -> Check {
    let mut missing: Vec<String> = Vec::new();
    let mut found: Vec<String> = Vec::new();
    for (name, file) in files {
        let out = probe(runner, kpsewhich, &[file.as_str()], work_dir, env);
        if out.started && !out.timed_out && out.status_code == Some(0) {
            found.push(format!("{name}: {}", first_line(&out.stdout).unwrap_or_else(|| file.clone())));
        } else {
            missing.push(name.clone());
        }
    }
    if missing.is_empty() {
        let mut c = Check::ok(id, format!("{} {what}(s) resolvable", files.len()));
        c.evidence = found;
        c
    } else {
        let mut c = Check::fail(id, format!("{what}(s) not resolvable: {}", missing.join(", ")), code)
            .with_cause(format!("the toolchain lacks the {what}(s) the generated style requires"))
            .with_fix(fix_for(&missing), false);
        c.evidence = found;
        c
    }
}

fn font_packages(tokens: &[String]) -> Vec<String> {
    tokens
        .iter()
        .map(|t| match t.as_str() {
            "tgheros" | "tgpagella" => "tex-gyre".to_string(),
            "lmodern" => "lm".to_string(),
            "libertinus-otf" => "libertinus-fonts libertinus-otf unicode-math".to_string(),
            other => other.to_string(),
        })
        .collect()
}

fn quarantine_check(
    runner: &mut dyn ProcessRunner,
    host: &HostEnv,
    tc: &ResolvedToolchain,
    work_dir: &Path,
    env: &[(String, String)],
    apply_fix: bool,
    actions: &mut Vec<String>,
) -> Check {
    if host.os != HostOs::MacOs {
        return Check::info("macos.quarantine", "not applicable on this platform");
    }
    if !matches!(tc.source, ToolchainSource::Managed { .. } | ToolchainSource::Explicit { .. }) {
        return Check::info("macos.quarantine", "not checked for a system toolchain");
    }
    let Some(xattr) = toolchain::find_tool_on_path("xattr", host).or_else(|| {
        let p = PathBuf::from("/usr/bin/xattr");
        p.is_file().then_some(p)
    }) else {
        return Check::info("macos.quarantine", "xattr is unavailable; quarantine not checked");
    };
    let binaries: Vec<&PathBuf> = [&tc.xelatex, &tc.biber, &tc.kpsewhich].into_iter().flatten().collect();
    let mut quarantined: Vec<PathBuf> = Vec::new();
    for bin in &binaries {
        if fix::is_quarantined(runner, &xattr, bin, work_dir, env) {
            quarantined.push((*bin).clone());
        }
    }
    if apply_fix && !quarantined.is_empty() {
        let mut still = Vec::new();
        for bin in &quarantined {
            actions.push(fix::clear_quarantine(runner, &xattr, bin, work_dir, env));
            if fix::is_quarantined(runner, &xattr, bin, work_dir, env) {
                still.push(bin.clone());
            }
        }
        quarantined = still;
    }
    if quarantined.is_empty() {
        Check::ok("macos.quarantine", "no managed binary carries com.apple.quarantine")
    } else {
        let list: Vec<String> = quarantined.iter().map(|p| p.display().to_string()).collect();
        Check::fail("macos.quarantine", format!("quarantined: {}", list.join(", ")), CODE_QUARANTINE)
            .with_cause("macOS Gatekeeper blocks downloaded executables until the quarantine attribute is cleared")
            .with_fix(
                quarantined.iter().map(|p| format!("xattr -d com.apple.quarantine {}", p.display())).collect::<Vec<_>>().join(" && "),
                true,
            )
    }
}

fn disk_space_check(runner: &mut dyn ProcessRunner, host: &HostEnv, dir: Option<&Path>, work_dir: &Path) -> Check {
    if host.os.is_windows() {
        return Check::info("disk.space", "free space is not checked on Windows in this version");
    }
    let target = dir.filter(|d| d.exists()).map(Path::to_path_buf).unwrap_or_else(|| host.temp_dir());
    let df = toolchain::find_tool_on_path("df", host).unwrap_or_else(|| PathBuf::from("df"));
    let env: Vec<(String, String)> = vec![("PATH".to_string(), host.path.to_string_lossy().into_owned())];
    let out = probe(runner, &df, &["-Pk", &target.display().to_string()], work_dir, &env);
    let Some(avail_kb) = parse_df_available_kb(&out.stdout) else {
        return Check::info("disk.space", format!("free space at {} could not be determined", target.display()));
    };
    let gib = avail_kb as f64 / (1024.0 * 1024.0);
    if avail_kb < 1024 * 1024 {
        Check::warn("disk.space", format!("{gib:.2} GiB free at {}", target.display()), CODE_DISK)
            .with_cause("a managed TeX Live needs roughly 0.5 GiB installed plus 150 MB of downloads")
            .with_fix("free at least 1 GiB, or choose --prefix on another volume", false)
    } else {
        Check::ok("disk.space", format!("{gib:.2} GiB free at {}", target.display()))
    }
}

fn parse_df_available_kb(stdout: &[u8]) -> Option<u64> {
    let text = String::from_utf8_lossy(stdout);
    let line = text.lines().nth(1)?;
    let fields: Vec<&str> = line.split_whitespace().collect();
    fields.get(3)?.parse().ok()
}

fn poppler_check(host: &HostEnv) -> Check {
    let mut c = Check::info("poppler.tools", "only Terse's own test suite needs pdftotext/pdfinfo/pdftoppm");
    for tool in ["pdftotext", "pdfinfo", "pdftoppm"] {
        match toolchain::find_tool_on_path(tool, host) {
            Some(p) => c.evidence.push(format!("{tool}: {}", p.display())),
            None => c.evidence.push(format!("{tool}: absent")),
        }
    }
    c
}

fn micro_compile_check(
    runner: &mut dyn ProcessRunner,
    tc: &ResolvedToolchain,
    work_dir: &Path,
    env: &[(String, String)],
) -> Check {
    let dir = work_dir.join("micro");
    if let Err(e) = std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::write(dir.join("paper.tex"), MICRO_TEX))
        .and_then(|_| std::fs::write(dir.join("paper.bib"), MICRO_BIB))
    {
        return Check::fail("compile.micro", format!("could not stage the micro document: {e}"), CODE_MICRO_COMPILE);
    }
    let (Some(xelatex), Some(biber)) = (&tc.xelatex, &tc.biber) else {
        return Check::info("compile.micro", "skipped: xelatex or biber is absent");
    };
    let config = EngineConfig { xelatex: xelatex.clone(), biber: biber.clone(), timeout: MICRO_COMPILE_TIMEOUT };
    match engine::compile_bounded_with_env(runner, &dir, "paper", &config, true, env) {
        Ok(passes) => {
            if dir.join("paper.pdf").is_file() {
                Check::ok("compile.micro", format!("xelatex, biber, xelatex produced a PDF in {} passes", passes.len()))
            } else {
                let mut c = Check::fail(
                    "compile.micro",
                    "every pass exited 0 but no paper.pdf was produced",
                    CODE_MICRO_COMPILE,
                );
                c.evidence = excerpt(passes.last());
                c
            }
        }
        Err((passes, failure)) => {
            let diag = engine::logs::interpret_failure(&failure);
            let mut c = Check::fail("compile.micro", diag.message.clone(), CODE_MICRO_COMPILE)
                .with_cause(format!("{} pass failed; see the log excerpt", passes.last().map(|p| format!("{:?}", p.kind)).unwrap_or_else(|| "engine".to_string())));
            if let Some(help) = diag.help {
                c = c.with_fix(help, false);
            }
            c.evidence = excerpt(passes.last());
            c
        }
    }
}

/// The last non-empty lines of a pass's output, as evidence.
fn excerpt(pass: Option<&engine::PassResult>) -> Vec<String> {
    let Some(pass) = pass else { return Vec::new() };
    let mut lines: Vec<String> = Vec::new();
    for bytes in [&pass.outcome.stdout, &pass.outcome.stderr] {
        for l in String::from_utf8_lossy(bytes).lines() {
            let t = l.trim();
            if !t.is_empty() {
                lines.push(t.to_string());
            }
        }
    }
    let keep = lines.len().saturating_sub(5);
    lines.split_off(keep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_banner_year_and_df_parsing() {
        assert_eq!(banner_year(b"XeTeX 3.141592653-2.6-0.999997 (TeX Live 2025)\n"), Some("2025".to_string()));
        assert_eq!(banner_year(b"XeTeX (MiKTeX 24.1)\n"), None);
        assert_eq!(
            parse_df_available_kb(b"Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/disk1 976562500 500000000 476562500 52% /\n"),
            Some(476_562_500)
        );
        assert_eq!(parse_df_available_kb(b""), None);
    }

    #[test]
    fn test_doctor_codes_are_unique_and_in_family() {
        let mut seen = std::collections::HashSet::new();
        for code in ALL_CODES {
            assert!(code.starts_with("E-TOOL-"), "{code}");
            assert!(seen.insert(*code), "duplicate {code}");
        }
        for taken in ["E-TOOL-013", "E-TOOL-015", "E-TOOL-016"] {
            assert!(!ALL_CODES.contains(&taken));
        }
    }
}
