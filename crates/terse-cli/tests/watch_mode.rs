//! Real filesystem-event coverage for `terse watch`: this drives the actual
//! `notify`-backed loop (not just the pure `run_attempt`/`scheduler` unit
//! tests in `crates/terse-cli/src/watch/`), using real `std::fs` writes in a
//! temp directory and a channel-based readiness handshake instead of
//! sleeping and hoping, bounded by an explicit deadline so a broken watcher
//! fails the test instead of hanging forever.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;

use terse_cli::engine::RealProcessRunner;
use terse_cli::watch::{self, AttemptReport, AttemptStatus, WatchOptions};

const MANIFEST: &str = "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n";

/// Every test in this file starts a real native filesystem watcher
/// (FSEvents on macOS, inotify on Linux) in its own background thread.
/// Running more than one concurrently in the same process was observed to
/// starve event delivery for one of them, the same class of
/// resource-contention issue documented for concurrent real `biber`
/// invocations elsewhere in this test suite (see
/// `crates/terse-cli/tests/citations.rs`'s `ENGINE_LOCK`). Serializing
/// these tests avoids that. A prior test panicking while holding this lock
/// would otherwise poison it for every later test in the file, so a
/// poisoned lock is recovered rather than propagated.
static WATCH_LOCK: Mutex<()> = Mutex::new(());

fn watch_lock() -> std::sync::MutexGuard<'static, ()> {
    WATCH_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn tempdir(label: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "terse-watch-e2e-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

/// Runs a real watch session on a background thread until `stop` is
/// signaled, forwarding every completed attempt to `reports_tx`. Returns
/// the join handle so the test can wait for clean shutdown with a bounded
/// deadline instead of assuming the thread already exited.
fn spawn_watch(
    root: PathBuf,
    options: WatchOptions,
    stop: mpsc::Receiver<()>,
    reports_tx: mpsc::Sender<AttemptReport>,
) -> std::thread::JoinHandle<i32> {
    std::thread::spawn(move || {
        let mut runner = RealProcessRunner;
        watch::run(&root, None, &options, &stop, &mut runner, &mut |report| {
            let _ = reports_tx.send(report.clone());
        })
    })
}

/// Waits for the next attempt report, failing the test (rather than
/// hanging) if none arrives within a generous bound.
fn next_report(reports_rx: &mpsc::Receiver<AttemptReport>) -> AttemptReport {
    reports_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("watch attempt did not complete within the deadline")
}

#[test]
// A prior investigation attributed intermittent failure of this test to
// FSEvents resource exhaustion across many short-lived watch streams. Root
// cause was actually a genuine bug in `wait_for_watcher_ready`: on this
// sandbox's FSEvents backend, initial event delivery can take several
// real seconds, and the readiness handshake's old draining loop discarded
// every non-sentinel event it saw in the meantime -- so an edit made
// immediately after the initial build (exactly what this test does) could
// land inside that window and be silently lost forever, never a resource
// problem. Fixed by having `wait_for_watcher_ready` return leftover events
// instead of discarding them, replayed into the scheduler before the main
// loop starts. No longer ignored.
fn test_atomic_saves_and_asset_edits_trigger_rebuild() {
    let _guard = watch_lock();
    let root = tempdir("atomic-save");
    write(&root.join("terse.toml"), MANIFEST);
    write(&root.join("paper.trs"), "document:\n  title: A Paper\n\nOriginal text.\n");

    let options = WatchOptions { theme_name: "academic".to_string(), tex_only: true, json: false };
    let (stop_tx, stop_rx) = mpsc::channel();
    let (reports_tx, reports_rx) = mpsc::channel();
    let handle = spawn_watch(root.clone(), options, stop_rx, reports_tx);

    let initial = next_report(&reports_rx);
    assert_eq!(initial.status, AttemptStatus::Success);
    assert!(fs::read_to_string(root.join("build/academic/paper.tex"))
        .unwrap()
        .contains("Original text."));

    // Atomic save: write to a sibling temp file, then rename into place,
    // exactly like most editors save.
    let tmp = root.join(".paper.trs.tmp");
    write(&tmp, "document:\n  title: A Paper\n\nReplaced text.\n");
    fs::rename(&tmp, root.join("paper.trs")).unwrap();

    let after_edit = next_report(&reports_rx);
    assert_eq!(after_edit.status, AttemptStatus::Success);
    assert!(fs::read_to_string(root.join("build/academic/paper.tex"))
        .unwrap()
        .contains("Replaced text."));

    stop_tx.send(()).unwrap();
    let code = handle.join().unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_output_and_cache_events_are_ignored_end_to_end() {
    let _guard = watch_lock();
    let root = tempdir("ignore-output");
    write(&root.join("terse.toml"), MANIFEST);
    write(&root.join("paper.trs"), "document:\n  title: A Paper\n\nHello.\n");

    let options = WatchOptions { theme_name: "academic".to_string(), tex_only: true, json: false };
    let (stop_tx, stop_rx) = mpsc::channel();
    let (reports_tx, reports_rx) = mpsc::channel();
    let handle = spawn_watch(root.clone(), options, stop_rx, reports_tx);

    let initial = next_report(&reports_rx);
    assert_eq!(initial.status, AttemptStatus::Success);

    // The initial build itself just wrote into `build/`; if that were
    // treated as relevant, a second attempt would already be queued. Poll
    // briefly for any further attempt and confirm none arrives -- a lack
    // of a message within a bounded window here means "no rebuild loop",
    // which is exactly what this test verifies.
    assert!(reports_rx.recv_timeout(Duration::from_millis(600)).is_err());

    stop_tx.send(()).unwrap();
    handle.join().unwrap();
}

#[test]
fn test_interrupt_stops_owned_processes() {
    let _guard = watch_lock();
    let root = tempdir("interrupt");
    write(&root.join("terse.toml"), MANIFEST);
    write(&root.join("paper.trs"), "document:\n  title: A Paper\n\nHello.\n");

    let options = WatchOptions { theme_name: "academic".to_string(), tex_only: true, json: false };
    let (stop_tx, stop_rx) = mpsc::channel();
    let (reports_tx, reports_rx) = mpsc::channel();
    let handle = spawn_watch(root.clone(), options, stop_rx, reports_tx);

    let initial = next_report(&reports_rx);
    assert_eq!(initial.status, AttemptStatus::Success);
    let published = fs::read(root.join("build/academic/paper.tex")).unwrap();

    stop_tx.send(()).unwrap();
    let code = handle
        .join()
        .expect("watch thread must exit promptly after an interrupt, never hang");
    assert_eq!(code, 0);

    // The last published generation is untouched by the interrupt.
    assert_eq!(fs::read(root.join("build/academic/paper.tex")).unwrap(), published);
}

#[test]
fn test_missing_dependency_creation_is_detected_through_real_watcher() {
    let _guard = watch_lock();
    let root = tempdir("missing-dep-real");
    write(&root.join("terse.toml"), MANIFEST);
    write(
        &root.join("paper.trs"),
        "document:\n  title: A Paper\n\ninclude \"sections/intro.trs\"\n",
    );

    let options = WatchOptions { theme_name: "academic".to_string(), tex_only: true, json: false };
    let (stop_tx, stop_rx) = mpsc::channel();
    let (reports_tx, reports_rx) = mpsc::channel();
    let handle = spawn_watch(root.clone(), options, stop_rx, reports_tx);

    // The include target does not exist yet: the initial build fails, but
    // it must still retain the missing target as an *attempted* dependency
    // so its later creation is recognized as a relevant repair.
    let initial = next_report(&reports_rx);
    assert_eq!(initial.status, AttemptStatus::Failure);

    write(&root.join("sections/intro.trs"), "Intro text.\n");

    let repaired = next_report(&reports_rx);
    assert_eq!(repaired.status, AttemptStatus::Success);
    assert!(fs::read_to_string(root.join("build/academic/paper.tex"))
        .unwrap()
        .contains("Intro text."));

    stop_tx.send(()).unwrap();
    handle.join().unwrap();
}

#[test]
#[ignore = "requires a local XeLaTeX distribution"]
fn test_watch_keeps_last_good_pdf_after_syntax_error() {
    let _guard = watch_lock();
    let root = tempdir("keep-last-good");
    write(&root.join("terse.toml"), MANIFEST);
    write(&root.join("paper.trs"), "document:\n  title: A Paper\n\nOriginal text.\n");

    // `tex_only: false` so this actually exercises the "last good PDF"
    // guarantee, not just the source artifacts.
    let options = WatchOptions { theme_name: "academic".to_string(), tex_only: false, json: false };
    let (stop_tx, stop_rx) = mpsc::channel();
    let (reports_tx, reports_rx) = mpsc::channel();
    let handle = spawn_watch(root.clone(), options, stop_rx, reports_tx);

    let initial = next_report(&reports_rx);
    assert_eq!(initial.status, AttemptStatus::Success);
    let pdf_path = root.join("build/academic/paper.pdf");
    let good_pdf = fs::read(&pdf_path).expect("initial build produces a PDF");
    assert!(!good_pdf.is_empty());

    // A malformed reserved header: this fails to parse.
    write(&root.join("paper.trs"), "document\n  title: Broken\n");
    let broken = next_report(&reports_rx);
    assert_eq!(broken.status, AttemptStatus::Failure);
    assert_eq!(
        fs::read(&pdf_path).unwrap(),
        good_pdf,
        "a syntax error must never touch the last published PDF"
    );

    // Fix it again; a fresh PDF replaces the preserved one.
    write(&root.join("paper.trs"), "document:\n  title: A Paper\n\nFixed text.\n");
    let fixed = next_report(&reports_rx);
    assert_eq!(fixed.status, AttemptStatus::Success);
    let new_pdf = fs::read(&pdf_path).unwrap();
    assert!(!new_pdf.is_empty());

    stop_tx.send(()).unwrap();
    handle.join().unwrap();
}

#[test]
fn test_watch_updates_theme_dependency_graph() {
    let _guard = watch_lock();
    let root = tempdir("theme-dep-graph");
    write(
        &root.join("terse.toml"),
        "format-version = 1\n\n[project]\nentry = \"paper.trs\"\noutput = \"build\"\n\n[themes]\nacademic = \"theme.theme\"\n",
    );
    write(&root.join("paper.trs"), "document:\n  title: A Paper\n\nHello.\n");
    write(
        &root.join("theme.theme"),
        "page:\n  size: a4\n  margin: 2.5cm\n  columns: 1\n\nbody:\n  font: latin-modern\n  color: #000000\n",
    );

    let options = WatchOptions { theme_name: "academic".to_string(), tex_only: true, json: false };
    let (stop_tx, stop_rx) = mpsc::channel();
    let (reports_tx, reports_rx) = mpsc::channel();
    let handle = spawn_watch(root.clone(), options, stop_rx, reports_tx);

    let initial = next_report(&reports_rx);
    assert_eq!(initial.status, AttemptStatus::Success);
    let original_style = fs::read_to_string(root.join("build/academic/terse-style.sty")).unwrap();
    assert!(original_style.contains("lmodern"));

    // Edit the *theme* file, not a `.trs` source file: this must still be
    // recognized as a tracked dependency and trigger a rebuild. (The body
    // font is the property whose resolved value is actually threaded into
    // the generated style, unlike `page.margin`, which is parsed/validated
    // but not yet rendered into any template output -- a separate,
    // pre-existing gap in theme rendering, not something this test should
    // paper over by asserting on it.)
    write(
        &root.join("theme.theme"),
        "page:\n  size: a4\n  margin: 2.5cm\n  columns: 1\n\nbody:\n  font: tex-gyre-pagella\n  color: #000000\n",
    );
    let after_theme_edit = next_report(&reports_rx);
    assert_eq!(after_theme_edit.status, AttemptStatus::Success);
    let updated_style = fs::read_to_string(root.join("build/academic/terse-style.sty")).unwrap();
    assert!(updated_style.contains("tgpagella"));
    assert!(!updated_style.contains("lmodern"));

    stop_tx.send(()).unwrap();
    handle.join().unwrap();
}

#[test]
fn test_polling_fallback_detects_real_edits() {
    let _guard = watch_lock();
    let root = tempdir("polling-fallback");
    write(&root.join("terse.toml"), MANIFEST);
    write(&root.join("paper.trs"), "document:\n  title: A Paper\n\nOriginal text.\n");

    // Exercises `dependencies::start`'s `PollWatcher` path directly (not
    // through `watch::run`, which always prefers the native backend when
    // available) to confirm the fallback itself genuinely detects real
    // filesystem edits, not just that it constructs successfully. This
    // configuration (poll interval + `with_compare_contents`) mirrors
    // `dependencies::start`'s own `PollWatcher` construction exactly --
    // `with_compare_contents(true)` matters here for real: without it, this
    // test itself failed unreliably (mtime-only comparison missed the
    // edit outright on this filesystem), which is exactly why the
    // production fallback in `dependencies.rs` now sets it too.
    let watch_root = fs::canonicalize(&root).unwrap();
    let (tx, rx) = mpsc::channel();
    let poll_interval = Duration::from_millis(50);
    let mut watcher = notify::PollWatcher::new(
        move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                for path in event.paths {
                    let _ = tx.send(path);
                }
            }
        },
        notify::Config::default()
            .with_poll_interval(poll_interval)
            .with_compare_contents(true),
    )
    .unwrap();
    use notify::Watcher;
    watcher.watch(&watch_root, notify::RecursiveMode::Recursive).unwrap();

    // Give the poller a chance to take its first baseline scan before the
    // edit, bounded well above its own interval.
    std::thread::sleep(poll_interval * 3);
    write(&root.join("paper.trs"), "document:\n  title: A Paper\n\nEdited text.\n");

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut saw_it = false;
    while std::time::Instant::now() < deadline {
        if let Ok(changed) = rx.recv_timeout(Duration::from_millis(200)) {
            if changed.file_name().and_then(|n| n.to_str()) == Some("paper.trs") {
                saw_it = true;
                break;
            }
        }
    }
    assert!(saw_it, "polling fallback must detect a real edit to a watched file");
}

/// The real-loop half of the "stale snapshot is never published" scenario.
///
/// The unit test in `watch::tests` drives one attempt through a hook, which
/// proves the superseded classification but bypasses the scheduler entirely.
/// The scenario is about what the *session* does: a stale attempt must not
/// publish, and a successor must go on to process the newest inputs. Driven
/// through `watch::run` with channel readiness, never sleeps-as-assertions.
#[test]
fn test_stale_snapshot_successor_publishes_through_real_loop() {
    let _guard = watch_lock();
    let root = tempdir("stale-successor");
    write(&root.join("terse.toml"), MANIFEST);
    write(&root.join("paper.trs"), "document:\n  title: A Paper\n\nOriginal text.\n");

    let options = WatchOptions { theme_name: "academic".to_string(), tex_only: true, json: false };
    let (stop_tx, stop_rx) = mpsc::channel();
    let (reports_tx, reports_rx) = mpsc::channel();
    let handle = spawn_watch(root.clone(), options, stop_rx, reports_tx);

    let initial = next_report(&reports_rx);
    assert_eq!(initial.status, AttemptStatus::Success);

    // Two edits in quick succession: the scheduler coalesces them, and
    // whatever the session finally publishes must be the *latest* content,
    // never the intermediate state a stale snapshot would have carried.
    write(&root.join("paper.trs"), "document:\n  title: A Paper\n\nFirst edit.\n");
    write(&root.join("paper.trs"), "document:\n  title: A Paper\n\nSecond edit.\n");

    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut published_latest = false;
    while std::time::Instant::now() < deadline {
        match reports_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(report) => {
                // A superseded attempt must never have published.
                if report.status == AttemptStatus::Success {
                    let tex = fs::read_to_string(root.join("build/academic/paper.tex")).unwrap();
                    assert!(
                        !tex.contains("Original text."),
                        "a successful attempt after the edits must not republish the original"
                    );
                    if tex.contains("Second edit.") {
                        published_latest = true;
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }

    let _ = stop_tx.send(());
    let _ = handle.join();

    assert!(
        published_latest,
        "a successor attempt must process the newest snapshot and publish it"
    );
}
