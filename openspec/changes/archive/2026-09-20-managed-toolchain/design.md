## Context

Terse compiles a small structured language into portable LaTeX and, optionally, into a PDF through XeLaTeX and Biber. The `terse` change (ten capabilities, 150 of 156 tasks closed) treats the TeX toolchain as an externally installed prerequisite located by a bare `PATH` lookup. Four facts make that insufficient for an open-source release:

- `engine::find_tool` searches `PATH` for an exact filename, so `xelatex.exe` is never found on Windows although the release matrix publishes a Windows executable; nothing is executed before a build, so a present-but-broken tool is only discovered mid-build.
- `RealProcessRunner` kills only the direct child on timeout and reports it as `E-LATEX-011` (nonzero exit, unknown status). The `terse` design and the latex-generation spec already require process-tree termination.
- `kpsewhich` is spawned outside the runner without a timeout during export validation, and `compiled-profile` is granted on the `xelatex --version` banner year alone, which the arxiv-export spec forbids.
- The pinned CI environment is Debian bookworm's TeX Live 2022 while the profile says 2025, and the heavy lane runs on a floating `texlive:latest-full` image.

The triggering incident: an unattended TeX Live 2025 to 2026 upgrade on the author's machine left `biber` (a PAR-packed Perl executable) hanging on a stale unpack cache at `$TMPDIR/par-<hex(username)>/`. Even `biber --version` never returned. Terse reported an unknown nonzero status. Deleting the cache directory fixed it in fourteen seconds. That failure class is computable, detectable with a timeout, and repairable without touching any TeX tree.

Constraints carried over from `terse`: `terse-core` stays effect-free; generated text artifacts are toolchain-independent; the only network path today is `refs resolve`; publication is transactional through `publication.rs`; diagnostics use stable code families and a versioned JSON envelope; tests use injectable `ProcessRunner`, transport, and clock fakes.

Decisions already taken with the author: provide both `doctor` and a managed installation; managed installation targets Linux and macOS in this change, Windows keeps `doctor` and correct discovery against a system installation.

## Goals / Non-Goals

**Goals:**
- A user with no TeX installation reaches a PDF with documented commands only: source install, `terse toolchain install`, `terse doctor`, `terse build --require-pdf`.
- A user with a system TeX Live gets a precise diagnosis of what is wrong and the command that fixes it, and safe repairs applied on request.
- Every command that starts a TeX process resolves tools the same way, with a stated reason, and never lets tool location change generated text.
- Timeouts are distinct, kill the whole tree, and point at `doctor`.
- The pinned acceptance environment is produced by the same command users run, from a committed profile, and the acceptance suite runs in it offline.

**Non-Goals:**
- Replacing XeLaTeX/Biber with Tectonic: Tectonic has no native Biber, so the dependency would move, not disappear, and the citation capability depends on BibLaTeX/Biber.
- Bundling TeX inside the Terse binary, or installing anything from `build`, `check`, `fmt`, `watch`, `export`, or `doctor`.
- Managed installation on Windows, MiKTeX-specific support, or GPG verification of TeX Live containers (`tlmgr` verifies its own containers; the `install-tl` archive is pinned by SHA-512).
- Reimplementing `tlmgr`.

## Decisions

**Anchor on TeX Live 2025 from the frozen historic archive.** arXiv compiles with TeX Live 2025 by default. `install-tl` from the main mirrors installs only the current year (2026), which is exactly the unattended upgrade that broke the author's machine. The managed installer therefore uses the profile year and the historic `tlnet-final` repository for that year, so a managed prefix is reproducible and matches the export profile. Alternative considered: track the current year and update the profile yearly; rejected because it makes `compiled-profile` a moving target and reintroduces upgrade drift.

**One locator, one precedence.** `crates/terse-cli/src/toolchain/mod.rs` resolves `ToolchainSelector {Auto, System, Managed, Dir}` into `ResolvedToolchain {source, reason, bin_dir, xelatex, biber, kpsewhich, lock}`. Precedence is flag, manifest `[latex] toolchain`, managed prefix when `lock.year == profile.year`, then `PATH`, mirroring the existing rule "flags, then manifest, then defaults". No environment variable is introduced: `--toolchain DIR` and `--prefix` cover tests and CI, so the `terse` rule that environment variables never change semantics stays intact. Alternative considered: `TERSE_TOOLCHAIN_DIR`; rejected to avoid reopening that rule.

**The runner applies exactly the prepared environment.** `toolchain/env.rs::prepare_child_env` builds the allowlist (`PATH` with the managed `bin` first, `HOME`/`USERPROFILE`, `TMPDIR`/`TEMP`/`TMP`, `SYSTEMROOT`, `LANG=C.UTF-8`, Terse-owned `TEXMFHOME`/`TEXMFVAR`/`TEXMFCONFIG`). `RealProcessRunner` becomes `env_clear()` plus `invocation.env` and no longer restores `PATH` itself, so the environment has one owner. Every existing `ProcessInvocation` constructor (XeLaTeX, Biber, `kpsewhich`, watch) migrates to the prepared list; this is a task, not a side effect. `HostEnv::capture()` in `toolchain/host.rs` is the single place that reads `std::env`, and tests build `HostEnv` literals.

**Platform-correct discovery without execution.** `find_tool_in(name, dirs, host)` tries `PATHEXT` extensions on Windows and requires the executable bit on Unix. `engine::find_tool` becomes a thin wrapper until all callers migrate.

**Timeouts are a distinct failure and kill the tree.** `ProcessOutcome.timed_out` feeds `CompileFailure::Timeout {kind, secs}`, rendered as `E-LATEX-014` with the hint to run `terse doctor`. On Unix the child is spawned in its own process group (`Command::process_group(0)`, in std since Rust 1.64, within the pinned `rust-version`) and the group is killed with `SIGKILL` through the same `extern "C"` pattern already used for `signal` in `lib.rs`, so no `libc` dependency is added; on Windows `taskkill /T /F /PID` runs first and `kill()` remains the fallback. Consequence: children no longer receive the terminal's SIGINT, so the watch interrupt path must kill the group; `test_interrupt_stops_owned_processes` guards it. `known_distribution_roots()` becomes `distribution_roots(runner, kpsewhich, env)` with a 10 s timeout per variable, computed once per validation.

**Doctor is a list of runner invocations with timeouts.** `crates/terse-cli/src/doctor/` produces `DoctorReport {version, terse_version, toolchain, checks}` where each `Check` carries id, status, evidence, probable cause, fix command, code, and `auto_fixable`. Fonts are checked with `kpsewhich` on a representative file per font package (`texgyreheros-regular.otf`, `texgyrepagella-regular.otf`, `lmroman10-regular.otf`, `LibertinusSerif-Regular.otf`) rather than `fc-list`, because the generated style loads TeX Gyre by filename through kpathsea. The stale PAR cache path is a pure function in `terse-core` (`toolchain/par.rs`: `tmpdir/par-<lowercase hex of username>`), so it is computed without running Biber. `--fix` is an enumerated allowlist: remove that directory; clear `com.apple.quarantine` on managed binaries. The micro-compile reuses `build::compile_pdf` with `needs_biber = true` on an embedded `paper.tex`/`paper.bib` (the stem is fixed by `compile_pdf_in`). JSON is the serde form of `DoctorReport` with the shared envelope version; checks have no source positions, so they are not forced into `JsonDiagnostic`.

**Managed installation shells out to `install-tl` and `tlmgr`, transactionally.** `crates/terse-core/profiles/toolchain-texlive-2025.toml` (embedded like the export profile; a separate file because "what arXiv provides" and "how to provision it" change on different schedules) pins the year, the repository list, the `install-tl` archive URL and SHA-512, and the package closure, with entries the recorder cannot observe (`biber`, `hyphen-*`, `xetex`/`latex-bin` for formats) marked separately. Flow: reject unsupported platforms; verify prerequisites before any byte is downloaded (`perl`, and `wget` or `curl` because `install-tl`/`tlmgr` fall back to them when `libwww-perl` is absent, plus `tar`/`xz`); download to a same-filesystem staging sibling; verify SHA-512 before executing anything; extract with `tar`+`flate2`, rejecting traversal like `export::validate::extract_zip`; write an `install.profile` with `scheme-infraonly`, `instopt_portable 1` (relocatable, so the final rename is safe), `instopt_adjustpath 0`, no docs or sources; run `install-tl -no-gui -profile ... -repository <repo>` (10 min) and `tlmgr --repository <repo> install <closure>` (30 min) through a streaming runner method that tees progress to stderr; fill the lock from `tlmgr info --only-installed --data name,revision`; write the lock and the ownership marker; publish through `publication::publish` for the atomic rename, backup, and journal already proven for build output. Any failure removes staging and leaves a previous prefix untouched. Downloads use a new `ArchiveDownloader` (streaming to file, 64 MiB bound, redirects disabled, `transport::validate_hop` per hop) rather than `MetadataTransport`, whose in-memory 5 MiB bound `refs resolve` depends on. `RealArchiveDownloader` is constructed only inside the toolchain install/update path in `lib.rs`, exactly as `RealTransport::new()` exists only in `run_refs_resolve`.

**The package closure is derived, then pinned.** A seed list is visibly incomplete (`l3backend`, `libertinus-otf`, and the dozen small packages `hyperref` pulls were missing from the first draft). `scripts/derive-toolchain-closure.sh` installs `infraonly`, compiles `tests/fixtures/full-paper` under both themes with `-recorder`, maps every `INPUT` under `TEXMFDIST` through the pure `terse-core` `toolchain/tlpdb.rs::owning_packages` (a parser of `texlive.tlpdb` `name`/`runfiles` records), and emits the list that is pinned. The heavy test `test_pinned_closure_covers_full_paper_inputs` repeats that mapping after installation and fails naming any package the closure lacks, so drift between `generate_style` and the closure is a red test rather than a user report.

**Offline installation is a relocatable archive of an installed prefix.** `tlmgr` has no download-only mode that would let Terse build a local mirror, so `toolchain status --archive` produces the tarball CI caches and `install --offline --from` consumes, verifying the archived lock's year, platform, and checksums before the rename.

**Profile alignment.** `compiled-profile` requires `probe::verify_profile` (year plus every package, font, and `.ldf` resolvable through `kpsewhich` via the runner); otherwise `compiled-local` records the unverified items. `transparent` leaves the export profile because `latex/mod.rs` documents it is never emitted under XeLaTeX. `[latex] engine` is validated against `profile.engine` (`E-CONFIG-008`) and `[latex] pdf` becomes the default compilation mode; a manifest field that is parsed and ignored is worse than none.

**CI by dogfooding.** `tests/toolchain/Dockerfile` becomes a two-stage build: compile `terse` on a Rust image, then install it on `debian:bookworm-slim` with `perl wget xz-utils ca-certificates poppler-utils fontconfig` and run `terse toolchain install`. `ci.yml` builds that image keyed by the hash of the Dockerfile and the toolchain profile, caches the relocatable archive once, and runs the heavy suite with the network disabled. Only the image build and the archive job touch the network.

Amended `terse` design passages (carried into the archived design): the environment paragraph gains "The toolchain used for optional PDF compilation is located by the documented precedence (`--toolchain`, `[latex] toolchain`, the managed prefix, then `PATH`); its location never affects generated `.tex`/`.bib` bytes." The engine paragraph becomes "No package installation, remote bibliography sources, or engine downloads occur in `build`, `check`, `fmt`, `watch`, or `export`. Provisioning a TeX distribution is the explicit `terse toolchain` command, which shares the explicit-network boundary of `refs resolve` and is the only other command permitted to use the network." The installation paragraph becomes "A compatible TeX distribution/Biber is a documented prerequisite for PDFs, satisfied either by the user's system installation or by the explicit `terse toolchain install` command; it is never a hidden step in `terse build`." This change also supersedes the `terse` task 5.7 claim of explicit tool preflight; only an `is_file()` lookup existed.

## Risks / Trade-offs

- [Historic `tlnet-final` mirrors are slow or disappear] -> pinned mirror list in the profile; the committed lock and the cached relocatable archive keep CI and offline installs working; a mirror change is a profile edit, not a code change.
- [`perl` or a downloader is missing on the host] -> checked before any download with a per-OS install command; macOS `perl` is checked, not assumed, because Apple has deprecated bundled scripting runtimes.
- [Portable prefix cannot resolve fonts without fontconfig] -> the doctor micro-compile and gate M test exactly this; if it fails, the prefix ships its own `TEXMFSYSVAR/fonts/conf`, added as a task in the install group.
- [Process groups change interrupt semantics] -> the watch interrupt path kills the group; `test_interrupt_stops_owned_processes` and `test_timeout_terminates_process_tree` guard both directions.
- [Removing the runner's `PATH` restore breaks a caller that was never migrated] -> the migration is an explicit task and `test_child_env_is_prepared_allowlist` records every invocation's environment.
- [Download size and disk] -> about 150 MB downloaded, 300 to 500 MB installed; users who never build PDFs pay nothing; `disk.space` is a doctor check.
- [The pinned closure drifts from `generate_style`] -> `test_profile_packages_equal_emitted_set` (fast) and `test_pinned_closure_covers_full_paper_inputs` (heavy) fail naming the package.
- [SHA-512 without GPG] -> the checksum is pinned in a versioned file and verified before execution; `tlmgr` verifies package containers itself; GPG is left out because it cannot be verified offline without shipping a keyring.

## Migration Plan

1. Sync the `terse` delta specs into `openspec/specs/` so the MODIFIED deltas here have targets; `terse` stays open.
2. Land groups in order: resolution layer and runner hardening (behavior-preserving for users on `PATH`), doctor, managed installation, profile/CI/docs, gate M.
3. Archive `managed-toolchain`; then close `terse` tasks 26.7/26.8 on the managed image, tick its 1.2 to 1.4 bookkeeping, and archive `terse` with `--skip-specs`.
4. Rollback: every group is additive behind new commands or flags except the runner environment change; reverting that group restores the previous `PATH`-restoring runner.

## Open Questions

- Which `tlnet-final` mirrors to pin beyond the Utah primary: resolved when the closure derivation script first runs and records the mirror it used.
- Whether `libertinus-otf` needs fontconfig on the portable prefix: resolved by the micro-compile in group 3.
