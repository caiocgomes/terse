## Why

Authors need to publish the same paper in corporate and academic presentations without duplicating prose or maintaining LaTeX boilerplate. Terse will separate semantic content from presentation while delivering readable, editable LaTeX projects that remain usable independently of the compiler.

## What Changes

- Introduce a concise UTF-8 document language, provisionally `.trs`, with explicit structural syntax and specified indentation. Cover metadata (title, subtitle, authors, affiliations, date, language, abstract, keywords), three heading levels, paragraphs, inline formatting, links, code, footnotes, lists, TeX mathematics, equations, figures with captions/alt text/roles, basic tables, theorem/proposition/lemma/definition/example/remark/proof blocks, citations, bibliographies, labels, cross-references, includes, and explicit raw TeX. Preserve the brief's authoring model and comparable concision.
- Introduce bounded, declarative `.theme` files for page settings, typography, spacing, headings, title material, theorem-like blocks, equations, figures, tables, citations, bibliography, headers, footers, logos, and watermarks. Academic and Magalu presentations must preserve every authored element and its order. Themes may add page furniture and derived presentation elements, but contain no document prose or content selection, suppression, reordering, or rewriting rules. Ordinary content rejects visual properties; semantic roles remain permitted.
- Generate complete LaTeX projects containing readable `.tex`, a style layer normally emitted as `.sty`, generated `.bib`, required assets, and optional `.bbl`/PDF outputs. Prefer a shared semantic TeX body across themes. Output must use relative paths, escape ordinary text without modifying raw math/TeX, and compile without Terse, network access, or original-machine state using a documented Unicode-capable toolchain.
- Add explicit bibliography resolution into a human-readable, versioned `references.lock`, including normalized metadata and explicit user overrides. DOI and arXiv resolution are required; ISBN and URL syntax is reserved if reliable resolution is deferred. Narrative, parenthetical, grouped citations, and locators are distinct semantics. Normal checks/builds use locked data without network access or lockfile mutation and fail actionably on unknown or unresolved cited aliases.
- Provide `init`, `check`, `fmt`, `fmt --check`, `refs resolve`, `build`, `watch`, and `export --target arxiv`. Define manifest/CLI/environment precedence, explicit module includes, project-wide symbols, dependency tracking, deterministic formatting without gratuitous paragraph reflow, and noninteractive CI behavior. Builds support `--tex-only` and optional PDF compilation when a configured compatible engine is available. Watch preserves the last valid output after errors and rebuilds after corrections.
- Produce self-contained arXiv directories and ZIP archives with file manifests, only required source/style/bibliography/assets, dependency checks, and clean-directory compilation when a compatible engine is available. Report validation limits honestly; never upload or claim compatibility that was not validated.
- Make source-located diagnostics, deterministic artifacts, safe path handling, and atomic publication part of the product contract. Strict checking warns about raw TeX's portability boundary. Ordinary source is declarative; shell escape is disabled by default, subprocesses avoid shell-string interpolation, network activity requires explicit reference resolution, and includes/assets outside the project root are rejected by default. Packaging prevents traversal and unintended inclusion; writes cannot overwrite unrelated files outside their configured scope.

### Non-goals

The MVP excludes replacement math notation, browser/WYSIWYG editing, prose rewriting or audience-specific content selection, CSS compatibility, exhaustive LaTeX class/package support, reverse conversion from arbitrary TeX, multiplayer editing, automatic submission, a Zotero-equivalent manager or integration, title-based reference search, secondary output backends, and plugins or arbitrary executable document code. Raw TeX is an explicit escape hatch with separate trust and portability limits.

## Capabilities

### New Capabilities

- `language-parsing`: Formal grammar, consistent whitespace, concise block/inline syntax, TeX math/raw blocks, and precise rejection of unsupported or presentation-only content attributes.
- `semantic-ast`: Presentation-independent document meaning, ordered content, complete element coverage, source spans, and backend boundaries that allow future targets without implementing them now.
- `themes`: Validated typographic tokens and semantic component/role rules, theme resolution, style compilation, and enforceable preservation of authored content.
- `latex-generation`: Portable human-readable projects, semantic macros, escaping, stable asset copying, Unicode strategy, optional safe engine execution, and generated-to-source mapping.
- `citations`: Aliases, citation forms and locators, DOI/arXiv providers, reserved identifiers, explicit metadata overrides, stable lockfile serialization, offline validation, and generated bibliography records.
- `multi-file-projects`: Manifest configuration, documented path resolution, explicit nested module includes, shared identifiers, complete cycle diagnostics, duplicate-definition locations, root confinement, and transitive dependencies.
- `git-oriented-tooling`: CLI contracts, safe initialization and additive `.gitignore` updates, idempotent source/theme formatting, deterministic text generation, disposable caches, meaningful exit codes, and CI operation.
- `diagnostics`: Stable codes, severity, original filenames and available line/column positions, related locations, reliable correction suggestions, JSON output, and mapped LaTeX errors where possible.
- `watch-mode`: Debounced watching of transitive source/theme/reference/asset dependencies, dependency updates, recovery after failures, and preservation of successful output.
- `arxiv-export`: Self-contained directory/ZIP packaging, bibliography material, included-file manifests, dependency/path checks, exclusion of unused files and machine metadata, clean compilation, and explicit validation reporting.

### Modified Capabilities

## Impact

Terse is the author's personal project and will be released as open source. Plan for one primary maintainer, reproducible public examples and contribution instructions, and a license to be selected by the author. Magalu is a theme demonstration, not a corporate ownership or sponsorship assumption; public builds and tests must be reproducible with redistributable example assets.

This is a new compiler and CLI with no existing capability specifications to modify. Rust is preferred for a single cross-platform executable; the technical design will justify language/toolchain choices and define grammar, AST/source maps, compiler stages, theme compilation, citation providers, deterministic serialization, cache boundaries, LaTeX invocation, and packaging. Compatible LaTeX/bibliography tools are optional external dependencies for PDF generation; metadata services are used only by explicit resolution.

Implementation will proceed in end-to-end milestones. Automated tests must cover every acceptance gate:

| Scenario | Required evidence |
| --- | --- |
| A | One paper, two visibly different themes, identical ordered semantic content. |
| B | Generated project compiles offline on a clean compatible toolchain without Terse. |
| C | Explicit DOI resolution produces locked metadata and valid bibliography for offline builds. |
| D | Unknown citation fails with original location, alias, and resolution guidance. |
| E | Cross-file equation reference resolves with valid numbering and link. |
| F | Indirect include cycle fails before generation and reports the complete cycle. |
| G | Identical inputs, compiler version, command, and environment-independent configuration produce byte-identical text artifacts in clean builds. |
| H | Second formatting pass changes nothing; `fmt --check` succeeds. |
| I | Visual properties in content receive precise validation errors. |
| J | Raw TeX is preserved; strict checking reports its portability limit. |
| K | Watch retains valid output through a syntax error and rebuilds after correction. |
| L | Export produces a self-contained ZIP and manifest, rejects missing/external dependencies, and never uploads. |

Deliverables include a committed multi-file paper fixture with academic and Magalu themes, reproducible citation metadata, and all required assets; automated coverage for the additional MVP requirements, including arXiv resolution; and installation, project layout, language, theme, citation, Git workflow, and export documentation. The release must support the complete clone/install/init/author/check/format/build/inspect/PDF/export workflow through one documented installation command, without hidden files, manually maintained BibTeX, manual repairs to generated TeX, network-dependent normal builds, or theme-specific edits to content.
