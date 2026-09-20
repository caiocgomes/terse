//! The host environment as data. `HostEnv::capture()` is the only place in
//! the toolchain code that reads `std::env`; every consumer takes a
//! `&HostEnv`, so tests build literals and never touch the process
//! environment.

use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOs {
    Linux,
    MacOs,
    Windows,
    Other,
}

impl HostOs {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            HostOs::Windows
        } else if cfg!(target_os = "macos") {
            HostOs::MacOs
        } else if cfg!(target_os = "linux") {
            HostOs::Linux
        } else {
            HostOs::Other
        }
    }

    pub fn is_windows(self) -> bool {
        self == HostOs::Windows
    }
}

pub const DEFAULT_PATHEXT: &[&str] = &[".EXE", ".CMD", ".BAT"];

#[derive(Debug, Clone)]
pub struct HostEnv {
    pub os: HostOs,
    pub path: OsString,
    pub pathext: Vec<String>,
    pub home: Option<PathBuf>,
    pub userprofile: Option<PathBuf>,
    pub tmpdir: Option<String>,
    pub temp: Option<String>,
    pub tmp: Option<String>,
    pub systemroot: Option<String>,
    pub username: Option<String>,
    pub local_app_data: Option<PathBuf>,
    pub xdg_data_home: Option<PathBuf>,
}

impl HostEnv {
    pub fn capture() -> Self {
        let pathext = std::env::var("PATHEXT")
            .ok()
            .map(|v| v.split(';').filter(|s| !s.is_empty()).map(str::to_string).collect::<Vec<_>>())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_PATHEXT.iter().map(|s| s.to_string()).collect());
        Self {
            os: HostOs::current(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            pathext,
            home: std::env::var_os("HOME").map(PathBuf::from),
            userprofile: std::env::var_os("USERPROFILE").map(PathBuf::from),
            tmpdir: std::env::var("TMPDIR").ok(),
            temp: std::env::var("TEMP").ok(),
            tmp: std::env::var("TMP").ok(),
            systemroot: std::env::var("SYSTEMROOT").ok(),
            username: std::env::var("USER")
                .ok()
                .or_else(|| std::env::var("USERNAME").ok())
                .or_else(|| std::env::var("LOGNAME").ok()),
            local_app_data: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
            xdg_data_home: std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
        }
    }

    /// A literal with only an OS and a search path; every other field is
    /// absent. Tests fill in what a case needs.
    pub fn minimal(os: HostOs, path: impl Into<OsString>) -> Self {
        Self {
            os,
            path: path.into(),
            pathext: DEFAULT_PATHEXT.iter().map(|s| s.to_string()).collect(),
            home: None,
            userprofile: None,
            tmpdir: None,
            temp: None,
            tmp: None,
            systemroot: None,
            username: None,
            local_app_data: None,
            xdg_data_home: None,
        }
    }

    /// The scratch directory child processes are told about. Mirrors what
    /// `std::env::temp_dir` would choose from the same variables, without
    /// reading the process environment again.
    pub fn temp_dir(&self) -> PathBuf {
        if let Some(t) = &self.tmpdir {
            return PathBuf::from(t);
        }
        if self.os.is_windows() {
            if let Some(t) = self.temp.as_ref().or(self.tmp.as_ref()) {
                return PathBuf::from(t);
            }
        }
        std::env::temp_dir()
    }

    pub fn path_dirs(&self) -> Vec<PathBuf> {
        std::env::split_paths(&self.path).collect()
    }
}
