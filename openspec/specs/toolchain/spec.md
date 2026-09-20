# toolchain Specification

## Purpose
TBD - created by archiving change managed-toolchain. Update Purpose after archive.
## Requirements
### Requirement: One documented toolchain resolution precedence
Every command that may start a TeX process (`build`, `watch`, `export`, `check --target`, `doctor`) SHALL resolve the toolchain through one shared locator with the precedence: explicit `--toolchain <auto|system|managed|DIR>` flag, then manifest `[latex] toolchain`, then the managed prefix when it is installed and its lock year equals the profile year, then `PATH`. The resolved location SHALL be reported with the reason it was chosen. The resolution MUST NOT affect generated text artifacts: `.tex`, `.sty`, `.bib`, maps, and manifests SHALL be byte-identical for the same inputs under every resolution. An explicit `DIR` or `managed` selection that does not contain `xelatex` MUST fail as a configuration error with a hint naming `terse toolchain install`.

#### Scenario: Flag, manifest, managed prefix, and PATH are ordered
- **GIVEN** a project whose manifest selects `system`, an installed managed prefix matching the profile year, and `xelatex` on `PATH`
- **WHEN** `build --toolchain managed`, plain `build`, and `build` with the manifest line removed each resolve tools
- **THEN** they choose the managed prefix, the `PATH` executable, and the managed prefix respectively, each reporting its reason, and the generated text artifacts are byte-identical across all three

#### Scenario: Managed prefix with a different year is skipped
- **GIVEN** a managed prefix whose lock records TeX Live 2026 and a profile anchored to 2025
- **WHEN** automatic resolution runs
- **THEN** the managed prefix is skipped with a stated reason and `PATH` is used

#### Scenario: Explicit selection without an engine fails clearly
- **WHEN** `build --toolchain managed` runs with no managed prefix installed
- **THEN** the command exits `2` with a diagnostic that names `terse toolchain install` and touches no output

### Requirement: Platform-correct executable discovery
Discovery SHALL locate executables without running them. On Windows it SHALL try the bare name and each `PATHEXT` extension (defaulting to `.EXE;.CMD;.BAT`). On Unix a candidate MUST be a regular file with an executable bit; a plain file named like the tool MUST be skipped. Directories named like the tool MUST be skipped on every platform.

#### Scenario: Windows extension resolution
- **GIVEN** a directory containing `xelatex.exe` and `PATHEXT=.COM;.EXE;.BAT`
- **WHEN** discovery searches for `xelatex` with Windows semantics
- **THEN** it returns the `.exe` path without spawning any process

#### Scenario: Non-executable file is not a tool
- **GIVEN** a `PATH` entry containing a non-executable regular file named `xelatex`
- **WHEN** discovery searches on Unix
- **THEN** the file is skipped and discovery reports the tool as absent

### Requirement: Prepared child environment
Every TeX, Biber, `kpsewhich`, `install-tl`, and `tlmgr` child process SHALL receive an explicitly prepared environment: `PATH` (with the managed `bin` directory first when a managed toolchain is selected), `HOME`/`USERPROFILE`, `TMPDIR`/`TEMP`/`TMP`, `SYSTEMROOT`, `LANG=C.UTF-8`, and Terse-owned `TEXMFHOME`, `TEXMFVAR`, and `TEXMFCONFIG`. Inherited `TEXINPUTS`, `BIBINPUTS`, `TEXMFCNF`, and every other `TEXMF*` variable MUST be absent. The process runner SHALL apply exactly the prepared list and MUST NOT restore any inherited variable on its own.

#### Scenario: Search-path overrides never leak
- **GIVEN** a shell exporting `TEXINPUTS`, `BIBINPUTS`, and `TEXMFHOME` to arbitrary directories
- **WHEN** a build starts XeLaTeX
- **THEN** the recorded invocation environment contains none of those values, contains Terse-owned `TEXMF*` directories, and contains only the documented allowlist

#### Scenario: Managed binaries find each other
- **WHEN** a managed toolchain is selected and Biber runs
- **THEN** the invocation `PATH` begins with the managed `bin` directory so `biber` resolves `kpsewhich` from the same prefix

### Requirement: Doctor runs bounded checks and names causes
`terse doctor` SHALL execute every check through the bounded process runner with an explicit timeout, never by presence alone. Each check SHALL report an identifier, a status (`ok`, `warn`, `fail`, `info`), evidence, a probable cause, and a per-OS fix command when one exists. Checks SHALL cover the Terse version, the selected toolchain source and reason, the manifest engine against the profile, `xelatex --version` and its TeX Live year against the profile, `biber --version`, `kpsewhich`, every profile package as a resolvable `.sty` file, every profile font as a resolvable font file, the profile's babel language definition files, installation prerequisites (`perl` and a downloader usable by `install-tl`), macOS quarantine attributes on managed binaries, free disk space, and the poppler tools at `info` level. A Biber that does not answer within the timeout SHALL be reported with the computed stale PAR cache directory (`<tmpdir>/par-<lowercase hex of the username>`) as the probable cause. Without `--fix`, `doctor` MUST NOT write outside its temporary working directory. Exit code SHALL be `0` with no `fail` check, `1` with any `fail` check, and `2` for usage errors; `--json` SHALL emit a versioned envelope on stdout only.

#### Scenario: Hung Biber is diagnosed, not mislabeled
- **GIVEN** a `biber` executable that never exits
- **WHEN** `terse doctor` runs
- **THEN** the Biber check reports `fail` after its timeout with the PAR cache path as the probable cause and a removal command, the process tree is terminated, the exit code is `1`, and no file outside the temporary directory changed

#### Scenario: JSON report is a versioned envelope
- **WHEN** `terse doctor --json` runs under two different locales
- **THEN** stdout parses as the same versioned envelope with identical check identifiers and statuses, and all progress text is on stderr

### Requirement: Doctor fix applies only enumerated safe repairs
`terse doctor --fix` SHALL apply only repairs from a documented allowlist: removing a stale Biber PAR cache directory and clearing the macOS quarantine attribute from binaries inside the managed prefix. Each applied repair SHALL be reported and re-probed. `--fix` MUST NOT modify any TeX tree, `terse.toml`, project files, or files outside the allowlisted locations.

#### Scenario: Stale PAR cache is removed and nothing else
- **GIVEN** a temporary directory containing `par-<hex>/cache-x/` and an unrelated sibling file
- **WHEN** `terse doctor --fix` runs with a hanging Biber
- **THEN** the `par-<hex>` directory is deleted, the sibling file is byte-identical, the report lists the removal, and Biber is probed again

#### Scenario: Doctor without fix is read-only
- **WHEN** `terse doctor` runs without `--fix` against the same state
- **THEN** a hash of the temporary directory tree is unchanged afterwards

### Requirement: Doctor micro-compile is the definitive check
When the engine checks pass, `doctor` SHALL compile an embedded document containing one citation through XeLaTeX, Biber, and XeLaTeX in a temporary directory using the same bounded runner and prepared environment as `build`. A failure SHALL be a distinct `fail` check carrying the relevant log excerpt and MUST NOT be reported as a generic engine error.

#### Scenario: Micro-compile failure is distinct
- **GIVEN** a runner scripted to fail the Biber pass
- **WHEN** `terse doctor` runs
- **THEN** the micro-compile check reports `fail` with the Biber log excerpt while the earlier presence and version checks remain `ok`

#### Scenario: Micro-compile succeeds on the managed prefix
- **GIVEN** an installed managed toolchain
- **WHEN** `terse doctor --toolchain managed` runs
- **THEN** every check is `ok` or `info`, a PDF is produced in the temporary directory, and the exit code is `0`

### Requirement: Managed toolchain installation is transactional and pinned
`terse toolchain install` SHALL provision a private TeX Live for the profile's year into a user-scoped prefix (`$XDG_DATA_HOME` or `~/.local/share` on Linux, `~/Library` on macOS, under a `terse/toolchain/texlive-<year>` path) or an explicit `--prefix`. The prefix MUST NOT contain whitespace, because XeTeX passes its output driver's location through a shell; a whitespace prefix SHALL be refused before any download. It SHALL verify installation prerequisites before any download, download `install-tl` from a pinned repository list, verify its pinned SHA-512 before executing anything, install with `scheme-infraonly` in portable mode from the frozen historic archive of that year, and install the pinned package closure with `tlmgr` against the same repository. All work SHALL happen in a same-filesystem staging directory that is atomically renamed into place only after a lock file (year, repository, `install-tl` checksum, package names and revisions, Terse version, platform) and the ownership marker are written. Any failure MUST remove the staging directory and leave a previous prefix untouched. Windows MUST be rejected as unsupported for installation in this version with a diagnostic pointing to the system-toolchain path.

#### Scenario: Checksum mismatch stops before execution
- **GIVEN** a download whose bytes do not match the pinned SHA-512
- **WHEN** `terse toolchain install` runs
- **THEN** it fails with exit `3` before running `install-tl`, no staging directory remains, and no prefix exists

#### Scenario: Package installation failure leaves the previous prefix
- **GIVEN** an installed managed prefix and a `tlmgr` scripted to fail
- **WHEN** `terse toolchain update` runs
- **THEN** the previous prefix and its lock are byte-identical afterwards and no partial staging directory remains

#### Scenario: Lock records reproducible pins
- **WHEN** installation succeeds
- **THEN** the lock file lists the year, repository URL, `install-tl` checksum, every installed package with its revision, the Terse version, and the platform, in a fixed field order without timestamps

### Requirement: Managed toolchain provisioning is the only new network path
Only `terse toolchain install` and `terse toolchain update` SHALL construct a network downloader. `check`, `build`, `fmt`, `watch`, `export`, `doctor`, and `toolchain status|uninstall` MUST NOT perform any network request. Downloads SHALL use HTTPS only, validate every redirect hop against local and private targets, and enforce a size bound.

#### Scenario: Every other command is network-free
- **GIVEN** a denied network capability
- **WHEN** `check`, `build --require-pdf`, `fmt`, `watch`, `export`, and `doctor` run against a project with a managed toolchain
- **THEN** each succeeds or fails on its own terms and zero network requests are recorded

### Requirement: Offline installation from a produced archive
`terse toolchain status --archive <file>` SHALL produce a relocatable archive of an installed managed prefix including its lock. `terse toolchain install --offline --from <file>` SHALL install from such an archive with no network request, verify the archived lock's year and platform against the profile and host, and verify recorded checksums before the atomic rename. `--offline` without `--from` and `--from` with a mismatched archive MUST fail without changes.

#### Scenario: Tampered archive is rejected before rename
- **GIVEN** an archive whose payload was modified after its lock was written
- **WHEN** `terse toolchain install --offline --from` runs
- **THEN** it fails with exit `3`, zero network requests occur, and no prefix or staging directory remains

#### Scenario: Offline install then required PDF build
- **GIVEN** a valid archive produced on the same platform
- **WHEN** `terse toolchain install --offline --from` and then `terse build --require-pdf --toolchain managed` run with the network denied
- **THEN** the prefix is installed atomically and the build produces a PDF

### Requirement: Status, update, and uninstall are explicit
`terse toolchain status` SHALL report the resolved source, prefix, lock contents, and whether the lock year matches the profile, and SHALL exit `0` whether or not a managed prefix exists. `terse toolchain uninstall` SHALL remove only a prefix carrying the Terse ownership marker and MUST refuse any other directory. `terse toolchain update` SHALL refuse a prefix whose lock year differs from the profile year.

#### Scenario: Uninstall removes only the owned prefix
- **GIVEN** a managed prefix and an unrelated sibling directory
- **WHEN** `terse toolchain uninstall` runs
- **THEN** the prefix is gone, the sibling is untouched, and a second uninstall reports nothing to remove with exit `0`

#### Scenario: Unowned directory is refused
- **WHEN** `terse toolchain uninstall --prefix` names a populated directory without the ownership marker
- **THEN** the command fails with exit `2` and deletes nothing

### Requirement: Managed prefix never enters artifacts
The managed prefix path SHALL appear only in diagnostics, doctor reports, and the local cache report. Generated `.tex`, `.sty`, `.bib`, source maps, build manifests, export archives, and export manifests MUST NOT contain the prefix path or any host identifier derived from it.

#### Scenario: Prefix path is absent from published output
- **WHEN** a project is built and exported with a managed toolchain
- **THEN** no published text artifact or archive member contains the prefix path

