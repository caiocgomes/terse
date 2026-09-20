//! The one place a child process environment is assembled. The runner
//! applies exactly this list on top of a cleared environment; nothing is
//! inherited implicitly, so `TEXINPUTS`/`BIBINPUTS`/`TEXMFCNF` and every
//! other `TEXMF*` override a shell may carry never reach a build.

use std::path::PathBuf;

use super::host::HostEnv;
use super::paths::TexmfDirs;
use super::ResolvedToolchain;

/// Variables passed through from the host when present. Everything else
/// is dropped.
pub const INHERITED_ALLOWLIST: &[&str] = &["HOME", "USERPROFILE", "TMPDIR", "TEMP", "TMP", "SYSTEMROOT"];

pub fn prepare_child_env(tc: &ResolvedToolchain, host: &HostEnv, texmf: &TexmfDirs) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = Vec::new();

    let path_value = match &tc.bin_dir {
        Some(bin) => {
            let mut dirs: Vec<PathBuf> = vec![bin.clone()];
            dirs.extend(host.path_dirs());
            std::env::join_paths(dirs)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| host.path.to_string_lossy().into_owned())
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
    env.push(("TEXMFHOME".to_string(), texmf.home.to_string_lossy().into_owned()));
    env.push(("TEXMFVAR".to_string(), texmf.var.to_string_lossy().into_owned()));
    env.push(("TEXMFCONFIG".to_string(), texmf.config.to_string_lossy().into_owned()));
    env
}

/// Every key `prepare_child_env` can ever emit.
pub const EMITTED_KEYS: &[&str] = &[
    "PATH",
    "HOME",
    "USERPROFILE",
    "TMPDIR",
    "TEMP",
    "TMP",
    "SYSTEMROOT",
    "LANG",
    "TEXMFHOME",
    "TEXMFVAR",
    "TEXMFCONFIG",
];
