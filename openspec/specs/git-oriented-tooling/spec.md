# Git-Oriented Tooling Specification

## Purpose
Specifies the noninteractive CLI workflow around a versioned project: safe initialization, engine-free checking, deterministic formatting, reproducible generated text, disposable caches, transactional output publication, meaningful exit codes, and the public reproducible personal-project release workflow.
## Requirements
### Requirement: Safe project initialization
`terse init` SHALL create a minimal valid entry, versioned manifest, local academic theme reference/file, empty versioned citation lock, and appropriate ignore entries. It SHALL preflight scaffold conflicts before writing and refuse overwrites without `--force`. Existing `.gitignore` entries SHALL be preserved and missing Terse/LaTeX ignore entries added without duplicate accumulation. Force MUST NOT discard unrelated files or ignore entries. Defaults SHALL support immediate engine-free checking/building.

#### Scenario: Initialize a usable project
- **WHEN** `terse init` runs in an empty target directory
- **THEN** the scaffold passes `check`, `fmt --check`, and `build --tex-only` without resolving references or installing TeX

#### Scenario: Existing source is protected
- **GIVEN** a preexisting `paper.trs` and unrelated `.gitignore` entries
- **WHEN** initialization runs without force
- **THEN** it reports the conflict before modifying scaffold files or the ignore file

#### Scenario: Ignore entries are additive
- **GIVEN** an existing `.gitignore` and no conflicting scaffold files
- **WHEN** initialization succeeds and later runs with explicit force
- **THEN** original ignore entries remain and each required Terse ignore entry occurs once, without blanket-ignoring authored figure PDFs

### Requirement: Complete engine-free project checking
`terse check` SHALL parse and validate the reachable project, includes, symbols, references/lock/overrides, themes, assets, and selected target compatibility without requiring PDF generation. It SHALL check all declared themes by default or only the explicitly selected theme. It MUST NOT mutate sources, lockfiles, or successful output. Invalid inputs MUST produce a meaningful failure exit status.

#### Scenario: Check without a TeX installation
- **GIVEN** a valid locked multi-file project and no available engine
- **WHEN** `terse check` runs
- **THEN** it validates the project successfully without starting a subprocess or using the network

#### Scenario: Theme selection controls validation scope
- **GIVEN** one valid and one invalid declared theme
- **WHEN** checking all themes and then only the valid selected theme
- **THEN** the first check reports the invalid theme and the selected-theme check validates only the selected presentation dependencies

### Requirement: Deterministic formatting with minimal diffs
`terse fmt` SHALL format source/theme files using a deterministic transform that preserves semantic meaning, paragraph wrapping, comment placement, metadata/author order, citation order, escaped text, and opaque code/math/raw payload bytes. It SHALL canonicalize structural indentation/separators and non-payload line endings and remove an optional initial BOM. Without explicit paths it SHALL process reachable modules and declared themes in sorted path order. All requested inputs SHALL parse before writes; malformed input MUST leave every requested file unchanged. Successful writes SHALL be atomic per file. Unresolved semantic references alone MUST NOT prevent formatting syntactically valid source.

#### Scenario: Formatting stability [Acceptance H]
- **GIVEN** a valid project with source and theme files
- **WHEN** formatting runs twice and then `fmt --check` runs
- **THEN** the second run changes no bytes, the check succeeds, and the semantic content equals the pre-format content

#### Scenario: Opaque payload and paragraph wrapping are preserved
- **WHEN** formatting a file with deliberately wrapped paragraphs and raw/math payloads containing extra indentation, CRLF, or whitespace-only lines
- **THEN** prose line-break choices and payload bytes are preserved while only supported surrounding structural formatting changes

#### Scenario: Malformed file prevents batch writes
- **GIVEN** several selected files, one syntactically invalid
- **WHEN** `terse fmt` runs
- **THEN** it reports the invalid file and none of the selected files is modified

### Requirement: Formatting check never writes
`terse fmt --check` SHALL compute the same formatting result in memory, report affected files, and return `1` when differences exist, without modifying files. It SHALL return `0` when no changes are needed and SHALL expose diagnostics in the selected human/JSON mode.

#### Scenario: CI detects formatting drift
- **GIVEN** syntactically valid source with noncanonical structural spacing
- **WHEN** `fmt --check` runs
- **THEN** it returns `1`, identifies the file requiring formatting, and all input bytes remain unchanged

### Requirement: Reproducible generated text
Identical authoritative inputs, lockfile, compiler version, command, and environment-independent configuration SHALL generate byte-identical Terse-produced text artifacts. Serializers SHALL use fixed field/map ordering, portable relative paths, explicit versions, and no generated timestamps, random IDs, or host state. Explicit authored dates SHALL be retained. Assets SHALL copy unchanged. PDF byte identity across toolchains is excluded; optional externally generated `.bbl` comparisons SHALL use a pinned compatible toolchain.

#### Scenario: Deterministic output [Acceptance G]
- **GIVEN** two clean project copies at different absolute paths with identical inputs/configuration/compiler
- **WHEN** the same source-only build runs under different locales and directory enumeration orders
- **THEN** every corresponding generated text artifact is byte-identical and contains no absolute host path

### Requirement: Disposable caches cannot change results
Build/cache directories SHALL be disposable and excluded from authoritative state. Cached parsing, provider responses, or process results MUST NOT bypass current input/lock/dependency validation. Corrupt or absent cache entries SHALL trigger recomputation rather than semantic changes. Provider caches SHALL be used only during explicit resolution, and refresh SHALL bypass them. Generated/cache files MUST NOT become implicit dependencies.

#### Scenario: Clean and cached builds agree
- **WHEN** a project is built with populated caches, no caches, and corrupted cache entries
- **THEN** successful generated text bytes agree and no normal build performs provider requests

### Requirement: Noninteractive CLI and meaningful exit codes
The CLI SHALL expose init, check, fmt, refs resolve, build, watch, arXiv export, doctor, and toolchain (install, update, status, uninstall) as distinct commands, support `--diagnostics human|json`, and print its version with `--version`. Commands SHALL operate without interactive prompts in CI. Exit codes SHALL be `0` for success, `1` for input/validation errors, formatting differences, or failed doctor checks, `2` for usage/configuration errors including an unusable explicit toolchain selection, and `3` for I/O/provider/tool execution failures including toolchain download or provisioning failures. Explicit destructive scaffold replacement SHALL require a force flag; lack of confirmation SHALL never imply consent. Warning-denial policy SHALL result in a validation failure.

#### Scenario: CI observes failure categories
- **WHEN** independent invocations encounter an unknown citation, an invalid CLI flag, and a failing provider request
- **THEN** they terminate noninteractively with exit codes `1`, `2`, and `3` respectively and parseable diagnostics when JSON mode is selected

#### Scenario: Toolchain commands classify their failures
- **WHEN** independent invocations run `doctor` against a hung Biber, `build --toolchain managed` without an installed prefix, and `toolchain install` against a checksum mismatch
- **THEN** they exit `1`, `2`, and `3` respectively

#### Scenario: Version flag
- **WHEN** `terse --version` runs
- **THEN** it prints the workspace package version on stdout and exits `0`

### Requirement: Transactional output publication and recovery
Build/export publication SHALL stage a complete generation on the same filesystem as its destination and serialize writers with an output lock. A populated unowned destination MUST be rejected. Publication SHALL use atomic replacement where available, or a backup/rename/journal fallback that restores the last valid generation after a failed or interrupted replacement. Successful output MUST NOT mix files from different generations. Publication MUST stay inside the validated output scope and never delete unrelated source files. Documentation SHALL state that rebuilding replaces managed generated edits and explain copying output for independent editing.

#### Scenario: Publication failure rolls back
- **GIVEN** a previous successful managed output and a completed new staged generation
- **WHEN** publication fails between fallback renames
- **THEN** rollback or next-start recovery restores a complete valid generation instead of leaving mixed or missing authoritative output

#### Scenario: Unowned destination is protected
- **WHEN** output targets a populated directory with no valid Terse ownership manifest
- **THEN** publication fails without replacing or deleting its contents

### Requirement: Public reproducible personal-project workflow
Terse SHALL be distributable as a single compiler executable on the documented Linux/macOS/Windows matrix. PDF generation SHALL be satisfied either by a documented system TeX Live/Biber installation verified with `terse doctor` or by `terse toolchain install` on Linux and macOS. The public repository SHALL provide one documented source-install command, locked build dependencies, README/CONTRIBUTING guidance, per-OS installation documentation covering both toolchain paths, and local commands equivalent to CI. It SHALL include a complete multi-file two-theme fixture with locked DOI/arXiv metadata and redistributable assets. The demonstration `magalu` theme SHALL be identified as unofficial and require no corporate resources. Installation, layout, language, themes, citations, Git workflow, and export documentation SHALL cover the full release workflow. The pinned Linux acceptance environment SHALL be produced by `terse toolchain install` from the committed toolchain profile, and the acceptance suite SHALL run in it with the network disabled after provisioning. The author SHALL choose the open-source license before public release; tooling MUST NOT assume a particular license or impose one on authored document content.

#### Scenario: A contributor reproduces the demonstration
- **GIVEN** a clean public checkout and documented prerequisites
- **WHEN** a contributor follows the one-command source installation and documented fixture commands
- **THEN** they can check, format, build both themes, inspect the LaTeX, and export the fixture without private repositories, hidden files, manually maintained BibTeX, or source edits per theme

#### Scenario: One maintainer can run release validation
- **WHEN** the maintainer runs the checked-in validation workflow
- **THEN** compiler/executable checks cover the supported OS matrix and one pinned Linux TeX environment, provisioned by `terse toolchain install` and then isolated from the network, runs all A–M PDF/export acceptance cases without requiring a separate review team

#### Scenario: A user without TeX reaches a PDF with documented commands only
- **GIVEN** a Linux or macOS machine with the documented prerequisites and no TeX installation
- **WHEN** the user runs the documented source install, `terse toolchain install`, `terse doctor`, and `terse build --require-pdf` on the fixture
- **THEN** every step succeeds without manual TeX package or font installation

