## MODIFIED Requirements

### Requirement: Bounded presentation components
The compiler default SHALL be the unmodified LaTeX `article`: with no theme, or a theme that sets no property, the generated style SHALL load no font package and no `geometry`, the title SHALL be set by `\maketitle`, headings by `\section`, `\subsection`, and `\subsubsection`, and the abstract by the `abstract` environment. Every theme property SHALL be a change over that baseline, and a theme that sets only `page: size` SHALL change the paper while keeping the text-block width and margin rule the class computes for that paper. The theme schema SHALL expose typed presentation settings for page size/margins/columns; body font/color; three heading levels; title and subtitle, in paper and cover variants; all theorem-like kinds; figure alignment/percentage width/placement; table padding/rules/header; citation style; bibliography typography; logo; and watermark. A4/letter, single/two columns, paper/cover titles, author-year/numeric citations, and decimal/roman/none heading numbering SHALL be supported. Every setting the schema accepts SHALL reach the generated style: a property that validates and produces no observable difference in the emitted `terse-style.sty` or the rendered document is a defect, not an accepted value. Unknown values MUST produce property-level errors instead of being passed to LaTeX. Body size/line-height/paragraph-spacing/indentation, equation spacing and number position, figure and table captions, proof styling, citation delimiters and link color, header and footer slots, and derived contents are outside the initial schema; their selectors MUST be rejected as unknown components rather than silently accepting properties that do nothing.

#### Scenario: No theme is the plain article
- **GIVEN** a document with a title, two authors with affiliations, an abstract, headings, and paragraphs, and an empty theme file
- **WHEN** it is built and compiled
- **THEN** the generated style loads neither `fontspec` nor `geometry`, the title page is produced by `\maketitle` with both affiliations visible, the abstract is the `abstract` environment, headings are numbered by `\section`, the PDF embeds Latin Modern, and the page is the class's letter layout

#### Scenario: Page size alone changes only the paper
- **GIVEN** a theme declaring `page: size: a4` and nothing else
- **WHEN** the document is built under it and under an empty theme
- **THEN** the styles differ only in a `geometry` line, the rendered pages are A4 and letter respectively, and `\textwidth` is 345pt in both

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

### Requirement: Portable font and asset selection
With no font token the document SHALL use the LaTeX kernel's default family under the pinned engine (Latin Modern, which XeLaTeX embeds without `fontspec`) for the text of supported languages, and the style MUST NOT load `fontspec`. Font tokens SHALL select supported TeX-distributed Libertinus, Latin Modern, or TeX Gyre families by filename, without host-family discovery, and setting one SHALL load `fontspec`. Arbitrary custom font loading SHALL be unsupported in the initial theme schema. Logos SHALL be explicit root-contained local assets copied into output. Watermark labels SHALL come from localized compiler-owned `none`, `internal-use`, or `draft` choices, not arbitrary theme prose. Public example themes SHALL use redistributable demonstration assets.

#### Scenario: Theme needs no private corporate resources
- **GIVEN** a clean checkout of the public two-theme fixture
- **WHEN** the demonstration `magalu` theme is built using the documented distribution fonts
- **THEN** its logo and styling work without company repositories, private fonts, credentials, or installed host-only fonts

#### Scenario: Undeclared external font or logo
- **WHEN** a theme requests an unsupported font or an asset outside the project
- **THEN** validation fails with the theme field's location and a supported local-resource correction

#### Scenario: Default font renders accents without fontspec
- **GIVEN** a `pt-BR` document with accented prose and an empty theme
- **WHEN** it is compiled with the documented toolchain
- **THEN** the style loads no `fontspec`, the PDF embeds Latin Modern, and the engine log reports no missing character

### Requirement: Style compilation uses a stable semantic interface
The backend SHALL own trusted semantic macro/environment templates, counters, anchors, and traversal. Under the compiler default those templates SHALL delegate to the `article` class: the title block SHALL collect title, subtitle, authors with affiliations, and date and run `\maketitle`, setting an empty date when the document has none so no build timestamp is inserted; the abstract SHALL use the `abstract` environment; headings SHALL use `\section`, `\subsection`, and `\subsubsection`, with `numbering: none` using the unnumbered form while keeping an anchor and `numbering: roman` redefining the counter presentation. Theme compilation SHALL supply only validated presentation values. For identical content and target configuration, switching themes SHALL keep generated main `.tex` and `.bib` bytes identical and express appearance through `terse-style.sty` and theme assets. Authored references SHALL retain valid anchors even when number presentation changes; unnumbered headings SHALL use title references and proofs SHALL use proof labels/explicit relationships.

#### Scenario: Only presentation changes between builds
- **WHEN** identical inputs are built into academic and demonstration corporate destinations
- **THEN** their main TeX and bibliography bytes match, their style bytes differ, and both compile with all authored references resolved

#### Scenario: Heading numbering is disabled
- **WHEN** a theme disables heading numbering for a referenced section
- **THEN** the section still has an anchor and the reference uses its title rather than a missing number

#### Scenario: Default title block is maketitle
- **GIVEN** a document with two authors carrying affiliations, a subtitle, and no date
- **WHEN** it is built with an empty theme and compiled
- **THEN** the rendered title shows both authors with their affiliations and the subtitle, and neither the generated files nor the PDF contain a build date
