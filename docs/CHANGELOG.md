# `poker_tracker` blueprint changelog

History only, never a specification. The current spec and living rules are listed in `AGENTS.md` §1; where this file and them disagree, they are right.

| Version | Crate | Tests | Summary |
| :--- | :--- | :--- | :--- |
| v3.7.1 | 0.5.0 | 242 (241 on Windows) | Audit fixes: exact bb_won aggregation, fail-closed fallbacks, robust CLI, parser strictness (exact amounts, heads-up button/blind, EV cashout street restriction, multi-run shared cards, leniencies tightened). |
| v3.7 | 0.5.0 | 182 | Derived statistics cache (M13–M16): positions, opportunity counters, reports, and exact all-in equity. |
| v3.6.1 | 0.5.0 | 123 | Audit fixes: refuse six never-observed hand shapes, summary-card and `won`/`collected` checks completed, database identity read as one snapshot (foreign files refused at once), docs synced with code. |
| v3.6 | 0.5.0 | 121 | Closes four §8 limitations using real GGPoker data: heads-up summary labels, uncontested summary shows, straddles, and EV Cashout receiving side; verified across 10 session exports (989 hands). |
| v3.5 | 0.5.0 | 104 | Fixes from two reviews of v3.4 and an independent code review: betting replay (short all-ins, raise legality, minimum sizes, incomplete raises, blind sizes, all-in flag), side-pot eligibility, `Dealt to`/order/summary grammar rules, raw text + SHA-256 and `ContentConflict`, quarantine, true micro-units (migration 2), sightings-based undo, database identity, lock handling, stack-continuity warnings, CLI exit codes, extraction tool and lockfile. Details: blueprint §0. |
| v3.4 | 0.4.0 | 75 | Betting replay; strict per-verb regexes; summary cross-checks; append-only migrations; import runs and provenance; `CurrencyConflict`; `--delete-run`. |
| v3.3 | — | 64 | Site- and currency-independent model: `CurrencyCode`, `PotDeduction`, `PlayerAdjustment`, object-safe `HandParser`, `ParserRegistry`, `--ggpoker-currency`. |
| v3.2 | — | 55 | Fixes from the review of v3.1 against the real export: per-milestone file variants, lossless money, multi-run boards, dead blinds, EV Cashout, one-card shows, strict validation. |
| v3.1 | — | — | The original blueprint (Milestone 2 did not compile). |

## v3.7.1 → v3.8

- **M17.1 (Column specs, one selection, checked report totals):** `ColumnSpec`/`ColumnKind`/`COLUMN_SPECS`/`column_spec` define metadata for all 76 stats cache columns (with storage `CHECK`s driven by `signed`); stats report sums and maps columns by name; reports and stakes probe unify under one `Selection` fragment; report totals in all four reports use checked addition; `Stakes` moves to `domain::money`.
- **M17.1 review:** the stakes condition uses row values (`IN (VALUES …)`), so a stakes list of 994 or more items no longer exceeds SQLite's expression depth; every name the stats report reads is in a checked table (`STAT_TABLE` with a `Denominator`, `AGGRESSION_STREETS`) and its sums are read fail-closed; tests for the empty stakes list, the fragment, and the `figure` of every checked total.
- **M17.2 (Typed report rows, build_report and write_csv, PositionLabel):** `build_report` returns typed `Report`/`ReportRows` (`ResultsRow`, `PositionsRow`, `SessionsRow`, `StatsRow`) with exact money, figures of scale 2 that are never negative zero, and `None` on a zero denominator; `write_csv` formats typed reports to CSV; `write_report` delegates to `build_report` followed by `write_csv`; `PositionLabel` represents derived position labels; `Stakes` and `ReportKind` implement serde.
- **M17.2 review:** tests for `None` through `build_report` (positions and stats), for `CorruptRow` on an unparseable stored currency in every report, and for the `docs/stats.md` §2 position table; the VPIP denominator and the session hand count are checked instead of saturating; `None` again only for a zero denominator, as before M17.2; the overflow case of `bb_won_exact_rational_aggregation` gets its original values back.

## v3.7 → v3.7.1

- **M16.1.1 (Exact `bb_won` aggregation):** `bb_won`, `bb/100`, and `ev_bb_per_100` aggregated as exact rational fractions over big blinds, rounded half-even to 2 decimals, with `ReportError::ArithmeticOverflow` on overflow.
- **M16.1.2 (Derived-data fail-closed fallbacks):** `split_by_largest_remainder` sums weights and parts in `i128` (clamping negative weights to 0); equity pot/deduction and money post sums fail closed without plausible partial values.
- **M16.1.3 (Non-Unicode CLI arguments):** CLI reads `std::env::args_os()`, preserving path arguments as `PathBuf` and exiting 2 on non-Unicode text option values instead of panicking with exit 101.
- **M16.1.4 (No import run on failed first batch):** import run registration deferred to batch/quarantine transaction, leaving no dangling `import_runs` rows on failure.
- **M16.1.5 (Testable rebuild-once decision):** `rebuild_if_stale` factored to verify that opens finding the current `STATS_VERSION` under the write lock do not rebuild.
- **M16.1.6 (Rebuild errors name the hand):** rebuild derived errors wrap load and insert errors in `DerivedRebuildFailed` naming the platform and hand ID; missing seated players return `MissingPlayerId`.
- **M16.1.7 (CSV RFC 4180 quoting):** report text fields containing commas, double quotes, CR, or LF are quoted with doubled quotes.
- **M16.1.8 (Docs and CLI wording):** updated `docs/invariants.md` and `docs/CHANGELOG.md`; refined CLI usage strings, `--rebuild-stats` combined option error messages, and `--delete-run` notices.
- **M16.1 review:** `bb_won` sums each big blind's net in checked `i128` (it saturated in `i64`); an `invested` that cannot be represented makes the whole rake and fee split `0`; the equity fails closed on the deductions sum and on `ev_net_micros`; sessions compute `bb/100` once. New tests pin the fraction reduction across ten big blinds, CSV quoting per character, the start time of a quarantine-only run, and both branches of `rebuild_if_stale`.
- **M16.2.1 (Exact amounts and at most two decimals):** `parse_decimal` uses `Decimal::from_str_exact`, rejecting rounded values beyond 28 digits; `amt!()` restricted to at most two decimal places.
- **M16.2.2 (Heads-up button seated and posts small blind):** heads-up validation requires a seated player at the button seat who posts the small blind.
- **M16.2.3 (EV Cashout preflop restriction in GGPoker parser):** site-neutral core allows preflop cashout (pinned by canonical test); GGPoker parser restricts `Chooses to EV Cashout` to `Flop | Turn`.
- **M16.2.4 (Multi-run runouts retain cards bet on):** `validate_boards` verifies multi-run runouts share cards through the street betting closed; parser restricts subsequent flop/turn headers to streets without actions and rejects duplicate summary `Board` lines.
- **M16.2.5 (Validator detail assertions and near-miss verification):** `assert_violation` asserts error detail substrings; near-miss tests verified for nine validator rules.
- **M16.2.6 (Leniencies tightened):** card tokens require observed case (`23456789TJQKA` / `cdhs`); `shows` regex restricted to single parenthesised description; duplicate summary position labels rejected; opponent `Dealt to` trailing space documented as not enforced.
- **M16.2.7 (Exhaustive matching and domain documentation):** `validate_betting` matches exhaustively on `BettingLimit` and `GameType`; domain doc comments reworded site-neutrally; living invariants updated.
- **M16.2 review:** a street dealt after the betting ended is dealt anew in each runout, so a runout that repeats it is refused as `DuplicateCard` (before, a runout equal to the first on such a street counted as shared); a body `shows` description may not be empty. New tests pin `from_str_exact`, the `SECOND TURN` header rule, the flop and river branches of the shared-streets rule, and repeated `(button)` and `(small blind)` labels. M16.2 also gave the second hero of `rejects_zero_or_multiple_heroes` hole cards, so that the rule the test names fires (an edit the spec did not list).
- **Docs condensed (after v3.7.1; the crate is unchanged):** the finished specs and `BLUEPRINT.md` moved unchanged to `docs/history/` (index in its `README.md`); the contracts still in force (stats and report APIs, cache lifecycle, CLI, CSV columns, the M17–M20 plan) moved into `ARCHITECTURE.md`, `docs/invariants.md` and `docs/known_limitations.md`; `docs/invariants.md` §3 keeps only what `docs/stats.md` does not define; `.agents/rules/TASKS.md` merged into `AGENTS.md` §4.

## v3.6.1 → v3.7

- **M16 (Exact all-in equity and equity-adjusted results):**
  - Stats version bumped (`STATS_VERSION = 4`, 76 cache columns) adding `ev_qualified`, `ev_equity_num`, `ev_equity_den`, and `ev_net_micros`.
  - Bitmask 7-card evaluator in `src/stats/eval.rs` ranking 5–7 cards into 9 hand categories with kickers as a comparable integer, without floats or external dependencies.
  - Exact equity calculation (`exact_equity`) in `src/stats/equity.rs` enumerating all board completions; reduced integer fractions in lowest terms.
  - Qualification rules: ≥2 live players at end, all-in on preflop/flop/turn, 2 known hole cards per live player, and equal live contributions (side pots excluded).
  - Expectation on pot after all deductions (`total_pot − Σ deductions`); a player who took EV Cashout keeps their actual net; half-even rounding of EV net to micro-units.
  - Results report appends `allin_hands`, `ev_net`, and `ev_bb_per_100`; sessions report joins `hand_player_stats` and appends `ev_net`.
  - Review fix-up: half-even rounding of `ev_net_micros` is unit-tested (floor division, so negative quotients round correctly too); the four-way preflop equity and the folded-player test assert exact fractions; the EV Cashout ratio test checks its pot against the parsed hand; `tests/golden/` is protected from line-ending conversion.

- **M15 (Results, sessions, and reports):**
  - Stats version bumped (`STATS_VERSION = 3`, 72 cache columns) adding `rake_attributed_micros`, `fees_attributed_micros`, `blinds_posted_micros`, and `straddles_posted_micros`.
  - Largest-remainder attribution for rake and other deductions using 128-bit integer arithmetic with lower-seat tie breaking; exact conservation verified over the corpus.
  - Report engine in `src/report/` generating four CSV reports (`results`, `positions`, `sessions`, `stats`) in a single read transaction protected by derived cache version guard.
  - Half-even midpoint rounding to 2 decimals for percentages, factors, and bb amounts; exact lossless decimal formatting for money columns.
  - Table sessions formed by platform, table name, and currency with 30-minute inactivity boundary; one time basis per table (UTC when every hand states an offset, otherwise as printed).
  - CLI flags `--report`, `--stakes`, and `--out` in `poker_tracker`; pure CSV output on stdout, status and missing stakes notes on stderr; non-destructive execution (never creates a database file).
  - Review fix-up: the sessions report fails closed on a stored timestamp it cannot read (`CorruptRow`); sessions grouping is a tested function; `bb_won` divides once per big-blind value in every report (sessions divided hand by hand, which can round a half-even midpoint the wrong way); `--stakes` amounts must be plain decimals; worked-example money and the `ALL`, sessions and version-guard tests assert exact values; `docs/invariants.md` gains the M15 rules.

- **M14 (Opportunity counters):**
  - Stats version bumped (`STATS_VERSION = 2`, 68 cache columns).
  - Preflop opportunity and action counters: `p_rfi_opp`/`p_rfi`, `p_3bet_opp`/`p_3bet`, `p_3bet_faced`/`_fold`/`_call`, `p_4bet_opp`/`p_4bet`, `p_4bet_faced`/`_fold`/`_call`, `p_cc_opp`/`p_cc`, `p_steal_opp`/`p_steal`, `p_sb_steal_faced`/`_fold`/`_raise`, `p_bb_steal_faced`/`_fold`/`_raise`.
  - Postflop opportunity and action counters: `f_cbet_opp`/`f_cbet`, `f_cbet_faced`/`_fold`/`_call`/`_raise`, `t_cbet_opp`/`t_cbet`, `t_cbet_faced`/`_fold`/`_call`/`_raise`, `f_xr_opp`/`f_xr`.
  - Macro-generated column definitions, field mappings, and infallible `get()` lookup.
  - Canonical unit tests for all opportunity rules and near misses; full assertions for the 3 worked examples across all 68 columns; whole-folder pinned table validation (Hero, 989 hands).

- **M13 (Derived statistics cache, positions and basic facts):**
  - Schema migration 3 adds `derived_cache` metadata table (`version = 1`, `STATS_VERSION = 1`).
  - Derived cache table `hand_player_stats` automatically populated on insert savepoint and rebuilt on database open under `BEGIN IMMEDIATE` if cache is absent or stale.
  - Strict version guard in `insert_sourced_hands_batch` prevents inserting into stale cache without corrupting quarantine.
  - CLI `--rebuild-stats` forces full cache recomputation from stored canonical hands.
  - 10-seat positional mapping (`assign_positions`) supporting heads-up through full ring.
  - M13 counters and basic facts: `hands`, `p_walk`, `p_vpip`, `p_pfr_opp`, `p_pfr`, street action counts (`bets`, `raises`, `calls`, `checks`, `folds`), street saw indicators (`f_saw`, `t_saw`, `r_saw`), showdown indicators (`sd_went`, `sd_won`), and win indicators (`won_hand`, `f_saw_won`).

Final v3.7 test count: 182 tests passing (zero failures).

## v3.6 → v3.6.1 (audit fixes)

From the audit of `1af1f57`; see `docs/history/SPEC_v3.6.1.md`.

- **Never-observed shapes now refused:** heads-up missed blind, all-in straddle, blind for less than the stake, a second straddle by the same player, heads-up straddle, heads-up button not posting the small blind.
- **Summary checks:** an opponent's uncontested summary show includes every body-shown card; `won` needs a flop on every `showed` line.
- **Storage:** database identity is read in one read transaction and only a busy lock is retried; the M6 change that retried `NotAPokerTrackerDatabase` for 5 s is removed.
- **Tests:** the min-raise-over-straddle refusal is a consistent edit; new refusal tests for the shapes above and an EV Cashout in a multi-run hand.

Final test count: 123 tests passing (zero failures).

## v3.5 → v3.6

Closes four §8 limitations using real GGPoker data across 10 session files (989 hands):

- **M6 (Heads-up summary labels):** Support heads-up hands where the button posts the small blind and its summary line carries `(small blind)` only.
- **M7 (Uncontested summary shows):** Support uncontested winners showing 1 or 2 cards in the summary without hand rank description, and require `won` iff a flop was dealt and `collected` iff not.
- **M8 (Straddles):** Support forced posts with up to two straddles (`ActionType::PostStraddle`), preflop action starting left of the straddler, and minimum raise rules.
- **M9 (EV Cashout receiving side):** Support EV Cashout receiving side (`AdjustmentKind::EvCashoutReceived`), requiring river settlement with body/summary agreement for losing players.
- **M10 (Regression fixtures & session reconciliation):** 12 single-hand regression fixtures and full whole-folder reconciliation across 10 session files (989 hands, 1,130,742 bytes, Hero net `-$25.79`, 101 unique opponent hashes across 101 sessions).

New stored identifiers: `PostStraddle` (`ActionType::PostStraddle`), `EvCashoutReceived` (`AdjustmentKind::EvCashoutReceived`).
Final test count: 121 tests passing (zero failures).

## v3.3 → v3.4 (review fixes)

| Finding | v3.3 behaviour | v3.4 fix |
| :--- | :--- | :--- |
| Action order unchecked | A deleted fold, a double check, an empty street or a skipped blind still parsed | `validate_hand` replays the betting (see the list after this table) |
| Summary only partly cross-checked | Wrong position labels, fold streets or shown cards were accepted | Each summary seat line is parsed with a strict grammar and checked against the body (see below) |
| New kinds forced a re-import | Fixed `CHECK (kind IN …)` lists; `CREATE TABLE IF NOT EXISTS`; older databases refused | Kind columns are free text validated by the Rust enums, so a new variant needs no schema change; numbered, append-only `MIGRATIONS` keyed on `user_version` |
| No provenance | The source file and import run were never stored | `hands.import_run_id`, `source_file` and `source_line`, plus an `import_runs` table; `SqliteRepository::delete_import_run` and CLI `--delete-run` undo an import |
| Wrong currency silently kept | A re-import in another currency counted as a duplicate | Rejected with `StorageError::CurrencyConflict`; a same-currency re-import is still a duplicate |
| Repeated site flag | Two `--ggpoker-currency` options made every file ambiguous | Any option given twice is a usage error |
| Stray text accepted | `folds $7.5` and `calls $0.2 to $9` parsed; `shows` and EV Cashout were accepted before the deal | One regex per verb; only seats and blinds are accepted before `*** HOLE CARDS ***` |
| Validator gaps for future sites | Actions on undealt streets or by all-in players were accepted | Covered by the betting replay |
| Tests | 64 | 75 |

**Betting replay rules** (in `validate_hand`, site-neutral):

- turn order: preflop starts left of the first big blind, later streets left of the button;
- every player who can still act must act before a round closes, and a bet or raise reopens the
  action;
- checks, calls, bets and raises must be legal for the amount faced;
- an uncalled bet must return exactly the unmatched part of the largest bet;
- actions only on dealt streets, no street dealt after the hand was decided, and a contested
  hand reaches the river.

**Summary checks** (GGPoker parser): `(button)`, `(small blind)` and `(big blind)` labels, fold
street, `(didn't bet)`, shown cards, and one outcome per runout.

## v3.2 → v3.3 (multi-platform, multi-currency readiness)

| Area | v3.2 | v3.3 |
| :--- | :--- | :--- |
| Currency | `String` field; GGPoker parser defaulted to `USD` | `CurrencyCode` type, valid by construction; no default anywhere; the GGPoker parser requires the account currency (`GgPokerParser::new(CurrencyCode::CAD)`) |
| Choosing a parser | Caller passed one parser for a whole directory | `ParserRegistry` detects the single parser that recognizes each file; unrecognized or ambiguous files are reported, never guessed |
| Parser contract | Generic-only trait (not usable as `dyn`); fixed blank-line splitting | Object-safe `HandParser` with `describe`, `recognizes`, overridable `split_blocks` and `parse_session` |
| House deductions | GGPoker-shaped fields (`rake`, `jackpot`, `other_fees`) | `Vec<PotDeduction>` with a neutral `DeductionKind` (`Rake`/`Jackpot`/`Other`) plus the site's own label |
| Off-pot money | GGPoker-specific `EvCashout` struct | Generic signed `PlayerAdjustment` with `AdjustmentKind` (EV Cashout is one kind; insurance and similar features add kinds) |
| Time zone | Not modelled | `HandHeader::utc_offset_seconds: Option<i32>` (`None` for GGPoker, which does not state one) |
| Storage | `ev_cashouts` table; fee columns named after GGPoker fields | Schema v3: `pot_deductions` and `player_adjustments` detail tables, a `currency`/`platform` index, and per-hand currency throughout |
| CLI | `--currency` defaulting to USD | Each site is enabled by its own option, e.g. `--ggpoker-currency CAD`; at least one is required |
| Tests | 55 | 64 (adds registry detection, multi-currency storage, format sniffing) |

## v3.1 → v3.2

- **Real GGPoker formats supported:**
  - multi-run showdowns (`*** FIRST/SECOND/THIRD SHOWDOWN ***`);
  - dead blinds (`posts missed blind`);
  - EV Cashout;
  - one-card shows.
- **Strict validator:**
  - chip conservation, the `street_total_after` chain, exactly one hero;
  - stack and all-in consistency, street order, fold rules, run structure.
- **Parser checks:**
  - betting legality;
  - the summary section is cross-checked against the hand body.
- **Storage and pipeline:**
  - per-hand savepoints, so one bad hand never discards a batch;
  - per-player invested/won/net;
  - failure reports with hand id and source-file line.
- **Build plan:** milestone file sets that compile at every gate.
