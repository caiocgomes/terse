# LaTeX Generation Specification

## Purpose
Specifies the complete, human-readable, portable LaTeX deliverables Terse generates, context-aware escaping, deterministic asset copying, the documented XeLaTeX/BibLaTeX/Biber toolchain, PDF compilation modes, controlled bounded engine execution, and publication only after a successful build.
## Requirements
### Requirement: Complete readable LaTeX deliverables
`terse build` SHALL generate a complete human-readable LaTeX project under the configured output/theme directory. For `paper.trs`, default deliverables SHALL include `paper.tex`, `terse-style.sty`, `references.bib`, `paper.map.json`, `build-manifest.json`, `COMPILE.txt`, and all used assets, with optional successfully compiled PDF and `.bbl`. The main file SHALL use a conventional preamble and readable semantic macros/environments. Every custom macro definition SHALL be included locally; comments and source-map paths SHALL be root-relative. The generated project MUST NOT require Terse, network access, or original-machine paths to compile.

#### Scenario: Portable LaTeX project [Acceptance B]
- **GIVEN** a successful build with required inputs and documented compatible tooling
- **WHEN** only the generated deliverables are copied to a clean environment without Terse and with network access disabled
- **THEN** the commands in `COMPILE.txt` compile the paper without source-tree files, machine-specific paths, or hidden Terse state

#### Scenario: Output remains editable
- **WHEN** a user opens the generated main TeX and style files
- **THEN** authored content and presentation definitions are available as ordinary formatted text rather than an opaque encoded payload or Terse runtime call

### Requirement: Semantic emission preserves all authored content
The backend SHALL traverse the resolved document in order and emit each authored node once using trusted semantic macros/environments. It SHALL retain all metadata, supported inline/block elements, figure captions/alt data/roles, tables, equations, citations, IDs, and references. Metadata SHALL render as title, subtitle, authors/affiliations, explicit date, abstract, keywords, then body. Themes SHALL supply presentation through the style layer, not different authored bodies. Generated title/date logic MUST NOT invent a date or substitute document text.

#### Scenario: All-element fixture renders
- **WHEN** the complete multi-file fixture is generated and compiled under both supported demonstration themes
- **THEN** every authored element is represented in order and neither body adds or loses content due to theme selection

### Requirement: Context-aware escaping preserves text and TeX payloads
The generator SHALL escape ordinary text, URL destinations, inline code, metadata/PDF strings, bibliography fields, macro arguments, and paths according to their distinct LaTeX contexts. It MUST preserve accepted math and explicit raw payload bytes after structural dedenting. Ordinary text containing TeX metacharacters MUST remain literal and MUST NOT become commands. Only HTTP, HTTPS, and mailto links SHALL be accepted; destinations MUST NOT be fetched. Dangerous schemes and unsupported control characters MUST fail.

#### Scenario: Literal metacharacters and math coexist
- **GIVEN** prose containing ampersands, percentages, underscores, braces, backslashes, and a validated math expression
- **WHEN** the document is generated and compiled
- **THEN** prose displays literally without injection or compile errors and the math payload is not text-escaped or rewritten

#### Scenario: Dangerous link scheme
- **WHEN** source links to an executable or unsupported URI scheme
- **THEN** validation fails at the destination before generation or process execution

### Requirement: Deterministic asset copying and relative paths
Only referenced document/theme assets and explicitly declared support files SHALL be copied into the output. Names SHALL be stable and portable, with deterministic disambiguation where required. The generator MUST reject collisions with generated files, missing assets, and external dependencies prohibited by project validation. Asset content SHALL be copied byte-for-byte; no network retrieval or implicit format conversion SHALL occur.

#### Scenario: Same basenames from different directories
- **GIVEN** two distinct referenced files named `plot.pdf` in different project directories
- **WHEN** the project is generated twice
- **THEN** both assets have stable distinct relative output names and the correct source bytes, with references targeting the correct copies

#### Scenario: Unused file stays out of output
- **WHEN** an unrelated asset exists beside a referenced figure
- **THEN** the build copies the referenced figure and does not copy the unrelated file

### Requirement: Documented Unicode and bibliography toolchain
The initial backend SHALL target XeLaTeX with the bounded supported package set and BibLaTeX/Biber for bibliography processing. Fonts SHALL use supported distribution filenames and supported language mappings. `.bib` and source text SHALL support Unicode metadata/input; known missing glyphs during compilation MUST fail with actionable diagnostics. Without engine execution, the command SHALL identify that rendering/glyph coverage was not validated. Additional engines MUST NOT be silently substituted.

#### Scenario: Unicode paper compiles
- **GIVEN** a supported-language paper and bibliography containing accented author names and prose supported by the configured fonts
- **WHEN** the documented toolchain compiles it
- **THEN** those characters are retained without transliteration or substitution

#### Scenario: Missing glyph is reported
- **WHEN** compilation reports a missing glyph for authored text
- **THEN** the build fails with the available source/context and does not publish a misleading successful PDF

### Requirement: Numbered and unnumbered references remain valid
Identified equations SHALL have numbered anchors; display equations without IDs SHALL be unnumbered. Figures, tables, and theorem-like statements SHALL receive deterministic counters and valid anchors. Cross-references SHALL use resolved kinds rather than infer meaning from label names. Proofs SHALL retain proof anchors and explicit `of` relationships without inventing theorem content. Unnumbered heading/proof references SHALL have meaningful labels.

#### Scenario: Cross-file numbering converges [Acceptance E]
- **GIVEN** a forward reference to an identified equation in another included module
- **WHEN** the supported multi-pass compilation completes
- **THEN** the equation number/link is resolved and no unresolved-reference warning remains

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

### Requirement: Custom TeX support is explicit and portable within normal builds
Manifest-declared support files/packages SHALL be validated and copied with their relative dependencies for normal raw-TeX builds. A built-in optional `tikz` package selection SHALL support the drawing escape hatch without automatic installation. Package identifiers MUST NOT be arbitrary preamble fragments, and non-built-in packages SHALL require corresponding declared support files. Strict checking SHALL report their trust/portability boundary. Export SHALL apply its stricter extension policy instead of silently dropping support.

#### Scenario: Explicit drawing support
- **GIVEN** a raw TikZ drawing and explicit supported package selection with the package installed in the compatible toolchain
- **WHEN** the paper is built normally
- **THEN** the generated project includes the required package setup and compiles without a Terse-specific runtime

### Requirement: Publication occurs only after the requested build succeeds
Source generation and requested PDF/bibliography execution SHALL finish in staging before publication. Output replacement SHALL follow the transactional publication contract, and any generation/engine failure MUST retain the previous valid generation. Engine auxiliary files/logs SHALL remain disposable diagnostics rather than published deliverables.

#### Scenario: Engine error preserves prior artifacts
- **GIVEN** an existing successful output directory
- **WHEN** a new build fails during XeLaTeX or Biber execution
- **THEN** the prior published TeX/style/bibliography/assets/PDF bytes remain intact and the failure is reported against original source where possible

