## MODIFIED Requirements

### Requirement: Explicit offline export target
`terse export --target arxiv` SHALL build from current authoritative inputs using an explicit versioned offline compatibility profile, initially `texlive-2025-xelatex`. It SHALL validate selected theme/content, engine/package/font assumptions, assets, and bibliography strategy without fetching profile updates or packaging an unchecked previous build. The profile's package list SHALL equal the set of packages the generated style can require, so a package the compiler never emits MUST NOT appear in the profile. Unknown profiles and unsupported target dependencies MUST fail. The command MUST NOT upload, submit, or contact arXiv.

#### Scenario: Current source controls export
- **GIVEN** an old successful build and current source containing an unresolved reference
- **WHEN** arXiv export runs
- **THEN** export fails on current input instead of archiving the old build

#### Scenario: Profile packages equal the emitted set
- **WHEN** the profile package list is compared with every package name the style generator can emit
- **THEN** the two sets are equal

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
