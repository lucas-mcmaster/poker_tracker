---
trigger: model_decision
description: poker_tracker architecture (layers, domain model, storage, pipeline, CLI, stats and report contracts) and how to extend it. Read before designing, writing, reviewing or debugging any code in src/ or tests/, before adding a platform, currency, format, invariant or statistic, and whenever a gate fails.
---

# `poker_tracker` Architecture

How the system is built, its module and CLI contracts, and how to extend it. The rules
themselves are not repeated here:

- invariants, the GGPoker format rules and the cache and report rules: `docs/invariants.md`;
- statistic, position, money, session and equity definitions: `docs/stats.md`;
- scope, what is deferred and what is planned: `docs/known_limitations.md`;
- the work in progress: the current spec, `docs/SPEC_v<version>.md` (none between versions).

## 1. Scope

- **The product is an application with a user interface** over this crate's library API. The UI
  technology is not chosen yet (planned for v3.9). Until then:
  - every contract is stated as a Rust API first: public types and functions, with `serde`
    `Serialize` on the data a UI displays and `Deserialize` on what it sends (filters);
  - the library never prints, never reads stdin and never exits; errors are typed enums a UI can
    match on, and their `Display` is a message a person can read;
  - the CLI (`main.rs`) is a test harness that exposes each API once and adds nothing but
    argument parsing, exit codes and printing.
- **Built:**
  - hand-history ingestion and persistence; the first source is GGPoker (PokerCraft export),
    No-Limit Hold'em cash, account currency **CAD**;
  - from v3.7 (M13–M16), the derived-statistics layer: positions, stat counters, results and
    sessions, CSV reports, exact all-in equity;
  - from v3.8 (M17), filters, saved filters, typed report rows and the selection of any player
    (`docs/SPEC_v3.8.md` while it is the current spec).
- **Designed for:** more platforms and currencies, added as new site parsers without changing the
  domain, validator, storage, pipeline, stats or reports.
- **Out of scope until specified:** currency conversion, Omaha, other limits, tournaments, HUDs,
  live import (`docs/known_limitations.md` has the plan).

## 2. Layers

```
session file ──► ParserRegistry::detect ──► site parser (src/parser/<site>.rs)
                 (exactly one parser must recognize the file)
                                     │ CanonicalHand (site-neutral)
                                     ▼
                              validate_hand  (site-neutral invariants)
                                     ▼
                 SqliteRepository (per-hand savepoints; platform + currency are data)
                   │      ▲            └─► stats::derive_hand_stats ─► hand_player_stats (cache)
                   │      │                 (insert: same savepoint; open/rebuild: one transaction)
                   │   DirectoryIngester (walk *.txt sorted, detect, parse, batch, quarantine,
                   │                      stack continuity, report)
                   ▼
            report::write_report (integer SUMs → Decimal → CSV)
```

| Module | Responsibility | May know about sites? |
| :--- | :--- | :--- |
| `domain/money.rs` | `CurrencyCode`, micro-unit conversion, `Stakes` | No |
| `domain/card.rs` | 1-byte `Card` (`rank << 2 \| suit`), `Rank`, `Suit` | No |
| `domain/canonical.rs` | Canonical hand model, per-player results, stored identifiers | Only `Platform` variants |
| `parser/base.rs` | `ParseError`, `HandParser` trait, `HandBlock`, splitter, `validate_hand` | No |
| `parser/registry.rs` | `ParserRegistry`, `DetectionError` | No |
| `parser/ggpoker.rs` | GGPoker text format → canonical model | Yes (only here) |
| `storage/` | Identity, append-only migrations, batch insert, conflicts, sightings, quarantine, `load_hand`, the derived cache | No |
| `pipeline/` | Directory ingestion, `IngestionReport`, `HandFailure`, stack continuity | No |
| `stats/` | `STATS_VERSION`, positions, counters, money attribution, hand evaluator, all-in equity; pure functions of a `CanonicalHand`, depending only on `domain` (§8) | No |
| `report/` | Report rows and CSV formatting from integer aggregates (§8) | No |
| `main.rs` | CLI; registers site parsers | Registration only |

## 3. Money and currency

- In memory: `rust_decimal::Decimal`. In SQLite: `i64` micro-units, **millionths**
  (`MICRO_UNIT_SCALE = 1_000_000`).
- `decimal_to_micro_units` returns `None` (never rounds) for precision below `0.000001` or
  overflow. `micro_units_to_decimal` returns normalised values.
- `CurrencyCode([u8; 3])`: exactly 3 uppercase ASCII letters, valid by construction
  (`FromStr`, `from_bytes`, consts `CAD`/`USD`/`EUR`/`GBP`); serializes as a string. The CLI
  upper-cases its option before parsing.
- `Stakes { small_blind_micros: i64, big_blind_micros: i64 }` is a domain type (`domain::money`)
  implementing `FromStr` for `<sb>/<bb>` and `Display` in the report's `stakes` format;
  re-exported in `report`.
- Every `HandHeader` has a `currency`; every money column is in that hand's currency. Aggregate
  per currency (`GROUP BY hands.currency`); never sum across currencies.
- Where a currency comes from is a parser concern: read it from the file if the site prints it,
  otherwise require it as configuration. GGPoker prints `$` for CAD and USD accounts alike, so
  `GgPokerParser::new(currency)` requires it and the CLI requires `--ggpoker-currency`.
- Derived money (rake attribution, all-in equity shares) is defined in `docs/stats.md` §6,
  including its only rounding rules.

## 4. Domain model (`domain/canonical.rs`)

**Enums** (each has a stable `as_str()` stored in SQLite, `ALL`, and `FromStr` as its exact
inverse via `stored_identifier!`, which fails to compile if a variant is missing):

- `Platform { GGPoker }`, `GameType { Cash }`, `BettingLimit { NoLimit }`. `validate_betting`
  matches both of the last two exhaustively, so a new variant needs a decision there.
- `Street { Preflop=0, Flop=1, Turn=2, River=3, Showdown=4 }` with `code`/`from_code`; actions
  never use `Showdown`.
- `ActionType { PostSmallBlind, PostBigBlind, PostDeadBlind, PostStraddle, Fold, Check, Call,
  Bet, Raise, UncalledBetReturned }`.
  - `is_contribution()`: blinds (incl. dead), straddles, call, bet, raise.
  - `adds_to_street_commitment()`: live blinds, straddles, call, bet, raise (not dead blinds).
- `DeductionKind { Rake, Jackpot, Other }`: house money taken from the pot.
- `AdjustmentKind { EvCashoutRiskPaid, EvCashoutReceived }`: money between a player and the
  site outside the pot; `is_credit()` fixes the sign (risk paid is a debit, received a credit).

**Structs:**

- `HandHeader`: `platform`, `site_hand_id`, `game_type`, `limit`, `currency`, `small_blind`,
  `big_blind`, `timestamp: NaiveDateTime` (as printed), `utc_offset_seconds: Option<i32>`
  (`None` = unknown), `table_name`, `max_seats`, `button_seat`.
- `Player`: `seat`, `name`, `starting_stack`, `hole_cards: [Option<Card>; 2]` (slots fill in
  order), `is_hero`.
- `StreetAction`: `sequence`, `street`, `player_name`, `action_type`, `amount` (incremental),
  `street_total_after`, `is_all_in` (exactly when the contribution commits the whole stack).
- `CommunityCards`: `run_index` (`0` single board; `1..=MAX_RUNOUTS` (3) multi-run), `flop`,
  `turn`, `river`.
- `PotWinner { player_name, amount, run_index }`, `PotDeduction { kind, label, amount }`,
  `PlayerAdjustment { player_name, kind, street, amount (signed) }`.
- `HandOutcome { total_pot, deductions, uncalled_returned, winners, adjustments }`;
  `CanonicalHand { header, players, actions, boards, outcome }`.

**Derived results:** `checked_player_results()` → `PlayerResult { gross_committed, invested,
won, adjustments, net }` with `invested = gross − returned uncalled bets` and
`net = won − invested + adjustments`. All arithmetic is checked.

## 5. Parser contract

`HandParser` is object-safe and must stay so: `platform()`, `describe()` (e.g.
`"GGPoker (CAD)"`), `recognizes(raw)` (cheap sniff of the opening line(s)), `split_blocks(raw)`
(default: blank-line separation, BOM/CRLF, 1-based `start_line`), `parse_hand(block)` (must end
with `validate_hand`), `peek_hand_id(block)`, `parse_session(raw)`.

`ParserRegistry::detect(raw)` returns the single recognizing parser, or `Unrecognized` /
`Ambiguous`. Register one parser per *source* (site format + configuration such as currency).
`ParseError` line numbers are relative to the hand block and 1-based; `0` means no line.

The betting replay, side-pot bound and every other `validate_hand` rule, and the GGPoker format
rules, are in `docs/invariants.md` §1 and §2.

## 6. Storage (`storage/`)

- **Identity first:** `open` reads `application_id`, `user_version` and the schema in one read
  transaction before writing (only a busy lock is retried). Ours (`0x504B5452`) → migrate; empty
  file → create; id 0 + version 1 + `hands`/`import_runs` → v3.4, upgrade in place; anything
  else → `NotAPokerTrackerDatabase`, file untouched (v3.3 databases: delete and re-import).
  Newer version → `UnsupportedSchemaVersion`. Only then WAL, `synchronous=NORMAL`,
  `foreign_keys=ON`. `open_existing` never creates a file.
- **Migrations:** append-only `MIGRATIONS`; each runs under `BEGIN IMMEDIATE`, re-reading
  `user_version` first so concurrent opens never repeat a step. Migration 1 = v3.4 baseline;
  migration 2 (v3.5) rescaled money ×100 to micro-units and added raw text, sightings and
  quarantine; migration 3 (v3.7, M13) adds only `derived_cache`, which also makes older builds
  refuse a v3.7 database (`UnsupportedSchemaVersion`). `SCHEMA_VERSION` is `MIGRATIONS.len()`.
- **Kind columns** are non-empty text validated by the Rust enums; a new variant needs no
  migration.
- **Tables (all `STRICT`):** `players(platform, screen_name)`;
  `import_runs(id, started_at, tool_version)`; `hands` (header, `currency`, pot, aggregate
  `rake/jackpot/other_deductions_micros`, `uncalled_returned_micros`, storing run + file + line,
  `raw_text`, `raw_sha256`; `UNIQUE(platform, site_hand_id)`); `hand_players` (with
  `CHECK(net = won − invested + adjustments)`); `actions`; `board_cards`; `pot_winners`;
  `pot_deductions`; `player_adjustments`;
  `hand_sightings(hand_id, import_run_id, source_file, source_line)`;
  `quarantined_hands(run, raw_sha256, raw_text, parser, site_hand_id, stage, error, file,
  line)`; view `quarantine_pending` (quarantined texts not stored as a hand);
  `derived_cache(name, version, built_at)`; the cache table `hand_player_stats` (created by the
  rebuild, not by `MIGRATIONS`, with `idx_hand_player_stats_player(player_id, hand_id)`;
  `CHECK (<col> >= 0)` on every column of `COLUMN_SPECS` that is not `signed`).
- **Inserts:** one `BEGIN IMMEDIATE` transaction per batch, one savepoint per hand, 5 s busy
  timeout (`DEFAULT_BUSY_TIMEOUT`). Validation, conversion, conflict and constraint errors reject
  that hand (`BatchInsertStats::rejected`); lock, I/O and other database-wide errors return
  `Err` and nothing from the batch is stored. A stored hand's `hand_player_stats` rows are
  written in its savepoint.
- **Duplicates:** same `(platform, site_hand_id)` is a duplicate only with the same currency
  (else `CurrencyConflict`) and the same content (else `ContentConflict`): the SHA-256 of
  `normalized_hand_text` when both sides carry raw text, otherwise the canonical hand from
  `load_hand`. A duplicate records a sighting and fills in missing raw text.
- **Sources:** `insert_sourced_hands_batch(hands, sources)` takes a `HandSource {
  import_run_id, file_path, start_line, raw_text }` per hand; `insert_hands_batch(hands)` stores
  none. `insert_sourced_hands_batch_at` and `quarantine_hands_at` also take the run's
  `started_at: Option<DateTime<Utc>>` and register the run in the same transaction if it is new.
  `set_busy_timeout` changes the 5 s default. `register_import_run` is public but unused since
  M16.1.
- **Undo:** `delete_import_run(run)` → `DeletedImportRun { hands_deleted, hands_kept,
  quarantined_deleted }`: deletes hands this run stored that no other run contained, keeps the
  rest (re-pointing provenance to the earliest remaining run), drops the run's quarantine rows
  and the run; unknown run → `UnknownImportRun`. Hands stored without a source are never
  deleted (they count in `hands_kept`). Derived rows go with their hand (`ON DELETE CASCADE`).
- **Quarantine:** `quarantine_hands(&[QuarantinedHand])`, once per run and text.
- **Derived cache:** `rebuild_derived(&mut self) -> Result<DerivedRebuild, StorageError>`
  rebuilds unconditionally; `rebuilt_on_open(&self) -> Option<DerivedRebuild>` reports whether
  the open rebuilt; `DerivedRebuild { hands: usize, version: i64 }`. The rebuild-on-open, the
  version guard (`StaleDerivedCache`) and `DerivedRebuildFailed` are rules:
  `docs/invariants.md` §3.

## 7. Pipeline and CLI

- `DirectoryIngester::ingest_directory(root, &ParserRegistry, &mut repo)` (or
  `ingest_directory_at(…, started_at)`, which records the given start time): resolves `root` to an
  absolute path; walks `*.txt` (following symlinks) in sorted order; records unreadable,
  unrecognized, ambiguous files and "no files found" in `file_errors`; parses each block and
  flushes every 1,000 hands or rejections; registers the run, with the import's real start time,
  in the same transaction as its first write (a batch that fails leaves no run); quarantines
  every parse and storage rejection with its raw text.
- `HandFailure { file_path, parser, block_start_line, hand_id, stage }`, `file_line()`.
- `IngestionReport` adds `started_at`, `hands_quarantined`, `stack_warnings`, `is_complete()`.
- **Stack continuity** (`check_stack_continuity`): per file, consecutive hands at a table (by
  timestamp) must not shrink a player's stack: each player starts at least where the previous
  hand ended (`starting + net`). Violations are `StackWarning`s, not failures. Equal timestamps,
  or an unparsed block in between, skip the pair.
- A fatal storage error returns `PipelineError::Storage { source, partial_report }`.

**CLI** (`main.rs`; the commands and exit codes are in `AGENTS.md` §6):

- Arguments are read with `std::env::args_os()`. Paths (`--db`, the input directory, `--out`)
  stay `OsString`/`PathBuf`. An option whose value must be text (`--report`, `--stakes`,
  `--ggpoker-currency`, `--delete-run`) that is not valid Unicode is a usage error (`2`).
- `--help` exits `0`. Any option given twice is a usage error. A missing input directory is a
  usage error, checked before any database is opened.
- **Import:** `<dir> --ggpoker-currency CAD [--db path]`. It prints the ingestion summary with its
  Batch Run ID and exits `3` when files or hands were rejected or no `.txt` file was found.
- **Undo:** `--delete-run <RUN-ID> [--db path]`, with no input directory or site option (usage
  error `2`). Its message says that the kept hands include hands stored without a source.
- **`--rebuild-stats [--db path]`** opens with `open_existing` (never creates a file), calls
  `rebuild_derived` unless the open already rebuilt, prints
  `Rebuilt statistics for <N> hand(s) (stats version <V>)` and exits `0` (`1` on error).
  Combined with `--ggpoker-currency`, `--delete-run` or an input directory it is a usage error
  (`2`) whose message names what it was combined with.
- Whenever any other command opened a database whose cache was rebuilt with at least one hand,
  it first prints `Statistics cache rebuilt for <N> hand(s) (stats version <V>)` to **stderr**. A
  new, empty database prints nothing.
- **`--report <results|positions|sessions|stats> [--db path] [--stakes <sb>/<bb>[,…]] [--out
  path]`** opens with `open_existing` (it never creates a database file) and writes the CSV
  (`docs/invariants.md` §3 Reports) to `--out`, which it replaces without asking, or to stdout,
  where only CSV goes.
  - The kind is matched in any letter case.
  - Exit codes: `0` ok; `1` for a missing database, a stale cache, an I/O error or a figure that
    overflows; `2` for an unknown kind, or when combined with an import, a site option
    (`--ggpoker-currency`), `--delete-run` or `--rebuild-stats`.
  - `--stakes` restricts every report to the hero's hands at the listed stakes; without it every
    stake is included. A malformed or empty list, or `--stakes` without `--report`, is a usage error
    (`2`), and so is `--out` without `--report`. Each item that matches no hand prints `No hands at
    stakes <item>` (the item as `Stakes` displays it) to stderr; it is not an error.
  - `USAGE` and the module doc list every command and exit code.

## 8. Stats and reports

**`src/stats/`** (the hand evaluator is written in-crate, with no dependency):

- `pub const STATS_VERSION: i64`: `1` = M13, `2` = M14, `3` = M15, `4` = M16 (today).
- `pub fn derive_hand_stats(hand: &CanonicalHand) -> Vec<PlayerHandStats>`: one entry per player,
  in seat order. It is **infallible and never panics** for any `CanonicalHand` storage can load,
  including hands that would not pass `validate_hand` today (the v3.4 test database stores one
  with no players, which yields no rows). It uses checked or saturating integer arithmetic and
  `i128` for money products, treats a missing lookup as "no opportunity" (never an error), and
  fails closed on unrepresentable amounts (`docs/invariants.md` §3).
- `pub const COLUMN_SPECS: &[ColumnSpec]` lists every column's name, `ColumnKind` (`Flag`,
  `Count`, `Money`, `EquityNumerator`, `EquityDenominator`), and `signed` flag in schema order.
  `column_spec(name)` looks up a spec by name.
- `pub const COLUMNS: &[&str]` lists every counter and stored money/equity column of
  `docs/stats.md` in schema order: the names of `COLUMN_SPECS`, which one macro emits with it.
  The two are the single source for the `hand_player_stats` schema (`COLUMN_SPECS` for the
  `CREATE`, `COLUMNS` for the insert).
  `PlayerHandStats` exposes the player name, `position: Option<u8>` (codes of `stats.md` §2) and
  each column by name: `fn get(&self, column: &str) -> Option<i64>`.
- `pub fn exact_equity(hole_cards: &[[Card; 2]], board: &[Card]) -> Option<Vec<(u64, u64)>>`:
  each player's equity as a reduced fraction over every completion of `board`
  (`stats.md` §6.2). Returns `None` for fewer than two players, more than five board cards or a
  repeated card.
- `PositionLabel` (`PositionLabel::{Ep, Mp, Co, Btn, Sb, Bb, Unknown}`): report-time position
  labels with `PositionLabel::of(code: Option<u8>, max_seats: u8)`, `as_str()` (`"EP" … "BB"`, `"?"`),
  `ALL`, and serde. `pub fn position_label(code: u8, max_seats: u8) -> &'static str` delegates to it.
- **Performance** (release build): one heads-up preflop all-in (1,712,304 runouts) in under 1 s;
  a full `--rebuild-stats` of the corpus in under 10 s. The debug `cargo test --locked` stays
  under 60 s; `[profile.dev] opt-level = 1` keeps it well inside that.

**`src/report/`:**

- `pub fn build_report(repo: &SqliteRepository, kind: ReportKind, options: &ReportOptions) -> Result<Report, ReportError>`:
  builds a typed report in a single read transaction after the version guard.
- `Report { rows: ReportRows, unmatched_stakes: Vec<Stakes> }` (with `#[serde(flatten)] rows`), and
  `ReportRows::{Results(Vec<ResultsRow>), Positions(Vec<PositionsRow>), Sessions(Vec<SessionsRow>), Stats(Vec<StatsRow>)}`
  (tagged adjacently: `kind`, `rows`).
- Typed rows: `ResultsRow`, `PositionsRow`, `SessionsRow`, `StatsRow` with typed fields
  (`CurrencyCode`, `Option<Stakes>`, `PositionLabel`, `Platform`, `NaiveDateTime`, `Decimal`,
  `Option<Decimal>`).
- `pub fn write_csv(report: &Report, out: &mut impl Write) -> Result<(), ReportError>`: formats a
  typed report as CSV (text fields quoted per RFC 4180, lines ending in LF). It fails only with `Io`.
- `pub fn write_report(repo: &SqliteRepository, kind: ReportKind, options: &ReportOptions, out: &mut impl Write) -> Result<ReportOutcome, ReportError>`:
  thin wrapper calling `build_report` followed by `write_csv`.
- `PositionLabel` is re-exported from `report`.
- Every report query, and the unmatched-stakes probe, use one selection fragment
  (`report::selection::selection`) joining `hands h`, `hand_players hp` and `hand_player_stats hps`,
  with aliases `h`, `hp`, and `hps`. Its stakes condition is
  `(h.small_blind_micros, h.big_blind_micros) IN (VALUES (?, ?), …)` (`1` for no selection, `0`
  for an empty list), whose depth does not grow with the list.
- The stats report reads every name it uses from two constant tables, `STAT_TABLE` (stat,
  numerator column, `Denominator`) and `AGGRESSION_STREETS` (the AF and AFq columns per street),
  and a unit test checks each name is a `Flag` or `Count` column.
- `ReportKind::{Results, Positions, Sessions, Stats}` (with snake_case serde).
- `ReportOptions { stakes: Option<Vec<Stakes>> }`: `None` (the `Default`) means every stake. M17
  filters will subsume it; until then it is the only selection.
- `Stakes` is a domain type (`crate::domain::money::Stakes`), re-exported from `report`.
- `ReportOutcome { unmatched_stakes: Vec<Stakes> }`: the selected stakes that matched no hero
  hand, in the order given.
- `ReportError::{Storage(StorageError), Io(std::io::Error), ArithmeticOverflow { figure:
  &'static str }}`. Report totals are checked; an overflow or a stale cache
  (`Storage(StaleDerivedCache { .. })`) writes nothing to `out`.

## 9. Extending

**New platform or source:**

1. Collect real exports as fixtures.
2. Add a `Platform` variant (and list it in `stored_identifier!`).
3. Write `src/parser/<site>.rs` implementing `HandParser`. It needs:
   - sniffing that cannot match other sites;
   - its own `split_blocks` if needed;
   - the site's vocabulary mapped onto `DeductionKind` / `AdjustmentKind`, keeping the original
     labels;
   - the currency from the file or the constructor;
   - `utc_offset_seconds` if the site states it;
   - `is_all_in` on every whole-stack contribution;
   - the site's observed line order and grammar, enforced.
4. Re-export it from `parser/mod.rs` and add a CLI option.
5. Tests:
   - per-hand assertions;
   - whole-session reconciliation (conservation and stack continuity, with no stack warnings on
     import);
   - fault injection, including single-line mutations;
   - cross-site non-recognition.

   Statistics need no change: they read only the canonical model.

**New canonical concept:** a site-neutral enum variant with a new, stable `as_str()`; no schema
change. A new column or table is a new entry appended to `MIGRATIONS`. Never add a
site-specific field to the canonical model.

**New format on an existing site:** add the real hand verbatim to `tests/common/mod.rs`, write a
failing test first, then extend the parser. Hands quarantined before the change are stored by
the next import of the same folder.

**New statistic** (specified by the owner in a spec, since `docs/stats.md` and `tools/` are
owner-controlled):

1. Define its counters (numerator and opportunity) in `docs/stats.md`, in canonical terms.
2. Add them to `stats::COLUMNS` and compute them in `stats::derive_hand_stats`; bump
   `STATS_VERSION` (the cache rebuilds on the next open; no migration).
3. Implement the same definition in `tools/oracles/v3_7_oracle.py` and take the expected numbers
   from it.
4. Tests: worked examples on real fixtures, canonical near-miss tests, whole-folder totals.

**Data and oracles.** The corpus is 1,283 real hands, which make 6,840 player-hands: 989 in the
10 v3.6 exports and 294 in the v3.5 export. `EV_CASHOUT_RECEIVED` (HD145319803) exists only in
`tests/common/mod.rs`. "Hero" is the player with `is_hero` in each hand.

`tools/oracles/v3_7_oracle.py` is an independent Python implementation of `docs/stats.md`. It reads
a `poker_tracker` database, shares no code with the crate, and uses the standard library only, plus
`eval7` for equity. Every pinned number came from it; to reproduce them, follow
`tools/oracles/README.md`. The pinned numbers live in the tests, in `tests/golden/`, and in
`docs/history/SPEC_v3.7.md` §4–§7.
