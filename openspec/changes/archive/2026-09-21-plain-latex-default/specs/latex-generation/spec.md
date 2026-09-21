## MODIFIED Requirements

### Requirement: Documented Unicode and bibliography toolchain
The initial backend SHALL target XeLaTeX with the bounded supported package set and BibLaTeX/Biber for bibliography processing. With no font token the kernel's Latin Modern SHALL carry the text of supported languages without `fontspec`; font tokens SHALL use supported distribution filenames and supported language mappings. `.bib` and source text SHALL support Unicode metadata/input; known missing glyphs during compilation MUST fail with actionable diagnostics. Without engine execution, the command SHALL identify that rendering/glyph coverage was not validated. Additional engines MUST NOT be silently substituted.

#### Scenario: Unicode paper compiles
- **GIVEN** a supported-language paper and bibliography containing accented author names and prose supported by the configured fonts
- **WHEN** the documented toolchain compiles it
- **THEN** those characters are retained without transliteration or substitution

#### Scenario: Missing glyph is reported
- **WHEN** compilation reports a missing glyph for authored text
- **THEN** the build fails with the available source/context and does not publish a misleading successful PDF
