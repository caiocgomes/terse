# Installation

## Prerequisites

- Rust, pinned in `rust-toolchain.toml`. `rustup` fetches it automatically
  the first time you build in this repository.
- Optional, for PDF output: a TeX toolchain providing `xelatex` and `biber`.
  There are two supported ways to get one, described below. `terse check`,
  `terse fmt`, and `terse build --tex-only` never start a TeX process, so the
  compiler is fully usable without any TeX installation.

## Install Terse from source

```sh
cargo install --locked --path crates/terse-cli
terse --version
```

This is the only supported install path. `--locked` builds against the
committed `Cargo.lock`, matching exactly what the test suite validated.

## Get a TeX toolchain

Terse targets TeX Live 2025 with XeLaTeX and Biber, the same generation arXiv
compiles with by default (see
`crates/terse-core/profiles/texlive-2025-xelatex.toml`). Pick one of the two
paths; `terse doctor` tells you whether what you have is usable.

### Path A: managed toolchain (Linux and macOS)

Terse can provision a private, minimal, pinned TeX Live 2025 for you. Nothing
outside its own data directory is touched, and your system TeX (if any) is
left alone.

```sh
terse toolchain install
terse doctor
```

What this does: verifies the prerequisites (`perl`, `tar`, `xz`, and `curl`
or `wget`), downloads the TeX Live installer from the frozen 2025 archive and
checks it against the SHA-512 pinned in
`crates/terse-core/profiles/toolchain-texlive-2025.toml`, installs the
`infraonly` scheme in portable mode, then installs exactly the package
closure the compiler's generated style requires. Expect roughly 150 MB of
download and 360 MB on disk. The prefix is
`~/.local/share/terse/toolchain/texlive-2025` on Linux (or under
`$XDG_DATA_HOME`) and `~/Library/terse/toolchain/texlive-2025` on macOS
(not `Application Support`: XeTeX starts its output driver through `sh`
with its own path, so a prefix containing whitespace produces no PDF, and
the installer refuses one); `--prefix DIR` overrides it. Installation is transactional: a
failed or interrupted run leaves no partial prefix and never modifies a
previous one. A lock file inside the prefix records the year, repository,
installer checksum, and every package revision.

`terse toolchain install` and `terse toolchain update` are the only commands
besides `refs resolve` that use the network. Everything else, including
`build --require-pdf` and `export`, works offline.

Prerequisites per OS:

- Debian/Ubuntu: `sudo apt-get install perl wget xz-utils ca-certificates`
- Fedora: `sudo dnf install perl wget xz`
- macOS: `perl` and `curl` ship with the system; if `xz` is missing,
  `brew install xz`.
- Windows: managed installation is not available yet; use path B.

Other subcommands:

```sh
terse toolchain status                    # prefix, lock, year match with the profile
terse toolchain status --archive tl.tar.gz   # relocatable archive for offline installs
terse toolchain install --offline --from tl.tar.gz   # no network; checksums verified
terse toolchain update                    # reinstall against the pinned repository
terse toolchain uninstall                 # removes only a prefix Terse created
```

### Path B: your own TeX distribution

Install TeX Live 2025 (or MacTeX 2025, or MiKTeX on Windows) so that
`xelatex`, `biber`, and `kpsewhich` are on `PATH`, then run `terse doctor`.
Every check that fails prints its probable cause and the command that fixes
it on your OS. A different TeX Live year usually compiles fine; `doctor`
reports it as a warning, and `export --target arxiv --require-compile` will
report `compiled-local` rather than `compiled-profile` because the toolchain
was not verified against the pinned profile.

## Which toolchain a command uses

`build`, `watch`, `export`, `check --target`, and `doctor` resolve tools in
this order and report which one they picked:

1. `--toolchain auto|system|managed|DIR` on the command line;
2. `[latex] toolchain = "..."` in `terse.toml`;
3. the managed prefix, when it is installed and its lock year matches the
   export profile;
4. `PATH`.

The choice never changes generated text: `paper.tex`, `terse-style.sty`, and
`references.bib` are byte-identical whichever toolchain compiles them. TeX
processes run with a prepared environment (Terse-owned `TEXMFHOME`,
`TEXMFVAR`, `TEXMFCONFIG`; no inherited `TEXINPUTS`/`BIBINPUTS`), so a
misconfigured shell cannot leak into a build.

## Diagnose problems

```sh
terse doctor            # every check, with causes and fix commands
terse doctor --json     # the same as a versioned JSON envelope on stdout
terse doctor --fix      # apply the safe repairs listed below, then re-check
```

`doctor` runs each tool with a timeout instead of only looking for it, then
compiles a five-line document with a citation through XeLaTeX and Biber. The
two repairs `--fix` may apply are removing a stale Biber unpack cache
(`$TMPDIR/par-<hex>`; a Biber that hangs forever after a TeX Live upgrade is
almost always this) and clearing the macOS quarantine attribute from binaries
inside the managed prefix. It never touches a TeX tree or your project.

## Verify

```sh
mkdir /tmp/terse-smoke && cd /tmp/terse-smoke
terse init
terse check
terse build --tex-only
terse build --require-pdf   # needs a toolchain from path A or B
```

## Uninstall

```sh
terse toolchain uninstall   # only if you used path A
cargo uninstall terse
```
