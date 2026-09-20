## MODIFIED Requirements

### Requirement: Deterministic project discovery and manifest configuration
Terse SHALL require `terse.toml`, locating it by walking upward from an explicit entry's directory or the working directory when no entry is supplied. The manifest directory SHALL be the project root. The manifest SHALL define a versioned schema for entry/output, named themes, XeLaTeX/PDF settings, reference lock/overrides, optional local TeX support/packages, and export profile. Within `[latex]`, `engine` SHALL name the engine and MUST equal the export profile's engine, `pdf` SHALL set the default compilation mode, and `toolchain` SHALL select `auto`, `system`, `managed`, or an explicit directory for tool resolution. Unknown keys and unsupported versions MUST fail. `academic` SHALL be the default theme when declared; otherwise theme-dependent commands MUST require a selection. Explicit CLI settings SHALL override manifest settings, which override documented defaults. Environment variables MUST NOT alter semantic configuration or generated text; tool location MAY come from `PATH` but MUST NOT change generated text.

#### Scenario: Entry discovery from a subdirectory
- **GIVEN** a root manifest whose entry is `paper.trs`
- **WHEN** `terse check` runs from a descendant directory without an entry argument
- **THEN** it finds that manifest and checks its root-relative entry

#### Scenario: Missing or invalid manifest
- **WHEN** no ancestor manifest exists or the manifest contains an unknown field/version
- **THEN** the command fails with configuration diagnostics and, for a missing manifest, guidance to run `terse init`

#### Scenario: Explicit settings win without environment drift
- **GIVEN** a manifest selecting PDF auto behavior and an academic theme
- **WHEN** `build --theme magalu --tex-only` runs under different locales and TeX search-path environments
- **THEN** it selects `magalu`, invokes no engine, and generates the same text artifacts from identical inputs

#### Scenario: Engine field must agree with the profile
- **GIVEN** a manifest declaring `[latex] engine = "pdflatex"` and the `texlive-2025-xelatex` profile
- **WHEN** any command loads the manifest
- **THEN** it fails with a configuration diagnostic naming both values and exit code `2`
