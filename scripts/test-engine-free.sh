#!/usr/bin/env bash
# The required engine-free lane: unit and integration tests that never
# need XeLaTeX/Biber installed. Runs on every supported OS.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

cargo test --workspace --locked
