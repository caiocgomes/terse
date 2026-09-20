//! Locating the external TeX toolchain: one documented precedence shared
//! by every command that may start a TeX process, platform-correct
//! executable discovery that never runs anything, and the prepared child
//! environment. Tool location never affects generated text artifacts.

pub mod archive;
pub mod commands;
pub mod download;
pub mod env;
pub mod host;
pub mod install;
pub mod lock;
pub mod paths;
pub mod probe;
pub mod tlmgr;

use std::path::{Path, PathBuf};

pub use env::prepare_child_env;
pub use host::{HostEnv, HostOs};
pub use lock::ToolchainLock;
pub use paths::TexmfDirs;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolchainSelector {
    Auto,
    System,
    Managed,
    Dir(PathBuf),
}

impl ToolchainSelector {
    /// `--toolchain <auto|system|managed|DIR>`: anything that is not one of
    /// the three keywords is a directory, resolved against `base`.
    pub fn parse_flag(value: &str, base: &Path) -> Self {
        match value {
            "auto" => ToolchainSelector::Auto,
            "system" => ToolchainSelector::System,
            "managed" => ToolchainSelector::Managed,
            other => ToolchainSelector::Dir(absolutize(Path::new(other), base)),
        }
    }

    /// `[latex] toolchain`: the three keywords, or a directory that is
    /// absolute or starts with `./`/`../`; a bare word is a configuration
    /// error, since it is neither a keyword nor unmistakably a path.
    pub fn parse_manifest(value: &str, root: &Path) -> Result<Self, ToolchainError> {
        match value {
            "auto" => Ok(ToolchainSelector::Auto),
            "system" => Ok(ToolchainSelector::System),
            "managed" => Ok(ToolchainSelector::Managed),
            other => {
                let p = Path::new(other);
                let explicit = p.is_absolute() || other.starts_with("./") || other.starts_with("../");
                if explicit {
                    Ok(ToolchainSelector::Dir(absolutize(p, root)))
                } else {
                    Err(ToolchainError::InvalidManifestValue(other.to_string()))
                }
            }
        }
    }
}

fn absolutize(p: &Path, base: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolchainSource {
    System,
    Managed { prefix: PathBuf },
    Explicit { dir: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedToolchain {
    pub source: ToolchainSource,
    pub reason: String,
    pub bin_dir: Option<PathBuf>,
    pub xelatex: Option<PathBuf>,
    pub biber: Option<PathBuf>,
    pub kpsewhich: Option<PathBuf>,
    pub lock: Option<ToolchainLock>,
}

impl ResolvedToolchain {
    pub fn source_label(&self) -> &'static str {
        match self.source {
            ToolchainSource::System => "system",
            ToolchainSource::Managed { .. } => "managed",
            ToolchainSource::Explicit { .. } => "explicit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolchainError {
    /// An explicit directory selection without `xelatex` inside it.
    NotAToolchain(PathBuf),
    /// `managed` was selected but no prefix is installed.
    ManagedNotInstalled(PathBuf),
    /// `managed` was selected but the installed lock names another year.
    ManagedYearMismatch { prefix: PathBuf, lock_year: String, profile_year: String },
    /// `managed` was selected but the host has no data directory at all.
    NoDataDirectory,
    /// `[latex] toolchain` holds neither a keyword nor an explicit path.
    InvalidManifestValue(String),
}

/// Every code toolchain resolution can emit (see [`ToolchainError::code`]);
/// `E-CONFIG-009` belongs to the configuration family and is listed here
/// only so the code-family test sees the complete resolution surface.
pub const RESOLUTION_CODES: &[&str] = &["E-TOOL-013", "E-TOOL-015", "E-TOOL-016", "E-CONFIG-009"];

impl ToolchainError {
    pub fn code(&self) -> &'static str {
        match self {
            ToolchainError::NotAToolchain(_) => "E-TOOL-013",
            ToolchainError::ManagedNotInstalled(_) | ToolchainError::NoDataDirectory => "E-TOOL-015",
            ToolchainError::ManagedYearMismatch { .. } => "E-TOOL-016",
            ToolchainError::InvalidManifestValue(_) => "E-CONFIG-009",
        }
    }

    pub fn message(&self) -> String {
        match self {
            ToolchainError::NotAToolchain(dir) => format!(
                "'{}' is not a TeX toolchain directory (no xelatex executable found in it or its bin/ subdirectories)",
                dir.display()
            ),
            ToolchainError::ManagedNotInstalled(prefix) => format!(
                "no managed toolchain is installed at '{}'; run `terse toolchain install` or select --toolchain system",
                prefix.display()
            ),
            ToolchainError::NoDataDirectory => {
                "no user data directory is available for a managed toolchain; select --toolchain system or a directory".to_string()
            }
            ToolchainError::ManagedYearMismatch { prefix, lock_year, profile_year } => format!(
                "the managed toolchain at '{}' is TeX Live {lock_year} but the profile requires {profile_year}; run `terse toolchain install` or select --toolchain system",
                prefix.display()
            ),
            ToolchainError::InvalidManifestValue(v) => format!(
                "[latex] toolchain must be 'auto', 'system', 'managed', or an explicit directory path (absolute, ./ or ../); found '{v}'"
            ),
        }
    }

    /// Every resolution failure is a configuration error.
    pub const EXIT_CODE: i32 = 2;
}

/// Combines the flag and the manifest into the effective selector:
/// flag, then `[latex] toolchain`, then `auto`.
pub fn select(
    flag: Option<&ToolchainSelector>,
    manifest_value: Option<&str>,
    root: &Path,
) -> Result<ToolchainSelector, ToolchainError> {
    if let Some(f) = flag {
        return Ok(f.clone());
    }
    match manifest_value {
        Some(v) => ToolchainSelector::parse_manifest(v, root),
        None => Ok(ToolchainSelector::Auto),
    }
}

/// Resolves the selector against the host: `auto` prefers an installed
/// managed prefix whose lock year equals `profile_year`, otherwise `PATH`.
pub fn resolve(
    selector: &ToolchainSelector,
    profile_year: &str,
    host: &HostEnv,
) -> Result<ResolvedToolchain, ToolchainError> {
    match selector {
        ToolchainSelector::System => Ok(from_path(host, "selected explicitly (system)")),
        ToolchainSelector::Dir(dir) => match from_prefix(dir, host) {
            Some(tc) => Ok(ResolvedToolchain {
                source: ToolchainSource::Explicit { dir: dir.clone() },
                reason: format!("selected explicitly ({})", dir.display()),
                ..tc
            }),
            None => Err(ToolchainError::NotAToolchain(dir.clone())),
        },
        ToolchainSelector::Managed => {
            let prefix = paths::managed_prefix(host, profile_year).ok_or(ToolchainError::NoDataDirectory)?;
            let Some(tc) = from_prefix(&prefix, host) else {
                return Err(ToolchainError::ManagedNotInstalled(prefix));
            };
            match &tc.lock {
                Some(lock) if lock.texlive_year == profile_year => Ok(ResolvedToolchain {
                    reason: format!("selected explicitly (managed, TeX Live {profile_year})"),
                    ..tc
                }),
                Some(lock) => Err(ToolchainError::ManagedYearMismatch {
                    prefix,
                    lock_year: lock.texlive_year.clone(),
                    profile_year: profile_year.to_string(),
                }),
                None => Err(ToolchainError::ManagedNotInstalled(prefix)),
            }
        }
        ToolchainSelector::Auto => {
            let Some(prefix) = paths::managed_prefix(host, profile_year) else {
                return Ok(from_path(host, "no managed toolchain data directory; using PATH"));
            };
            match from_prefix(&prefix, host) {
                Some(tc) => match &tc.lock {
                    Some(lock) if lock.texlive_year == profile_year => Ok(ResolvedToolchain {
                        reason: format!(
                            "managed toolchain at '{}' matches the profile year {profile_year}",
                            prefix.display()
                        ),
                        ..tc
                    }),
                    Some(lock) => Ok(from_path(
                        host,
                        &format!(
                            "managed toolchain at '{}' is TeX Live {} but the profile requires {profile_year}; using PATH",
                            prefix.display(),
                            lock.texlive_year
                        ),
                    )),
                    None => Ok(from_path(
                        host,
                        &format!("managed prefix '{}' has no valid lock; using PATH", prefix.display()),
                    )),
                },
                None => Ok(from_path(
                    host,
                    &format!("no managed toolchain installed at '{}'; using PATH", prefix.display()),
                )),
            }
        }
    }
}

fn from_path(host: &HostEnv, reason: &str) -> ResolvedToolchain {
    let dirs = host.path_dirs();
    ResolvedToolchain {
        source: ToolchainSource::System,
        reason: reason.to_string(),
        bin_dir: None,
        xelatex: find_tool_in("xelatex", &dirs, host),
        biber: find_tool_in("biber", &dirs, host),
        kpsewhich: find_tool_in("kpsewhich", &dirs, host),
        lock: None,
    }
}

/// A prefix is a toolchain when `xelatex` is discoverable in `<prefix>/bin`
/// or in exactly one of its platform subdirectories (`bin/x86_64-linux`,
/// `bin/universal-darwin`, ...). `None` otherwise.
fn from_prefix(prefix: &Path, host: &HostEnv) -> Option<ResolvedToolchain> {
    let bin_dir = locate_bin_dir(prefix, host)?;
    let dirs = vec![bin_dir.clone()];
    let lock = ToolchainLock::read_from_prefix(prefix).and_then(Result::ok);
    Some(ResolvedToolchain {
        source: ToolchainSource::Managed { prefix: prefix.to_path_buf() },
        reason: String::new(),
        xelatex: find_tool_in("xelatex", &dirs, host),
        biber: find_tool_in("biber", &dirs, host),
        kpsewhich: find_tool_in("kpsewhich", &dirs, host),
        bin_dir: Some(bin_dir),
        lock,
    })
}

fn locate_bin_dir(prefix: &Path, host: &HostEnv) -> Option<PathBuf> {
    let bin = prefix.join("bin");
    if find_tool_in("xelatex", std::slice::from_ref(&bin), host).is_some() {
        return Some(bin);
    }
    let mut subdirs: Vec<PathBuf> = std::fs::read_dir(&bin)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    subdirs.sort();
    subdirs
        .into_iter()
        .find(|d| find_tool_in("xelatex", std::slice::from_ref(d), host).is_some())
}

/// Locates an executable named `name` in `dirs`, in order, without
/// running it. Windows semantics try the bare name and every `PATHEXT`
/// extension; Unix semantics require a regular file with an executable
/// bit. Directories named like the tool are never a match.
pub fn find_tool_in(name: &str, dirs: &[PathBuf], host: &HostEnv) -> Option<PathBuf> {
    for dir in dirs {
        if dir.as_os_str().is_empty() {
            continue;
        }
        for candidate in candidate_names(name, host) {
            let path = dir.join(&candidate);
            if is_executable_file(&path, host) {
                return Some(path);
            }
        }
    }
    None
}

/// Discovery over the host `PATH`.
pub fn find_tool_on_path(name: &str, host: &HostEnv) -> Option<PathBuf> {
    find_tool_in(name, &host.path_dirs(), host)
}

fn candidate_names(name: &str, host: &HostEnv) -> Vec<String> {
    if !host.os.is_windows() {
        return vec![name.to_string()];
    }
    let mut names = vec![name.to_string()];
    let lower = name.to_ascii_lowercase();
    let already_has_ext = host
        .pathext
        .iter()
        .any(|ext| lower.ends_with(&ext.to_ascii_lowercase()));
    if !already_has_ext {
        for ext in &host.pathext {
            // Lowercase first: the conventional on-disk spelling, and the
            // one a case-insensitive filesystem should report back.
            let lower_ext = ext.to_ascii_lowercase();
            names.push(format!("{name}{lower_ext}"));
            if lower_ext != *ext {
                names.push(format!("{name}{ext}"));
            }
        }
    }
    names
}

fn is_executable_file(path: &Path, host: &HostEnv) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    if !meta.is_file() {
        return false;
    }
    if host.os.is_windows() {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
