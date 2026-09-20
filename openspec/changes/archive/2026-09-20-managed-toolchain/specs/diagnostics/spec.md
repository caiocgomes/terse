## MODIFIED Requirements

### Requirement: Stable diagnostic codes and failure policy
Codes SHALL have stable meanings within a diagnostic schema version and SHALL NOT be repurposed. Code families SHALL cover parsing, metadata, includes, IDs, references, citations, themes, assets, configuration, LaTeX, export, and toolchain. `E-CITE-001` SHALL mean unknown alias, `E-ID-002` duplicate ID, `E-INCLUDE-003` include cycle, `W-TEX-001` raw portability warning, `E-EXPORT-004` unverifiable raw export dependency boundary, `E-LATEX-014` engine timeout with process-tree termination, and `E-CONFIG-008` manifest engine disagreeing with the export profile. The `E-TOOL-*` family SHALL cover doctor check failures and toolchain resolution, prerequisite, download, checksum, installation, and publication failures. `--deny-warnings` SHALL change command failure policy without renaming warning codes.

#### Scenario: Strict raw warning remains recognizable
- **WHEN** `check --strict` and `check --strict --deny-warnings` inspect the same raw block
- **THEN** both report `W-TEX-001`, while only the warning-denying invocation fails solely because of that warning

#### Scenario: Toolchain codes are stable and distinct
- **WHEN** a build times out and a doctor check fails in separate invocations
- **THEN** the first reports `E-LATEX-014` and the second reports a code in the `E-TOOL-*` family, and neither reuses an existing LaTeX or export code
