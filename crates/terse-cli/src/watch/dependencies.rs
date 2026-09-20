//! Filesystem watching for `terse watch`: real OS events via `notify` with
//! an explicit polling fallback, plus the pure relevance/ignore logic that
//! decides whether a changed path should ever reach the scheduler.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::{Config, PollWatcher, RecommendedWatcher, RecursiveMode, Watcher};
use terse_core::project::dependencies::DependencySet;

/// True when `changed` (a root-relative, `/`-separated logical path) falls
/// under the generated output directory or the disposable cache directory.
/// Both are written by a successful build itself, so treating their own
/// events as relevant would create a rebuild loop.
pub fn is_ignored(changed: &str, output_dir: &str, cache_dir: &str) -> bool {
    let first = changed.split('/').next().unwrap_or("");
    (!output_dir.is_empty() && first == output_dir) || (!cache_dir.is_empty() && first == cache_dir)
}

/// True when a change at `changed` should trigger a rebuild: it is already
/// a tracked dependency, it is (or is inside) a path a previous attempt
/// tried and failed to find (so its creation is a repair), or it is one of
/// the fixed project-level files watch always cares about even before the
/// first successful load discovers them.
pub fn is_relevant(changed: &str, deps: &DependencySet, output_dir: &str, cache_dir: &str) -> bool {
    if is_ignored(changed, output_dir, cache_dir) {
        return false;
    }
    if changed == "terse.toml" || changed == "references.lock" || changed == "references.overrides.toml" {
        return true;
    }
    if deps.resolved.contains(changed) {
        return true;
    }
    deps.attempted
        .iter()
        .any(|p| p == changed || changed.starts_with(&format!("{p}/")) || p.starts_with(&format!("{changed}/")))
}

const CACHE_DIR_NAME: &str = ".terse-cache";

pub fn cache_dir_name() -> &'static str {
    CACHE_DIR_NAME
}

/// A live filesystem watch on `root`, recursive, reporting each changed
/// path as a root-relative logical string (`/`-separated) on `events`.
/// Prefers the platform's native watcher (`notify`'s `RecommendedWatcher`,
/// e.g. inotify/FSEvents); if that fails to initialize (for example, no
/// inotify instances available in a constrained sandbox), falls back to
/// `notify`'s polling watcher so watch mode still functions, only less
/// promptly.
pub struct FsWatch {
    _watcher: Box<dyn Watcher + Send>,
    pub events: mpsc::Receiver<PathBuf>,
    pub backend: WatchBackend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchBackend {
    Native,
    Polling,
}

pub fn start(root: &Path) -> Result<FsWatch, String> {
    let (tx, rx) = mpsc::channel();

    let forward = move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            for path in event.paths {
                let _ = tx.send(path);
            }
        }
    };

    match RecommendedWatcher::new(forward.clone(), Config::default()) {
        Ok(mut watcher) => {
            watcher
                .watch(root, RecursiveMode::Recursive)
                .map_err(|e| e.to_string())?;
            Ok(FsWatch { _watcher: Box::new(watcher), events: rx, backend: WatchBackend::Native })
        }
        Err(_) => {
            // `compare_contents` is required for reliable detection here:
            // mtime-only comparison (the default) was observed to miss a
            // real edit outright when a file is rewritten quickly relative
            // to this filesystem's mtime granularity -- content comparison
            // catches it regardless of timestamp resolution, at the cost of
            // reading every watched file on each poll tick (acceptable for
            // a project-sized source tree, and this fallback only runs at
            // all when the native OS backend is unavailable).
            let mut watcher = PollWatcher::new(
                forward,
                Config::default()
                    .with_poll_interval(Duration::from_millis(100))
                    .with_compare_contents(true),
            )
            .map_err(|e| e.to_string())?;
            watcher
                .watch(root, RecursiveMode::Recursive)
                .map_err(|e| e.to_string())?;
            Ok(FsWatch { _watcher: Box::new(watcher), events: rx, backend: WatchBackend::Polling })
        }
    }
}

/// Converts an absolute path reported by the watcher into the root-relative
/// logical form [`DependencySet`]/[`is_relevant`] use, or `None` if it falls
/// outside `root` (should not normally happen for a watch rooted at
/// `root`, but a watcher may occasionally report paths for the root
/// directory itself).
pub fn root_relative(root: &Path, changed: &Path) -> Option<String> {
    let rel = changed.strip_prefix(root).ok()?;
    if rel.as_os_str().is_empty() {
        return None;
    }
    let joined: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    Some(joined.join("/"))
}

/// Every parent directory (root-relative) implied by a set of dependencies,
/// useful only for diagnostics/tests that want to describe what a watch
/// session is actually covering; the live watch itself covers the whole
/// project root recursively rather than granular per-file watches, since a
/// single recursive watch is what actually observes atomic-save
/// replacement (rename into place) and newly created files without extra
/// bookkeeping.
pub fn implied_directories(deps: &DependencySet) -> BTreeSet<String> {
    let mut dirs = BTreeSet::new();
    for p in deps.resolved.iter().chain(deps.attempted.iter()) {
        if let Some(idx) = p.rfind('/') {
            dirs.insert(p[..idx].to_string());
        } else {
            dirs.insert(String::new());
        }
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deps_with(resolved: &[&str], attempted: &[&str]) -> DependencySet {
        let mut d = DependencySet::new();
        for r in resolved {
            d.add_resolved(*r);
        }
        for a in attempted {
            d.add_attempted(*a);
        }
        d
    }

    #[test]
    fn test_output_and_cache_events_are_ignored() {
        assert!(is_ignored("build/academic/paper.tex", "build", ".terse-cache"));
        assert!(is_ignored(".terse-cache/abcd.bin", "build", ".terse-cache"));
        assert!(!is_ignored("paper.trs", "build", ".terse-cache"));

        let deps = deps_with(&["paper.trs"], &[]);
        assert!(!is_relevant("build/academic/paper.tex", &deps, "build", ".terse-cache"));
        assert!(!is_relevant(".terse-cache/x.bin", &deps, "build", ".terse-cache"));
    }

    #[test]
    fn test_resolved_dependency_change_is_relevant() {
        let deps = deps_with(&["paper.trs", "sections/intro.trs"], &[]);
        assert!(is_relevant("paper.trs", &deps, "build", ".terse-cache"));
        assert!(is_relevant("sections/intro.trs", &deps, "build", ".terse-cache"));
        assert!(!is_relevant("sections/unrelated.trs", &deps, "build", ".terse-cache"));
    }

    #[test]
    fn test_missing_dependency_creation_is_detected() {
        let deps = deps_with(&[], &["sections/missing.trs"]);
        assert!(is_relevant("sections/missing.trs", &deps, "build", ".terse-cache"));
        // Creating the parent directory itself is also a relevant repair.
        assert!(is_relevant("sections", &deps, "build", ".terse-cache"));
    }

    #[test]
    fn test_manifest_and_lock_are_always_relevant() {
        let deps = DependencySet::new();
        assert!(is_relevant("terse.toml", &deps, "build", ".terse-cache"));
        assert!(is_relevant("references.lock", &deps, "build", ".terse-cache"));
        assert!(is_relevant("references.overrides.toml", &deps, "build", ".terse-cache"));
    }
}
