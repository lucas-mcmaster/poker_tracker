# poker_tracker — v3.8 spec (M17: filters)

Nov 4, 2026 · @Lucas · from `PLAN_v3.8.md` revision 2 and its review (owner decisions in §13)

## 0. Agent brief

This spec takes `poker_tracker` from v3.7.1 to v3.8 in six milestones, **M17.1 to M17.6**. The
starting point is `50adfe8` plus the reviewer's preparation commits (§0.1): **242 tests** on
Linux, 241 on Windows.

**The product is an application with a user interface; nobody will use the CLI once it exists**
(`AGENTS.md` preamble, ARCHITECTURE §1). So every contract below is a Rust API first. The CLI
only exposes each API once, so that the owner and the tests can drive it before a UI exists.

| Milestone | Delivers | Visible change | Code it may change |
| --- | --- | --- | --- |
| M17.1 | Column specs; one shared selection for every report query; checked report sums; `Stakes` moves to `domain` | **None** | `src/stats/mod.rs` (the column macro only), `src/storage/repository.rs` (`create_hand_player_stats_sql` only), `src/domain/money.rs` and `src/domain/mod.rs` (`Stakes` only), `src/report/` |
| M17.2 | Typed report rows (`build_report`, `Report`), CSV written from them, `PositionLabel` | **None** | `src/report/`, `src/stats/mod.rs` (`PositionLabel` only) |
| M17.3 | `src/filter/`: the filter tree, its JSON document, validation, the SQL compiler, the catalog; game and counter filters; `--filter-file`, `--filter-catalog` | New | `src/filter/` (new), `src/lib.rs`, `src/report/`, `src/main.rs` |
| M17.4 | Hand filters: position, result, pot size, went all-in, hole cards | New | `src/filter/` |
| M17.5 | Saved filters: migration 4, repository API, `--filter`, `--save-filter`, `--list-filters`, `--delete-filter` | New | `src/storage/`, `src/filter/`, `src/main.rs` |
| M17.6 | Any player: `PlayerSelection`, `list_players`, `filter_values`; `--player`, `--platform`, `--list-players`, `--filter-values` | New | `src/report/`, `src/storage/`, `src/main.rs` |

### 0.1 Before you start

Check that `master` has these commits, newest last. If any is missing, stop and tell the owner.

1. `docs: condense the agent docs; finished specs and the blueprint to docs/history (gate: 242 tests passing)`
2. `doc changes`
3. `docs: v3.8 scope, the UI is the product; card encoding and filter vocabulary are stored identifiers (gate: 242 tests passing)`
4. `build: serde_json becomes a dependency, owner decision D11 (gate: 242 tests passing)`
5. `docs: stats.md §9 filter definitions, owner decision D12 (gate: 242 tests passing)`
6. `tools: v3_8_oracle.py and db_compare.py; filter fixtures and the v3.8 golden cases (gate: 242 tests passing)`
7. `docs: SPEC_v3.8.md (gate: 242 tests passing)`

### 0.2 How to work through it

- **Reading.**
  - Read `AGENTS.md`, then this spec in full.
  - Then read, for each milestone, the sections it names: `docs/stats.md` §1, §2, §4, §6.1 and §9; `docs/invariants.md` §3; ARCHITECTURE §1, §2 and §6–§8.
  - Do not read `docs/history/`, `src/parser/` or the fixture texts.
  - Find hands by grepping `pub const <NAME>` in `tests/common/mod.rs`. You may read the filter documents in `tests/fixtures/filters/` and the case files `tests/golden/v3_8/*/case.json`: they are inputs.
- **Order.** Do the milestones one at a time, in order. After each one: commit, report and wait. The commit title is `milestone 17.<n>: <summary> (gate: <count> tests passing)`.
- **Editing existing tests.** Only the edits this spec lists are authorized: one in M17.2 (a unit test in `src/report/sessions.rs`) and one in M17.3 (`tests/report.rs::stakes_options`). Every other existing test stays unchanged and must pass. List each edit you make in the report.
- **Existing pinned numbers do not change.**
  - These stay byte for byte: the four `tests/golden/v3_6_*.csv`, the v3.5 `results` row, every pinned table of `docs/history/SPEC_v3.7.md`, and `tools/oracles/compare_cache.py`'s verdict.
  - `STATS_VERSION` stays **4** throughout. No stored column changes, and filters are computed at report time (`AGENTS.md` §3.10).
  - The text of the `hand_player_stats` schema stays byte for byte (M17.1.2).
- **New pinned outputs.**
  - The reviewer wrote `tests/golden/v3_8/<case>/{results,positions,sessions,stats}.csv` with the independent oracle (`tools/oracles/v3_8_oracle.py`).
  - Never regenerate or edit them.
  - If the program disagrees with one, find which side departs from `docs/stats.md` §9 or this spec, and report it.
- **Dependencies.** `serde_json` is now in `[dependencies]` (commit 4). Change nothing else in `Cargo.toml` or `Cargo.lock`.
- **Gate.** As in `AGENTS.md` §4. The passed count rises at every milestone.
- **Report.**
  - The gate block, the test count, **Deviations** (or "none"), the timings asked for, and `git show --stat HEAD`.
  - Write it from the final commit. Name tests as the code names them, and check each name with `cargo test --locked -- --list`.
  - Keep a note in `target/agent-notes/M17.<n>.md`.

## 1. Ground rules for v3.8

1. **The library API is the contract** (ARCHITECTURE §1).
   - The library never prints, reads stdin or exits.
   - New public data types derive `serde::Serialize` where this spec says so, and `Deserialize` only for the filter.
   - Errors are typed `thiserror` enums a UI can match on. Their `Display` is a readable sentence that names the offending value.
2. **SQL.**
   - **No SQL from text** (`AGENTS.md` §3.11). Every value is bound. Identifiers come only from `stats::COLUMN_SPECS` (M17.1), `hps.position`, or the fixed `hands`, `hand_players` and `actions` columns of §5.4.
   - **No division in SQL**, and **no integer product that can overflow** (`AGENTS.md` §3.1). SQLite turns an overflowing product into a `REAL`: `2000000 * 9000000000000` has `typeof` `real`.
   - **Every filter compiles to an expression that is `0` or `1`, never `NULL`** (§5.4).
3. **JSON never carries a floating-point number.**
   - `rust_decimal` serializes as a string (`"-25.79"`) and integers as integers.
   - A rounded figure is a `Decimal` with scale 2 that is **never negative zero**. `-Decimal::new(0, 2)` exists and displays `-0.00`, so normalise it.
4. **One query per report.**
   - A report runs one query, plus one existence probe per `--stakes` item, as today, and from M17.6 one lookup of the selected player's id.
   - A filter adds conditions to that query, never a query per hand.
   - `went_all_in` is an `EXISTS` subquery served by an index on `actions(hand_id, …)`. SQLite picks the `UNIQUE(hand_id, sequence)` autoindex; do not assert an index name.

## 2. Architecture after v3.8

```
UI (v3.9+) / CLI harness (main.rs)
   │  ReportOptions { stakes, filter: Option<Filter>, player: PlayerSelection }
   ▼
report::build_report ──► Selection (one FROM/JOIN/WHERE fragment, §4)
   │                        ├─ player predicate      (hero, or one player id; M17.6)
   │                        ├─ stakes predicate      (ReportOptions::stakes, as today)
   │                        └─ filter::compile(…)    (the filter tree as SQL; M17.3, M17.4)
   ▼
Report { rows: ReportRows, unmatched_stakes }  ──► write_csv (CLI) / serde (UI)

filter (pure: tree, JSON, validation, catalog, SQL) ◄── storage (saved_filters, M17.5)
```

| Module | Responsibility (new in v3.8) | Depends on |
| --- | --- | --- |
| `stats` | `COLUMN_SPECS` / `ColumnKind` (M17.1); `PositionLabel` (M17.2) | `domain` |
| `filter` | `Filter`, `FilterError`, the document format, `validate`, `catalog`, and a `pub(crate)` compiler to SQL over the selection's aliases | `domain`, `stats` |
| `report` | `Selection`; `build_report`, `Report`, typed rows, `write_csv`; `PlayerSelection`, `filter_values` | `domain`, `stats`, `storage`, `filter` |
| `storage` | `saved_filters` (migration 4) and its API; `list_players` | `domain`, `stats`, `filter` |

`filter` never depends on `storage` or `report`.

## 3. Data, oracle and golden cases

**The database.**
- Every case runs on the **989 hands of `tests/fixtures/ggpoker/v3_6/`**, the database `golden_v3_6_reports_match_byte_for_byte` builds. Build it the same way: `insert_hands_batch`, no sources.
- In that folder:
  - all hands are CAD at 0.05/0.1, at 6-max tables, on GGPoker;
  - the dates are 2026-08-21 (seven files, 736 hands) and 2026-09-25 (three files, 253 hands);
  - there are 102 players, the hero (`Hero`) and 101 opponent hashes.
- So stakes, table size, currency and platform filters are checked here only as conditions that match everything. Their refusals and their real selections are tested on canonical hands built in the tests.

**The oracle** (`tools/oracles/README.md`).
- `v3_8_oracle.py` evaluates filters in Python on its own per-player-hand values, never on `hand_player_stats`, and writes the four reports.
- With no filter it reproduces the four `v3_6_*.csv` goldens byte for byte (checked on Nov 4).

**A case** is a directory `tests/golden/v3_8/<case>/` holding `case.json` and the four CSVs. `case.json` is:

```json
{
  "milestone": "M17.3",
  "filter": "tests/fixtures/filters/three_bet_opp.json",
  "player": null,
  "stakes": ["0.05/0.1", "1/2"],
  "unmatched_stakes": ["1/2"]
}
```

- `filter` is a path from the repository root, or `null`.
- `player` is `"<platform>/<screen name>"`, split at the first `/`, or `null` (the hero).
- `stakes` is `ReportOptions::stakes` as `Stakes::from_str` strings, or `null`.
- `unmatched_stakes` is the expected `ReportOutcome::unmatched_stakes`, as `Display` strings.

| Case | Milestone | Filter (`tests/fixtures/filters/…`) | Player-hands | Sessions |
| --- | --- | --- | --- | --- |
| `m17_3_date_aug21` | M17.3 | `date_aug21` | 736 | 7 |
| `m17_3_not_date_aug21` | M17.3 | `not_date_aug21` | 253 | 3 |
| `m17_3_three_bet_opp` | M17.3 | `three_bet_opp` | 299 | 10 |
| `m17_3_vpip_or_walk` | M17.3 | `vpip_or_walk` | 293 | 10 |
| `m17_3_saw_flop_no_showdown` | M17.3 | `saw_flop_no_showdown` | 143 | 10 |
| `m17_3_preflop_raises_2` | M17.3 | `preflop_raises_2` | 4 | 4 |
| `m17_3_two_tables` | M17.3 | `two_tables` | 198 | 3 |
| `m17_3_no_match` | M17.3 | `no_match` | 0 (header lines only) | 0 |
| `m17_3_stakes_and_filter` | M17.3 | `three_bet_opp`, with `stakes` `0.05/0.1,1/2` | 299 (`1/2` unmatched) | 10 |
| `m17_4_late_position` | M17.4 | `late_position` | 363 | 10 |
| `m17_4_not_blinds` | M17.4 | `not_blinds` | 598 | 10 |
| `m17_4_net_won` | M17.4 | `net_won` | 191 | 10 |
| `m17_4_net_zero` | M17.4 | `net_zero` | 447 | 10 |
| `m17_4_lost_big_pots` | M17.4 | `lost_big_pots` | 75 | 10 |
| `m17_4_pot_10_to_30` | M17.4 | `pot_10_to_30` | 172 | 10 |
| `m17_4_went_all_in` | M17.4 | `went_all_in` | 17 | 11 |
| `m17_4_ev_qualified_not_all_in` | M17.4 | `ev_qualified_not_all_in` | 4 | 4 |
| `m17_4_premiums` | M17.4 | `premiums` | 33 | 11 |
| `m17_4_suited_late` | M17.4 | `suited_late` | 14 | 9 |
| `m17_6_opponent` | M17.6 | none; player `GGPoker/c7bdd1e4` | 182 | 1 |
| `m17_6_opponent_known_cards` | M17.6 | `known_hole_cards`; same player | 5 | 2 |
| `m17_6_opponent_unknown_cards` | M17.6 | `unknown_hole_cards`; same player | 177 | 1 |
| `m17_6_hero_by_name` | M17.6 | none; player `GGPoker/Hero` | 989 (equal to `v3_6_*.csv`) | 10 |

Three cases check rules, not just numbers:

- **Sessions.** `m17_4_premiums` and `m17_4_went_all_in` have **more** sessions than the unfiltered 10. Sessions are formed from the selected hands (`docs/stats.md` §9), so a gap of more than 30 minutes between two selected hands starts a new session.
- **Stakes and filter.** `m17_3_stakes_and_filter` has the same CSVs as `m17_3_three_bet_opp`: the stakes selection and the filter are combined with AND.
- **Unknown cards.** `m17_6_opponent_unknown_cards` plus `m17_6_opponent_known_cards` give all 182 of the opponent's hands. The opponent showed cards in 5, so "not known cards" must include the 177 hands whose cards are unknown.

## 4. M17.1 — Column specs and one selection (no visible change)

**Why.** Today:
- `hand_player_stats` columns are a bare name list.
- Storage decides which column is signed with the literal `if *col == "ev_net_micros"` (`repository.rs:530`).
- The stats report lists 61 `SUM`s by hand and reads them back by index (`report/stats.rs:107–250`), so a column inserted in the middle silently shifts every later stat.
- Five queries repeat the hero join with the aliases `h`/`hp`/`hps`, which nothing ties to `stakes_clause`.
- Four files add report totals with `saturating_add`, which contradicts `invariants.md` §3 "Exact rates".

Filters need column metadata and one place to put a predicate.

### M17.1.1 `ColumnSpec`

```rust
// src/stats/mod.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnKind { Flag, Count, Money, EquityNumerator, EquityDenominator }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ColumnSpec { pub name: &'static str, pub kind: ColumnKind, pub signed: bool }

pub const COLUMN_SPECS: &[ColumnSpec];          // schema order
pub fn column_spec(name: &str) -> Option<&'static ColumnSpec>;
```

- `define_stats_columns!` takes each column's kind (and signedness), and emits `COLUMN_SPECS` alongside what it emits today.
- `COLUMNS` stays, with the same names in the same order.
- **The kind of every column is the contract in Appendix A:**
  - 49 `Flag`;
  - 20 `Count`;
  - 5 `Money`, of which only `ev_net_micros` is `signed`;
  - `ev_equity_num` is `EquityNumerator` and `ev_equity_den` is `EquityDenominator`.

### M17.1.2 Storage reads the specs

- `create_hand_player_stats_sql` emits `CHECK (<col> >= 0)` exactly for the columns that are not `signed`. The string literal goes.
- The text SQLite stores must not change by one byte. Databases on disk keep their statement until the next rebuild, so old and new must be identical.
- A new test (`tests/storage.rs`) pins it. On a new database, the SHA-256 of `SELECT sql FROM sqlite_master WHERE name = ?` is:
  - `hand_player_stats` (4,242 bytes): `d15e06c9474da75be98319f2091430b7e207f3f26bf792e4d8cf456f80a0d6fb`;
  - `idx_hand_player_stats_player`: `b909b540d7db74d79c2d46ea9c999058e2e6ddbcc321b4178aafe406a79501e2`.

  The reviewer computed both from a database made by `50adfe8`.

### M17.1.3 The stats report reads columns by name

- The stats query sums **every `Flag` and `Count` column** of `COLUMN_SPECS`, by name.
- The row is read into a map from column name to sum.
- The stat table is a constant list of `(stat, numerator column, denominator column)` names, in `docs/stats.md` §5 order. AF and AFq are computed from the named action counts.
- `CurrencyStats` and the indexed `row.get(k)` reads go.
- A unit test checks that every name the stat table uses is a `Flag` or `Count` column of `column_spec`. A misspelled name then fails that test instead of printing 0.

### M17.1.4 One selection

Every report query, and the unmatched-stakes probe, use one fragment built in one place (for example `report::selection`):

```sql
FROM hands h
JOIN hand_players hp ON hp.hand_id = h.id
JOIN hand_player_stats hps ON hps.hand_id = hp.hand_id AND hps.player_id = hp.player_id
WHERE (<player>) AND (<stakes>) AND (<filter>)
```

- `<player>` is `hp.is_hero = 1` until M17.6.

- `<stakes>` is today's stakes condition, `(h.small_blind_micros = ? AND h.big_blind_micros = ?) OR …`.
  - With no stakes selection it is `1`.
  - With an empty list it is `0` (today's `AND 1 = 0`).
- `<filter>` is `1` until M17.3.
- From M17.6, `<player>` is `hp.player_id = ?` when another player is selected.
- The aliases `h`, `hp` and `hps` are part of this contract: the filter compiler (§5.4) emits them.
- Moving `is_hero` from the `ON` clause to the `WHERE` clause is equivalent for inner joins.
- The probe now joins `hand_player_stats` too. That is equivalent, because every `hand_players` row has its cache row (`invariants.md` §3 "Shape").
- No other query in `src/report/` names `hands`, `hand_players` or `hand_player_stats` in a `FROM` or `JOIN`.

### M17.1.5 Report totals are checked

- Replace every `saturating_add` on a report total with checked addition. An overflow is `ReportError::ArithmeticOverflow { figure }` and writes nothing.
- The sites:
  - `results.rs` (the `ALL` row);
  - `positions.rs` (merging the `(max_seats, big_blind)` groups into one label);
  - `sessions.rs` (session net and EV net);
  - `stats.rs` (the AF and AFq sums).
- `figure` is the CSV column the total feeds (`"hands"`, `"net"`, `"rake"`, `"fees"`, `"blinds"`, `"straddles"`, `"allin_hands"`, `"ev_net"`, `"vpip"`, `"pfr"`, `"rfi"`), or, in `stats.rs`, the stat (`"af_pf"`, `"afq_total"`, …).
- SQLite's own `SUM` already fails on overflow, as today.

### M17.1.6 `Stakes` moves to `domain`

- `filter` must parse and compare stakes (§5.3), and `filter` may not depend on `report` (§2).
- So `Stakes` (with `FromStr`, `Display` and its plain-decimal check) moves to `src/domain/money.rs`, unchanged.
- `report` re-exports it (`pub use crate::domain::money::Stakes`), so `poker_tracker::report::Stakes` keeps working and no test changes.

**Tests** (new; the four overflow tests fail today):

1. Unit tests in `src/report/`, one per site, on the aggregation helpers. Factor them out so that they can be called with plain values:
   - two stakes groups with `net_micros` `i64::MAX` and `1` → `ArithmeticOverflow { figure: "net" }`;
   - two groups that merge into one label with `hands` `i64::MAX` and `1` → `figure: "hands"`;
   - two session hands with net `i64::MAX` and `1` → `figure: "net"`;
   - `p_bets = i64::MAX`, `p_raises = 1` → `figure: "af_pf"`.
2. `COLUMNS` equals the names of `COLUMN_SPECS`, in order.
3. The kinds and signedness equal Appendix A (one test, spelled out from the appendix, not generated from the code).
4. The schema hashes of M17.1.2.
5. The stat-table name check of M17.1.3.

**Docs.**
- ARCHITECTURE §6 "Derived cache": the `CHECK`s come from `COLUMN_SPECS`.
- ARCHITECTURE §8: `COLUMN_SPECS`, and the selection fragment with its aliases.
- ARCHITECTURE §3: `Stakes` is a domain type.
- `invariants.md` §3 "Reading": one selection.
- One CHANGELOG line under a new `## v3.7.1 → v3.8` heading.

**Done when.**
- The gate passes and the existing golden and v3.5 tests pass unchanged.
- A release import of `tests/fixtures/ggpoker` gives the same four `--report` outputs as before. Run `cmp` against the goldens for the v3.6 folder.
- The reviewer also runs `tools/oracles/db_compare.py` against `50adfe8`'s binary (rows **and** `sqlite_master.sql`), plus a mutation sweep of the diff.

## 5. The filter document (the contract M17.3–M17.5 share)

Saved filters (M17.5) store this document, and a UI writes it, so it is a stored identifier (`AGENTS.md` §3.7). Its node types, field names, operators and labels can be added to, never renamed. A change that would make this build misread a saved definition bumps `FILTER_FORMAT_VERSION`.

### 5.1 The document

```json
{"format_version": 1, "filter": <node>}
```

- The document is read in this order:
  1. It must be a JSON object with an integer `format_version`; otherwise it is `Malformed`.
  2. A `format_version` other than `1` is `UnsupportedFormatVersion { found }`. This comes before anything else is read, because a later version may add fields: `{"format_version":2,"filter":{"type":"bogus"},"note":1}` is `UnsupportedFormatVersion { found: 2 }`.
  3. Unknown top-level fields, and a `filter` that is not a valid node, are `Malformed`.
  4. Then validation (§5.3).
- `pub const FILTER_FORMAT_VERSION: u32 = 1;`

### 5.2 Nodes

Every node is a JSON object with a `"type"`. Unknown types and unknown fields are `Malformed`. Field order in input is free.

| `type` | Fields | Group | From |
| --- | --- | --- | --- |
| `and` | `children`: array of nodes | logic | M17.3 |
| `or` | `children`: array of nodes | logic | M17.3 |
| `not` | `child`: node | logic | M17.3 |
| `currency` | `any_of`: array of currency codes (`"CAD"`, exactly 3 uppercase letters) | game | M17.3 |
| `stakes` | `any_of`: array of `"<sb>/<bb>"` strings (the `Stakes::from_str` grammar) | game | M17.3 |
| `platform` | `any_of`: array of `Platform::as_str` values (`"GGPoker"`) | game | M17.3 |
| `table_size` | `min`, `max`: integers, both required, `2 ≤ min ≤ max ≤ 10` | game | M17.3 |
| `table_name` | `any_of`: array of non-empty strings | game | M17.3 |
| `date` | `from`, `to`: optional `"YYYY-MM-DD"` strings, at least one | game | M17.3 |
| `counter` | `column`: a `Flag` or `Count` column; `op`: `"="`, `"!="`, `"<"`, `"<="`, `">"`, `">="`; `value`: integer, `0 … 1` for a `Flag`, `0 … 1000` for a `Count` | counter | M17.3 |
| `position` | `any_of`: array of `"EP"`, `"MP"`, `"CO"`, `"BTN"`, `"SB"`, `"BB"`, `"?"` | hand | M17.4 |
| `result` | `is`: `"net_won"`, `"net_lost"` or `"net_zero"` | hand | M17.4 |
| `pot_size` | `min_big_blinds`, `max_big_blinds`: optional integers `1 … 10000`, at least one, `min ≤ max` | hand | M17.4 |
| `went_all_in` | none | hand | M17.4 |
| `hole_cards` | `any_of`: array of classes (`"AA"`, `"AKs"`, `"AKo"`; §5.3) | hand | M17.4 |

What each one means is `docs/stats.md` §9, which outranks this table on meaning. "Won the pot", "went to showdown" and "EV-qualified" are counters (`won_hand`, `sd_went`, `ev_qualified`). They need no node of their own; a UI can offer them as shortcuts that build a `counter` node.

**The Rust type** (public API; JSON through serde derive):

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum Filter {
    And { children: Vec<Filter> },
    Or { children: Vec<Filter> },
    Not { child: Box<Filter> },
    Currency { any_of: Vec<String> },
    Stakes { any_of: Vec<String> },
    Platform { any_of: Vec<String> },
    TableSize { min: i64, max: i64 },
    TableName { any_of: Vec<String> },
    Date {
        #[serde(default, skip_serializing_if = "Option::is_none")] from: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")] to: Option<String>,
    },
    Counter { column: String, op: CompareOp, value: i64 },
    // M17.4
    Position { any_of: Vec<PositionLabel> },
    Result { is: ResultKind },
    PotSize {
        #[serde(default, skip_serializing_if = "Option::is_none")] min_big_blinds: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")] max_big_blinds: Option<i64>,
    },
    WentAllIn {},
    HoleCards { any_of: Vec<String> },
}
pub enum CompareOp { Eq, Ne, Lt, Le, Gt, Ge }   // serde: "=", "!=", "<", "<=", ">", ">="
pub enum ResultKind { NetWon, NetLost, NetZero } // serde: snake_case
```

- **Data values are strings**: currencies, stakes, platforms, dates and classes. They are validated after deserialization, so their errors carry a path (§5.3). Closed vocabularies that belong to the format (`type`, `op`, `is`, position labels) fail as `Malformed`.
- **`WentAllIn` is an empty struct variant `{}`, not a unit variant.** With `deny_unknown_fields`, serde ignores unknown fields on a unit variant of an internally tagged enum: `{"type":"went_all_in","x":1}` is accepted. The empty struct variant refuses it (checked by the reviewer with serde 1.0 / serde_json 1.0.154).
- The variant names and field names above are fixed. Derive order and attributes beyond these are yours.

### 5.3 Validation (`Filter::validate`, `filter::parse_document`)

```rust
pub fn parse_document(json: &str) -> Result<Filter, FilterError>; // parse, check the version, validate
pub fn to_document(filter: &Filter) -> Result<String, FilterError>; // canonical document (§5.5)
impl Filter { pub fn validate(&self) -> Result<(), FilterError>; }

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize)]
pub enum FilterError {
    Malformed { message: String },                     // not JSON, or not this format (serde's message)
    UnsupportedFormatVersion { found: i64 },
    Invalid { path: Vec<usize>, problem: FilterProblem },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum FilterProblem {
    EmptyGroup, EmptyList, EmptyValue, DuplicateValue { value: String },
    InvalidCurrency { value: String }, InvalidPlatform { value: String },
    InvalidStakes { value: String, reason: String }, InvalidDate { value: String },
    EmptyRange, InvertedRange,
    OutOfRange { field: &'static str, value: i64, min: i64, max: i64 },
    UnknownColumn { column: String }, NotACounter { column: String },
    InvalidHoleClass { value: String },
    TooDeep { max: usize }, TooManyNodes { max: usize }, TooManyValues { max: usize },
}
```

- **Path.**
  - The child indexes from the root: `[]` is the root, `[0, 2]` the third child of the first child.
  - The child of a `not` is index `0`.
- `to_document` returns `Result` because `serde_json::to_string` does; it cannot fail for a `Filter`, and `src/` may not `expect` (`AGENTS.md` §3.2). A failure would be `Malformed`.
- Both error enums derive `Serialize` with serde's default representation, so a UI can show the path and the problem.
- **The first problem is returned.**
  - First the three limits below, in the order depth, nodes, values.
  - Then the other rules, visiting nodes in pre-order (a node before its children, children in order).
  - Within a node, fields are checked in table order, and list entries in order.
- **Limits** (the catalog states them, M17.3.4):
  - depth at most **16**, the root being depth 1: `TooDeep` at the first node deeper;
  - at most **256** nodes: `TooManyNodes`, path `[]`;
  - at most **1,000** list entries over the whole tree: `TooManyValues`, path `[]`.
- **Rules:**
  - `EmptyGroup`: `and`/`or` with no child.
  - `EmptyList`: an empty `any_of`.
  - `EmptyValue`: an empty string in `any_of` or in `column`.
  - `DuplicateValue`: an entry equal to an earlier one of the same list. Stakes are compared numerically, so `0.05/0.1` and `0.05/0.10` are duplicates; every other value is compared exactly.
  - Currency: `CurrencyCode::from_str` without upper-casing, so `"cad"` is `InvalidCurrency`.
  - Platform: `Platform::from_str`.
  - Stakes: `Stakes::from_str`; its message is the `reason`.
  - Date:
    - exactly `YYYY-MM-DD`, four-digit year, a real calendar date (`2026-02-30` is invalid);
    - `to` must have a next day (`9999-12-31` is invalid);
    - neither bound is `EmptyRange`; `from > to` is `InvertedRange`.
  - Table size: out of bounds is `OutOfRange` (`field` `"min"` or `"max"`); `min > max` is `InvertedRange`.
  - Pot size: out of bounds is `OutOfRange` (`field` `"min_big_blinds"` or `"max_big_blinds"`); `min > max` is `InvertedRange`; neither bound is `EmptyRange`.
  - Counter:
    - an unknown `column` is `UnknownColumn`;
    - a `Money` or `Equity*` column is `NotACounter`;
    - a `value` out of its kind's range is `OutOfRange { field: "value" }`.
  - Hole class:
    - exactly `RR` (a pair), or `HLs` / `HLo`;
    - ranks from `23456789TJQKA` in upper case, higher rank first, suffix lower case;
    - anything else is `InvalidHoleClass`: `"KAs"`, `"AAs"`, `"AK"`, `"aks"`, `"AKS"`.
- **`Display`.**
  - `Invalid` reads `filter node [0, 2]: <problem>`, where the problem names the value (`unknown column 'p_3bets'`).
  - `Malformed` reads `invalid filter document: <serde message>`.
  - `UnsupportedFormatVersion` reads `filter format version <n> is not supported (this build reads version 1)`.
  - Every other `Display` text, including the new `StorageError` and `ReportError` variants, is yours, as long as it names the offending value. Tests assert variants and fields, not wording. The CLI tests only check that stderr names the file or value.

### 5.4 Compilation to SQL (normative)

Each node compiles to a parenthesised SQL expression over the aliases of §4, with `?` placeholders bound in order. Whitespace is free, and so is extra parenthesisation. The semantics, and the absence of any product that can overflow, are not.

- **Two-valued.** Every node compiles to an expression that is `0` or `1`, never `NULL`.
  - The leaves below guarantee it: every nullable column is tested with `IS NOT NULL` first, and SQLite's `AND` is `0` when its left side is `0`.
  - So `not` is plain `(NOT e)`, and data that is missing makes a leaf false and its negation true (`docs/stats.md` §9).
- **Precedence.** SQLite binds `& | << >>` looser than `+ -` and tighter than comparisons, so the bit expressions below are parenthesised as written. Do not remove those parentheses.

| Node | SQL |
| --- | --- |
| `and` / `or` | `(e1 AND e2 …)` / `(e1 OR e2 …)` |
| `not` | `(NOT e)` |
| `currency` | `(h.currency IN (?, …))` |
| `stakes` | `((h.small_blind_micros, h.big_blind_micros) IN (VALUES (?, ?), …))`. Not an `OR` chain: SQLite refuses an expression deeper than 1,000, and an `OR` of 995 or more pairs exceeds it, although the document allows 1,000 values. Row values need SQLite 3.15; the crate bundles 3.46. |
| `platform` | `(h.platform IN (?, …))` with `Platform::as_str` |
| `table_size` | `(h.max_seats BETWEEN ? AND ?)` |
| `table_name` | `(h.table_name IN (?, …))` |
| `date` | `from`: `(h.timestamp >= ?)` bound to `'YYYY-MM-DD'`; `to`: `(h.timestamp < ?)` bound to the **next** day; both: joined with `AND`. Correct because `hands.timestamp` is `%Y-%m-%dT%H:%M:%S%.f` (`TIMESTAMP_FORMAT`): `'2026-08-21T23:59:59' < '2026-08-22'`. |
| `counter` | `(hps.<column> <op> ?)`, `<column>` from `column_spec`, `<op>` one of `= <> < <= > >=` |
| `position` | `((hps.position IS NOT NULL AND (L1 OR L2 …)) OR (hps.position IS NULL))`, where the `IS NULL` part is present only when `?` is listed, and the `IS NOT NULL AND (…)` part only when another label is. The labels: `SB` `(hps.position = 9)`; `BB` `(hps.position = 8)`; `BTN` `(hps.position = 0)`; `CO` `(hps.position = 1)`; `MP` `(hps.position = 2 OR (hps.position IN (3, 4) AND h.max_seats >= 7))`; `EP` `(hps.position IN (5, 6, 7) OR (hps.position IN (3, 4) AND h.max_seats <= 6))`. The `IS NOT NULL` guard matters: `(hps.position = 9)` is `NULL` for a `NULL` position, and so is its `NOT`. |
| `result` | `(hp.net_micros > 0)` / `(hp.net_micros < 0)` / `(hp.net_micros = 0)` |
| `pot_size` | min `n`: `(CASE WHEN h.big_blind_micros > ? THEN 0 ELSE h.total_pot_micros >= ? * h.big_blind_micros END)`; max `n`: the same with `THEN 1` and `<=`; the first `?` is `lim = i64::MAX / n`, computed in Rust; both bounds: joined with `AND` |
| `went_all_in` | `(EXISTS (SELECT 1 FROM actions a WHERE a.hand_id = h.id AND a.player_id = hp.player_id AND a.is_all_in = 1))` |
| `hole_cards` | `(hp.hole_card_1 IS NOT NULL AND hp.hole_card_2 IS NOT NULL AND (max(hp.hole_card_1 >> 2, hp.hole_card_2 >> 2) * 32 + min(hp.hole_card_1 >> 2, hp.hole_card_2 >> 2) * 2 + ((hp.hole_card_1 & 3) = (hp.hole_card_2 & 3))) IN (?, …))`, each class bound as `hi * 32 + lo * 2 + suited`, ranks `2 = 0 … A = 12`, `suited` 1 for `s` and 0 for `o` and pairs: `AKs` = 407, `AKo` = 406, `77` = 170 |

**Why `pot_size` is exact.** With `lim = ⌊i64::MAX / n⌋`:
- If `bb ≤ lim`, then `n·bb ≤ i64::MAX` and the product fits.
- If `bb > lim`, then `bb ≥ lim + 1`, so `n·bb ≥ n·lim + n > i64::MAX ≥ total_pot`. "At least" is therefore false and "at most" true, without computing the product.

SQLite evaluates only the branch of `CASE` it takes. The schema bounds `big_blind_micros` only from below, which is why bounding `n` alone would not be enough.

No result can show that the guard is missing. SQLite compares an integer with the `REAL` an overflowing product becomes, and an unguarded expression gives the same answers on every test hand, with a comparison whose `typeof` is still `integer`. So the guard is pinned by a unit test on the compiled fragment (M17.4 test 4).

### 5.5 Canonical form

- `to_document` writes `{"format_version":1,"filter":…}` with no whitespace.
- Fields come in the order of §5.2, `type` first; absent optional fields are omitted.
- Values are kept as given, never normalised: `"0.05/0.10"` stays `"0.05/0.10"`.
- `parse_document(&to_document(&f)) == Ok(f)` for every valid `f`.

For `tests/fixtures/filters/saw_flop_no_showdown.json` the canonical form is exactly:

```
{"format_version":1,"filter":{"type":"and","children":[{"type":"currency","any_of":["CAD"]},{"type":"platform","any_of":["GGPoker"]},{"type":"stakes","any_of":["0.05/0.10"]},{"type":"table_size","min":6,"max":6},{"type":"counter","column":"f_saw","op":">=","value":1},{"type":"not","child":{"type":"counter","column":"sd_went","op":"=","value":1}}]}}
```

## 6. M17.2 — Typed reports (no visible change)

**Why.** A UI renders, sorts and pages tables. Parsing CSV back would duplicate the rounding rules and lose exactness. So reports are built as typed rows, and CSV becomes one writer over them.

### M17.2.1 API

```rust
// src/report/
pub fn build_report(repo: &SqliteRepository, kind: ReportKind, options: &ReportOptions)
    -> Result<Report, ReportError>;
pub fn write_csv(report: &Report, out: &mut impl Write) -> Result<(), ReportError>; // Io only
pub fn write_report(...) -> Result<ReportOutcome, ReportError>; // unchanged signature: build, then write_csv

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report { #[serde(flatten)] pub rows: ReportRows, pub unmatched_stakes: Vec<Stakes> }

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "rows", rename_all = "snake_case")]
pub enum ReportRows {
    Results(Vec<ResultsRow>), Positions(Vec<PositionsRow>),
    Sessions(Vec<SessionsRow>), Stats(Vec<StatsRow>),
}
```

- `Report` serializes as `{"kind":"results","rows":[…],"unmatched_stakes":[…]}`. The reviewer checked `flatten` with an adjacently tagged enum.
- `ReportKind` gets `Serialize` and `Deserialize` (snake case).
- `Stakes` serializes as its `Display` string (`"0.05/0.1"`) and deserializes through `FromStr`.
- Everything `build_report` reads happens in the one read transaction after the version guard, as today. A stale cache or an overflow returns `Err` before any row exists.

### M17.2.2 Rows

The existing row structs, whose fields today are preformatted `String`s, become:

| Row | Fields (CSV column order; serde names = CSV column names) |
| --- | --- |
| `ResultsRow` | `currency: CurrencyCode`, `stakes: Option<Stakes>` (`None` = the `ALL` row; CSV `ALL`; JSON `null`), `hands: i64`, `net: Decimal`, `net_per_100: Decimal`, `bb_won: Decimal`, `bb_per_100: Decimal`, `rake`, `fees`, `blinds`, `straddles: Decimal`, `allin_hands: i64`, `ev_net: Decimal`, `ev_bb_per_100: Decimal` |
| `PositionsRow` | `currency`, `position: PositionLabel`, `hands: i64`, `net: Decimal`, `bb_per_100: Decimal`, `vpip`, `pfr`, `rfi: Option<Decimal>` |
| `SessionsRow` | `currency`, `platform: Platform`, `table: String`, `start`, `end: NaiveDateTime`, `duration_seconds: i64`, `hands: i64`, `net: Decimal`, `bb_per_100: Decimal`, `ev_net: Decimal` |
| `StatsRow` | `currency`, `stat: &'static str`, `numerator: i64`, `denominator: i64`, `value: Option<Decimal>` |

- **Money** (`net`, `rake`, `fees`, `blinds`, `straddles`, `ev_net`) is **exact**: `micro_units_to_decimal`, normalised. CSV prints it with `Display` (`-25.79`, `14.23115`, `0`, `-1`), as today.
- **Figures** (`net_per_100`, `bb_won`, `bb_per_100`, `ev_bb_per_100`, the percentages and factors) are the value `docs/stats.md` §1.2 prints:
  - rounded half-even once from the exact value, as a `Decimal` with **scale 2**;
  - **never negative zero**;
  - CSV prints them with `Display` (`-26.08`, `0.00`, `-208.10`).
  - The exact rationals stay internal. A UI shows and sorts the rounded value, and a later spec can add the exact value if a chart needs it.
- `None` (a stat whose denominator is 0) prints as an empty CSV cell and as JSON `null`.
- `start`/`end` print with `TIMESTAMP_FORMAT`, which is how they are stored (`2026-08-21T16:54:10`). Not with `NaiveDateTime`'s `Display`, which puts a space before the time. serde's chrono format already writes the `T`.
- The internal `HandEntry` of `sessions.rs` keeps its text fields (`platform`, `currency`), and the unit tests that build hands on `"PlatformA"`/`"PlatformB"` keep passing unchanged. Text becomes `Platform` and `CurrencyCode` only when a `SessionsRow` is built. Text that does not parse there is `StorageError::CorruptRow`.

**The one authorized edit in M17.2:** `src/report/sessions.rs::tests::session_bb_per_100_divides_once_per_big_blind` may change:
- its two assertions on `row.net` and `row.bb_per_100` to compare `Decimal`s (`1237.23`, `5155.12`);
- its hands' platform from `"PlatformA"` to `"GGPoker"`, so that the row can be built.

Nothing else in that test changes.

### M17.2.3 `PositionLabel`

```rust
// src/stats/mod.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PositionLabel { Ep, Mp, Co, Btn, Sb, Bb, Unknown } // serde and as_str: "EP" … "BB", "?"
impl PositionLabel {
    pub const ALL: [Self; 7];                                  // report order: EP, MP, CO, BTN, SB, BB, ?
    pub fn as_str(self) -> &'static str;
    pub fn of(code: Option<u8>, max_seats: u8) -> Self;        // docs/stats.md §2; None → Unknown
}
```

`position_label(code, max_seats)` stays with the same signature, because tests call it. For every code `0 … 9` and `max_seats` `2 … 10`, `PositionLabel::of(Some(code), max_seats).as_str() == position_label(code, max_seats)` (a test).

**Tests** (new):

1. The existing golden and v3.5 tests pass unchanged (through `write_report`).
2. `build_report` on the v3.6 database returns typed values equal to the goldens. For example, the results row has `net = dec!(-25.79)`, `bb_per_100 = dec!(-26.08)` with scale 2, `straddles = dec!(0)`, `allin_hands = 9`, and `ev_net = dec!(-19.396794)`. The positions rows come in the order `EP, MP, CO, BTN, SB, BB`.
3. `serde_json::to_string(&build_report(…Results…))` on the v3.6 database is exactly:

   ```
   {"kind":"results","rows":[{"currency":"CAD","stakes":"0.05/0.1","hands":989,"net":"-25.79","net_per_100":"-2.61","bb_won":"-257.90","bb_per_100":"-26.08","rake":"14.23115","fees":"2.500423","blinds":"29.25","straddles":"0","allin_hands":9,"ev_net":"-19.396794","ev_bb_per_100":"-19.61"}],"unmatched_stakes":[]}
   ```

4. A figure that rounds to zero from a negative value is a `Decimal` with `is_sign_negative() == false`. Use the existing `(100_000, -100)` `bb_won` case.
5. A positions or stats value with a zero denominator is `None` and prints empty.
6. `PositionLabel::of` against `position_label`, as above.

**Docs.**
- ARCHITECTURE §8 `src/report/`: `build_report`, `Report`, the rows, `write_csv`, `PositionLabel`.
- `invariants.md` §3 "Reports": typed rows, with the display rule stated once for both CSV and JSON.

**Done when.** As M17.1: the goldens with `cmp`, and the reviewer's `db_compare`.

## 7. M17.3 — The filter core

### M17.3.1 The module

- `src/filter/` (`pub mod filter` in `lib.rs`) implements §5 for the M17.3 node types.
- The M17.4 types are not variants yet. JSON naming them is `Malformed` ("unknown variant").
- The compiler is `pub(crate)` and only `report` calls it.

### M17.3.2 Reports accept a filter

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReportOptions { pub stakes: Option<Vec<Stakes>>, pub filter: Option<Filter> }
```

- `build_report` validates the filter first. An invalid one is the new `ReportError::Filter(FilterError)` and writes nothing.
- It then compiles the filter into the selection's `<filter>`.
- The stakes selection and the filter combine with AND.
- `ReportOptions` derives `Serialize` and `Deserialize` (the UI may send it), with `#[serde(default)]` on every field.
- The unmatched-stakes probe ignores the filter, as today: an item is unmatched when the selected player has no hand at those stakes at all.

**The one authorized edit of an existing test:** `tests/report.rs::stakes_options` may change its literal to `ReportOptions { stakes: Some(...), ..ReportOptions::default() }`. No other edit.

### M17.3.3 CLI

- `--filter-file <path>` is allowed only with `--report` (and, from M17.5, with `--save-filter`); anything else is usage error `2`.
  - It reads the file as UTF-8 and calls `parse_document`.
  - A file that is missing, unreadable, not UTF-8 or invalid is exit `2`. The message names the path and the `FilterError`.
  - The file is read and validated before any database is opened, as a missing input directory is today. With both the file and the database missing, the exit is `2`.
- `--filter-catalog` prints `serde_json::to_string_pretty(&filter::catalog())` and exits `0`.
  - It needs no database.
  - Combined with any other command or option it is usage error `2`.
- `USAGE` lists both.

### M17.3.4 The catalog

```rust
pub fn catalog() -> Catalog;   // Serialize
```

- **Pinned:** `serde_json::to_string(&catalog())` equals `tests/golden/v3_8/catalog_m17_3.json` (one line, without its trailing newline) in M17.3, and `catalog_m17_4.json` from M17.4. The reviewer wrote both from this section. The keys come in this order:
- `format_version`: `1`.
- `limits`: `{"max_depth":16,"max_nodes":256,"max_values":1000,"table_size_min":2,"table_size_max":10,"counter_flag_max":1,"counter_count_max":1000,"pot_size_min_big_blinds":1,"pot_size_max_big_blinds":10000}`. The `pot_size_*` entries arrive in M17.4.
- `node_types`: `[{"type":…,"group":…,"fields":[…]}]`, in §5.2 order, for the node types this build has. `fields` lists the field names in §5.2 order (`["min_big_blinds","max_big_blinds"]`; `[]` for `went_all_in`).
- `counter_columns`: the 69 `Flag` and `Count` entries of `COLUMN_SPECS`, in schema order, as `{"name","kind","signed"}`.
- `compare_ops`: `["=","!=","<","<=",">",">="]`.
- From M17.4:
  - `position_labels`: `["EP","MP","CO","BTN","SB","BB","?"]`;
  - `result_kinds`: `["net_won","net_lost","net_zero"]`;
  - `hole_classes`: the 169 classes in grid order. For each higher rank from `A` down to `2`, take the pair first, then each lower rank in descending order, suited before offsuit: `AA, AKs, AKo, AQs, …, A2o, KK, KQs, …, 32o, 22`.

Display labels ("3Bet Opportunity") are not in the catalog: they belong to the UI and its localisation.

**Tests** (new; most in `tests/filters.rs`):

1. **Golden cases.**
   - `v3_8_golden_cases_m17_3` imports the v3.6 folder once, then for every `tests/golden/v3_8/*/case.json` with `"milestone": "M17.3"`:
     - builds `ReportOptions` from the case;
     - compares the four `write_report` outputs byte for byte with the case's CSVs;
     - compares `unmatched_stakes`.
   - It asserts that it ran **9** cases, so a missing directory fails.
   - Do not hard-code the case names.
2. **Refusals**, each asserting the variant, the path and the detail. The exact inputs and errors are in the table after this list.
   - **Malformed:** not JSON; `format_version` missing; an unknown top-level field; `{"type":"bogus"}`; an unknown field on `counter`; `"value":1.0`; `{"type":"went_all_in","x":1}` (from M17.4).
   - **Version:** `"format_version":2` → `UnsupportedFormatVersion { found: 2 }`.
   - **`Invalid`**, one test per `FilterProblem` the M17.3 types can produce:
     - `EmptyGroup`, `EmptyList`, `EmptyValue`;
     - `DuplicateValue`, including `0.05/0.1` against `0.05/0.10`;
     - `InvalidCurrency` (`"cad"`), `InvalidPlatform`, `InvalidStakes`;
     - `InvalidDate` (`2026-02-30`, `26-08-21`, `9999-12-31` as `to`);
     - `EmptyRange`, `InvertedRange`;
     - `OutOfRange` (table size 1 and 11; `value` 2 on a `Flag`; `value` 1001 on a `Count`; `value` −1);
     - `UnknownColumn` (`"p_vpip; DROP TABLE hands"`), `NotACounter` (`rake_attributed_micros`, `ev_equity_num`);
     - `TooDeep` (17 nested `not`s → path of 16 zeros), `TooManyNodes`, `TooManyValues`.
   - **Path:** one case at depth, `{"type":"and","children":[A,{"type":"or","children":[B,C,BAD]}]}` → path `[1, 2]`.
3. **Canonical form.** The pinned string of §5.5, and the round trip for every file in `tests/fixtures/filters/` that this milestone's types can read.
4. **Two-valued.**
   - For `date`, `counter` and `table_name`, on the v3.6 database: `hands(x) + hands(not x) = 989`.
   - For `currency`, `stakes` and `table_size`, the same on the canonical database of test 6. On the v3.6 database each of them matches everything.
5. **Values are bound, and big lists compile.**
   - `table_name` `["x' OR '1'='1"]` selects nothing.
   - A unit test in `src/filter` compiles one node of every type. Every `alias.column` token in the SQL must be one of: `h.`, `hp.` or `a.` with a column named in §5.4; `hps.position`; or `hps.` with a `column_spec` name.
   - A `stakes` node with 1,000 entries (the `max_values` limit) runs on the v3.6 database and selects its 989 hands when `0.05/0.1` is among them.
6. **Currency, stakes and table size on canonical hands** (`tests/report.rs` style). `hero_sb_folds` is private to `tests/report.rs`, so copy it into `tests/filters.rs` or into `tests/support/`.
   - `Platform` has one variant, so `platform` can only be checked as matching everything.
   - The canonical database:
     - two currencies: `currency` selects one block;
     - two stakes: `stakes` selects one row and drops the `ALL` row;
     - table sizes 6 and 9: `table_size 7–10` selects the 9-max hand;
     - one table name of two.
7. **CLI** (`tests/cli.rs`):
   - `--report results --filter-file tests/fixtures/filters/three_bet_opp.json` on a database imported from the v3.6 folder → stdout equals `tests/golden/v3_8/m17_3_three_bet_opp/results.csv`;
   - an invalid filter file → exit `2`, and stderr names the file and the problem;
   - `--filter-file` without `--report` → `2`;
   - `--filter-catalog` → `0`, valid JSON whose `format_version` is `1` and which has 69 `counter_columns`;
   - `--filter-catalog --db x` → `2`.
8. **The catalog** equals `tests/golden/v3_8/catalog_m17_3.json` (M17.3.4).

**The refusal inputs of test 2.** `D(x)` is the document `{"format_version":1,"filter":x}`. `C` is `{"type":"currency","any_of":["CAD"]}`. `Ctr(col, op, v)` is a `counter` node.

| # | Input | Error |
| --- | --- | --- |
| 1 | `not json` | `Malformed` |
| 2 | `{"filter":C}` | `Malformed` (no version) |
| 3 | `{"format_version":1,"filter":C,"note":"x"}` | `Malformed` |
| 4 | `{"format_version":2,"filter":{"type":"bogus"},"note":1}` | `UnsupportedFormatVersion { found: 2 }` |
| 5 | `D({"type":"bogus"})` | `Malformed` |
| 6 | `D({"type":"counter","column":"p_vpip","op":"=","value":1,"extra":2})` | `Malformed` |
| 7 | `D({"type":"counter","column":"p_vpip","op":"=","value":1.0})` | `Malformed` |
| 8 | `D({"type":"and","children":[]})` | `Invalid { path: [], EmptyGroup }` |
| 9 | `D({"type":"currency","any_of":[]})` | `[]`, `EmptyList` |
| 10 | `D({"type":"table_name","any_of":["NLHRed4",""]})` | `[]`, `EmptyValue` |
| 11 | `D({"type":"stakes","any_of":["0.05/0.1","0.05/0.10"]})` | `[]`, `DuplicateValue { value: "0.05/0.10" }` |
| 12 | `D({"type":"currency","any_of":["cad"]})` | `[]`, `InvalidCurrency { value: "cad" }` |
| 13 | `D({"type":"platform","any_of":["ggpoker"]})` | `[]`, `InvalidPlatform { value: "ggpoker" }` |
| 14 | `D({"type":"stakes","any_of":["0.05"]})` | `[]`, `InvalidStakes { value: "0.05", reason }` (`reason` is `Stakes::from_str`'s message) |
| 15 | `D({"type":"date","from":"2026-02-30"})` | `[]`, `InvalidDate { value: "2026-02-30" }` |
| 16 | `D({"type":"date","to":"26-08-21"})` | `[]`, `InvalidDate` |
| 17 | `D({"type":"date","to":"9999-12-31"})` | `[]`, `InvalidDate` (no next day) |
| 18 | `D({"type":"date"})` | `[]`, `EmptyRange` |
| 19 | `D({"type":"date","from":"2026-09-25","to":"2026-08-21"})` | `[]`, `InvertedRange` |
| 20 | `D({"type":"table_size","min":1,"max":6})` | `[]`, `OutOfRange { field: "min", value: 1, min: 2, max: 10 }` |
| 21 | `D({"type":"table_size","min":6,"max":11})` | `[]`, `OutOfRange { field: "max", value: 11, min: 2, max: 10 }` |
| 22 | `D({"type":"table_size","min":9,"max":6})` | `[]`, `InvertedRange` |
| 23 | `D(Ctr("p_vpip","=",2))` | `[]`, `OutOfRange { field: "value", value: 2, min: 0, max: 1 }` |
| 24 | `D(Ctr("p_bets",">=",1001))` | `[]`, `OutOfRange { field: "value", value: 1001, min: 0, max: 1000 }` |
| 25 | `D(Ctr("p_bets",">=",-1))` | `[]`, `OutOfRange { field: "value", value: -1, min: 0, max: 1000 }` |
| 26 | `D(Ctr("p_vpip; DROP TABLE hands","=",1))` | `[]`, `UnknownColumn` |
| 27 | `D(Ctr("position","=",1))` | `[]`, `UnknownColumn` (`position` is not in `COLUMN_SPECS`; the `position` node covers it) |
| 28 | `D(Ctr("","=",1))` | `[]`, `EmptyValue` |
| 29 | `D(Ctr("rake_attributed_micros",">",0))` and `D(Ctr("ev_equity_num",">",0))` | `[]`, `NotACounter` |
| 30 | 17 nested `not` around `C` | `[0 × 16]` (16 zeros), `TooDeep { max: 16 }` |
| 31 | an `or` with 256 children `C` (257 nodes) | `[]`, `TooManyNodes { max: 256 }` |
| 32 | `D({"type":"currency","any_of":[1,001 × "CAD"]})` | `[]`, `TooManyValues { max: 1000 }` (the limits come before `DuplicateValue`) |
| 33 | `D({"type":"and","children":[C,{"type":"or","children":[C,C,{"type":"currency","any_of":[]}]}]})` | `[1, 2]`, `EmptyList` |
| 34 | `D({"type":"and","children":[{"type":"currency","any_of":[]},{"type":"date"}]})` | `[0]`, `EmptyList` (pre-order: the first problem wins) |

**Docs.**
- ARCHITECTURE §2: the `filter` module row.
- ARCHITECTURE §8: a new subsection, "Filters". Copy §5 of this spec into it as the living contract, for the types built so far.
- ARCHITECTURE §7: the CLI options.
- `invariants.md` §3: a "Filters" subsection with the compilation rules (two-valued, bound values, guarded products).
- `known_limitations.md`: date filters use timestamps as printed, and sessions under a filter (`docs/stats.md` §9).
- CHANGELOG.

**Report timings.** `cargo test --locked` before and after, and `tests/filters.rs` alone.

## 8. M17.4 — Hand filters

- Add the `position`, `result`, `pot_size`, `went_all_in` and `hole_cards` variants exactly as in §5.2–§5.4.
- Add the catalog entries of M17.3.4.
- `docs/stats.md` §9 defines them.

**Tests** (new):

1. `v3_8_golden_cases_m17_4`: the cases with `"milestone": "M17.4"`, run as in M17.3. It asserts **10** cases.
2. **Position SQL.** For every `code` in `0 … 9` and `max_seats` in `2 … 10`, and for `NULL`, the compiled `position` expression for each label is true exactly when `PositionLabel::of(code, max_seats)` is that label. Evaluate it in SQLite on a one-row query with the same aliases.
3. **Two-valued with missing data.** One hand in which the hero's position is `NULL` (build it as `tests/report.rs::positions_null_gives_question_mark_row` does):
   - `position ["EP","MP","CO","BTN","SB","BB"]` excludes it;
   - `not` of that includes it;
   - `position ["?"]` includes it.
4. **Pot size without floating point.**
   - Build a canonical heads-up hero hand at big blind **4,000,000,000,000** (4 × 10¹² currency units = 4 × 10¹⁸ micro-units). The small blind is half that, and both stacks are 5 × 10¹² units.
   - The hero posts the small blind and folds, so the pot is 1.5 big blinds = 6 × 10¹⁸ micro-units.
   - `i64::MAX ≈ 9.22 × 10¹⁸`, so `n = 1` and `2` take the product branch, and `n = 3` and `10000` the guard branch:
     - `min_big_blinds` 1 selects it;
     - `min_big_blinds` 2 does not;
     - `min_big_blinds` 3 does not (guard);
     - `max_big_blinds` 1 does not;
     - `max_big_blinds` 2 selects it;
     - `max_big_blinds` 3 selects it (guard);
     - `max_big_blinds` 10000 selects it (guard).
   - The reviewer checked these seven results in SQLite.
   - These results alone cannot detect a missing guard (§5.4). So a unit test in `src/filter` checks the compiled fragment and its binds for `min_big_blinds` 3, `max_big_blinds` 3, and both together:
     - each bound compiles to the `CASE WHEN h.big_blind_micros > ? THEN … ELSE … END` form of §5.4 (whitespace aside);
     - its binds are `[i64::MAX / 3, 3]` in that order;
     - a product `? * h.big_blind_micros` appears only inside an `ELSE` branch.
5. **The catalog** equals `tests/golden/v3_8/catalog_m17_4.json` (M17.3.4).
6. **Hole classes.**
   - The refusals of §5.3 (`"KAs"`, `"AAs"`, `"AK"`, `"aks"`, `"AKS"`, a duplicate).
   - The codes `AKs` = 407, `AKo` = 406, `77` = 170 in a unit test.
   - Every one of the 169 catalog classes parses.
7. **Canonical form and round trip** for every fixture file.
8. **Two-valued**, as M17.3 test 4, for each new leaf type.

**Docs.** As M17.3, for the new types (ARCHITECTURE §8 "Filters", `invariants.md` §3), and the CHANGELOG.

## 9. M17.5 — Saved filters

### M17.5.1 Migration 4

`MIGRATIONS` gets a fourth entry, so `SCHEMA_VERSION` is 4. Its `CREATE TABLE` text is exactly this; `sqlite_master.sql` must equal it byte for byte:

```sql
CREATE TABLE saved_filters (
    name TEXT NOT NULL PRIMARY KEY COLLATE NOCASE CHECK (length(name) BETWEEN 1 AND 100),
    format_version INTEGER NOT NULL CHECK (format_version >= 1),
    definition TEXT NOT NULL CHECK (length(definition) > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT
```

- The migration creates nothing else and touches no other table.
- Opening a v3.7.1 database (schema 3) migrates it and keeps every row. It does **not** rebuild the statistics cache: `rebuilt_on_open()` is `None`.
- A v3.7.1 build refuses a migrated database with `UnsupportedSchemaVersion`. Record that under "Schema history" in `known_limitations.md`.

### M17.5.2 API

```rust
// SqliteRepository
pub fn save_filter(&mut self, name: &str, filter: &Filter, overwrite: bool)
    -> Result<SavedFilterInfo, StorageError>;
pub fn list_filters(&self) -> Result<Vec<SavedFilterInfo>, StorageError>;   // ordered by name (NOCASE)
pub fn load_filter(&self, name: &str) -> Result<Filter, StorageError>;
pub fn delete_filter(&mut self, name: &str) -> Result<(), StorageError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SavedFilterInfo { pub name: String, pub format_version: u32,
                             pub created_at: DateTime<Utc>, pub updated_at: DateTime<Utc> }
```

- **Names** are 1–100 characters (Unicode scalar values), with no leading or trailing whitespace and no control characters (`char::is_control`). Anything else is `StorageError::InvalidFilterName { name, reason }`.
  - Uniqueness ignores ASCII case (`COLLATE NOCASE`): saving `big pots` when `Big pots` exists conflicts.
  - Case outside ASCII counts: `Étude` and `étude` are two names. Document both.
- **Save.**
  - It validates `filter` first; an invalid one is `StorageError::InvalidSavedFilter { name, source: FilterError }`.
  - It stores `to_document(filter)` and `format_version` 1.
  - An existing name is `SavedFilterExists { name }` unless `overwrite`. Overwriting keeps `created_at`, sets `updated_at`, and keeps the stored spelling of the name.
  - Times are UTC, stored as `%Y-%m-%dT%H:%M:%SZ` like `import_runs.started_at`.
  - It runs in one `BEGIN IMMEDIATE` transaction.
- **Load** runs `parse_document` on the stored text, so a stored definition is validated again every time.
  - A stored `format_version` or document that this build cannot read is `InvalidSavedFilter { name, source }` (for example `UnsupportedFormatVersion { found: 2 }`). It is never skipped and never repaired.
- **Delete and load of an unknown name** are `UnknownSavedFilter { name }`.
- **Names are matched ignoring ASCII case**, by the `NOCASE` collation: `load_filter("BIG POTS")` finds `Big pots`, and so does `delete_filter`. Saving with `overwrite` keeps the stored spelling.
- **Two versions are stored.** The `format_version` column is checked first: a value other than `1` is `InvalidSavedFilter { source: UnsupportedFormatVersion { found } }`. Then the document's own version is checked by `parse_document`. Both must be `1`.
- `SavedFilterInfo` serializes its times as RFC 3339 (`2026-11-04T20:46:00Z`), which is serde's chrono format.
- The name is always a bound parameter (`AGENTS.md` §3.11).

### M17.5.3 CLI

| Command | Behaviour |
| --- | --- |
| `--report <kind> --filter <name> [--db]` | Loads the saved filter and reports with it. `--filter` with `--filter-file` → `2`. Unknown name → `1`. A stored definition this build cannot read (`InvalidSavedFilter`) → `1`: that is the database's state, not a usage error. |
| `--save-filter <name> --filter-file <path> [--overwrite] [--db]` | Saves and prints `Saved filter '<name>'`. Invalid name or filter → `2`; name exists without `--overwrite` → `1`. `--overwrite` without `--save-filter` → `2`. |
| `--list-filters [--db]` | Prints CSV `name,format_version,created_at,updated_at`, quoting the name as `invariants.md` §3 quotes text. Times are printed as stored, `%Y-%m-%dT%H:%M:%SZ`, not with `DateTime<Utc>`'s `Display`. |
| `--delete-filter <name> [--db]` | Prints `Deleted filter '<name>'`; unknown → `1`. |

- All of them open the database with `open_existing`: a missing database is `1`.
- Each command combined with an import, `--delete-run`, `--rebuild-stats` or another of these commands is `2`.

**Tests** (new):

1. The migration:
   - a new database's `saved_filters` SQL equals M17.5.1 byte for byte;
   - a schema-3 database holding a few hands opens, reaches 4, keeps its hands and does not rebuild. Make it with this build, then drop `saved_filters` and set `user_version` to 3 with plain SQL.
2. Save, list, load and delete round trips. Overwrite keeps `created_at` and moves `updated_at` (≥, since the clock may not move).
3. Names:
   - `""`, `" x"`, `"x "`, `"a\u{7}b"` and a 101-character name are refused;
   - a 100-character name is saved;
   - `Big pots` then `big pots` conflict;
   - `Étude` and `étude` coexist;
   - `load_filter` and `delete_filter` with a different ASCII case find the filter;
   - `x'; DROP TABLE hands; --` saves, loads and deletes, and `hands` is intact.
4. Validation on load: rows inserted with plain SQL that hold `format_version` 2, a document with `"format_version":2`, and a document naming an unknown column each give `InvalidSavedFilter` with the right `source`.
5. CLI: save the `premiums` fixture under a name; `--list-filters`; `--report positions --filter <name>` equals `tests/golden/v3_8/m17_4_premiums/positions.csv`; delete it; then the exit codes of the table.

**Docs.**
- ARCHITECTURE §6: migration 4, `saved_filters` and the API.
- ARCHITECTURE §7: the commands.
- `invariants.md` §3: "Saved filters".
- `known_limitations.md`: "Schema history".
- CHANGELOG.

## 10. M17.6 — Any player, and lookups

### M17.6.1 Player selection

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlayerSelection { #[default] Hero, Player { platform: Platform, screen_name: String } }

pub struct ReportOptions { pub stakes: …, pub filter: …, pub player: PlayerSelection }
```

- `Hero` is today's `hp.is_hero = 1`.
- `Player` resolves `SELECT id FROM players WHERE platform = ? AND screen_name = ?` (exact, case-sensitive), then uses `hp.player_id = ?`.
- An unknown player is the new `ReportError::UnknownPlayer { platform, screen_name }`, and writes nothing.
- Every report covers the selected player's player-hands: results, positions, sessions (`docs/stats.md` §6.1 as amended) and stats. The unmatched-stakes probe too.
- `docs/invariants.md` §3 "Hero only, per currency" becomes "The selected player only, per currency" (the hero by default).

### M17.6.2 Lookups for a UI

```rust
// SqliteRepository
pub fn list_players(&self) -> Result<Vec<PlayerSummary>, StorageError>;
pub fn platforms_of_player(&self, screen_name: &str) -> Result<Vec<String>, StorageError>;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlayerSummary { pub platform: Platform, pub screen_name: String, pub hands: i64, pub hero_hands: i64 }

// report
pub fn filter_values(repo: &SqliteRepository, player: &PlayerSelection) -> Result<FilterValues, ReportError>;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FilterValues {
    pub currencies: Vec<CurrencyCode>, pub platforms: Vec<Platform>, pub stakes: Vec<Stakes>,
    pub table_sizes: Vec<u8>, pub table_names: Vec<String>,
    pub first_date: Option<NaiveDate>, pub last_date: Option<NaiveDate>,
}
```

- **`list_players`** lists the players with at least one `hand_players` row. `hands` counts those rows and `hero_hands` the ones with `is_hero`. The order is platform, then screen name, in byte order.
- **`platforms_of_player`** returns the platforms, as stored text in byte order, of every `players` row with that exact screen name, whether or not it has hands. A UI's player search and the CLI's `--player` both use it. It returns text, not `Platform`, so a row from a platform this build does not know is still visible.
- **`filter_values`** gives the distinct values present among the selected player's hands, which are the values a UI's pickers offer:
  - `currencies` in `CurrencyCode` order (alphabetical);
  - `platforms` in `Platform::as_str` byte order;
  - `stakes` ordered by big blind, then small blind;
  - `table_names` in byte order;
  - table sizes ascending;
  - dates from the timestamps as printed.

  It reads in one transaction after the version guard.

### M17.6.3 CLI

- `--player <name> [--platform <p>]` goes with `--report` or `--filter-values`.
  - Without `--platform`, the CLI calls `platforms_of_player`:
    - none → exit `1` (unknown player);
    - one that this build knows → that platform;
    - one that it does not know → exit `1`, naming it;
    - several → exit `2`, naming each platform as stored.
  - `--platform` is matched case-insensitively against `Platform::as_str`. An unknown value is `2`, and `--platform` without `--player` is `2`.
- `--list-players [--db]` prints CSV `platform,screen_name,hands,hero_hands`, with text quoted.
- `--filter-values [--player …] [--db]` prints `serde_json::to_string_pretty(&FilterValues)`.

**Tests** (new):

1. `v3_8_golden_cases_m17_6`: the **4** cases with `"milestone": "M17.6"`. `m17_6_hero_by_name` equals `v3_6_*.csv`.
2. `UnknownPlayer` writes nothing; `PlayerSelection::default()` is `Hero`.
3. `list_players` on the v3.6 database:
   - 102 rows;
   - the first is `GGPoker, 1072d9a5, 33, 0`;
   - `GGPoker, Hero, 989, 989`;
   - `GGPoker, c7bdd1e4, 182, 0`.
4. `filter_values` on the v3.6 database:
   - for the hero:
     - currencies `[CAD]`; platforms `[GGPoker]`; stakes `[0.05/0.1]`; table sizes `[6]`;
     - table names `NLHRed10, NLHRed14, NLHRed15, NLHRed18, NLHRed19, NLHRed21, NLHRed3, NLHRed4, NLHRed7`;
     - dates `2026-08-21` to `2026-09-25`;
   - for `c7bdd1e4`: table names `[NLHRed4]`, dates `2026-08-21` to `2026-08-21`.
5. CLI:
   - `--report results --player c7bdd1e4` equals the `m17_6_opponent` golden;
   - after a plain-SQL insert of a `players` row `('PokerStars', 'c7bdd1e4')`, the same command is exit `2` and names `GGPoker` and `PokerStars`, while `--platform GGPoker` resolves;
   - `--player nobody` → `1`;
   - `--platform` alone → `2`;
   - `--list-players` gives 103 lines (header plus 102).

**Docs.**
- ARCHITECTURE §7 and §8: `PlayerSelection`, the lookups and the CLI.
- `invariants.md` §3: "selected player".
- CHANGELOG, plus the version row of the table: `v3.8 | 0.5.0 | <count> (<count − 1> on Windows) | Filters (M17): column specs, one selection, typed reports, the filter document and compiler, saved filters, any player`.

**After M17.6** the reviewer copies whatever still holds of §5 into the living docs (the agent did most of it in M17.3–M17.5) and moves this spec to `docs/history/`.

## 11. v3.9 outlook (planned, flexible)

The owner can reorder or drop any of this. Each item gets a spec first. What v3.8 does for each:

| v3.9 item | What v3.8 prepares |
| --- | --- |
| UI technology chosen; the UI's first screens | Every API serializes (`Report`, `Filter`, `Catalog`, `FilterValues`, `PlayerSummary`, `SavedFilterInfo`); the library does no I/O of its own |
| No rebuild inside `open`: stale-cache state, then a rebuild on request with progress and cancellation | Nothing yet; v3.8 bumps no version, so it never triggers a rebuild |
| All-in equity kept across rebuilds, keyed on its inputs (live hole cards and the all-in board), under its own version | `ColumnKind::EquityNumerator`/`EquityDenominator` identify the columns to keep |
| The rebuild's second players query per hand removed | — |
| Final hand and board texture: definitions, a per-hand cache table, `STATS_VERSION` 5, their filter nodes | The filter format is open to new node types without a format-version bump; the catalog lists what a build has |
| Money filters ("amount won ≥ 5 bb"), session filters | `ColumnKind::Money` and `signed`; the guarded-product rule of §5.4 |
| Display labels and localisation | Kept out of the core on purpose (M17.3.4) |

## 12. Out of scope for v3.8

- A text syntax for filters (`position in (BTN, CO) and won`).
- Range strings (`A2s+`).
- Display labels.
- "The pool" (every non-hero player-hand together; M18).
- Session filters.
- Money filters.
- Final hand and board texture.
- Currency conversion.
- Anything in `src/parser/`.

## 13. Owner decisions (Nov 4, 2026)

| # | Decision |
| --- | --- |
| D1 | Filters are a JSON tree with `format_version` (§5); no text syntax in v3.8. |
| D2 | Saved filters live in the database (migration 4). |
| D3 | `--player` is an exact name; `--platform` is required when the name is on more than one platform; the pool is deferred to M18. |
| D4 | The size of the real database informs v3.9 only. |
| D5 | Session-detail filters: v3.9 or later. |
| D6 | Dates are calendar dates of the timestamps as printed, inclusive, `YYYY-MM-DD`. |
| D7 | Report totals are checked, in all four files (M17.1.5). |
| D8 | `docs/history/BLUEPRINT.md` stays as committed (CRLF since `50adfe8`). It causes no issue: no test reads it; it is `-text`, so git never converts it; and the only tool that reads it, `tools/blueprint`, strips `\r` and has a test for a CRLF copy. |
| D9 | The rebuild's second players query: v3.9. |
| D10 | UI technology: decided before v3.9. |
| D11 | `serde_json` moves to `[dependencies]` (done in the preparation commits; `Cargo.lock` unchanged). |
| D12 | The filter definitions of `docs/stats.md` §9, with the pot size on the total pot before deductions. |
| D13 | The CLI is a test harness: it exposes each API once and gains no behaviour of its own. |

## Appendix A — Column kinds (`COLUMN_SPECS`)

Schema order is `stats::COLUMNS`; this appendix fixes only the kind and the sign. Every column is unsigned except `ev_net_micros`. Over the full scratch database (6,846 player-hands), every `Flag` column holds only 0 or 1, and the largest `Count` value is 2.

- **`Flag` (49):**
  - preflop: `hands`, `p_walk`, `p_vpip`, `p_pfr_opp`, `p_pfr`, `p_rfi_opp`, `p_rfi`, `p_3bet_opp`, `p_3bet`, `p_3bet_faced`, `p_3bet_fold`, `p_3bet_call`, `p_4bet_opp`, `p_4bet`, `p_4bet_faced`, `p_4bet_fold`, `p_4bet_call`, `p_cc_opp`, `p_cc`, `p_steal_opp`, `p_steal`, `p_sb_steal_faced`, `p_sb_steal_fold`, `p_sb_steal_raise`, `p_bb_steal_faced`, `p_bb_steal_fold`, `p_bb_steal_raise`;
  - postflop: `f_saw`, `t_saw`, `r_saw`, `f_cbet_opp`, `f_cbet`, `f_cbet_faced`, `f_cbet_fold`, `f_cbet_call`, `f_cbet_raise`, `t_cbet_opp`, `t_cbet`, `t_cbet_faced`, `t_cbet_fold`, `t_cbet_call`, `t_cbet_raise`, `f_xr_opp`, `f_xr`;
  - outcome: `sd_went`, `sd_won`, `won_hand`, `f_saw_won`;
  - equity: `ev_qualified`.
- **`Count` (20):** `p_bets`, `p_raises`, `p_calls`, `p_checks`, `p_folds`, `f_bets`, `f_raises`, `f_calls`, `f_checks`, `f_folds`, `t_bets`, `t_raises`, `t_calls`, `t_checks`, `t_folds`, `r_bets`, `r_raises`, `r_calls`, `r_checks`, `r_folds`.
- **`Money` (5):** `rake_attributed_micros`, `fees_attributed_micros`, `blinds_posted_micros`, `straddles_posted_micros`, `ev_net_micros` (**signed**).
- **`EquityNumerator`:** `ev_equity_num`. **`EquityDenominator`:** `ev_equity_den`.

Total: 49 + 20 + 5 + 2 = **76**.

## Appendix B — CLI after v3.8

```
poker_tracker <input-dir> --ggpoker-currency <CODE> [--db <path>]
poker_tracker --delete-run <RUN-ID> [--db <path>]
poker_tracker --rebuild-stats [--db <path>]
poker_tracker --report <KIND> [--db <path>] [--stakes <LIST>] [--filter-file <path> | --filter <name>]
              [--player <name> [--platform <p>]] [--out <path>]
poker_tracker --filter-catalog
poker_tracker --save-filter <name> --filter-file <path> [--overwrite] [--db <path>]
poker_tracker --list-filters [--db <path>]
poker_tracker --delete-filter <name> [--db <path>]
poker_tracker --list-players [--db <path>]
poker_tracker --filter-values [--player <name> [--platform <p>]] [--db <path>]
```

The exit codes are unchanged: `0` ok, `1` fatal (including an unknown saved filter or player, or a name conflict), `2` usage (including an invalid filter document or name), `3` import with rejections.
