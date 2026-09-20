## MODIFIED Requirements

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
