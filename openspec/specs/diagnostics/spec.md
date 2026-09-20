# Diagnostics Specification

## Purpose
Defines the shared diagnostic data model, stable codes and failure policy, accurate Unicode and include positions, deterministic human/JSON output, generated-to-source maps for engine failures, and bounded recovery that never reports invalid output as successful.
## Requirements
### Requirement: Shared actionable diagnostic data
Every error SHALL expose severity, stable code, concise message, original source/configuration filename when available, line/column when available, related locations, and a reliable correction when one exists. Diagnostics SHALL share a versioned data model across parser, resolver, references, themes, backend, engine, watch, and export. File-wide or tool errors without an exact source position SHALL use an explicit absent span and relevant context rather than fabricated coordinates.

#### Scenario: Duplicate identifier reports both definitions
- **WHEN** the resolver finds a duplicate ID
- **THEN** the diagnostic contains `E-ID-002`, severity error, the offending original definition, the first definition as a related location, and a rename/removal correction

#### Scenario: Tool startup has no invented source line
- **WHEN** the configured LaTeX executable cannot be started
- **THEN** the diagnostic identifies the tool failure with an absent primary span where appropriate and the engine configuration as related context

### Requirement: Accurate original positions across Unicode and includes
Source positions SHALL derive from original file byte spans. Human line/column positions SHALL be one-based Unicode scalar positions; JSON SHALL also expose byte ranges. Included content SHALL report its original filename and retain its include chain as related context. Root-relative diagnostic paths SHALL avoid machine-specific absolute prefixes where a project source is known.

#### Scenario: Unicode before a cited alias
- **GIVEN** a paragraph with accented/multibyte text before an unknown citation in an included module
- **WHEN** checking fails
- **THEN** the displayed column counts Unicode scalar values, the byte range indexes the original bytes, and the filename is the included module

### Requirement: Stable diagnostic codes and failure policy
Codes SHALL have stable meanings within a diagnostic schema version and SHALL NOT be repurposed. Code families SHALL cover parsing, metadata, includes, IDs, references, citations, themes, assets, configuration, LaTeX, export, and toolchain. `E-CITE-001` SHALL mean unknown alias, `E-ID-002` duplicate ID, `E-INCLUDE-003` include cycle, `W-TEX-001` raw portability warning, `E-EXPORT-004` unverifiable raw export dependency boundary, `E-LATEX-014` engine timeout with process-tree termination, and `E-CONFIG-008` manifest engine disagreeing with the export profile. The `E-TOOL-*` family SHALL cover doctor check failures and toolchain resolution, prerequisite, download, checksum, installation, and publication failures. `--deny-warnings` SHALL change command failure policy without renaming warning codes.

#### Scenario: Strict raw warning remains recognizable
- **WHEN** `check --strict` and `check --strict --deny-warnings` inspect the same raw block
- **THEN** both report `W-TEX-001`, while only the warning-denying invocation fails solely because of that warning

#### Scenario: Toolchain codes are stable and distinct
- **WHEN** a build times out and a doctor check fails in separate invocations
- **THEN** the first reports `E-LATEX-014` and the second reports a code in the `E-TOOL-*` family, and neither reuses an existing LaTeX or export code

### Requirement: Machine-readable output is deterministic and isolated
Commands SHALL support `--diagnostics human|json`. JSON SHALL use a documented versioned envelope, deterministic source-traversal/position/code ordering, and no ANSI escapes or interleaved progress prose on stdout. Progress SHALL go to stderr. Watch JSON SHALL use one object per completed attempt, with a monotonic attempt number, status, diagnostics, and publication result. Successful empty diagnostics SHALL still be representable.

#### Scenario: CI parses repeated validation failures
- **WHEN** two checks run on identical invalid inputs in JSON mode
- **THEN** their diagnostic arrays are equal in order/content and stdout parses as the documented JSON envelope

#### Scenario: Watch distinguishes failed and published attempts
- **WHEN** a watched build fails and the next one succeeds in JSON mode
- **THEN** two records have increasing attempt numbers and accurately report whether output was published

### Requirement: Generated-to-source maps explain engine failures
LaTeX generation SHALL emit a versioned map with relative source filenames and sorted generated line/column intervals. It SHALL map math/raw payload lines and source-derived style parameters; escaped text and expansions SHALL fall back to the nearest originating node/field. Engine errors SHALL prefer original `.trs` or `.theme` locations and include the generated position as related debugging context. Bibliography errors SHALL use alias/declaration/override provenance. Compiler-owned scaffolding SHALL be explicitly marked generated.

#### Scenario: Engine error in included raw block
- **GIVEN** an engine failure at a generated line corresponding to raw TeX in `sections/method.trs`
- **WHEN** the log is interpreted
- **THEN** the error points to the original block/line and also exposes the generated TeX location

#### Scenario: Unmappable package failure
- **WHEN** the engine reports an error in distribution-owned scaffolding without a reliable source mapping
- **THEN** the diagnostic preserves the generated/tool context and explicitly omits a guessed `.trs` location

### Requirement: Recovery never reports invalid output as successful
Parsing/validation SHALL collect additional diagnostics only at recoverable structural boundaries, with a bounded diagnostic count. Any error SHALL prevent successful publication. A truncated diagnostic list SHALL identify that more errors were suppressed. Suggested corrections SHALL not claim missing metadata, identifiers, or source text can be inferred when they cannot.

#### Scenario: Several independent source errors
- **WHEN** multiple recoverable invalid blocks appear in one project
- **THEN** checking reports bounded, ordered diagnostics and returns failure without generating a successful artifact plan

