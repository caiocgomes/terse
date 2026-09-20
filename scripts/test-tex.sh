#!/usr/bin/env bash
# The heavy lane: needs a real XeLaTeX/Biber toolchain, either the managed
# one from `terse toolchain install` or one on PATH. `terse doctor` is the
# preflight: any failing check aborts the lane with its cause and fix, so a
# hung Biber or a missing font is never reported as a test failure. Inside
# tests/toolchain/Dockerfile this runs with the network disabled.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

cargo build --locked -p terse-cli --bin terse
# Honor CARGO_TARGET_DIR: inside the pinned container the host's target/
# holds foreign-platform binaries.
BIN="${CARGO_TARGET_DIR:-$(pwd)/target}/debug/terse"

if ! "$BIN" doctor; then
  echo "error: terse doctor reported failing checks; fix them (or run 'terse toolchain install') before the heavy lane" >&2
  exit 1
fi

WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

(
  cd "$WORKDIR"
  "$BIN" init
  "$BIN" check
  "$BIN" fmt --check
  "$BIN" build --tex-only
  "$BIN" build --require-pdf
  test -f build/academic/paper.pdf
)

echo "test-tex.sh: real XeLaTeX smoke build succeeded"

cargo test --locked -p terse-cli --test e2e -- --ignored
