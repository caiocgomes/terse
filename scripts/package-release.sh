#!/usr/bin/env bash
# Build a standalone `terse` executable with locked dependencies for the
# current host target. The documented release matrix (built and smoke-
# tested by CI on real runners, not this script alone) is:
#   x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu,
#   x86_64-apple-darwin, aarch64-apple-darwin,
#   x86_64-pc-windows-msvc
# This script only builds and smoke-tests the HOST's own target; it does
# not cross-compile. Cross-compilation for the other matrix entries is
# left to CI's per-OS runners (.github/workflows/release.yml), since this
# sandbox cannot exercise Windows/other-arch binaries directly.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

TARGET="$(rustc -vV | sed -n 's/^host: //p')"
echo "packaging for host target: ${TARGET}"

cargo build --workspace --locked --release

BIN="target/release/terse"
if [ ! -x "${BIN}" ]; then
  echo "error: expected binary at ${BIN} was not produced" >&2
  exit 1
fi

OUT="dist"
mkdir -p "${OUT}"
ARCHIVE="${OUT}/terse-${TARGET}.tar.gz"
tar -C target/release -czf "${ARCHIVE}" terse
echo "packaged: ${ARCHIVE}"

# Smoke-test the packaged binary standalone (not the just-built target/
# path binary) to prove the archive itself is usable.
SMOKE="$(mktemp -d)"
trap 'rm -rf "${SMOKE}"' EXIT
tar -C "${SMOKE}" -xzf "${ARCHIVE}"
(
  cd "${SMOKE}"
  mkdir project && cd project
  "../terse" init
  "../terse" check
  "../terse" build --tex-only
)
echo "package-release.sh: host-target package (${TARGET}) smoke-tested successfully"
echo "note: this script verified only ${TARGET} locally; other matrix entries are CI-only"
