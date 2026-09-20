# Editing and version control

Terse's generated output is designed to be committed and edited
independently of Terse itself.

## What to commit

- Your `.trs` sources, `.theme` files, `terse.toml`, and
  `references.lock`/`references.overrides.toml`.
- Generated output (`build/`) is optional to commit; it is fully
  reproducible from source (`test_text_artifacts_are_reproducible`) except
  for the compiled PDF itself, which embeds tool/timestamp data and is
  explicitly outside the byte-reproducibility contract.

## What not to commit

- `.terse-cache/` — a disposable, content-addressed cache
  (`crates/terse-cli/src/cache.rs`). Deleting it never changes program
  behavior, only rebuild speed. Add it to `.gitignore` (the scaffold from
  `terse init` does this for you, additively — it never overwrites an
  existing `.gitignore`).

## Editing generated LaTeX directly

Generated `.tex`/`.sty`/`.bib` files are ordinary, human-readable LaTeX —
no Terse-specific build step is required to recompile them
(`test_generated_sources_are_human_editable`, `COMPILE.txt` in the output
directory documents the exact `xelatex`/`biber` sequence). This is
intentional: if you need a one-off manual fix, you can edit the generated
source and recompile conventionally. Re-running `terse build` will
overwrite that edit the next time, since generated output is not the
source of truth — the `.trs` files are.

## `watch`

```sh
terse watch
```

Rebuilds on save with a 150ms trailing debounce. A syntax error during
watch never removes the last good PDF; it stays in place until a
subsequent build succeeds (`test_watch_keeps_last_good_pdf_after_syntax_error`).
Ctrl-C stops scheduling, terminates any in-flight XeLaTeX/Biber process
tree, and leaves a consistent published generation.
