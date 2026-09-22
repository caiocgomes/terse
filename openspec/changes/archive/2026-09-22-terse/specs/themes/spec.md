## ADDED Requirements

### Requirement: Independent declarative theme files
Themes SHALL be independent UTF-8 `.theme` files using the language's indentation/scalar conventions and a separate closed schema. Selectors SHALL address supported semantic components and the exact `figure[role=wide]` variant. Duplicate selectors/properties, unknown properties, per-ID selectors, combinators, imports, arbitrary expressions, raw TeX, executable hooks, and content-selection conditions MUST fail validation. Themes MUST NOT contain authored document prose.

#### Scenario: Supported semantic selector
- **WHEN** a theme defines base `figure` settings and `figure[role=wide]` settings
- **THEN** both selectors validate and apply only to their corresponding semantic component/role

#### Scenario: Theme attempts to select authored content
- **WHEN** a theme contains an ID selector, conditional inclusion, a prose replacement, or an omit/reorder rule
- **THEN** validation fails at that rule rather than transforming the document

### Requirement: Bounded presentation components
The theme schema SHALL expose typed presentation settings for page size/margins/columns; body font/size/line height/paragraph spacing/indentation/color; three heading levels; title and subtitle; all theorem-like kinds and proof; equation spacing/number position; figure alignment/percentage width/placement/captions; table padding/rules/header/captions; citation style/delimiters/link color; bibliography typography; header/footer slots; logo; watermark; and derived contents. A4/letter, single/two columns, paper/cover titles, author-year/numeric citations, and decimal/roman/none heading numbering SHALL be supported. Unknown values MUST produce property-level errors instead of being passed to LaTeX.

#### Scenario: Corporate and academic presentations
- **GIVEN** an academic theme with a conventional title and a demonstration `magalu` theme with a cover, logo, running header, and internal-use watermark
- **WHEN** both themes are resolved and the same paper is compiled
- **THEN** the requested presentation differences appear through the supported components

#### Scenario: Invalid typed value
- **WHEN** a theme supplies a nonnumeric width, unknown citation style, or unsupported page size
- **THEN** checking reports the selector, property, and accepted value type without invoking a LaTeX engine

### Requirement: Deterministic rule precedence
Specific role settings SHALL override base component settings, which SHALL override documented compiler defaults. The theorem base SHALL provide defaults for theorem-like variants, with kind-specific settings overriding it. File ordering MUST NOT create an implicit cascade. The same validated property set SHALL resolve identically regardless of selector declaration order.

#### Scenario: Wide figure override
- **GIVEN** base figure width `75%` and wide-role width `100%`
- **WHEN** ordinary and wide figures are styled
- **THEN** they receive their respective widths without changing source attributes

#### Scenario: Reordered selectors
- **WHEN** distinct nonduplicate theme selectors are reordered without changing values
- **THEN** the resolved presentation settings remain equal

### Requirement: Themes preserve complete ordered content
Themes SHALL operate on immutable semantic content and MUST NOT omit, add authored prose, duplicate authored nodes, reorder, or rewrite metadata, paragraphs, headings, equations, figures, tables, labels, references, citation groups, or bibliography membership/order. All provided metadata SHALL render in the semantic order defined by the document model. Derived page furniture, table of contents, running title, numbering, and localized bibliography headings SHALL remain separate presentation elements. Float placement SHALL affect page layout, not the emitted semantic node sequence.

#### Scenario: Same content under different themes [Acceptance A]
- **GIVEN** one valid multi-file document with complete metadata and all MVP elements
- **WHEN** it is built with `academic` and `magalu`
- **THEN** semantic projections and emitted authored node sequences are equal while style files and rendered presentation differ

#### Scenario: Title cover retains metadata
- **WHEN** a cover theme renders a document with subtitle, affiliations, abstract, date, and keywords
- **THEN** every provided metadata field is rendered without being discarded to fit the cover layout

### Requirement: Theme settings cannot deliberately hide content
Theme validation SHALL require finite positive text sizes, nonnegative spacing, usable page content boxes, widths within their container, and documented versioned numeric bounds. It MUST reject zero/transparent text, equal foreground/background text colors, clipping/off-page transformations, or furniture layouts that cover authored body content. Watermarks SHALL be drawn behind content with bounded opacity. The compiler SHALL diagnose unsupported hiding mechanisms rather than implement them.

#### Scenario: Invalid geometry or visibility
- **WHEN** a theme sets zero body size, margins consuming the whole page, width above the container limit, or matching text/background colors
- **THEN** checking fails at the offending property before generation

#### Scenario: Watermark remains furniture
- **WHEN** a valid internal-use watermark is enabled
- **THEN** the rendering retains readable authored content and the watermark is not inserted into the authored AST

### Requirement: Portable font and asset selection
Initial font tokens SHALL select supported TeX-distributed Libertinus, Latin Modern, or TeX Gyre families by filename, without host-family discovery. Arbitrary custom font loading SHALL be unsupported in the initial theme schema. Logos SHALL be explicit root-contained local assets copied into output. Watermark labels SHALL come from localized compiler-owned `none`, `internal-use`, or `draft` choices, not arbitrary theme prose. Public example themes SHALL use redistributable demonstration assets.

#### Scenario: Theme needs no private corporate resources
- **GIVEN** a clean checkout of the public two-theme fixture
- **WHEN** the demonstration `magalu` theme is built using the documented distribution fonts
- **THEN** its logo and styling work without company repositories, private fonts, credentials, or installed host-only fonts

#### Scenario: Undeclared external font or logo
- **WHEN** a theme requests an unsupported font or an asset outside the project
- **THEN** validation fails with the theme field's location and a supported local-resource correction

### Requirement: Style compilation uses a stable semantic interface
The backend SHALL own trusted semantic macro/environment templates, counters, anchors, and traversal. Theme compilation SHALL supply only validated presentation values. For identical content and target configuration, switching themes SHALL keep generated main `.tex` and `.bib` bytes identical and express appearance through `terse-style.sty` and theme assets. Authored references SHALL retain valid anchors even when number presentation changes; unnumbered headings SHALL use title references and proofs SHALL use proof labels/explicit relationships.

#### Scenario: Only presentation changes between builds
- **WHEN** identical inputs are built into academic and demonstration corporate destinations
- **THEN** their main TeX and bibliography bytes match, their style bytes differ, and both compile with all authored references resolved

#### Scenario: Heading numbering is disabled
- **WHEN** a theme disables heading numbering for a referenced section
- **THEN** the section still has an anchor and the reference uses its title rather than a missing number
