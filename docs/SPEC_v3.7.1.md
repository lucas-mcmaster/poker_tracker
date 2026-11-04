# poker_tracker — v3.7.1 amendment spec (fixes from the v3.7 review)

Nov 3, 2026 · @Lucas · from the deep review of v3.7 (M13–M16)

## 0. Agent brief

This spec moves `poker_tracker` from v3.7 (M16 plus the review's test patch, **209 tests**) to
v3.7.1 in two milestones, **M16.1** and **M16.2**. It amends `docs/SPEC_v3.7.md`: for the items
below, this spec is the contract; everything else in `SPEC_v3.7.md` still holds. v3.8 (M17
filters, `SPEC_v3.7.md` §8) starts only after M16.2.

| Milestone | Delivers | Code it may change |
| --- | --- | --- |
| M16.1 | Exact `bb_won` aggregation; derived-data fallbacks; non-Unicode CLI arguments; no import run left by a failed first batch; testable rebuild-once; rebuild errors that name the hand; CSV quoting | `src/report/`, `src/stats/money.rs`, `src/stats/mod.rs`, `src/storage/repository.rs`, `src/pipeline/ingester.rs`, `src/main.rs` |
| M16.2 | Parser strictness: exact amounts, the heads-up button, the GGPoker EV Cashout street (the core keeps allowing a preflop cashout), runouts that keep the cards already bet on, specific validator tests | `src/parser/`, `src/domain/` (doc comments only) |

**How to work through it:** read `AGENTS.md`, then this spec in full, then the sections of
`docs/stats.md`, `docs/invariants.md` and `docs/SPEC_v3.7.md` that each task names. Do not read
`docs/BLUEPRINT.md`, the historical specs or fixture files. Find fixtures by grepping
`pub const <NAME>` in `tests/common/mod.rs`. Do M16.1, commit, report and wait; then M16.2.

**Editing existing tests.** Only the edits this spec lists are authorized. Every other existing
test stays unchanged and must pass. List each edit you make in the report.

**No pinned number changes.** No corpus hand is quarantined that imports today; the four golden
CSVs, the v3.5 `results` row, every pinned table of `SPEC_v3.7.md` and
`tools/oracles/compare_cache.py` stay exactly as they are. `STATS_VERSION` stays **4**: no stored
column of a stored hand changes (the fallbacks of M16.1.2 are unreachable for hands storage can
hold), and every M16.1 report change is computed at report time (`AGENTS.md` §3.10).

**Gate** as in `AGENTS.md` §4. With `[profile.dev] opt-level = 1` (the review's patch 4, approved
as D3) `cargo test --locked` takes about 8 s; without it 44–60 s depending on the
machine (59 s on the owner's).

## 1. M16.1 — Results, derived data, storage and CLI

### M16.1.1 `bb_won` is aggregated exactly (owner decision D1)

**Why.** `bb_won` adds `Σ net / big_blind` per big-blind value with `Decimal` (`BbWon::total`).
A quotient that does not terminate (a big blind of 3.00 or 6.00) is rounded to 28 digits, and two
such quotients of different magnitude can leave the sum just off a half-even midpoint. With Hero
losing 29.99 at 1.5/3 and winning 23.59 at 3/6, the exact `bb_won` of the `ALL` row is
−9.99666… + 3.93166… = **−6.065**, which rounds half-even to **−6.06**; the program prints
**−6.07**. Positions and sessions rows that mix big blinds are affected the same way.

**Contract** (`docs/stats.md` §6.1–§6.2 as amended by the review's docs patch):

- `bb_won` of any row is the exact rational `Σ_g net_g / bb_g` over the row's big-blind values
  `g` (integers in micro-units). `bb/100` is exactly `100 × bb_won / hands`. `ev_bb_per_100` is
  the same with `ev_net_micros`. Each printed figure is that exact value rounded half-even to 2
  decimals, printed with 2 decimals; a value that rounds to zero prints `0.00`, never `-0.00`.
- This applies to `results` (stakes rows and `ALL` rows: `bb_won`, `bb_per_100`,
  `ev_bb_per_100`), `positions` (`bb_per_100`) and `sessions` (`bb_per_100`). Every other figure
  is unchanged (a single integer division with `Decimal` is already exact enough, because a
  non-terminating quotient is never a midpoint).
- Integer arithmetic only (`i128`, checked). If a figure does not fit, the report fails with the
  new `ReportError::ArithmeticOverflow { figure: &'static str }` and writes nothing (the CLI exits
  `1`); its message names the figure. No real database comes near this; it is fail-closed rather
  than approximate.
- Design the internals as you like (for example `BbWon::total` returning a reduced fraction and
  one shared `round_half_even` for fractions). Keep "divide once per big-blind value" visible in
  the code.

**Tests** (new; write them first and see them fail where marked):

1. Unit tests in `src/report/` on the aggregation, as `(big blind micros, net micros)` groups →
   `bb_won`, and `bb/100` over `hands`:
   - `(3_000_000, −29_990_000), (6_000_000, 23_590_000)` → `-6.06` (fails today: `-6.07`);
     over 2 hands → `-303.25`.
   - The same with both nets negated → `6.06`; over 2 hands → `303.25`.
   - `(240_000, −10_000), (480_000, −20_000), (960_000, −40_000)` → exactly −0.125 → `-0.12`
     (fails today: `-0.13`); over 4 hands → exactly −3.125 → `-3.12`.
   - `(100_000, −100)` → −0.001 → `0.00`.
   - The overflow path returns `ArithmeticOverflow` (construct big blinds whose least common
     multiple overflows `i128`).
2. `tests/report.rs`, through `write_report` on a file database. Two heads-up hands at one
   table and one time, Hero on seat 1 (button, small blind), Villain on seat 2, stacks 100, no
   deductions. Preflop: Hero posts the small blind, Villain the big blind, Hero calls, Villain
   checks. Flop `2c 7d 9h`: Villain checks, Hero bets `X`, Villain calls. Turn `Ks` and river
   `3s`: both check. Showdown; the winner takes `2 × (bb + X)`.
   - `L1` at 1.5/3, `X = 26.99`, Hero `Ah Ad`, Villain `Kh Kd` (Villain's set wins): Hero net
     −29.99.
   - `W1` at 3/6, `X = 17.59`, Hero `Kh Kd`, Villain `Ah Ad` (Hero's set wins): Hero net +23.59.

   `results` must be exactly:

   ```csv
   currency,stakes,hands,net,net_per_100,bb_won,bb_per_100,rake,fees,blinds,straddles,allin_hands,ev_net,ev_bb_per_100
   CAD,1.5/3,1,-29.99,-2999.00,-10.00,-999.67,0,0,1.5,0,0,-29.99,-999.67
   CAD,3/6,1,23.59,2359.00,3.93,393.17,0,0,3,0,0,23.59,393.17
   CAD,ALL,2,-6.4,-320.00,-6.06,-303.25,0,0,4.5,0,0,-6.4,-303.25
   ```

   (Today the `ALL` row prints `-6.07`.) `positions` is `CAD,SB,2,-6.4,-303.25,100.00,0.00,0.00`
   and `sessions` is
   `CAD,GGPoker,TestTable,2026-08-21T17:00:00,2026-08-21T17:00:00,0,2,-6.4,-303.25,-6.4`
   (with `make_canonical_hand`'s table and timestamp).

### M16.1.2 Derived-data fallbacks fail closed, and the split cannot overflow

`derive_hand_stats` must stay infallible (`SPEC_v3.7.md` §2). One policy, which you document in
`docs/invariants.md` §3 (M16.1.8): **an amount that cannot be represented in micro-units, or a
sum that overflows, never becomes a plausible value.** For equity, such a hand does not qualify;
for a split, every part is `0`. None of this is reachable for a hand storage can hold (every
stored amount is an `i64` of micro-units, and storage computes and stores the sums itself), so
no definition in `docs/stats.md` changes for any storable hand and `STATS_VERSION` stays 4.

1. `stats::money::split_by_largest_remainder` sums the weights and the base parts in `i128`
   (with negative weights clamped to 0 in both the products and the sum), so it never overflows
   or panics. Test: two players invest `5_000_000_000_000` each (5e12, `total_pot` `1.0`,
   `Rake` `0.01`): `derive_hand_stats` does not panic in a debug build and the rake parts are
   `[5_000, 5_000]` (10,000 micro-units × 5e18 / 1e19). Today it panics with
   `attempt to add with overflow` in the weights' `sum()` (`money.rs:35`).
2. In `compute_all_in_equity` (`src/stats/mod.rs`): if a live player's `invested` or
   `PostDeadBlind` amounts, the deductions sum, or the pot after deductions cannot be converted
   or computed, the hand does not qualify (`ev_qualified = 0`, `0/1`, `ev_net_micros =
   net_micros`). Today they become `0` (equal contributions qualify; a pot of `0`). Test: two
   live players all-in on the turn, each investing 10, both hole cards known, winners `20`,
   `total_pot` `20.0000005` (not representable): nobody qualifies and each `ev_net_micros` is
   the player's net (`+10_000_000` and `−10_000_000`). Today both qualify with
   `ev_net_micros = −10_000_000`.
3. Replace the `as` casts in `compute_all_in_equity` and `split_by_largest_remainder` with
   `try_from` (or keep a cast only where a comment states why it is lossless).
4. `money.rs` blind and straddle sums: a sum that overflows makes that column `0` (today the
   overflowing post is skipped and the column keeps a partial sum).

### M16.1.3 Command-line arguments that are not Unicode (High: a panic)

`main.rs` reads `std::env::args()`, which **panics** on an argument that is not valid Unicode
(exit 101; for example a Linux path containing byte `0xE9`). Read `std::env::args_os()`; keep
paths (`--db`, the input directory, `--out`) as `OsString`/`PathBuf`; an option whose value must
be text (`--report`, `--stakes`, `--ggpoker-currency`, `--delete-run`) that is not valid
Unicode is a usage error (exit `2`). Tests in `tests/cli.rs` under `#[cfg(unix)]` (build the
argument with `std::os::unix::ffi::OsStrExt`): `--rebuild-stats --db <dir>/caf\xE9.sqlite`
(missing) exits `1`; an import of a directory into `--db <dir>/caf\xE9.sqlite` exits `0` and
creates that file; `--report` with the value `b"r\xE9sults"` exits `2`.

### M16.1.4 A failed first batch leaves no import run

`DirectoryIngester::flush` commits `register_import_run` in its own transaction before the
guarded batch. When the batch then fails (a stale cache, or a lock taken between the two), the
`import_runs` row stays, with no hands, and the CLI prints its run id. `SPEC_v3.7.md` §2 step 5
says nothing from the batch is stored. Write the run (with the import's real `started_at`,
`.agents/rules/ARCHITECTURE.md` §7) in the same transaction as the first batch, and keep later
batches of the same import using that run. Public API changes are yours to design (for example
passing `started_at` to the batch); update the callers and keep `insert_sourced_hands_batch`'s
existing behaviour for its other callers. **Authorized test edit:** in
`tests/storage.rs::a_stale_cache_import_quarantines_nothing_even_with_a_broken_hand`, add
`assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM import_runs"), 0);` (it fails today with `1`).
Today no test checks the stored start time either (replacing `register_import_run` with
`Ok(())` passes the suite, because the batch then registers the run at flush time). Make the
time a parameter the test can see and add a test that the stored `import_runs.started_at` is the
`started_at` the import was given (for example a public `ingest_directory_at` or an
`ImportRun { id, started_at }` argument; your design, listed under Deviations).

### M16.1.5 The rebuild-once decision can be tested

`four_concurrent_opens_of_stale_database_rebuild_exactly_once` cannot fail: replacing the
re-read of `derived_cache.version` under the lock with "rebuild" still passes 6 runs in 6,
because the migration loop already serialises the opens. Factor the decision so it takes the
version read under the lock as input (for example `fn rebuild_if_stale(conn: &Connection) ->
Result<Option<DerivedRebuild>, StorageError>` called inside the `BEGIN IMMEDIATE` transaction),
and add a unit test in `src/storage/repository.rs`: an open whose first, unlocked read saw a
stale version but whose locked read sees `STATS_VERSION` does not rebuild (`built_at` and the
cache rows are unchanged). The existing concurrency test stays as it is.

### M16.1.6 Rebuild errors name the hand

In `rebuild_derived_in_tx`, a derived row whose player has no `player_id` is skipped today
(`if let Some(&pid)`); the insert path returns `MissingPlayerId` instead. Make the rebuild return
an error too (`MissingPlayerId`), and wrap every error raised while one hand is processed (the
player query, `insert_player_stats`) in `DerivedRebuildFailed { platform, site_hand_id, source }`
like a load error. The loader and the player query read the same tables, so the missing-player
path cannot be reached through the public API; no new test is required for it (optionally, have
the loader return the player ids and drop the second query).

### M16.1.7 CSV text fields are quoted when needed

Reports print text fields (`currency`, `stakes`, `platform`, `table`, `position`, `stat`)
unquoted. A table name is free text from the site (the GGPoker parser accepts any `[^']+`), so a
comma would shift every later column. Quote a field per RFC 4180 when it contains `,`, `"`, CR
or LF (double the quotes); every other field stays as today, so no golden changes. Test: a
sessions report for a hand at table `Table, "A"` prints `"Table, ""A"""` in the `table` column.

### M16.1.8 Docs

`docs/invariants.md` §3 (Reports: the exact `bb_won` rule, the overflow error and CSV quoting;
All-in equity and Money: the fail-closed policy; Cache lifecycle: rebuild errors name the hand),
`docs/known_limitations.md` (nothing to remove), and a `## v3.7 → v3.7.1` section in
`docs/CHANGELOG.md` with one line per task. Do not edit `docs/stats.md` (the docs patch already
did). Three wording fixes in `src/main.rs` and its docs: `USAGE` and the module doc list exit `3`
also for "no `.txt` files found" (`AGENTS.md` §6); `--rebuild-stats` combined with
`--ggpoker-currency` names that flag in its usage error (today it says "an input directory or
--delete-run"); and the `--delete-run` message and `DeletedImportRun::hands_kept`'s doc say that
the kept hands include hands stored without a source.

**Done when:** gates green; the tests above added; the four goldens, the v3.5 row and
`compare_cache.py --equity` unchanged (a scratch run per `SPEC_v3.7.md` §3); the report lists
the authorized test edit and the new test count.

## 2. M16.2 — Parser strictness

`SPEC_v3.7.md` §9 kept the parser out of v3.7; these are the bugs the review found, so they are
in scope now. Each item is a refusal of input that no real export contains: every corpus hand
still imports, with the same digests. Follow `AGENTS.md` §4 "a new site format" step (4):
near-miss refusal tests. Unless noted, a refusal test parses a fixture after the exact text
replacement given and asserts the error variant **and** a detail that identifies the rule.

### M16.2.1 Amounts are exact and have at most two decimals

`parse_decimal` uses `Decimal::from_str`, which **rounds** an input of more than 28 significant
digits (`AGENTS.md` §3.1: money is rejected, never rounded), and `amt!()` accepts any number of
decimals although every observed GGPoker amount has at most two (no amount in
`tests/fixtures/` or `tests/common/mod.rs` has more). Use `Decimal::from_str_exact` and `amt!() = [0-9]+(?:\.[0-9]{1,2})?`.
Refusal tests on `HU_SHOWDOWN`: `Hero ($13.8 in chips)` → `Hero ($13.801 in chips)`, and →
`Hero ($13.80000000000000000000000000001 in chips)`; both are refused (today both are accepted,
the second with the stack rounded to `13.8`).

### M16.2.2 Heads-up: the button must be seated and post the small blind

The heads-up check `(p.seat == button) != posted_small` is false for both players when nobody
sits at the button seat and nobody posts a small blind, so such a hand is accepted;
`docs/invariants.md` §2 says any other arrangement is refused. Require, heads-up, that a player
sits at the button seat and posts the small blind. Refusal test on `HU_SHOWDOWN`: `Seat #6 is the
button` → `Seat #1 is the button`, remove the line `445e6868: posts small blind $0.05`,
`445e6868: calls $0.05` → `445e6868: calls $0.1`, and `445e6868 (small blind) showed` →
`445e6868 showed` (accepted today).

### M16.2.3 EV Cashout before the flop: allowed by the core, refused by the GGPoker parser (owner decision D2)

**Owner decision D2:** a cashout chosen preflop is allowed, because other platforms offer one.
Where each layer stands:

- **The site-neutral core already allows it.** `validate_hand` accepts an adjustment on any
  street except Showdown. The EV Cashout rule of `docs/stats.md` §6.2 does not depend on the
  street. Pin both, so that neither can be narrowed by accident:
  - Add a canonical test (no site text), heads-up, no deductions, stacks 10:
    - A (seat 1, button, small blind) holds `Ah As`; B (seat 2) holds `Kc Kd`.
    - Preflop: A posts 0.05, B posts 0.10, A raises all-in to 10, B calls all-in.
    - Board `2c 7d 9h 3s 4c`; A wins 20.
    - A has an `EvCashoutRiskPaid` adjustment of `-0.5` with `street: Preflop`.
  - Expected: `validate_hand` accepts the hand, and in `derive_hand_stats`:
    - A: `ev_qualified = 1`, equity `29603/36432`, `ev_net_micros = 9_500_000` (A's net: the
      cashing player keeps it);
    - B: `ev_qualified = 1`, equity `6829/36432`, `ev_net_micros = −6_251_098`
      (round(6829 × 20,000,000 / 36432) = 3,748,902, minus 10,000,000: B is adjusted).

    These values were checked against today's code.
  - Add to `docs/invariants.md` §1, under Adjustments: "An adjustment may be triggered on any
    street before the showdown, preflop included; a site's parser may allow fewer (§2)."
- **The GGPoker parser refuses it.**
  - GGPoker never offers a cashout before the flop, so a preflop `Chooses to EV Cashout` line is
    a format no real export contains (`AGENTS.md` §3.6). It must be quarantined, as
    `docs/invariants.md` §2 already says ("chosen on flop or turn").
  - Today the parser refuses only a river cashout. Make it accept only `Flop | Turn`.
  - Refusal test on `EV_CASHOUT_FLOP`: replace
    `*** FLOP *** [Kc Jd 4s]\n62688fe9: Chooses to EV Cashout\n` with
    `62688fe9: Chooses to EV Cashout\n*** FLOP *** [Kc Jd 4s]\n`.
- **A future site's parser** whose real exports show a preflop cashout accepts it, with its own
  fixture and tests (`AGENTS.md` §4, "a new site format"). Nothing in the core needs to change.

### M16.2.4 A later runout keeps the cards already bet on

Nothing requires every runout to share the board up to the street the betting ended on: in
`RIT_TWICE_TURN` (all-in on the turn), a second run printed with its own flop and turn is
accepted. Add the site-neutral rule to `validate_hand` (`docs/invariants.md` §1 Boards): in a
multi-run hand, every runout has the first runout's cards on each street up to and including the
street of the last action that is not `UncalledBetReturned`. In the GGPoker parser, a
`*** SECOND FLOP ***` / `*** SECOND TURN ***` (and third) header is accepted only when no action
took place on that street. Also refuse a summary `Board` line whose run already had one
(`FIRST Board` twice). Refusal tests on `RIT_TWICE_TURN`:

- `*** SECOND RIVER *** [9h Kc 6d Js] [Kh]` → `*** SECOND FLOP *** [2c 3c 4c]\n*** SECOND TURN
  *** [2c 3c 4c] [5c]\n*** SECOND RIVER *** [2c 3c 4c 5c] [Kh]` and `SECOND Board [Kh]` →
  `SECOND Board [2c 3c 4c 5c Kh]` (accepted today);
- `SECOND Board [Kh]` → `FIRST Board [Ks]` (accepted today);
- and a canonical `validate_hand` near miss with the second runout's turn changed.

### M16.2.5 Validator tests that test the rule they name

`assert_violation` (`src/parser/base.rs` tests) checks only `is_err()`. Two tests pass because a
different rule fires: `rejects_commitment_beyond_stack_and_false_all_in` (a stack of 0.05 trips
"not marked all-in") and `rejects_folded_winner_and_action_after_fold` (a winner who invested 0
trips the side-pot rule). Deleting any of these nine rules fails no parser or domain test:

| Rule (`src/parser/base.rs`, v3.7 lines) | Message today |
| --- | --- |
| over-stack commitment (~372) | `'<name>' commits <amount> with a starting stack of <stack>` |
| folded winner (~491) | `folded player '…' wins a pot` |
| uncalled return while betting is open (~1035) | `uncalled bet returned while betting was open` |
| every runout pays (~510) | `every runout must award its share of the pot` |
| a check keeps the right to raise (~1083) | `illegal <action> to <total> by '<name>' facing …` |
| cross-run flop duplicates (~692) | `ParseError::DuplicateCard` (from `check_and_insert_card`) |
| `0 < small blind <= big blind` (~225) | `blinds must satisfy 0 < small blind <= big blind` |
| multi-run boards dealt to the river (~660) | `every multi-run board must be dealt to the river` |
| a pot has a winner (~474) | `pot has no winner` |

Make `assert_violation` take the expected detail (as `betting_error(..).contains(..)` already
does), and give each rule one near-miss test that fails when the rule is deleted (check it: delete
the rule, see the test fail, restore). **Authorized test edits:** change every existing
`assert_violation` call to pass its detail, and fix the two tests above so each mutation trips the
rule the test names.

`tests/ggpoker_parser.rs` (the `multi_run_cashout` case, "Spec s9", v3.7 line ~1798): its hand
has no settlement line, so the "unsettled cashout" check refuses it whatever the multi-run rule
does. **Authorized test edit:** settle the cashout (`53be55c0: Pays Cashout Risk ($x)` before
the showdowns and `, Cashout Risk ($x)` on its summary line, or the receiving form) so the
multi-run refusal is what fires, and assert its message. In
`src/parser/ggpoker.rs` (~1191), `|| multi` is redundant (a multi-run hand always has at least
two winner entries); remove it or comment why it stays.

### M16.2.6 Leniencies

- Card tokens: the GGPoker parser accepts any letter case (`card.rs` parses case-insensitively).
  Require the observed case (ranks `23456789TJQKA`, suits `cdhs`) in `ggpoker.rs`, leaving
  `Card::from_str` as it is. Refusal test: `Dealt to Hero [2d Jh]` → `Dealt to Hero [2D Jh]` in
  `HU_SHOWDOWN`.
- `RE_SHOWS` accepts any text after the cards (`(?: \(.*\))?`):
  `445e6868: shows [4c Tc] (a pair of Fours)` → `… (a pair of Fours) and collects $9 (bonus)`
  is accepted. Allow one parenthesised description without nested parentheses and nothing after
  it.
- Summary position labels may repeat (`(button) (button)`). Refuse a repeated label.
- Opponent `Dealt to` lines: `docs/invariants.md` §2 says they end with a trailing space, but
  `parse_hand` trims line ends, so the space is not checked. Do not enforce it; reword the
  invariant ("printed with a trailing space; the parser does not depend on it").

### M16.2.7 Robustness and wording

- `validate_hand` replays No-Limit betting for every hand. Match exhaustively on
  `hand.header.limit` (and `game_type`) so a new variant cannot compile without a decision.
- `src/domain/` doc comments quote GGPoker text (`canonical.rs` "GGPoker hashes", "Bingo", the
  `Pays Cashout Risk` / `Receives Cashout` lines; `money.rs`). Reword them site-neutrally
  (`AGENTS.md` §3.3; `SPEC_v3.6.md` is history).
- `docs/invariants.md` §1: add "a pot larger than zero has a winner"; say that straddles are the
  last forced posts (the order of the other posts is not checked); `validate_boards` reports
  `IncompleteStreet { line: 0 }` although lines are 1-based: use the hand's first line or document
  `0` as "no line".

**Done when:** gates green; every refusal test above added and seen failing before the fix; all
1,283 corpus hands and HD145319803 import exactly as before (goldens, the v3.5 row and
`compare_cache.py --equity` unchanged); the report lists the authorized test edits.

## 3. Owner decisions

| # | Decision | Recommended | Status |
| --- | --- | --- | --- |
| D1 | Aggregate `bb_won` across big blinds exactly (closes the open item of `HANDOFF_v3.7_review.md` §10.2) | Yes: today's code prints a wrong last digit for realistic stakes (1.5/3 with 3/6) | **Approved** (Nov 4) |
| D2 | A cashout chosen preflop | Refuse in the GGPoker parser | **Decided** (Nov 4): allowed, because other platforms offer it. The core already allows it and M16.2.3 pins that; the GGPoker parser refuses it, because GGPoker never prints one (`AGENTS.md` §3.6) |
| D3 | `[profile.dev] opt-level = 1` (`SPEC_v3.7.md` §10 decision 14): the gate's tests take about 8 s instead of 44–60 s (59 s on the owner's machine); an incremental build takes 1–3 s longer | Apply (review patch 4) | **Approved** (Nov 4): apply review patch 4 |
