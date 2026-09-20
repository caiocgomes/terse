pub mod args;
pub mod build;
pub mod cache;
pub mod diagnostics;
pub mod doctor;
pub mod engine;
pub mod export;
pub mod format;
pub mod init;
pub mod project;
pub mod publication;
pub mod references;
pub mod toolchain;
pub mod watch;

use std::path::Path;

use clap::Parser;

use toolchain::{HostEnv, ToolchainSelector};

/// Injectable application entrypoint: takes explicit argv and working
/// directory rather than reading `std::env` itself, so tests can drive the
/// full CLI without touching the real process environment. The host
/// environment (tool search path, temp and home directories) is captured
/// once here; [`run_with_host`] takes it as data.
pub fn run<I, S>(raw_args: I, cwd: &Path) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<std::ffi::OsString> + Clone,
{
    run_with_host(raw_args, cwd, &HostEnv::capture())
}

/// As [`run`], with an explicit host environment so tests can control
/// where tools and the managed prefix are looked for.
pub fn run_with_host<I, S>(raw_args: I, cwd: &Path, host: &HostEnv) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<std::ffi::OsString> + Clone,
{
    // The real downloader exists only for `toolchain install|update`; the
    // factory is invoked inside that path and nowhere else, so every
    // other command is network-free by construction, not by convention.
    let mut make_downloader =
        || -> Box<dyn toolchain::download::ArchiveDownloader> { Box::new(toolchain::download::RealArchiveDownloader::new()) };
    run_with_host_and_downloader(raw_args, cwd, host, &mut make_downloader)
}

/// As [`run_with_host`], with the archive downloader supplied lazily so
/// tests can prove which commands ask for it (only `toolchain
/// install|update`) and record every request.
pub fn run_with_host_and_downloader<I, S>(
    raw_args: I,
    cwd: &Path,
    host: &HostEnv,
    make_downloader: &mut dyn FnMut() -> Box<dyn toolchain::download::ArchiveDownloader>,
) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<std::ffi::OsString> + Clone,
{
    let cli = match args::Cli::try_parse_from(raw_args) {
        Ok(cli) => cli,
        // `--version`/`--help` are successful informational output on
        // stdout, not usage errors.
        Err(e) if matches!(e.kind(), clap::error::ErrorKind::DisplayVersion | clap::error::ErrorKind::DisplayHelp) => {
            print!("{e}");
            return 0;
        }
        Err(e) => {
            eprint!("{e}");
            return 2;
        }
    };

    let flag = |value: &Option<String>| value.as_deref().map(|v| ToolchainSelector::parse_flag(v, cwd));

    match cli.command {
        args::Command::Init { directory, force } => {
            init::run_init(cwd, directory.as_deref(), force)
        }
        args::Command::Check {
            entry,
            theme,
            strict,
            deny_warnings,
            json,
            target,
            toolchain,
        } => match target {
            Some(target) => run_check_target(
                cwd,
                entry.as_deref(),
                theme.as_deref(),
                &target,
                json,
                flag(&toolchain).as_ref(),
                host,
            ),
            None => build::run_check(cwd, entry.as_deref(), theme.as_deref(), strict, deny_warnings, json),
        },
        args::Command::Build {
            entry,
            theme,
            tex_only,
            require_pdf,
            json,
            toolchain,
        } => build::run_build_with_host(
            cwd,
            entry.as_deref(),
            theme.as_deref(),
            tex_only,
            require_pdf,
            json,
            flag(&toolchain).as_ref(),
            host,
            &mut engine::RealProcessRunner,
        ),
        args::Command::Fmt { paths, check, json } => format::run_fmt(cwd, &paths, check, json),
        args::Command::Export {
            entry,
            theme,
            target,
            include_bbl,
            json,
            require_compile,
            toolchain,
        } => run_export(
            cwd,
            entry.as_deref(),
            theme.as_deref(),
            &target,
            include_bbl.as_deref(),
            require_compile,
            json,
            flag(&toolchain).as_ref(),
            host,
        ),
        args::Command::Refs { action } => match action {
            args::RefsAction::Resolve { entry, refresh, offline, prune } => {
                run_refs_resolve(cwd, entry.as_deref(), refresh, offline, prune)
            }
        },
        args::Command::Watch { entry, theme, tex_only, json, toolchain } => run_watch(
            cwd,
            entry.as_deref(),
            theme.as_deref(),
            tex_only,
            json,
            flag(&toolchain).as_ref(),
            host,
        ),
        args::Command::Doctor { json, fix, toolchain } => {
            let opts = doctor::DoctorOptions { fix, json, toolchain: flag(&toolchain) };
            doctor::run_doctor_cli(cwd, &opts, host, &mut engine::RealProcessRunner)
        }
        args::Command::Toolchain { action } => {
            engine::clear_interrupt();
            toolchain::commands::run_toolchain(cwd, &action, host, &mut engine::RealProcessRunner, make_downloader)
        }
    }
}

/// Installs (unix only) a `SIGINT` handler that only ever sets an atomic
/// flag -- no allocation, no locking, nothing async-signal-unsafe -- and
/// returns a channel that receives exactly one message once it fires. On
/// non-unix platforms `terse watch` still runs; it can only be stopped by
/// the OS terminating the process (no owned child process is ever left
/// running past that, since every engine invocation is bounded by its own
/// timeout regardless of how the process itself ends).
#[cfg(unix)]
fn install_interrupt_channel() -> std::sync::mpsc::Receiver<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;

    static INTERRUPTED: AtomicBool = AtomicBool::new(false);
    extern "C" fn handle_sigint(_signum: i32) {
        INTERRUPTED.store(true, Ordering::SeqCst);
        // Children run in their own process group, so the terminal's SIGINT
        // never reaches them; the runner polls this flag and kills the
        // group itself.
        engine::request_interrupt();
    }
    extern "C" {
        fn signal(signum: i32, handler: usize) -> usize;
    }
    const SIGINT: i32 = 2;
    unsafe {
        signal(SIGINT, handle_sigint as *const () as usize);
    }

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || loop {
        if INTERRUPTED.load(Ordering::SeqCst) {
            let _ = tx.send(());
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    });
    rx
}

#[cfg(not(unix))]
fn install_interrupt_channel() -> std::sync::mpsc::Receiver<()> {
    // No message is ever sent; the receiver simply never fires on this
    // platform, matching pre-interrupt-support behavior.
    let (_tx, rx) = std::sync::mpsc::channel();
    rx
}

fn run_watch(
    cwd: &Path,
    entry: Option<&Path>,
    theme: Option<&str>,
    tex_only: bool,
    json: bool,
    flag: Option<&ToolchainSelector>,
    host: &HostEnv,
) -> i32 {
    let options = watch::WatchOptions {
        theme_name: theme.unwrap_or("academic").to_string(),
        tex_only,
        json,
    };
    engine::clear_interrupt();
    let stop = install_interrupt_channel();
    let mut runner = engine::RealProcessRunner;
    watch::run_with_toolchain(cwd, entry, &options, &stop, &mut runner, flag, host, &mut |report| {
        let label = match report.status {
            watch::AttemptStatus::Success => "success",
            watch::AttemptStatus::Failure => "failure",
            watch::AttemptStatus::Superseded => "superseded",
        };
        if let Some(json) = &report.json {
            println!("{json}");
        } else {
            eprintln!("watch [{label}]: {}", report.message);
        }
    })
}

fn run_check_target(
    cwd: &Path,
    entry: Option<&Path>,
    theme: Option<&str>,
    target: &str,
    json: bool,
    flag: Option<&ToolchainSelector>,
    host: &HostEnv,
) -> i32 {
    let (proj, entry_path) = match project::resolve_project(entry, cwd) {
        Ok(v) => v,
        Err(e) => return build::report_project_error(&e),
    };
    if target != "arxiv" {
        eprintln!("error[E-EXPORT-001]: unknown export target '{target}'");
        return 2;
    }
    // Target validation is engine-free, but an explicit toolchain
    // selection that cannot resolve is still a configuration error here,
    // so `check --target` and `export --target` agree.
    if let Err(code) = build::resolve_toolchain_for(&proj, flag, host) {
        return code;
    }
    let theme_name = theme.unwrap_or("academic");
    match export::validate_target(&proj, &entry_path, theme_name, export::DEFAULT_TARGET_PROFILE) {
        Ok(_) => {
            if json {
                println!("{{\"target\": \"{target}\", \"valid\": true}}");
            } else {
                println!("check --target {target}: ok");
            }
            0
        }
        Err(e) => report_target_error(&proj, &entry_path, e, json),
    }
}

fn run_export(
    cwd: &Path,
    entry: Option<&Path>,
    theme: Option<&str>,
    target: &str,
    include_bbl: Option<&Path>,
    require_compile: bool,
    json: bool,
    flag: Option<&ToolchainSelector>,
    host: &HostEnv,
) -> i32 {
    let (proj, entry_path) = match project::resolve_project(entry, cwd) {
        Ok(v) => v,
        Err(e) => return build::report_project_error(&e),
    };
    if target != "arxiv" {
        eprintln!("error[E-EXPORT-001]: unknown export target '{target}'");
        return 2;
    }
    let toolchain = match build::resolve_toolchain_for(&proj, flag, host) {
        Ok(tc) => tc,
        Err(code) => return code,
    };
    let theme_name = theme.unwrap_or("academic");
    // Snapshot the current dependency set's bytes now, before the
    // (possibly slow, when `--require-compile` runs a real engine)
    // validation/staging work below: if any of these change before
    // publication, the just-built candidate reflects inputs that are no
    // longer current and must not be published (reusing the same
    // stale-snapshot discipline group 21's watcher already established).
    let pre_deps = match project::load_modules(&proj.root, &entry_path) {
        Ok(l) => Some(l.dependencies),
        Err(_) => None,
    };
    let pre_snapshot = pre_deps.as_ref().map(|d| watch::snapshot(&proj.root, d));
    let validated = match export::validate_target(&proj, &entry_path, theme_name, export::DEFAULT_TARGET_PROFILE) {
        Ok(v) => v,
        Err(e) => return report_target_error(&proj, &entry_path, e, json),
    };

    // Default membership never includes a prebuilt `.bbl` -- source
    // generation below produces only `references.bib`, so there is
    // nothing to strip; a requested `.bbl` is verified and appended
    // explicitly, never substituted or renamed to fit.
    let bbl_file = match include_bbl {
        Some(path) => match export::verify_include_bbl(path, validated.needs_biber) {
            Ok(bytes) => Some(("paper.bbl".to_string(), bytes)),
            Err(e) => {
                eprintln!("error[E-EXPORT-005]: --include-bbl rejected: {e:?}");
                return 2;
            }
        },
        None => None,
    };

    let manifest = match terse_core::artifact::plan_source_artifacts_with_support(
        &validated.plan.module,
        &validated.theme,
        &validated.extra_packages,
        &validated.support_files,
        &validated.cited,
    ) {
        Ok(m) => m,
        Err(terse_core::artifact::SupportFileCollision(name)) => {
            eprintln!("error[E-LATEX-004]: declared support file '{name}' collides with a generated file name");
            return 2;
        }
    };
    // The portable arXiv allowlist (group 23): drop internal-only
    // artifacts (`COMPILE.txt`, `build-manifest.json`) that a normal
    // build emits but an extracted, Terse-free package has no use for;
    // retain everything else the current validated plan generated
    // (source, style, bibliography, used assets/support files),
    // plus a verified `.bbl` if one was requested.
    let mut export_files = terse_core::artifact::export::export_membership(&manifest.files);
    if let Some((path, bytes)) = bbl_file {
        export_files.push(terse_core::artifact::GeneratedFile { logical_path: path, bytes });
    }
    export_files.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));

    let paths: Vec<String> = export_files.iter().map(|f| f.logical_path.clone()).collect();
    let violations = terse_core::artifact::export::validate_portable_paths(&paths);
    if !violations.is_empty() {
        eprintln!("error[E-EXPORT-006]: export archive members are not portable: {violations:?}");
        return 2;
    }

    let export_manifest = terse_core::artifact::export::build_export_manifest(&export_files);
    export_files.push(export_manifest);
    export_files.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));

    let file_tuples: Vec<(String, Vec<u8>)> =
        export_files.iter().map(|f| (f.logical_path.clone(), f.bytes.clone())).collect();

    let entry_stem = entry_path.file_stem().and_then(|s| s.to_str()).unwrap_or("paper");
    let zip_bytes = export::archive::write_zip(&export_files);

    // Extraction and manifest verification always happen BEFORE
    // publication (group 24.2): a corrupted/tampered archive must never
    // reach a real compile or replace a prior, working export. An actual
    // clean-environment compile is only ATTEMPTED when `--require-compile`
    // asks for one -- ordinary `export` remains a fast, purely structural
    // operation exactly as before this group, reporting an honest
    // `static-only` result rather than silently taking on real-compile
    // cost and failure modes nobody asked for.
    let scratch = build::unique_temp_dir("export-validate");
    let compile_outcome = (|| -> Result<export::validate::CompileReport, String> {
        std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
        export::validate::extract_zip(&zip_bytes, &scratch).map_err(|e| format!("{e:?}"))?;
        let manifest_text = export_files
            .iter()
            .find(|f| f.logical_path == terse_core::artifact::export::MANIFEST_FILE_NAME)
            .map(|f| String::from_utf8_lossy(&f.bytes).into_owned())
            .unwrap_or_default();
        export::validate::verify_extracted_manifest(&scratch, &manifest_text).map_err(|e| format!("{e:?}"))?;

        if !require_compile {
            return Ok(export::validate::CompileReport::StaticOnly {
                reason: "compilation was not attempted (pass --require-compile to compile and verify the \
                         extracted package in a clean environment)"
                    .to_string(),
            });
        }
        let mut runner = engine::RealProcessRunner;
        export::validate::compile_extracted(
            &scratch,
            entry_stem,
            validated.needs_biber,
            require_compile,
            &validated.profile,
            &toolchain,
            host,
            &mut runner,
        )
        .map_err(|e| format!("{e:?}"))
    })();
    let _ = std::fs::remove_dir_all(&scratch);

    let report = match compile_outcome {
        Ok(report) => report,
        Err(e) => {
            eprintln!("error[E-EXPORT-007]: extracted-package validation failed: {e}");
            return 3;
        }
    };
    eprintln!("{}", report.render(&validated.profile.name));

    // Group 25: the extracted directory, its sibling ZIP, and the
    // compilation report are one managed generation. `generation_dir`
    // (the theme-level parent, `<output>/export/<theme>/`) is the single
    // publication unit -- everything is staged into ONE tree and
    // installed with ONE atomic rename via `publication`, exactly as an
    // ordinary build's output directory is (group 5). This composes
    // directly with that module's existing rollback/restart-journal
    // recovery rather than needing an export-specific transaction
    // mechanism: a crash between what used to be three independent
    // writes can no longer happen, because there is only one write.
    let generation_dir = proj
        .root
        .join(&proj.manifest.project.output)
        .join("export")
        .join(theme_name);
    let output_dir = generation_dir.join("arxiv");
    let zip_path = generation_dir.join(format!("{entry_stem}-arxiv.zip"));
    let report_path = generation_dir.join(format!("{entry_stem}-arxiv-report.txt"));

    // The compilation report is deliberately never a member of the
    // deterministic source archive itself (group 23's byte-determinism
    // contract must not depend on this-machine-specific data like a
    // detected local toolchain version) -- it is staged as a sibling
    // file within the same managed generation instead.
    let mut generation_files: Vec<(String, Vec<u8>)> = file_tuples
        .iter()
        .map(|(name, bytes)| (format!("arxiv/{name}"), bytes.clone()))
        .collect();
    generation_files.push((format!("{entry_stem}-arxiv.zip"), zip_bytes));
    generation_files.push((
        format!("{entry_stem}-arxiv-report.txt"),
        report.render(&validated.profile.name).into_bytes(),
    ));

    if let (Some(deps), Some(before)) = (&pre_deps, &pre_snapshot) {
        let after = watch::snapshot(&proj.root, deps);
        if &after != before {
            eprintln!(
                "error[E-EXPORT-008]: a source, theme, asset, or support file changed while this export was \
                 being validated; re-run export against the current sources rather than publishing a stale generation"
            );
            return 3;
        }
    }

    if let Err(e) = publication::check_destination_ownership(&generation_dir) {
        eprintln!("error[E-CONFIG-007]: refusing to replace an unowned populated export directory ({e:?})");
        return 3;
    }
    let staged = match publication::stage(&generation_dir, &generation_files) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: staging the export failed: {e:?}");
            return 3;
        }
    };
    if let Err(e) = publication::publish(staged, &generation_dir) {
        eprintln!("error: publishing the export failed: {e:?}");
        return 3;
    }

    if json {
        println!(
            "{{\"target\": \"{target}\", \"published\": true, \"files\": {}, \"archive\": \"{}\", \"report\": \"{}\"}}",
            file_tuples.len(),
            zip_path.display(),
            report_path.display()
        );
    } else {
        println!(
            "export: published {} files to {} (archive: {}, report: {})",
            file_tuples.len(),
            output_dir.display(),
            zip_path.display(),
            report_path.display()
        );
    }
    0
}

fn report_target_error(proj: &project::ProjectContext, entry_path: &Path, e: export::TargetError, json: bool) -> i32 {
    match e {
        export::TargetError::Diagnostics(diags) => {
            let file_index = project::load_modules(&proj.root, entry_path)
                .ok()
                .map(|l| l.file_index)
                .unwrap_or_default();
            if json {
                println!("{}", diagnostics::render_json(&diags, &file_index, entry_path));
            } else {
                for d in &diags {
                    eprintln!("{}", build::render(&file_index, entry_path, d));
                }
            }
            1
        }
        export::TargetError::UnknownProfile(name) => {
            eprintln!("error[E-EXPORT-002]: unknown export compatibility profile '{name}'");
            2
        }
        export::TargetError::UnsupportedDependency(msg) => {
            eprintln!("error[E-EXPORT-003]: {msg}");
            2
        }
        export::TargetError::Cli(code) => code,
    }
}

fn run_refs_resolve(
    cwd: &Path,
    entry: Option<&Path>,
    refresh: bool,
    offline: bool,
    prune: bool,
) -> i32 {
    let (ctx, entry_path) = match project::resolve_project(entry, cwd) {
        Ok(v) => v,
        Err(e) => return build::report_project_error(&e),
    };

    let options = references::resolve::ResolveOptions { refresh, offline, prune };
    let mut transport = references::transport::RealTransport::new();
    let mut clock = references::clock::RealClock;

    match references::resolve::run(&ctx.root, &entry_path, options, &mut transport, &mut clock) {
        Ok(report) => {
            for alias in &report.fetched {
                eprintln!("resolved {alias}");
            }
            for alias in &report.resealed {
                eprintln!("resealed {alias} (offline)");
            }
            for alias in &report.unchanged {
                eprintln!("unchanged {alias}");
            }
            for alias in &report.pruned {
                eprintln!("pruned {alias}");
            }
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            match e {
                references::resolve::ResolveError::ConflictingFlags => 2,
                _ => 4,
            }
        }
    }
}
