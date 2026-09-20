# Terse

Terse compiles a small declarative document language (`.trs`) into
readable, self-contained LaTeX projects. Themes are a separate style layer,
so the same authored content can render as, for example, an academic paper
or a corporate-styled document without touching a single sentence.

See `openspec/changes/terse/` for the full design, capability specs, and
task checklist this implementation follows.

## Prerequisites

- Rust (pinned in `rust-toolchain.toml`; `rustup` will fetch it automatically).
- Optional, for PDF output: XeLaTeX and Biber from TeX Live 2025, either
  provisioned by `terse toolchain install` (Linux/macOS, a private pinned
  installation in your user data directory) or from your own distribution on
  `PATH`, verified with `terse doctor`. `build --tex-only` works without any
  TeX installation. See [`docs/installation.md`](docs/installation.md).

## Install

```sh
cargo install --locked --path crates/terse-cli
terse toolchain install   # optional: managed TeX Live 2025 for PDF output
terse doctor              # check the toolchain, with fix commands per OS
```

## Quickstart

```sh
terse init                 # scaffold a manifest, entry, academic theme, empty lock
terse check                # validate without needing a TeX engine
terse fmt --check          # verify canonical formatting
terse build --tex-only     # generate paper.tex/terse-style.sty/references.bib/COMPILE.txt
terse build --require-pdf  # also compile a PDF (needs xelatex/biber on PATH)
terse watch                # rebuild on save, 150ms trailing debounce
terse refs resolve         # fetch/lock declared DOI/arXiv citations (the only networked command)
terse export --target arxiv --require-compile  # offline, portable arXiv package
```

`build` in `auto` mode (no `--tex-only`/`--require-pdf`) compiles a PDF when
`xelatex` is available and falls back to a source-only build with a warning
otherwise. `COMPILE.txt` in the output directory documents the equivalent
`xelatex`/`biber` sequence to run without Terse.

## Commands

| Command | What it does |
|---|---|
| `init [dir] [--force]` | Scaffold a new project. |
| `check [entry] [--theme] [--strict] [--deny-warnings] [--json] [--target arxiv]` | Read-only, engine-free validation of all reachable content, references, and declared themes. |
| `build [entry] [--theme] [--tex-only \| --require-pdf] [--toolchain SEL] [--json]` | Generate LaTeX sources and, optionally, a PDF. |
| `fmt [paths...] [--check] [--json]` | Lossless, semantic-preserving formatting. |
| `refs resolve [--refresh \| --offline] [--prune]` | Explicit, transactional, all-or-nothing reference resolution; uses the network. |
| `export [entry] [--theme] [--target arxiv] [--include-bbl PATH] [--require-compile] [--toolchain SEL] [--json]` | Build a self-contained, portable export package. |
| `watch [entry] [--theme] [--tex-only] [--toolchain SEL] [--json]` | Rebuild automatically on relevant filesystem changes. |
| `doctor [--json] [--fix] [--toolchain SEL]` | Run every toolchain check with a timeout, report causes and per-OS fixes, apply safe repairs with `--fix`. |
| `toolchain install [--prefix DIR] [--offline --from FILE]` | Provision a private, pinned TeX Live 2025 (Linux/macOS); with `update`, the only other networked command. |
| `toolchain status [--archive FILE] \| update \| uninstall` | Inspect, archive for offline installs, refresh, or remove the managed prefix. |

`SEL` is `auto` (default), `system`, `managed`, or a directory. Only
`refs resolve` and `toolchain install|update` ever use the network.

See `docs/` for a fuller guide to each area.

## Documentation

- [`docs/installation.md`](docs/installation.md)
- [`docs/language.md`](docs/language.md) — the `.trs` language, limits, diagnostics
- [`docs/themes.md`](docs/themes.md)
- [`docs/citations.md`](docs/citations.md)
- [`docs/git-workflow.md`](docs/git-workflow.md) — editing generated output, `watch`
- [`docs/export.md`](docs/export.md)

## Testing

- `scripts/test-engine-free.sh` — the required lane (`cargo test --workspace
  --locked`), no TeX installation needed. Runs on every supported OS.
- `scripts/test-tex.sh` — the heavy lane: `terse doctor` as preflight, a real
  XeLaTeX/Biber smoke build, plus the `--ignored` e2e suite. Needs a working
  toolchain (managed or on `PATH`), or run it inside the pinned environment
  from `tests/toolchain/Dockerfile`, which is built by `terse toolchain
  install` itself and runs the suite with the network disabled.
- `scripts/derive-toolchain-closure.sh` — maintainer tool that recomputes the
  pinned TeX package closure from a real `-recorder` compile of the fixture.
- `scripts/release-checks.sh` — runs both lanes plus the traceability audit,
  the same checks CI runs before a release.

CI and local development run the same scripts; there is no separate hosted-only
validation path. This repository does not itself use git (there is no `.git`
directory here); the scripts and CI config below assume you'll set one up when
publishing, and make no other git-specific assumption.

## Project layout

- `crates/terse-core` — the compiler library. No filesystem, process,
  network, or clock access: it takes an in-memory source snapshot and
  returns diagnostics plus a generated-file plan.
- `crates/terse-cli` — the `terse` executable: project discovery, the
  XeLaTeX/Biber process runner, transactional output publication, reference
  resolution, watch mode, export, and the command-line interface.
- `openspec/changes/terse/` — the design, specs, and task checklist.
- `docs/` — user-facing documentation.

## License

Not yet chosen. See `openspec/changes/terse/tasks.md` task 26.5 — this is
deliberately left open pending the author's decision and does not block any
other work.
