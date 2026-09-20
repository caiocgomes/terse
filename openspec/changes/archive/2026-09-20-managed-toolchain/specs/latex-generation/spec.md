## MODIFIED Requirements

### Requirement: Explicit PDF compilation modes
`build --tex-only` SHALL validate and emit source without starting any engine or bibliography processor. The compilation mode SHALL be the explicit `--tex-only`/`--require-pdf` flag when given, otherwise the manifest `[latex] pdf` value, otherwise `auto`. Default `auto` mode SHALL compile when an engine resolved through the documented toolchain precedence is available; an absent engine SHALL yield sources with an explicit warning naming the resolution that was attempted. `--require-pdf` SHALL fail when required tools are absent. An available engine or bibliography processor that fails MUST cause build failure rather than silently falling back to source-only success. Conflicting compilation flags MUST be usage errors.

#### Scenario: Source generation without installed TeX
- **WHEN** a valid project is built with `--tex-only` and no TeX installation
- **THEN** complete source artifacts are produced and no subprocess is invoked

#### Scenario: Auto mode and required PDF differ
- **GIVEN** no configured engine is available
- **WHEN** default build and `build --require-pdf` are run separately
- **THEN** the default reports source-only success with a warning, while the required-PDF invocation fails without replacing previous valid output

#### Scenario: Manifest PDF mode is the default
- **GIVEN** a manifest declaring `[latex] pdf = "require-pdf"` and no engine available
- **WHEN** `build` runs without compilation flags, then with `--tex-only`
- **THEN** the first invocation fails as a required-PDF build and the second emits sources without starting any process

### Requirement: Controlled bounded engine execution
Processes SHALL be invoked by executable/argument vectors in a clean staging directory without shell-string interpolation. XeLaTeX SHALL run with shell escape disabled, noninteractive halt-on-error behavior, file-line errors, and dependency recording. The application SHALL clear inherited TeX/Biber search-path overrides through the prepared child environment, restrict resource lookup to staged files and documented distribution resources, and never install or download packages within `build`, `check`, `fmt`, `watch`, or `export`. `kpsewhich` lookups SHALL go through the same bounded runner. Citation builds SHALL run Biber and repeat engine passes until references converge, with at most five total engine passes. Nonzero exit, timeouts, missing assets, or unresolved citations/references after the limit MUST fail. A timeout SHALL be reported as `E-LATEX-014`, distinct from a nonzero exit, with a hint to run `terse doctor`. Timeouts SHALL terminate the process tree, including grandchildren. Raw/custom TeX SHALL remain documented trusted input, not be advertised as fully sandboxed by these flags.

#### Scenario: Path is not a shell command
- **GIVEN** a valid project directory containing shell-sensitive characters
- **WHEN** PDF generation invokes the engine
- **THEN** the path is passed as an argument/working directory, no shell is launched, and no embedded text is executed as a command

#### Scenario: Compilation does not converge
- **WHEN** unresolved references remain after the maximum engine passes
- **THEN** the build stops, reports the unresolved state, and preserves the previous successful output

#### Scenario: Timeout is distinct from a crash
- **GIVEN** a runner whose engine pass never exits and another whose engine pass exits nonzero
- **WHEN** each build runs
- **THEN** the first reports `E-LATEX-014` with the elapsed limit and the doctor hint, the second reports `E-LATEX-011`, and both preserve the previous output

#### Scenario: Timeout kills the whole tree
- **GIVEN** an engine stand-in that spawns a long-lived grandchild
- **WHEN** the pass exceeds its timeout
- **THEN** neither the child nor the grandchild is running afterwards
