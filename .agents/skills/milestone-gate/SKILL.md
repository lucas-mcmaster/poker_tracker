---
name: milestone-gate
description: Runs the poker_tracker milestone gate (rustfmt check, clippy with warnings denied, full test suite with locked dependencies) and verifies the passed-test count. Use after creating or changing any files in a milestone, before committing, or when the user asks whether a milestone passes.
---

# Milestone gate

## Inputs

- The current milestone number *M* (ask the user if it is not clear from the conversation).
- The expected passed-test count:
  - if the current spec states an exact count for *M*, that count;
  - otherwise, for a **code** milestone, strictly more than the previous milestone's count; for
    a **docs-only** milestone, exactly the previous count. The previous count is in the newest
    `milestone <N>: … (gate: <count> tests passing)` commit message.

## Steps

1. From the repository root, run each command separately and keep its full output:
   1. `cargo fmt --check`
   2. `cargo clippy --locked --all-targets -- -D warnings`
   3. `cargo test --locked`
2. Stop at the first command that fails, and do **not** run later steps as a substitute.
3. If all commands succeed, find every line in the `cargo test --locked` output that starts with
   `test result:`. Add up the number that follows `ok.` on each of those lines; that sum is the
   passed-test count. There must be no `FAILED` result anywhere.
4. Compare the count with the expected count for milestone *M*.

## Report (always in this format)

```
Milestone M gate
- fmt:     PASS | FAIL
- clippy:  PASS | FAIL
- tests:   PASS | FAIL (<passed> passed, expected <expected or "> previous">)
Result:    PASS | FAIL
```

On failure, add:

- the first error, verbatim (file, line, message);
- the most likely cause.

## Rules

- Never "fix" a failure by editing an existing test (unless the current spec explicitly
  authorizes that test), fixtures, `Cargo.toml` lints, `#[allow]` attributes, `Cargo.lock`,
  expected counts or the spec's expected numbers. Never add `--ignore-rust-version`.
- Never run `tools/blueprint extract` or `check`: `docs/BLUEPRINT.md` is frozen at v3.5 and
  extraction would overwrite later work.
- If `cargo fmt --check` fails, run `cargo fmt` on the modified Rust files and re-run the gate.
- A `real_session` or `real_session_v3_6` SHA-256 failure means a fixture file is not
  byte-for-byte the original export (line endings or trailing spaces); restore the original file.
- A whole-folder test that reports quarantined hands or stack warnings is a failure, even if the
  test count is right.
