//! Pure per-OS locations for the managed toolchain and the Terse-owned
//! TeX user tree. Nothing here touches the filesystem.

use std::path::PathBuf;

use super::host::{HostEnv, HostOs};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TexmfDirs {
    pub home: PathBuf,
    pub var: PathBuf,
    pub config: PathBuf,
}

impl TexmfDirs {
    pub fn under(base: &std::path::Path) -> Self {
        Self {
            home: base.join("home"),
            var: base.join("var"),
            config: base.join("config"),
        }
    }
}

/// The user-scoped data directory Terse owns: `$XDG_DATA_HOME/terse` or
/// `~/.local/share/terse` on Linux and other Unix systems, `~/Library/terse`
/// on macOS, `%LOCALAPPDATA%\terse` on Windows. `None` when the host gives
/// no base at all.
///
/// macOS deliberately avoids `~/Library/Application Support`: XeTeX hands
/// its output driver path (derived from the binary's own location) to
/// `sh`, and a space in that path breaks every PDF with
/// `sh: .../Application: No such file or directory`. A managed prefix
/// must therefore never contain a space; `~/Library/terse` never does.
pub fn data_dir(host: &HostEnv) -> Option<PathBuf> {
    match host.os {
        HostOs::Windows => host
            .local_app_data
            .clone()
            .or_else(|| host.userprofile.as_ref().map(|p| p.join("AppData").join("Local")))
            .map(|p| p.join("terse")),
        HostOs::MacOs => host.home.as_ref().map(|h| h.join("Library").join("terse")),
        HostOs::Linux | HostOs::Other => host
            .xdg_data_home
            .clone()
            .or_else(|| host.home.as_ref().map(|h| h.join(".local").join("share")))
            .map(|p| p.join("terse")),
    }
}

pub fn managed_prefix(host: &HostEnv, year: &str) -> Option<PathBuf> {
    data_dir(host).map(|d| d.join("toolchain").join(format!("texlive-{year}")))
}

/// The persistent Terse-owned `TEXMFHOME`/`TEXMFVAR`/`TEXMFCONFIG` trees
/// used for ordinary builds. Falls back to the host temp directory when
/// no data directory exists, so a child process never inherits the user's
/// own TeX tree.
pub fn texmf_dirs(host: &HostEnv) -> TexmfDirs {
    let base = data_dir(host)
        .map(|d| d.join("texmf"))
        .unwrap_or_else(|| host.temp_dir().join("terse-texmf"));
    TexmfDirs::under(&base)
}
