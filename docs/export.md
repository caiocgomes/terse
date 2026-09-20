# Export

```sh
terse check --target arxiv           # validate current sources for export, read-only
terse export --target arxiv          # build the offline export package
terse export --target arxiv --require-compile   # also compile the extracted package
```

Only the `arxiv` target exists today, validated against the versioned,
offline `texlive-2025-xelatex` profile
(`crates/terse-core/profiles/texlive-2025-xelatex.toml`). The profile is a
static, documented list of guaranteed engine/package/font/bibliography
assumptions — it never fetches a current package list.

## What export never does

- Never contacts a network transport, tool downloader, or upload endpoint,
  under any flag.
- Never trusts a stale cached build: every export target validation
  rebuilds a fresh plan from current sources, theme, and locks.
- Never silently drops content to make an export "pass": raw `tex:`
  blocks and unsupported dependencies fail loudly (`E-EXPORT-004`) rather
  than being stripped.

## What's in the archive

Only generated source/style/bibliography, a `.bib` (not a pre-built
`.bbl`, unless you pass `--include-bbl` with a filename stem that matches
your main `.tex` file exactly), used assets, and a `MANIFEST.json` listing
every other file's size and SHA-256 hash. Diagnostic reports, source maps,
auxiliary engine files, your original `.trs` sources, and the rendered
paper PDF are excluded. The ZIP is byte-deterministic across machines and
mtimes (fixed epoch, fixed permission bits, sorted member order).

## Honest compilation reporting

`--require-compile` extracts the completed ZIP into an isolated scratch
directory (empty `TEXMFHOME`, no project/Terse/network dependencies) and
compiles it there. The report is always one of:

- `static-only` — source structure and package list validated, not
  compiled.
- `compiled-local` — actually compiled successfully with whatever
  XeLaTeX/Biber happen to be installed on this machine.
- `compiled-profile` — compiled *and* every package/font/engine actually
  used was verified against the exact TeX Live year the profile
  documents.

No report ever claims or implies acceptance by arXiv or any other venue —
only what was locally, verifiably true about the compile. The compilation
report is a separate file next to the archive, never bundled inside it (so
it never perturbs the archive's determinism).

Export publishes the extracted directory, the ZIP, and the report as one
managed generation: a failure at any stage leaves the previous export
output completely untouched.
