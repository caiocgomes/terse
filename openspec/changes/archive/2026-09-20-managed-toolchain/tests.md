## Test Strategy

This is the test contract for the `managed-toolchain` change, written before implementation. It maps every scenario in the six delta specs (one new capability, five modified) to named tests, reusing the conventions of the `terse` change's [tests.md](../terse/tests.md): the standard Rust test runner, `terse-core` unit tests in module `tests.rs` files, `terse-cli` integration tests under `crates/terse-cli/tests/`, and heavy E2E modules declared from `crates/terse-cli/tests/e2e.rs` with explicit ignore reasons. Scenarios inherited unchanged from `terse` keep their existing test names and are marked as such; only new or changed assertions are described.

### Framework and conventions

Fakes already in the codebase are reused, not duplicated: `FakeProcessRunner` (`crates/terse-cli/src/engine/mod.rs`) records every `ProcessInvocation`, including its environment, and returns scripted outcomes; `FakeTransport` and `FakeClock` (`crates/terse-cli/src/references/`) stand in for network and time. Two fakes are added by this change: `ProcessOutcome::timed_out()` on the existing fake, and `FakeArchiveDownloader` (`crates/terse-cli/src/toolchain/download.rs`) scripted by URL, returning bytes or a transport error and counting requests. `HostEnv` (`crates/terse-cli/src/toolchain/host.rs`) is constructed as a literal in tests so no test reads or mutates the process environment.

Engine-free tests never start a real TeX tool: presence is simulated with executable sentinel files in temporary directories, execution with the fake runner. Real-process tests (`test_unix_discovery_requires_executable_bit`, `test_timeout_terminates_process_tree`) use `sh`/`sleep` stand-ins, never TeX, and run only on capable Unix jobs. Heavy E2E tests need the pinned image or an installed managed prefix and are the only tests allowed to invoke real `xelatex`/`biber`; exactly one test, `test_managed_toolchain_provisions_ci_image`, is allowed real network access and runs only in the CI provisioning job.

### Fixtures and observable assertions

New fixtures under `tests/fixtures/toolchain/`: a `fake-prefix/` tree with `bin/<platform>/{xelatex,biber,kpsewhich}` sentinel executables, a valid `terse-toolchain.lock.json`, and the ownership marker; a `tlpdb-excerpt.txt` (thirty lines of real `texlive.tlpdb` records with provenance) for the ownership parser; a `doctor-report.json` golden; an `install-tl.profile` golden. The relocatable archive used by offline tests is produced by `terse toolchain status --archive` in CI and cached, never committed.

Assert observable behavior: recorded invocations (program, arguments, environment, working directory, timeout), files and their hashes before and after, exit codes, stdout/stderr separation, and JSON parsed against the envelope version. Hash every file that must not change (previous prefix, sibling sentinels, published outputs) before the action and compare afterwards.

### Controlled side effects and failure testing

- Timeouts are driven by the fake runner's `timed_out` outcome in engine-free tests and by a real `sh -c 'sleep 30 & wait'` stand-in in the one process-tree test.
- Installation transaction tests inject failures at download (checksum), `install-tl`, `tlmgr`, and publication rename boundaries and assert the previous prefix and lock are byte-identical and no staging directory remains.
- Network-boundary tests wire a denied `FakeArchiveDownloader`/`FakeTransport` into every command and assert zero requests.
- Doctor tests plant a `par-<hex>` directory with a sibling sentinel and assert deletion is exact.

### Execution tiers and red-green workflow

```text
cargo test --workspace --locked
cargo test --locked -p terse-cli --test e2e -- --ignored
```

The first command is engine-free on Linux, macOS, and Windows. The second runs inside the image produced by `terse toolchain install` (Dockerfile in `tests/toolchain/`) with the network disabled after provisioning; it preflights with `terse doctor` and fails if any check fails. `cargo` is not on `PATH` in the author's shell; use `~/.cargo/bin/cargo`. Each task group writes its tests first, demonstrates behavioral Red, then implements. The traceability floor asserted by `test_maintainer_can_reproduce_release_checks` rises by the number of tests added here.

## Spec-to-Test Mapping

### Capability: toolchain

Source: [toolchain spec](specs/toolchain/spec.md).

#### Scenario: Flag, manifest, managed prefix, and PATH are ordered

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/toolchain.rs`
- **Test name**: `test_toolchain_precedence_is_documented`
- **Setup (GIVEN)**: Temporary root with `terse.toml` selecting `toolchain = "system"`, a fake managed prefix whose lock year is the profile year, and a `PATH` directory with an executable `xelatex` sentinel; `HostEnv` literal pointing at both.
- **Action (WHEN)**: Resolve for `--toolchain managed`, for no flag, and for no flag with the manifest line removed; build each with `--tex-only` and with the fake runner.
- **Assert (THEN)**: Resolved `xelatex` paths are managed, `PATH`, managed; each `reason` names its source; `paper.tex`, `terse-style.sty`, and `paper.bib` bytes are identical across the three builds.
- **Edge cases**: `--toolchain DIR` beats a manifest `managed`; a manifest value outside `auto|system|managed|DIR` is `E-CONFIG` exit `2`.

#### Scenario: Managed prefix with a different year is skipped

- **Test type**: unit
- **Test file**: `crates/terse-cli/src/toolchain/tests.rs`
- **Test name**: `test_managed_prefix_year_mismatch_is_skipped`
- **Setup (GIVEN)**: Fake prefix with lock year `2026`, profile year `2025`, `xelatex` on the literal `PATH`.
- **Action (WHEN)**: `resolve(ToolchainSelector::Auto, None, "2025", &host)`.
- **Assert (THEN)**: Source is `System`; `reason` mentions the lock year mismatch.
- **Edge cases**: Integration counterpart `test_toolchain_status_reports_year_match` (`crates/terse-cli/tests/toolchain.rs`) asserts `terse toolchain status` prints the mismatch and exits `0`.

#### Scenario: Explicit selection without an engine fails clearly

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/toolchain.rs`
- **Test name**: `test_explicit_toolchain_without_engine_is_config_error`
- **Setup (GIVEN)**: No managed prefix; a previous build output with recorded hashes.
- **Action (WHEN)**: `terse build --toolchain managed`; `terse build --toolchain <empty dir>`.
- **Assert (THEN)**: Exit `2`; diagnostic in the `E-TOOL-*` family naming `terse toolchain install`; output hashes unchanged.

#### Scenario: Windows extension resolution

- **Test type**: unit
- **Test file**: `crates/terse-cli/src/toolchain/tests.rs`
- **Test name**: `test_windows_pathext_discovery`
- **Setup (GIVEN)**: Temporary directory containing `xelatex.exe`; `HostEnv` literal with `os = Windows` and `pathext = [".COM", ".EXE", ".BAT"]`.
- **Action (WHEN)**: `find_tool_in("xelatex", &[dir], &host)` on every host OS the suite runs on.
- **Assert (THEN)**: Returns the `.exe` path; the fake runner records zero invocations.
- **Edge cases**: A directory named `xelatex` is skipped; empty `PATHEXT` falls back to the documented default.

#### Scenario: Non-executable file is not a tool

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/toolchain.rs`
- **Test name**: `test_unix_discovery_requires_executable_bit`
- **Setup (GIVEN)**: Real temporary directory with a mode `0644` file named `xelatex` and a second directory with a mode `0755` file (Unix only).
- **Action (WHEN)**: Discovery over both directories in that order.
- **Assert (THEN)**: The executable one is returned; with only the first directory, the tool is absent.

#### Scenario: Search-path overrides never leak

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/latex_generation.rs`
- **Test name**: `test_child_env_is_prepared_allowlist`
- **Setup (GIVEN)**: `HostEnv` literal carrying `TEXINPUTS`, `BIBINPUTS`, `TEXMFHOME`, and `TEXMFCNF` values; project with a citation; fake runner.
- **Action (WHEN)**: `build --require-pdf` and `export --target arxiv --require-compile`.
- **Assert (THEN)**: Every recorded invocation's environment contains only the allowlisted keys, contains Terse-owned `TEXMFHOME`/`TEXMFVAR`/`TEXMFCONFIG`, and contains none of the injected override values.
- **Edge cases**: The runner itself adds nothing: an invocation with an empty `env` records an empty environment.

#### Scenario: Managed binaries find each other

- **Test type**: unit
- **Test file**: `crates/terse-cli/src/toolchain/tests.rs`
- **Test name**: `test_managed_bin_dir_leads_child_path`
- **Setup (GIVEN)**: Resolved managed toolchain with `bin_dir`.
- **Action (WHEN)**: `prepare_child_env`.
- **Assert (THEN)**: `PATH` begins with `bin_dir` followed by the host `PATH`; for a `System` toolchain `PATH` is the host value unchanged.

#### Scenario: Hung Biber is diagnosed, not mislabeled

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/doctor.rs`
- **Test name**: `test_doctor_checks_have_timeouts_and_causes`
- **Setup (GIVEN)**: Fake runner scripted: `xelatex --version` succeeds with a 2025 banner, `biber --version` returns `timed_out`; temporary tree hashed.
- **Action (WHEN)**: `terse doctor`.
- **Assert (THEN)**: The `biber.runs` check is `fail`, its probable cause contains `par_cache_dir(tmpdir, username)`, its fix command removes that path, every recorded invocation carried a timeout, exit code `1`, tree hash unchanged.
- **Edge cases**: `test_par_cache_dir_hex_encodes_username` (unit, `crates/terse-core/src/toolchain/tests.rs`) asserts `par_cache_dir("/tmp", "cgomes") == "/tmp/par-63676f6d6573"` and lowercase hex for mixed-case names; a missing `xelatex` yields `fail` on `xelatex.present` and skips the micro-compile with `info`.

#### Scenario: JSON report is a versioned envelope

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/doctor.rs`
- **Test name**: `test_doctor_json_envelope_is_versioned`
- **Setup (GIVEN)**: Same fake runner script; `HostEnv` literals differing in `LANG` and `TERM`.
- **Action (WHEN)**: `terse doctor --json` twice.
- **Assert (THEN)**: Stdout parses as JSON with `version == JSON_ENVELOPE_VERSION`, identical check ids and statuses, stderr carries all prose; matches the `doctor-report.json` golden after normalizing paths.

#### Scenario: Stale PAR cache is removed and nothing else

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/doctor.rs`
- **Test name**: `test_doctor_fix_removes_only_stale_par_cache`
- **Setup (GIVEN)**: Temporary `tmpdir` containing `par-<hex>/cache-x/file` and `sibling.txt` with recorded hash; fake runner scripted `timed_out` then success for Biber.
- **Action (WHEN)**: `terse doctor --fix`.
- **Assert (THEN)**: `par-<hex>` is gone, `sibling.txt` hash unchanged, the report lists the removal, Biber was probed twice, exit code `0`.
- **Edge cases**: E2E `test_doctor_detects_corrupt_par_cache_and_fixes_it` (`crates/terse-cli/tests/e2e/toolchain.rs`) plants the directory against real Biber on the managed prefix.

#### Scenario: Doctor without fix is read-only

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/doctor.rs`
- **Test name**: `test_doctor_fix_never_writes_without_flag`
- **Setup (GIVEN)**: Same planted tree.
- **Action (WHEN)**: `terse doctor`.
- **Assert (THEN)**: Recursive hash of `tmpdir` and of the project root unchanged; report marks the repair as `auto_fixable` but not applied.

#### Scenario: Micro-compile failure is distinct

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/doctor.rs`
- **Test name**: `test_doctor_microcompile_reports_failure_distinctly`
- **Setup (GIVEN)**: Fake runner: all version checks succeed, `kpsewhich` resolves everything, the Biber pass of the micro-compile exits nonzero with a log line.
- **Action (WHEN)**: `terse doctor`.
- **Assert (THEN)**: `compile.micro` is `fail` with that log line as evidence; presence/version checks are `ok`; the recorded micro-compile invocations use stem `paper` and the prepared environment.

#### Scenario: Micro-compile succeeds on the managed prefix

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/toolchain.rs`
- **Test name**: `test_doctor_microcompile_succeeds_on_managed_prefix`
- **Setup (GIVEN)**: Installed managed prefix (pinned image).
- **Action (WHEN)**: `terse doctor --toolchain managed --json`.
- **Assert (THEN)**: No `fail` check; `compile.micro` is `ok`; exit `0`.

#### Scenario: Checksum mismatch stops before execution

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/toolchain.rs`
- **Test name**: `test_install_rejects_checksum_mismatch_before_running_anything`
- **Setup (GIVEN)**: `FakeArchiveDownloader` serving bytes whose SHA-512 differs from the profile pin; fake runner.
- **Action (WHEN)**: `terse toolchain install --prefix <tmp>`.
- **Assert (THEN)**: Exit `3`, `E-TOOL-021`; zero runner invocations; no `<prefix>` and no `.terse-staging-*` sibling.
- **Edge cases**: `test_toolchain_spec_parses_and_pins_sha512` (unit, `crates/terse-core/src/toolchain/tests.rs`) parses the embedded profile and asserts a 128-hex-character pin, a nonempty repository list, and `scheme-infraonly` first in the closure; prerequisites missing (`perl` absent from the literal `PATH`) fail with `E-TOOL-020` and zero downloads.

#### Scenario: Package installation failure leaves the previous prefix

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/toolchain.rs`
- **Test name**: `test_toolchain_install_is_transactional`
- **Setup (GIVEN)**: Fake prefix with lock, hashed recursively; downloader serving a correct archive; fake runner scripted: `install-tl` succeeds, `tlmgr install` exits nonzero.
- **Action (WHEN)**: `terse toolchain update --prefix <that prefix>`.
- **Assert (THEN)**: Exit `3`, `E-TOOL-023`; prefix hash unchanged; no staging sibling remains.
- **Edge cases**: Injected publication rename failure yields `E-TOOL-024` with the same invariants; `test_install_tl_profile_is_portable_and_infraonly` (unit) asserts the generated `install.profile` matches the golden with `scheme-infraonly`, `instopt_portable 1`, `instopt_adjustpath 0`, no doc/src.

#### Scenario: Lock records reproducible pins

- **Test type**: unit
- **Test file**: `crates/terse-cli/src/toolchain/tests.rs`
- **Test name**: `test_toolchain_lock_records_pins`
- **Setup (GIVEN)**: `tlmgr info` fixture output with two packages and revisions.
- **Action (WHEN)**: Build and serialize the lock, then decode it.
- **Assert (THEN)**: Fields in fixed order (version, year, repository, install-tl sha512, packages sorted by name with revisions, terse version, platform); no timestamp key; round trip is identical bytes.

#### Scenario: Every other command is network-free

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/toolchain.rs`
- **Test name**: `test_toolchain_is_only_new_network_path`
- **Setup (GIVEN)**: Project with a managed fake prefix; denied `FakeArchiveDownloader` and denied `FakeTransport` injected through the application entrypoint.
- **Action (WHEN)**: `check`, `build --require-pdf`, `fmt --check`, `watch` (one attempt), `export --target arxiv`, `doctor`, `toolchain status`.
- **Assert (THEN)**: Zero requests recorded on both fakes; each command's exit code is what its own inputs dictate.
- **Edge cases**: Structural check: `grep` proves `RealArchiveDownloader::new` appears only in the toolchain install/update path of `lib.rs`.

#### Scenario: Tampered archive is rejected before rename

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/toolchain.rs`
- **Test name**: `test_toolchain_offline_verifies_checksums`
- **Setup (GIVEN)**: Archive produced by `terse toolchain status --archive` from the fake prefix, then one payload byte flipped.
- **Action (WHEN)**: `terse toolchain install --offline --from <archive> --prefix <tmp>`.
- **Assert (THEN)**: Exit `3`; zero downloader requests; no prefix or staging directory.
- **Edge cases**: `--offline` without `--from` and `--refresh`-style conflicting flags exit `2`; an archive whose lock year differs from the profile is refused before extraction completes.

#### Scenario: Offline install then required PDF build

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/toolchain.rs`
- **Test name**: `test_offline_install_then_build_require_pdf`
- **Setup (GIVEN)**: Cached relocatable archive; fresh prefix path; network disabled.
- **Action (WHEN)**: `terse toolchain install --offline --from <archive> --prefix <tmp>` then `terse build --require-pdf --toolchain <tmp>` on `tests/fixtures/full-paper` for both themes.
- **Assert (THEN)**: Prefix exists with lock and marker; both builds exit `0` and produce PDFs whose extracted text contains the fixture's sentinel prose.

#### Scenario: Uninstall removes only the owned prefix

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/toolchain.rs`
- **Test name**: `test_toolchain_uninstall_removes_only_prefix`
- **Setup (GIVEN)**: Fake owned prefix and a sibling directory with a hashed sentinel.
- **Action (WHEN)**: `terse toolchain uninstall --prefix <prefix>` twice.
- **Assert (THEN)**: Prefix gone after the first run; sibling hash unchanged; second run exits `0` reporting nothing to remove.

#### Scenario: Unowned directory is refused

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/toolchain.rs`
- **Test name**: `test_toolchain_uninstall_refuses_unowned_prefix`
- **Setup (GIVEN)**: Populated directory without the ownership marker.
- **Action (WHEN)**: `terse toolchain uninstall --prefix <dir>`.
- **Assert (THEN)**: Exit `2`; directory hash unchanged.

#### Scenario: Prefix path is absent from published output

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/toolchain.rs`
- **Test name**: `test_managed_prefix_absent_from_artifacts`
- **Setup (GIVEN)**: Fake managed prefix at a distinctive path; fake runner producing a PDF byte stub for engine passes.
- **Action (WHEN)**: `build --toolchain managed` and `export --target arxiv --toolchain managed`.
- **Assert (THEN)**: No published text artifact, `MANIFEST.json`, or ZIP member contains the prefix path; the doctor report and `.terse-cache/` report may.

### Capability: latex-generation

Source: [latex-generation delta](specs/latex-generation/spec.md).

#### Scenario: Source generation without installed TeX

- Unchanged from `terse`: `test_tex_only_starts_no_processes` (`crates/terse-cli/tests/latex_generation.rs`).

#### Scenario: Auto mode and required PDF differ

- Unchanged from `terse`: `test_auto_vs_required_pdf_without_engine`; its warning text now names the attempted resolution (assertion added).

#### Scenario: Manifest PDF mode is the default

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/latex_generation.rs`
- **Test name**: `test_manifest_pdf_mode_is_default`
- **Setup (GIVEN)**: Manifest `[latex] pdf = "require-pdf"`; no engine on the literal `PATH`; fake runner.
- **Action (WHEN)**: `build`, then `build --tex-only`.
- **Assert (THEN)**: First exits `3` with `E-LATEX-002`; second exits `0` with zero invocations.
- **Edge cases**: `pdf = "tex-only"` plus `--require-pdf` compiles (flag wins); an unknown `pdf` value is `E-CONFIG` exit `2`.

#### Scenario: Path is not a shell command

- Unchanged from `terse`: `test_subprocess_arguments_are_not_shell_text`.

#### Scenario: Compilation does not converge

- Unchanged from `terse`: `test_engine_pass_limit_is_enforced`.

#### Scenario: Timeout is distinct from a crash

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/latex_generation.rs`
- **Test name**: `test_timeout_is_distinct_from_crash`
- **Setup (GIVEN)**: Previous good output hashed; fake runner A returns `timed_out` on the first pass; fake runner B returns nonzero exit.
- **Action (WHEN)**: `build --require-pdf` with each.
- **Assert (THEN)**: A reports `E-LATEX-014` mentioning the limit in seconds and `terse doctor`; B reports `E-LATEX-011`; both exit `3`; output hashes unchanged.

#### Scenario: Timeout kills the whole tree

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/latex_generation.rs`
- **Test name**: `test_timeout_terminates_process_tree`
- **Setup (GIVEN)**: Unix only; a script that spawns `sleep 30` in the background and waits; `RealProcessRunner` with a 1 s timeout.
- **Action (WHEN)**: Run the invocation.
- **Assert (THEN)**: Outcome is `timed_out`; within a bounded deadline neither the script nor the `sleep` process exists (checked by pid and process group).
- **Edge cases**: `test_kpsewhich_uses_bounded_runner` (integration, `crates/terse-cli/tests/arxiv_export.rs`) asserts every `kpsewhich` call during export validation is recorded by the fake runner with a timeout and the prepared environment.

### Capability: git-oriented-tooling

Source: [git-oriented-tooling delta](specs/git-oriented-tooling/spec.md).

#### Scenario: CI observes failure categories

- Unchanged from `terse`: `test_exit_codes_and_json_are_meaningful`.

#### Scenario: Toolchain commands classify their failures

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/git_oriented_tooling.rs`
- **Test name**: `test_toolchain_commands_classify_exit_codes`
- **Setup (GIVEN)**: Fake runner with hung Biber; no managed prefix; downloader with a bad checksum.
- **Action (WHEN)**: `doctor`; `build --toolchain managed`; `toolchain install`.
- **Assert (THEN)**: Exit codes `1`, `2`, `3`; JSON mode yields parseable output for each.

#### Scenario: Version flag

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/git_oriented_tooling.rs`
- **Test name**: `test_version_flag_prints_cargo_version`
- **Setup (GIVEN)**: The built binary.
- **Action (WHEN)**: `terse --version`.
- **Assert (THEN)**: Stdout equals `terse <CARGO_PKG_VERSION>` with a trailing newline; exit `0`.

#### Scenario: A contributor reproduces the demonstration

- Unchanged from `terse`: `test_public_checkout_full_user_workflow`.

#### Scenario: One maintainer can run release validation

- **Test type**: e2e
- **Test file**: `crates/terse-cli/tests/e2e/git_oriented_tooling.rs`
- **Test name**: `test_maintainer_can_reproduce_release_checks` (existing; expectations changed)
- **Setup (GIVEN)**: Pinned image built by `terse toolchain install`; network disabled.
- **Action (WHEN)**: Run the documented scripts.
- **Assert (THEN)**: `scripts/test-tex.sh` preflights with `terse doctor` and fails on any `fail` check; discovered test counts meet the raised floor; the heavy lane runs every gate A–M test.

#### Scenario: A user without TeX reaches a PDF with documented commands only

- **Test type**: e2e (CI provisioning job; the only test with real network)
- **Test file**: `crates/terse-cli/tests/e2e/toolchain.rs`
- **Test name**: `test_managed_toolchain_provisions_ci_image`
- **Setup (GIVEN)**: Slim Debian with the documented prerequisites and no TeX; the built `terse` binary.
- **Action (WHEN)**: `terse toolchain install`, `terse doctor`, `terse build --require-pdf` on the fixture; then `terse toolchain status --archive` to produce the cached archive.
- **Assert (THEN)**: Each exits `0`; the lock matches the committed profile year and closure; the PDF text contains the fixture sentinel.
- **Edge cases**: `test_pinned_closure_covers_full_paper_inputs` (e2e, same file) compiles the fixture under both themes with `-recorder`, maps every `INPUT` under `TEXMFDIST` through `owning_packages` against the prefix's `texlive.tlpdb`, and asserts the owners are a subset of the closure, printing any missing package name; `test_owning_packages_maps_runfiles_to_package` (unit, `crates/terse-core/src/toolchain/tests.rs`) checks the parser on the excerpt fixture.

### Capability: arxiv-export

Source: [arxiv-export delta](specs/arxiv-export/spec.md).

#### Scenario: Current source controls export

- Unchanged from `terse`: `test_export_validates_current_sources`.

#### Scenario: Profile packages equal the emitted set

- **Test type**: unit
- **Test file**: `crates/terse-core/src/artifact/tests.rs`
- **Test name**: `test_profile_packages_equal_emitted_set`
- **Setup (GIVEN)**: The embedded `texlive-2025-xelatex` profile and the style generator's exported package registry (always-loaded, conditional, built-in).
- **Action (WHEN)**: Compare the two sets.
- **Assert (THEN)**: Equal; `transparent` absent from both.

#### Scenario: Local toolchain differs from target profile

- Unchanged from `terse`: `test_local_compile_is_not_claimed_as_profile_match`.

#### Scenario: No compatible engine is available

- Unchanged from `terse`: `test_no_engine_reports_static_only_or_fails_required`.

#### Scenario: Clean compilation uncovers a hidden dependency

- Unchanged from `terse`: `test_recorder_detects_hidden_dependency`; `distribution_roots` now come through the fake runner (see `test_kpsewhich_uses_bounded_runner`).

#### Scenario: Banner year alone is not a profile match

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/arxiv_export.rs`
- **Test name**: `test_compiled_profile_requires_verified_packages`
- **Setup (GIVEN)**: Fake runner: `xelatex --version` banner `TeX Live 2025`; `kpsewhich` succeeds for every package and font except `texgyrepagella-regular.otf`; compile passes succeed.
- **Action (WHEN)**: `export --target arxiv --require-compile`.
- **Assert (THEN)**: Report is `compiled-local`; `tested_assumptions` names the missing font; with all lookups succeeding the same run reports `compiled-profile`.

### Capability: multi-file-projects

Source: [multi-file-projects delta](specs/multi-file-projects/spec.md).

#### Scenario: Entry discovery from a subdirectory

- Unchanged from `terse`: `test_manifest_discovery_from_descendant`.

#### Scenario: Missing or invalid manifest

- Unchanged from `terse`: `test_missing_or_invalid_manifest_fails`.

#### Scenario: Explicit settings win without environment drift

- Unchanged from `terse`: `test_cli_precedence_is_environment_independent`; adds `[latex] toolchain` permutations to its table.

#### Scenario: Engine field must agree with the profile

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/multi_file_projects.rs`
- **Test name**: `test_engine_field_disagreeing_with_profile_is_config_error`
- **Setup (GIVEN)**: Manifest with `[latex] engine = "pdflatex"`.
- **Action (WHEN)**: `check`, `build --tex-only`, `doctor`.
- **Assert (THEN)**: Each exits `2` with `E-CONFIG-008` naming `pdflatex` and `xelatex`; `engine = "xelatex"` and an absent field both pass.

### Capability: diagnostics

Source: [diagnostics delta](specs/diagnostics/spec.md).

#### Scenario: Strict raw warning remains recognizable

- Unchanged from `terse`: `test_deny_warnings_does_not_rename_code`.

#### Scenario: Toolchain codes are stable and distinct

- **Test type**: integration
- **Test file**: `crates/terse-cli/tests/diagnostics.rs`
- **Test name**: `test_toolchain_codes_are_stable_and_distinct`
- **Setup (GIVEN)**: Fake runner producing a timeout in one build and a hung Biber in one doctor run.
- **Action (WHEN)**: Render both in JSON.
- **Assert (THEN)**: Codes are `E-LATEX-014` and an `E-TOOL-0xx` code; the set of all codes emitted by the suite's diagnostics module contains no duplicates across families (table-driven over `engine/logs.rs` and `doctor/report.rs` code constants).

## Coverage Summary

### Acceptance gate M

| Gate | Primary tests | Evidence |
| --- | --- | --- |
| M | `test_managed_toolchain_provisions_ci_image`; `test_toolchain_install_is_transactional`; `test_doctor_microcompile_succeeds_on_managed_prefix` | The pinned Linux image is produced by `terse toolchain install` from the committed profile, the install is atomic under injected failures, and with the network disabled every A–L case plus the doctor micro-compile passes on that prefix. |

Gates A–L from `terse` remain required and run on the image this change produces.

### Every spec scenario

| Capability | Scenario | Test file | Test name | Type |
|------------|----------|-----------|-----------|------|
| toolchain | Flag, manifest, managed prefix, and PATH are ordered | `crates/terse-cli/tests/toolchain.rs` | `test_toolchain_precedence_is_documented` | integration |
| toolchain | Managed prefix with a different year is skipped | `crates/terse-cli/src/toolchain/tests.rs` | `test_managed_prefix_year_mismatch_is_skipped` | unit |
| toolchain | Explicit selection without an engine fails clearly | `crates/terse-cli/tests/toolchain.rs` | `test_explicit_toolchain_without_engine_is_config_error` | integration |
| toolchain | Windows extension resolution | `crates/terse-cli/src/toolchain/tests.rs` | `test_windows_pathext_discovery` | unit |
| toolchain | Non-executable file is not a tool | `crates/terse-cli/tests/toolchain.rs` | `test_unix_discovery_requires_executable_bit` | integration |
| toolchain | Search-path overrides never leak | `crates/terse-cli/tests/latex_generation.rs` | `test_child_env_is_prepared_allowlist` | integration |
| toolchain | Managed binaries find each other | `crates/terse-cli/src/toolchain/tests.rs` | `test_managed_bin_dir_leads_child_path` | unit |
| toolchain | Hung Biber is diagnosed, not mislabeled | `crates/terse-cli/tests/doctor.rs` | `test_doctor_checks_have_timeouts_and_causes` | integration |
| toolchain | JSON report is a versioned envelope | `crates/terse-cli/tests/doctor.rs` | `test_doctor_json_envelope_is_versioned` | integration |
| toolchain | Stale PAR cache is removed and nothing else | `crates/terse-cli/tests/doctor.rs` | `test_doctor_fix_removes_only_stale_par_cache` | integration |
| toolchain | Doctor without fix is read-only | `crates/terse-cli/tests/doctor.rs` | `test_doctor_fix_never_writes_without_flag` | integration |
| toolchain | Micro-compile failure is distinct | `crates/terse-cli/tests/doctor.rs` | `test_doctor_microcompile_reports_failure_distinctly` | integration |
| toolchain | Micro-compile succeeds on the managed prefix | `crates/terse-cli/tests/e2e/toolchain.rs` | `test_doctor_microcompile_succeeds_on_managed_prefix` | e2e |
| toolchain | Checksum mismatch stops before execution | `crates/terse-cli/tests/toolchain.rs` | `test_install_rejects_checksum_mismatch_before_running_anything` | integration |
| toolchain | Package installation failure leaves the previous prefix | `crates/terse-cli/tests/toolchain.rs` | `test_toolchain_install_is_transactional` | integration |
| toolchain | Lock records reproducible pins | `crates/terse-cli/src/toolchain/tests.rs` | `test_toolchain_lock_records_pins` | unit |
| toolchain | Every other command is network-free | `crates/terse-cli/tests/toolchain.rs` | `test_toolchain_is_only_new_network_path` | integration |
| toolchain | Tampered archive is rejected before rename | `crates/terse-cli/tests/toolchain.rs` | `test_toolchain_offline_verifies_checksums` | integration |
| toolchain | Offline install then required PDF build | `crates/terse-cli/tests/e2e/toolchain.rs` | `test_offline_install_then_build_require_pdf` | e2e |
| toolchain | Uninstall removes only the owned prefix | `crates/terse-cli/tests/toolchain.rs` | `test_toolchain_uninstall_removes_only_prefix` | integration |
| toolchain | Unowned directory is refused | `crates/terse-cli/tests/toolchain.rs` | `test_toolchain_uninstall_refuses_unowned_prefix` | integration |
| toolchain | Prefix path is absent from published output | `crates/terse-cli/tests/toolchain.rs` | `test_managed_prefix_absent_from_artifacts` | integration |
| latex-generation | Source generation without installed TeX | `crates/terse-cli/tests/latex_generation.rs` | `test_tex_only_starts_no_processes` | integration |
| latex-generation | Auto mode and required PDF differ | `crates/terse-cli/tests/latex_generation.rs` | `test_auto_vs_required_pdf_without_engine` | integration |
| latex-generation | Manifest PDF mode is the default | `crates/terse-cli/tests/latex_generation.rs` | `test_manifest_pdf_mode_is_default` | integration |
| latex-generation | Path is not a shell command | `crates/terse-cli/tests/latex_generation.rs` | `test_subprocess_arguments_are_not_shell_text` | integration |
| latex-generation | Compilation does not converge | `crates/terse-cli/tests/latex_generation.rs` | `test_engine_pass_limit_is_enforced` | integration |
| latex-generation | Timeout is distinct from a crash | `crates/terse-cli/tests/latex_generation.rs` | `test_timeout_is_distinct_from_crash` | integration |
| latex-generation | Timeout kills the whole tree | `crates/terse-cli/tests/latex_generation.rs` | `test_timeout_terminates_process_tree` | integration |
| git-oriented-tooling | CI observes failure categories | `crates/terse-cli/tests/git_oriented_tooling.rs` | `test_exit_codes_and_json_are_meaningful` | integration |
| git-oriented-tooling | Toolchain commands classify their failures | `crates/terse-cli/tests/git_oriented_tooling.rs` | `test_toolchain_commands_classify_exit_codes` | integration |
| git-oriented-tooling | Version flag | `crates/terse-cli/tests/git_oriented_tooling.rs` | `test_version_flag_prints_cargo_version` | integration |
| git-oriented-tooling | A contributor reproduces the demonstration | `crates/terse-cli/tests/e2e/git_oriented_tooling.rs` | `test_public_checkout_full_user_workflow` | e2e |
| git-oriented-tooling | One maintainer can run release validation | `crates/terse-cli/tests/e2e/git_oriented_tooling.rs` | `test_maintainer_can_reproduce_release_checks` | e2e |
| git-oriented-tooling | A user without TeX reaches a PDF with documented commands only | `crates/terse-cli/tests/e2e/toolchain.rs` | `test_managed_toolchain_provisions_ci_image` | e2e |
| arxiv-export | Current source controls export | `crates/terse-cli/tests/arxiv_export.rs` | `test_export_validates_current_sources` | integration |
| arxiv-export | Profile packages equal the emitted set | `crates/terse-core/src/artifact/tests.rs` | `test_profile_packages_equal_emitted_set` | unit |
| arxiv-export | Local toolchain differs from target profile | `crates/terse-cli/tests/arxiv_export.rs` | `test_local_compile_is_not_claimed_as_profile_match` | integration |
| arxiv-export | No compatible engine is available | `crates/terse-cli/tests/arxiv_export.rs` | `test_no_engine_reports_static_only_or_fails_required` | integration |
| arxiv-export | Clean compilation uncovers a hidden dependency | `crates/terse-cli/tests/arxiv_export.rs` | `test_recorder_detects_hidden_dependency` | integration |
| arxiv-export | Banner year alone is not a profile match | `crates/terse-cli/tests/arxiv_export.rs` | `test_compiled_profile_requires_verified_packages` | integration |
| multi-file-projects | Entry discovery from a subdirectory | `crates/terse-cli/tests/multi_file_projects.rs` | `test_manifest_discovery_from_descendant` | integration |
| multi-file-projects | Missing or invalid manifest | `crates/terse-cli/tests/multi_file_projects.rs` | `test_missing_or_invalid_manifest_fails` | integration |
| multi-file-projects | Explicit settings win without environment drift | `crates/terse-cli/tests/multi_file_projects.rs` | `test_cli_precedence_is_environment_independent` | integration |
| multi-file-projects | Engine field must agree with the profile | `crates/terse-cli/tests/multi_file_projects.rs` | `test_engine_field_disagreeing_with_profile_is_config_error` | integration |
| diagnostics | Strict raw warning remains recognizable | `crates/terse-cli/tests/diagnostics.rs` | `test_deny_warnings_does_not_rename_code` | integration |
| diagnostics | Toolchain codes are stable and distinct | `crates/terse-cli/tests/diagnostics.rs` | `test_toolchain_codes_are_stable_and_distinct` | integration |

Supporting tests named as edge cases above: `test_toolchain_status_reports_year_match`, `test_par_cache_dir_hex_encodes_username`, `test_doctor_detects_corrupt_par_cache_and_fixes_it`, `test_toolchain_spec_parses_and_pins_sha512`, `test_install_tl_profile_is_portable_and_infraonly`, `test_kpsewhich_uses_bounded_runner`, `test_pinned_closure_covers_full_paper_inputs`, `test_owning_packages_maps_runfiles_to_package`.

### Coverage maintenance

New tests: 29 primary plus 8 supporting (10 unit, 21 integration, 6 E2E). Every scenario in the six delta specs maps to at least one test above; the five scenarios inherited unchanged keep their `terse` tests. `crates/terse-cli/tests/e2e.rs` must declare `toolchain.rs`; the discovery floor in `test_maintainer_can_reproduce_release_checks` rises accordingly. Only `test_managed_toolchain_provisions_ci_image` may use the network, and only in the CI provisioning job.
