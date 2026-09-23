## ADDED Requirements

### Requirement: Code blocks render through listings
The style layer SHALL load `listings` and define a semantic `TerseCode` environment over it only for a document containing at least one code block; a document with none SHALL NOT require `listings` to compile. Without a theme, it SHALL present code in the default monospace family at the body size, with keywords bold, comments italic, no color, straight quotes, preserved spaces, and long lines wrapped. Each code block SHALL be emitted once, in source order, as `\begin{TerseCode}`, its content bytes unchanged and unescaped, and `\end{TerseCode}`. A language tag present in the documented closed map SHALL add the mapped `listings` language as the environment's option. Any other tag, or no tag, SHALL emit the environment without a language option and SHALL NOT be an error. An author's tag MUST NOT reach LaTeX except as a mapped language name. Engines SHALL keep running with shell escape disabled. Because `listings` is emitted by the generator, it SHALL belong to the style package set that the export profile and the pinned toolchain closure must cover.

#### Scenario: Tagged Python block
- **WHEN** a document with a ```` ```python ```` block is generated
- **THEN** the body contains `\begin{TerseCode}[language=Python]`, the exact code lines, and `\end{TerseCode}`, and the style loads `listings` and defines `TerseCode`

#### Scenario: Unknown or missing tag
- **WHEN** a block is tagged `rust`, tagged `x]{evil}`, or untagged
- **THEN** each is emitted as `\begin{TerseCode}` with no option and the build succeeds

#### Scenario: Code is inert in the PDF
- **GIVEN** a code block containing `\input{secret.txt}`, `\write18{touch PWNED}`, and `^^5cinput`
- **WHEN** the document is compiled
- **THEN** those sequences appear literally in the PDF text, no file is read or created, and the compile succeeds

#### Scenario: A document without code blocks never requires listings
- **WHEN** the style is generated for a document with no code block, and separately for one with a ```` ```python ```` block
- **THEN** only the second style contains `\RequirePackage{listings}` and `TerseCode`; the two styles differ

#### Scenario: Package set stays closed
- **WHEN** the export profile's package list is compared with the style generator's emittable set
- **THEN** both contain `listings`, and the pinned toolchain closure derived from the full-paper fixture (which contains a code block) includes the `listings` package
