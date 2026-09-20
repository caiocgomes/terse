# Toolchain fixtures

`fake-prefix/` is a hand-authored stand-in for a managed TeX Live prefix:
three sentinel executables (`#!/bin/sh` scripts that exit 0, never real
TeX tools) under `bin/fake-platform/`, a valid `terse-toolchain.lock.json`
for TeX Live 2025 with placeholder revisions and a zero checksum, and the
Terse ownership marker. Tests copy it into a temporary directory (see
`crates/terse-cli/tests/common/mod.rs::install_fake_prefix`) and set the
executable bit explicitly, since file modes are not reliable across
checkouts. Nothing here was captured from a real installation.

`tlpdb-excerpt.txt` is four records copied verbatim (long descriptions,
catalogue lines, and container checksums removed) from the frozen TeX Live
2025 repository catalog, `tlpkg/texlive.tlpdb` at
`https://ftp.math.utah.edu/pub/tex/historic/systems/texlive/2025/tlnet-final/`,
fetched 2026-09-20: `booktabs` (relocated package: `RELOC/` paths, `docfiles`
and `srcfiles` sections that ownership mapping must skip), `hyphen-portuguese`
(TLCore with `depend`/`execute` lines), `biber` (a package with no
`runfiles`), and `biber.universal-darwin` (per-architecture `binfiles`).
texlive.tlpdb is public domain metadata.

`install-tl.profile` is the golden `install-tl` profile Terse generates for
a managed installation, with `/staging` and `/data` placeholder paths:
`scheme-infraonly`, portable mode, no path adjustment, no documentation or
source files, no backups, no desktop integration. Hand-authored from the
profile that completed a real install on 2026-09-20.
