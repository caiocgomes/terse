## ADDED Requirements

### Requirement: Deterministic project discovery and manifest configuration
Terse SHALL require `terse.toml`, locating it by walking upward from an explicit entry's directory or the working directory when no entry is supplied. The manifest directory SHALL be the project root. The manifest SHALL define a versioned schema for entry/output, named themes, XeLaTeX/PDF settings, reference lock/overrides, optional local TeX support/packages, and export profile. Unknown keys and unsupported versions MUST fail. `academic` SHALL be the default theme when declared; otherwise theme-dependent commands MUST require a selection. Explicit CLI settings SHALL override manifest settings, which override documented defaults. Environment variables MUST NOT alter semantic configuration or generated text.

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

### Requirement: One documented path-resolution rule per declaring context
Explicit CLI entry paths SHALL resolve relative to the invocation directory; manifest paths SHALL resolve relative to the project root. Include/asset paths SHALL resolve relative to the declaring `.trs` file, and theme assets relative to their `.theme` file. Canonicalization SHALL enforce root confinement after resolving internal parent components and symlinks. All commands SHALL use these same rules.

#### Scenario: Asset in a sibling directory
- **GIVEN** `sections/method.trs` references `../figures/model.pdf` within the project
- **WHEN** the project is checked and built
- **THEN** both operations resolve the same root-contained asset, regardless of the invocation directory

#### Scenario: Theme-relative logo
- **WHEN** `themes/academic.theme` references `assets/logo.pdf`
- **THEN** the dependency is `themes/assets/logo.pdf`, not a working-directory-relative file

### Requirement: Includes are explicit ordered modules
Includes SHALL name individual `.trs` files without wildcard ordering. Each include occurrence SHALL expand its parsed module at the authored position. Repeated inclusion MUST repeat content instead of silently deduplicating it. A shared parse cache MUST NOT change expansion semantics. Nested includes SHALL work, while included metadata and nested declaration placements prohibited by the language SHALL fail.

#### Scenario: Repeated module without IDs
- **GIVEN** a module containing an unlabelled paragraph is included twice
- **WHEN** the entry is resolved
- **THEN** the paragraph appears twice at the two authored include positions

#### Scenario: Wildcard inclusion is rejected
- **WHEN** an include declares `sections/*.trs`
- **THEN** validation fails with guidance to list explicit module paths and does not use filesystem enumeration to determine content

### Requirement: Missing includes and complete cycle diagnostics
Missing includes SHALL fail before backend generation and anchor diagnostics to the include statement. Cycle detection SHALL use the active expansion path and report the complete cycle, edge locations, and entry context rather than mistaking a repeated acyclic include for a cycle.

#### Scenario: Indirect include cycle [Acceptance F]
- **GIVEN** `paper.trs` includes `method.trs`, which includes `appendix.trs`, which includes `paper.trs`
- **WHEN** `terse check` runs
- **THEN** it fails with `E-INCLUDE-003`, reports `paper.trs → method.trs → appendix.trs → paper.trs`, and invokes no LaTeX generation

#### Scenario: Include target is absent
- **WHEN** an include names a file that does not exist
- **THEN** checking fails at the including statement and reports the unresolved target path

### Requirement: Global identifiers and typed cross-file references
IDs SHALL occupy one case-sensitive namespace across the expanded document. Headings, equations, figures, tables, theorem-like blocks, and proofs SHALL resolve forward and cross-file references. Duplicate IDs MUST fail with `E-ID-002`, both definition locations, and include routes when needed. Unknown reference targets and a proof `of` target that is not theorem-like MUST fail without generating dangling links.

#### Scenario: Multi-file equation reference [Acceptance E]
- **GIVEN** an identified equation in `sections/method.trs` and a reference in `sections/results.trs`
- **WHEN** the entry is checked and built with a compatible engine
- **THEN** the reference resolves to that equation and the PDF has a valid numbered link without unresolved-reference warnings

#### Scenario: Duplicate definitions are actionable
- **WHEN** two modules define `demand-model`, including through repeated inclusion
- **THEN** checking fails with `E-ID-002`, identifying both original definitions and distinct include routes where their original spans coincide

#### Scenario: Wrong proof target
- **WHEN** a proof's `of` attribute names a figure
- **THEN** validation identifies the proof attribute and the figure definition and requires a theorem-like target

### Requirement: Global bibliography declarations and placement
Reference aliases SHALL have one namespace across included modules; duplicate declarations MUST fail even for equal identifiers. There SHALL be at most one explicit module-level bibliography marker in the expanded document. Without one, a document with citations SHALL receive a derived bibliography after the body. Bibliography entries SHALL contain all cited works once in first-citation order; theme selection MUST NOT filter or reorder them.

#### Scenario: Declaration and citation in different modules
- **GIVEN** one module declares an alias with a matching lock entry and another cites it
- **WHEN** the project is resolved
- **THEN** the citation binds successfully and the work appears once in the global bibliography

#### Scenario: Multiple bibliography markers
- **WHEN** two included modules each introduce `bibliography:`
- **THEN** validation fails with both marker locations rather than dropping either marker silently

### Requirement: Root-contained inputs and safe output destinations
All authoritative input paths, includes, assets, theme files, lock/override files, and declared support files SHALL remain within the project root by default. Root escapes through parent paths, absolute external/drive/UNC paths, symlinks, dangling symlinks, and special files MUST be rejected. Output SHALL be a proper descendant of the root and MUST NOT overlap source directories or authoritative files. Portable path collisions SHALL be diagnosed even on a case-sensitive host. Generated artifact paths SHALL use portable relative names without machine-specific prefixes.

#### Scenario: Symlink escapes the project
- **WHEN** a source asset or include resolves through an in-root symlink to an outside file
- **THEN** validation rejects the reference before reading/copying the outside file

#### Scenario: Unsafe output setting
- **WHEN** the configured output is the project root, a source directory, or an external directory
- **THEN** the command fails before modifying source or existing output

### Requirement: Assets and support files are explicit dependencies
The project SHALL validate existence and supported type of each referenced figure/logo asset; the initial semantic image formats SHALL be PDF, PNG, and JPEG. Remote assets and automatic conversions/downloads MUST be rejected. Manifest-declared local TeX support files/packages SHALL be validated, included as dependencies, and prevented from replacing generated files. Their raw trust and target-compatibility rules SHALL apply consistently.

#### Scenario: Missing figure and unsupported format
- **WHEN** a figure names a missing file or an SVG requiring conversion
- **THEN** checking fails at the source path with guidance to supply an existing supported local asset

### Requirement: Complete dependency information includes failed lookups
Dependency tracking SHALL include the entry, transitive modules, manifest, lockfile, optional overrides, selected themes, and all referenced assets/support files. It SHALL also retain attempted missing paths and their parent directories so watch mode can detect repairs. Dependency traversal order MUST NOT determine authored order or serialized map order.

#### Scenario: Newly referenced missing file can be repaired
- **WHEN** a source changes to reference a missing module or asset
- **THEN** the failed build records that path as an attempted dependency, allowing its later creation to trigger a rebuild
