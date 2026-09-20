# Contributing

## Workflow

This implementation follows the OpenSpec changes in `openspec/changes/`
(`design.md`, `specs/*/spec.md`, `tests.md`, `tasks.md` in each): `terse` for
the compiler and `managed-toolchain` for toolchain resolution, `doctor`, and
managed TeX Live provisioning. The synced main specs live in
`openspec/specs/`. Read the relevant spec and the test plan before changing
behavior in that area; each `tasks.md`'s checkboxes track implementation
status per group.

## Building and testing

```sh
cargo build --workspace --locked
scripts/test-engine-free.sh   # required: no TeX installation needed
scripts/test-tex.sh           # heavy: needs a toolchain; `terse doctor` runs first
```

For the heavy lane, either `terse toolchain install` (Linux/macOS) or a TeX
Live 2025 on `PATH` works; `terse doctor` tells you which checks fail and how
to fix them. `tests/toolchain/Dockerfile` is the pinned environment CI uses;
it is provisioned by `terse toolchain install` and then run with the network
disabled, so a test that needs the network fails there by design. Only one
test may use the network, `test_managed_toolchain_provisions_ci_image`, and
only in the CI provisioning job.

Run the specific tests relevant to your change with
`cargo test --locked <test_name>` before running the full suite.

## Code organization

- `crates/terse-core` must stay effect-free: no filesystem, process,
  network, or clock access. If your change needs any of those, it belongs
  in `crates/terse-cli`.
- Diagnostic codes follow the `E-<AREA>-<NNN>` (error) / `W-<AREA>-<NNN>`
  (warning) convention already used throughout `crates/terse-core/src/
  diagnostic/` and callers — pick a new, unused number in the relevant
  area rather than reusing one.
- Tests are TDD: write the failing test first (see `tests.md` for the
  setup/assertion/edge-case contract expected of each named test), then
  implement.

## Determinism

Any place a generated artifact's content is derived from unordered data
(`HashMap`/`HashSet` iteration, filesystem enumeration order) must be
sorted before serialization. This has been a real, recurring bug class in
this codebase — audit new code that touches artifact generation for it.

## Pull requests

Include the specific `cargo test --locked <name>` output for the tests
your change adds or affects, not just a full-suite pass/fail line.
