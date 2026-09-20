## Why

Terse is going open source, and its PDF path depends on external tools (`xelatex`, `biber`, TeX packages, TeX Gyre fonts) that every user must install and keep healthy by hand. Discovery is a bare `PATH` lookup (no `.exe` on Windows, no execution, no timeout), a hung engine is reported as `E-LATEX-011 "nonzero status (unknown)"`, and the "pinned" CI environment is Debian's TeX Live 2022 while the arXiv profile says 2025. The concrete trigger: an unattended TeX Live 2025 to 2026 upgrade on the author's machine left `biber` hanging forever on a stale PAR cache, and nothing in Terse could name the cause or the fix. An external user would have no way to diagnose it.

## What Changes

- New `terse doctor [--json] [--fix]`: runs every toolchain check through the bounded process runner with a timeout (never a bare `PATH` lookup), reports status, evidence, probable cause and a per-OS fix command for each, applies only enumerated safe repairs under `--fix` (stale biber PAR cache, macOS quarantine on managed binaries), and finishes with a definitive micro-compile (`xelatex` then `biber` then `xelatex` on an embedded five-line document).
- New `terse toolchain install|update|status|uninstall`: provisions a private, minimal, pinned TeX Live (`install-tl --scheme=infraonly` from the frozen historic `tlnet-final` archive of the profile year, then `tlmgr install` of a derived, pinned package closure) into a user-scoped prefix, transactionally (staging directory, atomic rename, rollback) with a lock file (year, repository, package revisions, checksums, Terse version). `--offline --from <archive>` installs from a previously produced relocatable archive without any network. Linux and macOS in this change; Windows is limited to `doctor` and correct discovery.
- One documented toolchain precedence shared by `build`, `watch`, `export`, `check --target`, and `doctor`: `--toolchain` flag, then manifest `[latex] toolchain`, then the managed prefix when its lock year matches the profile, then `PATH`. Discovery is platform-correct (`PATHEXT`, executable bit). Child processes receive an explicitly prepared environment (Terse-owned `TEXMFHOME`/`TEXMFVAR`/`TEXMFCONFIG`, allowlisted variables only).
- Runner hardening: a timeout is a distinct failure (`E-LATEX-014`) with a remediation hint, timeouts terminate the whole process tree as the spec already requires, and `kpsewhich` goes through the same bounded runner as every other tool.
- Network boundary amendment: package installation and engine downloads remain forbidden in `build`, `check`, `fmt`, `watch`, `export`, and `doctor`; `terse toolchain install|update` is the only new command permitted to use the network, on the same explicit boundary `refs resolve` already has for references.
- Profile alignment: `compiled-profile` in export reports requires verified engine year, packages, and fonts (via `kpsewhich` through the runner), not only the `xelatex --version` banner; the never-emitted `transparent` package leaves the `texlive-2025-xelatex` profile. The dead manifest fields `[latex] engine` and `[latex] pdf` become meaningful (`engine` must match the profile; `pdf` is the default compilation mode when no flag is given).
- Pinned CI by dogfooding: the heavy-test image is built by `terse toolchain install` on a slim Debian base plus `poppler-utils`, replacing the floating `texlive/texlive:latest-full` image and the unpinned Dockerfile; after provisioning, the acceptance suite runs with the network disabled.
- `terse --version` exists; `docs/installation.md` gains per-OS steps and the choice between the managed toolchain and a user-supplied TeX Live 2025 verified by `terse doctor`.
- Correction of record: the `terse` change's task 5.7 claims explicit tool preflight; only an `is_file()` lookup existed. This change supersedes that claim.

Non-goals: switching engines (Tectonic has no native Biber, so it would move the dependency, not remove it); bundling TeX inside the Terse binary; any installation triggered from `build`; MiKTeX support beyond "a system installation on `PATH` works".

## Capabilities

### New Capabilities
- `toolchain`: locating, preparing, diagnosing, and provisioning the external TeX toolchain: resolution precedence, platform-correct discovery, prepared child environment, `doctor` checks and safe fixes, managed installation with lock and offline archive, and the explicit network boundary.

### Modified Capabilities
- `latex-generation`: "Controlled bounded engine execution" gains a distinct timeout diagnostic, process-tree termination as tested behavior, `kpsewhich` through the bounded runner, and the install/download prohibition scoped to the compilation commands; "Explicit PDF compilation modes" resolves the engine through the documented toolchain precedence and honors `[latex] pdf` as the default mode.
- `git-oriented-tooling`: "Public reproducible personal-project workflow" makes the pinned Linux environment a product of `terse toolchain install` and requires per-OS installation documentation and `terse --version`; "Noninteractive CLI and meaningful exit codes" adds `doctor` and `toolchain` with their exit classification.
- `arxiv-export`: "Clean extracted-package validation reports actual coverage" requires verified package and font conditions for `compiled-profile`; "Explicit offline export target" requires the profile package list to equal the emitted set (no `transparent`).
- `multi-file-projects`: "Deterministic project discovery and manifest configuration" adds `[latex] toolchain` and gives `[latex] engine` and `[latex] pdf` defined semantics.
- `diagnostics`: reserves the `E-TOOL-*` code family and `E-LATEX-014`.

## Impact

- Code: new `crates/terse-cli/src/toolchain/` and `crates/terse-cli/src/doctor/`; `crates/terse-cli/src/engine/mod.rs` and `engine/logs.rs` (timeout variant, process groups, streaming runner); `crates/terse-cli/src/build.rs`, `watch/mod.rs`, `export/validate.rs`, `args.rs`, `lib.rs` (resolved toolchain threaded through every engine caller; `RealProcessRunner` no longer restores `PATH` itself); `crates/terse-core/src/toolchain/` (pure profile parsing, PAR cache path, `texlive.tlpdb` ownership mapping) and `crates/terse-core/profiles/` (new `toolchain-texlive-2025.toml`, edited `texlive-2025-xelatex.toml`).
- Dependencies: `tar` added; `flate2` promoted from dev-dependency; `reqwest` reused for streaming downloads with the existing redirect validation.
- Repository: `tests/toolchain/Dockerfile`, `.github/workflows/ci.yml`, `scripts/test-tex.sh`, new `scripts/derive-toolchain-closure.sh`, `docs/installation.md`, `README.md`.
- Tests: new unit, integration, and heavy E2E lanes under `crates/terse-cli/tests/toolchain.rs`, `tests/doctor.rs`, `tests/e2e/toolchain.rs`; the traceability floor in `test_maintainer_can_reproduce_release_checks` rises; new acceptance gate M.
- Users: an explicit one-time download (about 150 MB) into a user-scoped data directory when they choose the managed toolchain; no change for `--tex-only` users.
