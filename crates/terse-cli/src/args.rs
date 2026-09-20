use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "terse", version, about = "Terse: a document compiler for LaTeX output")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Scaffold a new project: manifest, entry, academic theme, empty lock.
    Init {
        directory: Option<PathBuf>,
        #[arg(long)]
        force: bool,
    },
    /// Validate reachable content without requiring a TeX engine.
    Check {
        entry: Option<PathBuf>,
        #[arg(long)]
        theme: Option<String>,
        #[arg(long)]
        strict: bool,
        #[arg(long = "deny-warnings")]
        deny_warnings: bool,
        #[arg(long)]
        json: bool,
        /// Also validate a named offline export target (currently only
        /// `arxiv`) against current sources, without producing an export.
        #[arg(long)]
        target: Option<String>,
        /// Where engine executables are located for `--target` validation:
        /// `auto`, `system`, `managed`, or a directory.
        #[arg(long)]
        toolchain: Option<String>,
    },
    /// Generate (and optionally compile) the LaTeX project.
    Build {
        entry: Option<PathBuf>,
        #[arg(long)]
        theme: Option<String>,
        #[arg(long, conflicts_with = "require_pdf")]
        tex_only: bool,
        #[arg(long)]
        require_pdf: bool,
        #[arg(long)]
        json: bool,
        /// Where engine executables are located: `auto` (managed prefix
        /// when installed and matching the profile year, else PATH),
        /// `system` (PATH only), `managed`, or a directory. Never affects
        /// generated text.
        #[arg(long)]
        toolchain: Option<String>,
    },
    /// Format `.trs`/`.theme` files.
    Fmt {
        paths: Vec<PathBuf>,
        #[arg(long)]
        check: bool,
        #[arg(long)]
        json: bool,
    },
    /// Reference metadata operations.
    Refs {
        #[command(subcommand)]
        action: RefsAction,
    },
    /// Build a self-contained, offline-compatible export package for an
    /// external target (currently only `arxiv`). Never contacts a
    /// network transport, tool downloader, or upload endpoint.
    Export {
        entry: Option<PathBuf>,
        #[arg(long)]
        theme: Option<String>,
        /// The export target's name. Currently only `arxiv`.
        #[arg(long, default_value = "arxiv")]
        target: String,
        /// Include a pre-built `.bbl` instead of the default `.bib`-only
        /// membership. Requires a matching main-stem filename and a
        /// verified-compatible bibliography backend; never silently
        /// renamed or substituted.
        #[arg(long)]
        include_bbl: Option<PathBuf>,
        /// Fail instead of reporting `static-only` when no compatible
        /// engine (or, with citations, Biber) is available to actually
        /// compile the extracted package.
        #[arg(long)]
        require_compile: bool,
        #[arg(long)]
        json: bool,
        /// Where engine executables are located: `auto`, `system`,
        /// `managed`, or a directory.
        #[arg(long)]
        toolchain: Option<String>,
    },
    /// Run every toolchain check through the bounded process runner and
    /// report evidence, probable causes, and per-OS fixes. Never contacts
    /// the network. Exit `0` with no failing check, `1` otherwise.
    Doctor {
        #[arg(long)]
        json: bool,
        /// Apply only the documented safe repairs (stale Biber PAR cache,
        /// macOS quarantine on managed binaries) and probe again.
        #[arg(long)]
        fix: bool,
        /// Where engine executables are located: `auto`, `system`,
        /// `managed`, or a directory.
        #[arg(long)]
        toolchain: Option<String>,
    },
    /// Provision, inspect, or remove the managed TeX Live toolchain. The
    /// `install` and `update` actions are the only commands, besides
    /// `refs resolve`, that ever use the network.
    Toolchain {
        #[command(subcommand)]
        action: ToolchainAction,
    },
    /// Rebuild automatically on relevant filesystem changes.
    Watch {
        entry: Option<PathBuf>,
        #[arg(long)]
        theme: Option<String>,
        #[arg(long)]
        tex_only: bool,
        #[arg(long)]
        json: bool,
        /// Where engine executables are located: `auto`, `system`,
        /// `managed`, or a directory.
        #[arg(long)]
        toolchain: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ToolchainAction {
    /// Install a private, pinned TeX Live for the export profile's year
    /// (Linux and macOS): `install-tl --scheme=infraonly` from the frozen
    /// repository, then `tlmgr install` of the pinned closure, into a
    /// user-scoped prefix, transactionally.
    Install {
        /// TeX Live year to provision; defaults to the profile's year.
        #[arg(long)]
        year: Option<String>,
        /// Install into this directory instead of the default user prefix.
        #[arg(long)]
        prefix: Option<PathBuf>,
        /// Never contact the network; install from `--from`.
        #[arg(long, requires = "from")]
        offline: bool,
        /// A relocatable archive produced by `toolchain status --archive`.
        #[arg(long, requires = "offline")]
        from: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Reinstall the pinned closure over an existing managed prefix,
    /// reporting package revision changes. Refuses a prefix whose lock
    /// year differs from the profile.
    Update {
        #[arg(long)]
        prefix: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Report the resolved toolchain source, the managed prefix, its
    /// lock, and whether the lock year matches the profile. Exit `0`
    /// whether or not a managed prefix exists.
    Status {
        #[arg(long)]
        prefix: Option<PathBuf>,
        /// Write a relocatable archive of the managed prefix (with a
        /// checksum manifest) for `install --offline --from`.
        #[arg(long)]
        archive: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Remove the managed prefix. Refuses any directory without the
    /// Terse ownership marker.
    Uninstall {
        #[arg(long)]
        prefix: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum RefsAction {
    /// Resolve declared DOI/arXiv references, updating `references.lock`.
    /// The only command that ever contacts a metadata provider.
    Resolve {
        entry: Option<PathBuf>,
        /// Bypass cached metadata and re-fetch every resolvable alias.
        #[arg(long, conflicts_with = "offline")]
        refresh: bool,
        /// Never contact a provider; only reseal existing records against
        /// current overrides. Fails for anything that would need a fetch.
        #[arg(long)]
        offline: bool,
        /// Remove lock entries for aliases no longer declared in source.
        #[arg(long)]
        prune: bool,
    },
}
