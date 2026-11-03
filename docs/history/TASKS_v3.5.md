> **Archived.** The v3.5 milestone plan (M0–M5), moved from `.agents/rules/TASKS.md` in v3.7.
> It is history: never run its gate (`tools/blueprint check`) again. Current milestones are in
> `docs/SPEC_v3.7.md`. Section numbers below (e.g. `ARCHITECTURE.md` §10) refer to the v3.5
> versions of those files in git history.

# `poker_tracker` Milestone Plan (v3.5)

Work strictly in order. A milestone is **done** only when its gate passes with the exact expected
test count and the work is committed. Never start the next milestone in the same step as
finishing the previous one; report and wait for the user.

## Gate (every milestone)

```
cargo run -q --manifest-path tools/blueprint/Cargo.toml -- check <M>
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

Pass = all four succeed **and** the sum of `passed` across all `test result:` lines equals
the expected count. The `milestone-gate` skill runs and checks this.

## File-variant rule

Blueprint headers look like `## FILE: <path> · Milestone <N>`.

- At milestone *M*, a file belongs to the milestone if its header's *N* ≤ *M*.
- When a path appears more than once (`src/lib.rs` at M1/M2/M4/M5, `src/parser/mod.rs` at
  M2/M3), use the block with the highest *N* ≤ *M*, replacing the previous content.
- `tools/blueprint … extract <M>` applies this rule and writes the files exactly; `check <M>`
  reports any file that is missing, differs, or belongs to a later milestone.

---

## Milestone 0 — Repository setup (user + agent)

**User does:**

1. Create the repository folder and open it in VS Code. Run `git init`.
2. Unpack the v3.5 setup zip at the repository root. It contains `AGENTS.md`, `.agents/`,
   `docs/BLUEPRINT.md`, `docs/CHANGELOG.md`, `tools/blueprint/`, `Cargo.lock`, `.editorconfig`
   and `.gitattributes`.
3. Copy the GGPoker export `GG20260821-2055 - NLHRed15 - 0.05 - 0.1 - 6max.txt` byte-for-byte to
   `tests/fixtures/ggpoker/NLHRed15_2026-08-21.txt`.

**Agent does:**

1. Verify the items above exist.
2. Run `cargo --version` and report the toolchain; it must be ≥ 1.88.
3. Run `cargo run -q --manifest-path tools/blueprint/Cargo.toml -- list 5` and report that it
   lists 23 files.
4. Create `.gitignore` containing `target/` and `*.sqlite*`.
5. Make the first commit: `milestone 0: repository scaffolding`.

**Done when:** the fixture file is present, the tool runs, and the toolchain is reported. No
crate code yet.

---

## Milestone 1 — Domain primitives

**Files (extract 1):**

- `Cargo.toml`
- `src/lib.rs` (M1 variant: `pub mod domain;`)
- `src/domain/mod.rs`, `src/domain/money.rs`, `src/domain/card.rs`, `src/domain/canonical.rs`

**What it establishes:**

- `CurrencyCode`;
- lossless micro-units (`1.00 = 1,000,000`);
- the 1-byte `Card`;
- the site-neutral canonical model (deductions, adjustments, `PlayerResult`) and `FromStr`
  for every stored identifier.

**Expected tests: 11** — card ×4, money ×3, canonical ×4.

**Spot checks:**

- `decimal_to_micro_units(dec!(18.6)) == Some(18_600_000)`;
- `"cad".parse::<CurrencyCode>()` fails;
- all 52 cards round-trip; every `as_str()` parses back.

**Commit:** `milestone 1: domain primitives (gate: 11 tests passing)`

---

## Milestone 2 — Parser contract, validator, registry

**Files (extract 2):**

- `src/lib.rs` (M2 variant: adds `pub mod parser;`)
- `src/parser/mod.rs` (M2 variant: `base` + `registry` only — **no** `ggpoker`)
- `src/parser/base.rs`, `src/parser/registry.rs`

**What it establishes:**

- `ParseError`;
- the object-safe `HandParser`;
- `split_hand_blocks` with `start_line`;
- the full `validate_hand`: the betting replay (turn order, round completion including the
  lone-player rule, legality and minimum sizes, incomplete all-in raises, blind sizes,
  uncalled-bet size, street/board consistency), the all-in flag, and side-pot eligibility;
- `ParserRegistry` with unrecognized/ambiguous detection.

**Expected tests: 39** — the 11 above, plus base ×26 and registry ×2.

**Spot checks:** validator unit tests accept a short all-in call that closes the betting for
the big blind, and reject:

- actions that don't sum to the pot, two heroes, a folded winner, duplicate cards, a wrong
  adjustment sign;
- out-of-turn actions, a betting round that closes early, actions after going all-in;
- a raise with no bet or no opponent able to respond, undersized bets and raises, a re-raise
  after an incomplete all-in;
- blinds that differ from the stakes, a whole-stack commitment not flagged all-in;
- a payout beyond side-pot eligibility.

**Commit:** `milestone 2: parser contract, validator and registry (gate: 39 tests passing)`

---

## Milestone 3 — GGPoker parser on real hands

**Files (extract 3):**

- `src/parser/mod.rs` (M3 variant: adds `ggpoker`)
- `src/parser/ggpoker.rs`
- `tests/common/mod.rs`, `tests/ggpoker_parser.rs`, `tests/real_session.rs`
- Requires the fixture from Milestone 0.

**What it establishes:** every observed GGPoker format:

- multi-run showdowns, dead blinds, EV Cashout, one-card shows;
- one pattern per verb, and nothing but seats and blinds before the deal;
- observed order: ascending seats, small blind first, one `Dealt to` line per seat;
- summary cross-checks with the strict `showed` grammar, position labels, fold streets and shown
  cards;
- currency from configuration (CAD).

**Expected tests: 80** — 39 + ggpoker_parser ×36 + real_session ×5.

**Spot checks** (asserted by tests):

- all 294 real hands parse, and the fixture's SHA-256 matches;
- Hero net **+2.86 CAD**, top-ups 5.99, final stack 18.85;
- session deductions 17.35 and adjustments −0.48.

**If `real_session` fails:** a compile error means the fixture path or name is wrong; a SHA-256
mismatch means the copy is not byte-for-byte (line endings, trailing spaces). Fix the file,
never the test.

**Commit:** `milestone 3: GGPoker parser on real hands (gate: 80 tests passing)`

---

## Milestone 4 — SQLite storage

**Files (extract 4):**

- `src/lib.rs` (M4 variant: adds `pub mod storage;`)
- `src/storage/mod.rs`, `src/storage/schema.rs`, `src/storage/repository.rs`
- `tests/storage.rs`

**What it establishes:**

- STRICT tables through append-only `MIGRATIONS` (schema version 2) and `application_id`;
  foreign files refused untouched; v3.4 databases upgraded in place;
- per-hand currency, `pot_deductions`, `player_adjustments`, raw text and its SHA-256;
- duplicates vs `CurrencyConflict` / `ContentConflict`; `load_hand`;
- `hand_sightings`, `delete_import_run` that keeps shared hands, `quarantined_hands`;
- per-hand savepoints, `BEGIN IMMEDIATE`, busy timeout, fatal lock errors.

**Expected tests: 95** — 80 + storage ×15.

**Spot checks:**

- 294 inserted, then 294 duplicates; every hand reads back identical;
- Hero net by currency is `[("CAD", 2860000)]` micro-units;
- Σ net = Σ adjustments − Σ deductions;
- re-importing USD-stored hands as CAD fails with `CurrencyConflict`.

**Commit:** `milestone 4: SQLite storage (gate: 95 tests passing)`

---

## Milestone 5 — Pipeline and CLI

**Files (extract 5):**

- `src/lib.rs` (M5 variant: adds `pub mod pipeline;`)
- `src/pipeline/mod.rs`, `src/pipeline/continuity.rs`, `src/pipeline/ingester.rs`
- `src/main.rs`
- `tests/pipeline.rs`, `tests/cli.rs`

**What it establishes:**

- per-file parser detection; unrecognized, ambiguous and empty inputs reported;
- `HandFailure` with parser, hand id and source line; partial report on fatal error;
- quarantine of every rejected hand; stack-continuity warnings;
- absolute source paths, symlinks followed, the run's real start time;
- CLI with `--ggpoker-currency`, `--delete-run`, `--help`, and exit codes 0/1/2/3.

**Expected tests: 104** — 95 + pipeline ×7 + cli ×2.

**Acceptance run** (after the gate):

```
cargo run --release --locked -- tests/fixtures/ggpoker --ggpoker-currency CAD --db target/pt_check.sqlite
```

Expect `Files Processed: 1`, `GGPoker (CAD): 1 file(s)`, `Hands Ingested: 294`,
`Hands Failed: 0`, `Stack Warnings: 0`, exit code 0. A second run should report
`Hands Ingested: 0` and `Duplicates Skipped: 294`. Then:

```
cargo run --release --locked -- --delete-run <Batch Run ID of the first run> --db target/pt_check.sqlite
cargo run --release --locked -- --delete-run <Batch Run ID of the second run> --db target/pt_check.sqlite
```

Expect `Deleted 0 hand(s) … kept 294 hand(s) that another import also contained`, then
`Deleted 294 hand(s)`. Delete `target/pt_check.sqlite*` afterwards.

**Commit:** `milestone 5: ingestion pipeline and CLI (gate: 104 tests passing)`

---

## After Milestone 5 — Backlog (not specified; plan first)

Nothing below is designed yet. For any item:

1. Propose an implementation plan (files, public API, schema changes, tests).
2. Wait for the user's approval.
3. Implement against the same gate.

Every existing test must keep passing, and new code follows `ARCHITECTURE.md` §10.

| Candidate | Notes |
| :--- | :--- |
| Hero results queries | Sessions, net and bb/100 **per currency** (micro-units ÷ 1e6); rake paid from `pot_deductions` |
| Position derivation | From seats, button and posting order; store or compute as a view |
| Re-parse from the database | Re-run parsers over `quarantine_pending` and stored `raw_text` after a parser update |
| Import tracking | Sightings exist per hand; add a file table (size, hash) to skip unchanged files; watch folder |
| More GGPoker formats | Only with real sample hands: straddle, bomb pot, EV Cashout receiving side, all-in blinds, heads-up labels |
| Second platform | Real exports first; follow the §10 checklist |
| Currency conversion | Reporting layer only, with explicit rates and dates; storage stays per currency |
