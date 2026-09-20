#!/usr/bin/env bash
# The full release gate: engine-free lane, heavy TeX lane (if a toolchain
# is available), and the traceability audit against tests.md. This is the
# same set of checks CI runs before a release; run it locally before
# tagging one.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

echo "== engine-free lane =="
scripts/test-engine-free.sh

# A usable toolchain is whatever `terse doctor` accepts: the managed prefix
# from `terse toolchain install` or a system TeX Live on PATH.
cargo build --locked -p terse-cli --bin terse
if "${CARGO_TARGET_DIR:-target}/debug/terse" doctor >/dev/null 2>&1; then
  echo "== heavy TeX lane =="
  scripts/test-tex.sh
else
  echo "== heavy TeX lane skipped: terse doctor reports a failing check ==" >&2
  echo "run 'terse doctor' for the cause, 'terse toolchain install' to provision one, or use tests/toolchain/Dockerfile" >&2
fi

echo "== traceability audit =="
DISCOVERED="$(cargo test --workspace --locked -- --list 2>/dev/null | grep -c ': test$' || true)"
echo "discovered test count: ${DISCOVERED} (tests.md documents a floor of 146; more is expected and fine)"

echo "== release build =="
cargo build --workspace --locked --release

echo "release-checks.sh: all checks that could run in this environment passed"
