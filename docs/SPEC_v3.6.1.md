# poker_tracker — v3.6.1 spec (M12: audit fixes)

Nov 2, 2026 · @Lucas

## 0. Agent brief

M12 applies the fixes from the v3.6 audit of commit `1af1f57`. The code is already written and
verified as `v3_6_audit_fixes.patch`. Your job is to apply it unchanged, add the two doc entries
below, run the gate, and commit. Do not redesign, reformat or extend any of it.

**Authorization (overrides `AGENTS.md` §4 for this milestone only).** The owner approves these
edits, which `AGENTS.md` would otherwise forbid:

- the patch changes one existing test: case 6 of `straddle_refusal_tests` in
  `tests/ggpoker_parser.rs` (the old edit was refused by a later line, not the rule it tests);
- the patch changes `docs/invariants.md`, `docs/known_limitations.md` and
  `src/storage/repository.rs`;
- you add entries to `docs/CHANGELOG.md` and `docs/SPEC_v3.6.md` (§3 below).

Everything else in `AGENTS.md` still applies. Do not touch `Cargo.toml`, `Cargo.lock`, fixtures,
`AGENTS.md`, `.agents/` or `tools/`. Do not bump the crate version (it would change `Cargo.lock`).

**Stop and report, without improvising, if:** `HEAD` is not `1af1f57` or the tree is dirty;
`git apply --check` fails; any gate fails; the test count is not exactly **123**.

## 1. What the patch changes

| Finding | Change | Files |
| --- | --- | --- |
| F1 | Database identity read in one read transaction; only a busy lock is retried (replaces the 5 s retry of `NotAPokerTrackerDatabase` added in M6) | `src/storage/repository.rs`, `tests/storage.rs` |
| F2 | GGPoker parser refuses six never-observed shapes: heads-up missed blind, all-in straddle, blind for less than the stake, a second straddle by the same player, heads-up straddle, heads-up button not posting the small blind | `src/parser/ggpoker.rs`, `src/parser/base.rs` |
| F3 | An opponent's uncontested summary show must include every card shown in the body | `src/parser/ggpoker.rs` |
| F4 | `won` requires a flop on every `showed` line, with or without a hand description | `src/parser/ggpoker.rs` |
| F5 | Min-raise-over-straddle test rebuilt as a consistent edit (0.30 refused, 0.40 accepted) | `tests/ggpoker_parser.rs` |
| F8, F10 | Direct slice indexing replaced; `validate_betting` rustdoc describes straddles | `src/parser/base.rs`, `src/parser/ggpoker.rs` |
| F11 | Docs corrected to match the code | `docs/invariants.md`, `docs/known_limitations.md` |

New tests: `v3_6_audit_refusals` (9 refusals, 1 acceptance) and
`a_foreign_database_is_refused_without_waiting_for_the_busy_timeout`. Test count: 121 → 123.

## 2. Tasks

1. `git status` is clean and `git log -1 --format=%h` prints `1af1f57`.
2. The owner places the patch at the repository root as `v3_6_audit_fixes.patch`.
   Run `git apply --check v3_6_audit_fixes.patch`, then `git apply --index v3_6_audit_fixes.patch`.
   Never hand-merge a failed hunk.
3. Delete `v3_6_audit_fixes.patch` from the working tree. It is not committed.
4. Add the `docs/CHANGELOG.md` and `docs/SPEC_v3.6.md` text in §3, verbatim.
5. Run the gate (`milestone-gate` skill, M12, expected **123**).
6. Scratch import:
   `cargo run --locked -q -- tests/fixtures/ggpoker/v3_6/ --ggpoker-currency CAD --db /tmp/m12_scratch.sqlite`.
   Expect exit code `0`, 989 ingested, 0 failed, 0 quarantined, 0 stack warnings. Then delete
   `/tmp/m12_scratch.sqlite*`.
7. Commit `docs/SPEC_v3.6.1.md` (this file) together with the change:
   `milestone 12: v3.6 audit fixes (gate: 123 tests passing)`.
8. Report the gate block, the scratch-import numbers and `git show --stat HEAD`.

## 3. Doc text to add (verbatim)

**`docs/CHANGELOG.md`**, a new first row of the version table:

```
| v3.6.1 | 0.5.0 | 123 | Audit fixes: refuse six never-observed hand shapes, summary-card and `won`/`collected` checks completed, database identity read as one snapshot (foreign files refused at once), docs synced with code. |
```

and a new section above `## v3.5 → v3.6`:

```
## v3.6 → v3.6.1 (audit fixes)

From the audit of `1af1f57`; see `docs/SPEC_v3.6.1.md`.

- **Never-observed shapes now refused:** heads-up missed blind, all-in straddle, blind for less than the stake, a second straddle by the same player, heads-up straddle, heads-up button not posting the small blind.
- **Summary checks:** an opponent's uncontested summary show includes every body-shown card; `won` needs a flop on every `showed` line.
- **Storage:** database identity is read in one read transaction and only a busy lock is retried; the M6 change that retried `NotAPokerTrackerDatabase` for 5 s is removed.
- **Tests:** the min-raise-over-straddle refusal is a consistent edit; new refusal tests for the shapes above and an EV Cashout in a multi-run hand.

Final test count: 123 tests passing (zero failures).
```

**`docs/SPEC_v3.6.md`**, a new section at the end:

```
## 11. Errata (v3.6.1)

Found by the audit of `1af1f57`; implemented in M12 (`docs/SPEC_v3.6.1.md`).

1. **M7.2:** `RE_SEAT_SHOWED_UNCONTESTED` is tried *before* `RE_SEAT_SHOWED`. In the original order, `showed [c1 c2] and won ($x)` matches the general shape first and cannot import.
2. **M8.3:** a straddle that uses the poster's whole stack is refused by the GGPoker parser (`IllegalAction`), as §9 requires. The validator still allows it site-neutrally.
3. **§9 also refuses:** a second straddle by the same player, a heads-up straddle, and a heads-up hand where the button does not post the small blind (owner decision, 2026-11-02).
4. **M8.6:** refusal 6 is tested with a consistent edit (the straddler calls; pot and payout +$0.20), so only the minimum-raise rule can refuse it.
5. **Storage:** `SqliteRepository::initialize` reads the database identity in one read transaction and retries only a busy lock.
```
