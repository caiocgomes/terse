# arXiv Export Specification

## Purpose
Produces a self-contained, deterministic arXiv source package (directory, ZIP, and manifest) from current inputs against a versioned offline compatibility profile, validates it by clean extraction and compilation, and publishes it transactionally without ever contacting arXiv.
## Requirements
### Requirement: Explicit offline export target
`terse export --target arxiv` SHALL build from current authoritative inputs using an explicit versioned offline compatibility profile, initially `texlive-2025-xelatex`. It SHALL validate selected theme/content, engine/package/font assumptions, assets, and bibliography strategy without fetching profile updates or packaging an unchecked previous build. The profile's package list SHALL equal the set of packages the generated style can require, so a package the compiler never emits MUST NOT appear in the profile. Unknown profiles and unsupported target dependencies MUST fail. The command MUST NOT upload, submit, or contact arXiv.

#### Scenario: Current source controls export
- **GIVEN** an old successful build and current source containing an unresolved reference
- **WHEN** arXiv export runs
- **THEN** export fails on current input instead of archiving the old build

#### Scenario: Profile packages equal the emitted set
- **WHEN** the profile package list is compared with every package name the style generator can emit
- **THEN** the two sets are equal

### Requirement: Self-contained source directory and ZIP
Successful export SHALL produce a self-contained source directory and ZIP archive with main `.tex`, required `.sty`, generated `.bib`, permitted optional `.bbl`, all used figures/theme assets, and `MANIFEST.json`. The default layout SHALL be `build/export/<theme>/arxiv/` with a sibling `<entry>-arxiv.zip`; ZIP members SHALL be rooted at the archive root. Standard distribution dependencies SHALL be documented, and required custom generated styles SHALL be included. Compilation MUST require neither Terse nor the original project nor network access.

#### Scenario: arXiv package [Acceptance L]
- **GIVEN** a valid semantic-only academic paper with locked references and referenced assets
- **WHEN** it is exported and compiled using a compatible clean toolchain
- **THEN** the directory, ZIP, and manifest are produced, the extracted package compiles without Terse/network/source-tree access, and no upload occurs

### Requirement: Package membership is minimal and explicit
Export SHALL copy only the validated artifact allowlist, not recursively archive the source/build directory. It MUST exclude logs, caches, editor files, unused assets, source maps, original `.trs` files, host metadata, and the rendered paper PDF. Referenced PDF figures SHALL remain included. Custom executable extensions SHALL be subject to the explicit rejection policy below. Validation reports SHALL remain outside the source archive.

#### Scenario: Unused files and rendered paper are excluded
- **GIVEN** referenced figure PDFs alongside an unused image, editor backup, log, and compiled paper PDF
- **WHEN** export builds its file set
- **THEN** the referenced figure PDFs are included and the unused/editor/log/rendered-paper files are absent

### Requirement: Paths and dependency closure are verified
Export SHALL reject absolute paths, traversal, root/package escapes, unsafe symlinks, missing assets, case-fold output collisions, and dependencies outside the packaged files or known compatible distribution resources. It SHALL verify paths before copying/archiving. Existing successful export artifacts MUST remain intact after validation failure. No font/package/image conversion or metadata fetch SHALL repair an invalid export implicitly.

#### Scenario: Missing or external dependency [Acceptance L]
- **WHEN** an asset is absent or resolves outside the project/package boundary
- **THEN** export fails with the original reference location, explains the dependency problem, and publishes no replacement archive

#### Scenario: Portable filename collision
- **WHEN** distinct output members would collide on a case-insensitive filesystem
- **THEN** export diagnoses the collision rather than creating an archive whose behavior depends on the extraction host

### Requirement: Bibliography material respects the selected profile
Default export SHALL include `.bib` and omit prebuilt `.bbl`. `--include-bbl` SHALL require verified compatible bibliography tooling and a `.bbl` matching the main TeX stem. Incompatible or unverifiable `.bbl` data MUST cause an explicit failure. The package SHALL preserve complete cited bibliography content and require no manual BibTeX edits. The bibliography source and selected processor SHALL agree.

#### Scenario: Default export avoids local BBL version coupling
- **GIVEN** a normal build containing a locally generated `.bbl`
- **WHEN** default arXiv export succeeds
- **THEN** the ZIP contains the generated `.bib` and excludes that `.bbl`, allowing the compatible target to process bibliography sources

#### Scenario: Requested incompatible BBL is rejected
- **WHEN** `--include-bbl` is requested without a verified matching bibliography profile or main-file stem
- **THEN** export fails with a bibliography compatibility diagnostic and does not silently include or discard the requested file

### Requirement: Raw TeX and custom executable extensions are unsupported for MVP export
MVP arXiv export SHALL reject raw `tex:` blocks and additional custom executable support files/packages whose dependency closure cannot be verified. It SHALL use `E-EXPORT-004` with each offending source/configuration location. Successful ordinary LaTeX compilation MUST NOT waive this policy. Export MUST NOT remove or rewrite unsupported authored content to make the package pass; normal build support for raw TeX remains separate.

#### Scenario: Normally valid raw drawing is not silently omitted
- **GIVEN** a raw drawing that compiles successfully in a normal build
- **WHEN** arXiv export runs
- **THEN** it fails with `E-EXPORT-004` at the raw block, explains the MVP limitation, and leaves the authored source and any prior successful export unchanged

### Requirement: Stable file manifest and archive metadata
The manifest SHALL list sorted relative payload paths with sizes and content hashes and explicitly exclude its own hash from that list. Export SHALL verify the manifest after extracting the finished ZIP. Archive member order, representable timestamp epoch, permissions, platform attributes, and compression settings SHALL be fixed; source modification times and owner/machine information MUST NOT leak into members. Identical inputs/compiler/profile SHALL yield identical manifests and source-package ZIP bytes.

#### Scenario: Repeated export is deterministic
- **GIVEN** identical committed inputs but different source-file mtimes and clean output directories
- **WHEN** source packages are exported twice with the same compiler/profile
- **THEN** manifests and ZIP bytes match, and every listed payload hash matches the extracted file

### Requirement: Clean extracted-package validation reports actual coverage
Export SHALL extract the completed ZIP into a clean temporary directory and compile it when compatible local tooling is available, using controlled resource paths, an empty user TeX tree, and the prepared child environment. It SHALL inspect dependency recording to distinguish packaged inputs from known distribution resources, obtained through the bounded runner, and reject unintended inputs. Validation results SHALL report `static-only`, `compiled-local`, or `compiled-profile` according to actual checks. `compiled-profile` SHALL require a verified engine year and every profile package, font, and language definition resolvable in the toolchain that compiled the package; a matching engine banner alone MUST yield `compiled-local` with the unverified items recorded. No result SHALL claim guaranteed arXiv acceptance.

#### Scenario: Local toolchain differs from target profile
- **WHEN** the extracted package compiles with compatible local tools whose full target-profile match has not been verified
- **THEN** the report says `compiled-local`, records the tested assumptions, and does not claim an exact arXiv environment match

#### Scenario: No compatible engine is available
- **GIVEN** a statically valid package within the supported semantic-only subset
- **WHEN** export runs without a compatible engine
- **THEN** it produces an explicitly `static-only` report describing the missing compilation check; with `--require-compile`, it instead fails and preserves the previous export

#### Scenario: Clean compilation uncovers a hidden dependency
- **WHEN** extracted-package compilation tries to use a file outside the package or approved distribution dependencies
- **THEN** validation fails actionably and no replacement export is published

#### Scenario: Banner year alone is not a profile match
- **GIVEN** a toolchain whose `xelatex --version` names the profile year but lacks one profile font
- **WHEN** export validation compiles the extracted package
- **THEN** the report is `compiled-local` and names the missing font among the unverified assumptions

### Requirement: Export publication is transactional
Source directory, archive, and reports SHALL be staged and validated before replacing a managed export generation. A generation manifest/transaction SHALL keep these outputs consistent through publication failures and interrupted writes. Export MUST remain within its configured output scope and SHALL never overwrite authoritative sources or unrelated files.

#### Scenario: Archive or validation fails after staging
- **GIVEN** a previous successful export
- **WHEN** archive creation, extraction verification, or requested compilation fails
- **THEN** the previous directory/ZIP/reports remain a consistent valid generation and the command returns failure

