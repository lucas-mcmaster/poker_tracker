---
name: milestone-gate
description: Runs the poker_tracker milestone gate (blueprint drift check, rustfmt check, clippy with warnings denied, full test suite with the locked dependencies) and checks the passed-test count against the expected count for the current milestone. Use after creating or changing any files in a milestone, before committing, or when the user asks whether a milestone passes.
---

# Milestone gate

## Inputs

- The current milestone number *M* (ask the user if it is not clear from the conversation).
- Expected passed-test counts: **M1 = 11, M2 = 39, M3 = 80, M4 = 95, M5 = 104.**

## Steps

1. From the repository root, run each command separately and keep its full output:
   1. `cargo run -q --manifest-path tools/blueprint/Cargo.toml -- check M`
   2. `cargo fmt --check`
   3. `cargo clippy --locked --all-targets -- -D warnings`
   4. `cargo test --locked`
2. Stop at the first command that fails, and do **not** run later steps as a substitute.
3. If all four succeed, find every line in the `cargo test` output that starts with
   `test result:`. Add up the number that follows `ok.` on each of those lines; that sum is the
   passed-test count. There must be no `FAILED` result anywhere.
4. Compare the count with the expected count for milestone *M*.

## Report (always in this format)

```
Milestone M gate
- drift:   PASS | FAIL
- fmt:     PASS | FAIL
- clippy:  PASS | FAIL
- tests:   PASS | FAIL (<passed> passed, expected <expected>)
Result:    PASS | FAIL
```

On failure, add:

- the first error, verbatim (file, line, message);
- the most likely cause.

## Rules

- Never "fix" a failure by editing tests, fixtures, `Cargo.toml` lints, `#[allow]` attributes,
  `Cargo.lock`, or expected counts. Never add `--ignore-rust-version`.
- A drift failure (`MISSING`, `DIFFERS`, `EARLY`) is fixed by re-running
  `cargo run -q --manifest-path tools/blueprint/Cargo.toml -- extract M` and deleting files
  reported as `EARLY`; never by editing the blueprint.
- `cargo fmt --check` failures on blueprint files mean the copy differs from the blueprint.
  Re-extract; do not run `cargo fmt` to paper over it.
- A count lower than expected usually means a test file or a milestone file variant is missing.
  A higher count means files from a later milestone were added early.
- A `real_session` SHA-256 failure means the fixture is not byte-for-byte the export (line
  endings or trailing spaces); restore the original file.
