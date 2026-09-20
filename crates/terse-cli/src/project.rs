//! Application-owned project discovery, manifest loading, and canonical
//! filesystem checks. `terse-core` never touches the filesystem itself.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use terse_core::project::config::{self, ConfigError, Manifest};
use terse_core::project::dependencies::DependencySet;
use terse_core::project::paths::{self, PathError};
use terse_core::source::{FileId, SourceFile};

pub const MANIFEST_FILE_NAME: &str = "terse.toml";

#[derive(Debug)]
pub enum ProjectError {
    ManifestNotFound { start_dir: PathBuf },
    Config(ConfigError),
    /// `[latex] engine` disagrees with the export profile's engine.
    EngineMismatch { configured: String, profile: String, profile_name: String },
    Path(PathError),
    Io(String),
    Lock(String),
    Overrides(String),
}

pub const LOCK_FILE_NAME: &str = "references.lock";
pub const DEFAULT_PROFILE_NAME: &str = "texlive-2025-xelatex";

/// The export profile the project is anchored to: `[export.arxiv]
/// profile` when declared, otherwise the compiler default.
pub fn profile_name(manifest: &Manifest) -> String {
    manifest
        .export
        .arxiv
        .as_ref()
        .map(|a| a.profile.clone())
        .unwrap_or_else(|| DEFAULT_PROFILE_NAME.to_string())
}

/// The TeX Live year the project's profile describes; the compiler
/// default's year when the profile is unknown (that error surfaces where
/// the profile is actually used).
pub fn profile_year(manifest: &Manifest) -> String {
    terse_core::artifact::profile::resolve_profile(&profile_name(manifest))
        .or_else(|_| terse_core::artifact::profile::resolve_profile(DEFAULT_PROFILE_NAME))
        .map(|p| p.texlive_year)
        .unwrap_or_else(|_| "2025".to_string())
}
pub const OVERRIDES_FILE_NAME: &str = "references.overrides.toml";

/// Reads and decodes `references.lock` at the project root, if present.
/// A missing lock is not an error: it is equivalent to no reference being
/// resolved yet.
pub fn load_lock(root: &Path) -> Result<Option<terse_core::references::lock::LockFile>, ProjectError> {
    let path = root.join(LOCK_FILE_NAME);
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|e| ProjectError::Io(e.to_string()))?;
    terse_core::references::lock::decode(&text)
        .map(Some)
        .map_err(|e| ProjectError::Lock(e.to_string()))
}

/// Reads and decodes `references.overrides.toml` at the project root, if
/// present. A missing file is equivalent to no overrides.
pub fn load_overrides(
    root: &Path,
) -> Result<HashMap<String, terse_core::references::overrides::OverridePatch>, ProjectError> {
    let path = root.join(OVERRIDES_FILE_NAME);
    if !path.is_file() {
        return Ok(HashMap::new());
    }
    let text = fs::read_to_string(&path).map_err(|e| ProjectError::Io(e.to_string()))?;
    terse_core::references::overrides::decode_file(&text).map_err(ProjectError::Overrides)
}

#[derive(Debug, Clone)]
pub struct ProjectContext {
    pub root: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest: Manifest,
}

/// Walks upward from `start_dir` looking for `terse.toml`.
pub fn discover_manifest(start_dir: &Path) -> Option<PathBuf> {
    let mut dir = Some(start_dir.to_path_buf());
    while let Some(d) = dir {
        let candidate = d.join(MANIFEST_FILE_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    None
}

/// Loads and validates the project whose manifest is discovered from
/// `start_dir`.
pub fn load_project(start_dir: &Path) -> Result<ProjectContext, ProjectError> {
    let manifest_path =
        discover_manifest(start_dir).ok_or_else(|| ProjectError::ManifestNotFound {
            start_dir: start_dir.to_path_buf(),
        })?;
    let root = manifest_path
        .parent()
        .expect("a file path always has a parent")
        .to_path_buf();

    let text = fs::read_to_string(&manifest_path).map_err(|e| ProjectError::Io(e.to_string()))?;
    let manifest = config::parse_manifest(&text).map_err(ProjectError::Config)?;

    // A manifest engine that can disagree with the pinned profile is worse
    // than none: the profile is the single statement of what the toolchain
    // must provide, so the two are reconciled at load time, before any
    // command acts on either.
    let profile_name = manifest
        .export
        .arxiv
        .as_ref()
        .map(|a| a.profile.clone())
        .unwrap_or_else(|| DEFAULT_PROFILE_NAME.to_string());
    if let Ok(profile) = terse_core::artifact::profile::resolve_profile(&profile_name) {
        if manifest.latex.engine != profile.engine {
            return Err(ProjectError::EngineMismatch {
                configured: manifest.latex.engine.clone(),
                profile: profile.engine.clone(),
                profile_name,
            });
        }
    }

    paths::validate_output_scope(&manifest.project.entry, &manifest.project.output)
        .map_err(ProjectError::Path)?;
    let output_components = paths::normalize_logical(&manifest.project.output)
        .map_err(ProjectError::Path)?;
    validate_output_fs(&root, &output_components)?;

    Ok(ProjectContext {
        root,
        manifest_path,
        manifest,
    })
}

/// Resolves the project and entry file path for a command. An explicit
/// entry is relative to `invocation_dir` and starts discovery from its own
/// directory; otherwise discovery starts at `invocation_dir`.
pub fn resolve_project(
    explicit_entry: Option<&Path>,
    invocation_dir: &Path,
) -> Result<(ProjectContext, PathBuf), ProjectError> {
    let start_dir = match explicit_entry {
        Some(entry) => {
            let abs_entry = invocation_dir.join(entry);
            abs_entry
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| invocation_dir.to_path_buf())
        }
        None => invocation_dir.to_path_buf(),
    };

    let project = load_project(&start_dir)?;
    let entry_path = match explicit_entry {
        Some(entry) => invocation_dir.join(entry),
        None => project.root.join(&project.manifest.project.entry),
    };
    Ok((project, entry_path))
}

/// Canonical-filesystem guard against a symlink escape: walks the output
/// path's components, canonicalizing whichever prefix already exists, and
/// rejects an existing ancestor that resolves outside the project root.
fn validate_output_fs(root: &Path, output_components: &[String]) -> Result<(), ProjectError> {
    let root_canonical = fs::canonicalize(root).map_err(|e| ProjectError::Io(e.to_string()))?;
    let mut current = root_canonical.clone();
    for component in output_components {
        let candidate = current.join(component);
        match fs::canonicalize(&candidate) {
            Ok(canonical) => {
                if !canonical.starts_with(&root_canonical) {
                    return Err(ProjectError::Path(PathError::Traversal));
                }
                current = canonical;
            }
            Err(_) => {
                // Remaining components do not exist yet; the logical-path
                // check already ruled out traversal in the undiscovered
                // suffix.
                break;
            }
        }
    }
    Ok(())
}

// --- General canonical root confinement -------------------------------

#[derive(Debug)]
pub enum ConfinementError {
    Io(std::io::Error),
    Escapes,
    NotAFile,
}

/// Confines a root-relative logical path (given as `/`-separated
/// components) to `root`: canonicalizes both, so the candidate must
/// already exist, and rejects anything a `..` component, an absolute
/// path, or a symlink (including a symlink chain through an intermediate
/// directory) would let escape the root. Also rejects dangling symlinks
/// and anything that is not a regular file (directories, FIFOs, device
/// files). This is the one confinement rule every root-contained read in
/// the project uses: sources, includes, themes, lock/overrides, assets,
/// and support files.
pub fn confine_and_read(root: &Path, logical: &[String]) -> Result<Vec<u8>, ConfinementError> {
    let root_canonical = fs::canonicalize(root).map_err(ConfinementError::Io)?;
    let mut candidate = root_canonical.clone();
    for component in logical {
        candidate.push(component);
    }
    let canonical = fs::canonicalize(&candidate).map_err(ConfinementError::Io)?;
    if !canonical.starts_with(&root_canonical) {
        return Err(ConfinementError::Escapes);
    }
    if !canonical.is_file() {
        return Err(ConfinementError::NotAFile);
    }
    fs::read(&canonical).map_err(ConfinementError::Io)
}

// --- Multi-file include discovery and loading --------------------------

/// The entry plus every module it transitively includes, ready to hand to
/// `terse_core::compile`, alongside everything the walk touched (or tried
/// to touch), for dependency tracking.
pub struct LoadedProject {
    pub snapshot: terse_core::InputSnapshot,
    pub dependencies: DependencySet,
    /// Every loaded file's path and text, keyed by the `FileId` assigned
    /// to it, so diagnostics anchored in any included module (not just the
    /// entry) can be rendered with the right source.
    pub file_index: HashMap<FileId, (PathBuf, String)>,
}

/// Best-effort discovery parse: used only to find `include` targets to
/// walk toward. Parse failures here are not reported; the authoritative
/// parse (with real diagnostics) happens inside `terse_core::compile`.
fn discover_includes(text: &str) -> Vec<String> {
    let file_id = FileId(0);
    let Ok(lines) = terse_core::syntax::lexer::lex_lines(text) else {
        return Vec::new();
    };
    let Ok(blocks) = terse_core::syntax::blocks::parse_module(&lines, text, file_id, 0) else {
        return Vec::new();
    };
    blocks
        .into_iter()
        .filter_map(|b| match b {
            terse_core::syntax::blocks::TopBlock::Include { path, .. } => Some(path),
            _ => None,
        })
        .collect()
}

/// Resolves a declared include path to its root-relative logical key,
/// discarding (rather than erroring on) a wildcard or non-`.trs` path: the
/// discovery walk only needs to find *valid* targets to keep walking, and
/// leaves reporting an invalid declaration itself to `terse_core::expand`,
/// which runs during the authoritative compile.
fn resolve_include_target(base_dir: &[String], declared: &str) -> Option<String> {
    if declared.contains('*') || declared.contains('?') || !declared.ends_with(".trs") {
        return None;
    }
    paths::resolve_declared_path(base_dir, declared)
        .ok()
        .map(|c| c.join("/"))
}

fn logical_key_of(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Reads the entry, then walks its `include`s (and their `include`s,
/// transitively) breadth-first, confining every read to `root` and
/// parsing only to discover further targets. A target that cannot be
/// confined/read is recorded as an attempted dependency and left out of
/// the snapshot's modules, so `terse_core::compile` reports the missing
/// include itself, anchored at the include statement.
pub fn load_modules(root: &Path, entry_path: &Path) -> Result<LoadedProject, ProjectError> {
    let entry_key = logical_key_of(root, entry_path);
    let entry_bytes = fs::read(entry_path).map_err(|e| ProjectError::Io(e.to_string()))?;
    let entry_text = String::from_utf8_lossy(&entry_bytes).into_owned();
    let entry_file = SourceFile::new(FileId(0), entry_path.to_string_lossy(), entry_bytes)
        .map_err(|e| ProjectError::Io(format!("{e:?}")))?;

    let mut dependencies = DependencySet::new();
    dependencies.add_resolved(entry_key.clone());

    let mut modules: HashMap<String, SourceFile> = HashMap::new();
    let mut file_index: HashMap<FileId, (PathBuf, String)> = HashMap::new();
    file_index.insert(FileId(0), (entry_path.to_path_buf(), entry_text.clone()));

    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(entry_key.clone());
    let mut queue: VecDeque<(String, String)> = VecDeque::new();
    queue.push_back((entry_key.clone(), entry_text));

    let mut next_file_id = 1u32;
    while let Some((key, text)) = queue.pop_front() {
        let base_dir = terse_core::project::expand::dir_of(&key);
        for declared in discover_includes(&text) {
            let Some(target) = resolve_include_target(&base_dir, &declared) else {
                continue;
            };
            if visited.contains(&target) {
                continue;
            }
            visited.insert(target.clone());

            let target_components: Vec<String> = target.split('/').map(str::to_string).collect();
            match confine_and_read(root, &target_components) {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    let file_id = FileId(next_file_id);
                    next_file_id += 1;
                    let path = root.join(&target);
                    match SourceFile::new(file_id, path.to_string_lossy(), bytes) {
                        Ok(file) => {
                            dependencies.add_resolved(target.clone());
                            file_index.insert(file_id, (path, text.clone()));
                            modules.insert(target.clone(), file);
                            queue.push_back((target, text));
                        }
                        Err(_) => {
                            // Invalid encoding: leave unresolved so the
                            // authoritative compile reports it at the
                            // include statement.
                            dependencies.add_attempted(target);
                        }
                    }
                }
                Err(_) => {
                    dependencies.add_attempted(target);
                }
            }
        }
    }

    let lock = load_lock(root)?;
    let overrides = load_overrides(root)?;
    if root.join(LOCK_FILE_NAME).is_file() {
        dependencies.add_resolved(LOCK_FILE_NAME.to_string());
    }
    if root.join(OVERRIDES_FILE_NAME).is_file() {
        dependencies.add_resolved(OVERRIDES_FILE_NAME.to_string());
    }

    Ok(LoadedProject {
        snapshot: terse_core::InputSnapshot {
            entry_key,
            entry: entry_file,
            modules,
            lock,
            overrides,
        },
        dependencies,
        file_index,
    })
}
