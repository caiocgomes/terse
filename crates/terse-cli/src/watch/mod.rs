//! `terse watch`: an immediate initial build, then debounced rebuilds
//! triggered by relevant filesystem changes, with the last successful
//! generation always preserved across a failing or superseded attempt.

pub mod dependencies;
pub mod scheduler;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use terse_core::project::dependencies::DependencySet;

use crate::build;
use crate::engine::{self, ProcessRunner};
use crate::project::{self, ProjectContext};
use crate::publication;
use crate::toolchain::{HostEnv, ResolvedToolchain, ToolchainSelector};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptStatus {
    Success,
    Failure,
    Superseded,
}

#[derive(Debug, Clone)]
pub struct AttemptReport {
    pub status: AttemptStatus,
    /// Human-readable summary; JSON mode additionally receives the raw
    /// diagnostics through [`AttemptReport::json`].
    pub message: String,
    pub json: Option<String>,
}

struct AttemptOutcome {
    report: AttemptReport,
    dependencies: DependencySet,
}

pub struct WatchOptions {
    pub theme_name: String,
    pub tex_only: bool,
    pub json: bool,
}

/// Reads every currently-tracked dependency's bytes (missing files simply
/// contribute no bytes), for detecting whether any of them changed between
/// the moment an attempt read its inputs and the moment it is about to
/// publish (21.5): a mismatch means the attempt is stale and must not be
/// published, even though it may otherwise have succeeded.
pub(crate) fn snapshot(root: &Path, deps: &DependencySet) -> Vec<(String, Option<Vec<u8>>)> {
    let mut out: Vec<(String, Option<Vec<u8>>)> = deps
        .resolved
        .iter()
        .map(|p| (p.clone(), std::fs::read(root.join(p)).ok()))
        .collect();
    out.sort();
    out
}

/// Runs exactly one build attempt: loads sources, compiles, resolves the
/// theme/assets, generates source artifacts (through the same disposable
/// cache and optional XeLaTeX/Biber path `build`/`check` use), and
/// publishes -- unless the tracked inputs changed again after this attempt
/// read them, in which case it reports `Superseded` and touches nothing.
fn run_attempt(
    project: &ProjectContext,
    entry_path: &Path,
    options: &WatchOptions,
    engine: Option<&(ResolvedToolchain, Vec<(String, String)>)>,
    runner: &mut dyn ProcessRunner,
) -> AttemptOutcome {
    run_attempt_with_hook(project, entry_path, options, engine, runner, &mut || {})
}

/// As [`run_attempt`], but invokes `mid_attempt` once, right after this
/// attempt has taken its pre-publication input snapshot. Production always
/// passes a no-op; tests use it to simulate an edit landing while a build
/// is still in flight, without needing a real background thread or a slow
/// real compile to create the race window.
fn run_attempt_with_hook(
    project: &ProjectContext,
    entry_path: &Path,
    options: &WatchOptions,
    engine: Option<&(ResolvedToolchain, Vec<(String, String)>)>,
    runner: &mut dyn ProcessRunner,
    mid_attempt: &mut dyn FnMut(),
) -> AttemptOutcome {
    let loaded = match project::load_modules(&project.root, entry_path) {
        Ok(l) => l,
        Err(e) => {
            return AttemptOutcome {
                report: AttemptReport {
                    status: AttemptStatus::Failure,
                    message: format!("error loading sources: {e:?}"),
                    json: None,
                },
                dependencies: DependencySet::new(),
            }
        }
    };
    let mut dependencies = loaded.dependencies.clone();
    dependencies.add_resolved("terse.toml");

    let (diags, plan) = terse_core::compile(&loaded.snapshot);
    if !diags.is_empty() {
        let message = diags
            .iter()
            .map(|d| build::render(&loaded.file_index, entry_path, d))
            .collect::<Vec<_>>()
            .join("\n");
        let json = options
            .json
            .then(|| crate::diagnostics::render_json(&diags, &loaded.file_index, entry_path));
        return AttemptOutcome {
            report: AttemptReport { status: AttemptStatus::Failure, message, json },
            dependencies,
        };
    }
    let mut plan = plan.expect("no diagnostics implies a valid artifact plan");

    let (mut theme, theme_dir) = match build::resolve_theme(project, &options.theme_name) {
        Ok(v) => v,
        Err(_) => {
            return AttemptOutcome {
                report: AttemptReport {
                    status: AttemptStatus::Failure,
                    message: format!("theme '{}' failed to resolve", options.theme_name),
                    json: None,
                },
                dependencies,
            }
        }
    };
    if let Some(declared) = project.manifest.themes.get(&options.theme_name) {
        dependencies.add_resolved(declared.clone());
    }

    let entry_dir = entry_path.parent().unwrap_or(&project.root);
    for f in terse_core::artifact::assets::collect_figure_paths(&plan.module.blocks) {
        if let Some(rel) = root_relative_join(&project.root, entry_dir, &f) {
            dependencies.add_resolved(rel);
        }
    }
    if let Some(logo) = &theme.logo_path {
        if let Some(rel) = root_relative_join(&project.root, &theme_dir, logo) {
            dependencies.add_resolved(rel);
        }
    }
    for sf in &project.manifest.latex.support_files {
        dependencies.add_resolved(sf.clone());
    }

    // Taken as early as the full dependency set is known, so it covers the
    // entire remaining generation/compile window, not just the instant
    // before publication.
    let before = snapshot(&project.root, &dependencies);
    mid_attempt();

    let asset_files = match build::resolve_assets(project, entry_dir, &theme_dir, &mut plan.module, &mut theme) {
        Ok(v) => v,
        Err(_) => {
            return AttemptOutcome {
                report: AttemptReport {
                    status: AttemptStatus::Failure,
                    message: "asset resolution failed".to_string(),
                    json: None,
                },
                dependencies,
            }
        }
    };

    let extra_packages = match terse_core::latex::validate_packages(&project.manifest.latex.packages) {
        Ok(v) => v,
        Err(terse_core::latex::UnknownPackage(name)) => {
            return AttemptOutcome {
                report: AttemptReport {
                    status: AttemptStatus::Failure,
                    message: format!("unknown declared package '{name}'"),
                    json: None,
                },
                dependencies,
            }
        }
    };
    let mut support_files = match build::load_support_files(&project.root, &project.manifest.latex.support_files) {
        Ok(v) => v,
        Err(_) => {
            return AttemptOutcome {
                report: AttemptReport {
                    status: AttemptStatus::Failure,
                    message: "reading a declared support file failed".to_string(),
                    json: None,
                },
                dependencies,
            }
        }
    };
    let mut all_files = asset_files;
    all_files.append(&mut support_files);

    let cited_aliases = terse_core::semantic::collect_cited_aliases(&plan.module);
    let cited: BTreeMap<_, _> = plan
        .bindings
        .authorized
        .iter()
        .filter(|(alias, _)| cited_aliases.contains(*alias))
        .map(|(alias, record)| (alias.clone(), record.clone()))
        .collect();
    let needs_biber = !cited.is_empty();

    let manifest = match terse_core::artifact::plan_source_artifacts_with_support(
        &plan.module,
        &theme,
        &extra_packages,
        &all_files,
        &cited,
    ) {
        Ok(m) => m,
        Err(terse_core::artifact::SupportFileCollision(name)) => {
            return AttemptOutcome {
                report: AttemptReport {
                    status: AttemptStatus::Failure,
                    message: format!("declared support file '{name}' collides with a generated file name"),
                    json: None,
                },
                dependencies,
            }
        }
    };
    let mut files = manifest.files;

    if let (false, Some((tc, env))) = (options.tex_only, engine) {
        match build::find_pdf_engine(tc, false) {
            build::EngineAvailability::Present { xelatex, biber } => {
                match build::compile_pdf(&files, &xelatex, biber.as_deref(), needs_biber, env, runner) {
                    Ok(pdf_bytes) => files.push(terse_core::artifact::GeneratedFile {
                        logical_path: "paper.pdf".to_string(),
                        bytes: pdf_bytes,
                    }),
                    Err(failure) => {
                        let diag = engine::logs::interpret_failure(&failure);
                        return AttemptOutcome {
                            report: AttemptReport {
                                status: AttemptStatus::Failure,
                                message: crate::diagnostics::render_human(&diag, "(engine)", ""),
                                json: None,
                            },
                            dependencies,
                        };
                    }
                }
            }
            _ => {
                // Missing (and not required): fall back to source-only,
                // exactly like `build` does.
            }
        }
    }

    // 21.5: re-read every tracked dependency right before publication. A
    // difference from what this attempt actually compiled against means an
    // edit landed mid-build; publishing now would ship stale content, so
    // this attempt is superseded instead -- the scheduler's pending-event
    // bookkeeping (a filesystem event already arrived) ensures a fresh
    // attempt follows.
    let after = snapshot(&project.root, &dependencies);
    if before != after {
        return AttemptOutcome {
            report: AttemptReport {
                status: AttemptStatus::Superseded,
                message: "inputs changed during this attempt; scheduling a fresh build".to_string(),
                json: None,
            },
            dependencies,
        };
    }

    let output_dir = project.root.join(&project.manifest.project.output).join(&options.theme_name);
    let file_tuples: Vec<(String, Vec<u8>)> = files.into_iter().map(|f| (f.logical_path, f.bytes)).collect();
    if let Err(e) = publication::check_destination_ownership(&output_dir) {
        return AttemptOutcome {
            report: AttemptReport {
                status: AttemptStatus::Failure,
                message: format!("refusing to replace an unowned populated output directory ({e:?})"),
                json: None,
            },
            dependencies,
        };
    }
    let staged = match publication::stage(&output_dir, &file_tuples) {
        Ok(s) => s,
        Err(e) => {
            return AttemptOutcome {
                report: AttemptReport {
                    status: AttemptStatus::Failure,
                    message: format!("staging failed: {e:?}"),
                    json: None,
                },
                dependencies,
            }
        }
    };
    if let Err(e) = publication::publish(staged, &output_dir) {
        return AttemptOutcome {
            report: AttemptReport {
                status: AttemptStatus::Failure,
                message: format!("publishing failed: {e:?}"),
                json: None,
            },
            dependencies,
        };
    }

    AttemptOutcome {
        report: AttemptReport {
            status: AttemptStatus::Success,
            message: format!("build: published {} files to {}", file_tuples.len(), output_dir.display()),
            json: None,
        },
        dependencies,
    }
}

/// Blocks (up to a bounded deadline) until `watch` reports an event for a
/// freshly-written, uniquely-named sentinel file under `root`, draining and
/// discarding every event seen along the way so none of them are mistaken
/// for a real dependency change once the main loop starts. If the deadline
/// passes with no confirmation (an unresponsive or broken watcher), this
/// simply returns anyway rather than hanging watch forever; the ordinary
/// polling loop would eventually catch the same failure.
/// Blocks (up to a bounded deadline) until the watcher confirms it has
/// started delivering events, by round-tripping a private sentinel file
/// through it. A real edit can race with this handshake -- on backends with
/// nontrivial startup latency (observed: several seconds for a fresh
/// FSEvents stream in some sandboxes), a genuine dependency change can
/// arrive on `watch.events` before the sentinel does. Earlier versions of
/// this function silently discarded every non-sentinel event seen during
/// the wait, which dropped such a race-landing edit forever; this version
/// instead returns every other path observed along the way, so the caller
/// can feed them into the ordinary event-handling path once the loop
/// starts, exactly as if they had arrived after readiness was confirmed.
fn wait_for_watcher_ready(root: &Path, watch: &dependencies::FsWatch) -> Vec<PathBuf> {
    let sentinel = root.join(format!(".terse-watch-ready-{}", std::process::id()));
    if std::fs::write(&sentinel, b"").is_err() {
        return Vec::new();
    }
    let mut leftover = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match watch.events.recv_timeout(Duration::from_millis(50)) {
            Ok(path) if path == sentinel => break,
            Ok(path) => leftover.push(path),
            Err(_) => {}
        }
    }
    let _ = std::fs::remove_file(&sentinel);
    // The sentinel's own create/remove events may also have been queued
    // behind it (or arrive later); they carry no dependency meaning and
    // would otherwise show up as a spurious "relevant" path.
    leftover.retain(|p| p != &sentinel);
    leftover
}

fn root_relative_join(root: &Path, base_dir: &Path, relative: &str) -> Option<String> {
    let candidate = base_dir.join(relative);
    let rel = candidate.strip_prefix(root).ok()?;
    Some(rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/"))
}

/// Drives one full watch session. `stop` signals interruption: as soon as a
/// message arrives (or the sender is dropped), the loop stops scheduling
/// new work, lets any owned `runner` invocation it is mid-attempt with have
/// already returned (attempts are synchronous, so there is no orphaned
/// child process to separately kill beyond what `runner`/`compile_pdf`
/// already bound to a timeout), and returns without publishing a partial
/// generation. `on_attempt` is invoked once per completed attempt in the
/// order they finish.
pub fn run(
    cwd: &Path,
    entry: Option<&Path>,
    options: &WatchOptions,
    stop: &mpsc::Receiver<()>,
    runner: &mut dyn ProcessRunner,
    on_attempt: &mut dyn FnMut(&AttemptReport),
) -> i32 {
    run_with_toolchain(cwd, entry, options, stop, runner, None, &HostEnv::capture(), on_attempt)
}

/// As [`run`], with the toolchain selection and host environment explicit.
/// The toolchain is resolved once, before the initial build; an explicit
/// selection that cannot resolve is a configuration error before any
/// watching starts.
#[allow(clippy::too_many_arguments)]
pub fn run_with_toolchain(
    cwd: &Path,
    entry: Option<&Path>,
    options: &WatchOptions,
    stop: &mpsc::Receiver<()>,
    runner: &mut dyn ProcessRunner,
    flag: Option<&ToolchainSelector>,
    host: &HostEnv,
    on_attempt: &mut dyn FnMut(&AttemptReport),
) -> i32 {
    let (project, entry_path) = match project::resolve_project(entry, cwd) {
        Ok(v) => v,
        Err(e) => return build::report_project_error(&e),
    };
    let engine = if options.tex_only {
        None
    } else {
        match build::resolve_toolchain_for(&project, flag, host) {
            Ok(tc) => {
                let env = build::child_env_for(&tc, host);
                Some((tc, env))
            }
            Err(code) => return code,
        }
    };
    let engine = engine.as_ref();

    let mut scheduler = scheduler::Scheduler::new();

    // The watcher is started, and confirmed ready, *before* the initial
    // build runs (and, critically, before `on_attempt` is invoked for it).
    // A caller reacting to "the initial build finished" by immediately
    // editing a source file is an entirely ordinary, expected sequence
    // (every test in this file does exactly that) -- if the watcher did
    // not exist yet at that moment, that edit's filesystem event would
    // never be generated at all, not merely dropped, since the OS has no
    // stream to report it on. Creating the watcher first means any such
    // edit's event is queued on the (buffered) `watch.events` channel
    // immediately, to be picked up once the main loop starts, however
    // quickly the caller reacts.
    //
    // The OS watcher (FSEvents on macOS in particular) reports paths in
    // their canonical form, which can differ from `project.root` itself
    // when the root is reached through a symlink (as `std::env::temp_dir`
    // commonly is on macOS: `/var/folders/...` -> `/private/var/folders/...`).
    // Canonicalizing once here keeps every reported path's `strip_prefix`
    // against the root working.
    let watch_root = std::fs::canonicalize(&project.root).unwrap_or_else(|_| project.root.clone());
    let watch = match dependencies::start(&watch_root) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("error: cannot watch filesystem: {e}");
            return 4;
        }
    };
    // The native backend (FSEvents on macOS in particular) has a real,
    // observed startup latency: an edit made shortly after the stream is
    // created can be missed entirely even though `start` already returned
    // successfully. Rather than guessing a fixed delay (which real system
    // load can still exceed), write a private sentinel file and block until
    // the watcher itself reports it -- a genuine readiness handshake,
    // bounded so a truly broken watcher fails fast instead of hanging
    // watch forever.
    let mut leftover_events = wait_for_watcher_ready(&watch_root, &watch);

    scheduler.start_initial_build();
    let outcome = run_attempt(&project, &entry_path, options, engine, runner);
    let mut tracked = outcome.dependencies.clone();
    scheduler.build_finished(Instant::now());

    // Drain anything that had already queued up (from the readiness
    // handshake, or from an edit landing during the initial build itself)
    // before invoking `on_attempt`, now that `tracked` actually exists to
    // judge relevance against. This must never be silently discarded --
    // an earlier version of this function did exactly that during the
    // readiness wait, which lost a real edit whenever one raced with
    // watcher startup.
    while let Ok(changed) = watch.events.try_recv() {
        leftover_events.push(changed);
    }
    for changed in &leftover_events {
        if let Some(rel) = dependencies::root_relative(&watch_root, changed) {
            if dependencies::is_relevant(&rel, &tracked, &project.manifest.project.output, dependencies::cache_dir_name()) {
                scheduler.on_event(Instant::now());
            }
        }
    }

    on_attempt(&outcome.report);

    loop {
        match stop.try_recv() {
            Ok(()) => break,
            Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {}
        }

        if let Ok(changed) = watch.events.recv_timeout(Duration::from_millis(20)) {
            if let Some(rel) = dependencies::root_relative(&watch_root, &changed) {
                if dependencies::is_relevant(&rel, &tracked, &project.manifest.project.output, dependencies::cache_dir_name()) {
                    scheduler.on_event(Instant::now());
                }
            }
        }

        if scheduler.poll(Instant::now()) {
            let outcome = run_attempt(&project, &entry_path, options, engine, runner);
            on_attempt(&outcome.report);
            match outcome.report.status {
                // 21.4: on success, the tracked set is refreshed to exactly
                // what the successful build actually used (obsolete
                // dependencies are pruned); on failure or supersession, the
                // union with the last successful set is kept so watch never
                // stops observing a file a correction still needs.
                AttemptStatus::Success => tracked = outcome.dependencies,
                AttemptStatus::Failure | AttemptStatus::Superseded => {
                    tracked.merge(outcome.dependencies);
                }
            }
            scheduler.build_finished(Instant::now());
        }
    }

    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{FakeProcessRunner, ProcessOutcome};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn tempdir(label: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "terse-watch-test-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_minimal_project(root: &Path) {
        std::fs::write(
            root.join("terse.toml"),
            "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n",
        )
        .unwrap();
        std::fs::write(root.join("paper.trs"), "document:\n  title: A Paper\n\nHello.\n").unwrap();
    }

    #[test]
    fn test_watch_recovers_from_initial_error() {
        let root = tempdir("initial-error");
        std::fs::write(
            root.join("terse.toml"),
            "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n",
        )
        .unwrap();
        // A malformed reserved header: this fails to parse.
        std::fs::write(root.join("paper.trs"), "document\n  title: A Paper\n").unwrap();

        let (_tx, rx) = mpsc::channel::<()>();
        let mut runner = FakeProcessRunner::new(vec![]);
        let options = WatchOptions { theme_name: "academic".to_string(), tex_only: true, json: false };
        let (project, entry_path) = project::resolve_project(None, &root).unwrap();

        let first = run_attempt(&project, &entry_path, &options, None, &mut runner);
        assert_eq!(first.report.status, AttemptStatus::Failure);

        // Correct the source and retry manually (this test exercises
        // `run_attempt` directly rather than the real event loop, since a
        // real loop needs real OS filesystem events, covered separately).
        std::fs::write(root.join("paper.trs"), "document:\n  title: A Paper\n\nHello.\n").unwrap();
        let second = run_attempt(&project, &entry_path, &options, None, &mut runner);
        assert_eq!(second.report.status, AttemptStatus::Success);
        assert!(root.join("build/academic/paper.tex").exists());
        drop(rx);
    }

    #[test]
    fn test_watch_retains_output_on_lock_or_engine_failure() {
        let root = tempdir("retain-output");
        write_minimal_project(&root);
        let options = WatchOptions { theme_name: "academic".to_string(), tex_only: true, json: false };
        let (project, entry_path) = project::resolve_project(None, &root).unwrap();

        let mut runner = FakeProcessRunner::new(vec![]);
        let first = run_attempt(&project, &entry_path, &options, None, &mut runner);
        assert_eq!(first.report.status, AttemptStatus::Success);
        let published = std::fs::read(root.join("build/academic/paper.tex")).unwrap();

        // Break the source, forcing a failed attempt.
        std::fs::write(root.join("paper.trs"), "document\n  title: Broken\n").unwrap();
        let mut runner = FakeProcessRunner::new(vec![]);
        let second = run_attempt(&project, &entry_path, &options, None, &mut runner);
        assert_eq!(second.report.status, AttemptStatus::Failure);

        // The previously published output is untouched.
        assert_eq!(std::fs::read(root.join("build/academic/paper.tex")).unwrap(), published);
    }

    #[test]
    fn test_watch_json_reports_each_attempt() {
        let root = tempdir("json-report");
        std::fs::write(
            root.join("terse.toml"),
            "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n",
        )
        .unwrap();
        std::fs::write(root.join("paper.trs"), "document\n  title: Broken\n").unwrap();

        let options = WatchOptions { theme_name: "academic".to_string(), tex_only: true, json: true };
        let (project, entry_path) = project::resolve_project(None, &root).unwrap();
        let mut runner = FakeProcessRunner::new(vec![]);
        let outcome = run_attempt(&project, &entry_path, &options, None, &mut runner);
        assert_eq!(outcome.report.status, AttemptStatus::Failure);
        let json = outcome.report.json.expect("json mode reports structured diagnostics");
        assert!(json.contains("\"diagnostics\""));
    }

    #[test]
    fn test_missing_dependency_creation_is_detected_end_to_end() {
        let root = tempdir("missing-dep");
        std::fs::write(
            root.join("terse.toml"),
            "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n",
        )
        .unwrap();
        std::fs::write(root.join("paper.trs"), "document:\n  title: A Paper\n\ninclude \"sections/intro.trs\"\n").unwrap();

        let options = WatchOptions { theme_name: "academic".to_string(), tex_only: true, json: false };
        let (project, entry_path) = project::resolve_project(None, &root).unwrap();
        let mut runner = FakeProcessRunner::new(vec![]);
        let first = run_attempt(&project, &entry_path, &options, None, &mut runner);
        assert_eq!(first.report.status, AttemptStatus::Failure);
        assert!(first.dependencies.attempted.contains("sections/intro.trs"));

        std::fs::create_dir_all(root.join("sections")).unwrap();
        std::fs::write(root.join("sections/intro.trs"), "Intro text.\n").unwrap();
        let mut runner = FakeProcessRunner::new(vec![]);
        let second = run_attempt(&project, &entry_path, &options, None, &mut runner);
        assert_eq!(second.report.status, AttemptStatus::Success);
    }

    #[test]
    fn test_stale_snapshot_never_published() {
        let root = tempdir("stale-snapshot");
        write_minimal_project(&root);
        let options = WatchOptions { theme_name: "academic".to_string(), tex_only: true, json: false };
        let (project, entry_path) = project::resolve_project(None, &root).unwrap();
        let mut runner = FakeProcessRunner::new(vec![]);

        let paper_path = root.join("paper.trs");
        let outcome = run_attempt_with_hook(&project, &entry_path, &options, None, &mut runner, &mut || {
            // Simulate an edit landing while this attempt is still running,
            // after it already snapshotted its inputs.
            std::fs::write(&paper_path, "document:\n  title: Edited Mid-Build\n\nHello.\n").unwrap();
        });

        assert_eq!(outcome.report.status, AttemptStatus::Superseded);
        assert!(!root.join("build/academic").exists());
    }

    #[allow(dead_code)]
    fn silence_unused(_o: ProcessOutcome) {}
}
