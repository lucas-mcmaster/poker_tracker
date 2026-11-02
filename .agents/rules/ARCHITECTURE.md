---
trigger: model_decision
description: poker_tracker architecture and domain specification (v3.5). Read before designing, writing, reviewing or debugging any code in src/ or tests/, before adding a platform, currency, format or invariant, and whenever a gate fails.
---

# `poker_tracker` Architecture & Domain Specification (v3.5)

Verified code for everything below is in `docs/BLUEPRINT.md`. This document explains the design
so you can follow, debug and extend it correctly.

## 1. Scope

- **Now:** hand-history ingestion and persistence. First source: GGPoker (PokerCraft export),
  No-Limit Hold'em cash, account currency **CAD**.
- **Designed for:** additional platforms and currencies, added as new site parsers without
  changing the domain, validator, storage or pipeline.
- **Out of scope until specified:** statistics (positions, VPIP/PFR, all-in EV, bb/100),
  currency conversion, Omaha, other limits, tournaments, GUI.

## 2. Layers

```
session file ──► ParserRegistry::detect ──► site parser (src/parser/<site>.rs)
                 (exactly one parser must recognize the file)
                                     │ CanonicalHand (site-neutral)
                                     ▼
                              validate_hand  (site-neutral invariants)
                                     ▼
                 SqliteRepository (per-hand savepoints; platform + currency are data)
                                     ▲
                 DirectoryIngester (walk *.txt sorted, detect, parse, batch, quarantine,
                                    stack continuity, report)
```

| Module | Responsibility | May know about sites? |
| :--- | :--- | :--- |
| `domain/money.rs` | `CurrencyCode`, micro-unit conversion | No |
| `domain/card.rs` | 1-byte `Card`, `Rank`, `Suit` | No |
| `domain/canonical.rs` | Canonical hand model, per-player results, stored identifiers | Only `Platform` variants |
| `parser/base.rs` | `ParseError`, `HandParser` trait, `HandBlock`, splitter, `validate_hand` | No |
| `parser/registry.rs` | `ParserRegistry`, `DetectionError` | No |
| `parser/ggpoker.rs` | GGPoker text format → canonical model | Yes (only here) |
| `storage/` | Identity, append-only migrations, batch insert, conflicts, sightings, quarantine, `load_hand` | No |
| `pipeline/` | Directory ingestion, `IngestionReport`, `HandFailure`, stack continuity | No |
| `main.rs` | CLI; registers site parsers | Registration only |

## 3. Money and currency

- In memory: `rust_decimal::Decimal`. In SQLite: `i64` micro-units, **millionths**
  (`MICRO_UNIT_SCALE = 1_000_000`), so SQL divides `*_micros` by `1e6`.
- `decimal_to_micro_units` returns `None` (never rounds) for precision below `0.000001` or
  overflow. `micro_units_to_decimal` returns normalised values.
- `CurrencyCode([u8; 3])`: exactly 3 uppercase ASCII letters, valid by construction
  (`FromStr`, `from_bytes`, consts `CAD`/`USD`/`EUR`/`GBP`); serializes as a string. The CLI
  upper-cases its option before parsing.
- Every `HandHeader` has a `currency`. Every money column is in that hand's currency.
  Aggregate per currency (`GROUP BY hands.currency`); never sum across currencies.
- Where a currency comes from is a parser concern: read it from the file if the site prints it,
  otherwise require it as configuration. GGPoker prints `$` for CAD and USD accounts alike, so
  `GgPokerParser::new(currency)` requires it and the CLI requires `--ggpoker-currency`.

## 4. Domain model (`domain/canonical.rs`)

**Enums** (each has a stable `as_str()` stored in SQLite, `ALL`, and `FromStr` as its exact
inverse via `stored_identifier!`, which fails to compile if a variant is missing):

- `Platform { GGPoker }`, `GameType { Cash }`, `BettingLimit { NoLimit }`.
- `Street { Preflop=0, Flop=1, Turn=2, River=3, Showdown=4 }` with `code`/`from_code`; actions
  never use `Showdown`.
- `ActionType { PostSmallBlind, PostBigBlind, PostDeadBlind, Fold, Check, Call, Bet, Raise,
  UncalledBetReturned }`.
  - `is_contribution()`: blinds (incl. dead), call, bet, raise.
  - `adds_to_street_commitment()`: live blinds, call, bet, raise (not dead blinds).
- `DeductionKind { Rake, Jackpot, Other }`: house money taken from the pot.
- `AdjustmentKind { EvCashoutRiskPaid }`: money between a player and the site outside the pot;
  `is_credit()` fixes the required sign (`EvCashoutRiskPaid` is always a debit).

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

**Accounting identities:** `total_pot − Σ deductions = Σ winners`;
`Σ contributions − Σ uncalled returns = total_pot`; `Σ net = Σ adjustments − Σ deductions`.

## 5. Parser contract and betting replay

`HandParser` is object-safe and must stay so: `platform()`, `describe()` (e.g.
`"GGPoker (CAD)"`), `recognizes(raw)` (cheap sniff of the opening line(s)), `split_blocks(raw)`
(default: blank-line separation, BOM/CRLF, 1-based `start_line`), `parse_hand(block)` (must end
with `validate_hand`), `peek_hand_id(block)`, `parse_session(raw)`.

`ParserRegistry::detect(raw)` returns the single recognizing parser, or `Unrecognized` /
`Ambiguous`. Register one parser per *source* (site format + configuration such as currency).
`ParseError` line numbers are relative to the hand block.

**Betting replay** (`validate_betting`; No-Limit; seats clockwise by seat number):

- Blinds lead: at most one small blind, at least one big blind. A live blind equals the stake,
  or less only when the poster is all-in; dead money ≤ one big blind.
- Preflop starts left of the *first* big blind poster; later streets left of the button.
- Each action comes from the next player clockwise who owes one. Folded and all-in players owe
  none. A bet or raise makes everyone else who can act owe one again. A lone player who can
  still act and matches the highest commitment owes nothing (the big blind after a short
  all-in call, for example).
- Legality: check only when not facing a bet; a call matches the bet (less only all-in); a bet
  only when not facing one, ≥ big blind unless all-in; a raise only when facing a bet, by at
  least the last full bet/raise (initially the big blind) or completing a smaller all-in to the
  big blind, unless all-in; a bet or raise needs another player who can still act; a player who
  already acted may raise again only when facing a full raise since then (a check keeps the
  right). So an incomplete all-in raise does not reopen the betting.
- An uncalled bet comes only after betting closed, goes to the largest committer, and equals
  exactly the unmatched part.
- No actions on undealt streets; no street after folds decided the hand; a hand still
  contested reaches the river.

**Side pots** (`validate_side_pots`): for each winner's live contribution `L`, players whose
live contribution is ≤ `L` are paid at most `Σ_all min(live, L) + Σ dead money`.

## 6. GGPoker format rules (`parser/ggpoker.rs`)

Only these observed formats are accepted. Everything else fails the hand.

- **Header:** `Poker Hand #<id>: Hold'em No Limit ($<sb>/$<bb>) - YYYY/MM/DD HH:MM:SS`; other
  games or stakes → `UnsupportedGame` (still recognized, so they fail per hand).
- **Table:** `Table '<name>' <n>-max Seat #<b> is the button`.
- **Seats:** `Seat <n>: <name> ($<stack> in chips)`, ascending, before any post.
- **Before `*** HOLE CARDS ***` only:** `posts small blind $x` (first post, if any),
  `posts big blind $x` (several players may post one), `posts missed blind $x`
  (`PostDeadBlind`: in the pot, not in the street commitment). A post of the whole remaining
  stack is recorded as all-in; a posted `and is all-in` suffix is unrecognized.
- **Dealing:** exactly one `Dealt to <name>` line per seated player, in seat order, straight
  after `*** HOLE CARDS ***`. Opponents' lines end with a space; Hero's is
  `Dealt to Hero [c1 c2]` (known cards ⇒ `is_hero`).
- **Actions:** one regex per verb. `folds`/`checks` take no amount; `calls $x`/`bets $x` one;
  `raises $x to $y` both, where `$x` is the raise *over the bet faced*: require
  `facing + x = y`; stored `amount = y − current street total`, `street_total_after = y`.
  `and is all-in` only on calls, bets and raises. Check only when not facing a bet; a call
  equals the bet faced (all-in calls may be less); bet only when not facing a bet.
- **Uncalled bets:** `Uncalled bet ($x) returned to <name>` → `UncalledBetReturned`.
- **Shows:** `<name>: shows [c1 c2] (…)` or one card; may precede the boards, repeat, or come
  from a folded player; they record known cards only. Conflicts → `ConflictingHoleCards`.
- **Boards:** `*** FLOP *** [a b c]`, `*** TURN *** [a b c] [d]`, `*** RIVER *** [a b c d] [e]`
  (prefix must match). Multi-run: `FIRST|SECOND|THIRD FLOP|TURN|RIVER`; after a flop all-in, GG
  prints `FIRST FLOP` and later runs start at `SECOND TURN`.
- **EV Cashout:** `<name>: Chooses to EV Cashout` (once, never once the river is dealt), then
  `<name>: Pays Cashout Risk ($x)` → `PlayerAdjustment { EvCashoutRiskPaid, -x }`. Anything
  else, or no settlement → `UnsupportedAdjustment`.
- **Showdown:** `*** SHOWDOWN ***` or `*** FIRST|SECOND|THIRD SHOWDOWN ***` in run order;
  `<name> collected $x from pot` → `PotWinner` on the current run (pots are never labelled).
- **Summary:** `Total pot $p | Rake $r | Jackpot $j | Bingo $b | Fortune $f | Tax $t`
  (Rake→`Rake`, Jackpot→`Jackpot`, Bingo/Fortune/Tax→`Other`, labels and zeros kept);
  `Hand was run two|three times`; `Board [..]` or `FIRST|SECOND|THIRD Board [..]` (may print
  only the differing cards); `Seat <n>: <name>`, optional `(button)` `(small blind)`
  `(big blind)`, then exactly one of:
  - `folded before Flop [(didn't bet)]` / `folded on the Flop|Turn|River`;
  - `won ($x)` / `collected ($x)`;
  - `showed [c1 c2] and <outcome>[, and <outcome>]…[, Cashout Risk ($x)]`, each outcome
    `won ($x)` or `lost`, optionally `with <words, at most one comma>`.
- **Summary cross-checks** (→ `SummaryMismatch`): winnings and cashout risk per seat; boards
  (multi-run: suffix of each runout); run count; one seat line per player; labels match the
  button seat, the small blind poster and the first big blind poster; fold street and
  `(didn't bet)`; shown cards equal known cards, one outcome per runout; `won`/`collected`
  only when nobody else is still in (a winner may still show voluntarily).

## 7. Invariants (`validate_hand`, site-independent)

- **Header:** non-empty id and table; `2 ≤ max_seats ≤ 10`, button within the table;
  `0 < SB ≤ BB`; UTC offset within ±18 h.
- **Players:** 2..=max_seats, unique seats and names, positive stacks, canonical card slots,
  exactly one hero with two known cards.
- **Actions:** contiguous sequences from 0; streets never go backwards, none on `Showdown`;
  Fold/Check amount zero, others positive; `street_total_after` chain; gross ≤ stack; all-in
  flag ⇔ whole stack committed; nobody acts after folding.
- **Betting replay and side pots:** §5.
- **Outcome:** identities of §4; deductions nonnegative and labelled; `uncalled_returned` =
  Σ returns; payouts positive, never to a folded player; single board pays run 0, multi-run
  pays runs `1..=n`, each at least once; adjustments reference seated players, are nonzero,
  have their kind's sign, never at showdown; every net representable.
- **Boards:** turn needs flop, river needs turn; multi-run runs 1..=n (2 ≤ n ≤ 3), each to the
  river; 52-card uniqueness (`u64` bitmask) for hole cards vs each runout and across runouts
  (cards shared with the first runout counted once).

## 8. Storage (`storage/`)

- **Identity first:** `open` reads `application_id` and `user_version` before writing.
  Ours (`0x504B5452`) → migrate; empty file → create; id 0 + version 1 + `hands`/`import_runs`
  → v3.4, upgrade in place; anything else → `NotAPokerTrackerDatabase`, file untouched
  (v3.3 databases: delete and re-import). Newer version → `UnsupportedSchemaVersion`. Only then
  WAL, `synchronous=NORMAL`, `foreign_keys=ON`. `open_existing` never creates a file.
- **Migrations:** append-only `MIGRATIONS` (`SCHEMA_VERSION = 2`); each runs under
  `BEGIN IMMEDIATE`, re-reading `user_version` first so concurrent opens never repeat a step.
  Migration 2 rescaled money ×100 to micro-units and added raw text, sightings and quarantine.
- **Kind columns** are non-empty text validated by the Rust enums; a new variant needs no
  migration.
- **Tables (all `STRICT`):** `players(platform, screen_name)`;
  `import_runs(id, started_at, tool_version)`; `hands` (header, `currency`, pot, aggregate
  `rake/jackpot/other_deductions_micros`, `uncalled_returned_micros`, storing run + file + line,
  `raw_text`, `raw_sha256`; `UNIQUE(platform, site_hand_id)`); `hand_players` (with
  `CHECK(net = won − invested + adjustments)`); `actions`; `board_cards`; `pot_winners`;
  `pot_deductions`; `player_adjustments`; `hand_sightings(hand_id, import_run_id, file, line)`;
  `quarantined_hands(run, raw_sha256, raw_text, parser, site_hand_id, stage, error, file,
  line)`; view `quarantine_pending` (quarantined texts not stored as a hand).
- **Inserts:** one `BEGIN IMMEDIATE` transaction per batch, one savepoint per hand, 5 s busy
  timeout. Validation, conversion, conflict and constraint errors reject that hand
  (`BatchInsertStats::rejected`); lock, I/O and other database-wide errors return `Err` and
  nothing from the batch is stored.
- **Duplicates:** same `(platform, site_hand_id)` is a duplicate only with the same currency
  (else `CurrencyConflict`) and the same content (else `ContentConflict`): the SHA-256 of
  `normalized_hand_text` when both sides carry raw text, otherwise the canonical hand from
  `load_hand`. A duplicate records a sighting and fills in missing raw text.
- **Sources:** `insert_sourced_hands_batch(hands, sources)` takes a `HandSource { run, file,
  line, raw_text }` per hand; `insert_hands_batch(hands)` stores none.
- **Undo:** `delete_import_run(run)` → `DeletedImportRun { hands_deleted, hands_kept,
  quarantined_deleted }`: deletes hands this run stored that no other run contained, keeps the
  rest (re-pointing provenance to the earliest remaining run), drops the run's quarantine rows
  and the run; unknown run → `UnknownImportRun`. Hands stored without a source are never
  deleted.
- **Quarantine:** `quarantine_hands(&[QuarantinedHand])`, once per run and text.

## 9. Pipeline and CLI

- `DirectoryIngester::ingest_directory(root, &ParserRegistry, &mut repo)`: resolves `root` to an
  absolute path; walks `*.txt` (following symlinks) in sorted order; records unreadable,
  unrecognized, ambiguous files and "no files found" in `file_errors`; parses each block and
  flushes every 1,000 hands or rejections; registers the run (with the real start time) on the
  first write; quarantines every parse and storage rejection with its raw text.
- `HandFailure { file_path, parser, block_start_line, hand_id, stage }`, `file_line()`.
- `IngestionReport` adds `started_at`, `hands_quarantined`, `stack_warnings`, `is_complete()`.
- **Stack continuity** (`check_stack_continuity`): per file, consecutive hands at a table (by
  timestamp) must not shrink a player's stack: each player starts at least where the previous
  hand ended (`starting + net`). Violations are `StackWarning`s, not failures. Equal timestamps, or an unparsed block in between, skip the pair.
- A fatal storage error returns `PipelineError::Storage { source, partial_report }`.
- CLI: `poker_tracker <dir> --ggpoker-currency CAD [--db path]` imports;
  `poker_tracker --delete-run <RUN-ID> [--db path]` undoes; `--help` exits 0; a missing input
  directory is a usage error checked before any database is opened; any option twice is a usage
  error; exit codes `0` ok, `1` fatal, `2` usage, `3` partial or no files.

## 10. Extending

**New platform or source:**

1. Collect real exports as fixtures.
2. Add a `Platform` variant (and list it in `stored_identifier!`).
3. Write `src/parser/<site>.rs` implementing `HandParser`: sniffing that cannot match other
   sites; its own `split_blocks` if needed; vocabulary mapped onto `DeductionKind` /
   `AdjustmentKind` with original labels; currency from the file or the constructor;
   `utc_offset_seconds` if stated; `is_all_in` on every whole-stack contribution; the site's
   observed line order and grammar enforced.
4. Re-export it from `parser/mod.rs` and add a CLI option.
5. Tests: per-hand assertions, whole-session reconciliation (conservation + stack continuity,
   no stack warnings on import), fault injection including single-line mutations, cross-site
   non-recognition.

**New canonical concept:** a site-neutral enum variant with a new, stable `as_str()`; no schema
change. A new column or table is a new entry appended to `MIGRATIONS`. Never add a
site-specific field to the canonical model.

**New format on an existing site:** add the real hand verbatim to `tests/common/mod.rs`, write a
failing test first, then extend the parser. Hands quarantined before the change are stored by
the next import of the same folder.

## 11. Known limitations

- **EV Cashout receiving side:** unobserved, so rejected (quarantined).
- **All-in blinds for less than the stake:** validator accepts them, but the others must
  usually call the full big blind, which the replay does not model; GG parser rejects them.
- **Heads-up:** not in the fixture; the button's summary line is expected to read
  `(button) (small blind)`.
- **Side pots:** GGPoker does not label them; the validator bounds payouts, amounts per pot
  must be derived from contributions.
- **Unobserved GG lines are rejected:** straddles, antes, bomb pots, mucks, sit-out and seat
  changes, non-`$` stakes.
- **Time zones:** GGPoker timestamps have none.
- **Opponent names:** hashes; check stability across sessions before opponent features.
- **Cross-site hero accounts:** a future accounts layer.
- **No re-parse command yet:** raw text is stored; re-import the folder instead.
- **Betting replay assumes No-Limit:** limit games would need bet-size rules.
- **Schema history:** migration 1 = v3.4 baseline, migration 2 = v3.5. v3.4 databases upgrade
  in place; v3.3-or-earlier databases must be deleted and re-imported once.
