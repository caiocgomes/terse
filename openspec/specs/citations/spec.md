# Citations Specification

## Purpose
Defines citation semantics, the explicit `refs resolve` network boundary, DOI and arXiv provider normalization, the versioned `references.lock` and overrides files, and deterministic cited-only bibliography generation.

## Requirements

### Requirement: Citation semantics include form, grouping, and locators
Citations SHALL distinguish narrative, parenthetical, multi-work groups, and optional per-item locators, retaining authored order and literal locator text. Recognized `p.`, `pp.`, `ch.`, `sec.`, and `vol.` labels SHALL additionally be represented as typed locators. Unknown locator labels SHALL remain literal rather than being guessed. Theme selection SHALL control presentation without changing citation targets or order.

#### Scenario: Multiple independent locators
- **WHEN** `[@robins1986, pp. 10–12; @pearl2009, p. 42]` is resolved and emitted
- **THEN** both works remain in order and each locator stays attached to its own work

#### Scenario: Narrative versus parenthetical form
- **WHEN** `@paper` and `[@paper]` cite the same locked work
- **THEN** they retain distinct citation kinds and generate the corresponding narrative and parenthetical commands

### Requirement: Source declarations and matching locks are mandatory for cited aliases
Every cited alias SHALL have both a source declaration and a matching resolved lock entry. An undeclared alias MUST fail with `E-CITE-001`, even if a leftover lock entry exists. Missing, stale, invalid, or unresolved cited records MUST fail with source-located diagnostics and explicit reference-resolution guidance. Malformed and unsupported declared identifiers MUST fail checking even when uncited; an unused supported declaration lacking a lock entry SHALL produce a warning rather than a cited-reference error.

#### Scenario: Unknown citation alias [Acceptance D]
- **GIVEN** a citation `@acemoglu2027` absent from source declarations and the lockfile
- **WHEN** `terse check` runs
- **THEN** it fails with `E-CITE-001`, the original file/line/column and alias, and guidance to declare it and run `terse refs resolve`

#### Scenario: Leftover lock entry does not authorize a citation
- **GIVEN** an alias present only in `references.lock`
- **WHEN** that alias is cited
- **THEN** checking reports an undeclared alias without silently binding the leftover record

#### Scenario: Declared identifier changed
- **GIVEN** a cited alias whose source DOI differs from its locked identifier
- **WHEN** checking or building the project
- **THEN** the operation fails with stale-binding guidance and neither fetches metadata nor edits the lock

### Requirement: Explicit resolution is the only metadata network operation
`terse refs resolve` SHALL resolve new/changed identifiers and reseal changed overrides. Unchanged locked identities SHALL retain their metadata unless `--refresh` is supplied. `--offline` SHALL forbid provider calls and support only operations satisfied by existing normalized provider records. `--refresh --offline` MUST be rejected. Check, build, format, watch, and export MUST NOT fetch reference metadata or silently mutate the lockfile.

#### Scenario: DOI resolution enables offline builds [Acceptance C]
- **GIVEN** a declared DOI, no locked record, and a valid metadata-provider response
- **WHEN** the user runs `refs resolve` and then builds with network access disabled
- **THEN** normalized metadata is saved to the lock, the build succeeds using it, valid `.bib` data is emitted, and the lock remains byte-identical during the build

#### Scenario: Ordinary resolution retains an unchanged record
- **GIVEN** an already resolved identifier and unchanged overrides
- **WHEN** `refs resolve` runs without `--refresh`
- **THEN** its metadata remains unchanged without refetching that identifier

### Requirement: DOI provider normalizes and validates metadata
The DOI resolver SHALL normalize supported DOI prefixes, whitespace, and case and use HTTPS content negotiation requesting CSL JSON through `doi.org`. It SHALL verify returned identity and map supported provider fields into the Terse-owned reference schema without assuming all DOIs belong to one registration agency. Provider markup MUST be decoded through a bounded text/inline mapping, never executed as TeX. Malformed/executable markup, identity mismatches, and insufficient effective metadata MUST fail actionably.

#### Scenario: Normalized DOI identity
- **GIVEN** equivalent DOI declarations with prefix/case/whitespace differences and matching recorded provider responses
- **WHEN** references are explicitly resolved
- **THEN** they produce the same canonical identifier and normalized effective metadata

#### Scenario: Provider returns the wrong work
- **WHEN** a successful HTTP response contains a DOI different from the requested normalized identity
- **THEN** resolution fails and leaves the existing lock unchanged

### Requirement: arXiv provider pins exact versions
The arXiv resolver SHALL support modern and legacy IDs and optional `vN` suffixes using the Atom API's `id_list` lookup. Versionless declarations SHALL record the exact returned version and remain pinned until explicit refresh. XML parsing MUST disable external entities and DTD retrieval. Requests SHALL run sequentially with at least three seconds between provider calls, using bounded retries and server backoff. It MUST NOT scrape PDFs or article pages as fallback metadata sources.

#### Scenario: Latest-at-resolution becomes fixed metadata
- **GIVEN** a versionless arXiv declaration resolved to version `v2`
- **WHEN** later builds and non-refresh resolution run after a newer version becomes available
- **THEN** they continue using the locked `v2` record

#### Scenario: Explicit arXiv version is honored
- **WHEN** a legacy or modern ID with an explicit version is resolved
- **THEN** the requested version is verified and recorded, and a different returned version causes failure

### Requirement: Reserved providers fail explicitly
ISBN and generic URL declarations SHALL remain syntactically reserved but SHALL NOT resolve through a provider in the MVP. Resolution/checking SHALL report their unsupported provider with source context. The compiler MUST NOT invent metadata, scrape arbitrary URLs, or require manually maintained BibTeX as an implicit fallback. Ordinary hyperlinks SHALL remain independent of reference-provider support.

#### Scenario: Reserved ISBN declaration
- **WHEN** `refs resolve` encounters an ISBN-backed alias
- **THEN** it reports that ISBN resolution is unsupported, leaves the lock transaction uncommitted, and does not create a fabricated bibliography entry

### Requirement: Versioned normalized and effective reference records
`references.lock` SHALL use human-readable TOML with `lock-version = 1`, a normalization version, sorted alias tables, and fixed record-field ordering. Entries SHALL retain canonical declaration identity, exact resolved identity/version, provider/adapter identity, normalized provider data, the explicit override patch, and effective build data. Records SHALL preserve structured personal names, unparsed personal names, organizations, author/editor order, and available publication/container/identifier fields. Required effective fields SHALL include title, supported work type, and author/editor/organization or an explicit anonymous marker; missing dates SHALL remain undated. No generated timestamps, machine paths, or cache state SHALL enter serialization. Unsupported lock versions MUST fail without reinterpretation.

#### Scenario: Metadata order does not affect serialization
- **WHEN** equivalent provider objects arrive with different map ordering
- **THEN** their normalized lock records serialize byte-identically, while author order remains the provider's declared order

#### Scenario: Unparsed personal name is retained
- **WHEN** a provider supplies only a full personal name and no structured parts
- **THEN** the lock records an unparsed personal name without guessing family/given boundaries or converting the person to an organization

### Requirement: Explicit metadata overrides are sealed into the lock
The optional versioned `references.overrides.toml` SHALL accept normalized metadata patches by alias, replace arrays as whole values, and allow removal of optional fields through `remove`. Identity/provider changes, removal of required fields, and assigning/removing the same field MUST fail. Resolution SHALL apply the patch before validating effective metadata and retain both provider and effective records. Build/check SHALL compare current semantic patches with the sealed lock and fail on stale overrides rather than applying them implicitly. Formatting-only override changes MUST NOT invalidate bindings.

#### Scenario: Corrected author data works offline
- **GIVEN** a locked normalized provider record and an override correcting its title/authors
- **WHEN** `refs resolve --offline` runs and then an offline build runs
- **THEN** the effective record and `.bib` use the correction, the original provider record is retained, and no provider is called

#### Scenario: Unsealed override is diagnosed
- **WHEN** an override changes after resolution and the user runs `build`
- **THEN** the build fails with guidance to reseal it using `refs resolve`, without changing the lock or generated bibliography

### Requirement: Provider safety and transactional lock updates
Provider requests SHALL use HTTPS, a Terse user agent, bounded time/size/retries, and validated redirects excluding local/private targets. They SHALL fetch metadata only. The complete requested resolution SHALL succeed before an atomic lock replacement. Any provider, normalization, or override failure MUST leave the prior lock byte-identical. Existing undeclared entries SHALL remain until explicit `--prune`; pruning SHALL never authorize an undeclared citation.

#### Scenario: Mixed resolution fails atomically
- **GIVEN** two newly declared works and an existing lock
- **WHEN** one provider succeeds and the other fails or exceeds a response limit
- **THEN** the command fails and the previous lock remains unchanged

#### Scenario: Explicit pruning
- **GIVEN** a valid lock with an alias no longer declared in source
- **WHEN** `refs resolve --prune` succeeds
- **THEN** the undeclared entry is removed while retained entries preserve their effective metadata

### Requirement: Deterministic bibliography generation
The backend SHALL generate `.bib` records for cited works only, using stable aliases, sorted entry keys, fixed field ordering, context-aware escaping, and locked effective metadata. Citation/bibliography display SHALL preserve first-citation membership/order independently of `.bib` serialization order. Narrative, parenthetical, grouped citations, and per-work locators SHALL have valid BibLaTeX/Biber representations. Source authors MUST NOT need to write or edit generated BibTeX.

#### Scenario: Serialization and presentation ordering are independent
- **GIVEN** work `zeta` is cited before `alpha` and an unused locked work exists
- **WHEN** the project is generated and compiled
- **THEN** `.bib` contains `alpha` then `zeta`, excludes the unused work, and the displayed bibliography follows first-citation order
