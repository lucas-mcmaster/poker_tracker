---
name: milestone-gate
description: Runs the poker_tracker milestone gate (rustfmt check, clippy with warnings denied, full test suite with locked dependencies, plus blueprint drift check for M1–M5) and verifies the passed-test count. Use after creating or changing any files in a milestone, before committing, or when the user asks whether a milestone passes.
---

# Milestone gate

## Inputs

- The current milestone number *M* (ask the user if it is not clear from the conversation).
- Expected passed-test counts:
  - **Milestones 1–5 (v3.5 blueprint):** **M1 = 11, M2 = 39, M3 = 80, M4 = 95, M5 = 104.**
  - **Milestones 6–10 (v3.6 spec):** strictly greater than the previous milestone's passed-test count (starting from M5 = 104).
  - **Milestone 11 (v3.6 docs):** equal to Milestone 10's passed-test count.

## Steps

1. From the repository root, run each applicable command separately and keep its full output:
   1. **Only if *M* ≤ 5:** `cargo run -q --manifest-path tools/blueprint/Cargo.toml -- check M`
      *(For *M* ≥ 6, skip this step because `docs/BLUEPRINT.md` is frozen at v3.5).*
   2. `cargo fmt --check`
   3. `cargo clippy --locked --all-targets -- -D warnings`
   4. `cargo test --locked`
2. Stop at the first command that fails, and do **not** run later steps as a substitute.
3. If all commands succeed, find every line in the `cargo test --locked` output that starts with
   `test result:`. Add up the number that follows `ok.` on each of those lines; that sum is the
   passed-test count. There must be no `FAILED` result anywhere.
4. Compare the count with the expected count for milestone *M*.

## Report (always in this format)

```
Milestone M gate
- drift:   PASS | FAIL | N/A (v3.5 blueprint frozen)
- fmt:     PASS | FAIL
- clippy:  PASS | FAIL
- tests:   PASS | FAIL (<passed> passed, expected <expected>)
Result:    PASS | FAIL
```

On failure, add:

- the first error, verbatim (file, line, message);
- the most likely cause.

## Rules

- Never "fix" a failure by editing existing tests (unless explicitly instructed in `docs/SPEC_v3.6.md`), fixtures, `Cargo.toml` lints, `#[allow]` attributes, `Cargo.lock`, or expected counts. Never add `--ignore-rust-version`.
- For **Milestones 1–5**, a drift failure (`MISSING`, `DIFFERS`, `EARLY`) is fixed by re-running `cargo run -q --manifest-path tools/blueprint/Cargo.toml -- extract M`.
- For **Milestones 6–11**, **never** run `tools/blueprint extract` (doing so will overwrite v3.6 changes with v3.5 files). If `cargo fmt --check` fails in M6–M11, run `cargo fmt` on the modified Rust files and re-verify.
- A `real_session` or `real_session_v3_6` SHA-256 failure means a fixture file is not byte-for-byte the original export (line endings or trailing spaces); restore the original file.