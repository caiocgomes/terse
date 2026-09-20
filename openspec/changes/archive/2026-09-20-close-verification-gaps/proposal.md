## Why

A verification pass over the `terse` change on 2026-09-20 passed every check that counting can express: 154/156 tasks, 83 requirements, 146 scenarios, all 146 named tests present in the code, 346 tests passing with zero failures, and the heavy lane green on both the host and the pinned container. Reading the assertions and the emitting code instead of the names told a different story.

Four SHALL clauses have no implementation behind checked task boxes, and the theme layer, which is the product's headline promise, is largely inert. Of the seven theme components that accept properties, **eight individual properties validate and produce no effect**: `generate_style` never loads `geometry`, never emits a body color, never passes `style=` to BibLaTeX; figure alignment and placement are hardcoded in the theme-blind body; and `\TerseLogo` is defined and never invoked anywhere. The consequence is observable in the committed fixture: `academic.theme` declares A4, 2.5 cm margins and author-year citations, `magalu.theme` declares 2 cm margins, numeric citations and a logo, and both rendered PDFs come out with the `article` class defaults, identical numeric citations, and no logo on the page. `body: font` is only a `\RequirePackage`, so `libertinus-otf` and `tgpagella` work by accident (their packages set `\rmdefault`) while `tex-gyre-heros` sets only the sans family and leaves the magalu body in serif, contradicting the requirement that fonts are selected by filename.

None of this was caught because every affected test either asserts on a theme-blind function, or asserts `is_ok()`, or compares body bytes that are invariant by construction. The tests are honest about names and silent about substance.

## What Changes

- **Theme settings that validate now take effect.** `page.size`/`page.margin` load `geometry`; `page.columns` is honored; `body.color` is emitted; `body.font` emits `\setmainfont` by filename; `citation.style` reaches BibLaTeX; figure alignment and placement move from hardcoded body text into style macros, preserving the theme-blind body invariant; `\TerseLogo` is placed on the page.
- **Four new theme components** gain typed settings and style emission: `title` (paper and cover variants), `theorem` (with the `theorem[kind=...]` role selector the precedence requirement already demands), `table`, and `bibliography`. The body gains one theme-blind `TerseTitleBlock` environment so a cover can be a layout decision rather than a font size.
- **Five theme components are removed from the spec**, not silently left broken: `header`, `footer`, `contents`, `equation`, and `proof`. The magalu scenario loses its "running header" clause for the same reason. They return in a later change.
- **`paper.map.json` is emitted.** The spec lists it among default deliverables; `source_map.rs` exists, works, and is reachable only from tests. It joins the artifact plan, the build manifest's hashes, and the reproducibility comparison, and is excluded from export membership.
- **Resource limits cover all four documented categories**: source bytes and node count and include depth join structural nesting and diagnostic count.
- **The canonical projection retains effective bibliography records**, so two documents differing only in locked reference metadata stop producing an identical digest.
- **Unresolved references fail the build** instead of publishing a PDF containing `??`, scanning only the settled final XeLaTeX pass.
- **Test-strength repairs**: assertions that check `is_err()` where the scenario demands a code and a position, a cycle route asserted without its order or closing edge, an export cruft scenario that plants none of the four cruft kinds it names, a watch successor never asserted, and JSON stdout isolation proven only at library level.
- **Housekeeping**: the fixture-group contract is reconciled with the committed layout, and the declared MSRV is either exercised or lowered to what is actually tested.

This change does not add capabilities. It makes the specification and the implementation agree, in both directions, and records which direction each gap moved.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `themes`: narrow the component enumeration to what will exist, keep theorem-base inheritance (now implemented), and amend the two-theme scenario to drop the running header.
- `semantic-ast`: no requirement text changes; the resource-limit and projection requirements are satisfied by implementation.
- `latex-generation`: no requirement text changes; `paper.map.json` and the undefined-reference failure are satisfied by implementation.

## Impact

- Code: `crates/terse-core/src/latex/mod.rs` (style emission and the two hardcoded body sites), `crates/terse-core/src/theme/{resolve.rs,mod.rs}`, `crates/terse-core/src/{artifact/mod.rs,artifact/export.rs,latex/source_map.rs,semantic/projection.rs,source/mod.rs,syntax/blocks.rs,project/expand.rs}`, `crates/terse-cli/src/build.rs`.
- Pinned toolchain: `geometry` (and whatever `page.columns` needs) enter `STYLE_PACKAGES` and both profiles, so the closure must be re-derived with `scripts/derive-toolchain-closure.sh` and the managed prefix updated. Neither package is installed today.
- **Behavioral break worth stating plainly**: every existing project's `terse-style.sty` changes bytes, and documents whose themes declare page geometry or citation style will render differently, correctly, for the first time. Style goldens are reviewed as a behavior change, never regenerated in bulk.
- Corrects the record on `terse` tasks 9.3, 10.6, 16.4, 17.4, 17.5 and 1.3, which claimed work that this change actually performs. Task 9.4 is genuinely done and is not touched.
- One observation recorded for a future change rather than fixed here: `semantic::projection::project()` has no caller outside tests, so the theme-invariant projection is verified but never computed during a real build.
