## MODIFIED Requirements

### Requirement: Independent declarative theme files
Themes SHALL be independent UTF-8 `.theme` files using the language's indentation/scalar conventions and a separate closed schema. Selectors SHALL address supported semantic components and a closed set of variant attributes, namely the exact `figure[role=wide]` and `theorem[kind=<theorem-like kind>]` forms; the attribute key is fixed per component, so `theorem[role=…]` and `figure[kind=…]` MUST fail validation. Duplicate selectors/properties, unknown properties, per-ID selectors, combinators, imports, arbitrary expressions, raw TeX, executable hooks, and content-selection conditions MUST fail validation. Themes MUST NOT contain authored document prose.

#### Scenario: Supported semantic selector
- **WHEN** a theme defines base `figure` settings and `figure[role=wide]` settings
- **THEN** both selectors validate and apply only to their corresponding semantic component/role

#### Scenario: Theme attempts to select authored content
- **WHEN** a theme contains an ID selector, conditional inclusion, a prose replacement, or an omit/reorder rule
- **THEN** validation fails at that rule rather than transforming the document

#### Scenario: Variant attribute key is fixed per component
- **WHEN** a theme declares `theorem[kind=lemma]`, `theorem[role=lemma]`, and `figure[kind=wide]`
- **THEN** the first validates while the other two fail as invalid selectors, and `theorem[kind=lemma]` is not treated as a duplicate of `theorem[role=lemma]`

### Requirement: Bounded presentation components
The theme schema SHALL expose typed presentation settings for page size/margins/columns; body font/color; three heading levels; title and subtitle, in paper and cover variants; all theorem-like kinds; figure alignment/percentage width/placement; table padding/rules/header; citation style; bibliography typography; logo; and watermark. A4/letter, single/two columns, paper/cover titles, author-year/numeric citations, and decimal/roman/none heading numbering SHALL be supported. Every setting the schema accepts SHALL reach the generated style: a property that validates and produces no observable difference in the emitted `terse-style.sty` or the rendered document is a defect, not an accepted value. Unknown values MUST produce property-level errors instead of being passed to LaTeX. Body size/line-height/paragraph-spacing/indentation, equation spacing and number position, figure and table captions, proof styling, citation delimiters and link color, header and footer slots, and derived contents are outside the initial schema; their selectors MUST be rejected as unknown components rather than silently accepting properties that do nothing.

#### Scenario: Corporate and academic presentations
- **GIVEN** an academic theme with a conventional paper title and a demonstration `magalu` theme with a cover, logo, and internal-use watermark
- **WHEN** both themes are resolved and the same paper is compiled
- **THEN** the requested presentation differences appear through the supported components

#### Scenario: Invalid typed value
- **WHEN** a theme supplies a nonnumeric width, unknown citation style, or unsupported page size
- **THEN** checking reports the selector, property, and accepted value type without invoking a LaTeX engine

#### Scenario: Declared settings reach the output
- **GIVEN** two themes differing in page size, margin, columns, body font, body color, citation style, figure placement, and logo
- **WHEN** the same document is compiled under each
- **THEN** every one of those differences is observable in the generated style or the rendered PDF, and none of them alters the generated body bytes

#### Scenario: Deferred component is not silently accepted
- **WHEN** a theme declares a `header`, `footer`, `contents`, `equation`, or `proof` selector
- **THEN** validation fails naming the selector as an unknown component, rather than accepting it and discarding its properties

### Requirement: Deterministic rule precedence
Specific role settings SHALL override base component settings, which SHALL override documented compiler defaults. The theorem base SHALL provide defaults for theorem-like variants, with kind-specific settings overriding it. File ordering MUST NOT create an implicit cascade. The same validated property set SHALL resolve identically regardless of selector declaration order.

#### Scenario: Wide figure override
- **GIVEN** base figure width `75%` and wide-role width `100%`
- **WHEN** ordinary and wide figures are styled
- **THEN** they receive their respective widths without changing source attributes

#### Scenario: Reordered selectors
- **WHEN** distinct nonduplicate theme selectors are reordered without changing values
- **THEN** the resolved presentation settings remain equal

#### Scenario: Theorem kind inherits its base
- **GIVEN** a `theorem` base setting and a differing `theorem[kind=lemma]` setting
- **WHEN** a theorem and a lemma are styled
- **THEN** the lemma uses the kind-specific value, the theorem uses the base value, and every other theorem-like kind inherits the base
