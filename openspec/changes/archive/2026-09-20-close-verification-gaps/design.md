## Context

The `terse` implementation is complete by every count and green in every lane; the verification that prompted this change found that several of those greens are vacuous. The pattern is uniform: a theme property is parsed, validated, bounds-checked, stored in `ResolvedTheme`, and then never read by `generate_style`; or a module is written, unit-tested, and never wired into the pipeline that produces artifacts. Both failure modes survive a test suite that asserts on names, on `is_ok()`, or on functions that are invariant by construction.

The constraint that shapes every decision below is the theme-blind body. `latex::generate_document` must produce byte-identical output under any theme, because that invariant is what makes "same content, two presentations" verifiable at all. Every presentation setting therefore has exactly one legal path: `ResolvedTheme` → a macro or preamble line in `terse-style.sty` → referenced by a fixed name from the body. `\TerseFigureWidth` already does this correctly and is the template for the rest.

## Goals / Non-Goals

**Goals:** every accepted theme property produces an observable difference; the deliverable set matches the specification; the four documented resource-limit categories all exist; unresolved references fail rather than publish `??`; the spec loses the components this change will not implement, in writing, with history.

**Non-Goals:** header, footer, contents, equation and proof components; spanning wide figures across both columns in two-column mode; making `project()` part of the build pipeline; any new capability.

## Decisions

**Two-column via `\AtBeginDocument{\twocolumn}`, not `multicol`.** The `.sty` is loaded after `\documentclass`, so a class option is unavailable without touching the theme-blind body. Of the two remaining mechanisms, `multicol` handles floats badly and this document model has figures and tables, while LaTeX's native `\twocolumn` keeps the standard float machinery. It also needs no new package, so only `geometry` enters the closure. Wide-role figures span both columns correctly under this choice, because the body already emits `figure*` for that role (`crates/terse-core/src/latex/mod.rs:106`); an earlier draft of this document claimed the opposite as an accepted limitation and was wrong.

**`\setmainfont` by filename, for all four tokens, in addition to the existing package.** The requirement says families are selected "by filename, without host-family discovery", which rules out `\setmainfont{TeX Gyre Heros}` (a fontconfig family lookup) and points at `\setmainfont{texgyreheros-regular.otf}[BoldFont=…, ItalicFont=…, BoldItalicFont=…]`. Applying it uniformly rather than only to the broken sans tokens removes the accidental correctness of `libertinus-otf` and `tgpagella`, whose packages happen to set `\rmdefault` today. The `\RequirePackage` stays, because those packages also carry NFSS and math setup (`libertinus-otf` pulls `unicode-math`); dropping it would silently change math rendering. The token table therefore grows from one package name to a package name plus four filenames per family.

**The source map receives a `FileId` → root-relative path table.** `SourceSpan` carries only a `FileId`, and `ParsedModule` has no table to resolve it, so emitting `paper.map.json` from the artifact planner alone would produce numeric ids and violate the requirement's own "source-map paths SHALL be root-relative". The table already exists one level up, in `InputSnapshot.modules`, keyed by logical path. It is passed into the artifact planner as data, which keeps `terse-core` effect-free.

**Style bytes change for every existing project, deliberately.** Adding `geometry`, `\setmainfont`, a citation style option and new macros rewrites `terse-style.sty`. Style assertions are reviewed as behavior changes; regenerating goldens in bulk would reproduce exactly the blindness this change exists to remove.

**The closure is re-derived, never hand-edited.** `geometry` is not installed in the managed prefix today (`kpsewhich geometry.sty` is empty) and is absent from both profiles. The order is: add to `STYLE_PACKAGES` and the profiles, `terse toolchain update`, re-derive with `scripts/derive-toolchain-closure.sh`, then implement emission. Reversing that order leaves the suite red for the duration of the group.

## Risks / Trade-offs

- [`terse toolchain update` may only refresh revisions and not install packages newly added to the closure] -> verify before relying on it; if it does not grow the prefix, fixing `toolchain/install.rs` is part of this change, not a follow-up, because CI and users both depend on it.
- [Two-column interacts badly with the background watermark and with float placement] -> the acceptance evidence is a rendered PDF comparison, not a style-file assertion.
- [`\setmainfont` filenames differ per TeX Live generation] -> the four families are pinned by the same profile that pins the toolchain, and `doctor` already probes one representative file per family; extend it to the variants rather than trusting the regular weight alone.
- [Emitting `paper.map.json` widens the reproducibility and export surfaces] -> it enters `test_text_artifacts_are_reproducible` and must be excluded from export membership, where an independent expected-set test will catch an omission.

## Migration Plan

Groups run 1, 3, 2, 4, 5. Groups 1 and 3 are independent of the theme work and close five SHALL gaps; group 2 carries the layout decisions and the only cross-group dependency (`\TerseLogo` is placed by the title block). Rollback is per group: each is additive to the style file or the artifact plan, and reverting one restores the previous, verifiably weaker, behavior.

## Open Questions

None blocking. Two recorded observations for later changes: `project()` is never called outside tests, so the theme-invariant projection is verified but never computed in a real build; and wide figures cannot span columns without a body change.
