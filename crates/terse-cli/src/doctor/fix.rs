//! The enumerated allowlist of repairs `--fix` may apply: removing a
//! stale Biber PAR cache and clearing macOS quarantine from managed
//! binaries. Nothing else is ever written or deleted.

use std::path::Path;

use terse_core::toolchain::par::par_cache_dir;

use crate::engine::{ProcessInvocation, ProcessRunner};
use crate::toolchain::probe::PROBE_TIMEOUT;
use crate::toolchain::HostEnv;

/// Deletes `<tmpdir>/par-<hex(username)>` when it exists. Returns the
/// action line for the report, or `None` when there was nothing to do.
pub fn remove_stale_par_cache(host: &HostEnv) -> Option<String> {
    let user = host.username.as_deref()?;
    let dir = par_cache_dir(&host.temp_dir(), user);
    if !dir.is_dir() {
        return None;
    }
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => Some(format!("removed stale biber PAR cache {}", dir.display())),
        Err(e) => Some(format!("could not remove {}: {e}", dir.display())),
    }
}

pub fn is_quarantined(
    runner: &mut dyn ProcessRunner,
    xattr: &Path,
    binary: &Path,
    working_dir: &Path,
    env: &[(String, String)],
) -> bool {
    let inv = ProcessInvocation {
        program: xattr.to_path_buf(),
        args: vec!["-p".to_string(), "com.apple.quarantine".to_string(), binary.display().to_string()],
        working_dir: working_dir.to_path_buf(),
        env: env.to_vec(),
    };
    let out = runner.run(&inv, PROBE_TIMEOUT);
    out.started && !out.timed_out && out.status_code == Some(0)
}

pub fn clear_quarantine(
    runner: &mut dyn ProcessRunner,
    xattr: &Path,
    binary: &Path,
    working_dir: &Path,
    env: &[(String, String)],
) -> String {
    let inv = ProcessInvocation {
        program: xattr.to_path_buf(),
        args: vec!["-d".to_string(), "com.apple.quarantine".to_string(), binary.display().to_string()],
        working_dir: working_dir.to_path_buf(),
        env: env.to_vec(),
    };
    let out = runner.run(&inv, PROBE_TIMEOUT);
    if out.started && !out.timed_out && out.status_code == Some(0) {
        format!("cleared com.apple.quarantine on {}", binary.display())
    } else {
        format!("could not clear com.apple.quarantine on {}", binary.display())
    }
}
