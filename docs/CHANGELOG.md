# `poker_tracker` blueprint changelog

History only. The specification is `docs/SPEC_v3.6.md` (and frozen `docs/BLUEPRINT.md` for v3.5 baseline); where this file and the spec disagree, the spec is right.

| Version | Crate | Tests | Summary |
| :--- | :--- | :--- | :--- |
| v3.6.1 | 0.5.0 | 123 | Audit fixes: refuse six never-observed hand shapes, summary-card and `won`/`collected` checks completed, database identity read as one snapshot (foreign files refused at once), docs synced with code. |
| v3.6 | 0.5.0 | 121 | Closes four §8 limitations using real GGPoker data: heads-up summary labels, uncontested summary shows, straddles, and EV Cashout receiving side; verified across 10 session exports (989 hands). |
| v3.5 | 0.5.0 | 104 | Fixes from two reviews of v3.4 and an independent code review: betting replay (short all-ins, raise legality, minimum sizes, incomplete raises, blind sizes, all-in flag), side-pot eligibility, `Dealt to`/order/summary grammar rules, raw text + SHA-256 and `ContentConflict`, quarantine, true micro-units (migration 2), sightings-based undo, database identity, lock handling, stack-continuity warnings, CLI exit codes, extraction tool and lockfile. Details: blueprint §0. |
| v3.4 | 0.4.0 | 75 | Betting replay; strict per-verb regexes; summary cross-checks; append-only migrations; import runs and provenance; `CurrencyConflict`; `--delete-run`. |
| v3.3 | — | 64 | Site- and currency-independent model: `CurrencyCode`, `PotDeduction`, `PlayerAdjustment`, object-safe `HandParser`, `ParserRegistry`, `--ggpoker-currency`. |
| v3.2 | — | 55 | Fixes from the review of v3.1 against the real export: per-milestone file variants, lossless money, multi-run boards, dead blinds, EV Cashout, one-card shows, strict validation. |
| v3.1 | — | — | The original blueprint (Milestone 2 did not compile). |

## v3.6 → v3.6.1 (audit fixes)

From the audit of `1af1f57`; see `docs/SPEC_v3.6.1.md`.

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
