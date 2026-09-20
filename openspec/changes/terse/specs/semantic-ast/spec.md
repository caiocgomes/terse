## ADDED Requirements

### Requirement: Complete backend-independent document model
The semantic model SHALL represent metadata, headings, paragraphs, all supported inline elements, ordered/unordered lists, TeX math, equations, figures with caption/alt/role, tables, every theorem-like kind, proofs, citation groups/locators, bibliography placement, cross-references, explicit includes, and raw TeX. These SHALL be typed document concepts rather than pre-rendered LaTeX commands. Only explicit math/raw payloads SHALL retain TeX source as their meaning. No secondary backend is required.

#### Scenario: Every MVP element has a typed representation
- **WHEN** the complete language fixture is lowered to semantic modules
- **THEN** each element has its own kind/fields, with figure alt text, author affiliations, table structure, and citation intent retained independently of LaTeX rendering

### Requirement: Ordered content and typed declarations
The model SHALL retain the authored ordering of blocks, inlines, list items, table rows/cells, author/affiliation lists, citation-group members, and keywords. Metadata and reference declarations SHALL be typed records independent of their source-map field ordering. Resolved documents SHALL expose global symbols, effective bibliography data, and the dependency set without replacing authored nodes with theme-specific copies.

#### Scenario: Order survives include resolution
- **GIVEN** content before and after two explicit includes
- **WHEN** semantic resolution expands the includes
- **THEN** expanded blocks occur at their include positions and all within-module authored sequences retain their original order

### Requirement: Source identity survives all compiler stages
Every node and diagnosable field SHALL retain original file identity and a byte-range source span. Expanded includes SHALL additionally retain their occurrence/include route. Deterministic node identities MUST NOT depend on random IDs or absolute machine paths. Escaping or lowering a node for a backend MUST preserve its original provenance.

#### Scenario: Included equation keeps its source location
- **GIVEN** an equation in `sections/method.trs` included from `paper.trs`
- **WHEN** it passes through symbol resolution and LaTeX emission
- **THEN** its source span still points to the equation in `sections/method.trs`, with `paper.trs` recorded only as include context

#### Scenario: Repeated inclusion has distinct occurrence context
- **WHEN** a module is included at two source positions
- **THEN** nodes retain the same original file spans but distinct include occurrences, allowing duplicate-definition diagnostics to identify both routes

### Requirement: Theme-invariant semantic projection
The compiler SHALL define a versioned canonical projection of authored semantic content including metadata, ordered nodes, IDs and targets, math/raw payloads, figure fields, citation intent/locators, and effective bibliography records. It SHALL exclude spans, incidental node IDs, theme settings, and derived page furniture. Theme resolution MUST NOT mutate this projection or the resolved document. A digest SHALL support comparison but MUST NOT replace structural/content assertions.

#### Scenario: Same document under two themes [Acceptance A]
- **GIVEN** one valid resolved paper and valid `academic` and `magalu` themes
- **WHEN** both theme-specific build plans are generated
- **THEN** structural comparison of their semantic projections succeeds and their canonical content digests are identical

#### Scenario: Presentation-only data stays outside authored content
- **WHEN** one theme enables a table of contents, running headers, and an internal-use watermark
- **THEN** these appear as derived presentation data without adding, removing, or replacing authored semantic nodes

### Requirement: Explicit stages and effect-free compilation core
Parsing, module expansion, symbol resolution, lockfile binding, semantic/target validation, theme resolution, and backend emission SHALL have explicit boundaries. The compilation core SHALL operate on supplied snapshots and return diagnostics/artifact plans without networking, process execution, clock reads, or output writes. Invalid or incomplete semantic input MUST NOT produce a successful artifact plan.

#### Scenario: Compilation does not need services or effects
- **GIVEN** all source/theme/lock/asset inputs in a supplied snapshot and no network/process/write capabilities
- **WHEN** the compiler core generates a LaTeX artifact plan
- **THEN** it succeeds for valid input and returns all publication/process work as data for the application layer

#### Scenario: Invalid references prevent emission
- **WHEN** symbol or citation binding fails
- **THEN** the core returns diagnostics and no successful publishable artifact plan

### Requirement: Bounded processing fails explicitly
The compiler SHALL enforce documented versioned limits for source bytes, node counts, nesting/include depth, and diagnostic accumulation. Exceeding a limit MUST report the relevant input or context and fail without a panic, unbounded processing, or a partially successful document. Diagnostic truncation SHALL be explicit.

#### Scenario: Excessive nesting is bounded
- **WHEN** a document exceeds the configured implementation limit for nested structures
- **THEN** processing returns a resource-limit diagnostic and no output publication is authorized
