//! `check` and `build`: engine-free validation, source generation, optional
//! XeLaTeX/Biber compilation, and transactional publication.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use terse_core::artifact::GeneratedFile;
use terse_core::source::{FileId, SourceFile};
use terse_core::theme::ResolvedTheme;
use terse_core::{compile, ArtifactPlan};

use crate::engine::{self, EngineConfig, ProcessRunner};
use crate::project::{self, ProjectContext, ProjectError};
use crate::publication;
use crate::toolchain::{self, HostEnv, ResolvedToolchain, ToolchainSelector};

/// Resolves the toolchain for `project` from the flag, then `[latex]
/// toolchain`, then the managed prefix, then `PATH`. A failure has already
/// been printed and is a configuration error (exit `2`).
pub fn resolve_toolchain_for(
    project: &ProjectContext,
    flag: Option<&ToolchainSelector>,
    host: &HostEnv,
) -> Result<ResolvedToolchain, i32> {
    let year = project::profile_year(&project.manifest);
    let selector = toolchain::select(flag, project.manifest.latex.toolchain.as_deref(), &project.root)
        .map_err(|e| report_toolchain_error(&e))?;
    toolchain::resolve(&selector, &year, host).map_err(|e| report_toolchain_error(&e))
}

fn report_toolchain_error(e: &toolchain::ToolchainError) -> i32 {
    eprintln!("error[{}]: {}", e.code(), e.message());
    toolchain::ToolchainError::EXIT_CODE
}

/// The complete child environment for engine processes under `tc`,
/// with the Terse-owned TeX user tree created on demand so a child never
/// falls back to the user's own.
pub fn child_env_for(tc: &ResolvedToolchain, host: &HostEnv) -> Vec<(String, String)> {
    let texmf = toolchain::paths::texmf_dirs(host);
    for dir in [&texmf.home, &texmf.var, &texmf.config] {
        let _ = std::fs::create_dir_all(dir);
    }
    toolchain::prepare_child_env(tc, host, &texmf)
}

pub fn run_check(
    cwd: &Path,
    entry: Option<&Path>,
    theme: Option<&str>,
    strict: bool,
    deny_warnings: bool,
    json: bool,
) -> i32 {
    let (code, entry_path, diags) = match check_diagnostics(cwd, entry, theme, strict, deny_warnings) {
        Ok(v) => v,
        Err(code) => return code,
    };
    // Re-resolve for rendering only: `check_diagnostics`'s own file index
    // stays internal to keep its signature stable for callers that only
    // need codes/spans, not rendered text.
    let file_index = project::resolve_project(entry, cwd)
        .ok()
        .and_then(|(project, entry_path)| project::load_modules(&project.root, &entry_path).ok())
        .map(|loaded| loaded.file_index)
        .unwrap_or_default();
    if json {
        // Diagnostics are the only thing `check --json` writes to stdout;
        // every other message (including the plain-mode "check: ok") is
        // reserved for stderr so stdout always parses as one JSON value.
        println!("{}", crate::diagnostics::render_json(&diags, &file_index, &entry_path));
    } else {
        for d in &diags {
            eprintln!("{}", render(&file_index, &entry_path, d));
        }
        if code == 0 {
            println!("check: ok");
        }
    }
    code
}

/// Runs `check` and returns the exit code, the resolved entry path, and
/// every diagnostic (errors, plus `--strict` warnings) instead of printing
/// them, so tests can assert on diagnostic codes/spans directly. When
/// `theme` is `None`, every theme declared in the manifest's `[themes]`
/// table is validated (falling back to just the built-in `academic`
/// default when none are declared); an explicit `theme` narrows checking
/// to exactly that one presentation and its own dependencies.
pub fn check_diagnostics(
    cwd: &Path,
    entry: Option<&Path>,
    theme: Option<&str>,
    strict: bool,
    deny_warnings: bool,
) -> Result<(i32, PathBuf, Vec<terse_core::diagnostic::Diagnostic>), i32> {
    let (project, entry_path) = project::resolve_project(entry, cwd).map_err(|e| report_project_error(&e))?;

    let theme_names: Vec<String> = match theme {
        Some(t) => vec![t.to_string()],
        None => {
            let mut names: Vec<String> = project.manifest.themes.keys().cloned().collect();
            if names.is_empty() {
                names.push("academic".to_string());
            }
            names.sort();
            names
        }
    };

    let (mut diags, plan, _file_index) = compile_entry(&project, &entry_path)?;
    if !diags.is_empty() {
        return Ok((1, entry_path, diags));
    }
    let plan = plan.expect("no diagnostics implies a valid artifact plan");

    // Every requested theme must itself resolve, and every asset the
    // content declares must exist under that theme's own resolution
    // (each theme's logo lives in its own directory) -- this is what
    // makes `check` a genuine cross-theme validation and not just a
    // syntax/semantics check that happens to ignore presentation.
    for name in &theme_names {
        let (mut resolved_theme, theme_dir) = resolve_theme(&project, name)?;
        let mut module_copy = plan.module.clone();
        let entry_dir = entry_path.parent().unwrap_or(&project.root);
        resolve_assets(&project, entry_dir, &theme_dir, &mut module_copy, &mut resolved_theme)?;
    }

    let mut warnings = Vec::new();
    if strict {
        warnings = terse_core::semantic::collect_warnings(&plan.module);
    }
    let code = if strict && deny_warnings && !warnings.is_empty() {
        1
    } else {
        0
    };
    diags.extend(warnings);
    Ok((code, entry_path, diags))
}

pub fn run_build(
    cwd: &Path,
    entry: Option<&Path>,
    theme_name: Option<&str>,
    tex_only: bool,
    require_pdf: bool,
    json: bool,
) -> i32 {
    run_build_with_runner_json(
        cwd,
        entry,
        theme_name,
        tex_only,
        require_pdf,
        json,
        &mut engine::RealProcessRunner,
    )
}

/// Same as [`run_build`], but with the process runner injected. Exposed so
/// tests can drive the full command (tool discovery, generation,
/// publication) while controlling exactly what the "engine" reports,
/// without spawning a real XeLaTeX/Biber.
pub fn run_build_with_runner(
    cwd: &Path,
    entry: Option<&Path>,
    theme_name: Option<&str>,
    tex_only: bool,
    require_pdf: bool,
    runner: &mut dyn ProcessRunner,
) -> i32 {
    run_build_with_runner_json(cwd, entry, theme_name, tex_only, require_pdf, false, runner)
}

/// As [`run_build_with_runner`], with `--json` control over how
/// compile-time diagnostics (not engine/publication failures, which have
/// no `.trs` position to report as structured JSON) are printed.
pub fn run_build_with_runner_json(
    cwd: &Path,
    entry: Option<&Path>,
    theme_name: Option<&str>,
    tex_only: bool,
    require_pdf: bool,
    json: bool,
    runner: &mut dyn ProcessRunner,
) -> i32 {
    run_build_with_host(
        cwd,
        entry,
        theme_name,
        tex_only,
        require_pdf,
        json,
        None,
        &HostEnv::capture(),
        runner,
    )
}

/// The full `build` command with every external input explicit: the
/// toolchain selection flag, the host environment, and the process
/// runner. Every other `run_build*` entry delegates here.
#[allow(clippy::too_many_arguments)]
pub fn run_build_with_host(
    cwd: &Path,
    entry: Option<&Path>,
    theme_name: Option<&str>,
    tex_only: bool,
    require_pdf: bool,
    json: bool,
    flag: Option<&ToolchainSelector>,
    host: &HostEnv,
    runner: &mut dyn ProcessRunner,
) -> i32 {
    if tex_only && require_pdf {
        eprintln!("error[E-CONFIG-005]: --tex-only and --require-pdf are mutually exclusive");
        return 2;
    }

    let (project, entry_path) = match project::resolve_project(entry, cwd) {
        Ok(v) => v,
        Err(e) => return report_project_error(&e),
    };

    // Flags win; otherwise `[latex] pdf` is the default compilation mode.
    let (tex_only, require_pdf) = if tex_only || require_pdf {
        (tex_only, require_pdf)
    } else {
        match project.manifest.latex.pdf {
            terse_core::project::config::PdfMode::Auto => (false, false),
            terse_core::project::config::PdfMode::TexOnly => (true, false),
            terse_core::project::config::PdfMode::RequirePdf => (false, true),
        }
    };

    let toolchain = if tex_only {
        None
    } else {
        match resolve_toolchain_for(&project, flag, host) {
            Ok(tc) => Some(tc),
            Err(code) => return code,
        }
    };

    let theme_name = theme_name.unwrap_or("academic");
    let (mut theme, theme_dir) = match resolve_theme(&project, theme_name) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let (diags, plan, file_index) = match compile_entry(&project, &entry_path) {
        Ok(v) => v,
        Err(code) => return code,
    };
    if !diags.is_empty() {
        if json {
            println!("{}", crate::diagnostics::render_json(&diags, &file_index, &entry_path));
        } else {
            for d in &diags {
                eprintln!("{}", render(&file_index, &entry_path, d));
            }
        }
        return 1;
    }
    let mut plan: ArtifactPlan = plan.expect("no diagnostics implies a valid artifact plan");

    let mut asset_files = match resolve_assets(
        &project,
        entry_path.parent().unwrap_or(&project.root),
        &theme_dir,
        &mut plan.module,
        &mut theme,
    ) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let extra_packages = match terse_core::latex::validate_packages(&project.manifest.latex.packages) {
        Ok(v) => v,
        Err(terse_core::latex::UnknownPackage(name)) => {
            eprintln!("error[E-LATEX-003]: unknown declared package '{name}'");
            return 2;
        }
    };
    let mut support_files = match load_support_files(&project.root, &project.manifest.latex.support_files) {
        Ok(v) => v,
        Err(code) => return code,
    };
    support_files.append(&mut asset_files);
    let cited_aliases = terse_core::semantic::collect_cited_aliases(&plan.module);
    let cited: std::collections::BTreeMap<_, _> = plan
        .bindings
        .authorized
        .iter()
        .filter(|(alias, _)| cited_aliases.contains(*alias))
        .map(|(alias, record)| (alias.clone(), record.clone()))
        .collect();
    let needs_biber = !cited.is_empty();

    // Disposable, content-addressed cache over source generation only:
    // never consulted for the reference lock (already its own persistent
    // source of truth) or for the engine/PDF step, which always runs
    // fresh. A miss (including a corrupted entry) is silently treated as
    // absent and simply recomputes; deleting the cache directory must
    // never change output, only performance.
    let cache_key_parts: Vec<Vec<u8>> = {
        let mut modules: Vec<&(PathBuf, String)> = file_index.values().collect();
        modules.sort_by(|a, b| a.0.cmp(&b.0));
        let mut parts: Vec<Vec<u8>> = modules.iter().map(|(_, text)| text.as_bytes().to_vec()).collect();
        parts.push(theme_name.as_bytes().to_vec());
        parts.push(format!("{theme:?}").into_bytes());
        parts.push(format!("{extra_packages:?}").into_bytes());
        for (path, bytes) in &support_files {
            parts.push(path.as_bytes().to_vec());
            parts.push(bytes.clone());
        }
        parts.push(format!("{cited:?}").into_bytes());
        parts
    };
    let cache_key_refs: Vec<&[u8]> = cache_key_parts.iter().map(|p| p.as_slice()).collect();
    let cache_key = crate::cache::compute_key(&cache_key_refs);

    let mut files = if let Some(cached) = crate::cache::load(&project.root, &cache_key) {
        cached
            .into_iter()
            .map(|(logical_path, bytes)| GeneratedFile { logical_path, bytes })
            .collect()
    } else {
        let manifest = match terse_core::artifact::plan_source_artifacts_with_support(
            &plan.module,
            &theme,
            &extra_packages,
            &support_files,
            &cited,
            &plan.file_paths,
        ) {
            Ok(m) => m,
            Err(terse_core::artifact::SupportFileCollision(name)) => {
                eprintln!(
                    "error[E-LATEX-004]: declared support file '{name}' collides with a generated file name"
                );
                return 2;
            }
        };
        let files = manifest.files;
        let cacheable: crate::cache::CachedFiles = files
            .iter()
            .map(|f| (f.logical_path.clone(), f.bytes.clone()))
            .collect();
        crate::cache::store(&project.root, &cache_key, &cacheable);
        files
    };

    if let Some(tc) = &toolchain {
        match find_pdf_engine(tc, require_pdf) {
            EngineAvailability::Missing => {
                eprintln!(
                    "warning: xelatex not found ({}); wrote sources only (rendering and glyph coverage were not engine-validated)",
                    tc.reason
                );
            }
            EngineAvailability::MissingButRequired => {
                eprintln!(
                    "error[E-LATEX-002]: --require-pdf requested but xelatex was not found ({})",
                    tc.reason
                );
                return 3;
            }
            EngineAvailability::Present { xelatex, biber } => {
                let env = child_env_for(tc, host);
                match compile_pdf(&files, &xelatex, biber.as_deref(), needs_biber, &env, runner) {
                    Ok(pdf_bytes) => files.push(GeneratedFile {
                        logical_path: "paper.pdf".to_string(),
                        bytes: pdf_bytes,
                    }),
                    Err(failure) => {
                        let diag = crate::engine::logs::interpret_failure(&failure);
                        eprintln!("{}", crate::diagnostics::render_human(&diag, "(engine)", ""));
                        return 3;
                    }
                }
            }
        }
    }

    let output_dir = project
        .root
        .join(&project.manifest.project.output)
        .join(theme_name);

    let file_tuples: Vec<(String, Vec<u8>)> = files
        .into_iter()
        .map(|f| (f.logical_path, f.bytes))
        .collect();

    if let Err(e) = publication::check_destination_ownership(&output_dir) {
        eprintln!(
            "error[E-CONFIG-007]: refusing to replace an unowned populated output directory ({e:?})"
        );
        return 3;
    }
    let staged = match publication::stage(&output_dir, &file_tuples) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: staging the build failed: {e:?}");
            return 3;
        }
    };
    if let Err(e) = publication::publish(staged, &output_dir) {
        eprintln!("error: publishing the build failed: {e:?}");
        return 3;
    }

    println!(
        "build: published {} files to {} (rendering/glyph checks {})",
        file_tuples.len(),
        output_dir.display(),
        if tex_only { "not run" } else { "run" }
    );
    0
}

pub(crate) enum EngineAvailability {
    Missing,
    MissingButRequired,
    Present {
        xelatex: PathBuf,
        biber: Option<PathBuf>,
    },
}

pub(crate) fn find_pdf_engine(tc: &ResolvedToolchain, require_pdf: bool) -> EngineAvailability {
    match &tc.xelatex {
        Some(xelatex) => EngineAvailability::Present {
            xelatex: xelatex.clone(),
            biber: tc.biber.clone(),
        },
        None if require_pdf => EngineAvailability::MissingButRequired,
        None => EngineAvailability::Missing,
    }
}

pub(crate) fn compile_pdf(
    files: &[GeneratedFile],
    xelatex: &Path,
    biber: Option<&Path>,
    needs_biber: bool,
    env: &[(String, String)],
    runner: &mut dyn ProcessRunner,
) -> Result<Vec<u8>, engine::CompileFailure> {
    if needs_biber && biber.is_none() {
        return Err(engine::CompileFailure::ToolStartFailed { program: "biber".to_string() });
    }
    let work_dir = unique_temp_dir("engine-work");
    let result = compile_pdf_in(&work_dir, files, xelatex, biber, needs_biber, env, runner);
    let _ = std::fs::remove_dir_all(&work_dir);
    result
}

fn compile_pdf_in(
    work_dir: &Path,
    files: &[GeneratedFile],
    xelatex: &Path,
    biber: Option<&Path>,
    needs_biber: bool,
    env: &[(String, String)],
    runner: &mut dyn ProcessRunner,
) -> Result<Vec<u8>, engine::CompileFailure> {
    std::fs::create_dir_all(work_dir).expect("temp working directory must be creatable");
    for f in files {
        let dest = work_dir.join(&f.logical_path);
        if let Some(p) = dest.parent() {
            std::fs::create_dir_all(p).expect("temp working subdirectory must be creatable");
        }
        std::fs::write(&dest, &f.bytes).expect("temp working file must be writable");
    }

    let config = EngineConfig {
        xelatex: xelatex.to_path_buf(),
        biber: biber.map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("biber")),
        timeout: Duration::from_secs(60),
    };
    let passes = engine::compile_bounded_with_env(runner, work_dir, "paper", &config, needs_biber, env)
        .map_err(|(_, failure)| failure)?;
    // A missing glyph is only ever a warning to the engine's own exit
    // code (xelatex still exits 0), so it must be detected from the log
    // text itself rather than the process status, and turned into a real
    // build failure per this group's policy.
    if passes
        .iter()
        .filter(|p| p.kind == engine::PassKind::Xelatex)
        .any(|p| String::from_utf8_lossy(&p.outcome.stdout).contains("Missing character"))
    {
        return Err(engine::CompileFailure::MissingGlyph);
    }
    // Same shape of silent failure, different scope: xelatex exits 0 with
    // `??` in the PDF when a reference never resolves. Only the settled
    // final pass counts (see `has_undefined_references`).
    if engine::logs::has_undefined_references(&passes) {
        return Err(engine::CompileFailure::UndefinedReferences);
    }

    std::fs::read(work_dir.join("paper.pdf")).map_err(|_| engine::CompileFailure::NonZeroExit {
        kind: engine::PassKind::Xelatex,
        code: None,
    })
}

pub(crate) fn unique_temp_dir(label: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "terse-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Loads the entry and every module it transitively includes, then runs
/// the effect-free compile pipeline. Returns the diagnostics/plan plus a
/// file index so callers can render any diagnostic, whether it is
/// anchored in the entry or in an included module.
pub(crate) fn compile_entry(
    project: &ProjectContext,
    entry_path: &Path,
) -> Result<
    (
        Vec<terse_core::diagnostic::Diagnostic>,
        Option<ArtifactPlan>,
        std::collections::HashMap<terse_core::source::FileId, (PathBuf, String)>,
    ),
    i32,
> {
    let loaded = project::load_modules(&project.root, entry_path).map_err(|e| {
        eprintln!("error: loading project sources failed: {e:?}");
        2
    })?;
    let (diags, plan) = compile(&loaded.snapshot);
    Ok((diags, plan, loaded.file_index))
}

pub(crate) fn render(
    file_index: &std::collections::HashMap<terse_core::source::FileId, (PathBuf, String)>,
    entry_path: &Path,
    diag: &terse_core::diagnostic::Diagnostic,
) -> String {
    match diag.primary.and_then(|span| file_index.get(&span.file_id)) {
        Some((path, text)) => crate::diagnostics::render_human(diag, &path.to_string_lossy(), text),
        None => {
            let text = std::fs::read_to_string(entry_path).unwrap_or_default();
            crate::diagnostics::render_human(diag, &entry_path.to_string_lossy(), &text)
        }
    }
}

/// Reads each declared `[latex] support-files` entry relative to the
/// project root, rejecting absolute/traversing declarations before ever
/// touching the filesystem. Declared paths are also used as the file's
/// logical (generated-tree) path, so a declared `drawing/lib.tex` is
/// copied to `drawing/lib.tex` alongside `paper.tex`.
pub(crate) fn load_support_files(root: &Path, declared: &[String]) -> Result<Vec<(String, Vec<u8>)>, i32> {
    let mut out = Vec::new();
    for declared_path in declared {
        if terse_core::project::paths::normalize_logical(declared_path).is_err() {
            eprintln!(
                "error[E-CONFIG-008]: declared support file '{declared_path}' is not a valid relative path"
            );
            return Err(2);
        }
        let bytes = std::fs::read(root.join(declared_path)).map_err(|e| {
            eprintln!("error[E-CONFIG-009]: cannot read declared support file '{declared_path}': {e}");
            2
        })?;
        out.push((declared_path.clone(), bytes));
    }
    Ok(out)
}

/// Resolves the presentation for `theme_name`: either the compiler's
/// built-in `academic` defaults (when the manifest declares no override
/// for that name), or a declared `[themes]` entry's `.theme` file,
/// resolved against the project root. Returns the resolved theme and the
/// directory its own declarations (currently just its logo) resolve
/// relative to.
pub(crate) fn resolve_theme(project: &ProjectContext, theme_name: &str) -> Result<(ResolvedTheme, PathBuf), i32> {
    match project.manifest.themes.get(theme_name) {
        Some(declared_path) => {
            if terse_core::project::paths::normalize_logical(declared_path).is_err() {
                eprintln!(
                    "error[E-THEME-002]: declared theme path '{declared_path}' for '{theme_name}' is not a valid relative path"
                );
                return Err(2);
            }
            let theme_file = project.root.join(declared_path);
            let bytes = std::fs::read(&theme_file).map_err(|e| {
                eprintln!("error[E-THEME-003]: cannot read theme file '{declared_path}': {e}");
                2
            })?;
            let source = SourceFile::new(FileId(0), theme_file.to_string_lossy(), bytes).map_err(|e| {
                eprintln!("error[E-THEME-003]: invalid encoding in theme file '{declared_path}': {e:?}");
                2
            })?;
            let resolved = terse_core::theme::resolve_theme(theme_name, &source).map_err(|e| {
                eprintln!("error[E-THEME-004]: theme '{theme_name}' failed to resolve: {e:?}");
                2
            })?;
            let theme_dir = theme_file.parent().unwrap_or(&project.root).to_path_buf();
            Ok((resolved, theme_dir))
        }
        None if theme_name == "academic" => Ok((terse_core::theme::academic(), project.root.clone())),
        None => {
            eprintln!("error[E-THEME-001]: unknown theme '{theme_name}' (not declared in this project's [themes])");
            Err(2)
        }
    }
}

/// Reads and confines an existing file to `root`: resolves `relative`
/// against `base_dir`, canonicalizes it (so it must already exist), and
/// rejects anything a symlink or `..` component would let escape `root`.
pub(crate) fn read_confined_asset(root: &Path, base_dir: &Path, relative: &str) -> Result<Vec<u8>, String> {
    let root_canonical = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
    let candidate = base_dir.join(relative);
    let canonical = std::fs::canonicalize(&candidate).map_err(|e| format!("{}: {e}", candidate.display()))?;
    if !canonical.starts_with(&root_canonical) {
        return Err(format!("{} escapes the project root", candidate.display()));
    }
    if !canonical.is_file() {
        return Err(format!("{} is not a regular file", candidate.display()));
    }
    std::fs::read(&canonical).map_err(|e| e.to_string())
}

/// Resolves every referenced content figure (relative to the entry file's
/// own directory: this milestone has no include expansion yet) and the
/// theme's logo (relative to its declaring `.theme` file's directory),
/// validates their extensions, reads their bytes, and assigns each a
/// stable generated-tree logical path. Content figures live under
/// `assets/`; the theme logo lives under the separate `theme-assets/`
/// prefix so a theme's own resource can never collide with (and so never
/// changes the disambiguation of) an authored figure, keeping `paper.tex`
/// byte-identical across themes. Rewrites `module`'s figure paths and
/// `theme.logo_path` in place to their final logical paths.
pub(crate) fn resolve_assets(
    project: &ProjectContext,
    entry_dir: &Path,
    theme_dir: &Path,
    module: &mut terse_core::semantic::ParsedModule,
    theme: &mut ResolvedTheme,
) -> Result<Vec<(String, Vec<u8>)>, i32> {
    let mut out = Vec::new();

    let declared_paths = terse_core::artifact::assets::collect_figure_paths(&module.blocks);
    for path in &declared_paths {
        if let Err(e) = terse_core::artifact::assets::validate_asset_path(path) {
            eprintln!("error[E-ASSET-001]: figure asset '{path}' is invalid: {e:?}");
            return Err(2);
        }
    }
    let assigned = terse_core::artifact::assets::disambiguate_assets(&declared_paths);
    let mut rewrite = HashMap::new();
    for (declared, logical) in &assigned {
        let bytes = read_confined_asset(&project.root, entry_dir, declared).map_err(|e| {
            eprintln!("error[E-ASSET-002]: cannot read figure asset '{declared}': {e}");
            2
        })?;
        rewrite.insert(declared.clone(), logical.clone());
        out.push((logical.clone(), bytes));
    }
    terse_core::artifact::assets::rewrite_figure_paths(&mut module.blocks, &rewrite);

    if let Some(declared) = theme.logo_path.clone() {
        if let Err(e) = terse_core::artifact::assets::validate_asset_path(&declared) {
            eprintln!("error[E-ASSET-003]: theme logo '{declared}' is invalid: {e:?}");
            return Err(2);
        }
        let bytes = read_confined_asset(&project.root, theme_dir, &declared).map_err(|e| {
            eprintln!("error[E-ASSET-004]: cannot read theme logo '{declared}': {e}");
            2
        })?;
        let ext = declared.rsplit('.').next().unwrap_or("png");
        let logical = format!("theme-assets/logo.{ext}");
        out.push((logical.clone(), bytes));
        theme.logo_path = Some(logical);
    }

    Ok(out)
}

pub(crate) fn report_project_error(e: &ProjectError) -> i32 {
    match e {
        ProjectError::ManifestNotFound { start_dir } => {
            eprintln!(
                "error[E-CONFIG-001]: no terse.toml found from {} or its ancestors",
                start_dir.display()
            );
            eprintln!("  help: run `terse init` to create one");
        }
        ProjectError::Config(ce) => {
            eprintln!("error[E-CONFIG-002]: invalid project manifest: {ce:?}");
        }
        ProjectError::EngineMismatch { configured, profile, profile_name } => {
            eprintln!(
                "error[E-CONFIG-008]: [latex] engine = \"{configured}\" disagrees with the '{profile_name}' profile, which requires \"{profile}\""
            );
            eprintln!("  help: remove the field or set it to \"{profile}\"");
        }
        ProjectError::Path(pe) => {
            eprintln!("error[E-CONFIG-003]: invalid project path configuration: {pe:?}");
        }
        ProjectError::Io(msg) => {
            eprintln!("error[E-CONFIG-004]: {msg}");
        }
        ProjectError::Lock(msg) => {
            eprintln!("error[E-REF-007]: invalid references.lock: {msg}");
        }
        ProjectError::Overrides(msg) => {
            eprintln!("error[E-REF-008]: invalid references.overrides.toml: {msg}");
        }
    }
    2
}
