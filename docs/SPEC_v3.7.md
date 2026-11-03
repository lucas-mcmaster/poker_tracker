# poker_tracker — v3.7 implementation spec (derived statistics)

Nov 2, 2026 · @Lucas

## 0. Agent brief

This spec moves `poker_tracker` from v3.6.1 (M12, 123 tests) to v3.7: the derived-statistics
layer that `docs/known_limitations.md` has deferred so far. It is modelled on PokerTracker 4's
post-session analysis (reports, stats, all-in equity) with PokerTracker 3's statistic
definitions. The work is milestones **M13–M16**. §8 sketches M17 and M19 for planning only; do
not build them.

| Milestone | Delivers |
| --- | --- |
| M13 | The derived cache (`hand_player_stats`, rebuilt automatically), positions, basic facts (VPIP, PFR, streets seen, showdown, action counts) |
| M14 | Opportunity counters: RFI, 3-bet, 4-bet, cold call, steal and steal defence, c-bets, check-raise |
| M15 | Money per player-hand (rake and fees attributed, blinds), sessions, and `--report` CSV output |
| M16 | Hand evaluator and exact all-in equity; equity-adjusted results |

**How to work through it:**

1. Read `AGENTS.md`, `docs/invariants.md`, then **`docs/stats.md` in full** (the authoritative
   definition of every counter, position and money figure), then this spec.
2. Do the milestones in order. Each lists what to build, the contract you must meet, the tests to
   add, and a **Done when** checklist. Commit and report after each one, then wait.
3. Every product decision is made (§10). If the code, `docs/stats.md` and this spec disagree,
   stop and report the difference rather than choosing.

**Expected result:** after M16, `poker_tracker --report results --db <db>` on the 10 v3.6 exports
prints Hero's results, with all-in-adjusted figures, that match §7.5 exactly. Every existing test
still passes.

## 1. Ground rules and gates

**Sources of truth** are ordered in `AGENTS.md` §1: this spec for tasks, contracts and expected
numbers; `docs/stats.md` for every definition (on definitions it outranks this spec); then
`docs/invariants.md`, `docs/known_limitations.md` and the code. `docs/SPEC_v3.6*.md` and
`docs/BLUEPRINT.md` are history.

**The contract is fixed; the internals are yours.** This spec fixes public names, stored
identifiers, the schema, CLI flags, CSV columns and formats, and the expected numbers. Module
layout, private helpers, internal types and SQL are your design, within `AGENTS.md`. Where the
spec says "e.g.", it is a suggestion.

**Deviations.** If you must do something this spec does not name (a refactor, a fix to an
unrelated bug, a different file layout), put it in its own commit and list it under
**Deviations** in your milestone report. Never change behaviour silently.

**Docs move with the code.** Each milestone updates, in the same commit:

- `docs/invariants.md` for every rule it adds (M13 adds a "Derived data" section);
- `docs/known_limitations.md` for scope it closes or opens;
- a line in a new `## v3.6.1 → v3.7` section of `docs/CHANGELOG.md` (M16 also adds the v3.7
  row to the version table with the final test count).

`docs/stats.md` changes only if a definition is found to be wrong or ambiguous; then stop and
report first.

**Gates for every milestone** (all must pass):

```
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

The passed-test count goes up at every milestone (v3.6.1 ends at **123**). Record it in the
commit message: `milestone <N>: <summary> (gate: <count> tests passing)`.

**Dependencies.** None are needed. Do not add any (the hand evaluator is written in-crate).

**Fixtures.** No new fixtures are needed; all hands used below are already in
`tests/common/mod.rs` and `tests/fixtures/ggpoker/`. Fixtures stay read-only.

## 2. Architecture of the stats layer

```
CanonicalHand ──► stats::derive_hand_stats ──► Vec<PlayerHandStats>   (site-neutral, pure)
                                                     │
SqliteRepository: insert (same savepoint) ───────────┤
                  open / rebuild (one transaction) ──┴──► hand_player_stats   (cache)
                                                     │
report::write_report (integer SUMs → Decimal → CSV) ◄┘
```

| Module | Responsibility | May know about sites? |
| --- | --- | --- |
| `src/stats/` (new) | `STATS_VERSION`, positions, counters, money attribution, hand evaluator, all-in equity. Pure functions of a `CanonicalHand`; depends only on `domain` | No |
| `src/storage/` | Migration 3, the `hand_player_stats` table, writing it on insert, rebuilding it | No |
| `src/report/` (new, M15) | Report rows and CSV formatting; reads only integer aggregates | No |
| `src/main.rs` | `--rebuild-stats` (M13) and `--report` (M15) | Registration only |

**Contract of `src/stats/`:**

- `pub const STATS_VERSION: i64` — 1 in M13, 2 in M14, 3 in M15, 4 in M16.
- `pub fn derive_hand_stats(hand: &CanonicalHand) -> Vec<PlayerHandStats>`: one entry per player,
  in seat order. It is **infallible and never panics** for any `CanonicalHand` storage can load,
  including hands that would not pass `validate_hand` today (the v3.4 test database stores one
  with no players, which yields no rows). Use checked or saturating integer arithmetic and
  `i128` for money products; treat a missing lookup as "no opportunity", never as an error.
- `pub const COLUMNS: &[&str]` (this exact name) lists every counter and stored money/equity
  column of `stats.md` in schema order; it is the single source for the `hand_player_stats`
  schema. `PlayerHandStats` exposes the player name, `position: Option<u8>` (codes of `stats.md`
  §2) and each column's value by name (e.g. `fn get(&self, column: &str) -> Option<i64>`).
- From M16: `pub fn exact_equity(hole_cards: &[[Card; 2]], board: &[Card]) -> Option<Vec<(u64,
  u64)>>`, each player's equity as a reduced fraction over every completion of `board`
  (`stats.md` §6.2); `None` for fewer than two players, more than five board cards or a repeated
  card.
- `pub fn position_label(code: u8, max_seats: u8) -> &'static str` per `stats.md` §2.

**Derived cache lifecycle** (storage):

1. **Insert.** When `insert_one` stores a new hand, its `hand_player_stats` rows are written in
   the same savepoint. A duplicate writes nothing (its rows already exist).
2. **Open.** After migrations, `SqliteRepository::initialize` reads
   `derived_cache.version` for `name = 'hand_player_stats'`. If it is missing or differs from
   `STATS_VERSION`, it rebuilds in one `BEGIN IMMEDIATE` transaction, re-reading the version
   under the lock so concurrent opens rebuild only once: `DROP TABLE IF EXISTS
   hand_player_stats`, create it from `COLUMNS`, derive every stored hand (via the existing
   `load_hand_by_id`), and upsert `derived_cache`. A hand that cannot be loaded aborts the
   rebuild with its `StorageError` and nothing changes. A new, empty database is "rebuilt"
   instantly, so every existing test that opens a fresh database is unaffected.
3. **Forced rebuild.** `SqliteRepository::rebuild_derived(&mut self) -> Result<DerivedRebuild,
   StorageError>` does the same unconditionally. `DerivedRebuild { hands: usize, version: i64 }`.
   `SqliteRepository::rebuilt_on_open(&self) -> Option<DerivedRebuild>` reports whether step 2
   ran.
4. **Delete.** `ON DELETE CASCADE` on `hand_id` removes rows with their hand; `delete_import_run`
   needs no change.

`hand_player_stats` is created by the rebuild, **not** by `MIGRATIONS`: it is a cache whose shape
follows `STATS_VERSION`. Migration 3 adds only the `derived_cache` table, which also makes older
builds refuse a v3.7 database (`UnsupportedSchemaVersion`).

## 3. Data and the oracle

- **Corpus:** the 10 v3.6 exports (989 hands) and the v3.5 export (294 hands), 1,283 hands and
  6,840 player-hands; plus `EV_CASHOUT_RECEIVED` (HD145319803) for M16.
- **Independent oracle:** `tools/oracles/v3_7_oracle.py`, a Python implementation of
  `docs/stats.md` that reads a `poker_tracker` SQLite database and shares no code with the crate
  (standard library only; `eval7` for M16 equity). **Every expected number in this spec came from
  it**, and its output is labelled with the section that pins each number. To reproduce, follow
  `tools/oracles/README.md` (it also imports `EV_CASHOUT_RECEIVED`, which is not in a fixture
  file). Never "fix" an expected number to match your code: if
  they differ, find which side departs from `docs/stats.md` and report it.
- "Hero" means the player with `is_hero` in each hand. All pinned figures are Hero's unless a
  table says otherwise.

## 4. M13 — Derived cache, positions and basic facts

**Goal:** every stored hand has `hand_player_stats` rows with positions and the M13 counters, kept
current automatically.

**Tasks:**

1. **M13.1 Stats module.** Create `src/stats/` with the contract of §2 and `pub mod stats;` in
   `src/lib.rs`. `STATS_VERSION = 1`. Implement positions (`stats.md` §2) and the M13 counters:
   `hands`, `p_vpip`, `p_pfr`, `f_saw`, `t_saw`, `r_saw`, `sd_went`, `sd_won`, `won_hand`,
   `f_saw_won`, and the 20 action counts `{p,f,t,r}_{bets,raises,calls,checks,folds}`.
2. **M13.2 Migration 3** (append to `MIGRATIONS`; `SCHEMA_VERSION` becomes 3):

   ```sql
   CREATE TABLE derived_cache (
       name TEXT PRIMARY KEY CHECK (length(name) > 0),
       version INTEGER NOT NULL CHECK (version >= 1),
       built_at TEXT NOT NULL
   ) STRICT;
   ```

3. **M13.3 `hand_player_stats`** (created by the rebuild from `COLUMNS`):

   ```sql
   CREATE TABLE hand_player_stats (
       hand_id INTEGER NOT NULL REFERENCES hands(id) ON DELETE CASCADE,
       player_id INTEGER NOT NULL REFERENCES players(id) ON DELETE RESTRICT,
       position INTEGER CHECK (position IS NULL OR (position BETWEEN 0 AND 9)),
       -- one line per counter in COLUMNS order:
       <counter> INTEGER NOT NULL CHECK (<counter> >= 0),
       PRIMARY KEY (hand_id, player_id)
   ) STRICT, WITHOUT ROWID;
   CREATE INDEX idx_hand_player_stats_player ON hand_player_stats(player_id, hand_id);
   ```

   Every column added later gets the same `CHECK (... >= 0)` except `ev_net_micros` (M16), which
   is signed.
4. **M13.4 Lifecycle** of §2 (insert, open, forced rebuild, delete).
5. **M13.5 CLI.** `poker_tracker --rebuild-stats [--db <path>]` opens with `open_existing`
   (never creates a file), calls `rebuild_derived` unless the open already rebuilt, prints
   `Rebuilt statistics for <N> hand(s) (stats version <V>)` and exits `0` (`1` on error). It
   cannot be combined with an input directory or `--delete-run` (usage error, exit `2`). Whenever
   any other command opened a database whose cache was rebuilt with at least one hand, print
   `Statistics cache rebuilt for <N> hand(s) (stats version <V>)` to **stderr** first (stdout of
   `--report` holds only CSV; a new, empty database prints nothing). Update `USAGE` and the
   module doc.
6. **M13.6 Tests.**
   - `src/stats` unit tests, built from canonical actions with no site text: positions at 6, 5,
     4, 3 and 2 players; an extra big-blind poster and a missed-blind poster keep their seat's
     position; a straddler keeps theirs; a hand whose SB is not left of the button gets `NULL`
     positions; VPIP ignores posts and the BB's or straddler's check; a fold on the turn has
     `t_saw = 1`, `r_saw = 0`; a preflop all-in run out to the river is `sd_went` for both; a
     walk gives the BB `won_hand = 1` and `sd_went = 0`.
   - The three worked examples of `stats.md` §7, restricted to the M13 counters and positions.
   - Storage: rows written on insert equal the rows after `rebuild_derived` for all 1,283 real
     hands; deleting the `derived_cache` row (through `connection()`) and reopening a file
     database rebuilds it and `rebuilt_on_open()` reports it; `delete_import_run` removes the rows; a
     v3.4 database is upgraded and its cache built; four concurrent opens of a stale database
     rebuild exactly once (a new test beside `concurrent_opens_apply_each_migration_once`, which
     stays unchanged).
   - Whole folder (`tests/real_session_v3_6.rs`): the pinned tables below.
   - CLI: `--rebuild-stats` on a missing file exits `1`; combined with an input directory exits
     `2`.

**Pinned (Hero):**

| Position | EP | MP | CO | BTN | SB | BB |
| --- | --- | --- | --- | --- | --- | --- |
| v3.6 folder hands | 96 | 139 | 178 | 185 | 197 | 194 |

No player-hand in the corpus has a `NULL` position.

| Counter | v3.6 folder | v3.5 file |
| --- | --- | --- |
| `hands` | 989 | 294 |
| `p_vpip` | 258 | 67 |
| `p_pfr` | 224 | 57 |
| `f_saw` / `t_saw` / `r_saw` | 198 / 133 / 98 | 50 / 29 / 23 |
| `sd_went` / `sd_won` | 55 / 26 | 16 / 8 |
| `won_hand` / `f_saw_won` | 193 / 87 | 65 / 28 |

| v3.6 folder | bets | raises | calls | checks | folds |
| --- | --- | --- | --- | --- | --- |
| preflop (`p_`) | 0 | 228 | 47 | 30 | 685 |
| flop (`f_`) | 60 | 13 | 19 | 107 | 36 |
| turn (`t_`) | 35 | 7 | 25 | 55 | 22 |
| river (`r_`) | 23 | 11 | 6 | 35 | 24 |

**Docs:** add a **Derived data** section to `docs/invariants.md` (cache rules of §2 and
`stats.md` §1.3, positions summary); in `docs/known_limitations.md` replace the "Derived
statistics" bullet with what remains deferred (M14–M16 until they land, filters M17).

**Done when:**

- [ ] M13.1–M13.6 merged; gates green.
- [ ] A scratch import of `tests/fixtures/ggpoker/` exits `0`, then `--rebuild-stats` on the same
  database prints `Rebuilt statistics for 1283 hand(s) (stats version 1)`. Delete the scratch
  database.

## 5. M14 — Opportunity counters

**Goal:** the preflop and postflop counters of `stats.md` §4.1–§4.2 marked M14.

**Tasks:**

1. **M14.1** Implement every M14 counter exactly as defined (state at a decision, unopened,
   `can_raise`, `can_bet`, preflop aggressor, steal attempt: `stats.md` §3). `STATS_VERSION = 2`.
2. **M14.2 Tests.**
   - The three worked examples of `stats.md` §7 in full (every other counter of `stats.md` §4
     is `0`; later milestones' money and equity columns are not part of this assertion).
   - Canonical unit tests (no site text), one per rule and its near miss:
     - RFI: an open raise is `p_rfi`; a limper before the player removes `p_rfi_opp`; a player
       whose stack does not exceed the big blind has no `p_rfi_opp`;
     - 3-bet and 4-bet: facing one raise gives `p_3bet_opp`; facing an open and a 3-bet gives
       `p_3bet_faced` and `p_4bet_opp`; 4-bet then fold to a 5-bet counts in `p_4bet_fold`;
     - cold call: a blind poster calling a raise is not `p_cc_opp`;
     - steal: an open from CO/BTN/SB is `p_steal`; from MP it is not; a hand with a straddle has
       no steal opportunity; a caller between the steal and the BB removes `p_bb_steal_faced`;
       heads-up, the SB's open is a steal and the BB faces it;
     - c-bets: an all-in preflop aggressor has no `f_cbet_opp`; a raise between the c-bet and a
       player's decision removes `f_cbet_faced`; `t_cbet_opp` requires a flop c-bet by the same
       player;
     - check-raise: check, face a bet, raise is `f_xr`; check, face a bet, call is `f_xr_opp`
       only.
   - Whole folder: the pinned table below.

**Pinned (Hero, v3.6 folder):**

| Counter | Value | Counter | Value |
| --- | --- | --- | --- |
| `p_rfi_opp` / `p_rfi` | 474 / 144 | `p_steal_opp` / `p_steal` | 276 / 89 |
| `p_3bet_opp` / `p_3bet` | 299 / 36 | `p_sb_steal_faced` / `_fold` / `_raise` | 35 / 25 / 10 |
| `p_3bet_faced` / `_fold` / `_call` | 41 / 31 / 7 | `p_bb_steal_faced` / `_fold` / `_raise` | 46 / 33 / 2 |
| `p_4bet_opp` / `p_4bet` | 46 / 4 | `f_cbet_opp` / `f_cbet` | 106 / 52 |
| `p_4bet_faced` / `_fold` / `_call` | 10 / 4 / 5 | `f_cbet_faced` / `_fold` / `_call` / `_raise` | 24 / 13 / 9 / 2 |
| `p_cc_opp` / `p_cc` | 109 / 7 | `t_cbet_opp` / `t_cbet` | 22 / 10 |
| `f_xr_opp` / `f_xr` | 34 / 6 | `t_cbet_faced` / `_fold` / `_call` / `_raise` | 5 / 0 / 5 / 0 |

**Done when:**

- [ ] M14.1–M14.2 merged; gates green.
- [ ] A scratch import of `tests/fixtures/ggpoker/` exits `0`, then `--rebuild-stats` prints
  `Rebuilt statistics for 1283 hand(s) (stats version 2)`. Delete the scratch database.

## 6. M15 — Results, sessions and reports

**Goal:** money per player-hand, table sessions, and four CSV reports from the CLI.

**Tasks:**

1. **M15.1 Money columns** (`stats.md` §6.1): `rake_attributed_micros`,
   `fees_attributed_micros`, `blinds_posted_micros`, `straddles_posted_micros`.
   `STATS_VERSION = 3`. In each hand the attributed parts sum exactly to the hand's `Rake` and
   other deductions (test it over the corpus).
2. **M15.2 Report module** `src/report/`:
   `pub fn write_report(repo: &SqliteRepository, kind: ReportKind, out: &mut impl Write) ->
   Result<(), ReportError>` with `ReportKind::{Results, Positions, Sessions, Stats}`. Read only
   integer sums from SQLite; compute ratios with `Decimal`; format per `stats.md` §1.2. Hero only;
   every row is per currency; currencies in alphabetical order.
3. **M15.3 CLI.** `poker_tracker --report <results|positions|sessions|stats> [--db <path>]
   [--out <path>]`: opens with `open_existing`, writes CSV (with a header line, LF endings) to
   `--out` or stdout, exits `0`; `1` on a missing database or I/O error; `2` on an unknown kind or
   when combined with an import, `--delete-run` or `--rebuild-stats`. A database with no hands
   prints the header only.
4. **M15.4 CSV contracts** (columns in this order):

   | Report | Columns | Rows |
   | --- | --- | --- |
   | `results` | `currency,stakes,hands,net,net_per_100,bb_won,bb_per_100,rake,fees,blinds,straddles` | One per currency and stakes (`<sb>/<bb>`, e.g. `0.05/0.1`), by big blind then small blind; then `stakes = ALL` for each currency with more than one stake |
   | `positions` | `currency,position,hands,net,bb_per_100,vpip,pfr,rfi` | One per currency and position label, in the order `EP, MP, CO, BTN, SB, BB`, then `?` for `NULL` positions if any |
   | `sessions` | `currency,table,start,end,hands,net,bb_per_100` | One per session (`stats.md` §6.1), by currency, start time, then table; `start`/`end` as stored (`2026-08-21T16:54:10`) |
   | `stats` | `currency,stat,numerator,denominator,value` | One per stat of `stats.md` §5, in that table's order (`af_*` and `afq_*` last) |

5. **M15.5 Tests.** Golden CSVs for the v3.6 folder imported alone (below), the v3.5 results row,
   the `ALL` row and position `?` row on in-memory synthetic hands, the CLI paths above, and that
   `--report` never creates a database file.

**Pinned (v3.6 folder imported alone):**

```csv
currency,stakes,hands,net,net_per_100,bb_won,bb_per_100,rake,fees,blinds,straddles
CAD,0.05/0.1,989,-25.79,-2.61,-257.90,-26.08,14.23115,2.500423,29.25,0
```

```csv
currency,position,hands,net,bb_per_100,vpip,pfr,rfi
CAD,EP,96,3.64,37.92,25.00,23.96,24.21
CAD,MP,139,0.15,1.08,27.34,27.34,32.32
CAD,CO,178,-5.08,-28.54,23.03,21.35,22.41
CAD,BTN,185,19.31,104.38,25.95,23.24,32.99
CAD,SB,197,-17.03,-86.45,29.95,29.95,46.97
CAD,BB,194,-26.78,-138.04,24.74,11.86,0.00
```

```csv
currency,table,start,end,hands,net,bb_per_100
CAD,NLHRed14,2026-08-21T16:54:10,2026-08-21T17:05:03,24,7.75,322.92
CAD,NLHRed18,2026-08-21T16:54:55,2026-08-21T18:53:36,167,-17.71,-106.05
CAD,NLHRed4,2026-08-21T17:06:33,2026-08-21T18:46:04,182,-3.03,-16.65
CAD,NLHRed10,2026-08-21T18:07:04,2026-08-21T18:49:17,87,3.78,43.45
CAD,NLHRed14,2026-08-21T18:47:35,2026-08-21T19:45:04,90,-1,-11.11
CAD,NLHRed19,2026-08-21T18:52:15,2026-08-21T19:47:00,102,9.41,92.25
CAD,NLHRed7,2026-08-21T18:54:25,2026-08-21T19:45:18,84,-17.48,-208.10
CAD,NLHRed21,2026-09-25T01:00:42,2026-09-25T02:09:56,99,-3.25,-32.83
CAD,NLHRed3,2026-09-25T01:39:03,2026-09-25T02:36:41,100,3.45,34.50
CAD,NLHRed15,2026-09-25T02:18:14,2026-09-25T02:57:07,54,-7.71,-142.78
```

```csv
currency,stat,numerator,denominator,value
CAD,vpip,258,989,26.09
CAD,pfr,224,989,22.65
CAD,saw_flop,198,989,20.02
CAD,wtsd,55,198,27.78
CAD,wsd,26,55,47.27
CAD,wwsf,87,198,43.94
CAD,win,193,989,19.51
CAD,rfi,144,474,30.38
CAD,three_bet_pf,36,299,12.04
CAD,fold_to_three_bet_pf,31,41,75.61
CAD,call_three_bet_pf,7,41,17.07
CAD,four_bet_pf,4,46,8.70
CAD,fold_to_four_bet_pf,4,10,40.00
CAD,call_four_bet_pf,5,10,50.00
CAD,cold_call_pf,7,109,6.42
CAD,att_to_steal,89,276,32.25
CAD,fold_sb_to_steal,25,35,71.43
CAD,raise_sb_steal,10,35,28.57
CAD,fold_bb_to_steal,33,46,71.74
CAD,raise_bb_steal,2,46,4.35
CAD,cbet_flop,52,106,49.06
CAD,fold_to_cbet_flop,13,24,54.17
CAD,call_cbet_flop,9,24,37.50
CAD,raise_cbet_flop,2,24,8.33
CAD,cbet_turn,10,22,45.45
CAD,fold_to_cbet_turn,0,5,0.00
CAD,call_cbet_turn,5,5,100.00
CAD,raise_cbet_turn,0,5,0.00
CAD,check_raise_flop,6,34,17.65
CAD,af_pf,228,47,4.85
CAD,af_flop,73,19,3.84
CAD,af_turn,42,25,1.68
CAD,af_river,34,6,5.67
CAD,af_total,149,50,2.98
CAD,afq_pf,228,960,23.75
CAD,afq_flop,73,128,57.03
CAD,afq_turn,42,89,47.19
CAD,afq_river,34,64,53.12
CAD,afq_total,149,281,53.02
```

(`afq_river` is 53.125 and rounds half-even to 53.12.)

v3.5 file imported alone, `results` row:
`CAD,0.05/0.1,294,2.86,0.97,28.60,9.73,2.195316,0.353832,8.3,0`.

**Done when:**

- [ ] M15.1–M15.5 merged; gates green.
- [ ] On a scratch import of `tests/fixtures/ggpoker/v3_6/`, the four `--report` outputs are
  byte-identical to the pinned CSVs. Delete the scratch database.

## 7. M16 — All-in equity

**Goal:** an exact, float-free all-in equity for every qualifying hand (`stats.md` §6.2), and
equity-adjusted results.

**Tasks:**

1. **M16.1 Hand evaluator** in `src/stats/` (no dependencies): ranks any 5–7 card hand into the
   nine categories with kickers, compared as an integer. Exact equity by enumerating every board
   completion. No floats; equity is a reduced fraction of `u64`s.
   **Performance (release build):** one heads-up preflop all-in (1,712,304 runouts) in under 1 s;
   a full `--rebuild-stats` of the 1,283-hand corpus in under 10 s. A bitmask-based 7-card
   evaluator meets this; evaluating 21 five-card subsets per board does not.
   **Test time:** `cargo test --locked` (debug) must stay under 60 s wall-clock. Every import of
   the corpus now computes equity, so if the debug build is too slow you may add
   `[profile.dev] opt-level = 1` to `Cargo.toml` (§10 decision 14; it does not change
   `Cargo.lock`). List it under Deviations.
2. **M16.2 Columns** `ev_qualified`, `ev_equity_num`, `ev_equity_den`, `ev_net_micros` per
   `stats.md` §6.2. `STATS_VERSION = 4`.
3. **M16.3 Reports.** Append `allin_hands,ev_net,ev_bb_per_100` to `results`, and `ev_net` to
   `sessions`.
4. **M16.4 Tests** (§7.1–§7.5).

### 7.1 Evaluator unit tests

Every category beats the one below it; kickers decide within a category; the wheel (A-2-3-4-5) is
the lowest straight and the steel wheel the lowest straight flush; the board can play for both
players (a split); 5-, 6- and 7-card inputs agree.

### 7.2 Exact equities (from `eval7`, independent of the crate)

Assert through `stats::exact_equity`.

| Hands | Board | Equity (in order) |
| --- | --- | --- |
| Ah As vs Kc Kd | — | 29603/36432, 6829/36432 |
| Ac Kd vs Ah Kc | — | 862441/1712304, 849863/1712304 |
| As Ks vs Qh Qd vs Jc Tc | — | 61241/152306, 281378/685377, 256829/1370754 |
| Ah Ad vs Kh Kd | Ks 7c 2d | 17/198, 181/198 (PokerTracker 4's documented example, ≈ 8.6%) |

### 7.3 The four real EV Cashout hands

Equity of the cashing player at the street they **chose EV Cashout** (computed with
`stats::exact_equity` on that street's board; this is not always the all-in street), and
GGPoker's cashout as a share of `equity × pot after all deductions`:

| Hand | Fixture | Street | Cashing player's equity | Pot after deductions | Cashout | Ratio |
| --- | --- | --- | --- | --- | --- | --- |
| HD140653161 | `EV_CASHOUT_CALLER` | turn | 19/22 | 4.46 | 3.8 | 0.9865 |
| HD140647649 | `EV_CASHOUT_FLOP` | flop | 857/990 | 3.78 | 3.23 | 0.9871 |
| HD140642980 | `EV_CASHOUT` | turn | 37/44 | 2.81 | 2.33 | 0.9861 |
| HD145319803 | `EV_CASHOUT_RECEIVED` | flop | 346/495 | 9.93 | 6.87 | 0.9898 |

(For a risk-paid cashout, "cashout" is the payout minus the risk paid.) Assert the equities
exactly and `0.98 ≤ ratio ≤ 1.00` with `Decimal`. This pins the pot basis (§10 decision 4).

The `ev_equity_*` columns hold the equity at the **all-in street** instead: the cashing player's
is 19/22 (turn) in HD140653161, 1223075/1712304 (preflop) in HD140647649, 541/990 (flop) in
HD140642980 and 346/495 (flop) in HD145319803. Assert these too.

`EV_CASHOUT_RECEIVED` columns: `a2939c73` qualifies with equity 149/495 and `ev_net_micros =
-2_010_970`; `81ae8569` qualifies (346/495) but keeps `ev_net_micros = net_micros = 1_870_000`
(EV Cashout).

### 7.4 Qualification

In the corpus, 41 hands qualify (28 in the v3.6 folder, 13 in the v3.5 file). Four all-in hands
are excluded because live contributions differ (side pots): HD145383901 (`FOUR_WAY_ALL_IN`),
HD140643875 (`THREE_WAY_ALL_IN_SIDE_POT`), HD140650460 (second hand of `SAMPLE_SESSION`) and
HD140652493. Assert both facts.

Hero's qualifying hands (money printed exactly, as in the reports):

| Hand | All-in street | Hero equity | Pot after deductions | Hero net | Hero EV net |
| --- | --- | --- | --- | --- | --- |
| HD140645133 | flop | 41/1980 | 7.7 | -4.1 | -3.940556 |
| HD140643576 | turn | 1/1 | 19.81 | 9.43 | 9.43 |
| HD140652930 | preflop | 812323/1141536 | 7.89 | -4.1 | 1.514565 |
| HD140654227 | preflop | 49342/107019 | 5.31 | 2.67 | -0.19178 |
| HD140655678 | turn | 1/1 | 21.95 | 10.5 | 10.5 |
| HD140654333 | preflop | 253495/311328 | 4.01 | 2.01 | 1.265093 |
| HD140646342 | turn | 5/22 | 18.96 | -10 | -5.690909 |
| HD145383838 | turn | 21/22 | 12.25 | 6.08 | 5.523182 |
| HD145411928 | flop | 19/36 | 16.87 | -0.42 | 0.053611 |
| HD140647703 | flop | 32/33 | 2.55 | 1.29 | 1.212727 |
| HD140647294 | preflop | 1/2 | 6.84 | -0.07 | -0.07 |
| HD140643125 | flop | 881/990 | 2.74 | 1.35 | 1.048323 |

(The first nine are in the v3.6 folder; HD140647703, HD140647294 and HD140643125 are in the v3.5
file. HD145411928 and HD140647703 were run twice.)

### 7.5 Equity-adjusted results

| File | Qualifying hands | Hero in | Hero net | Hero EV net |
| --- | --- | --- | --- | --- |
| 20260821-2054 NLHRed14 | 0 | 0 | 7.75 | 7.75 |
| 20260821-2054 NLHRed18 | 5 | 2 | -17.71 | -17.550556 |
| 20260821-2106 NLHRed4 | 4 | 1 | -3.03 | 2.584565 |
| 20260821-2207 NLHRed10 | 8 | 1 | 3.78 | 0.91822 |
| 20260821-2247 NLHRed14 | 4 | 2 | -1 | -1.744907 |
| 20260821-2252 NLHRed19 | 1 | 0 | 9.41 | 9.41 |
| 20260821-2254 NLHRed7 | 2 | 1 | -17.48 | -13.170909 |
| 20260925-0500 NLHRed21 | 1 | 1 | -3.25 | -3.806818 |
| 20260925-0539 NLHRed3 | 2 | 1 | 3.45 | 3.923611 |
| 20260925-0618 NLHRed15 | 1 | 0 | -7.71 | -7.71 |
| **v3.6 total** | **28** | **9** | **-25.79** | **-19.396794** |
| v3.5 file | 13 | 3 | 2.86 | 2.48105 |

Reports for the v3.6 folder imported alone:

```csv
currency,stakes,hands,net,net_per_100,bb_won,bb_per_100,rake,fees,blinds,straddles,allin_hands,ev_net,ev_bb_per_100
CAD,0.05/0.1,989,-25.79,-2.61,-257.90,-26.08,14.23115,2.500423,29.25,0,9,-19.396794,-19.61
```

The `sessions` report gains `ev_net` as its last column; its values for the ten sessions, in the
order of §6, are `7.75, -17.550556, 2.584565, 0.91822, -1.744907, 9.41, -13.170909, -3.806818,
3.923611, -7.71`.

**Done when:**

- [ ] M16.1–M16.4 merged; gates green; the performance targets met (report the timings).
- [ ] `docs/CHANGELOG.md` has the v3.7 row and section; `docs/known_limitations.md` lists what
  stays deferred (M17 onward, side-pot all-in equity, the stats of `stats.md` §8).

## 8. Later milestones (planning only — do not build)

- **M17 — Filters.** A site-neutral filter tree (AND / OR / NOT, saved under a name) compiled to
  SQL over `hands` and `hand_player_stats`, accepted by every report (`--filter <name>`). Groups
  follow PokerTracker 4: game details (currency, stakes, table size, dates), hand details
  (position, won/lost, showdown, pot size), session details (session net, duration), hand values
  (hole cards, made hand), board texture, and actions and opportunities (any counter). Also
  `--player <name>` for an opponent or the pool.
- **M19 — Database management.** Re-parse stored raw text and quarantined hands after a parser
  upgrade (closes the known limitation); export selected hands as raw text; purge by date or run;
  SQLite online backup; hero aliases across sites (the accounts layer); optional "my currency"
  conversion with a stored rate per session (Game Currency vs My Currency, as in PT4), never
  mixing stored amounts.
- **Deferred further:** M18 leak report (Hero's stats by position against reference ranges and
  the pool); M20 hand review (tags, notes, an HTML replayer); a PT4 cross-check of the pinned
  stats on the same exports (owner, final testing).

## 9. Out of scope for v3.7

- Any parser or format change (no new line shapes; `src/parser/` changes only if a bug is found,
  as a deviation).
- All-in equity for hands with side pots, with unknown hole cards, or decided on the river.
- The stats of `stats.md` §8, opponent or pool reports, filters, HUDs, GUIs, live import.
- Floats, new dependencies, a crate version bump (it would change `Cargo.lock`).

## 10. Decisions (final)

| # | Decision | Final | Owner |
| --- | --- | --- | --- |
| 1 | Derived data | A rebuildable cache, versioned by `STATS_VERSION`, outside `MIGRATIONS`, rebuilt automatically on open | Owner (cache like PT4); Claude's call (automatic rebuild) |
| 2 | Stat catalogue | PokerTracker 3's statistical reference, restated in `docs/stats.md`; PT3 semantics where it states them (e.g. "Fold to 3Bet" counts every fold facing a 3-bet) | Owner |
| 3 | External check against PT4 | Later, for final testing; v3.7 uses the Python oracle | Owner |
| 4 | All-in equity pot | After **all** deductions (rake, jackpot, bingo, fortune, tax); GGPoker's cashout amounts confirm it (§7.3) | Owner (after rake); Claude's precision |
| 5 | Winrate unit | Big blinds (`bb/100`, PT4's No-Limit convention), not PT3's big bets | Claude's call |
| 6 | Positions | PT3/PT4 numbering (BTN 0 … BB 8, SB 9); PT4 labels (6-max: EP 3, MP 2); heads-up button = SB | Claude's call |
| 7 | Steals | No steal opportunities in a hand with a straddle | Claude's call |
| 8 | Multiway all-ins with side pots | Excluded from equity adjustment (PT4's rule) | Claude's call |
| 9 | EV Cashout in equity results | The cashing player keeps their actual net; opponents are adjusted | Claude's call |
| 10 | Rake attribution | Contributed method (by `invested`), largest-remainder rounding to micro-units | Claude's call |
| 11 | Sessions | Same table, gaps of at most 30 minutes; computed at report time | Claude's call |
| 12 | VPIP / PFR denominator | All hands dealt, walks included (PT3: "out of all hands") | Claude's call |
| 13 | Opportunity precision | First qualifying decision per player-hand; no reopening check after short all-ins | Claude's call |
| 14 | Debug test speed | If exact equity makes `cargo test` exceed 60 s, `[profile.dev] opt-level = 1` may be added to `Cargo.toml` (no `Cargo.lock` change) | Claude's call |

"Claude's call" decisions stand unless the owner changes them before M13 starts.
