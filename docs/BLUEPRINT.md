> **Note:** Frozen at v3.5. The current specification and living rules are in `docs/`; `AGENTS.md` §1 lists them in order of precedence (current spec: `docs/SPEC_v3.7.md`).

# `poker_tracker` — Reference Implementation Blueprint (v3.5)

Authoritative blueprint for the **hand-history ingestion and persistence backend** of
`poker_tracker`.

- **Architecture:** site- and currency-independent.
- **First supported source:** GGPoker No-Limit Hold'em cash games played in **Canadian dollars**.
- **Future sources:** other platforms and currencies plug in by adding a site parser. The domain
  model, validator, storage and pipeline stay unchanged.

Every `## FILE:` block in this document was compiled, tested and linted while producing it.
At **each** milestone, the exact file set listed for that milestone, with the `Cargo.lock`
shipped next to this document, passes:

- `cargo test --locked` (all targets);
- `cargo clippy --locked --all-targets -- -D warnings`, under the lint table in `Cargo.toml`
  (`unsafe_code = "forbid"`; `clippy::float_arithmetic`, `clippy::unwrap_used` and
  `clippy::expect_used` denied);
- `cargo fmt --check`.

**Toolchain.** Verified on **rustc 1.97.0**. The declared minimum, `rust-version = "1.88"`, is
the first release with stable `let`-chains in edition 2024. Every dependency version in the
shipped `Cargo.lock` declares a minimum of 1.88 or lower (the highest is 1.87), and Clippy's
default `incompatible_msrv` lint checks standard-library use against 1.88. A 1.88 toolchain was
not available while producing this document, so confirm once with `cargo +1.88 test --locked`
if you need to support toolchains older than the verified one. Never pass
`--ignore-rust-version`: it also makes Cargo resolve dependencies that need a newer Rust.

**Real-data verification.** The reference export is a GGPoker session from a CAD account:

- All **294 hands** parse, and every hand satisfies chip conservation.
- No player's stack ever drops between consecutive hands; every import now checks this.
- Hero's session net is **+2.86 CAD**. Together with **5.99 CAD** of automatic top-ups, that
  exactly explains Hero's stack moving from 10.00 to 18.85.
- **Mutation testing:** of 77,239 single-line edits of the real hands (delete, duplicate, swap
  with the next line, ±0.01 on an amount, rename a player), 223 are neither rejected nor flagged
  as a stack warning (v3.4 accepted 5,908). Of those, 210 change a starting stack in a way no
  check can see. The other 13 reorder lines (7), delete or move a folded player's one-card show
  (4), drop an extra big-blind post whose amount the player's later raise absorbs (1), or move
  an EV Cashout from the turn to the flop (1); none of them changes anyone's result.

---

## 0. What changed in v3.5

v3.5 implements two reviews of v3.4 (A and B below) plus an independent review of the v3.5
code. Earlier history is in `docs/CHANGELOG.md`; it is not part of this specification.

**Validator (site-neutral betting replay and outcome):**

| Finding | Source | v3.4 behaviour | v3.5 rule |
| :--- | :--- | :--- | :--- |
| Short all-in closes the round | A1 | A preflop call all-in for no more than the big blind, everyone else folding, was rejected ("uncalled bet returned while betting was open" / "ended before 1 player(s) acted") | A lone player who can still act and already matches the highest commitment owes no action |
| Raise legality | A4, B9 | A "raise" with no bet faced, or with every opponent all-in, was accepted | A raise needs a bet to raise and an opponent who can still respond (a bet too) |
| Incomplete all-in raise | B9 | Reopened the betting for everyone | Does not reopen it for players who already acted; cumulative short all-ins that add up to a full raise do |
| Bet and raise sizes | new (needed for the rule above) | Any size | A bet is at least the big blind; a raise adds at least the last full bet or raise, or completes a smaller all-in to the big blind; less only when all-in |
| All-in flag | A3, B9 | Committing the whole stack without the flag was accepted | The flag is set exactly when a contribution commits the whole stack |
| Blind sizes | B6 | A 0.06 small blind or a 0.25 big blind at 0.05/0.10 was accepted | A live blind equals the stake (less only all-in); dead money is at most a big blind |
| Side pots | B7 | A 2.71 all-in stack could be paid the whole 21.67 pot | Players who put in at most `L` together win at most `Σ min(contribution, L)` plus dead money |
| Call check | A6 | Contained a condition that was always true | Simplified |

**GGPoker parser (observed format rules):**

| Finding | Source | v3.4 behaviour | v3.5 rule |
| :--- | :--- | :--- | :--- |
| Who is Hero | B8 | Hero's cards on another seated player's `Dealt to` line were accepted (1,145 mutations) | Exactly one `Dealt to` line per seated player, in seat order, right after `*** HOLE CARDS ***` |
| Summary `showed` lines | B9 | Any text after `and won/lost` was accepted | Strict grammar: one `won ($x)`/`lost` per runout, an optional description of words, an optional `Cashout Risk ($x)` |
| Uncontested winner shows | A2 | `collected ($x)` was rejected if the winner had shown both cards | Accepted; a contested player summarised without a show is still refused |
| Blind that is the whole stack | A3, review | Imported with `is_all_in = 0` | Recorded as all-in, since GGPoker prints no suffix on blinds |
| Line order | mutation testing | Swapped seat lines or blind lines were accepted (1,570 mutations) | Seat lines ascend; the small blind is posted first; no EV Cashout once the river is dealt |

**Storage, pipeline and CLI:**

| Finding | Source | v3.4 behaviour | v3.5 behaviour |
| :--- | :--- | :--- | :--- |
| Same id, different hand | B1 | Silently counted as a duplicate; raw text not stored | `hands.raw_text` + `raw_sha256` (SHA-256 of normalised text); a re-import with different content is `StorageError::ContentConflict` (compared by canonical hand when either side lacks raw text) |
| Rejected hands lost | B1, B5 | Only reported | Kept with raw text in `quarantined_hands`; the `quarantine_pending` view lists those not stored yet |
| `_micros` columns were 10⁻⁴ | B2 | `1.00 = 10,000` | True micro-units, `1.00 = 1,000,000`; migration 2 rescales v3.4 databases in place |
| Undo removed shared hands | B3 | `--delete-run A` deleted hands that a later run B also imported | `hand_sightings` records every run that contained a hand; undo deletes only hands this run stored and no other run contained |
| Unknown run id | A5, B12 | `--delete-run <typo>` exited 0 ("Deleted 0") and could create the database | `StorageError::UnknownImportRun`, exit 1; `open_existing` never creates a file |
| Database identity | B4 | Any SQLite file got the schema; WAL was set before the version check; v3.3 databases were told to "upgrade poker_tracker" | `PRAGMA application_id`; the file is identified before anything is written; foreign files are untouched; v3.3 databases are told to delete and re-import |
| Locked database | B11 | No busy timeout; lock errors became per-hand failures | 5 s busy timeout, `BEGIN IMMEDIATE`, and lock/I/O errors abort the batch (exit 1) |
| Stack continuity | B10 | Test only | Every import warns when a stack shrinks between consecutive hands at a table |
| CLI rough edges | B12 | Symlinks skipped; 0 files exited 0; missing directory exited 3; `--help` exited 2; `cad` refused | Symlinks followed; no files → exit 3; missing directory → exit 2 before any database is opened; `--help` exits 0; currency case-insensitive |
| Storage nits | B16, A5 | Duplicate index; relative paths; `started_at` was the first write; no way to read a hand back | Index dropped; absolute paths; the run's real start time; `SqliteRepository::load_hand`; `INSERT … ON CONFLICT DO NOTHING` for runs |
| Concurrent first open | review | Two processes opening the same file could both run a migration (one then failed) | The schema version is re-read under the write lock |

**Document and tooling:** precise capturing `+ use<'_>` in the code, as §5 requires (B14); the
`tools/blueprint` extractor and drift check plus a shipped `Cargo.lock` (B13, B15); this
section no longer carries older history (B15); `.editorconfig` and `.gitattributes` keep
fixtures verbatim, and a test pins the export's SHA-256 (B15). **Tests: 75 → 104.** Of the 75
v3.4 tests, 69 are unchanged and 6 were adapted only for the micro-unit scale or the two API
changes (`HandSource::raw_text`, `DeletedImportRun`).

**Considered and not changed:** a re-import made only of duplicates still records an import run
(A5), because its sightings now give that run a meaning: undoing the earlier run keeps those
hands. A blind posted with `and is all-in` is still refused, and the EV Cashout receiving side
is still unsupported, until a real sample exists (§3). The blueprint stays one file (B13); the
extractor removes the retyping risk.

---

## 1. Architecture: what is shared and what is site-specific

```
                       ┌──────────────────────── site-specific ────────────────────────┐
 session file ──► ParserRegistry::detect ──► <site parser> (e.g. parser/ggpoker.rs)
                       └── recognizes(): exactly one parser must claim the file ───────┘
                                                   │
                                                   ▼  CanonicalHand (site-neutral)
                                          validate_hand  (site-neutral invariants)
                                                   │
                        rejected ◄─────────────────┼─────────────────► accepted
                            │                      │                      │
                            ▼                      ▼                      ▼
                   quarantined_hands      stack continuity      SqliteRepository
                   (raw text, error)      (warnings per file)   (platform + currency are data)
```

**Site-specific (one module per site):**

- the text format;
- format sniffing (`recognizes`);
- block splitting, if not blank-line separated;
- hand-id extraction;
- how the site's vocabulary maps onto canonical kinds (for example, GGPoker's
  Rake/Jackpot/Bingo/Fortune/Tax become `PotDeduction`s, and EV Cashout becomes a
  `PlayerAdjustment`);
- where the currency comes from: read from the file, or configured per source.

**Shared and site-neutral (never mention a site):**

- the `domain` module;
- `validate_hand`;
- the registry;
- storage;
- the pipeline, including stack continuity.

The only site names allowed outside site modules are `Platform` enum variants, plus their
registration in `main.rs`.

**Currencies:**

- Every hand carries a `CurrencyCode`, and every stored amount is in that hand's currency.
- Never add money across currencies. Group by `hands.currency`, or convert explicitly in a later
  reporting layer.
- Micro-units are millionths (`1.00 = 1,000,000`), so every `*_micros` column divides by
  `1e6`. Any currency with up to 6 decimal places fits; finer precision is rejected, never
  rounded.

**Sources.** A *source* is a site format plus whatever that format does not state about itself.
For GGPoker, that is the account currency, because GGPoker prints `$` for CAD and USD accounts
alike. Register one parser instance per source. If two registered parsers both recognize a file
(for example GGPoker-CAD and GGPoker-USD), the file is rejected as ambiguous. Keep exports from
different accounts in separate directories and run one import per directory.

---

## 2. Adding a new platform or source (checklist for coding agents)

1. **Collect real exports** for the new site and keep them verbatim as fixtures. Never write a
   pattern from documentation or memory alone.
2. **Add a `Platform` variant** with a stable `as_str()` value, and list it in the
   `stored_identifier!` call (a missing variant fails to compile). Never change existing values;
   they are stored in SQLite.
3. **Create `src/parser/<site>.rs`** implementing `HandParser`:
   - `recognizes` inspects only the opening line(s) and must not claim other sites' files;
   - override `split_blocks` if hands are not blank-line separated;
   - map site vocabulary onto canonical types. Each label the site prints must become a
     `PotDeduction` with the closest `DeductionKind`, keeping the original label;
   - read the currency from the file if the site prints it, otherwise take it as a constructor
     argument;
   - fill `utc_offset_seconds` if the site states a time zone;
   - set `is_all_in` on every contribution that commits the player's whole stack;
   - enforce the site's own observed line order and grammar (as GGPoker does for seat, blind,
     `Dealt to` and summary lines);
   - finish every successful parse with `validate_hand`.
4. **Extend canonical enums only with site-neutral names** when the site has a genuinely new
   concept (e.g. `AdjustmentKind::AllInInsurancePremium`).
   - Kind columns are free text, validated by the Rust enums, so a new variant needs **no**
     schema change.
   - Any other schema change is a new SQL string appended to `MIGRATIONS` in `schema.rs`.
     Never edit a released migration.
5. **Re-export the parser** from `src/parser/mod.rs` and add its CLI option in `src/main.rs`.
6. **Tests:**
   - parse every real fixture hand and assert results;
   - reconcile a whole session (chip conservation and stack continuity, as in
     `tests/real_session.rs`), and import it with no stack warnings;
   - fault-inject the new formats, including the single-line mutations of §0;
   - show that existing parsers do not recognize the new site's files, and vice versa.

---

## 3. Format-coverage policy

1. A line pattern is supported only if it has been observed in a real export.
   - Anything else must produce an error (`UnrecognizedLine`, `UnsupportedGame`,
     `UnsupportedAdjustment`, …). It is never silently skipped.
   - A rejected hand is reported individually, never aborts an import, and is kept with its raw
     text in `quarantined_hands`. After the parser learns the format, re-importing the same
     folder stores it.
2. To add a format:
   - add a real hand containing it, verbatim, to `tests/common/mod.rs`;
   - write a test asserting the parsed result;
   - then extend the parser.
3. Every parsed hand passes `validate_hand`, and storage re-validates before writing.
4. Money is `rust_decimal::Decimal` in memory and integer micro-units in SQLite. No floating
   point anywhere.

---

## 4. Milestones, file variants and verification gates

**File-variant rule.** A `## FILE:` header has the form `## FILE: <path> · Milestone <N>`.

- At milestone *M*, create every path whose milestone is ≤ *M*.
- Some paths appear more than once (`src/lib.rs`, `src/parser/mod.rs`). For those, use the
  variant with the highest milestone ≤ *M*.
- Do not create files from later milestones early.

**Extract, never retype.** The `tools/blueprint` tool (std only, shipped next to this document)
applies the rule above:

```
cargo run -q --manifest-path tools/blueprint/Cargo.toml -- extract <M>   # write milestone M
cargo run -q --manifest-path tools/blueprint/Cargo.toml -- check <M>     # report any drift
```

`check` lists files that are missing, differ from this document, or belong to a later
milestone, and exits with code 1 if there is any.

**Lockfile.** Copy the shipped `Cargo.lock` to the repository root before the first build and
pass `--locked` to every gate. `Cargo.toml` is the same at every milestone, so one lockfile
serves all five.

**Real-data fixture (required from Milestone 3).** Copy the GGPoker export
`GG20260821-2055 - NLHRed15 - 0.05 - 0.1 - 6max.txt` byte-for-byte to
`tests/fixtures/ggpoker/NLHRed15_2026-08-21.txt`.

- It is loaded with `include_str!`.
- `tests/real_session.rs` asserts exact totals from it and its SHA-256
  (`78d1e036…7b05e4`, 331,441 bytes), so a changed line ending or trailing space fails the gate.
- It came from a CAD account; `tests/common/mod.rs` records that as `FIXTURE_CURRENCY`.

| Milestone | `src/lib.rs` exports | Files added at this milestone | Gate |
| :--- | :--- | :--- | :--- |
| **1** | `domain` | `Cargo.toml`, `src/lib.rs`, `src/domain/{mod,money,card,canonical}.rs` | test · clippy · fmt |
| **2** | `domain`, `parser` | `src/lib.rs` (M2), `src/parser/mod.rs` (M2), `src/parser/{base,registry}.rs` | test · clippy · fmt |
| **3** | `domain`, `parser` | `src/parser/mod.rs` (M3), `src/parser/ggpoker.rs`, `tests/common/mod.rs`, `tests/ggpoker_parser.rs`, `tests/real_session.rs`, fixture | test · clippy · fmt |
| **4** | `domain`, `parser`, `storage` | `src/lib.rs` (M4), `src/storage/{mod,schema,repository}.rs`, `tests/storage.rs` | test · clippy · fmt |
| **5** | `domain`, `parser`, `pipeline`, `storage` | `src/lib.rs` (M5), `src/pipeline/{mod,continuity,ingester}.rs`, `src/main.rs`, `tests/pipeline.rs`, `tests/cli.rs` | test · clippy · fmt |

Expected test counts when the gate passes: **M1 11 · M2 39 · M3 80 · M4 95 · M5 104.**

**Running the CLI:**

```
poker_tracker <input-dir> --ggpoker-currency CAD [--db poker_tracker.sqlite]   # import
poker_tracker --delete-run <RUN-ID> [--db poker_tracker.sqlite]                # undo an import
poker_tracker --help
```

Each import prints its `Batch Run ID`, the counts (ingested, duplicates, failed, quarantined),
and any file errors, hand failures and stack warnings (on stderr). The currency code is accepted
in any letter case.

Exit codes:

| Code | Meaning |
| :--- | :--- |
| `0` | All files and hands ingested (stack warnings do not change this), or `--help` |
| `1` | Fatal error (database cannot be opened or is locked, unknown run id, missing database for `--delete-run`) |
| `2` | Usage error, including an input directory that does not exist; nothing is opened or created |
| `3` | Run completed, but some files or hands were rejected, or no `.txt` file was found |

---

## 5. Contracts vs implementation details

**Strict contracts (must match exactly):**

- **Public API:** all public types, field names, enum variants and function/trait signatures in
  - `src/domain/`;
  - `src/parser/base.rs` and `src/parser/registry.rs`;
  - the public API of `src/parser/ggpoker.rs`: `GgPokerParser::{new, currency}` and its
    `HandParser` impl;
  - `src/storage/` and `src/pipeline/`.
- **Object safety:** `HandParser` must stay usable as `dyn HandParser`. Add new trait methods
  with default implementations where possible.
- **Stable identifiers:** the strings returned by every `as_str()` (`Platform`, `GameType`,
  `BettingLimit`, `ActionType`, `DeductionKind`, `AdjustmentKind`) and `Street::code` are stored
  values. They may be added to, never changed; `FromStr` and `Street::from_code` are their exact
  inverses.
- **Schema:** the migrations in `src/storage/schema.rs`.
  - `MIGRATIONS` is append-only; a released entry is never edited or reordered.
  - `SCHEMA_VERSION = MIGRATIONS.len()` (currently 2), and `APPLICATION_ID = 0x504B5452`.
  - Opening a database identifies it first (§7), applies pending migrations, each in its own
    transaction, and refuses a database from a newer build.
- **Validator:** every invariant listed in section 6, enforced by the standalone
  `validate_hand(&CanonicalHand) -> Result<(), ParseError>`. Every parser and every
  `SqliteRepository` insert call it.
- **Money:**
  - lossless `decimal_to_micro_units(Decimal) -> Option<i64>` with `MICRO_UNIT_SCALE =
    1_000_000`, rejecting precision below `0.000001`;
  - `micro_units_to_decimal` returns normalised values (`18.6`, not `18.600000`);
  - `CurrencyCode` accepts exactly three uppercase ASCII letters (the CLI upper-cases its
    option first);
  - `Card` deserialization rejects raw values outside `0..52`.
- **Per-player money:**
  - `net = won − invested + adjustments`;
  - `invested` excludes returned uncalled bets;
  - `adjustments` is the signed sum of the player's `PlayerAdjustment`s.
- **Raw text:** `normalized_hand_text` and `hand_text_sha256` define the stored fingerprint;
  changing them would turn every re-import into a content conflict.

**Implementation details (free to reorganize):** private helpers and state structs inside site
parser modules, `src/storage/repository.rs` and `src/pipeline/`. They must keep to edition-2024
idioms (`let`-chains, `std::sync::LazyLock`, precise capturing `+ use<'_>` wherever a returned
`impl Trait` borrows its arguments) and the zero-panic lints.

---

## 6. Invariants enforced by `validate_hand` (site-independent)

**Header:**

- non-empty hand id and table name;
- `2 ≤ max_seats ≤ 10`, and the button seat is within the table;
- `0 < small_blind ≤ big_blind`;
- if a UTC offset is present, it is within ±18 h.

**Players:**

- between 2 and `max_seats` players;
- unique seats and names;
- positive starting stacks;
- hole cards in canonical slot order;
- **exactly one hero**, holding two known cards.

**Actions:**

- Sequences are `0..n` in order.
- Streets never go backwards, and no action is recorded on `Showdown`.
- Fold and Check carry no amount; every other action carries a positive amount.
- The `street_total_after` chain holds:
  - blinds, calls, bets and raises add their amount;
  - an uncalled-bet return subtracts it;
  - dead blinds, folds and checks leave it unchanged.
- A player never commits more than their starting stack.
- A contribution is flagged all-in **exactly** when it commits the player's whole stack.
- No player acts after folding.

**Betting replay** (No-Limit; seats clockwise by seat number):

- Blinds come first: at most one small blind and at least one big blind. A live blind equals the
  stake (the small or the big blind), or less only when the poster is all-in. Dead money is at
  most one big blind.
- Preflop action starts left of the first big blind poster; later streets start left of the
  button.
- Each action belongs to the next player clockwise who still owes an action. A player who
  folded or is all-in owes none.
- A round closes only when everyone who can act has acted. A bet or raise makes every other
  player who can act owe an action again. A lone player who can still act and already matches
  the highest commitment owes nothing (for example the big blind after a short all-in call).
- Legality:
  - a check only when not facing a bet;
  - a call matches the bet faced, or puts the player all-in for less;
  - a bet only when no bet is faced, of at least the big blind unless all-in;
  - a raise only when facing a bet, above it by at least the last full bet or raise (initially
    the big blind), or completing a smaller all-in to the big blind, unless all-in;
  - a bet or raise only when another player can still act;
  - a player who already acted on the street may raise again only when facing at least a full
    raise since then, so an all-in for less than a full raise does not reopen the betting (a
    check keeps the right to raise).
- An uncalled bet is returned only after betting closed, to the player with the largest
  commitment, and equals exactly the part nobody matched.
- Actions occur only on streets whose cards were dealt.
- No street is dealt after all but one player folded, and a hand still contested by two or more
  players reaches the river.

**Outcome:**

- **Summary arithmetic:** `total_pot − Σ deductions = Σ payouts`.
- **Chip conservation:** `Σ contributions − Σ uncalled returns = total_pot`.
- **Side pots:** for every winner's live contribution `L`, the players whose live contribution
  is at most `L` are paid at most `Σ_all players min(live contribution, L) + Σ dead money`.
- Deductions are nonnegative and labelled.
- `uncalled_returned` equals the sum of the returned-bet actions.
- Every payout is positive and goes to a player who did not fold.
- Single-board hands pay on run 0. Multi-run hands pay on runs `1..=n`, and every runout pays at
  least once.
- Adjustments:
  - reference seated players;
  - are nonzero and representable;
  - have the sign their kind requires;
  - are never triggered at showdown.
- Every player's net converts losslessly to micro-units.

**Boards:**

- Turn requires a flop; river requires a turn.
- Multi-run boards are runs `1..=n` with `2 ≤ n ≤ MAX_RUNOUTS` (3), each dealt to the river.
- No card appears twice among the hole cards and any single runout.
- No card is dealt twice across runouts; cards shared with the first runout are counted once.

**Site parsers additionally enforce their own format rules.** For GGPoker:

- seat lines come first, in ascending seat order; the small blind is the first blind posted;
- blinds may be posted only before `*** HOLE CARDS ***`; a blind that uses the poster's whole
  stack is recorded as all-in (GGPoker prints no suffix there);
- exactly one `Dealt to` line per seated player, in seat order, directly after
  `*** HOLE CARDS ***`; only Hero's line carries cards;
- check, call, bet and raise legality, and `raises $x to $y` requires `facing + x = y`;
- an EV Cashout is chosen at most once per player and never once the river is dealt;
- showdown headers come in run order;
- one pattern per verb, so an amount is accepted only where GGPoker prints one; before
  `*** HOLE CARDS ***` only seat and blind lines are accepted;
- the summary section must match the hand body:
  - per-seat winnings and cashout risk;
  - board lines (for multi-run hands, a suffix of each runout);
  - the stated run count;
  - every seat line follows the observed grammar: `folded before Flop [(didn't bet)]`,
    `folded on the <street>`, `won|collected ($x)`, or
    `showed [c1 c2] and <outcome>[, and <outcome>]…[, Cashout Risk ($x)]`, where each outcome is
    `won ($x)` or `lost`, optionally followed by `with <description>` made only of words (and at
    most one comma), so no amount can hide in it;
  - `(button)`, `(small blind)` and `(big blind)` labels match the button seat, the small blind
    poster and the first big blind poster;
  - the fold street matches the body;
  - `(didn't bet)` means the player put in no chips;
  - shown cards equal the player's known cards, with one outcome per runout;
  - a player still in a contested hand must show; an uncontested winner may show or not.

---

## 7. Storage and import semantics

**Identity and upgrades.** `SqliteRepository::open` reads `PRAGMA application_id` and
`user_version` before writing anything:

| File | Result |
| :--- | :--- |
| No tables, id 0, version 0 | New database: all migrations are applied |
| `application_id = 0x504B5452` | `poker_tracker` database: pending migrations are applied; a newer version is `UnsupportedSchemaVersion` |
| id 0, version 1, with `hands` and `import_runs` | v3.4 database: migration 2 upgrades it in place (no re-import) |
| Anything else | `NotAPokerTrackerDatabase`; the file is not modified (v3.3 databases are told to delete and re-import) |

Only then are the WAL and other pragmas applied. Each migration runs under `BEGIN IMMEDIATE`
and re-reads the version first, so two processes opening the same file never apply a step
twice. `open_existing` (used by `--delete-run`) refuses to create a missing file.

**Duplicates and conflicts.** The key is `(platform, site_hand_id)`. A hand already stored is:

- a **duplicate** if it has the same currency and the same content; content is the SHA-256 of
  `normalized_hand_text` (LF line endings, no trailing whitespace or blank lines) when both
  imports carry raw text, otherwise the canonical hand read back with `load_hand`;
- `CurrencyConflict` if the currency differs;
- `ContentConflict` if the content differs.

A duplicate fills in missing raw text. Delete the earlier import run to replace a conflicting
hand.

**Provenance and undo.** `hands.import_run_id/source_file/source_line` name the run that stored
the hand; `hand_sightings` lists every run (with file and line) that contained it, duplicates
included. `delete_import_run(run)` deletes the hands `run` stored that no other run contained,
keeps all other hands (re-pointing the provenance of those `run` stored to the earliest
remaining run), removes the run's quarantine entries and the run, and reports all three counts
in `DeletedImportRun`. A hand stored without a source is never deleted by an undo.
`import_runs.started_at` is the time the import started.

**Quarantine.** The pipeline stores every hand it could not store, parse and storage rejections
alike, in `quarantined_hands` with its raw text, parser, file, line, stage and error, once per
run and text. Entries stay until their run is deleted. The `quarantine_pending` view lists the
entries whose text is not stored as a hand, so a hand stored later drops off the list, and
comes back if that later import is undone.

**Batches and locks.** Each batch is one `BEGIN IMMEDIATE` transaction with a savepoint per
hand. Validation, conversion, conflict and constraint errors reject that hand only; lock, I/O
and other database-wide errors roll the batch back and stop the import (`PipelineError`, exit
1). Writes wait up to `DEFAULT_BUSY_TIMEOUT` (5 s) for another connection's lock.

**Stack continuity.** For each file, consecutive hands at the same table (ordered by
timestamp) are compared: every player seated in both must start the later hand with at least
the stack they ended the earlier one with (`starting + net`). A violation is a `StackWarning`,
not a failure. Pairs with equal timestamps, or with an unparsed block between them in the file,
are skipped.

---

## 8. Known limitations (deliberately deferred)

- **Derived statistics** (positions, VPIP/PFR, all-in EV, bb/100) belong to a later stats layer.
  The schema stores everything they need. Compute bb-normalized figures per currency, or convert
  explicitly.
- **Currency conversion** (e.g. CAD and USD results combined) is a reporting concern. Storage
  never mixes currencies.
- **Game variants.** The model is Hold'em: two hole-card slots, NL cash. Omaha, other limits and
  tournaments are the main future model extensions: hole-card arity, `GameType` and
  `BettingLimit` variants, and chips rather than currency.
- **EV Cashout, receiving side.** Only `Pays Cashout Risk ($x)` has been observed. A cashout
  taken by a player who then loses the pot is rejected with `UnsupportedAdjustment` (and
  quarantined) until a real sample is added; it would become a new `AdjustmentKind`.
- **All-in blinds for less than the stake.** The validator accepts them, but most sites make
  the others call the full big blind, which the replay does not model; the GGPoker parser
  refuses such hands until a real sample shows how GGPoker prints them. A blind of exactly the
  remaining stack is supported.
- **Heads-up hands** are not in the reference export. The label rule expects the button's
  summary line to read `(button) (small blind)`.
- **Side-pot structure.** GGPoker prints every payout as `collected $x from pot`. The validator
  bounds what each player can win, but main and side pot amounts, if ever needed, must be
  computed from contributions.
- **Not yet observed on GGPoker** (rejected until a sample is added):
  - straddles, antes or bomb pots;
  - `mucks hand`, seat-change and sit-out lines;
  - non-`$` stakes;
  - Omaha and other limits.
- **Timestamps** from GGPoker carry no time zone (`utc_offset_seconds = None`), so ordering
  hands across different sites is approximate until a zone is configured or stated.
- **Player identity.** The `players` table is keyed by `(platform, screen_name)`.
  - GGPoker anonymizes opponents with hashes; verify they are stable across sessions before
    building opponent-level features.
  - Linking the hero's accounts across sites belongs to a future accounts layer.
- **Re-parsing from the database.** Raw text is stored for every hand and every quarantined
  hand, but there is no command yet that re-parses it; re-import the original folder instead.
- **Schema history.** Migration 1 is the v3.4 baseline and migration 2 the v3.5 upgrade. A v3.4
  database upgrades in place; databases created by v3.3-or-earlier code must be deleted and
  re-imported once.
- **The betting replay assumes No-Limit.** Pot-Limit and Fixed-Limit support would add
  bet-size rules.

---

## FILE: Cargo.toml · Milestone 1

```toml
[package]
name = "poker_tracker"
version = "0.5.0"
edition = "2024"
# Minimum supported Rust: 1.88 (first release with stable `let`-chains in edition 2024).
# Verified toolchain for this blueprint: rustc 1.97.0.
rust-version = "1.88"
authors = ["djyoc"]
description = "A fast, modular post-session poker tracker and parser"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
rust_decimal = { version = "1.36", features = ["serde", "std"] }
regex = "1.11"
thiserror = "2.0"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.10", features = ["v7"] }
rusqlite = { version = "0.32", features = ["bundled"] }
sha2 = "0.10"
walkdir = "2.5"

[dev-dependencies]
pretty_assertions = "1.4"
rust_decimal_macros = "1.36"
serde_json = "1.0"
tempfile = "3.13"

[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
float_arithmetic = "deny"
unwrap_used = "deny"
expect_used = "deny"
dbg_macro = "warn"
todo = "warn"

[profile.release]
lto = "thin"
codegen-units = 1
strip = "symbols"
```

---

## FILE: src/lib.rs · Milestone 1

```rust
//! `poker_tracker` core library.
//!
//! Site-independent domain model and validator, pluggable site parsers selected per file,
//! transactional SQLite storage, and a deterministic batch ingestion pipeline.

pub mod domain;
```

---

## FILE: src/lib.rs · Milestone 2

```rust
//! `poker_tracker` core library.
//!
//! Site-independent domain model and validator, pluggable site parsers selected per file,
//! transactional SQLite storage, and a deterministic batch ingestion pipeline.

pub mod domain;
pub mod parser;
```

---

## FILE: src/lib.rs · Milestone 4

```rust
//! `poker_tracker` core library.
//!
//! Site-independent domain model and validator, pluggable site parsers selected per file,
//! transactional SQLite storage, and a deterministic batch ingestion pipeline.

pub mod domain;
pub mod parser;
pub mod storage;
```

---

## FILE: src/lib.rs · Milestone 5

```rust
//! `poker_tracker` core library.
//!
//! Site-independent domain model and validator, pluggable site parsers selected per file,
//! transactional SQLite storage, and a deterministic batch ingestion pipeline.

pub mod domain;
pub mod parser;
pub mod pipeline;
pub mod storage;
```

---

## FILE: src/domain/mod.rs · Milestone 1

```rust
//! Site-independent domain types and zero-cost poker primitives.

pub mod canonical;
pub mod card;
pub mod money;

pub use canonical::{
    ActionType, AdjustmentKind, BettingLimit, CanonicalHand, CommunityCards, DeductionKind,
    GameType, HandHeader, HandOutcome, MAX_RUNOUTS, Platform, Player, PlayerAdjustment,
    PlayerResult, PotDeduction, PotWinner, Street, StreetAction, UnknownIdentifier,
};
pub use card::{Card, CardParseError, Rank, Suit};
pub use money::{
    CurrencyCode, CurrencyParseError, MICRO_UNIT_DECIMALS, MICRO_UNIT_SCALE,
    decimal_to_micro_units, micro_units_to_decimal,
};
```

---

## FILE: src/domain/money.rs · Milestone 1

```rust
//! Currency codes and lossless fixed-point money conversion.
//!
//! Amounts are `rust_decimal::Decimal` in memory and integer micro-units (millionths of the
//! currency unit) in storage. Amounts in different currencies are never combined; every hand
//! carries its own [`CurrencyCode`].

use std::fmt;
use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// Fixed-point integer scaling factor for storage: one micro-unit is one millionth of the
/// currency unit (`1.00 = 1,000,000`), so every `*_micros` column divides by `1e6`.
///
/// Supports any currency with up to 6 decimal places. An amount with finer precision is
/// rejected (never rounded) by [`decimal_to_micro_units`].
pub const MICRO_UNIT_SCALE: i64 = 1_000_000;

/// Number of decimal places represented by one micro-unit.
pub const MICRO_UNIT_DECIMALS: u32 = 6;

/// Losslessly converts a [`Decimal`] amount into integer micro-units (`1.00 = 1,000,000`).
///
/// Returns `None` if `value * 1,000,000` overflows, retains a non-zero fractional part, or
/// overflows `i64`.
#[must_use]
pub fn decimal_to_micro_units(value: Decimal) -> Option<i64> {
    let scaled = value.checked_mul(Decimal::from(MICRO_UNIT_SCALE))?;
    if !scaled.fract().is_zero() {
        return None;
    }
    scaled.try_into().ok()
}

/// Converts 64-bit integer micro-units (`1.00 = 1,000,000`) back into a [`Decimal`], with
/// trailing zeros removed (`18_600_000` becomes `18.6`).
#[must_use]
pub fn micro_units_to_decimal(micros: i64) -> Decimal {
    Decimal::new(micros, MICRO_UNIT_DECIMALS).normalize()
}

/// Error returned for a malformed currency code.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("invalid currency code '{0}': expected 3 uppercase ASCII letters (ISO 4217, e.g. CAD)")]
pub struct CurrencyParseError(pub String);

/// ISO-4217-style currency code (three uppercase ASCII letters), valid by construction.
///
/// Where a hand's currency comes from is a parser concern: some sites print it in the hand
/// history, others (GGPoker) print only a symbol, so the account currency is configured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CurrencyCode([u8; 3]);

impl CurrencyCode {
    /// Canadian dollar.
    pub const CAD: Self = Self(*b"CAD");
    /// United States dollar.
    pub const USD: Self = Self(*b"USD");
    /// Euro.
    pub const EUR: Self = Self(*b"EUR");
    /// Pound sterling.
    pub const GBP: Self = Self(*b"GBP");

    /// Builds a code from three bytes, returning `None` unless all are uppercase ASCII letters.
    #[must_use]
    pub const fn from_bytes(code: [u8; 3]) -> Option<Self> {
        if code[0].is_ascii_uppercase()
            && code[1].is_ascii_uppercase()
            && code[2].is_ascii_uppercase()
        {
            Some(Self(code))
        } else {
            None
        }
    }

    /// The code as a string slice (e.g. `"CAD"`).
    #[must_use]
    pub fn as_str(&self) -> &str {
        // Always ASCII by construction.
        std::str::from_utf8(&self.0).unwrap_or("???")
    }
}

impl FromStr for CurrencyCode {
    type Err = CurrencyParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.as_bytes() {
            [a, b, c] => Self::from_bytes([*a, *b, *c]),
            _ => None,
        }
        .ok_or_else(|| CurrencyParseError(s.to_string()))
    }
}

impl fmt::Display for CurrencyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for CurrencyCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CurrencyCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn lossless_micro_unit_conversion_succeeds_for_6_decimal_places_or_fewer() {
        assert_eq!(MICRO_UNIT_SCALE, 1_000_000, "a micro-unit is one millionth");
        assert_eq!(decimal_to_micro_units(dec!(18.6)), Some(18_600_000));
        assert_eq!(decimal_to_micro_units(dec!(0.05)), Some(50_000));
        assert_eq!(decimal_to_micro_units(dec!(0.000001)), Some(1));
        assert_eq!(decimal_to_micro_units(dec!(0)), Some(0));
        assert_eq!(decimal_to_micro_units(dec!(-0.48)), Some(-480_000));
        assert_eq!(micro_units_to_decimal(18_600_000), dec!(18.6));
        assert_eq!(micro_units_to_decimal(18_600_000).to_string(), "18.6");
    }

    #[test]
    fn micro_unit_conversion_rejects_sub_micro_fractional_precision_and_overflow() {
        assert_eq!(decimal_to_micro_units(dec!(1.2345678)), None);
        assert_eq!(decimal_to_micro_units(dec!(0.0000001)), None);
        assert_eq!(decimal_to_micro_units(Decimal::MAX), None);
    }

    #[test]
    fn currency_codes_validate_and_round_trip() {
        assert_eq!("CAD".parse::<CurrencyCode>(), Ok(CurrencyCode::CAD));
        assert_eq!(CurrencyCode::USD.to_string(), "USD");
        for bad in ["cad", "CA", "CADX", "C$", "", "ÉUR"] {
            assert!(
                bad.parse::<CurrencyCode>().is_err(),
                "{bad} should be rejected"
            );
        }
        let json = serde_json::to_string(&CurrencyCode::CAD).unwrap();
        assert_eq!(json, "\"CAD\"");
        assert_eq!(
            serde_json::from_str::<CurrencyCode>(&json).unwrap(),
            CurrencyCode::CAD
        );
        assert!(serde_json::from_str::<CurrencyCode>("\"cad\"").is_err());
    }
}
```

---

## FILE: src/domain/card.rs · Milestone 1

```rust
//! Compact 1-byte playing card representation (`0..52`).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Playing card rank from `Two` (`0`) through `Ace` (`12`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Rank {
    Two = 0,
    Three = 1,
    Four = 2,
    Five = 3,
    Six = 4,
    Seven = 5,
    Eight = 6,
    Nine = 7,
    Ten = 8,
    Jack = 9,
    Queen = 10,
    King = 11,
    Ace = 12,
}

impl Rank {
    /// Converts a raw `0..=12` integer into a [`Rank`].
    #[must_use]
    pub const fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Two),
            1 => Some(Self::Three),
            2 => Some(Self::Four),
            3 => Some(Self::Five),
            4 => Some(Self::Six),
            5 => Some(Self::Seven),
            6 => Some(Self::Eight),
            7 => Some(Self::Nine),
            8 => Some(Self::Ten),
            9 => Some(Self::Jack),
            10 => Some(Self::Queen),
            11 => Some(Self::King),
            12 => Some(Self::Ace),
            _ => None,
        }
    }

    /// Parses an ASCII byte (`2..=9`, `T`, `J`, `Q`, `K`, `A`, case-insensitive) into a [`Rank`].
    #[must_use]
    pub const fn from_ascii(byte: u8) -> Option<Self> {
        match byte {
            b'2' => Some(Self::Two),
            b'3' => Some(Self::Three),
            b'4' => Some(Self::Four),
            b'5' => Some(Self::Five),
            b'6' => Some(Self::Six),
            b'7' => Some(Self::Seven),
            b'8' => Some(Self::Eight),
            b'9' => Some(Self::Nine),
            b'T' | b't' => Some(Self::Ten),
            b'J' | b'j' => Some(Self::Jack),
            b'Q' | b'q' => Some(Self::Queen),
            b'K' | b'k' => Some(Self::King),
            b'A' | b'a' => Some(Self::Ace),
            _ => None,
        }
    }

    /// Returns the canonical uppercase character for this [`Rank`].
    #[must_use]
    pub const fn to_char(self) -> char {
        match self {
            Self::Two => '2',
            Self::Three => '3',
            Self::Four => '4',
            Self::Five => '5',
            Self::Six => '6',
            Self::Seven => '7',
            Self::Eight => '8',
            Self::Nine => '9',
            Self::Ten => 'T',
            Self::Jack => 'J',
            Self::Queen => 'Q',
            Self::King => 'K',
            Self::Ace => 'A',
        }
    }
}

/// Playing card suit (`Clubs = 0`, `Diamonds = 1`, `Hearts = 2`, `Spades = 3`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Suit {
    Clubs = 0,
    Diamonds = 1,
    Hearts = 2,
    Spades = 3,
}

impl Suit {
    /// Converts a raw `0..=3` integer into a [`Suit`].
    #[must_use]
    pub const fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Clubs),
            1 => Some(Self::Diamonds),
            2 => Some(Self::Hearts),
            3 => Some(Self::Spades),
            _ => None,
        }
    }

    /// Parses an ASCII suit character (`c`, `d`, `h`, `s`, case-insensitive) into a [`Suit`].
    #[must_use]
    pub const fn from_ascii(byte: u8) -> Option<Self> {
        match byte {
            b'c' | b'C' => Some(Self::Clubs),
            b'd' | b'D' => Some(Self::Diamonds),
            b'h' | b'H' => Some(Self::Hearts),
            b's' | b'S' => Some(Self::Spades),
            _ => None,
        }
    }

    /// Returns the canonical lowercase character for this [`Suit`].
    #[must_use]
    pub const fn to_char(self) -> char {
        match self {
            Self::Clubs => 'c',
            Self::Diamonds => 'd',
            Self::Hearts => 'h',
            Self::Spades => 's',
        }
    }
}

/// Error returned when parsing an invalid card token.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("invalid card token: '{0}'")]
pub struct CardParseError(pub String);

/// Zero-cost 1-byte playing card (`0..52`), where bits `2..=5` encode [`Rank`]
/// and bits `0..=1` encode [`Suit`].
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Card(u8);

impl Card {
    /// Constructs a [`Card`] from a [`Rank`] and [`Suit`].
    #[must_use]
    pub const fn new(rank: Rank, suit: Suit) -> Self {
        Self(((rank as u8) << 2) | (suit as u8))
    }

    /// Constructs a [`Card`] from a raw `0..52` integer.
    #[must_use]
    pub const fn from_raw(raw: u8) -> Option<Self> {
        if raw < 52 { Some(Self(raw)) } else { None }
    }

    /// Returns the packed `0..52` integer representation.
    #[must_use]
    pub const fn raw(self) -> u8 {
        self.0
    }

    /// Extracts the [`Rank`] of this card.
    #[must_use]
    pub const fn rank(self) -> Rank {
        match Rank::from_u8(self.0 >> 2) {
            Some(rank) => rank,
            None => Rank::Two,
        }
    }

    /// Extracts the [`Suit`] of this card.
    #[must_use]
    pub const fn suit(self) -> Suit {
        match Suit::from_u8(self.0 & 0b11) {
            Some(suit) => suit,
            None => Suit::Clubs,
        }
    }
}

impl<'de> Deserialize<'de> for Card {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = u8::deserialize(deserializer)?;
        Self::from_raw(raw).ok_or_else(|| {
            serde::de::Error::custom(format!("invalid raw card value {raw}: expected 0..52"))
        })
    }
}

impl FromStr for Card {
    type Err = CardParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s.as_bytes();
        if let [r_byte, s_byte] = bytes
            && let Some(rank) = Rank::from_ascii(*r_byte)
            && let Some(suit) = Suit::from_ascii(*s_byte)
        {
            Ok(Self::new(rank, suit))
        } else {
            Err(CardParseError(s.to_string()))
        }
    }
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.rank().to_char(), self.suit().to_char())
    }
}

impl fmt::Debug for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Card({self})")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn all_52_cards_round_trip_and_pack_into_0_to_51() {
        let ranks = [
            '2', '3', '4', '5', '6', '7', '8', '9', 'T', 'J', 'Q', 'K', 'A',
        ];
        let suits = ['c', 'd', 'h', 's'];
        let mut seen_mask: u64 = 0;

        for r in ranks {
            for s in suits {
                let token = format!("{r}{s}");
                let card = Card::from_str(&token).expect("valid card token");
                assert_eq!(card.to_string(), token);
                assert!(card.raw() < 52);
                assert_eq!(Card::from_raw(card.raw()), Some(card));
                let bit = 1u64 << card.raw();
                assert_eq!(seen_mask & bit, 0, "duplicate raw card index");
                seen_mask |= bit;
            }
        }

        assert_eq!(seen_mask.count_ones(), 52);
        assert_eq!(Card::from_raw(52), None);
    }

    #[test]
    fn case_insensitive_parse_normalizes_display() {
        let card = Card::from_str("aH").expect("should parse mixed case");
        assert_eq!(card.rank(), Rank::Ace);
        assert_eq!(card.suit(), Suit::Hearts);
        assert_eq!(card.to_string(), "Ah");
    }

    #[test]
    fn rejects_invalid_card_strings() {
        for invalid in ["", "A", "10s", "1h", "AhAs", "Xz", "9x", "0d"] {
            assert!(
                Card::from_str(invalid).is_err(),
                "expected '{invalid}' to fail parsing"
            );
        }
    }

    #[test]
    fn serde_deserialization_enforces_0_to_51_invariant() {
        let c0: Card = serde_json::from_str("0").expect("0 is a valid card index");
        assert_eq!(c0.raw(), 0);

        let c51: Card = serde_json::from_str("51").expect("51 is a valid card index");
        assert_eq!(c51.raw(), 51);

        for invalid_json in ["52", "64", "255"] {
            assert!(
                serde_json::from_str::<Card>(invalid_json).is_err(),
                "expected raw value {invalid_json} to fail Card deserialization"
            );
        }
    }
}
```

---

## FILE: src/domain/canonical.rs · Milestone 1

```rust
//! Canonical, site-independent representation of a poker hand history.
//!
//! Every site parser produces this model, and storage and statistics consume only this model.
//! Site-specific vocabulary stays inside the site's parser; when a site has a concept the model
//! cannot express, extend the model with a site-neutral name (see `PotDeduction` and
//! `PlayerAdjustment`) rather than adding a site-specific field.

use std::fmt;
use std::str::FromStr;

use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::card::Card;
use crate::domain::money::CurrencyCode;

/// Maximum number of board runouts supported (run it once, twice or three times).
pub const MAX_RUNOUTS: usize = 3;

/// A stored identifier (an `as_str()` value or street code) that this build does not know.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("unknown {kind} identifier '{value}'")]
pub struct UnknownIdentifier {
    pub kind: &'static str,
    pub value: String,
}

/// Implements `ALL` and `FromStr` (the inverse of `as_str()`) for a stored identifier enum.
/// The variant list must be complete: a missing variant fails to compile.
macro_rules! stored_identifier {
    ($ty:ident, $kind:literal, [$($variant:ident),+ $(,)?]) => {
        impl $ty {
            /// Every variant, in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
        }

        const _: () = {
            // Compile-time completeness check for `ALL`.
            #[allow(dead_code)]
            const fn listed(v: $ty) {
                match v {
                    $($ty::$variant)|+ => {}
                }
            }
        };

        impl FromStr for $ty {
            type Err = UnknownIdentifier;

            /// Parses a stored `as_str()` value.
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::ALL
                    .iter()
                    .copied()
                    .find(|v| v.as_str() == s)
                    .ok_or_else(|| UnknownIdentifier {
                        kind: $kind,
                        value: s.to_string(),
                    })
            }
        }
    };
}

/// Poker network that produced a hand history. Add a variant together with its parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    GGPoker,
}

impl Platform {
    /// Stable identifier stored in SQLite. Never change an existing value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GGPoker => "GGPoker",
        }
    }
}

stored_identifier!(Platform, "platform", [GGPoker]);

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// High-level game format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameType {
    Cash,
}

impl GameType {
    /// Stable identifier stored in SQLite.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cash => "Cash",
        }
    }
}

stored_identifier!(GameType, "game type", [Cash]);

/// Betting structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BettingLimit {
    NoLimit,
}

impl BettingLimit {
    /// Stable identifier stored in SQLite.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoLimit => "NL",
        }
    }
}

stored_identifier!(BettingLimit, "betting limit", [NoLimit]);

/// Betting street inside a hand. `Showdown` is never used for player actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Street {
    Preflop = 0,
    Flop = 1,
    Turn = 2,
    River = 3,
    Showdown = 4,
}

impl Street {
    /// Integer code stored in SQLite (`0..=4`).
    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// The street with the given [`Street::code`], if any.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Preflop),
            1 => Some(Self::Flop),
            2 => Some(Self::Turn),
            3 => Some(Self::River),
            4 => Some(Self::Showdown),
            _ => None,
        }
    }
}

/// Player action or pot adjustment event on a street.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionType {
    /// Live small blind (counts toward the poster's street commitment).
    PostSmallBlind,
    /// Live big blind, including a big blind posted by a player entering out of position.
    PostBigBlind,
    /// Dead money (e.g. a missed small blind): enters the pot but does not count toward the
    /// poster's street commitment.
    PostDeadBlind,
    Fold,
    Check,
    Call,
    Bet,
    Raise,
    UncalledBetReturned,
}

impl ActionType {
    /// Stable identifier stored in SQLite.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PostSmallBlind => "PostSmallBlind",
            Self::PostBigBlind => "PostBigBlind",
            Self::PostDeadBlind => "PostDeadBlind",
            Self::Fold => "Fold",
            Self::Check => "Check",
            Self::Call => "Call",
            Self::Bet => "Bet",
            Self::Raise => "Raise",
            Self::UncalledBetReturned => "UncalledBetReturned",
        }
    }

    /// `true` if this action moves chips from the player's stack into the pot.
    #[must_use]
    pub const fn is_contribution(self) -> bool {
        matches!(
            self,
            Self::PostSmallBlind
                | Self::PostBigBlind
                | Self::PostDeadBlind
                | Self::Call
                | Self::Bet
                | Self::Raise
        )
    }

    /// `true` if this action's amount is added to the player's street commitment.
    #[must_use]
    pub const fn adds_to_street_commitment(self) -> bool {
        matches!(
            self,
            Self::PostSmallBlind | Self::PostBigBlind | Self::Call | Self::Bet | Self::Raise
        )
    }
}

stored_identifier!(
    ActionType,
    "action type",
    [
        PostSmallBlind,
        PostBigBlind,
        PostDeadBlind,
        Fold,
        Check,
        Call,
        Bet,
        Raise,
        UncalledBetReturned,
    ]
);

/// Metadata header for a single hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandHeader {
    pub platform: Platform,
    /// Hand identifier as printed by the site; unique per platform.
    pub site_hand_id: String,
    pub game_type: GameType,
    pub limit: BettingLimit,
    /// Currency of every amount in this hand.
    pub currency: CurrencyCode,
    pub small_blind: Decimal,
    pub big_blind: Decimal,
    /// Local timestamp exactly as printed by the site.
    pub timestamp: NaiveDateTime,
    /// UTC offset of `timestamp` in seconds, when the site states it (or the import is
    /// configured with it). `None` means unknown, so cross-site ordering is approximate.
    pub utc_offset_seconds: Option<i32>,
    pub table_name: String,
    pub max_seats: u8,
    pub button_seat: u8,
}

/// Seated player state within a hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub seat: u8,
    /// Screen name as printed by the site (may be anonymized, e.g. GGPoker hashes).
    pub name: String,
    pub starting_stack: Decimal,
    /// Known hole cards. Slots fill in order, so `[None, Some(_)]` never occurs. A single known
    /// card comes from a one-card show.
    pub hole_cards: [Option<Card>; 2],
    /// The account owner whose hand history this is (the player dealt known cards).
    pub is_hero: bool,
}

impl Player {
    /// Iterates over the hole cards known for this player.
    pub fn known_cards(&self) -> impl Iterator<Item = Card> + use<'_> {
        self.hole_cards.iter().flatten().copied()
    }

    /// Records a known hole card. Returns `false` if it conflicts with two cards already known.
    pub fn record_known_card(&mut self, card: Card) -> bool {
        match self.hole_cards {
            [Some(a), _] if a == card => true,
            [_, Some(b)] if b == card => true,
            [None, _] => {
                self.hole_cards = [Some(card), None];
                true
            }
            [Some(a), None] => {
                self.hole_cards = [Some(a), Some(card)];
                true
            }
            [Some(_), Some(_)] => false,
        }
    }
}

/// Sequential action recorded during a hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreetAction {
    pub sequence: u16,
    pub street: Street,
    pub player_name: String,
    pub action_type: ActionType,
    /// Incremental chips committed (or returned on `UncalledBetReturned`) in this specific action.
    pub amount: Decimal,
    /// Player's cumulative live chips committed on the current street after this action completes.
    pub street_total_after: Decimal,
    pub is_all_in: bool,
}

/// Community cards for a single board runout (`run_index = 0` for a single board,
/// `1..=MAX_RUNOUTS` for multi-run boards).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityCards {
    pub run_index: u8,
    pub flop: Option<[Card; 3]>,
    pub turn: Option<Card>,
    pub river: Option<Card>,
}

impl CommunityCards {
    /// All cards on this runout in deal order (flop, turn, river).
    pub fn cards(&self) -> impl Iterator<Item = Card> + use<'_> {
        self.flop
            .iter()
            .flatten()
            .copied()
            .chain(self.turn)
            .chain(self.river)
    }
}

/// Payout collected by a player. `run_index` is `0` for single-board hands and identifies the
/// runout for multi-run hands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PotWinner {
    pub player_name: String,
    pub amount: Decimal,
    pub run_index: u8,
}

/// Category of money the house takes out of the pot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeductionKind {
    /// Rake (the basis for rakeback and rake-paid statistics).
    Rake,
    /// Contribution to a jackpot.
    Jackpot,
    /// Any other site fee or promotion drop (keeps the site's label).
    Other,
}

impl DeductionKind {
    /// Stable identifier stored in SQLite.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rake => "Rake",
            Self::Jackpot => "Jackpot",
            Self::Other => "Other",
        }
    }
}

stored_identifier!(DeductionKind, "deduction kind", [Rake, Jackpot, Other]);

/// One amount the house took out of the pot, with the label the site printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PotDeduction {
    pub kind: DeductionKind,
    /// Site label, e.g. `"Rake"`, `"Jackpot"`, `"Bingo"`.
    pub label: String,
    pub amount: Decimal,
}

/// Category of money moving between a player and the site outside the pot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AdjustmentKind {
    /// EV Cashout taken, pot won afterwards: the player pays back the cashout risk
    /// (GGPoker `Pays Cashout Risk ($x)`). Always a debit.
    EvCashoutRiskPaid,
}

impl AdjustmentKind {
    /// Stable identifier stored in SQLite.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EvCashoutRiskPaid => "EvCashoutRiskPaid",
        }
    }

    /// `Some(true)` if amounts of this kind are always credits, `Some(false)` if always
    /// debits, `None` if either sign is possible.
    #[must_use]
    pub const fn is_credit(self) -> Option<bool> {
        match self {
            Self::EvCashoutRiskPaid => Some(false),
        }
    }
}

stored_identifier!(AdjustmentKind, "adjustment kind", [EvCashoutRiskPaid]);

/// Money credited to (positive) or debited from (negative) a player outside the pot.
/// Adjustments do not affect pot accounting but do affect the player's net result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerAdjustment {
    pub player_name: String,
    pub kind: AdjustmentKind,
    /// Street on which the adjustment was triggered (e.g. when an EV Cashout was chosen).
    pub street: Street,
    /// Signed amount: positive credits the player, negative debits them.
    pub amount: Decimal,
}

/// Financial summary and pot distribution at the conclusion of a hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandOutcome {
    pub total_pot: Decimal,
    /// Everything the house took out of the pot, in the order the site printed it.
    pub deductions: Vec<PotDeduction>,
    pub uncalled_returned: Decimal,
    pub winners: Vec<PotWinner>,
    pub adjustments: Vec<PlayerAdjustment>,
}

impl HandOutcome {
    /// Checked sum of all deductions, returning `None` on overflow.
    #[must_use]
    pub fn checked_total_deductions(&self) -> Option<Decimal> {
        self.deductions
            .iter()
            .try_fold(Decimal::ZERO, |acc, d| acc.checked_add(d.amount))
    }

    /// Checked sum of the deductions of one kind, returning `None` on overflow.
    #[must_use]
    pub fn checked_deductions_of(&self, kind: DeductionKind) -> Option<Decimal> {
        self.deductions
            .iter()
            .filter(|d| d.kind == kind)
            .try_fold(Decimal::ZERO, |acc, d| acc.checked_add(d.amount))
    }

    /// Checked sum of all payouts collected by winners, returning `None` on overflow.
    #[must_use]
    pub fn checked_total_collected(&self) -> Option<Decimal> {
        self.winners
            .iter()
            .try_fold(Decimal::ZERO, |acc, w| acc.checked_add(w.amount))
    }

    /// Checked signed sum of all player adjustments, returning `None` on overflow.
    #[must_use]
    pub fn checked_total_adjustments(&self) -> Option<Decimal> {
        self.adjustments
            .iter()
            .try_fold(Decimal::ZERO, |acc, a| acc.checked_add(a.amount))
    }
}

/// Per-player financial result of a hand, in the hand's currency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerResult {
    pub player_name: String,
    /// Chips moved into the pot before uncalled bets were returned.
    pub gross_committed: Decimal,
    /// Chips that stayed in the pot (`gross_committed - returned uncalled bets`).
    pub invested: Decimal,
    /// Sum of all pot payouts to this player.
    pub won: Decimal,
    /// Signed sum of this player's [`PlayerAdjustment`]s.
    pub adjustments: Decimal,
    /// `won - invested + adjustments`.
    pub net: Decimal,
}

/// Complete, validated canonical representation of a single poker hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalHand {
    pub header: HandHeader,
    pub players: Vec<Player>,
    pub actions: Vec<StreetAction>,
    pub boards: Vec<CommunityCards>,
    pub outcome: HandOutcome,
}

impl CanonicalHand {
    /// Number of runouts: `0` if no board was dealt, `1` for a single board, `2..` for multi-run.
    #[must_use]
    pub fn run_count(&self) -> usize {
        if self.boards.iter().any(|b| b.run_index > 0) {
            self.boards.len()
        } else {
            usize::from(!self.boards.is_empty())
        }
    }

    /// Checked sum of all chips that stayed in the pot (`contributions - uncalled returns`).
    #[must_use]
    pub fn checked_total_contributed(&self) -> Option<Decimal> {
        self.actions
            .iter()
            .try_fold(Decimal::ZERO, |acc, a| match a.action_type {
                ActionType::UncalledBetReturned => acc.checked_sub(a.amount),
                t if t.is_contribution() => acc.checked_add(a.amount),
                _ => Some(acc),
            })
    }

    /// Checked per-player results in seat order, returning `None` on arithmetic overflow.
    #[must_use]
    pub fn checked_player_results(&self) -> Option<Vec<PlayerResult>> {
        self.players
            .iter()
            .map(|p| {
                let mut gross = Decimal::ZERO;
                let mut returned = Decimal::ZERO;
                for a in self.actions.iter().filter(|a| a.player_name == p.name) {
                    if a.action_type == ActionType::UncalledBetReturned {
                        returned = returned.checked_add(a.amount)?;
                    } else if a.action_type.is_contribution() {
                        gross = gross.checked_add(a.amount)?;
                    }
                }
                let won = self
                    .outcome
                    .winners
                    .iter()
                    .filter(|w| w.player_name == p.name)
                    .try_fold(Decimal::ZERO, |acc, w| acc.checked_add(w.amount))?;
                let adjustments = self
                    .outcome
                    .adjustments
                    .iter()
                    .filter(|a| a.player_name == p.name)
                    .try_fold(Decimal::ZERO, |acc, a| acc.checked_add(a.amount))?;
                let invested = gross.checked_sub(returned)?;
                let net = won.checked_sub(invested)?.checked_add(adjustments)?;
                Some(PlayerResult {
                    player_name: p.name.clone(),
                    gross_committed: gross,
                    invested,
                    won,
                    adjustments,
                    net,
                })
            })
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use std::str::FromStr;

    #[test]
    fn record_known_card_fills_slots_in_order_and_detects_conflicts() {
        let c = |s: &str| Card::from_str(s).unwrap();
        let mut p = Player {
            seat: 1,
            name: "p".into(),
            starting_stack: dec!(10),
            hole_cards: [None, None],
            is_hero: false,
        };
        assert!(p.record_known_card(c("Jc")));
        assert_eq!(p.hole_cards, [Some(c("Jc")), None]);
        assert!(
            p.record_known_card(c("Jc")),
            "re-showing a known card is fine"
        );
        assert!(p.record_known_card(c("Kd")));
        assert_eq!(p.hole_cards, [Some(c("Jc")), Some(c("Kd"))]);
        assert!(p.record_known_card(c("Kd")));
        assert!(
            !p.record_known_card(c("2s")),
            "a third distinct card conflicts"
        );
    }

    #[test]
    fn action_type_contribution_semantics() {
        assert!(ActionType::PostDeadBlind.is_contribution());
        assert!(!ActionType::PostDeadBlind.adds_to_street_commitment());
        assert!(ActionType::Raise.adds_to_street_commitment());
        assert!(!ActionType::UncalledBetReturned.is_contribution());
        assert!(!ActionType::Check.is_contribution());
    }

    #[test]
    fn stored_identifiers_round_trip_and_unknown_ones_are_rejected() {
        fn round_trip<T>(all: &[T], as_str: fn(T) -> &'static str) -> usize
        where
            T: Copy + PartialEq + fmt::Debug + FromStr<Err = UnknownIdentifier>,
        {
            for v in all {
                assert_eq!(as_str(*v).parse::<T>(), Ok(*v));
            }
            all.len()
        }
        assert_eq!(round_trip(Platform::ALL, Platform::as_str), 1);
        assert_eq!(round_trip(GameType::ALL, GameType::as_str), 1);
        assert_eq!(round_trip(BettingLimit::ALL, BettingLimit::as_str), 1);
        assert_eq!(round_trip(ActionType::ALL, ActionType::as_str), 9);
        assert_eq!(round_trip(DeductionKind::ALL, DeductionKind::as_str), 3);
        assert_eq!(round_trip(AdjustmentKind::ALL, AdjustmentKind::as_str), 1);
        for code in 0..=4 {
            assert_eq!(Street::from_code(code).map(Street::code), Some(code));
        }
        assert_eq!(Street::from_code(5), None);
        assert_eq!(
            "Shove".parse::<ActionType>(),
            Err(UnknownIdentifier {
                kind: "action type",
                value: "Shove".into()
            })
        );
        assert!(
            "ggpoker".parse::<Platform>().is_err(),
            "identifiers are case-sensitive"
        );
    }

    #[test]
    fn deduction_and_adjustment_totals() {
        let outcome = HandOutcome {
            total_pot: dec!(3.05),
            deductions: vec![
                PotDeduction {
                    kind: DeductionKind::Rake,
                    label: "Rake".into(),
                    amount: dec!(0.14),
                },
                PotDeduction {
                    kind: DeductionKind::Jackpot,
                    label: "Jackpot".into(),
                    amount: dec!(0.1),
                },
                PotDeduction {
                    kind: DeductionKind::Other,
                    label: "Bingo".into(),
                    amount: dec!(0),
                },
            ],
            uncalled_returned: dec!(0),
            winners: vec![],
            adjustments: vec![PlayerAdjustment {
                player_name: "p".into(),
                kind: AdjustmentKind::EvCashoutRiskPaid,
                street: Street::Turn,
                amount: dec!(-0.48),
            }],
        };
        assert_eq!(outcome.checked_total_deductions(), Some(dec!(0.24)));
        assert_eq!(
            outcome.checked_deductions_of(DeductionKind::Rake),
            Some(dec!(0.14))
        );
        assert_eq!(outcome.checked_total_adjustments(), Some(dec!(-0.48)));
        assert_eq!(AdjustmentKind::EvCashoutRiskPaid.is_credit(), Some(false));
    }
}
```

---

## FILE: src/parser/mod.rs · Milestone 2

```rust
//! Site-independent parser contract, parser registry, and site-specific parsers.
//!
//! Layout: `base` (trait, validator, splitter) and `registry` are shared by every site.
//! Each site lives in its own module (`ggpoker`, ...) and is the only place that knows
//! that site's text format.

pub mod base;
pub mod registry;

pub use base::{
    HandBlock, HandParser, ParseError, ParsedBlocks, first_content_line, split_hand_blocks,
    validate_hand,
};
pub use registry::{DetectionError, ParserRegistry};
```

---

## FILE: src/parser/mod.rs · Milestone 3

```rust
//! Site-independent parser contract, parser registry, and site-specific parsers.
//!
//! Layout: `base` (trait, validator, splitter) and `registry` are shared by every site.
//! Each site lives in its own module (`ggpoker`, ...) and is the only place that knows
//! that site's text format.

pub mod base;
pub mod ggpoker;
pub mod registry;

pub use base::{
    HandBlock, HandParser, ParseError, ParsedBlocks, first_content_line, split_hand_blocks,
    validate_hand,
};
pub use ggpoker::GgPokerParser;
pub use registry::{DetectionError, ParserRegistry};
```

---

## FILE: src/parser/base.rs · Milestone 2

```rust
//! Site-independent parser trait, hand-block splitter, and the canonical-hand validator.

use std::collections::{HashMap, HashSet};

use rust_decimal::Decimal;
use thiserror::Error;

use crate::domain::canonical::{
    ActionType, CanonicalHand, MAX_RUNOUTS, Platform, Player, PlayerResult, Street,
};
use crate::domain::card::Card;
use crate::domain::money::decimal_to_micro_units;

/// Structured errors emitted during hand history parsing and validation.
///
/// `line` fields are 1-based line numbers relative to the start of the hand block; add
/// [`HandBlock::start_line`] `- 1` to get the line number in the source file.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ParseError {
    #[error("hand block is empty")]
    EmptyHandBlock,

    #[error("invalid hand header line: '{line}'")]
    InvalidHeader { line: String },

    #[error("game, limit or stakes format not supported by this parser: '{line}'")]
    UnsupportedGame { line: String },

    #[error("invalid table header line: '{line}'")]
    InvalidTableHeader { line: String },

    #[error("invalid card syntax: '{token}'")]
    InvalidCardSyntax { token: String },

    #[error("invalid monetary amount: '{raw}'")]
    InvalidAmount { raw: String },

    #[error("invalid timestamp: '{raw}'")]
    InvalidTimestamp { raw: String },

    #[error("action references unknown or unseated player: '{player_name}'")]
    UnknownPlayer { player_name: String },

    #[error("conflicting hole cards for player '{player_name}'")]
    ConflictingHoleCards { player_name: String },

    #[error("invalid street transition at line {line}: cannot move from {current} to {attempted}")]
    IncompleteStreet {
        line: usize,
        current: String,
        attempted: String,
    },

    #[error("illegal action at line {line} ({reason}): '{content}'")]
    IllegalAction {
        line: usize,
        content: String,
        reason: String,
    },

    #[error("physical card collision detected for card '{card}'")]
    DuplicateCard { card: String },

    #[error("unrecognized line at line {line}: '{content}'")]
    UnrecognizedLine { line: usize, content: String },

    #[error("unsupported off-pot adjustment (e.g. cashout/insurance) at line {line}: '{content}'")]
    UnsupportedAdjustment { line: usize, content: String },

    #[error("summary section disagrees with hand body in hand {hand_id}: {detail}")]
    SummaryMismatch { hand_id: String, detail: String },

    #[error(
        "mismatched pot accounting in hand {hand_id}: total_pot ({total_pot}) - deductions ({total_deductions}) != collected ({total_collected})"
    )]
    MismatchedPotAccounting {
        hand_id: String,
        total_pot: Decimal,
        total_deductions: Decimal,
        total_collected: Decimal,
    },

    #[error(
        "chip conservation failed in hand {hand_id}: actions put {contributed} into the pot but total_pot is {total_pot}"
    )]
    PotContributionMismatch {
        hand_id: String,
        contributed: Decimal,
        total_pot: Decimal,
    },

    #[error("invariant violated in hand {hand_id}: {detail}")]
    InvariantViolation { hand_id: String, detail: String },
}

/// One raw hand block from a session file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandBlock<'a> {
    /// Block text with trailing whitespace removed.
    pub text: &'a str,
    /// 1-based line number of the block's first line in the source text (after BOM removal).
    pub start_line: usize,
}

/// Splits a multi-hand session into hand blocks separated by blank lines (LF or CRLF),
/// stripping a leading UTF-8 BOM and recording each block's starting line number.
///
/// This is the default splitter for [`HandParser::split_blocks`]; sites with a different
/// container format override that method.
pub fn split_hand_blocks(raw_session: &str) -> impl Iterator<Item = HandBlock<'_>> {
    let raw = raw_session.strip_prefix('\u{feff}').unwrap_or(raw_session);
    let mut offset = 0usize;
    let mut line_no = 1usize;

    let next_line = |offset: usize| -> Option<&str> {
        let rest = raw.get(offset..)?;
        if rest.is_empty() {
            return None;
        }
        let len = rest.find('\n').map_or(rest.len(), |i| i + 1);
        rest.get(..len)
    };

    std::iter::from_fn(move || {
        loop {
            let line = next_line(offset)?;
            if !line.trim().is_empty() {
                break;
            }
            offset += line.len();
            line_no += 1;
        }
        let start = offset;
        let start_line = line_no;
        let mut end = offset;
        while let Some(line) = next_line(offset) {
            offset += line.len();
            line_no += 1;
            if line.trim().is_empty() {
                break;
            }
            end = offset;
        }
        raw.get(start..end).map(|text| HandBlock {
            text: text.trim_end(),
            start_line,
        })
    })
}

/// Returns the first non-blank line of `raw_session` (after a BOM), for format sniffing.
#[must_use]
pub fn first_content_line(raw_session: &str) -> Option<&str> {
    raw_session
        .strip_prefix('\u{feff}')
        .unwrap_or(raw_session)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
}

fn violation(hand: &CanonicalHand, detail: impl Into<String>) -> ParseError {
    ParseError::InvariantViolation {
        hand_id: hand.header.site_hand_id.clone(),
        detail: detail.into(),
    }
}

/// Checks if a [`Card`] is already present in `mask`; if not, sets its bit.
fn check_and_insert_card(mask: &mut u64, card: Card) -> Result<(), ParseError> {
    let bit = 1u64 << card.raw();
    if (*mask & bit) != 0 {
        return Err(ParseError::DuplicateCard {
            card: card.to_string(),
        });
    }
    *mask |= bit;
    Ok(())
}

/// Verifies a [`Decimal`] is losslessly convertible to integer micro-units.
fn validate_lossless(value: Decimal) -> Result<(), ParseError> {
    if decimal_to_micro_units(value).is_none() {
        return Err(ParseError::InvalidAmount {
            raw: value.to_string(),
        });
    }
    Ok(())
}

/// Verifies a [`Decimal`] is nonnegative and losslessly convertible to integer micro-units.
fn validate_nonnegative_monetary(value: Decimal) -> Result<(), ParseError> {
    if value < Decimal::ZERO {
        return Err(ParseError::InvalidAmount {
            raw: value.to_string(),
        });
    }
    validate_lossless(value)
}

fn validate_header(hand: &CanonicalHand) -> Result<(), ParseError> {
    let h = &hand.header;
    if h.site_hand_id.trim().is_empty() || h.table_name.trim().is_empty() {
        return Err(violation(hand, "empty hand id or table name"));
    }
    if !(2..=10).contains(&h.max_seats) || h.button_seat < 1 || h.button_seat > h.max_seats {
        return Err(violation(
            hand,
            format!(
                "invalid table size {} / button seat {}",
                h.max_seats, h.button_seat
            ),
        ));
    }
    if let Some(offset) = h.utc_offset_seconds
        && offset.unsigned_abs() > 18 * 3600
    {
        return Err(violation(
            hand,
            format!("UTC offset {offset}s out of range"),
        ));
    }
    validate_nonnegative_monetary(h.small_blind)?;
    validate_nonnegative_monetary(h.big_blind)?;
    if h.small_blind <= Decimal::ZERO || h.big_blind < h.small_blind {
        return Err(violation(
            hand,
            "blinds must satisfy 0 < small blind <= big blind",
        ));
    }
    Ok(())
}

/// Validates seating and hole cards; returns the bitmask of all known hole cards.
fn validate_players(hand: &CanonicalHand) -> Result<u64, ParseError> {
    if hand.players.len() < 2 || hand.players.len() > usize::from(hand.header.max_seats) {
        return Err(violation(
            hand,
            format!(
                "{} seated players at a {}-max table",
                hand.players.len(),
                hand.header.max_seats
            ),
        ));
    }
    let mut mask: u64 = 0;
    let mut seats: HashSet<u8> = HashSet::with_capacity(hand.players.len());
    let mut names: HashSet<&str> = HashSet::with_capacity(hand.players.len());
    let mut heroes = 0usize;

    for p in &hand.players {
        if p.seat < 1 || p.seat > hand.header.max_seats || !seats.insert(p.seat) {
            return Err(violation(
                hand,
                format!("invalid or duplicate seat {}", p.seat),
            ));
        }
        if p.name.trim().is_empty() || !names.insert(p.name.as_str()) {
            return Err(ParseError::UnknownPlayer {
                player_name: p.name.clone(),
            });
        }
        validate_nonnegative_monetary(p.starting_stack)?;
        if p.starting_stack <= Decimal::ZERO {
            return Err(violation(hand, format!("player '{}' has no chips", p.name)));
        }
        if p.hole_cards[0].is_none() && p.hole_cards[1].is_some() {
            return Err(violation(
                hand,
                format!("non-canonical hole cards for '{}'", p.name),
            ));
        }
        for card in p.known_cards() {
            check_and_insert_card(&mut mask, card)?;
        }
        if p.is_hero {
            heroes += 1;
            if p.known_cards().count() != 2 {
                return Err(violation(hand, "hero must have two known hole cards"));
            }
        }
    }
    if heroes != 1 {
        return Err(violation(
            hand,
            format!("expected exactly one hero, found {heroes}"),
        ));
    }
    Ok(mask)
}

fn validate_actions(
    hand: &CanonicalHand,
    by_name: &HashMap<&str, &Player>,
) -> Result<(), ParseError> {
    let mut commitments: HashMap<(Street, &str), Decimal> = HashMap::new();
    let mut gross: HashMap<&str, Decimal> = HashMap::new();
    let mut folded: HashSet<&str> = HashSet::new();
    let mut last_street = Street::Preflop;

    for (idx, act) in hand.actions.iter().enumerate() {
        if usize::from(act.sequence) != idx {
            return Err(violation(
                hand,
                format!("action sequence {} at position {idx}", act.sequence),
            ));
        }
        if act.street < last_street || act.street == Street::Showdown {
            return Err(violation(
                hand,
                format!(
                    "action {} recorded out of street order ({:?})",
                    act.sequence, act.street
                ),
            ));
        }
        last_street = act.street;

        let name = act.player_name.as_str();
        let Some(player) = by_name.get(name) else {
            return Err(ParseError::UnknownPlayer {
                player_name: act.player_name.clone(),
            });
        };
        if folded.contains(name) {
            return Err(violation(hand, format!("'{name}' acts after folding")));
        }
        validate_nonnegative_monetary(act.amount)?;
        validate_nonnegative_monetary(act.street_total_after)?;

        let t = act.action_type;
        let zero_amount = matches!(t, ActionType::Fold | ActionType::Check);
        if zero_amount != act.amount.is_zero() {
            return Err(violation(
                hand,
                format!(
                    "action {} ({}) has amount {}",
                    act.sequence,
                    t.as_str(),
                    act.amount
                ),
            ));
        }

        let key = (act.street, name);
        let prev = commitments.get(&key).copied().unwrap_or(Decimal::ZERO);
        let expected = if t.adds_to_street_commitment() {
            prev.checked_add(act.amount)
        } else if t == ActionType::UncalledBetReturned {
            prev.checked_sub(act.amount)
        } else {
            Some(prev)
        };
        if expected != Some(act.street_total_after) || act.street_total_after < Decimal::ZERO {
            return Err(violation(
                hand,
                format!(
                    "street_total_after chain broken at action {} ('{name}')",
                    act.sequence
                ),
            ));
        }
        commitments.insert(key, act.street_total_after);

        if t.is_contribution() {
            let g = gross
                .get(name)
                .copied()
                .unwrap_or(Decimal::ZERO)
                .checked_add(act.amount)
                .ok_or_else(|| violation(hand, "contribution overflow"))?;
            if g > player.starting_stack {
                return Err(violation(
                    hand,
                    format!(
                        "'{name}' commits {g} with a starting stack of {}",
                        player.starting_stack
                    ),
                ));
            }
            if act.is_all_in && g != player.starting_stack {
                return Err(violation(
                    hand,
                    format!(
                        "'{name}' is marked all-in after committing {g} of {}",
                        player.starting_stack
                    ),
                ));
            }
            if !act.is_all_in && g == player.starting_stack {
                return Err(violation(
                    hand,
                    format!(
                        "'{name}' commits the whole stack of {g} in action {} without being marked all-in",
                        act.sequence
                    ),
                ));
            }
            gross.insert(name, g);
        } else if act.is_all_in {
            return Err(violation(
                hand,
                format!("non-contribution action {} marked all-in", act.sequence),
            ));
        }
        if t == ActionType::Fold {
            folded.insert(name);
        }
    }
    Ok(())
}

fn validate_outcome(
    hand: &CanonicalHand,
    by_name: &HashMap<&str, &Player>,
    run_count: usize,
) -> Result<(), ParseError> {
    let o = &hand.outcome;
    validate_nonnegative_monetary(o.total_pot)?;
    validate_nonnegative_monetary(o.uncalled_returned)?;
    for d in &o.deductions {
        validate_nonnegative_monetary(d.amount)?;
        if d.label.trim().is_empty() {
            return Err(violation(hand, "pot deduction without a label"));
        }
    }

    // Summary arithmetic: total_pot - deductions == collected.
    let total_deductions = o
        .checked_total_deductions()
        .ok_or_else(|| violation(hand, "deductions overflow"))?;
    let total_collected = o
        .checked_total_collected()
        .ok_or_else(|| violation(hand, "collected overflow"))?;
    if o.total_pot.checked_sub(total_deductions) != Some(total_collected) {
        return Err(ParseError::MismatchedPotAccounting {
            hand_id: hand.header.site_hand_id.clone(),
            total_pot: o.total_pot,
            total_deductions,
            total_collected,
        });
    }

    // Chip conservation: what the actions put in is exactly the pot.
    let contributed = hand
        .checked_total_contributed()
        .ok_or_else(|| violation(hand, "contribution overflow"))?;
    if contributed != o.total_pot {
        return Err(ParseError::PotContributionMismatch {
            hand_id: hand.header.site_hand_id.clone(),
            contributed,
            total_pot: o.total_pot,
        });
    }
    let returned = hand
        .actions
        .iter()
        .filter(|a| a.action_type == ActionType::UncalledBetReturned)
        .try_fold(Decimal::ZERO, |acc, a| acc.checked_add(a.amount));
    if returned != Some(o.uncalled_returned) {
        return Err(violation(
            hand,
            "uncalled_returned disagrees with returned-bet actions",
        ));
    }

    // Winners.
    let folded: HashSet<&str> = hand
        .actions
        .iter()
        .filter(|a| a.action_type == ActionType::Fold)
        .map(|a| a.player_name.as_str())
        .collect();
    if o.total_pot > Decimal::ZERO && o.winners.is_empty() {
        return Err(violation(hand, "pot has no winner"));
    }
    let mut runs_with_winner: HashSet<u8> = HashSet::new();
    for w in &o.winners {
        if !by_name.contains_key(w.player_name.as_str()) {
            return Err(ParseError::UnknownPlayer {
                player_name: w.player_name.clone(),
            });
        }
        validate_nonnegative_monetary(w.amount)?;
        if w.amount.is_zero() {
            return Err(violation(
                hand,
                format!("zero payout to '{}'", w.player_name),
            ));
        }
        if folded.contains(w.player_name.as_str()) {
            return Err(violation(
                hand,
                format!("folded player '{}' wins a pot", w.player_name),
            ));
        }
        let valid_run = if run_count >= 2 {
            (1..=run_count).contains(&usize::from(w.run_index))
        } else {
            w.run_index == 0
        };
        if !valid_run {
            return Err(violation(
                hand,
                format!("payout on invalid run {}", w.run_index),
            ));
        }
        runs_with_winner.insert(w.run_index);
    }
    if run_count >= 2 && runs_with_winner.len() != run_count {
        return Err(violation(
            hand,
            "every runout must award its share of the pot",
        ));
    }

    // Off-pot adjustments.
    for a in &o.adjustments {
        if !by_name.contains_key(a.player_name.as_str()) {
            return Err(ParseError::UnknownPlayer {
                player_name: a.player_name.clone(),
            });
        }
        validate_lossless(a.amount)?;
        let sign_ok = match a.kind.is_credit() {
            Some(true) => a.amount > Decimal::ZERO,
            Some(false) => a.amount < Decimal::ZERO,
            None => !a.amount.is_zero(),
        };
        if !sign_ok {
            return Err(violation(
                hand,
                format!(
                    "{} adjustment for '{}' has amount {}",
                    a.kind.as_str(),
                    a.player_name,
                    a.amount
                ),
            ));
        }
        if a.street == Street::Showdown {
            return Err(violation(hand, "adjustment triggered at showdown"));
        }
    }

    // Per-player results must be computable and lossless.
    let results = hand
        .checked_player_results()
        .ok_or_else(|| violation(hand, "player result overflow"))?;
    for r in &results {
        validate_nonnegative_monetary(r.invested)?;
        validate_nonnegative_monetary(r.won)?;
        validate_lossless(r.net)?;
    }
    validate_side_pots(hand, &results)
}

/// Side-pot eligibility: the players who put at most `L` live chips into the pot can together
/// win at most what every player put in up to `L` each, plus all dead money. Checked for every
/// winner's level `L`, so a short all-in stack can never be paid from a side pot.
fn validate_side_pots(hand: &CanonicalHand, results: &[PlayerResult]) -> Result<(), ParseError> {
    let overflow = || violation(hand, "side-pot arithmetic overflow");
    let mut dead_total = Decimal::ZERO;
    let mut live: Vec<(Decimal, Decimal)> = Vec::with_capacity(results.len());
    for r in results {
        let dead = hand
            .actions
            .iter()
            .filter(|a| {
                a.player_name == r.player_name && a.action_type == ActionType::PostDeadBlind
            })
            .try_fold(Decimal::ZERO, |acc, a| acc.checked_add(a.amount))
            .ok_or_else(overflow)?;
        dead_total = dead_total.checked_add(dead).ok_or_else(overflow)?;
        let live_in = r.invested.checked_sub(dead).ok_or_else(overflow)?;
        live.push((live_in, r.won));
    }
    for &(level, won) in &live {
        if won.is_zero() {
            continue;
        }
        let mut cap = dead_total;
        let mut paid = Decimal::ZERO;
        for &(other_live, other_won) in &live {
            cap = cap
                .checked_add(other_live.min(level))
                .ok_or_else(overflow)?;
            if other_live <= level {
                paid = paid.checked_add(other_won).ok_or_else(overflow)?;
            }
        }
        if paid > cap {
            return Err(violation(
                hand,
                format!(
                    "players who put {level} or less into the pot are paid {paid}, more than the {cap} they could win"
                ),
            ));
        }
    }
    Ok(())
}

/// Validates board progression, run structure and 52-card uniqueness across all runouts.
fn validate_boards(hand: &CanonicalHand, hole_cards_mask: u64) -> Result<usize, ParseError> {
    let multi = hand.boards.iter().any(|b| b.run_index > 0);
    let run_count = hand.run_count();
    if multi {
        let contiguous = hand
            .boards
            .iter()
            .enumerate()
            .all(|(i, b)| usize::from(b.run_index) == i + 1);
        if !contiguous || !(2..=MAX_RUNOUTS).contains(&run_count) {
            return Err(violation(
                hand,
                "multi-run boards must be runs 1..=n with 2 <= n <= MAX_RUNOUTS",
            ));
        }
        if hand.boards.iter().any(|b| b.river.is_none()) {
            return Err(violation(
                hand,
                "every multi-run board must be dealt to the river",
            ));
        }
    } else if hand.boards.len() > 1 {
        return Err(violation(hand, "multiple boards without run indices"));
    }

    let mut global_dealt_mask = hole_cards_mask;
    let first_board = hand.boards.first();
    for (board_idx, board) in hand.boards.iter().enumerate() {
        if (board.turn.is_some() && board.flop.is_none())
            || (board.river.is_some() && board.turn.is_none())
        {
            return Err(ParseError::IncompleteStreet {
                line: 0,
                current: "BOARD".to_string(),
                attempted: format!("run_index {}", board.run_index),
            });
        }

        // Each runout on its own must not collide with any hole card.
        let mut single_board_mask = hole_cards_mask;
        for card in board.cards() {
            check_and_insert_card(&mut single_board_mask, card)?;
        }

        // Across runouts, only cards that are not shared with the first runout are new.
        let (shares_flop, shares_turn) = match first_board {
            Some(first) if board_idx > 0 => {
                let f = board.flop == first.flop;
                (f, f && board.turn == first.turn)
            }
            _ => (false, false),
        };
        if !shares_flop && let Some(flop) = board.flop {
            for card in flop {
                check_and_insert_card(&mut global_dealt_mask, card)?;
            }
        }
        if !shares_turn && let Some(turn) = board.turn {
            check_and_insert_card(&mut global_dealt_mask, turn)?;
        }
        if let Some(river) = board.river {
            check_and_insert_card(&mut global_dealt_mask, river)?;
        }
    }
    Ok(run_count)
}

/// First seat clockwise after `from` (wrapping; `from` itself is checked last) satisfying `pred`.
/// `seats` must be sorted ascending.
fn next_clockwise(seats: &[u8], from: u8, pred: impl Fn(u8) -> bool) -> Option<u8> {
    seats
        .iter()
        .copied()
        .filter(|s| *s > from)
        .chain(seats.iter().copied().filter(|s| *s <= from))
        .find(|s| pred(*s))
}

/// Mutable state of one hand while its betting is replayed.
struct BettingState {
    folded: HashSet<u8>,
    all_in: HashSet<u8>,
    gross: HashMap<u8, Decimal>,
    committed: HashMap<u8, Decimal>,
}

impl BettingState {
    fn live_count(&self, seats: &[u8]) -> usize {
        seats.iter().filter(|s| !self.folded.contains(s)).count()
    }

    fn can_act(&self, seat: u8) -> bool {
        !self.folded.contains(&seat) && !self.all_in.contains(&seat)
    }

    fn committed(&self, seat: u8) -> Decimal {
        self.committed.get(&seat).copied().unwrap_or(Decimal::ZERO)
    }

    fn max_committed(&self) -> Decimal {
        self.committed
            .values()
            .copied()
            .max()
            .unwrap_or(Decimal::ZERO)
    }

    /// Chips the player has put in over the whole hand (before any uncalled-bet return).
    fn gross(&self, seat: u8) -> Decimal {
        self.gross.get(&seat).copied().unwrap_or(Decimal::ZERO)
    }

    fn commit(&mut self, seat: u8, amount: Decimal, stack: Decimal) -> Option<()> {
        let g = self.gross(seat).checked_add(amount)?;
        if g == stack {
            self.all_in.insert(seat);
        }
        self.gross.insert(seat, g);
        Some(())
    }

    /// The round is settled for a lone player who can still act once they match the highest
    /// commitment: nobody is left to bet against, so they owe no action.
    fn settle_lone_player(&self, seats: &[u8], needs: &mut HashSet<u8>) {
        let mut able = seats.iter().copied().filter(|s| self.can_act(*s));
        if let (Some(only), None) = (able.next(), able.next())
            && self.committed(only) >= self.max_committed()
        {
            needs.remove(&only);
        }
    }
}

/// Replays the betting street by street (No-Limit rules, seat order clockwise by seat number):
///
/// - blinds come first and match the stakes (less only when the poster is all-in); preflop
///   action starts left of the first big blind poster, later streets start left of the button;
/// - every player still able to act must act, in turn, until the round closes; a bet or raise
///   makes everyone else still able to act owe an action again; a lone player who can still
///   act and already matches the highest commitment owes nothing;
/// - checks, calls, bets and raises must be legal for the amount faced: a raise needs a bet to
///   raise and an opponent who can still respond; a bet is at least the big blind and a raise
///   adds at least the last full bet or raise (or completes a smaller all-in to the big blind),
///   unless the player is all-in; an all-in for less than a full raise does not reopen the
///   betting for players who already acted;
/// - an uncalled bet returns exactly the unmatched part of the largest commitment, after the
///   betting closed;
/// - actions happen only on streets whose cards were dealt; no street is dealt after the hand
///   was decided by folds; a hand contested by two or more players reaches the river.
fn validate_betting(hand: &CanonicalHand) -> Result<(), ParseError> {
    let mut seats: Vec<u8> = hand.players.iter().map(|p| p.seat).collect();
    seats.sort_unstable();
    let seat_of: HashMap<&str, u8> = hand
        .players
        .iter()
        .map(|p| (p.name.as_str(), p.seat))
        .collect();
    let stack_of: HashMap<u8, Decimal> = hand
        .players
        .iter()
        .map(|p| (p.seat, p.starting_stack))
        .collect();
    let seat = |name: &str| -> Result<u8, ParseError> {
        seat_of
            .get(name)
            .copied()
            .ok_or_else(|| ParseError::UnknownPlayer {
                player_name: name.to_string(),
            })
    };
    let stack = |s: u8| stack_of.get(&s).copied().unwrap_or(Decimal::ZERO);
    let is_post = |t: ActionType| {
        matches!(
            t,
            ActionType::PostSmallBlind | ActionType::PostBigBlind | ActionType::PostDeadBlind
        )
    };
    let (small_blind, big_blind) = (hand.header.small_blind, hand.header.big_blind);

    // Blinds lead the hand and match the stakes.
    let posts = hand
        .actions
        .iter()
        .take_while(|a| is_post(a.action_type))
        .count();
    if hand
        .actions
        .iter()
        .skip(posts)
        .any(|a| is_post(a.action_type))
    {
        return Err(violation(hand, "blind posted after the betting started"));
    }
    let posted = hand.actions.get(..posts).unwrap_or(&[]);
    if posted
        .iter()
        .filter(|a| a.action_type == ActionType::PostSmallBlind)
        .count()
        > 1
    {
        return Err(violation(hand, "more than one small blind"));
    }
    for a in posted {
        let size_ok = match a.action_type {
            ActionType::PostSmallBlind => {
                a.amount == small_blind || (a.is_all_in && a.amount < small_blind)
            }
            ActionType::PostBigBlind => {
                a.amount == big_blind || (a.is_all_in && a.amount < big_blind)
            }
            _ => a.amount <= big_blind,
        };
        if !size_ok {
            return Err(violation(
                hand,
                format!(
                    "{} of {} by '{}' does not match the {small_blind}/{big_blind} stakes",
                    a.action_type.as_str(),
                    a.amount,
                    a.player_name
                ),
            ));
        }
    }
    let Some(first_big_blind) = posted
        .iter()
        .find(|a| a.action_type == ActionType::PostBigBlind)
    else {
        return Err(violation(hand, "no big blind posted"));
    };
    let big_blind_seat = seat(&first_big_blind.player_name)?;

    let mut state = BettingState {
        folded: HashSet::new(),
        all_in: HashSet::new(),
        gross: HashMap::new(),
        committed: HashMap::new(),
    };
    for a in posted {
        let s = seat(&a.player_name)?;
        state.committed.insert(s, a.street_total_after);
        state
            .commit(s, a.amount, stack(s))
            .ok_or_else(|| violation(hand, "contribution overflow"))?;
    }

    let reached = |street: Street| match street {
        Street::Preflop => true,
        Street::Flop => hand.boards.iter().any(|b| b.flop.is_some()),
        Street::Turn => hand.boards.iter().any(|b| b.turn.is_some()),
        Street::River => hand.boards.iter().any(|b| b.river.is_some()),
        Street::Showdown => false,
    };

    let mut idx = posts;
    for street in [Street::Preflop, Street::Flop, Street::Turn, Street::River] {
        if !reached(street) {
            break;
        }
        if street != Street::Preflop {
            if state.live_count(&seats) <= 1 {
                return Err(violation(
                    hand,
                    format!("{street:?} dealt after the hand was decided"),
                ));
            }
            state.committed.clear();
        }
        let able: Vec<u8> = seats
            .iter()
            .copied()
            .filter(|s| state.can_act(*s))
            .collect();
        let max = state.max_committed();
        let mut needs: HashSet<u8> = able
            .iter()
            .copied()
            .filter(|s| able.len() >= 2 || state.committed(*s) < max)
            .collect();
        // Players who already acted voluntarily on this street (posting a blind is not acting).
        let mut acted: HashSet<u8> = HashSet::new();
        // Size of the last full bet or raise; the minimum bet is the big blind.
        let mut min_raise = big_blind;
        let mut anchor = if street == Street::Preflop {
            big_blind_seat
        } else {
            hand.header.button_seat
        };

        while let Some(a) = hand.actions.get(idx).filter(|a| a.street == street) {
            idx += 1;
            let s = seat(&a.player_name)?;
            let current = state.committed(s);
            let prev_max = state.max_committed();

            if a.action_type == ActionType::UncalledBetReturned {
                if !needs.is_empty() && state.live_count(&seats) > 1 {
                    return Err(violation(
                        hand,
                        "uncalled bet returned while betting was open",
                    ));
                }
                let second = state
                    .committed
                    .iter()
                    .filter(|(other, _)| **other != s)
                    .map(|(_, v)| *v)
                    .max()
                    .unwrap_or(Decimal::ZERO);
                if current <= second || current.checked_sub(second) != Some(a.amount) {
                    return Err(violation(
                        hand,
                        format!(
                            "uncalled bet of {} to '{}' is not the unmatched part of the largest bet",
                            a.amount, a.player_name
                        ),
                    ));
                }
                state.committed.insert(s, a.street_total_after);
                continue;
            }

            if state.live_count(&seats) <= 1 {
                return Err(violation(
                    hand,
                    format!("'{}' acts after the hand was decided", a.player_name),
                ));
            }
            if next_clockwise(&seats, anchor, |x| needs.contains(&x)) != Some(s) {
                return Err(violation(
                    hand,
                    format!(
                        "'{}' acts out of turn on the {street:?} (action {})",
                        a.player_name, a.sequence
                    ),
                ));
            }
            let total = a.street_total_after;
            let goes_all_in = state.gross(s).checked_add(a.amount) == Some(stack(s));
            let opponent_can_respond = seats.iter().any(|x| *x != s && state.can_act(*x));
            let increment = total.checked_sub(prev_max).unwrap_or(Decimal::ZERO);
            // A player who already acted may raise again only when facing at least a full
            // raise since then (a check never gives up that right).
            let may_raise = !acted.contains(&s)
                || current.is_zero()
                || prev_max
                    .checked_sub(current)
                    .is_some_and(|d| d >= min_raise);
            let legal = match a.action_type {
                ActionType::Fold => true,
                ActionType::Check => current == prev_max,
                ActionType::Call => total <= prev_max && (total == prev_max || goes_all_in),
                ActionType::Bet => {
                    prev_max.is_zero()
                        && opponent_can_respond
                        && (a.amount >= big_blind || goes_all_in)
                }
                // After an all-in for less than the big blind, raising to the big blind
                // ("completing") is also a legal raise.
                ActionType::Raise => {
                    prev_max > Decimal::ZERO
                        && total > prev_max
                        && opponent_can_respond
                        && may_raise
                        && (increment >= min_raise
                            || goes_all_in
                            || (prev_max < big_blind && total >= big_blind))
                }
                _ => false,
            };
            if !legal {
                return Err(violation(
                    hand,
                    format!(
                        "illegal {} to {total} by '{}' facing {prev_max} on the {street:?}",
                        a.action_type.as_str(),
                        a.player_name
                    ),
                ));
            }

            needs.remove(&s);
            acted.insert(s);
            state.committed.insert(s, total);
            if a.action_type.is_contribution() {
                state
                    .commit(s, a.amount, stack(s))
                    .ok_or_else(|| violation(hand, "contribution overflow"))?;
            }
            if a.action_type == ActionType::Fold {
                state.folded.insert(s);
            }
            if total > prev_max {
                if increment >= min_raise {
                    min_raise = increment;
                }
                needs = seats
                    .iter()
                    .copied()
                    .filter(|x| *x != s && state.can_act(*x))
                    .collect();
            }
            state.settle_lone_player(&seats, &mut needs);
            anchor = s;
        }

        if state.live_count(&seats) >= 2 && !needs.is_empty() {
            return Err(violation(
                hand,
                format!(
                    "the {street:?} betting ended before {} player(s) acted",
                    needs.len()
                ),
            ));
        }
    }

    if idx < hand.actions.len() {
        return Err(violation(
            hand,
            "actions recorded on a street whose cards were never dealt",
        ));
    }
    if state.live_count(&seats) >= 2 && !reached(Street::River) {
        return Err(violation(
            hand,
            "hand contested by several players ended before the river",
        ));
    }
    Ok(())
}

/// Validates every structural, monetary, seating, betting, board, pot-conservation and
/// card-uniqueness invariant of a [`CanonicalHand`]. Site-independent.
///
/// Invoked by every parser after parsing and again by the storage layer before persisting.
///
/// # Errors
/// Returns the first [`ParseError`] describing a violated invariant.
pub fn validate_hand(hand: &CanonicalHand) -> Result<(), ParseError> {
    validate_header(hand)?;
    let hole_mask = validate_players(hand)?;
    let by_name: HashMap<&str, &Player> =
        hand.players.iter().map(|p| (p.name.as_str(), p)).collect();
    validate_actions(hand, &by_name)?;
    let run_count = validate_boards(hand, hole_mask)?;
    validate_betting(hand)?;
    validate_outcome(hand, &by_name, run_count)?;
    Ok(())
}

/// Iterator of parsed hand blocks produced by [`HandParser::parse_session`].
pub type ParsedBlocks<'a> =
    Box<dyn Iterator<Item = (HandBlock<'a>, Result<CanonicalHand, ParseError>)> + 'a>;

/// Contract implemented by every site parser. Object-safe, so parsers can be registered in a
/// [`crate::parser::ParserRegistry`] and selected per file at runtime.
pub trait HandParser {
    /// The [`Platform`] whose hand histories this parser reads.
    fn platform(&self) -> Platform;

    /// Short human-readable description used in diagnostics (e.g. `"GGPoker (CAD)"`).
    fn describe(&self) -> String;

    /// Returns `true` if `raw_session` looks like this site's export format. Must be cheap and
    /// must not claim files from other sites; it should inspect only the opening line(s).
    fn recognizes(&self, raw_session: &str) -> bool;

    /// Splits a session file into hand blocks. Defaults to blank-line separation.
    fn split_blocks<'a>(&self, raw_session: &'a str) -> Vec<HandBlock<'a>> {
        split_hand_blocks(raw_session).collect()
    }

    /// Parses a single raw hand block into a validated [`CanonicalHand`].
    ///
    /// # Errors
    /// Returns [`ParseError`] if syntax, betting rules, summary cross-checks or invariants fail.
    fn parse_hand(&self, raw_block: &str) -> Result<CanonicalHand, ParseError>;

    /// Best-effort extraction of the site hand id from a raw block, used for diagnostics when
    /// parsing fails.
    fn peek_hand_id(&self, raw_block: &str) -> Option<String>;

    /// Lazily parses every hand block in a session, yielding each block with its result.
    fn parse_session<'a>(&'a self, raw_session: &'a str) -> ParsedBlocks<'a> {
        Box::new(
            self.split_blocks(raw_session)
                .into_iter()
                .map(move |block| (block, self.parse_hand(block.text))),
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::domain::canonical::{
        AdjustmentKind, BettingLimit, CommunityCards, DeductionKind, GameType, HandHeader,
        HandOutcome, PlayerAdjustment, PotDeduction, PotWinner, StreetAction,
    };
    use crate::domain::money::CurrencyCode;
    use chrono::NaiveDate;
    use rust_decimal_macros::dec;
    use std::str::FromStr;

    fn c(s: &str) -> Card {
        Card::from_str(s).unwrap()
    }

    fn act(
        seq: u16,
        street: Street,
        name: &str,
        t: ActionType,
        amount: Decimal,
        total: Decimal,
    ) -> StreetAction {
        StreetAction {
            sequence: seq,
            street,
            player_name: name.to_string(),
            action_type: t,
            amount,
            street_total_after: total,
            is_all_in: false,
        }
    }

    fn player(seat: u8, name: &str, cards: [Option<Card>; 2], hero: bool) -> Player {
        Player {
            seat,
            name: name.to_string(),
            starting_stack: dec!(10),
            hole_cards: cards,
            is_hero: hero,
        }
    }

    /// SB limps, BB checks, SB bets the flop, BB folds, bet returned. Pot 0.20, rake 0.01.
    fn valid_hand() -> CanonicalHand {
        use ActionType::*;
        use Street::*;
        CanonicalHand {
            header: HandHeader {
                platform: Platform::GGPoker,
                site_hand_id: "HDTEST1".into(),
                game_type: GameType::Cash,
                limit: BettingLimit::NoLimit,
                currency: CurrencyCode::CAD,
                small_blind: dec!(0.05),
                big_blind: dec!(0.1),
                timestamp: NaiveDate::from_ymd_opt(2026, 8, 21)
                    .unwrap()
                    .and_hms_opt(17, 0, 0)
                    .unwrap(),
                utc_offset_seconds: None,
                table_name: "T".into(),
                max_seats: 6,
                button_seat: 3,
            },
            players: vec![
                player(1, "A", [None, None], false),
                player(2, "Hero", [Some(c("As")), Some(c("Kd"))], true),
                player(3, "C", [None, None], false),
            ],
            actions: vec![
                act(0, Preflop, "A", PostSmallBlind, dec!(0.05), dec!(0.05)),
                act(1, Preflop, "Hero", PostBigBlind, dec!(0.1), dec!(0.1)),
                act(2, Preflop, "C", Fold, dec!(0), dec!(0)),
                act(3, Preflop, "A", Call, dec!(0.05), dec!(0.1)),
                act(4, Preflop, "Hero", Check, dec!(0), dec!(0.1)),
                act(5, Flop, "A", Bet, dec!(0.2), dec!(0.2)),
                act(6, Flop, "Hero", Fold, dec!(0), dec!(0)),
                act(7, Flop, "A", UncalledBetReturned, dec!(0.2), dec!(0)),
            ],
            boards: vec![CommunityCards {
                run_index: 0,
                flop: Some([c("2c"), c("7d"), c("9h")]),
                turn: None,
                river: None,
            }],
            outcome: HandOutcome {
                total_pot: dec!(0.2),
                deductions: vec![PotDeduction {
                    kind: DeductionKind::Rake,
                    label: "Rake".into(),
                    amount: dec!(0.01),
                }],
                uncalled_returned: dec!(0.2),
                winners: vec![PotWinner {
                    player_name: "A".into(),
                    amount: dec!(0.19),
                    run_index: 0,
                }],
                adjustments: vec![],
            },
        }
    }

    fn assert_violation(h: &CanonicalHand) {
        assert!(
            validate_hand(h).is_err(),
            "expected validator to reject mutated hand"
        );
    }

    #[test]
    fn baseline_hand_is_valid_and_results_reconcile() {
        let h = valid_hand();
        validate_hand(&h).unwrap();
        let r = h.checked_player_results().unwrap();
        assert_eq!(r[0].net, dec!(0.09));
        assert_eq!(r[1].net, dec!(-0.1));
        assert_eq!(r[2].net, dec!(0));
        let total: Decimal = r.iter().map(|x| x.net).sum();
        assert_eq!(total, dec!(-0.01), "sum of nets equals minus the rake");
    }

    #[test]
    fn rejects_actions_that_do_not_sum_to_the_pot() {
        // Summary arithmetic still balances (0.25 - 0.06 = 0.19), but actions put in only 0.20.
        let mut h = valid_hand();
        h.outcome.total_pot = dec!(0.25);
        h.outcome.deductions[0].amount = dec!(0.06);
        assert!(matches!(
            validate_hand(&h),
            Err(ParseError::PotContributionMismatch { .. })
        ));
    }

    #[test]
    fn rejects_broken_street_total_chain() {
        let mut h = valid_hand();
        h.actions[0].street_total_after = dec!(7);
        assert_violation(&h);
    }

    #[test]
    fn rejects_zero_or_multiple_heroes() {
        let mut h = valid_hand();
        h.players[0].is_hero = true;
        assert_violation(&h);
        let mut h = valid_hand();
        h.players[1].is_hero = false;
        assert_violation(&h);
    }

    #[test]
    fn rejects_commitment_beyond_stack_and_false_all_in() {
        let mut h = valid_hand();
        h.players[0].starting_stack = dec!(0.05);
        assert_violation(&h);
        let mut h = valid_hand();
        h.actions[3].is_all_in = true;
        assert_violation(&h);
    }

    #[test]
    fn rejects_out_of_order_streets_and_sequences() {
        let mut h = valid_hand();
        h.actions[0].street = Street::River;
        assert_violation(&h);
        let mut h = valid_hand();
        h.actions[2].sequence = 9;
        assert_violation(&h);
    }

    #[test]
    fn rejects_folded_winner_and_action_after_fold() {
        let mut h = valid_hand();
        h.outcome.winners[0].player_name = "C".into();
        assert_violation(&h);
        let mut h = valid_hand();
        h.actions[4].player_name = "C".into();
        assert_violation(&h);
    }

    #[test]
    fn rejects_mismatched_summary_accounting() {
        let mut h = valid_hand();
        h.outcome.deductions[0].amount = dec!(0.02);
        assert!(matches!(
            validate_hand(&h),
            Err(ParseError::MismatchedPotAccounting { .. })
        ));
    }

    #[test]
    fn rejects_duplicate_physical_cards() {
        let mut h = valid_hand();
        h.boards[0].flop = Some([c("As"), c("7d"), c("9h")]);
        assert_eq!(
            validate_hand(&h),
            Err(ParseError::DuplicateCard { card: "As".into() })
        );
    }

    #[test]
    fn rejects_non_canonical_partial_hole_cards_and_unlabeled_deductions() {
        let mut h = valid_hand();
        h.players[0].hole_cards = [None, Some(c("2s"))];
        assert_violation(&h);
        let mut h = valid_hand();
        h.outcome.deductions[0].label = " ".into();
        assert_violation(&h);
    }

    #[test]
    fn validates_adjustment_players_and_signs() {
        let adjustment = |name: &str, amount: Decimal| PlayerAdjustment {
            player_name: name.into(),
            kind: AdjustmentKind::EvCashoutRiskPaid,
            street: Street::Flop,
            amount,
        };
        let mut h = valid_hand();
        h.outcome.adjustments.push(adjustment("A", dec!(-0.05)));
        validate_hand(&h).unwrap();
        assert_eq!(h.checked_player_results().unwrap()[0].net, dec!(0.04));

        let mut h = valid_hand();
        h.outcome.adjustments.push(adjustment("A", dec!(0.05)));
        assert_violation(&h);
        let mut h = valid_hand();
        h.outcome
            .adjustments
            .push(adjustment("Nobody", dec!(-0.05)));
        assert!(matches!(
            validate_hand(&h),
            Err(ParseError::UnknownPlayer { .. })
        ));
    }

    #[test]
    fn rejects_payout_on_nonexistent_run() {
        let mut h = valid_hand();
        h.outcome.winners[0].run_index = 2;
        assert_violation(&h);
    }

    fn renumber(h: &mut CanonicalHand) {
        for (i, a) in h.actions.iter_mut().enumerate() {
            a.sequence = u16::try_from(i).unwrap();
        }
    }

    fn betting_error(h: &CanonicalHand) -> String {
        match validate_hand(h) {
            Err(ParseError::InvariantViolation { detail, .. }) => detail,
            other => panic!("expected an invariant violation, got {other:?}"),
        }
    }

    #[test]
    fn rejects_actions_out_of_turn() {
        // Preflop action starts left of the big blind (seat 3), not with the small blind.
        let mut h = valid_hand();
        h.actions.swap(2, 3);
        renumber(&mut h);
        assert!(betting_error(&h).contains("out of turn"));
        // The same player checking twice in a row.
        let mut h = valid_hand();
        let repeat = h.actions[4].clone();
        h.actions.insert(5, repeat);
        renumber(&mut h);
        assert!(betting_error(&h).contains("out of turn"));
    }

    #[test]
    fn rejects_a_betting_round_that_closes_before_everyone_acted() {
        // The big blind never uses the option, yet the flop is dealt.
        let mut h = valid_hand();
        h.actions.remove(4);
        renumber(&mut h);
        assert!(betting_error(&h).contains("ended before"));
    }

    #[test]
    fn rejects_actions_and_cards_on_streets_that_cannot_exist() {
        // Flop actions without a flop.
        let mut h = valid_hand();
        h.boards.clear();
        assert!(betting_error(&h).contains("never dealt"));
        // A turn dealt after everyone but one player folded.
        let mut h = valid_hand();
        h.boards[0].turn = Some(c("Qc"));
        assert!(betting_error(&h).contains("after the hand was decided"));
    }

    #[test]
    fn rejects_an_uncalled_bet_of_the_wrong_size() {
        let mut h = valid_hand();
        h.actions[7].amount = dec!(0.1);
        h.actions[7].street_total_after = dec!(0.1);
        h.outcome.uncalled_returned = dec!(0.1);
        assert!(betting_error(&h).contains("unmatched part"));
    }

    #[test]
    fn rejects_a_contested_hand_that_ends_before_the_river() {
        // Hero calls the flop bet instead of folding, but no turn or river follows.
        let mut h = valid_hand();
        h.actions[6].action_type = ActionType::Call;
        h.actions[6].amount = dec!(0.2);
        h.actions[6].street_total_after = dec!(0.2);
        h.actions.truncate(7);
        h.outcome.uncalled_returned = dec!(0);
        assert!(betting_error(&h).contains("before the river"));
    }

    #[test]
    fn rejects_an_action_by_a_player_who_is_already_all_in() {
        // A's limp puts their whole 0.10 stack in; they cannot bet the flop afterwards.
        let mut h = valid_hand();
        h.players[0].starting_stack = dec!(0.1);
        h.actions[3].is_all_in = true;
        h.actions[5].amount = dec!(0.2);
        let err = validate_hand(&h).unwrap_err();
        assert!(
            matches!(err, ParseError::InvariantViolation { .. }),
            "{err:?}"
        );
    }

    /// Builds a consistent hand from betting steps. Each step's amount is the chips the action
    /// adds, except for raises, where it is the street total raised to (and for an uncalled bet,
    /// the chips returned). Seats follow `stacks` order from 1; the player named "Hero" holds
    /// As Kd. All-in flags, street totals, sequences, the pot and the returns are derived, and
    /// `streets_dealt` (0..=3) board streets are dealt from 2c 7d 9h / Qc / 3s.
    fn scenario(
        stacks: &[(&str, Decimal)],
        button: u8,
        steps: &[(Street, &str, ActionType, Decimal)],
        streets_dealt: usize,
        winners: &[(&str, Decimal)],
    ) -> CanonicalHand {
        let mut h = valid_hand();
        h.header.button_seat = button;
        h.players = stacks
            .iter()
            .enumerate()
            .map(|(i, (name, st))| Player {
                seat: u8::try_from(i + 1).unwrap(),
                name: (*name).to_string(),
                starting_stack: *st,
                hole_cards: if *name == "Hero" {
                    [Some(c("As")), Some(c("Kd"))]
                } else {
                    [None, None]
                },
                is_hero: *name == "Hero",
            })
            .collect();
        let mut committed: HashMap<(Street, &str), Decimal> = HashMap::new();
        let mut gross: HashMap<&str, Decimal> = HashMap::new();
        let (mut pot, mut returned) = (Decimal::ZERO, Decimal::ZERO);
        h.actions.clear();
        for (i, &(street, name, t, x)) in steps.iter().enumerate() {
            let cur = committed.get(&(street, name)).copied().unwrap_or_default();
            let (amount, total) = match t {
                ActionType::Raise => (x - cur, x),
                ActionType::UncalledBetReturned => (x, cur - x),
                ActionType::Fold | ActionType::Check => (Decimal::ZERO, cur),
                ActionType::PostDeadBlind => (x, cur),
                _ => (x, cur + x),
            };
            committed.insert((street, name), total);
            let stack = stacks.iter().find(|(n, _)| *n == name).unwrap().1;
            let mut all_in = false;
            if t.is_contribution() {
                let g = gross.get(name).copied().unwrap_or_default() + amount;
                gross.insert(name, g);
                all_in = g == stack;
                pot += amount;
            } else if t == ActionType::UncalledBetReturned {
                pot -= amount;
                returned += amount;
            }
            let mut a = act(u16::try_from(i).unwrap(), street, name, t, amount, total);
            a.is_all_in = all_in;
            h.actions.push(a);
        }
        h.boards = if streets_dealt == 0 {
            vec![]
        } else {
            vec![CommunityCards {
                run_index: 0,
                flop: Some([c("2c"), c("7d"), c("9h")]),
                turn: (streets_dealt >= 2).then(|| c("Qc")),
                river: (streets_dealt >= 3).then(|| c("3s")),
            }]
        };
        h.outcome = HandOutcome {
            total_pot: pot,
            deductions: vec![PotDeduction {
                kind: DeductionKind::Rake,
                label: "Rake".into(),
                amount: Decimal::ZERO,
            }],
            uncalled_returned: returned,
            winners: winners
                .iter()
                .map(|(n, amt)| PotWinner {
                    player_name: (*n).to_string(),
                    amount: *amt,
                    run_index: 0,
                })
                .collect(),
            adjustments: vec![],
        };
        h
    }

    const TEN: Decimal = dec!(10);

    #[test]
    fn a_short_all_in_preflop_closes_the_betting_for_the_big_blind() {
        use ActionType::*;
        use Street::*;
        // C (first to act) calls all-in for less than the big blind and A folds: the big blind
        // has nobody left to bet against, so the unmatched 0.02 goes straight back.
        let h = scenario(
            &[("A", TEN), ("Hero", TEN), ("C", dec!(0.08))],
            3,
            &[
                (Preflop, "A", PostSmallBlind, dec!(0.05)),
                (Preflop, "Hero", PostBigBlind, dec!(0.1)),
                (Preflop, "C", Call, dec!(0.08)),
                (Preflop, "A", Fold, dec!(0)),
                (Preflop, "Hero", UncalledBetReturned, dec!(0.02)),
            ],
            3,
            &[("C", dec!(0.21))],
        );
        assert!(h.actions[2].is_all_in);
        validate_hand(&h).unwrap();

        // All-in for exactly the big blind: the big blind owes no action and the board runs out.
        let h = scenario(
            &[("A", TEN), ("Hero", TEN), ("C", dec!(0.1))],
            3,
            &[
                (Preflop, "A", PostSmallBlind, dec!(0.05)),
                (Preflop, "Hero", PostBigBlind, dec!(0.1)),
                (Preflop, "C", Call, dec!(0.1)),
                (Preflop, "A", Fold, dec!(0)),
            ],
            3,
            &[("Hero", dec!(0.25))],
        );
        validate_hand(&h).unwrap();

        // A small blind posted all-in for less than the stake is still a legal blind.
        let h = scenario(
            &[("A", dec!(0.03)), ("Hero", TEN), ("C", TEN)],
            3,
            &[
                (Preflop, "A", PostSmallBlind, dec!(0.03)),
                (Preflop, "Hero", PostBigBlind, dec!(0.1)),
                (Preflop, "C", Fold, dec!(0)),
                (Preflop, "Hero", UncalledBetReturned, dec!(0.07)),
            ],
            3,
            &[("A", dec!(0.06))],
        );
        validate_hand(&h).unwrap();
    }

    #[test]
    fn rejects_raises_without_a_bet_or_without_an_opponent_who_can_respond() {
        use ActionType::*;
        use Street::*;
        // A "raise" on a flop where nobody has bet.
        let mut h = valid_hand();
        h.actions[5].action_type = Raise;
        assert!(betting_error(&h).contains("illegal Raise"));

        // C shoves, A folds: the big blind can call, but raising is pointless and illegal
        // because nobody is left who could respond.
        let steps = |hero: (ActionType, Decimal)| {
            let mut steps = vec![
                (Preflop, "A", PostSmallBlind, dec!(0.05)),
                (Preflop, "Hero", PostBigBlind, dec!(0.1)),
                (Preflop, "C", Raise, dec!(0.5)),
                (Preflop, "A", Fold, dec!(0)),
                (Preflop, "Hero", hero.0, hero.1),
            ];
            if hero.0 == Raise {
                steps.push((Preflop, "Hero", UncalledBetReturned, dec!(1)));
            }
            steps
        };
        let stacks = [("A", TEN), ("Hero", TEN), ("C", dec!(0.5))];
        let call = scenario(
            &stacks,
            3,
            &steps((Call, dec!(0.4))),
            3,
            &[("C", dec!(1.05))],
        );
        validate_hand(&call).unwrap();
        let raise = scenario(
            &stacks,
            3,
            &steps((Raise, dec!(1.5))),
            3,
            &[("C", dec!(1.05))],
        );
        assert!(betting_error(&raise).contains("illegal Raise"));
    }

    #[test]
    fn rejects_undersized_bets_and_raises_unless_all_in() {
        use ActionType::*;
        use Street::*;
        // A flop bet smaller than the big blind.
        let mut h = valid_hand();
        h.actions[5].amount = dec!(0.05);
        h.actions[5].street_total_after = dec!(0.05);
        h.actions[6].street_total_after = dec!(0);
        h.actions[7].amount = dec!(0.05);
        h.outcome.uncalled_returned = dec!(0.05);
        assert!(betting_error(&h).contains("illegal Bet"));

        // A preflop raise of only 0.05 over the big blind, legal only as an all-in.
        let steps = [
            (Preflop, "A", PostSmallBlind, dec!(0.05)),
            (Preflop, "Hero", PostBigBlind, dec!(0.1)),
            (Preflop, "C", Raise, dec!(0.15)),
            (Preflop, "A", Fold, dec!(0)),
            (Preflop, "Hero", Fold, dec!(0)),
            (Preflop, "C", UncalledBetReturned, dec!(0.05)),
        ];
        let winners = [("C", dec!(0.25))];
        let short = scenario(
            &[("A", TEN), ("Hero", TEN), ("C", TEN)],
            3,
            &steps,
            0,
            &winners,
        );
        assert!(betting_error(&short).contains("illegal Raise"));
        let all_in = scenario(
            &[("A", TEN), ("Hero", TEN), ("C", dec!(0.15))],
            3,
            &steps,
            0,
            &winners,
        );
        validate_hand(&all_in).unwrap();

        // After an all-in flop bet of 0.05, raising to the 0.10 big blind completes the bet.
        let steps = [
            (Preflop, "A", PostSmallBlind, dec!(0.05)),
            (Preflop, "Hero", PostBigBlind, dec!(0.1)),
            (Preflop, "C", Call, dec!(0.1)),
            (Preflop, "A", Call, dec!(0.05)),
            (Preflop, "Hero", Check, dec!(0)),
            (Flop, "A", Bet, dec!(0.05)),
            (Flop, "Hero", Raise, dec!(0.1)),
            (Flop, "C", Fold, dec!(0)),
            (Flop, "Hero", UncalledBetReturned, dec!(0.05)),
        ];
        let stacks = [("A", dec!(0.15)), ("Hero", TEN), ("C", TEN)];
        let h = scenario(&stacks, 3, &steps, 3, &[("A", dec!(0.4))]);
        validate_hand(&h).unwrap();
    }

    #[test]
    fn an_incomplete_all_in_raise_does_not_reopen_the_betting() {
        use ActionType::*;
        use Street::*;
        // C raises to 0.30 (a full raise of 0.20); D shoves 0.40, only 0.10 more. C already
        // acted and faces less than a full raise, so C may call or fold but not raise.
        let stacks = [("A", TEN), ("Hero", TEN), ("C", TEN), ("D", dec!(0.4))];
        let opening = [
            (Preflop, "A", PostSmallBlind, dec!(0.05)),
            (Preflop, "Hero", PostBigBlind, dec!(0.1)),
            (Preflop, "C", Raise, dec!(0.3)),
            (Preflop, "D", Raise, dec!(0.4)),
            (Preflop, "A", Fold, dec!(0)),
            (Preflop, "Hero", Call, dec!(0.3)),
        ];
        let mut call = opening.to_vec();
        call.push((Preflop, "C", Call, dec!(0.1)));
        for street in [Flop, Turn, River] {
            call.push((street, "Hero", Check, dec!(0)));
            call.push((street, "C", Check, dec!(0)));
        }
        let h = scenario(&stacks, 4, &call, 3, &[("D", dec!(1.25))]);
        validate_hand(&h).unwrap();

        let mut reraise = opening.to_vec();
        reraise.push((Preflop, "C", Raise, dec!(1.2)));
        reraise.push((Preflop, "Hero", Fold, dec!(0)));
        reraise.push((Preflop, "C", UncalledBetReturned, dec!(0.8)));
        let h = scenario(&stacks, 4, &reraise, 3, &[("D", dec!(1.25))]);
        assert!(betting_error(&h).contains("illegal Raise"));

        // Two short shoves that together add a full raise (0.40 -> 0.55 after 0.30) do reopen it.
        let stacks = [
            ("A", dec!(0.55)),
            ("Hero", TEN),
            ("C", TEN),
            ("D", dec!(0.4)),
        ];
        let steps = [
            (Preflop, "A", PostSmallBlind, dec!(0.05)),
            (Preflop, "Hero", PostBigBlind, dec!(0.1)),
            (Preflop, "C", Raise, dec!(0.3)),
            (Preflop, "D", Raise, dec!(0.4)),
            (Preflop, "A", Raise, dec!(0.55)),
            (Preflop, "Hero", Call, dec!(0.45)),
            (Preflop, "C", Raise, dec!(1.2)),
            (Preflop, "Hero", Fold, dec!(0)),
            (Preflop, "C", UncalledBetReturned, dec!(0.65)),
        ];
        let h = scenario(
            &stacks,
            4,
            &steps,
            3,
            &[("D", dec!(1.6)), ("A", dec!(0.45))],
        );
        validate_hand(&h).unwrap();
    }

    #[test]
    fn rejects_a_whole_stack_commitment_not_marked_all_in() {
        let mut h = valid_hand();
        h.players[0].starting_stack = dec!(0.1);
        h.actions.truncate(5);
        h.boards.clear();
        h.outcome.uncalled_returned = dec!(0);
        assert!(betting_error(&h).contains("without being marked all-in"));
    }

    #[test]
    fn rejects_blinds_that_do_not_match_the_stakes() {
        // A 0.06 small blind; A's completion shrinks to 0.04, so every total still balances.
        let mut h = valid_hand();
        h.actions[0].amount = dec!(0.06);
        h.actions[0].street_total_after = dec!(0.06);
        h.actions[3].amount = dec!(0.04);
        assert!(betting_error(&h).contains("does not match the 0.05/0.1 stakes"));

        // A 0.25 big blind.
        let mut h = valid_hand();
        h.actions[1].amount = dec!(0.25);
        h.actions[1].street_total_after = dec!(0.25);
        h.actions[3].amount = dec!(0.2);
        h.actions[3].street_total_after = dec!(0.25);
        h.actions[4].street_total_after = dec!(0.25);
        assert!(betting_error(&h).contains("PostBigBlind of 0.25"));

        // Dead money larger than a big blind.
        let mut h = valid_hand();
        h.actions[2] = act(
            2,
            Street::Preflop,
            "C",
            ActionType::PostDeadBlind,
            dec!(0.15),
            dec!(0),
        );
        h.actions.insert(
            3,
            act(3, Street::Preflop, "C", ActionType::Fold, dec!(0), dec!(0)),
        );
        renumber(&mut h);
        h.outcome.total_pot = dec!(0.35);
        h.outcome.winners[0].amount = dec!(0.34);
        assert!(betting_error(&h).contains("PostDeadBlind of 0.15"));
    }

    #[test]
    fn rejects_payouts_beyond_side_pot_eligibility() {
        use ActionType::*;
        use Street::*;
        // A is all-in for 0.20 against two 1.00 stacks: A can win the 0.60 main pot only.
        let mut steps = vec![
            (Preflop, "A", PostSmallBlind, dec!(0.05)),
            (Preflop, "Hero", PostBigBlind, dec!(0.1)),
            (Preflop, "C", Raise, dec!(1)),
            (Preflop, "A", Call, dec!(0.15)),
            (Preflop, "Hero", Call, dec!(0.9)),
        ];
        for street in [Flop, Turn, River] {
            steps.push((street, "Hero", Check, dec!(0)));
            steps.push((street, "C", Check, dec!(0)));
        }
        let stacks = [("A", dec!(0.2)), ("Hero", TEN), ("C", TEN)];
        let fair = scenario(&stacks, 3, &steps, 3, &[("A", dec!(0.6)), ("C", dec!(1.6))]);
        validate_hand(&fair).unwrap();
        for winners in [
            vec![("A", dec!(2.2))],
            vec![("A", dec!(1)), ("C", dec!(1.2))],
        ] {
            let h = scenario(&stacks, 3, &steps, 3, &winners);
            assert!(betting_error(&h).contains("more than the 0.60 they could win"));
        }
    }

    #[test]
    fn split_hand_blocks_handles_crlf_bom_and_reports_start_lines() {
        let raw = "\u{feff}Hand one line 1\r\nHand one line 2\r\n\r\n\r\nHand two line 1\nHand two line 2\n\n";
        let blocks: Vec<_> = split_hand_blocks(raw).collect();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].text, "Hand one line 1\r\nHand one line 2");
        assert_eq!(blocks[0].start_line, 1);
        assert_eq!(blocks[1].text, "Hand two line 1\nHand two line 2");
        assert_eq!(blocks[1].start_line, 5);
        assert_eq!(split_hand_blocks("\n\n  \n").count(), 0);
        assert_eq!(
            first_content_line("\u{feff}\n  \n  Header  \nx"),
            Some("Header")
        );
    }
}
```

---

## FILE: src/parser/registry.rs · Milestone 2

```rust
//! Runtime selection of the right site parser for each hand history file.

use thiserror::Error;

use crate::parser::base::{HandParser, first_content_line};

/// Why no single parser could be chosen for a file.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DetectionError {
    #[error(
        "no registered parser recognizes this file (first line: '{}')",
        first_line.as_deref().unwrap_or("<empty>")
    )]
    Unrecognized { first_line: Option<String> },

    #[error("several registered parsers claim this file: {candidates:?}")]
    Ambiguous { candidates: Vec<String> },
}

/// Ordered collection of site parsers. Each file must be recognized by exactly one parser.
///
/// One parser instance corresponds to one *source*: a site format plus anything the format
/// does not state itself (for GGPoker, the account currency). Registering two parsers that
/// recognize the same format is reported as [`DetectionError::Ambiguous`] rather than guessed.
#[derive(Default)]
pub struct ParserRegistry {
    parsers: Vec<Box<dyn HandParser>>,
}

impl ParserRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a parser (builder style).
    #[must_use]
    pub fn with(mut self, parser: impl HandParser + 'static) -> Self {
        self.register(parser);
        self
    }

    /// Adds a parser.
    pub fn register(&mut self, parser: impl HandParser + 'static) {
        self.parsers.push(Box::new(parser));
    }

    /// Number of registered parsers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.parsers.len()
    }

    /// `true` if no parser is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parsers.is_empty()
    }

    /// Picks the single parser that recognizes `raw_session`.
    ///
    /// # Errors
    /// Returns [`DetectionError`] if no parser, or more than one, recognizes the text.
    pub fn detect(&self, raw_session: &str) -> Result<&dyn HandParser, DetectionError> {
        let mut matches = self.parsers.iter().filter(|p| p.recognizes(raw_session));
        match (matches.next(), matches.next()) {
            (Some(only), None) => Ok(only.as_ref()),
            (None, _) => Err(DetectionError::Unrecognized {
                first_line: first_content_line(raw_session).map(|l| l.chars().take(80).collect()),
            }),
            (Some(first), Some(second)) => {
                let mut candidates = vec![first.describe(), second.describe()];
                candidates.extend(matches.map(|p| p.describe()));
                Err(DetectionError::Ambiguous { candidates })
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::domain::canonical::{CanonicalHand, Platform};
    use crate::parser::base::ParseError;

    /// Minimal parser that recognizes files whose first line starts with `prefix`.
    struct PrefixParser {
        prefix: &'static str,
    }

    impl HandParser for PrefixParser {
        fn platform(&self) -> Platform {
            Platform::GGPoker
        }
        fn describe(&self) -> String {
            format!("prefix {}", self.prefix)
        }
        fn recognizes(&self, raw_session: &str) -> bool {
            first_content_line(raw_session).is_some_and(|l| l.starts_with(self.prefix))
        }
        fn parse_hand(&self, _raw_block: &str) -> Result<CanonicalHand, ParseError> {
            Err(ParseError::EmptyHandBlock)
        }
        fn peek_hand_id(&self, _raw_block: &str) -> Option<String> {
            None
        }
    }

    #[test]
    fn detects_exactly_one_parser_or_explains_why_not() {
        let registry = ParserRegistry::new()
            .with(PrefixParser { prefix: "Alpha" })
            .with(PrefixParser { prefix: "Beta" });
        assert_eq!(registry.len(), 2);

        let chosen = registry.detect("\n\nBeta Hand #1\nbody").unwrap();
        assert_eq!(chosen.describe(), "prefix Beta");

        assert_eq!(
            registry.detect("Gamma Hand #1").err(),
            Some(DetectionError::Unrecognized {
                first_line: Some("Gamma Hand #1".into())
            })
        );

        let ambiguous = ParserRegistry::new()
            .with(PrefixParser { prefix: "Al" })
            .with(PrefixParser { prefix: "Alpha" });
        assert!(matches!(
            ambiguous.detect("Alpha Hand #1"),
            Err(DetectionError::Ambiguous { candidates }) if candidates.len() == 2
        ));
        assert!(ParserRegistry::new().is_empty());
    }

    #[test]
    fn parse_session_uses_the_parsers_block_splitter() {
        let p = PrefixParser { prefix: "Alpha" };
        let results: Vec<_> = p.parse_session("Alpha 1\n\nAlpha 2\n").collect();
        assert_eq!(results.len(), 2);
        assert_eq!(results[1].0.start_line, 3);
        assert!(results.iter().all(|(_, r)| r.is_err()));
    }
}
```

---

## FILE: src/parser/ggpoker.rs · Milestone 3

```rust
//! State-machine parser for GGPoker (PokerCraft) No-Limit Hold'em cash hand histories.
//!
//! This module is the only place that knows the GGPoker text format. It maps GGPoker
//! vocabulary onto the site-neutral canonical model (e.g. the summary's Rake/Jackpot/Bingo/
//! Fortune/Tax fields become [`PotDeduction`]s; EV Cashout becomes a [`PlayerAdjustment`]).
//!
//! Format coverage policy: every line pattern below was observed in real GGPoker exports.
//! Anything else is rejected with [`ParseError::UnrecognizedLine`] (or a more specific error)
//! so that new formats fail loudly instead of being silently mis-parsed. To support a new
//! line, add the pattern together with a real sample hand and a test.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::LazyLock;

use chrono::NaiveDateTime;
use regex::Regex;
use rust_decimal::Decimal;

use crate::domain::canonical::{
    ActionType, AdjustmentKind, BettingLimit, CanonicalHand, CommunityCards, DeductionKind,
    GameType, HandHeader, HandOutcome, Platform, Player, PlayerAdjustment, PotDeduction, PotWinner,
    Street, StreetAction,
};
use crate::domain::card::Card;
use crate::domain::money::CurrencyCode;
use crate::parser::base::{HandParser, ParseError, first_content_line, validate_hand};

/// Compiles a static regex pattern at startup.
#[allow(clippy::expect_used)]
fn compile_static_regex(pattern: &'static str) -> Regex {
    Regex::new(pattern).expect("static regex pattern must be valid")
}

macro_rules! amt {
    () => {
        r"[0-9]+(?:\.[0-9]+)?"
    };
}

static RE_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(
        r"^Poker Hand #(?P<id>[A-Za-z0-9]+): (?P<game>.+?) \((?P<stakes>[^)]*)\) - (?P<ts>\d{4}/\d{2}/\d{2} \d{2}:\d{2}:\d{2})$",
    )
});

static RE_STAKES: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^\$(?P<sb>",
        amt!(),
        r")/\$(?P<bb>",
        amt!(),
        r")$"
    ))
});

static RE_TABLE: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(
        r"^Table '(?P<table>[^']+)' (?P<max>\d+)-max Seat #(?P<btn>\d+) is the button$",
    )
});

static RE_SEAT: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^Seat (?P<seat>\d+): (?P<name>.+?) \(\$(?P<stack>",
        amt!(),
        r") in chips\)$"
    ))
});

static RE_DEALT: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(r"^Dealt to (?P<name>.+?)(?: \[(?P<c1>\S+) (?P<c2>\S+)\])?$")
});

static RE_STREET: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(
        r"^\*\*\* (?:(?P<run>FIRST|SECOND|THIRD) )?(?P<street>HOLE CARDS|FLOP|TURN|RIVER|SHOWDOWN|SUMMARY) \*\*\*(?: \[(?P<b1>[^\]]+)\])?(?: \[(?P<b2>[^\]]+)\])?$",
    )
});

// One pattern per verb: an amount (or `to`, or `and is all-in`) is accepted only where GGPoker
// prints one, so stray text can never be silently dropped.
static RE_POST: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^(?P<name>.+?): posts (?P<kind>small blind|big blind|missed blind) \$(?P<amt>",
        amt!(),
        r")$"
    ))
});

static RE_FOLD_CHECK: LazyLock<Regex> =
    LazyLock::new(|| compile_static_regex(r"^(?P<name>.+?): (?P<verb>folds|checks)$"));

static RE_CALL_BET: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^(?P<name>.+?): (?P<verb>calls|bets) \$(?P<amt>",
        amt!(),
        r")(?P<allin> and is all-in)?$"
    ))
});

static RE_RAISE: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^(?P<name>.+?): raises \$(?P<by>",
        amt!(),
        r") to \$(?P<to>",
        amt!(),
        r")(?P<allin> and is all-in)?$"
    ))
});

static RE_SHOWS: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(r"^(?P<name>.+?): shows \[(?P<cards>[^\]]+)\](?: \(.*\))?$")
});

static RE_UNCALLED: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^Uncalled bet \(\$(?P<amt>",
        amt!(),
        r")\) returned to (?P<name>.+)$"
    ))
});

static RE_COLLECTED: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^(?P<name>.+?) collected \$(?P<amt>",
        amt!(),
        r") from pot$"
    ))
});

static RE_EV_CHOOSE: LazyLock<Regex> =
    LazyLock::new(|| compile_static_regex(r"^(?P<name>.+?): Chooses to EV Cashout$"));

static RE_EV_PAYS: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^(?P<name>.+?): Pays Cashout Risk \(\$(?P<amt>",
        amt!(),
        r")\)$"
    ))
});

static RE_TOTAL_POT: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^Total pot \$(?P<pot>",
        amt!(),
        r") \| Rake \$(?P<rake>",
        amt!(),
        r") \| Jackpot \$(?P<jackpot>",
        amt!(),
        r") \| Bingo \$(?P<bingo>",
        amt!(),
        r") \| Fortune \$(?P<fortune>",
        amt!(),
        r") \| Tax \$(?P<tax>",
        amt!(),
        r")$"
    ))
});

static RE_SUMMARY_BOARD: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(r"^(?:(?P<run>FIRST|SECOND|THIRD) )?Board \[(?P<cards>[^\]]+)\]$")
});

static RE_RUN_COUNT: LazyLock<Regex> =
    LazyLock::new(|| compile_static_regex(r"^Hand was run (?P<n>two|three) times$"));

static RE_SUMMARY_SEAT: LazyLock<Regex> =
    LazyLock::new(|| compile_static_regex(r"^Seat (?P<seat>\d+): (?P<rest>.+)$"));

// Summary seat-line grammar (after `Seat <n>: <name>` and any position labels):
// ` folded before Flop [(didn't bet)]`, ` folded on the Flop|Turn|River`,
// ` won ($x)`, ` collected ($x)`, or
// ` showed [c1 c2] and <outcome>[, and <outcome>]…[, Cashout Risk ($x)]` with one outcome per
// runout, each `won ($x)` or `lost`, optionally followed by `with <hand description>` (words and
// at most one comma, never digits, so no amount can hide in it).
static RE_SEAT_LABEL: LazyLock<Regex> =
    LazyLock::new(|| compile_static_regex(r"^ \((?P<label>button|small blind|big blind)\)"));

static RE_SEAT_FOLDED: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(
        r"^ folded (?P<when>before Flop|on the Flop|on the Turn|on the River)(?P<nobet> \(didn't bet\))?$",
    )
});

static RE_SEAT_WON: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^ (?:won|collected) \(\$(?P<amt>",
        amt!(),
        r")\)$"
    ))
});

static RE_SEAT_SHOWED: LazyLock<Regex> =
    LazyLock::new(|| compile_static_regex(r"^ showed \[(?P<cards>[^\]]+)\] and (?P<outcomes>.+)$"));

static RE_SEAT_CASHOUT_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^(?P<body>.+), Cashout Risk \(\$(?P<amt>",
        amt!(),
        r")\)$"
    ))
});

static RE_SEAT_RUN_OUTCOME: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^(?:won \(\$(?P<amt>",
        amt!(),
        r")\)|lost)(?: with [A-Za-z]+(?: [A-Za-z]+)*(?:, [A-Za-z]+(?: [A-Za-z]+)*)?)?$"
    ))
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParserPhase {
    SeatingAndBlinds,
    Play(Street),
    Showdown,
    Summary,
}

impl ParserPhase {
    const fn as_str(self) -> &'static str {
        match self {
            Self::SeatingAndBlinds => "SEATING_AND_BLINDS",
            Self::Play(Street::Preflop) => "PREFLOP",
            Self::Play(Street::Flop) => "FLOP",
            Self::Play(Street::Turn) => "TURN",
            Self::Play(Street::River) => "RIVER",
            Self::Play(Street::Showdown) | Self::Showdown => "SHOWDOWN",
            Self::Summary => "SUMMARY",
        }
    }
}

/// Parser for GGPoker No-Limit Hold'em cash hand histories.
///
/// GGPoker prints `$` whatever the account currency (observed: a CAD account), so the
/// currency cannot be read from the file and must be supplied for each source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgPokerParser {
    currency: CurrencyCode,
}

impl GgPokerParser {
    /// Creates a parser that records every amount in `currency` (the account currency).
    #[must_use]
    pub const fn new(currency: CurrencyCode) -> Self {
        Self { currency }
    }

    /// The currency recorded on every parsed hand.
    #[must_use]
    pub const fn currency(&self) -> CurrencyCode {
        self.currency
    }
}

fn cap<'h>(caps: &regex::Captures<'h>, name: &str) -> &'h str {
    caps.name(name).map_or("", |m| m.as_str())
}

fn parse_decimal(raw: &str) -> Result<Decimal, ParseError> {
    Decimal::from_str(raw).map_err(|_| ParseError::InvalidAmount {
        raw: raw.to_string(),
    })
}

fn parse_card_token(token: &str) -> Result<Card, ParseError> {
    Card::from_str(token).map_err(|_| ParseError::InvalidCardSyntax {
        token: token.to_string(),
    })
}

fn parse_card_list(bracket_content: &str) -> Result<Vec<Card>, ParseError> {
    bracket_content
        .split_whitespace()
        .map(parse_card_token)
        .collect()
}

fn parse_three_cards(bracket_content: &str) -> Result<[Card; 3], ParseError> {
    match parse_card_list(bracket_content)?.as_slice() {
        [a, b, c] => Ok([*a, *b, *c]),
        _ => Err(ParseError::InvalidCardSyntax {
            token: bracket_content.to_string(),
        }),
    }
}

fn parse_single_card(bracket_content: &str) -> Result<Card, ParseError> {
    match parse_card_list(bracket_content)?.as_slice() {
        [a] => Ok(*a),
        _ => Err(ParseError::InvalidCardSyntax {
            token: bracket_content.to_string(),
        }),
    }
}

fn run_index_from_word(word: Option<&str>) -> u8 {
    match word {
        Some("FIRST") => 1,
        Some("SECOND") => 2,
        Some("THIRD") => 3,
        _ => 0,
    }
}

fn get_or_create_board(boards: &mut Vec<CommunityCards>, run_index: u8) -> &mut CommunityCards {
    if let Some(pos) = boards.iter().position(|b| b.run_index == run_index) {
        &mut boards[pos]
    } else {
        boards.push(CommunityCards {
            run_index,
            flop: None,
            turn: None,
            river: None,
        });
        let last_idx = boards.len() - 1;
        &mut boards[last_idx]
    }
}

/// For runouts after the first (`run_index > 1`) that start on the turn or river, copies the
/// shared flop (and turn) from the first runout.
fn prepare_board_for_street(boards: &mut Vec<CommunityCards>, run_index: u8, street: Street) {
    if run_index <= 1 || boards.iter().any(|b| b.run_index == run_index) {
        return;
    }
    let shared = boards
        .iter()
        .find(|b| b.run_index == 1)
        .map(|b1| (b1.flop, b1.turn));
    if let Some((flop, turn)) = shared {
        match street {
            Street::Turn if flop.is_some() => boards.push(CommunityCards {
                run_index,
                flop,
                turn: None,
                river: None,
            }),
            Street::River if flop.is_some() && turn.is_some() => boards.push(CommunityCards {
                run_index,
                flop,
                turn,
                river: None,
            }),
            _ => {}
        }
    }
}

/// Facts read from the `*** SUMMARY ***` section, cross-checked against the hand body.
#[derive(Debug, Default)]
struct SummaryFacts {
    saw_total_pot: bool,
    /// `(run_index, cards)` for each `Board [..]` / `FIRST Board [..]` line.
    board_lines: Vec<(u8, Vec<Card>)>,
    stated_run_count: Option<usize>,
    /// Seat number -> sum of `won ($x)` / `collected ($x)` amounts.
    seat_won: HashMap<u8, Decimal>,
    /// Seat number -> `Cashout Risk ($x)`.
    seat_cashout_risk: HashMap<u8, Decimal>,
    seat_lines: Vec<u8>,
    /// Seat number -> position labels and result printed on its summary line.
    seat_summaries: HashMap<u8, SeatSummary>,
}

/// How a player's hand ended, as printed on their summary seat line.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SeatResult {
    /// `folded before Flop` / `folded on the <street>`, with or without `(didn't bet)`.
    Folded { street: Street, didnt_bet: bool },
    /// `won ($x)` / `collected ($x)` without a showdown.
    Won,
    /// `showed [c1 c2] and won|lost …`, with one outcome per runout.
    Showed { cards: Vec<Card>, outcomes: usize },
}

/// Labels and result read from one summary seat line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SeatSummary {
    button: bool,
    small_blind: bool,
    big_blind: bool,
    result: SeatResult,
}

/// EV cashout chosen but not yet settled.
#[derive(Debug)]
struct PendingCashout {
    player_name: String,
    chosen_on: Street,
    risk_paid: Option<Decimal>,
}

/// Mutable state accumulated while walking one hand block.
#[derive(Debug, Default)]
struct HandState {
    players: Vec<Player>,
    actions: Vec<StreetAction>,
    boards: Vec<CommunityCards>,
    winners: Vec<PotWinner>,
    cashouts: Vec<PendingCashout>,
    total_pot: Decimal,
    deductions: Vec<PotDeduction>,
    uncalled_returned: Decimal,
    sequence: u16,
    street_commitments: HashMap<String, Decimal>,
    /// Run whose showdown is currently being reported (`0` = single board).
    showdown_run: u8,
    /// Seats named on `Dealt to` lines, in the order printed.
    dealt_seats: Vec<u8>,
    /// Set by the first preflop line after the `Dealt to` lines.
    dealing_done: bool,
    summary: SummaryFacts,
}

impl HandState {
    fn ensure_seated(&self, name: &str) -> Result<(), ParseError> {
        if self.players.iter().any(|p| p.name == name) {
            Ok(())
        } else {
            Err(ParseError::UnknownPlayer {
                player_name: name.to_string(),
            })
        }
    }

    fn player_mut(&mut self, name: &str) -> Result<&mut Player, ParseError> {
        self.players
            .iter_mut()
            .find(|p| p.name == name)
            .ok_or_else(|| ParseError::UnknownPlayer {
                player_name: name.to_string(),
            })
    }

    fn push_action(
        &mut self,
        street: Street,
        name: &str,
        action_type: ActionType,
        amount: Decimal,
        street_total_after: Decimal,
        is_all_in: bool,
    ) -> Result<(), ParseError> {
        self.actions.push(StreetAction {
            sequence: self.sequence,
            street,
            player_name: name.to_string(),
            action_type,
            amount,
            street_total_after,
            is_all_in,
        });
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| ParseError::InvalidAmount {
                raw: "action sequence overflow".to_string(),
            })?;
        Ok(())
    }

    fn record_shown_cards(&mut self, name: &str, cards_raw: &str) -> Result<(), ParseError> {
        let cards = parse_card_list(cards_raw)?;
        if cards.is_empty() || cards.len() > 2 {
            return Err(ParseError::InvalidCardSyntax {
                token: cards_raw.to_string(),
            });
        }
        let player = self.player_mut(name)?;
        for card in cards {
            if !player.record_known_card(card) {
                return Err(ParseError::ConflictingHoleCards {
                    player_name: name.to_string(),
                });
            }
        }
        Ok(())
    }

    /// Handles a line between the seat list and `*** HOLE CARDS ***`: only blind posts.
    fn apply_blind_line(&mut self, line_idx: usize, line: &str) -> Result<(), ParseError> {
        let Some(caps) = RE_POST.captures(line) else {
            return Err(ParseError::UnrecognizedLine {
                line: line_idx,
                content: line.to_string(),
            });
        };
        let name = cap(&caps, "name");
        self.ensure_seated(name)?;
        let amount = parse_decimal(cap(&caps, "amt"))?;
        if amount <= Decimal::ZERO {
            return Err(ParseError::IllegalAction {
                line: line_idx,
                content: line.to_string(),
                reason: "amount must be positive".to_string(),
            });
        }
        let current = self
            .street_commitments
            .get(name)
            .copied()
            .unwrap_or(Decimal::ZERO);
        // GGPoker prints the small blind first, then the big blind(s) and any missed blinds.
        if cap(&caps, "kind") == "small blind" && !self.actions.is_empty() {
            return Err(ParseError::UnrecognizedLine {
                line: line_idx,
                content: line.to_string(),
            });
        }
        let (action_type, total_after) = match cap(&caps, "kind") {
            "small blind" => (ActionType::PostSmallBlind, current.checked_add(amount)),
            "big blind" => (ActionType::PostBigBlind, current.checked_add(amount)),
            _ => (ActionType::PostDeadBlind, Some(current)),
        };
        let total_after = total_after.ok_or_else(|| ParseError::InvalidAmount {
            raw: line.to_string(),
        })?;
        // GGPoker prints no "and is all-in" on a blind; a post of the whole stack is one.
        let posted_before = self
            .actions
            .iter()
            .filter(|a| a.player_name == name)
            .try_fold(Decimal::ZERO, |acc, a| acc.checked_add(a.amount))
            .ok_or_else(|| ParseError::InvalidAmount {
                raw: line.to_string(),
            })?;
        let is_all_in = self
            .players
            .iter()
            .any(|p| p.name == name && posted_before.checked_add(amount) == Some(p.starting_stack));
        self.street_commitments
            .insert(name.to_string(), total_after);
        self.push_action(
            Street::Preflop,
            name,
            action_type,
            amount,
            total_after,
            is_all_in,
        )
    }

    /// Handles a line inside a betting street (after `*** HOLE CARDS ***`).
    fn apply_play_line(
        &mut self,
        line_idx: usize,
        line: &str,
        street: Street,
    ) -> Result<(), ParseError> {
        let illegal = |reason: &str| ParseError::IllegalAction {
            line: line_idx,
            content: line.to_string(),
            reason: reason.to_string(),
        };

        if let Some(caps) = RE_UNCALLED.captures(line) {
            let name = cap(&caps, "name");
            self.ensure_seated(name)?;
            let amount = parse_decimal(cap(&caps, "amt"))?;
            let current = self
                .street_commitments
                .get(name)
                .copied()
                .unwrap_or(Decimal::ZERO);
            let new_total = current
                .checked_sub(amount)
                .filter(|t| *t >= Decimal::ZERO && amount > Decimal::ZERO)
                .ok_or_else(|| illegal("returned more than was committed on this street"))?;
            self.street_commitments.insert(name.to_string(), new_total);
            self.uncalled_returned = self
                .uncalled_returned
                .checked_add(amount)
                .ok_or_else(|| illegal("overflow"))?;
            return self.push_action(
                street,
                name,
                ActionType::UncalledBetReturned,
                amount,
                new_total,
                false,
            );
        }

        if let Some(caps) = RE_SHOWS.captures(line) {
            let name = cap(&caps, "name");
            return self.record_shown_cards(name, cap(&caps, "cards"));
        }

        if let Some(caps) = RE_EV_CHOOSE.captures(line) {
            let name = cap(&caps, "name");
            self.ensure_seated(name)?;
            // A cashout is offered only while cards are still to come, and only once.
            if street == Street::River || self.cashouts.iter().any(|c| c.player_name == name) {
                return Err(ParseError::UnsupportedAdjustment {
                    line: line_idx,
                    content: line.to_string(),
                });
            }
            self.cashouts.push(PendingCashout {
                player_name: name.to_string(),
                chosen_on: street,
                risk_paid: None,
            });
            return Ok(());
        }

        if let Some(caps) = RE_EV_PAYS.captures(line) {
            let name = cap(&caps, "name");
            let amount = parse_decimal(cap(&caps, "amt"))?;
            let pending = self
                .cashouts
                .iter_mut()
                .find(|c| c.player_name == name && c.risk_paid.is_none())
                .ok_or_else(|| ParseError::UnsupportedAdjustment {
                    line: line_idx,
                    content: line.to_string(),
                })?;
            pending.risk_paid = Some(amount);
            return Ok(());
        }

        if RE_POST.is_match(line) {
            return Err(illegal("blind posted after hole cards were dealt"));
        }

        let (name, action_type, amounts, is_all_in) = if let Some(caps) = RE_RAISE.captures(line) {
            let by = parse_decimal(cap(&caps, "by"))?;
            let to = parse_decimal(cap(&caps, "to"))?;
            (
                cap(&caps, "name"),
                ActionType::Raise,
                Some((by, to)),
                caps.name("allin").is_some(),
            )
        } else if let Some(caps) = RE_CALL_BET.captures(line) {
            let amount = parse_decimal(cap(&caps, "amt"))?;
            let t = if cap(&caps, "verb") == "calls" {
                ActionType::Call
            } else {
                ActionType::Bet
            };
            (
                cap(&caps, "name"),
                t,
                Some((amount, Decimal::ZERO)),
                caps.name("allin").is_some(),
            )
        } else if let Some(caps) = RE_FOLD_CHECK.captures(line) {
            let t = if cap(&caps, "verb") == "folds" {
                ActionType::Fold
            } else {
                ActionType::Check
            };
            (cap(&caps, "name"), t, None, false)
        } else if line.contains("Cashout") {
            return Err(ParseError::UnsupportedAdjustment {
                line: line_idx,
                content: line.to_string(),
            });
        } else {
            return Err(ParseError::UnrecognizedLine {
                line: line_idx,
                content: line.to_string(),
            });
        };
        self.ensure_seated(name)?;

        let current = self
            .street_commitments
            .get(name)
            .copied()
            .unwrap_or(Decimal::ZERO);
        let facing = self
            .street_commitments
            .values()
            .copied()
            .max()
            .unwrap_or(Decimal::ZERO);
        let (first, second) = amounts.unwrap_or((Decimal::ZERO, Decimal::ZERO));
        if amounts.is_some() && (first <= Decimal::ZERO || second < Decimal::ZERO) {
            return Err(illegal("amount must be positive"));
        }
        let add = |amt: Decimal| current.checked_add(amt).ok_or_else(|| illegal("overflow"));

        let (amount, total_after) = match action_type {
            ActionType::Fold => (Decimal::ZERO, current),
            ActionType::Check => {
                if current != facing {
                    return Err(illegal("check while facing a bet"));
                }
                (Decimal::ZERO, current)
            }
            ActionType::Call => {
                let next = add(first)?;
                if next > facing || (!is_all_in && next != facing) {
                    return Err(illegal("call amount does not match the bet faced"));
                }
                (first, next)
            }
            ActionType::Bet => {
                if facing > Decimal::ZERO {
                    return Err(illegal("bet while facing a bet (expected a raise)"));
                }
                (first, add(first)?)
            }
            _ => {
                let (raise_by, to_total) = (first, second);
                if to_total <= current
                    || to_total <= facing
                    || facing.checked_add(raise_by) != Some(to_total)
                {
                    return Err(illegal("raise arithmetic is inconsistent"));
                }
                let incremental = to_total
                    .checked_sub(current)
                    .ok_or_else(|| illegal("overflow"))?;
                (incremental, to_total)
            }
        };

        self.street_commitments
            .insert(name.to_string(), total_after);
        self.push_action(street, name, action_type, amount, total_after, is_all_in)
    }

    /// Handles a line after a `*** SHOWDOWN ***` header.
    fn apply_showdown_line(&mut self, line_idx: usize, line: &str) -> Result<(), ParseError> {
        if let Some(caps) = RE_COLLECTED.captures(line) {
            let name = cap(&caps, "name");
            self.ensure_seated(name)?;
            let amount = parse_decimal(cap(&caps, "amt"))?;
            self.winners.push(PotWinner {
                player_name: name.to_string(),
                amount,
                run_index: self.showdown_run,
            });
            return Ok(());
        }
        if let Some(caps) = RE_SHOWS.captures(line) {
            let name = cap(&caps, "name");
            return self.record_shown_cards(name, cap(&caps, "cards"));
        }
        Err(ParseError::UnrecognizedLine {
            line: line_idx,
            content: line.to_string(),
        })
    }

    /// Handles a line in the `*** SUMMARY ***` section.
    fn apply_summary_line(&mut self, line_idx: usize, line: &str) -> Result<(), ParseError> {
        let unrecognized = || ParseError::UnrecognizedLine {
            line: line_idx,
            content: line.to_string(),
        };

        if let Some(caps) = RE_TOTAL_POT.captures(line) {
            if self.summary.saw_total_pot {
                return Err(unrecognized());
            }
            self.summary.saw_total_pot = true;
            self.total_pot = parse_decimal(cap(&caps, "pot"))?;
            // Map GGPoker's summary labels onto site-neutral deduction kinds.
            for (group, label, kind) in [
                ("rake", "Rake", DeductionKind::Rake),
                ("jackpot", "Jackpot", DeductionKind::Jackpot),
                ("bingo", "Bingo", DeductionKind::Other),
                ("fortune", "Fortune", DeductionKind::Other),
                ("tax", "Tax", DeductionKind::Other),
            ] {
                self.deductions.push(PotDeduction {
                    kind,
                    label: label.to_string(),
                    amount: parse_decimal(cap(&caps, group))?,
                });
            }
            return Ok(());
        }
        if line.starts_with("Total pot ") {
            // A fee label we have never seen: refuse rather than mis-account.
            return Err(unrecognized());
        }

        if let Some(caps) = RE_SUMMARY_BOARD.captures(line) {
            let run = run_index_from_word(caps.name("run").map(|m| m.as_str()));
            let cards = parse_card_list(cap(&caps, "cards"))?;
            self.summary.board_lines.push((run, cards));
            return Ok(());
        }

        if let Some(caps) = RE_RUN_COUNT.captures(line) {
            self.summary.stated_run_count = Some(if cap(&caps, "n") == "two" { 2 } else { 3 });
            return Ok(());
        }

        if let Some(caps) = RE_SUMMARY_SEAT.captures(line) {
            let seat: u8 = cap(&caps, "seat").parse().map_err(|_| unrecognized())?;
            let rest = cap(&caps, "rest");
            let player = self
                .players
                .iter()
                .find(|p| p.seat == seat)
                .ok_or_else(unrecognized)?;
            let after_name = rest
                .strip_prefix(player.name.as_str())
                .ok_or_else(unrecognized)?;
            if !(after_name.is_empty() || after_name.starts_with(' ')) {
                return Err(unrecognized());
            }
            if self.summary.seat_lines.contains(&seat) {
                return Err(unrecognized());
            }
            self.summary.seat_lines.push(seat);

            let mut summary = SeatSummary {
                button: false,
                small_blind: false,
                big_blind: false,
                result: SeatResult::Won,
            };
            let mut tail = after_name;
            while let Some(m) = RE_SEAT_LABEL.captures(tail) {
                match cap(&m, "label") {
                    "button" => summary.button = true,
                    "small blind" => summary.small_blind = true,
                    _ => summary.big_blind = true,
                }
                let consumed = m.get(0).map_or(0, |whole| whole.end());
                tail = tail.get(consumed..).unwrap_or("");
            }
            // A line that does not follow the grammar is refused; one that mentions a cashout in
            // an unknown form is reported as an unsupported adjustment.
            let refuse = || {
                if line.contains("Cashout") {
                    ParseError::UnsupportedAdjustment {
                        line: line_idx,
                        content: line.to_string(),
                    }
                } else {
                    unrecognized()
                }
            };
            let mut won = Decimal::ZERO;
            let mut cashout_risk = None;
            summary.result = if let Some(m) = RE_SEAT_FOLDED.captures(tail) {
                let street = match cap(&m, "when") {
                    "before Flop" => Street::Preflop,
                    "on the Flop" => Street::Flop,
                    "on the Turn" => Street::Turn,
                    _ => Street::River,
                };
                SeatResult::Folded {
                    street,
                    didnt_bet: m.name("nobet").is_some(),
                }
            } else if let Some(m) = RE_SEAT_WON.captures(tail) {
                won = parse_decimal(cap(&m, "amt"))?;
                SeatResult::Won
            } else if let Some(m) = RE_SEAT_SHOWED.captures(tail) {
                let mut body = cap(&m, "outcomes");
                if let Some(c) = RE_SEAT_CASHOUT_SUFFIX.captures(body) {
                    cashout_risk = Some(parse_decimal(cap(&c, "amt"))?);
                    body = c.name("body").map_or("", |b| b.as_str());
                }
                let mut outcomes = 0usize;
                for outcome in body.split(", and ") {
                    let o = RE_SEAT_RUN_OUTCOME.captures(outcome).ok_or_else(refuse)?;
                    if let Some(amt) = o.name("amt") {
                        won = won
                            .checked_add(parse_decimal(amt.as_str())?)
                            .ok_or_else(refuse)?;
                    }
                    outcomes += 1;
                }
                SeatResult::Showed {
                    cards: parse_card_list(cap(&m, "cards"))?,
                    outcomes,
                }
            } else {
                return Err(refuse());
            };
            self.summary.seat_summaries.insert(seat, summary);
            self.summary.seat_won.insert(seat, won);
            if let Some(risk) = cashout_risk {
                self.summary.seat_cashout_risk.insert(seat, risk);
            }
            return Ok(());
        }

        Err(unrecognized())
    }

    /// Cross-checks the summary section against the hand body.
    fn check_summary(&self, header: &HandHeader) -> Result<(), ParseError> {
        let hand_id = header.site_hand_id.as_str();
        let mismatch = |detail: String| ParseError::SummaryMismatch {
            hand_id: hand_id.to_string(),
            detail,
        };
        let s = &self.summary;

        // Runouts.
        let multi = self.boards.iter().any(|b| b.run_index > 0);
        let expected_runs = if multi { Some(self.boards.len()) } else { None };
        if s.stated_run_count != expected_runs {
            return Err(mismatch(format!(
                "summary states {:?} runs, body dealt {:?}",
                s.stated_run_count, expected_runs
            )));
        }

        // Board lines: a single board must match exactly; a multi-run summary may print only
        // the cards that differ from the first runout, so it must be a suffix.
        if s.board_lines.len() != self.boards.len() {
            return Err(mismatch(format!(
                "{} board lines for {} dealt boards",
                s.board_lines.len(),
                self.boards.len()
            )));
        }
        for (run, cards) in &s.board_lines {
            let board = self
                .boards
                .iter()
                .find(|b| b.run_index == *run)
                .ok_or_else(|| mismatch(format!("summary board for unknown run {run}")))?;
            let dealt: Vec<Card> = board.cards().collect();
            let ok = if multi {
                !cards.is_empty() && dealt.ends_with(cards)
            } else {
                dealt == *cards
            };
            if !ok {
                return Err(mismatch(format!(
                    "board for run {run} differs from summary"
                )));
            }
        }

        // One seat line per player, and its winnings / cashout risk match the body.
        if s.seat_lines.len() != self.players.len() {
            return Err(mismatch(
                "summary seat lines do not match seated players".to_string(),
            ));
        }
        for p in &self.players {
            let body_won = self
                .winners
                .iter()
                .filter(|w| w.player_name == p.name)
                .try_fold(Decimal::ZERO, |acc, w| acc.checked_add(w.amount))
                .ok_or_else(|| mismatch("overflow".to_string()))?;
            let summary_won = s.seat_won.get(&p.seat).copied().unwrap_or(Decimal::ZERO);
            if body_won != summary_won {
                return Err(mismatch(format!(
                    "'{}' collected {body_won} in the body but {summary_won} in the summary",
                    p.name
                )));
            }
            let body_risk = self
                .cashouts
                .iter()
                .find(|c| c.player_name == p.name)
                .and_then(|c| c.risk_paid);
            if body_risk != s.seat_cashout_risk.get(&p.seat).copied() {
                return Err(mismatch(format!(
                    "cashout risk for '{}' differs from summary",
                    p.name
                )));
            }
        }

        // Position labels, fold streets and shown cards on each seat line match the body.
        let first_big_blind = self
            .actions
            .iter()
            .find(|a| a.action_type == ActionType::PostBigBlind)
            .map(|a| a.player_name.as_str());
        let contested = self
            .players
            .iter()
            .filter(|p| {
                !self
                    .actions
                    .iter()
                    .any(|a| a.player_name == p.name && a.action_type == ActionType::Fold)
            })
            .count()
            >= 2;
        let runs = self.boards.len().max(1);
        for p in &self.players {
            let seat_line = s
                .seat_summaries
                .get(&p.seat)
                .ok_or_else(|| mismatch(format!("no summary line for '{}'", p.name)))?;
            let own: Vec<&StreetAction> = self
                .actions
                .iter()
                .filter(|a| a.player_name == p.name)
                .collect();
            let posted_small = own
                .iter()
                .any(|a| a.action_type == ActionType::PostSmallBlind);
            if seat_line.button != (p.seat == header.button_seat)
                || seat_line.small_blind != posted_small
                || seat_line.big_blind != (first_big_blind == Some(p.name.as_str()))
            {
                return Err(mismatch(format!(
                    "position labels for '{}' disagree with the button and blinds",
                    p.name
                )));
            }
            let folded_on = own
                .iter()
                .find(|a| a.action_type == ActionType::Fold)
                .map(|a| a.street);
            let contributed = own.iter().any(|a| a.action_type.is_contribution());
            let ok = match &seat_line.result {
                SeatResult::Folded { street, didnt_bet } => {
                    folded_on == Some(*street) && *didnt_bet == !contributed
                }
                // Cards an uncontested winner shows voluntarily do not change the line.
                SeatResult::Won => folded_on.is_none() && !contested,
                SeatResult::Showed { cards, outcomes } => {
                    let mut shown = cards.clone();
                    let mut known: Vec<Card> = p.known_cards().collect();
                    shown.sort_unstable();
                    known.sort_unstable();
                    folded_on.is_none() && shown.len() == 2 && shown == known && *outcomes == runs
                }
            };
            if !ok {
                return Err(mismatch(format!(
                    "summary result for '{}' disagrees with the hand body",
                    p.name
                )));
            }
        }
        Ok(())
    }

    fn apply_street_header(
        &mut self,
        phase: ParserPhase,
        line_idx: usize,
        caps: &regex::Captures<'_>,
    ) -> Result<ParserPhase, ParseError> {
        let run_index = run_index_from_word(caps.name("run").map(|m| m.as_str()));
        let street_token = cap(caps, "street");
        let fail = || ParseError::IncompleteStreet {
            line: line_idx,
            current: phase.as_str().to_string(),
            attempted: caps.get(0).map_or("", |m| m.as_str()).to_string(),
        };
        let multi = self.boards.iter().any(|b| b.run_index > 0);

        match street_token {
            "HOLE CARDS" => {
                if phase != ParserPhase::SeatingAndBlinds || run_index != 0 {
                    return Err(fail());
                }
                Ok(ParserPhase::Play(Street::Preflop))
            }
            "FLOP" => {
                let valid = (phase == ParserPhase::Play(Street::Preflop) && run_index <= 1)
                    || (phase == ParserPhase::Play(Street::River) && run_index > 1);
                if !valid || self.boards.iter().any(|b| b.run_index == run_index) {
                    return Err(fail());
                }
                if caps.name("b2").is_some() {
                    return Err(fail());
                }
                let flop = parse_three_cards(cap(caps, "b1"))?;
                get_or_create_board(&mut self.boards, run_index).flop = Some(flop);
                self.street_commitments.clear();
                Ok(ParserPhase::Play(Street::Flop))
            }
            "TURN" | "RIVER" => {
                let street = if street_token == "TURN" {
                    Street::Turn
                } else {
                    Street::River
                };
                prepare_board_for_street(&mut self.boards, run_index, street);
                let prev_street = if street == Street::Turn {
                    Street::Flop
                } else {
                    Street::Turn
                };
                let valid_phase = phase == ParserPhase::Play(prev_street)
                    || (phase == ParserPhase::Play(Street::River) && run_index > 1);
                let board = self
                    .boards
                    .iter_mut()
                    .find(|b| b.run_index == run_index)
                    .ok_or_else(fail)?;
                let ready = match street {
                    Street::Turn => board.flop.is_some() && board.turn.is_none(),
                    _ => board.turn.is_some() && board.river.is_none(),
                };
                if !valid_phase || !ready {
                    return Err(fail());
                }
                let prefix: Vec<Card> = board.cards().collect();
                let shown_prefix = parse_card_list(cap(caps, "b1"))?;
                let Some(new_card) = caps.name("b2") else {
                    return Err(fail());
                };
                if shown_prefix != prefix {
                    return Err(ParseError::InvalidCardSyntax {
                        token: cap(caps, "b1").to_string(),
                    });
                }
                let card = parse_single_card(new_card.as_str())?;
                if street == Street::Turn {
                    board.turn = Some(card);
                } else {
                    board.river = Some(card);
                }
                self.street_commitments.clear();
                Ok(ParserPhase::Play(street))
            }
            "SHOWDOWN" => {
                let valid = match run_index {
                    0 => matches!(phase, ParserPhase::Play(_)) && !multi,
                    1 => matches!(phase, ParserPhase::Play(_)) && multi,
                    n => {
                        phase == ParserPhase::Showdown
                            && n == self.showdown_run + 1
                            && usize::from(n) <= self.boards.len()
                    }
                };
                if !valid || caps.name("b1").is_some() {
                    return Err(fail());
                }
                self.showdown_run = run_index;
                self.street_commitments.clear();
                Ok(ParserPhase::Showdown)
            }
            "SUMMARY" => {
                let all_runs_shown = !multi || usize::from(self.showdown_run) == self.boards.len();
                if phase != ParserPhase::Showdown
                    || run_index != 0
                    || caps.name("b1").is_some()
                    || !all_runs_shown
                {
                    return Err(fail());
                }
                Ok(ParserPhase::Summary)
            }
            _ => Err(fail()),
        }
    }
}

impl HandParser for GgPokerParser {
    fn platform(&self) -> Platform {
        Platform::GGPoker
    }

    fn describe(&self) -> String {
        format!("GGPoker ({})", self.currency)
    }

    /// GGPoker exports start with `Poker Hand #<id>: <game> (<stakes>) - <timestamp>`.
    /// Any game is claimed here so that unsupported GGPoker games are reported per hand
    /// as `UnsupportedGame` rather than as an unrecognized file.
    fn recognizes(&self, raw_session: &str) -> bool {
        first_content_line(raw_session).is_some_and(|line| RE_HEADER.is_match(line))
    }

    fn peek_hand_id(&self, raw_block: &str) -> Option<String> {
        let first = raw_block.lines().next()?;
        let rest = first.strip_prefix("Poker Hand #")?;
        let id = rest.split(':').next()?;
        (!id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric())).then(|| id.to_string())
    }

    fn parse_hand(&self, raw_block: &str) -> Result<CanonicalHand, ParseError> {
        let mut lines = raw_block
            .lines()
            .enumerate()
            .map(|(idx, line)| (idx + 1, line.trim_end()))
            .filter(|(_, line)| !line.is_empty());

        // --- Header line -------------------------------------------------------------------
        let (_, header_line) = lines.next().ok_or(ParseError::EmptyHandBlock)?;
        let header_caps =
            RE_HEADER
                .captures(header_line)
                .ok_or_else(|| ParseError::InvalidHeader {
                    line: header_line.to_string(),
                })?;
        let unsupported = || ParseError::UnsupportedGame {
            line: header_line.to_string(),
        };
        if cap(&header_caps, "game") != "Hold'em No Limit" {
            return Err(unsupported());
        }
        let stakes = RE_STAKES
            .captures(cap(&header_caps, "stakes"))
            .ok_or_else(unsupported)?;
        let site_hand_id = cap(&header_caps, "id").to_string();
        let small_blind = parse_decimal(cap(&stakes, "sb"))?;
        let big_blind = parse_decimal(cap(&stakes, "bb"))?;
        let ts_raw = cap(&header_caps, "ts");
        let timestamp =
            NaiveDateTime::parse_from_str(ts_raw, "%Y/%m/%d %H:%M:%S").map_err(|_| {
                ParseError::InvalidTimestamp {
                    raw: ts_raw.to_string(),
                }
            })?;

        // --- Table line --------------------------------------------------------------------
        let (_, table_line) = lines.next().ok_or_else(|| ParseError::InvalidTableHeader {
            line: String::new(),
        })?;
        let bad_table = || ParseError::InvalidTableHeader {
            line: table_line.to_string(),
        };
        let table_caps = RE_TABLE.captures(table_line).ok_or_else(bad_table)?;
        let max_seats: u8 = cap(&table_caps, "max").parse().map_err(|_| bad_table())?;
        let button_seat: u8 = cap(&table_caps, "btn").parse().map_err(|_| bad_table())?;

        let header = HandHeader {
            platform: Platform::GGPoker,
            site_hand_id,
            game_type: GameType::Cash,
            limit: BettingLimit::NoLimit,
            currency: self.currency,
            small_blind,
            big_blind,
            timestamp,
            // GGPoker exports do not state a time zone.
            utc_offset_seconds: None,
            table_name: cap(&table_caps, "table").to_string(),
            max_seats,
            button_seat,
        };

        // --- Body --------------------------------------------------------------------------
        let mut state = HandState::default();
        let mut phase = ParserPhase::SeatingAndBlinds;
        let mut last_line_idx = 2;

        for (line_idx, line) in lines {
            last_line_idx = line_idx;

            if let Some(caps) = RE_STREET.captures(line) {
                phase = state.apply_street_header(phase, line_idx, &caps)?;
                continue;
            }

            match phase {
                ParserPhase::SeatingAndBlinds => {
                    if let Some(seat_caps) = RE_SEAT.captures(line) {
                        // Seat lines come first, in ascending seat order.
                        let misplaced = || ParseError::UnrecognizedLine {
                            line: line_idx,
                            content: line.to_string(),
                        };
                        if !state.actions.is_empty() {
                            return Err(misplaced());
                        }
                        let seat: u8 = cap(&seat_caps, "seat").parse().map_err(|_| misplaced())?;
                        if state.players.last().is_some_and(|p| p.seat >= seat) {
                            return Err(misplaced());
                        }
                        state.players.push(Player {
                            seat,
                            name: cap(&seat_caps, "name").to_string(),
                            starting_stack: parse_decimal(cap(&seat_caps, "stack"))?,
                            hole_cards: [None, None],
                            is_hero: false,
                        });
                    } else {
                        state.apply_blind_line(line_idx, line)?;
                    }
                }
                ParserPhase::Play(street) => {
                    if line.starts_with("Dealt to ") {
                        // One `Dealt to` line per seated player, in seat order, straight after
                        // `*** HOLE CARDS ***`.
                        let misplaced = || ParseError::UnrecognizedLine {
                            line: line_idx,
                            content: line.to_string(),
                        };
                        if street != Street::Preflop || state.dealing_done {
                            return Err(misplaced());
                        }
                        let dealt = RE_DEALT.captures(line).ok_or_else(|| {
                            ParseError::InvalidCardSyntax {
                                token: line.to_string(),
                            }
                        })?;
                        let name = cap(&dealt, "name");
                        state.ensure_seated(name)?;
                        let seat = state
                            .players
                            .iter()
                            .find(|p| p.name == name)
                            .map_or(0, |p| p.seat);
                        if state.dealt_seats.last().is_some_and(|last| *last >= seat) {
                            return Err(misplaced());
                        }
                        state.dealt_seats.push(seat);
                        if let (Some(c1), Some(c2)) = (dealt.name("c1"), dealt.name("c2")) {
                            let c1 = parse_card_token(c1.as_str())?;
                            let c2 = parse_card_token(c2.as_str())?;
                            let player = state.player_mut(name)?;
                            if player.is_hero
                                || !player.record_known_card(c1)
                                || !player.record_known_card(c2)
                            {
                                return Err(ParseError::ConflictingHoleCards {
                                    player_name: name.to_string(),
                                });
                            }
                            player.is_hero = true;
                        }
                    } else {
                        state.dealing_done = true;
                        state.apply_play_line(line_idx, line, street)?;
                    }
                }
                ParserPhase::Showdown => state.apply_showdown_line(line_idx, line)?,
                ParserPhase::Summary => state.apply_summary_line(line_idx, line)?,
            }
        }

        if phase != ParserPhase::Summary || !state.summary.saw_total_pot {
            return Err(ParseError::IncompleteStreet {
                line: last_line_idx,
                current: phase.as_str().to_string(),
                attempted: "SUMMARY".to_string(),
            });
        }
        if state.dealt_seats.len() != state.players.len() {
            return Err(ParseError::InvariantViolation {
                hand_id: header.site_hand_id,
                detail: format!(
                    "{} 'Dealt to' line(s) for {} seated players",
                    state.dealt_seats.len(),
                    state.players.len()
                ),
            });
        }
        state.check_summary(&header)?;

        // GGPoker EV Cashout -> site-neutral player adjustment (a debit of the risk paid).
        let mut adjustments = Vec::with_capacity(state.cashouts.len());
        for c in state.cashouts {
            let Some(risk_paid) = c.risk_paid else {
                return Err(ParseError::UnsupportedAdjustment {
                    line: last_line_idx,
                    content: format!(
                        "'{}' chose EV Cashout but no 'Pays Cashout Risk' settlement was found \
                         (the receiving-side format is not yet supported)",
                        c.player_name
                    ),
                });
            };
            adjustments.push(PlayerAdjustment {
                player_name: c.player_name,
                kind: AdjustmentKind::EvCashoutRiskPaid,
                street: c.chosen_on,
                amount: -risk_paid,
            });
        }

        let hand = CanonicalHand {
            header,
            players: state.players,
            actions: state.actions,
            boards: state.boards,
            outcome: HandOutcome {
                total_pot: state.total_pot,
                deductions: state.deductions,
                uncalled_returned: state.uncalled_returned,
                winners: state.winners,
                adjustments,
            },
        };

        validate_hand(&hand)?;
        Ok(hand)
    }
}
```

---

## FILE: tests/common/mod.rs · Milestone 3

```rust
//! Shared test fixtures. Every hand below is a verbatim GGPoker export from a CAD account
//! (GGPoker prints `$` regardless of account currency).
#![allow(dead_code)]

use poker_tracker::domain::CurrencyCode;
use poker_tracker::parser::GgPokerParser;

/// Currency of the account that produced every fixture hand.
pub const FIXTURE_CURRENCY: CurrencyCode = CurrencyCode::CAD;

/// GGPoker parser configured like the account that produced the fixtures.
pub fn gg() -> GgPokerParser {
    GgPokerParser::new(FIXTURE_CURRENCY)
}

/// Full real session (294 hands, newest first). Copy your export to this path.
pub const REAL_SESSION: &str = include_str!("../fixtures/ggpoker/NLHRed15_2026-08-21.txt");

/// Two hands from table NLHRed14 (single-board showdown; multi-raise flop all-in).
pub const SAMPLE_SESSION: &str = r#"Poker Hand #HD140650645: Hold'em No Limit ($0.05/$0.1) - 2026/08/21 17:05:03
Table 'NLHRed14' 6-max Seat #4 is the button
Seat 3: 4af6a555 ($9.55 in chips)
Seat 4: Hero ($18.6 in chips)
Seat 5: 2d5ce8a9 ($12.68 in chips)
2d5ce8a9: posts small blind $0.05
4af6a555: posts big blind $0.1
*** HOLE CARDS ***
Dealt to 4af6a555 
Dealt to Hero [5h 2s]
Dealt to 2d5ce8a9 
Hero: folds
2d5ce8a9: calls $0.05
4af6a555: checks
*** FLOP *** [3c 9h 7h]
2d5ce8a9: checks
4af6a555: checks
*** TURN *** [3c 9h 7h] [Qs]
2d5ce8a9: checks
4af6a555: checks
*** RIVER *** [3c 9h 7h Qs] [9c]
2d5ce8a9: checks
4af6a555: checks
2d5ce8a9: shows [4d 8d] (a pair of Nines)
4af6a555: shows [4s 6h] (a pair of Nines)
*** SHOWDOWN ***
2d5ce8a9 collected $0.19 from pot
*** SUMMARY ***
Total pot $0.2 | Rake $0.01 | Jackpot $0 | Bingo $0 | Fortune $0 | Tax $0
Board [3c 9h 7h Qs 9c]
Seat 3: 4af6a555 (big blind) showed [4s 6h] and lost with a pair of Nines
Seat 4: Hero (button) folded before Flop (didn't bet)
Seat 5: 2d5ce8a9 (small blind) showed [4d 8d] and won ($0.19) with a pair of Nines

Poker Hand #HD140650460: Hold'em No Limit ($0.05/$0.1) - 2026/08/21 17:00:21
Table 'NLHRed14' 6-max Seat #2 is the button
Seat 1: 9f78ead0 ($11.16 in chips)
Seat 2: 6570ba77 ($10.05 in chips)
Seat 3: 4af6a555 ($9.9 in chips)
Seat 4: Hero ($10 in chips)
Seat 5: 2d5ce8a9 ($10.37 in chips)
Seat 6: 24eaa482 ($1.99 in chips)
4af6a555: posts small blind $0.05
Hero: posts big blind $0.1
*** HOLE CARDS ***
Dealt to 9f78ead0 
Dealt to 6570ba77 
Dealt to 4af6a555 
Dealt to Hero [3s 3d]
Dealt to 2d5ce8a9 
Dealt to 24eaa482 
2d5ce8a9: folds
24eaa482: calls $0.1
9f78ead0: raises $0.2 to $0.3
6570ba77: folds
4af6a555: folds
Hero: calls $0.2
24eaa482: calls $0.2
*** FLOP *** [Ah 3c 9h]
Hero: checks
24eaa482: checks
9f78ead0: bets $0.48
Hero: raises $1.74 to $2.22
24eaa482: calls $1.69 and is all-in
9f78ead0: raises $7.08 to $9.3
Hero: raises $0.4 to $9.7 and is all-in
9f78ead0: calls $0.4
Hero: shows [3s 3d] (three of a kind, Threes)
24eaa482: shows [5h 3h] (a pair of Threes)
9f78ead0: shows [Ac Qs] (a pair of Aces)
*** TURN *** [Ah 3c 9h] [As]
*** RIVER *** [Ah 3c 9h As] [6s]
*** SHOWDOWN ***
Hero collected $5.75 from pot
Hero collected $15.2 from pot
*** SUMMARY ***
Total pot $22.04 | Rake $0.99 | Jackpot $0.1 | Bingo $0 | Fortune $0 | Tax $0
Board [Ah 3c 9h As 6s]
Seat 1: 9f78ead0 showed [Ac Qs] and lost with three of a kind, Aces
Seat 2: 6570ba77 (button) folded before Flop (didn't bet)
Seat 3: 4af6a555 (small blind) folded before Flop
Seat 4: Hero (big blind) showed [3s 3d] and won ($20.95) with a full house, Threes full of Aces
Seat 5: 2d5ce8a9 folded before Flop (didn't bet)
Seat 6: 24eaa482 showed [5h 3h] and lost with two pair, Aces and Threes"#;

/// Run it twice after a flop all-in (FIRST/SECOND SHOWDOWN). (HD140647703)
pub const RIT_TWICE_FLOP_ALL_IN: &str = r#"Poker Hand #HD140647703: Hold'em No Limit ($0.05/$0.1) - 2026/08/21 19:39:21
Table 'NLHRed15' 6-max Seat #2 is the button
Seat 1: 45745f0e ($1.26 in chips)
Seat 2: Hero ($17.29 in chips)
Seat 3: 99a2b579 ($11.87 in chips)
Seat 4: a74bbd06 ($4.11 in chips)
Seat 5: 888ffeb2 ($10 in chips)
Seat 6: a3c983d ($18.58 in chips)
99a2b579: posts small blind $0.05
a74bbd06: posts big blind $0.1
*** HOLE CARDS ***
Dealt to 45745f0e 
Dealt to Hero [Ah Th]
Dealt to 99a2b579 
Dealt to a74bbd06 
Dealt to 888ffeb2 
Dealt to a3c983d 
888ffeb2: folds
a3c983d: folds
45745f0e: raises $0.25 to $0.35
Hero: raises $0.85 to $1.2
99a2b579: folds
a74bbd06: folds
45745f0e: calls $0.85
*** FIRST FLOP *** [As Tc Js]
45745f0e: bets $0.06 and is all-in
Hero: calls $0.06
45745f0e: shows [2c 7h] (Ace high)
Hero: shows [Ah Th] (two pair, Aces and Tens)
*** FIRST TURN *** [As Tc Js] [Qc]
*** FIRST RIVER *** [As Tc Js Qc] [7s]
*** SECOND TURN *** [As Tc Js] [Ks]
*** SECOND RIVER *** [As Tc Js Ks] [3h]
*** FIRST SHOWDOWN ***
Hero collected $1.28 from pot
*** SECOND SHOWDOWN ***
Hero collected $1.27 from pot
*** SUMMARY ***
Total pot $2.67 | Rake $0.12 | Jackpot $0 | Bingo $0 | Fortune $0 | Tax $0
Hand was run two times
FIRST Board [As Tc Js Qc 7s]
SECOND Board [Ks 3h]
Seat 1: 45745f0e showed [2c 7h] and lost with a pair of Sevens, and lost
Seat 2: Hero (button) showed [Ah Th] and won ($1.28) with two pair, Aces and Tens, and won ($1.27)
Seat 3: 99a2b579 (small blind) folded before Flop
Seat 4: a74bbd06 (big blind) folded before Flop
Seat 5: 888ffeb2 folded before Flop (didn't bet)
Seat 6: a3c983d folded before Flop (didn't bet)"#;

/// Run it three times after a preflop all-in. (HD140645134)
pub const RIT_THREE_TIMES_PREFLOP: &str = r#"Poker Hand #HD140645134: Hold'em No Limit ($0.05/$0.1) - 2026/08/21 18:12:58
Table 'NLHRed15' 6-max Seat #2 is the button
Seat 1: 23d4a14f ($12.37 in chips)
Seat 2: Hero ($12.14 in chips)
Seat 3: f618b25c ($16.3 in chips)
Seat 4: 251f63a4 ($10 in chips)
Seat 5: 3fe4ea4e ($9.8 in chips)
Seat 6: 345644aa ($24.36 in chips)
f618b25c: posts small blind $0.05
251f63a4: posts big blind $0.1
*** HOLE CARDS ***
Dealt to 23d4a14f 
Dealt to Hero [4s 3d]
Dealt to f618b25c 
Dealt to 251f63a4 
Dealt to 3fe4ea4e 
Dealt to 345644aa 
3fe4ea4e: raises $0.2 to $0.3
345644aa: raises $0.75 to $1.05
23d4a14f: folds
Hero: folds
f618b25c: folds
251f63a4: folds
3fe4ea4e: raises $2.25 to $3.3
345644aa: raises $21.06 to $24.36 and is all-in
3fe4ea4e: calls $6.5 and is all-in
Uncalled bet ($14.56) returned to 345644aa
345644aa: shows [As Ah]
3fe4ea4e: shows [Tc 9c]
*** FIRST FLOP *** [Jc 5h Kc]
*** FIRST TURN *** [Jc 5h Kc] [6h]
*** FIRST RIVER *** [Jc 5h Kc 6h] [2c]
*** SECOND FLOP *** [6s 3c 8h]
*** SECOND TURN *** [6s 3c 8h] [9s]
*** SECOND RIVER *** [6s 3c 8h 9s] [Js]
*** THIRD FLOP *** [8d 5s Ks]
*** THIRD TURN *** [8d 5s Ks] [6c]
*** THIRD RIVER *** [8d 5s Ks 6c] [8c]
*** FIRST SHOWDOWN ***
3fe4ea4e collected $6.23 from pot
*** SECOND SHOWDOWN ***
345644aa collected $6.23 from pot
*** THIRD SHOWDOWN ***
345644aa collected $6.23 from pot
*** SUMMARY ***
Total pot $19.75 | Rake $0.96 | Jackpot $0.1 | Bingo $0 | Fortune $0 | Tax $0
Hand was run three times
FIRST Board [Jc 5h Kc 6h 2c]
SECOND Board [6s 3c 8h 9s Js]
THIRD Board [8d 5s Ks 6c 8c]
Seat 1: 23d4a14f folded before Flop (didn't bet)
Seat 2: Hero (button) folded before Flop (didn't bet)
Seat 3: f618b25c (small blind) folded before Flop
Seat 4: 251f63a4 (big blind) folded before Flop
Seat 5: 3fe4ea4e showed [Tc 9c] and won ($6.23) with a flush King high, and lost with a pair of Nines, and lost with a pair of Eights
Seat 6: 345644aa showed [As Ah] and lost with a pair of Aces, and won ($6.23) with a pair of Aces, and won ($6.23) with two pair, Aces and Eights"#;

/// `posts missed blind` dead money plus a new player's live big blind. (HD140644923)
pub const DEAD_MISSED_BLIND: &str = r#"Poker Hand #HD140644923: Hold'em No Limit ($0.05/$0.1) - 2026/08/21 18:05:22
Table 'NLHRed15' 6-max Seat #5 is the button
Seat 1: 23d4a14f ($14.87 in chips)
Seat 2: Hero ($12.44 in chips)
Seat 3: f618b25c ($12.95 in chips)
Seat 4: f43a60c0 ($2 in chips)
Seat 5: 3fe4ea4e ($10.86 in chips)
Seat 6: 345644aa ($13.8 in chips)
345644aa: posts small blind $0.05
23d4a14f: posts big blind $0.1
f618b25c: posts missed blind $0.05
f43a60c0: posts big blind $0.1
*** HOLE CARDS ***
Dealt to 23d4a14f 
Dealt to Hero [Ks 5d]
Dealt to f618b25c 
Dealt to f43a60c0 
Dealt to 3fe4ea4e 
Dealt to 345644aa 
Hero: folds
f618b25c: calls $0.1
f43a60c0: checks
3fe4ea4e: calls $0.1
345644aa: calls $0.05
23d4a14f: checks
*** FLOP *** [Qd 8c Kh]
345644aa: checks
23d4a14f: checks
f618b25c: checks
f43a60c0: checks
3fe4ea4e: checks
*** TURN *** [Qd 8c Kh] [Ad]
345644aa: checks
23d4a14f: checks
f618b25c: checks
f43a60c0: bets $0.55
3fe4ea4e: calls $0.55
345644aa: folds
23d4a14f: folds
f618b25c: folds
*** RIVER *** [Qd 8c Kh Ad] [4d]
f43a60c0: bets $1.35 and is all-in
3fe4ea4e: folds
Uncalled bet ($1.35) returned to f43a60c0
*** SHOWDOWN ***
f43a60c0 collected $1.57 from pot
*** SUMMARY ***
Total pot $1.65 | Rake $0.08 | Jackpot $0 | Bingo $0 | Fortune $0 | Tax $0
Board [Qd 8c Kh Ad 4d]
Seat 1: 23d4a14f (big blind) folded on the Turn
Seat 2: Hero folded before Flop (didn't bet)
Seat 3: f618b25c folded on the Turn
Seat 4: f43a60c0 won ($1.57)
Seat 5: 3fe4ea4e (button) folded on the River
Seat 6: 345644aa (small blind) folded on the Turn"#;

/// EV Cashout chosen on the turn; cashout risk paid after winning. (HD140642980)
pub const EV_CASHOUT: &str = r#"Poker Hand #HD140642980: Hold'em No Limit ($0.05/$0.1) - 2026/08/21 17:03:38
Table 'NLHRed15' 6-max Seat #1 is the button
Seat 1: aedbc17e ($1.6 in chips)
Seat 2: Hero ($10.41 in chips)
Seat 3: 1020a9fa ($10 in chips)
Seat 5: 79b6d419 ($1.5 in chips)
Seat 6: 29a4659c ($9.6 in chips)
Hero: posts small blind $0.05
1020a9fa: posts big blind $0.1
*** HOLE CARDS ***
Dealt to aedbc17e 
Dealt to Hero [2s Ts]
Dealt to 1020a9fa 
Dealt to 79b6d419 
Dealt to 29a4659c 
79b6d419: raises $0.1 to $0.2
29a4659c: folds
aedbc17e: folds
Hero: folds
1020a9fa: raises $0.75 to $0.95
79b6d419: calls $0.75
*** FLOP *** [8s 2c 7c]
1020a9fa: bets $0.98
79b6d419: calls $0.55 and is all-in
Uncalled bet ($0.43) returned to 1020a9fa
1020a9fa: shows [Qc Qs] (a pair of Queens)
79b6d419: shows [Kc 9c] (King high)
*** TURN *** [8s 2c 7c] [Qh]
1020a9fa: Chooses to EV Cashout
*** RIVER *** [8s 2c 7c Qh] [7s]
1020a9fa: Pays Cashout Risk ($0.48)
*** SHOWDOWN ***
1020a9fa collected $2.81 from pot
*** SUMMARY ***
Total pot $3.05 | Rake $0.14 | Jackpot $0.1 | Bingo $0 | Fortune $0 | Tax $0
Board [8s 2c 7c Qh 7s]
Seat 1: aedbc17e (button) folded before Flop (didn't bet)
Seat 2: Hero (small blind) folded before Flop
Seat 3: 1020a9fa (big blind) showed [Qc Qs] and won ($2.81) with a full house, Queens full of Sevens, Cashout Risk ($0.48)
Seat 5: 79b6d419 showed [Kc 9c] and lost with a pair of Sevens
Seat 6: 29a4659c folded before Flop (didn't bet)"#;

/// Folded player shows one card; duplicated show lines. (HD140646106)
pub const ONE_CARD_SHOW_AFTER_FOLD: &str = r#"Poker Hand #HD140646106: Hold'em No Limit ($0.05/$0.1) - 2026/08/21 18:45:57
Table 'NLHRed15' 6-max Seat #1 is the button
Seat 1: 23d4a14f ($17.21 in chips)
Seat 2: Hero ($10.01 in chips)
Seat 3: f618b25c ($14.94 in chips)
Seat 4: 10a37d12 ($9.49 in chips)
Seat 5: 888ffeb2 ($10 in chips)
Seat 6: ff6f7609 ($2.3 in chips)
Hero: posts small blind $0.05
f618b25c: posts big blind $0.1
ff6f7609: posts big blind $0.1
*** HOLE CARDS ***
Dealt to 23d4a14f 
Dealt to Hero [As 3s]
Dealt to f618b25c 
Dealt to 10a37d12 
Dealt to 888ffeb2 
Dealt to ff6f7609 
10a37d12: folds
888ffeb2: folds
ff6f7609: checks
23d4a14f: folds
Hero: raises $0.4 to $0.5
f618b25c: calls $0.4
ff6f7609: folds
*** FLOP *** [Js 9c Td]
Hero: checks
f618b25c: checks
*** TURN *** [Js 9c Td] [2d]
Hero: bets $0.55
f618b25c: calls $0.55
*** RIVER *** [Js 9c Td 2d] [7s]
Hero: checks
f618b25c: checks
Hero: shows [As 3s] (Ace high)
f618b25c: shows [Qs 7h] (a pair of Sevens)
ff6f7609: shows [Jc] (a pair of Jacks)
Hero: shows [As 3s] (Ace high)
f618b25c: shows [Qs 7h] (a pair of Sevens)
*** SHOWDOWN ***
f618b25c collected $2.09 from pot
*** SUMMARY ***
Total pot $2.2 | Rake $0.11 | Jackpot $0 | Bingo $0 | Fortune $0 | Tax $0
Board [Js 9c Td 2d 7s]
Seat 1: 23d4a14f (button) folded before Flop (didn't bet)
Seat 2: Hero (small blind) showed [As 3s] and lost with Ace high
Seat 3: f618b25c (big blind) showed [Qs 7h] and won ($2.09) with a pair of Sevens
Seat 4: 10a37d12 folded before Flop (didn't bet)
Seat 5: 888ffeb2 folded before Flop (didn't bet)
Seat 6: ff6f7609 folded before Flop"#;

/// Three-way all-in with a side pot (GG prints only `from pot`). (HD140643875)
pub const THREE_WAY_ALL_IN_SIDE_POT: &str = r#"Poker Hand #HD140643875: Hold'em No Limit ($0.05/$0.1) - 2026/08/21 17:30:48
Table 'NLHRed15' 6-max Seat #1 is the button
Seat 1: 23d4a14f ($12.88 in chips)
Seat 2: Hero ($12.49 in chips)
Seat 3: c8f1ba23 ($10 in chips)
Seat 4: 6bf8f391 ($2.71 in chips)
Seat 5: 3fe4ea4e ($10 in chips)
Seat 6: 29a4659c ($6.96 in chips)
Hero: posts small blind $0.05
c8f1ba23: posts big blind $0.1
*** HOLE CARDS ***
Dealt to 23d4a14f 
Dealt to Hero [9s Qd]
Dealt to c8f1ba23 
Dealt to 6bf8f391 
Dealt to 3fe4ea4e 
Dealt to 29a4659c 
6bf8f391: raises $0.2 to $0.3
3fe4ea4e: raises $0.57 to $0.87
29a4659c: folds
23d4a14f: folds
Hero: folds
c8f1ba23: calls $0.77
6bf8f391: raises $1.84 to $2.71 and is all-in
3fe4ea4e: raises $7.29 to $10 and is all-in
c8f1ba23: calls $9.13 and is all-in
3fe4ea4e: shows [As Qc]
c8f1ba23: shows [Ac Tc]
6bf8f391: shows [Kh Ad]
*** FLOP *** [4c Kd 6s]
*** TURN *** [4c Kd 6s] [2d]
*** RIVER *** [4c Kd 6s 2d] [2s]
*** SHOWDOWN ***
6bf8f391 collected $7.82 from pot
3fe4ea4e collected $13.85 from pot
*** SUMMARY ***
Total pot $22.76 | Rake $0.99 | Jackpot $0.1 | Bingo $0 | Fortune $0 | Tax $0
Board [4c Kd 6s 2d 2s]
Seat 1: 23d4a14f (button) folded before Flop (didn't bet)
Seat 2: Hero (small blind) folded before Flop
Seat 3: c8f1ba23 (big blind) showed [Ac Tc] and lost with a pair of Twos
Seat 4: 6bf8f391 showed [Kh Ad] and won ($7.82) with two pair, Kings and Twos
Seat 5: 3fe4ea4e showed [As Qc] and won ($13.85) with a pair of Twos
Seat 6: 29a4659c folded before Flop (didn't bet)"#;

/// Over-shove returned as uncalled bet; split pot. (HD140646332)
pub const UNCALLED_ALL_IN_SPLIT: &str = r#"Poker Hand #HD140646332: Hold'em No Limit ($0.05/$0.1) - 2026/08/21 18:54:04
Table 'NLHRed15' 6-max Seat #6 is the button
Seat 2: Hero ($11.96 in chips)
Seat 3: 99a2b579 ($10 in chips)
Seat 4: 10a37d12 ($5.68 in chips)
Seat 5: 888ffeb2 ($10 in chips)
Seat 6: ff6f7609 ($4.82 in chips)
Hero: posts small blind $0.05
99a2b579: posts big blind $0.1
*** HOLE CARDS ***
Dealt to Hero [2d 6s]
Dealt to 99a2b579 
Dealt to 10a37d12 
Dealt to 888ffeb2 
Dealt to ff6f7609 
10a37d12: folds
888ffeb2: folds
ff6f7609: raises $0.2 to $0.3
Hero: folds
99a2b579: raises $9.7 to $10 and is all-in
ff6f7609: calls $4.52 and is all-in
Uncalled bet ($5.18) returned to 99a2b579
99a2b579: shows [Ks Ah]
ff6f7609: shows [Ad Kc]
*** FLOP *** [2c Qh 8h]
*** TURN *** [2c Qh 8h] [3s]
*** RIVER *** [2c Qh 8h 3s] [3h]
*** SHOWDOWN ***
ff6f7609 collected $4.55 from pot
99a2b579 collected $4.56 from pot
*** SUMMARY ***
Total pot $9.69 | Rake $0.48 | Jackpot $0.1 | Bingo $0 | Fortune $0 | Tax $0
Board [2c Qh 8h 3s 3h]
Seat 2: Hero (small blind) folded before Flop
Seat 3: 99a2b579 (big blind) showed [Ks Ah] and won ($4.56) with a pair of Threes
Seat 4: 10a37d12 folded before Flop (didn't bet)
Seat 5: 888ffeb2 folded before Flop (didn't bet)
Seat 6: ff6f7609 (button) showed [Ad Kc] and won ($4.55) with a pair of Threes"#;

/// Everyone folds to the big blind. (HD140646565)
pub const WALK: &str = r#"Poker Hand #HD140646565: Hold'em No Limit ($0.05/$0.1) - 2026/08/21 19:02:12
Table 'NLHRed15' 6-max Seat #2 is the button
Seat 2: Hero ($10 in chips)
Seat 3: 99a2b579 ($11.97 in chips)
Seat 4: 10a37d12 ($8.95 in chips)
Seat 5: 888ffeb2 ($10 in chips)
99a2b579: posts small blind $0.05
10a37d12: posts big blind $0.1
*** HOLE CARDS ***
Dealt to Hero [Ts 6d]
Dealt to 99a2b579 
Dealt to 10a37d12 
Dealt to 888ffeb2 
888ffeb2: folds
Hero: folds
99a2b579: folds
Uncalled bet ($0.05) returned to 10a37d12
*** SHOWDOWN ***
10a37d12 collected $0.1 from pot
*** SUMMARY ***
Total pot $0.1 | Rake $0 | Jackpot $0 | Bingo $0 | Fortune $0 | Tax $0
Seat 2: Hero (button) folded before Flop (didn't bet)
Seat 3: 99a2b579 (small blind) folded before Flop
Seat 4: 10a37d12 (big blind) collected ($0.1)
Seat 5: 888ffeb2 folded before Flop (didn't bet)"#;
```

---

## FILE: tests/ggpoker_parser.rs · Milestone 3

```rust
//! GGPoker parser tests built on verbatim real hands, plus fault injection on those hands.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::str::FromStr;

use chrono::NaiveDate;
use pretty_assertions::assert_eq;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use common::{
    DEAD_MISSED_BLIND, EV_CASHOUT, FIXTURE_CURRENCY, ONE_CARD_SHOW_AFTER_FOLD,
    RIT_THREE_TIMES_PREFLOP, RIT_TWICE_FLOP_ALL_IN, SAMPLE_SESSION, THREE_WAY_ALL_IN_SIDE_POT,
    UNCALLED_ALL_IN_SPLIT, WALK, gg,
};
use poker_tracker::domain::{
    ActionType, AdjustmentKind, CanonicalHand, Card, CurrencyCode, DeductionKind, Platform, Street,
};
use poker_tracker::parser::{GgPokerParser, HandParser, ParseError};

fn card(s: &str) -> Card {
    Card::from_str(s).unwrap()
}

fn parse(raw: &str) -> CanonicalHand {
    gg().parse_hand(raw).expect("real hand must parse")
}

fn parse_err(raw: &str) -> ParseError {
    gg().parse_hand(raw)
        .expect_err("corrupted hand must be rejected")
}

fn net_of(hand: &CanonicalHand, name: &str) -> Decimal {
    hand.checked_player_results()
        .unwrap()
        .into_iter()
        .find(|r| r.player_name == name)
        .unwrap()
        .net
}

fn first_hand(session: &str) -> &str {
    session.split("\n\n").next().unwrap()
}

// ---------------------------------------------------------------------------------------------
// Real hands
// ---------------------------------------------------------------------------------------------

#[test]
fn parses_sample_session_with_street_total_after_accuracy() {
    let parser = gg();
    let hands: Vec<_> = parser
        .parse_session(SAMPLE_SESSION)
        .map(|(_, r)| r)
        .collect::<Result<Vec<_>, _>>()
        .expect("both sample hands should parse cleanly");
    assert_eq!(hands.len(), 2);

    let h1 = &hands[0];
    assert_eq!(h1.header.platform, Platform::GGPoker);
    assert_eq!(h1.header.site_hand_id, "HD140650645");
    assert_eq!(h1.header.currency, FIXTURE_CURRENCY);
    assert_eq!(h1.header.utc_offset_seconds, None);
    assert_eq!(h1.header.small_blind, dec!(0.05));
    assert_eq!(h1.header.big_blind, dec!(0.1));
    assert_eq!(
        h1.header.timestamp,
        NaiveDate::from_ymd_opt(2026, 8, 21)
            .unwrap()
            .and_hms_opt(17, 5, 3)
            .unwrap()
    );
    let hero = h1.players.iter().find(|p| p.is_hero).unwrap();
    assert_eq!(hero.name, "Hero");
    assert_eq!(hero.hole_cards, [Some(card("5h")), Some(card("2s"))]);
    let villain = h1.players.iter().find(|p| p.name == "2d5ce8a9").unwrap();
    assert_eq!(villain.hole_cards, [Some(card("4d")), Some(card("8d"))]);

    assert_eq!(h1.actions[0].action_type, ActionType::PostSmallBlind);
    assert_eq!(h1.actions[0].street_total_after, dec!(0.05));
    assert_eq!(h1.actions[3].action_type, ActionType::Call);
    assert_eq!(h1.actions[3].amount, dec!(0.05));
    assert_eq!(h1.actions[3].street_total_after, dec!(0.10));
    assert_eq!(h1.outcome.total_pot, dec!(0.2));
    assert_eq!(h1.outcome.winners[0].player_name, "2d5ce8a9");
    assert_eq!(h1.outcome.winners[0].run_index, 0);
    assert_eq!(net_of(h1, "2d5ce8a9"), dec!(0.09));

    let h2 = &hands[1];
    let flop: Vec<_> = h2
        .actions
        .iter()
        .filter(|a| a.street == Street::Flop)
        .collect();
    assert_eq!(flop.len(), 8);
    // Hero checked, then "raises $1.74 to $2.22": incremental 2.22.
    assert_eq!(flop[3].amount, dec!(2.22));
    assert_eq!(flop[3].street_total_after, dec!(2.22));
    // 9f78ead0 bet 0.48, then "raises $7.08 to $9.3": incremental 8.82.
    assert_eq!(flop[5].amount, dec!(8.82));
    assert_eq!(flop[5].street_total_after, dec!(9.3));
    // Hero "raises $0.4 to $9.7 and is all-in": a short all-in raise.
    assert_eq!(flop[6].amount, dec!(7.48));
    assert!(flop[6].is_all_in);
    assert_eq!(h2.outcome.checked_total_collected(), Some(dec!(20.95)));
    assert_eq!(net_of(h2, "Hero"), dec!(10.95));
}

#[test]
fn summary_fees_become_labeled_site_neutral_deductions() {
    let h = parse(RIT_THREE_TIMES_PREFLOP);
    let deductions: Vec<_> = h
        .outcome
        .deductions
        .iter()
        .map(|d| (d.kind, d.label.as_str(), d.amount))
        .collect();
    assert_eq!(
        deductions,
        vec![
            (DeductionKind::Rake, "Rake", dec!(0.96)),
            (DeductionKind::Jackpot, "Jackpot", dec!(0.1)),
            (DeductionKind::Other, "Bingo", dec!(0)),
            (DeductionKind::Other, "Fortune", dec!(0)),
            (DeductionKind::Other, "Tax", dec!(0)),
        ]
    );
    assert_eq!(
        h.outcome.checked_deductions_of(DeductionKind::Rake),
        Some(dec!(0.96))
    );
}

#[test]
fn run_it_twice_after_flop_all_in() {
    let h = parse(RIT_TWICE_FLOP_ALL_IN);
    assert_eq!(h.run_count(), 2);
    let flop = Some([card("As"), card("Tc"), card("Js")]);
    assert_eq!(h.boards[0].run_index, 1);
    assert_eq!(h.boards[0].flop, flop);
    assert_eq!(h.boards[0].turn, Some(card("Qc")));
    assert_eq!(h.boards[0].river, Some(card("7s")));
    assert_eq!(h.boards[1].run_index, 2);
    assert_eq!(h.boards[1].flop, flop, "second run shares the flop");
    assert_eq!(h.boards[1].turn, Some(card("Ks")));
    assert_eq!(h.boards[1].river, Some(card("3h")));

    let runs: Vec<_> = h
        .outcome
        .winners
        .iter()
        .map(|w| (w.run_index, w.amount))
        .collect();
    assert_eq!(runs, vec![(1, dec!(1.28)), (2, dec!(1.27))]);
    let shove = h.actions.iter().find(|a| a.is_all_in).unwrap();
    assert_eq!((shove.street, shove.amount), (Street::Flop, dec!(0.06)));
    assert_eq!(net_of(&h, "Hero"), dec!(1.29));
    assert_eq!(net_of(&h, "45745f0e"), dec!(-1.26));
}

#[test]
fn run_it_three_times_after_preflop_all_in() {
    let h = parse(RIT_THREE_TIMES_PREFLOP);
    assert_eq!(h.run_count(), 3);
    assert!(h.boards.iter().all(|b| b.river.is_some()));
    assert_eq!(h.boards[2].flop, Some([card("8d"), card("5s"), card("Ks")]));
    let runs: Vec<_> = h
        .outcome
        .winners
        .iter()
        .map(|w| (w.run_index, w.player_name.as_str()))
        .collect();
    assert_eq!(
        runs,
        vec![(1, "3fe4ea4e"), (2, "345644aa"), (3, "345644aa")]
    );
    assert_eq!(h.outcome.uncalled_returned, dec!(14.56));
    assert_eq!(net_of(&h, "3fe4ea4e"), dec!(-3.57));
    assert_eq!(net_of(&h, "345644aa"), dec!(2.66));
}

#[test]
fn dead_missed_blind_enters_pot_but_not_street_commitment() {
    let h = parse(DEAD_MISSED_BLIND);
    let types: Vec<_> = h.actions.iter().take(4).map(|a| a.action_type).collect();
    assert_eq!(
        types,
        vec![
            ActionType::PostSmallBlind,
            ActionType::PostBigBlind,
            ActionType::PostDeadBlind,
            ActionType::PostBigBlind,
        ]
    );
    let dead = &h.actions[2];
    assert_eq!(
        (dead.player_name.as_str(), dead.amount),
        ("f618b25c", dec!(0.05))
    );
    assert_eq!(dead.street_total_after, dec!(0));
    let call = h
        .actions
        .iter()
        .find(|a| a.player_name == "f618b25c" && a.action_type == ActionType::Call)
        .unwrap();
    assert_eq!(
        (call.amount, call.street_total_after),
        (dec!(0.1), dec!(0.1))
    );
    assert_eq!(h.checked_total_contributed(), Some(dec!(1.65)));
    assert_eq!(net_of(&h, "f618b25c"), dec!(-0.15));
}

#[test]
fn ev_cashout_becomes_a_debit_adjustment() {
    let h = parse(EV_CASHOUT);
    assert_eq!(h.outcome.adjustments.len(), 1);
    let a = &h.outcome.adjustments[0];
    assert_eq!(
        (a.player_name.as_str(), a.kind, a.street, a.amount),
        (
            "1020a9fa",
            AdjustmentKind::EvCashoutRiskPaid,
            Street::Turn,
            dec!(-0.48)
        )
    );
    let r = h
        .checked_player_results()
        .unwrap()
        .into_iter()
        .find(|r| r.player_name == "1020a9fa")
        .unwrap();
    assert_eq!(r.invested, dec!(1.5));
    assert_eq!(r.won, dec!(2.81));
    assert_eq!(r.adjustments, dec!(-0.48));
    // Verified against the export: this player's next hand starts at 10.83 from 10.00.
    assert_eq!(r.net, dec!(0.83));
    // Pot accounting itself is unaffected by the adjustment.
    assert_eq!(h.outcome.total_pot, dec!(3.05));
}

#[test]
fn one_card_show_after_fold_is_recorded_as_partial_hole_cards() {
    let h = parse(ONE_CARD_SHOW_AFTER_FOLD);
    let p = h.players.iter().find(|p| p.name == "ff6f7609").unwrap();
    assert_eq!(p.hole_cards, [Some(card("Jc")), None]);
    let hero = h.players.iter().find(|p| p.is_hero).unwrap();
    assert_eq!(hero.hole_cards, [Some(card("As")), Some(card("3s"))]);
}

#[test]
fn three_way_all_in_with_side_pot_is_unlabeled_by_ggpoker() {
    let h = parse(THREE_WAY_ALL_IN_SIDE_POT);
    assert_eq!(h.outcome.total_pot, dec!(22.76));
    assert!(h.outcome.winners.iter().all(|w| w.run_index == 0));
    assert_eq!(net_of(&h, "c8f1ba23"), dec!(-10));
    assert_eq!(net_of(&h, "3fe4ea4e"), dec!(3.85));
    assert_eq!(net_of(&h, "6bf8f391"), dec!(5.11));
}

#[test]
fn uncalled_over_shove_and_split_pot() {
    let h = parse(UNCALLED_ALL_IN_SPLIT);
    let returned = h
        .actions
        .iter()
        .find(|a| a.action_type == ActionType::UncalledBetReturned)
        .unwrap();
    assert_eq!(
        (
            returned.player_name.as_str(),
            returned.amount,
            returned.street_total_after
        ),
        ("99a2b579", dec!(5.18), dec!(4.82))
    );
    assert_eq!(h.outcome.winners.len(), 2);
}

#[test]
fn walk_returns_small_blind_and_awards_big_blind() {
    let h = parse(WALK);
    assert!(h.boards.is_empty());
    assert_eq!(h.run_count(), 0);
    assert_eq!(net_of(&h, "10a37d12"), dec!(0.05));
    assert_eq!(net_of(&h, "99a2b579"), dec!(-0.05));
}

#[test]
fn crlf_session_parses_and_reports_block_start_lines() {
    let crlf = SAMPLE_SESSION.replace('\n', "\r\n");
    let parser = gg();
    let blocks: Vec<_> = parser.parse_session(&crlf).collect();
    assert_eq!(blocks.len(), 2);
    assert!(blocks.iter().all(|(_, r)| r.is_ok()));
    assert_eq!(blocks[0].0.start_line, 1);
    let expected_second = SAMPLE_SESSION
        .lines()
        .position(|l| l.starts_with("Poker Hand #HD140650460"))
        .unwrap()
        + 1;
    assert_eq!(blocks[1].0.start_line, expected_second);
    assert_eq!(
        parser.peek_hand_id(blocks[1].0.text).as_deref(),
        Some("HD140650460")
    );
}

#[test]
fn currency_comes_from_the_configured_source_not_the_symbol() {
    for currency in [CurrencyCode::CAD, CurrencyCode::USD, CurrencyCode::EUR] {
        let parser = GgPokerParser::new(currency);
        assert_eq!(parser.currency(), currency);
        assert_eq!(parser.parse_hand(WALK).unwrap().header.currency, currency);
    }
    assert_eq!(gg().describe(), "GGPoker (CAD)");
}

#[test]
fn recognizes_ggpoker_exports_only() {
    let parser = gg();
    assert!(parser.recognizes(SAMPLE_SESSION));
    assert!(parser.recognizes(&format!("\u{feff}\n\n{WALK}")));
    // Unsupported GGPoker games are still claimed (and rejected per hand).
    assert!(parser.recognizes(&WALK.replace("Hold'em No Limit", "PLO")));
    assert!(!parser.recognizes(
        "PokerStars Hand #254000000000: Hold'em No Limit ($0.05/$0.10 USD) - 2026/08/21 17:05:03 ET"
    ));
    assert!(!parser.recognizes("random notes\nPoker Hand #HD1: ..."));
    assert!(!parser.recognizes(""));
}

// ---------------------------------------------------------------------------------------------
// Fault injection on real hands
// ---------------------------------------------------------------------------------------------

#[test]
fn rejects_summary_arithmetic_mismatch() {
    let bad = first_hand(SAMPLE_SESSION).replace("Total pot $0.2 |", "Total pot $0.25 |");
    assert!(matches!(
        parse_err(&bad),
        ParseError::MismatchedPotAccounting { .. }
    ));
}

#[test]
fn rejects_pot_that_disagrees_with_actions() {
    let bad = first_hand(SAMPLE_SESSION).replace(
        "Total pot $0.2 | Rake $0.01",
        "Total pot $0.25 | Rake $0.06",
    );
    assert!(matches!(
        parse_err(&bad),
        ParseError::PotContributionMismatch { .. }
    ));
}

#[test]
fn rejects_invalid_card_syntax() {
    let bad = first_hand(SAMPLE_SESSION).replace("Dealt to Hero [5h 2s]", "Dealt to Hero [1h 2s]");
    assert!(matches!(
        parse_err(&bad),
        ParseError::InvalidCardSyntax { .. }
    ));
}

#[test]
fn rejects_duplicate_physical_card() {
    let bad = first_hand(SAMPLE_SESSION).replace("3c 9h 7h", "5h 9h 7h");
    assert_eq!(
        parse_err(&bad),
        ParseError::DuplicateCard {
            card: "5h".to_string()
        }
    );
}

#[test]
fn rejects_missing_street_header() {
    let bad = first_hand(SAMPLE_SESSION).replace("*** FLOP *** [3c 9h 7h]\n", "");
    assert!(matches!(
        parse_err(&bad),
        ParseError::IncompleteStreet { .. }
    ));
}

#[test]
fn rejects_truncated_hand() {
    let truncated = first_hand(SAMPLE_SESSION)
        .split("*** SUMMARY ***")
        .next()
        .unwrap();
    assert!(matches!(
        parse_err(truncated),
        ParseError::IncompleteStreet { .. }
    ));
}

#[test]
fn rejects_inconsistent_raise_arithmetic() {
    let second = SAMPLE_SESSION.split("\n\n").nth(1).unwrap();
    let bad = second.replace("raises $1.74 to $2.22", "raises $1.74 to $5.00");
    assert!(matches!(parse_err(&bad), ParseError::IllegalAction { .. }));
}

#[test]
fn rejects_check_while_facing_a_bet() {
    let bad = first_hand(SAMPLE_SESSION).replace("Hero: folds", "Hero: checks");
    assert!(matches!(parse_err(&bad), ParseError::IllegalAction { .. }));
}

#[test]
fn rejects_blind_posted_after_hole_cards() {
    let bad = WALK.replace("888ffeb2: folds", "888ffeb2: posts big blind $0.1");
    assert!(matches!(parse_err(&bad), ParseError::IllegalAction { .. }));
}

#[test]
fn rejects_unrecognized_line() {
    let bad = first_hand(SAMPLE_SESSION).replace("Hero: folds", "Hero: performs a barrel roll");
    assert!(matches!(
        parse_err(&bad),
        ParseError::UnrecognizedLine { .. }
    ));
}

#[test]
fn rejects_conflicting_shown_cards() {
    let bad = first_hand(SAMPLE_SESSION).replace(
        "2d5ce8a9: shows [4d 8d]",
        "Hero: shows [As Ks]\n2d5ce8a9: shows [4d 8d]",
    );
    assert_eq!(
        parse_err(&bad),
        ParseError::ConflictingHoleCards {
            player_name: "Hero".to_string()
        }
    );
}

#[test]
fn rejects_summary_winnings_or_board_that_disagree_with_body() {
    let bad = first_hand(SAMPLE_SESSION).replace("won ($0.19)", "won ($0.18)");
    assert!(matches!(
        parse_err(&bad),
        ParseError::SummaryMismatch { .. }
    ));
    let bad =
        first_hand(SAMPLE_SESSION).replace("Board [3c 9h 7h Qs 9c]", "Board [3c 9h 7h Qs 9d]");
    assert!(matches!(
        parse_err(&bad),
        ParseError::SummaryMismatch { .. }
    ));
    let bad = RIT_TWICE_FLOP_ALL_IN.replace("run two times", "run three times");
    assert!(matches!(
        parse_err(&bad),
        ParseError::SummaryMismatch { .. }
    ));
}

#[test]
fn rejects_unsupported_games_and_stakes_formats() {
    let plo = WALK.replace("Hold'em No Limit", "Omaha Pot Limit");
    assert!(matches!(
        parse_err(&plo),
        ParseError::UnsupportedGame { .. }
    ));
    let euro = WALK.replace("($0.05/$0.1)", "(€0.05/€0.1)");
    assert!(matches!(
        parse_err(&euro),
        ParseError::UnsupportedGame { .. }
    ));
}

#[test]
fn rejects_unknown_or_unsettled_cashout_formats() {
    let unknown = EV_CASHOUT.replace("Pays Cashout Risk ($0.48)\n", "Receives Cashout ($1.20)\n");
    assert!(matches!(
        parse_err(&unknown),
        ParseError::UnsupportedAdjustment { .. }
    ));
    let unsettled = EV_CASHOUT
        .replace("1020a9fa: Pays Cashout Risk ($0.48)\n", "")
        .replace(", Cashout Risk ($0.48)", "");
    assert!(matches!(
        parse_err(&unsettled),
        ParseError::UnsupportedAdjustment { .. }
    ));
}

#[test]
fn rejects_missing_repeated_or_skipped_actions() {
    let hand = first_hand(SAMPLE_SESSION);
    // A deleted fold: the summary still says Hero folded before the flop.
    let deleted_fold = hand.replace("Hero: folds\n", "");
    assert!(matches!(
        parse_err(&deleted_fold),
        ParseError::SummaryMismatch { .. } | ParseError::InvariantViolation { .. }
    ));
    // The same player checking twice in a row.
    let double_check = hand.replace(
        "*** FLOP *** [3c 9h 7h]\n2d5ce8a9: checks\n",
        "*** FLOP *** [3c 9h 7h]\n2d5ce8a9: checks\n2d5ce8a9: checks\n",
    );
    assert!(matches!(
        parse_err(&double_check),
        ParseError::InvariantViolation { .. }
    ));
    // A flop on which nobody acts although two players can.
    let empty_flop = hand.replace(
        "*** FLOP *** [3c 9h 7h]\n2d5ce8a9: checks\n4af6a555: checks\n",
        "*** FLOP *** [3c 9h 7h]\n",
    );
    assert!(matches!(
        parse_err(&empty_flop),
        ParseError::InvariantViolation { .. }
    ));
}

#[test]
fn rejects_summary_labels_folds_and_shown_cards_that_disagree_with_the_body() {
    let hand = first_hand(SAMPLE_SESSION);
    for (from, to) in [
        // Position labels.
        ("Seat 4: Hero (button)", "Seat 4: Hero"),
        ("2d5ce8a9 (small blind) showed", "2d5ce8a9 showed"),
        ("4af6a555 (big blind)", "4af6a555 (button)"),
        // Fold street and "(didn't bet)".
        (
            "folded before Flop (didn't bet)",
            "folded on the Flop (didn't bet)",
        ),
        ("folded before Flop (didn't bet)", "folded before Flop"),
        // Shown cards.
        ("showed [4d 8d]", "showed [Kd Kh]"),
        // A player who reached showdown reported as winning without showing.
        (
            "showed [4d 8d] and won ($0.19) with a pair of Nines",
            "won ($0.19)",
        ),
        // A line format never observed.
        ("folded before Flop (didn't bet)", "mucked [Ah Ad]"),
    ] {
        assert!(hand.contains(from), "fixture changed: {from}");
        let bad = hand.replace(from, to);
        assert!(
            matches!(
                parse_err(&bad),
                ParseError::SummaryMismatch { .. } | ParseError::UnrecognizedLine { .. }
            ),
            "{from} -> {to} was not rejected"
        );
    }
}

#[test]
fn rejects_stray_amounts_and_lines_before_the_deal() {
    let hand = first_hand(SAMPLE_SESSION);
    for (from, to) in [
        ("Hero: folds", "Hero: folds $7.5"),
        (
            "4af6a555: checks\n*** FLOP",
            "4af6a555: checks $0.1\n*** FLOP",
        ),
        ("2d5ce8a9: calls $0.05", "2d5ce8a9: calls $0.05 to $9"),
        ("2d5ce8a9: calls $0.05", "2d5ce8a9: calls"),
        (
            "*** HOLE CARDS ***",
            "Hero: shows [5h 2s]\n*** HOLE CARDS ***",
        ),
        (
            "*** HOLE CARDS ***",
            "Hero: Chooses to EV Cashout\n*** HOLE CARDS ***",
        ),
        (
            "4af6a555: posts big blind $0.1",
            "4af6a555: posts big blind $0.1 and is all-in",
        ),
    ] {
        assert!(hand.contains(from), "fixture changed: {from}");
        let bad = hand.replace(from, to);
        assert!(
            matches!(parse_err(&bad), ParseError::UnrecognizedLine { .. }),
            "{to:?} was not rejected as unrecognized"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// v3.5 review fixes
// ---------------------------------------------------------------------------------------------

/// The real-session hand with the given id, verbatim.
fn real_hand(id: &str) -> &'static str {
    common::REAL_SESSION
        .split("\n\n")
        .map(str::trim)
        .find(|block| block.starts_with(&format!("Poker Hand #{id}:")))
        .unwrap()
}

/// `hand` with its lines rearranged by `edit` (line endings and trailing spaces preserved).
fn edit_lines(hand: &str, edit: impl FnOnce(&mut Vec<&str>)) -> String {
    let mut lines: Vec<&str> = hand.lines().collect();
    edit(&mut lines);
    lines.join("\n")
}

#[test]
fn requires_exactly_one_dealt_line_per_seat_in_seat_order() {
    let hand = first_hand(SAMPLE_SESSION);
    let dealt: Vec<usize> = hand
        .lines()
        .enumerate()
        .filter(|(_, l)| l.starts_with("Dealt to "))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(dealt.len(), 3);
    let misplaced = [
        // Hero's cards moved onto another seated player's line.
        hand.replace("Dealt to Hero [5h 2s]", "Dealt to 2d5ce8a9 [5h 2s]"),
        edit_lines(hand, |l| l.insert(dealt[0], l[dealt[0]])),
        edit_lines(hand, |l| l.swap(dealt[0], dealt[2])),
        // A deal line after the first action.
        edit_lines(hand, |l| l.swap(dealt[2], dealt[2] + 1)),
    ];
    for bad in &misplaced {
        assert!(
            matches!(parse_err(bad), ParseError::UnrecognizedLine { .. }),
            "{bad}"
        );
    }
    for missing in [
        edit_lines(hand, |l| {
            l.remove(dealt[2]);
        }),
        hand.replace("Dealt to Hero [5h 2s]\n", ""),
    ] {
        assert!(
            matches!(
                parse_err(&missing),
                ParseError::InvariantViolation { ref detail, .. } if detail.contains("'Dealt to'")
            ),
            "{missing}"
        );
    }
}

#[test]
fn a_blind_that_uses_the_whole_stack_is_an_all_in() {
    // GGPoker prints no "and is all-in" on a blind (the suffix itself is still refused).
    let short = WALK.replace(
        "Seat 4: 10a37d12 ($8.95 in chips)",
        "Seat 4: 10a37d12 ($0.1 in chips)",
    );
    let h = parse(&short);
    let post = h
        .actions
        .iter()
        .find(|a| a.action_type == ActionType::PostBigBlind)
        .unwrap();
    assert!(post.is_all_in);
    assert_eq!(net_of(&h, "10a37d12"), dec!(0.05));
}

#[test]
fn seat_and_blind_lines_follow_the_observed_order() {
    let hand = first_hand(SAMPLE_SESSION);
    let swapped_seats = hand.replace(
        "Seat 3: 4af6a555 ($9.55 in chips)\nSeat 4: Hero ($18.6 in chips)",
        "Seat 4: Hero ($18.6 in chips)\nSeat 3: 4af6a555 ($9.55 in chips)",
    );
    let swapped_blinds = hand.replace(
        "2d5ce8a9: posts small blind $0.05\n4af6a555: posts big blind $0.1",
        "4af6a555: posts big blind $0.1\n2d5ce8a9: posts small blind $0.05",
    );
    for bad in [swapped_seats, swapped_blinds] {
        assert_ne!(bad, hand);
        assert!(
            matches!(parse_err(&bad), ParseError::UnrecognizedLine { .. }),
            "{bad}"
        );
    }
    // An EV Cashout chosen after the river is dealt cannot happen.
    let late = EV_CASHOUT.replace(
        "1020a9fa: Chooses to EV Cashout\n*** RIVER *** [8s 2c 7c Qh] [7s]\n",
        "*** RIVER *** [8s 2c 7c Qh] [7s]\n1020a9fa: Chooses to EV Cashout\n",
    );
    assert_ne!(late, EV_CASHOUT);
    assert!(matches!(
        parse_err(&late),
        ParseError::UnsupportedAdjustment { .. }
    ));
}

#[test]
fn summary_show_lines_follow_the_observed_grammar() {
    let hand = first_hand(SAMPLE_SESSION);
    for (from, to) in [
        // Text that could carry money after the hand description.
        (
            "won ($0.19) with a pair of Nines",
            "won ($0.19) with a pair of Nines, plus $0.50",
        ),
        (
            "lost with a pair of Nines",
            "lost with a pair of Nines (2 outs)",
        ),
        ("and lost with", "and lost to"),
    ] {
        assert!(hand.contains(from), "fixture changed: {from}");
        let bad = hand.replace(from, to);
        assert!(
            matches!(parse_err(&bad), ParseError::UnrecognizedLine { .. }),
            "{to:?} was not rejected as unrecognized"
        );
    }
    // One outcome per runout: an extra outcome on a single board disagrees with the body.
    let extra = hand.replace(
        "won ($0.19) with a pair of Nines",
        "won ($0.19) with a pair of Nines, and lost",
    );
    assert!(matches!(
        parse_err(&extra),
        ParseError::SummaryMismatch { .. }
    ));
    // Every observed shape still parses: three runouts, a bare `lost`, and a cashout suffix.
    for raw in [RIT_THREE_TIMES_PREFLOP, RIT_TWICE_FLOP_ALL_IN, EV_CASHOUT] {
        parse(raw);
    }
}

#[test]
fn an_uncontested_winner_may_show_cards() {
    // The big blind collects uncontested and shows both cards; GGPoker keeps `collected`.
    let shown = WALK.replace(
        "Uncalled bet ($0.05) returned to 10a37d12\n",
        "Uncalled bet ($0.05) returned to 10a37d12\n10a37d12: shows [Ah Kh]\n",
    );
    let h = parse(&shown);
    let bb = h.players.iter().find(|p| p.name == "10a37d12").unwrap();
    assert_eq!(bb.hole_cards, [Some(card("Ah")), Some(card("Kh"))]);
    assert_eq!(net_of(&h, "10a37d12"), dec!(0.05));
    // A contested player summarised as winning without a show is still refused (see above).
}

#[test]
fn rejects_blinds_and_payouts_the_stakes_and_side_pots_do_not_allow() {
    // A 0.06 small blind whose difference the later `raises $0.8 to $1` total would absorb.
    let bad = real_hand("HD140647049").replace(
        "Hero: posts small blind $0.05",
        "Hero: posts small blind $0.06",
    );
    assert!(matches!(
        parse_err(&bad),
        ParseError::InvariantViolation { ref detail, .. } if detail.contains("stakes")
    ));
    // The 2.71 all-in stack paid the whole pot: it can win only the main pot.
    let bad = THREE_WAY_ALL_IN_SIDE_POT
        .replace(
            "6bf8f391 collected $7.82 from pot\n3fe4ea4e collected $13.85 from pot",
            "6bf8f391 collected $21.67 from pot",
        )
        .replace("and won ($7.82)", "and won ($21.67)")
        .replace(
            "showed [As Qc] and won ($13.85) with",
            "showed [As Qc] and lost with",
        );
    assert!(matches!(
        parse_err(&bad),
        ParseError::InvariantViolation { ref detail, .. } if detail.contains("could win")
    ));
}
```

---

## FILE: tests/real_session.rs · Milestone 3

```rust
//! Whole-session reconciliation against a real GGPoker export from a CAD account
//! (`tests/fixtures/ggpoker/NLHRed15_2026-08-21.txt`, 294 hands).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::HashMap;

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sha2::{Digest, Sha256};

use common::{FIXTURE_CURRENCY, REAL_SESSION, gg};
use poker_tracker::domain::{ActionType, CanonicalHand, DeductionKind};
use poker_tracker::parser::HandParser;

fn parse_all() -> Vec<CanonicalHand> {
    let parser = gg();
    let mut hands = Vec::new();
    for (block, result) in parser.parse_session(REAL_SESSION) {
        match result {
            Ok(h) => hands.push(h),
            Err(e) => panic!(
                "hand {:?} at line {} failed: {e}",
                parser.peek_hand_id(block.text),
                block.start_line
            ),
        }
    }
    hands.sort_by_key(|h| h.header.timestamp);
    hands
}

#[test]
fn every_hand_in_the_real_session_parses() {
    let hands = parse_all();
    assert_eq!(hands.len(), 294);
    assert!(hands.iter().all(|h| h.header.currency == FIXTURE_CURRENCY));
    assert_eq!(hands.iter().filter(|h| h.run_count() >= 2).count(), 4);
    assert_eq!(
        hands
            .iter()
            .filter(|h| !h.outcome.adjustments.is_empty())
            .count(),
        1
    );
    assert_eq!(
        hands
            .iter()
            .flat_map(|h| &h.actions)
            .filter(|a| a.action_type == ActionType::PostDeadBlind)
            .count(),
        1
    );
    assert_eq!(
        hands
            .iter()
            .flat_map(|h| &h.players)
            .filter(|p| p.hole_cards[0].is_some() && p.hole_cards[1].is_none())
            .count(),
        1
    );
}

#[test]
fn money_is_conserved_across_the_whole_session() {
    // Sum of every player's net == -(all pot deductions) + (all off-pot adjustments).
    let hands = parse_all();
    let mut total_net = Decimal::ZERO;
    let mut deductions = Decimal::ZERO;
    let mut rake = Decimal::ZERO;
    let mut adjustments = Decimal::ZERO;
    for h in &hands {
        for r in h.checked_player_results().unwrap() {
            total_net += r.net;
        }
        deductions += h.outcome.checked_total_deductions().unwrap();
        rake += h
            .outcome
            .checked_deductions_of(DeductionKind::Rake)
            .unwrap();
        adjustments += h.outcome.checked_total_adjustments().unwrap();
    }
    assert_eq!(total_net, adjustments - deductions);
    assert_eq!(deductions, dec!(17.35));
    assert_eq!(adjustments, dec!(-0.48));
    assert!(rake > Decimal::ZERO && rake < deductions);
}

#[test]
fn stacks_reconcile_between_consecutive_hands() {
    // A player's next starting stack equals their previous ending stack plus any top-up.
    // In this session no stack ever drops between hands, and Hero's results reconcile with
    // the observed stack change: 10.00 -> 18.85 = +2.86 net + 5.99 of auto top-ups.
    let hands = parse_all();
    let mut ending: HashMap<String, Decimal> = HashMap::new();
    let mut hero_top_ups = Decimal::ZERO;
    let mut hero_net = Decimal::ZERO;
    for h in &hands {
        for p in &h.players {
            if let Some(prev_end) = ending.get(&p.name) {
                let gap = p.starting_stack - *prev_end;
                assert!(
                    gap >= Decimal::ZERO,
                    "{} lost {gap} between hands before {}",
                    p.name,
                    h.header.site_hand_id
                );
                if p.name == "Hero" {
                    hero_top_ups += gap;
                }
            }
        }
        for (p, r) in h.players.iter().zip(h.checked_player_results().unwrap()) {
            ending.insert(p.name.clone(), p.starting_stack + r.net);
            if p.is_hero {
                hero_net += r.net;
            }
        }
    }
    assert_eq!(hero_net, dec!(2.86));
    assert_eq!(hero_top_ups, dec!(5.99));
    assert_eq!(ending["Hero"], dec!(18.85));
}

#[test]
fn the_fixture_is_the_reference_export_byte_for_byte() {
    // Guards against editors or Git rewriting line endings or trailing spaces in the export.
    let digest = Sha256::digest(REAL_SESSION.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(REAL_SESSION.len(), 331_441);
    assert_eq!(
        hex,
        "78d1e036752c92c3064154596bd168d3bb92871c54cd8aa2626d181e627b05e4"
    );
}

#[test]
fn every_real_hand_survives_a_json_round_trip() {
    for h in parse_all() {
        let json = serde_json::to_string(&h).unwrap();
        let back: CanonicalHand = serde_json::from_str(&json).unwrap();
        assert_eq!(back, h);
    }
}
```

---

## FILE: src/storage/mod.rs · Milestone 4

```rust
//! SQLite persistence layer: `STRICT` tables, numbered migrations, per-hand savepoints,
//! per-hand provenance (every import run, file and line that contained a hand), the raw text of
//! every hand, and a quarantine for hands an import rejected.

pub mod repository;
pub mod schema;

pub use repository::{
    BatchInsertStats, DEFAULT_BUSY_TIMEOUT, DeletedImportRun, HandSource, QuarantinedHand,
    RejectedHand, SqliteRepository, StorageError, hand_text_sha256, normalized_hand_text,
};
pub use schema::{APPLICATION_ID, MIGRATIONS, PRAGMA_TUNING, SCHEMA_VERSION};
```

---

## FILE: src/storage/schema.rs · Milestone 4

```rust
//! SQLite connection pragmas, database identity and numbered schema migrations.
//!
//! The schema is site- and currency-independent: `platform` and `currency` are columns, and
//! site vocabulary is stored as canonical kind strings (`as_str()` values) plus the site's
//! original label. Kind columns are deliberately *not* restricted to a fixed list: the Rust
//! enums are the vocabulary, so adding a variant (a new action, fee or adjustment kind) needs
//! no migration. Every money column is in micro-units (millionths, `1.00 = 1,000,000`) of the
//! hand's own `currency`.
//!
//! **Changing the schema:** never edit a released migration. Append a new SQL string to
//! [`MIGRATIONS`]; databases are upgraded in order, each step in its own transaction, and
//! `PRAGMA user_version` records how many have been applied.

/// `PRAGMA application_id` of every `poker_tracker` database (`"PKTR"` in ASCII). A file with
/// another id, or an id of `0` and tables that are not a v3.4 `poker_tracker` schema, is
/// refused before anything is written to it.
pub const APPLICATION_ID: i64 = 0x504B_5452;

/// Connection-level performance and integrity pragmas, applied only after the file has been
/// identified as a `poker_tracker` database (or as a new, empty one).
pub const PRAGMA_TUNING: &str = r"
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;
PRAGMA temp_store = MEMORY;
";

/// Migration 1: baseline schema (v3.4). Money columns were then in units of `0.0001`;
/// migration 2 rescales them to true micro-units.
const MIGRATION_0001_BASELINE: &str = r"
CREATE TABLE import_runs (
    id TEXT PRIMARY KEY,
    started_at TEXT NOT NULL,
    tool_version TEXT NOT NULL
) STRICT;

CREATE TABLE players (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    platform TEXT NOT NULL,
    screen_name TEXT NOT NULL,
    UNIQUE(platform, screen_name)
) STRICT;

CREATE TABLE hands (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    platform TEXT NOT NULL CHECK (length(platform) > 0),
    site_hand_id TEXT NOT NULL CHECK (length(site_hand_id) > 0),
    game_type TEXT NOT NULL CHECK (length(game_type) > 0),
    betting_limit TEXT NOT NULL CHECK (length(betting_limit) > 0),
    currency TEXT NOT NULL CHECK (length(currency) = 3 AND currency = upper(currency)),
    small_blind_micros INTEGER NOT NULL CHECK (small_blind_micros > 0),
    big_blind_micros INTEGER NOT NULL CHECK (big_blind_micros >= small_blind_micros),
    timestamp TEXT NOT NULL,
    utc_offset_seconds INTEGER CHECK (utc_offset_seconds BETWEEN -64800 AND 64800),
    table_name TEXT NOT NULL,
    max_seats INTEGER NOT NULL CHECK (max_seats BETWEEN 2 AND 10),
    button_seat INTEGER NOT NULL CHECK (button_seat >= 1 AND button_seat <= max_seats),
    run_count INTEGER NOT NULL CHECK (run_count >= 0),
    total_pot_micros INTEGER NOT NULL CHECK (total_pot_micros >= 0),
    rake_micros INTEGER NOT NULL CHECK (rake_micros >= 0),
    jackpot_micros INTEGER NOT NULL CHECK (jackpot_micros >= 0),
    other_deductions_micros INTEGER NOT NULL CHECK (other_deductions_micros >= 0),
    uncalled_returned_micros INTEGER NOT NULL CHECK (uncalled_returned_micros >= 0),
    import_run_id TEXT REFERENCES import_runs(id) ON DELETE SET NULL,
    source_file TEXT,
    source_line INTEGER CHECK (source_line IS NULL OR source_line >= 1),
    CHECK (rake_micros + jackpot_micros + other_deductions_micros <= total_pot_micros),
    UNIQUE(platform, site_hand_id)
) STRICT;

CREATE TABLE hand_players (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    hand_id INTEGER NOT NULL REFERENCES hands(id) ON DELETE CASCADE,
    player_id INTEGER NOT NULL REFERENCES players(id) ON DELETE RESTRICT,
    seat INTEGER NOT NULL CHECK (seat >= 1),
    starting_stack_micros INTEGER NOT NULL CHECK (starting_stack_micros > 0),
    hole_card_1 INTEGER CHECK (hole_card_1 IS NULL OR (hole_card_1 >= 0 AND hole_card_1 < 52)),
    hole_card_2 INTEGER CHECK (hole_card_2 IS NULL OR (hole_card_2 >= 0 AND hole_card_2 < 52)),
    is_hero INTEGER NOT NULL CHECK (is_hero IN (0, 1)),
    invested_micros INTEGER NOT NULL CHECK (invested_micros >= 0 AND invested_micros <= starting_stack_micros),
    won_micros INTEGER NOT NULL CHECK (won_micros >= 0),
    adjustments_micros INTEGER NOT NULL,
    net_micros INTEGER NOT NULL,
    CHECK (hole_card_2 IS NULL OR (hole_card_1 IS NOT NULL AND hole_card_1 != hole_card_2)),
    CHECK (is_hero = 0 OR hole_card_2 IS NOT NULL),
    CHECK (net_micros = won_micros - invested_micros + adjustments_micros),
    UNIQUE(hand_id, seat),
    UNIQUE(hand_id, player_id)
) STRICT;

CREATE TABLE actions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    hand_id INTEGER NOT NULL REFERENCES hands(id) ON DELETE CASCADE,
    player_id INTEGER NOT NULL REFERENCES players(id) ON DELETE RESTRICT,
    sequence INTEGER NOT NULL CHECK (sequence >= 0),
    street INTEGER NOT NULL CHECK (street >= 0 AND street <= 3),
    action_type TEXT NOT NULL CHECK (length(action_type) > 0),
    amount_micros INTEGER NOT NULL CHECK (amount_micros >= 0),
    street_total_after_micros INTEGER NOT NULL CHECK (street_total_after_micros >= 0),
    is_all_in INTEGER NOT NULL CHECK (is_all_in IN (0, 1)),
    UNIQUE(hand_id, sequence)
) STRICT;

CREATE TABLE board_cards (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    hand_id INTEGER NOT NULL REFERENCES hands(id) ON DELETE CASCADE,
    run_index INTEGER NOT NULL CHECK (run_index >= 0),
    flop_1 INTEGER CHECK (flop_1 IS NULL OR (flop_1 >= 0 AND flop_1 < 52)),
    flop_2 INTEGER CHECK (flop_2 IS NULL OR (flop_2 >= 0 AND flop_2 < 52)),
    flop_3 INTEGER CHECK (flop_3 IS NULL OR (flop_3 >= 0 AND flop_3 < 52)),
    turn INTEGER CHECK (turn IS NULL OR (turn >= 0 AND turn < 52)),
    river INTEGER CHECK (river IS NULL OR (river >= 0 AND river < 52)),
    UNIQUE(hand_id, run_index)
) STRICT;

CREATE TABLE pot_winners (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    hand_id INTEGER NOT NULL REFERENCES hands(id) ON DELETE CASCADE,
    player_id INTEGER NOT NULL REFERENCES players(id) ON DELETE RESTRICT,
    pot_sequence INTEGER NOT NULL CHECK (pot_sequence >= 0),
    run_index INTEGER NOT NULL CHECK (run_index >= 0),
    amount_micros INTEGER NOT NULL CHECK (amount_micros > 0),
    UNIQUE(hand_id, pot_sequence)
) STRICT;

CREATE TABLE pot_deductions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    hand_id INTEGER NOT NULL REFERENCES hands(id) ON DELETE CASCADE,
    deduction_sequence INTEGER NOT NULL CHECK (deduction_sequence >= 0),
    kind TEXT NOT NULL CHECK (length(kind) > 0),
    site_label TEXT NOT NULL,
    amount_micros INTEGER NOT NULL CHECK (amount_micros >= 0),
    UNIQUE(hand_id, deduction_sequence)
) STRICT;

CREATE TABLE player_adjustments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    hand_id INTEGER NOT NULL REFERENCES hands(id) ON DELETE CASCADE,
    player_id INTEGER NOT NULL REFERENCES players(id) ON DELETE RESTRICT,
    adjustment_sequence INTEGER NOT NULL CHECK (adjustment_sequence >= 0),
    kind TEXT NOT NULL CHECK (length(kind) > 0),
    street INTEGER NOT NULL CHECK (street >= 0 AND street <= 3),
    amount_micros INTEGER NOT NULL CHECK (amount_micros != 0),
    UNIQUE(hand_id, adjustment_sequence)
) STRICT;

CREATE INDEX idx_hands_timestamp ON hands(timestamp);
CREATE INDEX idx_hands_currency_platform ON hands(currency, platform);
CREATE INDEX idx_hands_import_run ON hands(import_run_id);
CREATE INDEX idx_hand_players_player ON hand_players(player_id, hand_id);
CREATE INDEX idx_actions_hand_street ON actions(hand_id, street, sequence);
CREATE INDEX idx_pot_winners_hand ON pot_winners(hand_id, pot_sequence);
";

/// Migration 2 (v3.5):
///
/// - every `*_micros` column becomes true micro-units (`1.00 = 1,000,000`, previously `10,000`);
/// - `hands.raw_text` keeps the hand exactly as imported, and `hands.raw_sha256` fingerprints
///   it (SHA-256 of the text with line endings and trailing spaces normalised), so a re-import
///   with different content is detected and old hands can be re-parsed later;
/// - `hand_sightings` records every import run that contained a hand, so undoing one run never
///   deletes a hand that another run also imported;
/// - `quarantined_hands` keeps the raw text of every hand an import rejected, per run, and the
///   `quarantine_pending` view lists the entries whose text is not (or no longer) stored;
/// - the index duplicating `pot_winners`' unique `(hand_id, pot_sequence)` is dropped.
const MIGRATION_0002_MICRO_UNITS_RAW_TEXT_SIGHTINGS_QUARANTINE: &str = r"
UPDATE hands SET
    small_blind_micros = small_blind_micros * 100,
    big_blind_micros = big_blind_micros * 100,
    total_pot_micros = total_pot_micros * 100,
    rake_micros = rake_micros * 100,
    jackpot_micros = jackpot_micros * 100,
    other_deductions_micros = other_deductions_micros * 100,
    uncalled_returned_micros = uncalled_returned_micros * 100;
UPDATE hand_players SET
    starting_stack_micros = starting_stack_micros * 100,
    invested_micros = invested_micros * 100,
    won_micros = won_micros * 100,
    adjustments_micros = adjustments_micros * 100,
    net_micros = net_micros * 100;
UPDATE actions SET
    amount_micros = amount_micros * 100,
    street_total_after_micros = street_total_after_micros * 100;
UPDATE pot_winners SET amount_micros = amount_micros * 100;
UPDATE pot_deductions SET amount_micros = amount_micros * 100;
UPDATE player_adjustments SET amount_micros = amount_micros * 100;

ALTER TABLE hands ADD COLUMN raw_text TEXT;
ALTER TABLE hands ADD COLUMN raw_sha256 TEXT
    CHECK (raw_sha256 IS NULL OR (length(raw_sha256) = 64 AND raw_text IS NOT NULL));

CREATE TABLE hand_sightings (
    hand_id INTEGER NOT NULL REFERENCES hands(id) ON DELETE CASCADE,
    import_run_id TEXT NOT NULL REFERENCES import_runs(id) ON DELETE CASCADE,
    source_file TEXT NOT NULL,
    source_line INTEGER NOT NULL CHECK (source_line >= 1),
    PRIMARY KEY (hand_id, import_run_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX idx_hand_sightings_run ON hand_sightings(import_run_id);
INSERT INTO hand_sightings (hand_id, import_run_id, source_file, source_line)
    SELECT id, import_run_id, source_file, source_line FROM hands
    WHERE import_run_id IS NOT NULL AND source_file IS NOT NULL AND source_line IS NOT NULL;

CREATE TABLE quarantined_hands (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    import_run_id TEXT NOT NULL REFERENCES import_runs(id) ON DELETE CASCADE,
    raw_sha256 TEXT NOT NULL CHECK (length(raw_sha256) = 64),
    raw_text TEXT NOT NULL CHECK (length(raw_text) > 0),
    parser TEXT NOT NULL,
    site_hand_id TEXT,
    stage TEXT NOT NULL CHECK (length(stage) > 0),
    error TEXT NOT NULL,
    source_file TEXT NOT NULL,
    source_line INTEGER NOT NULL CHECK (source_line >= 1),
    UNIQUE (import_run_id, raw_sha256)
) STRICT;
CREATE INDEX idx_quarantined_hands_sha ON quarantined_hands(raw_sha256);
CREATE INDEX idx_hands_raw_sha256 ON hands(raw_sha256);
CREATE VIEW quarantine_pending AS
    SELECT q.* FROM quarantined_hands q
    WHERE NOT EXISTS (SELECT 1 FROM hands h WHERE h.raw_sha256 = q.raw_sha256);

DROP INDEX idx_pot_winners_hand;
";

/// All migrations, in order. Migration *n* (1-based) upgrades `user_version` *n − 1* to *n*.
/// Append only; never edit or reorder a released entry.
pub const MIGRATIONS: &[&str] = &[
    MIGRATION_0001_BASELINE,
    MIGRATION_0002_MICRO_UNITS_RAW_TEXT_SIGHTINGS_QUARANTINE,
];

/// Schema version this build creates and expects (`PRAGMA user_version`).
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;
```

---

## FILE: src/storage/repository.rs · Milestone 4

```rust
//! Transactional SQLite repository for batch-inserting canonical hands from any site.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::{
    Connection, ErrorCode, OpenFlags, OptionalExtension, Row, TransactionBehavior, params,
};
use rust_decimal::Decimal;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::canonical::{
    ActionType, AdjustmentKind, BettingLimit, CanonicalHand, CommunityCards, DeductionKind,
    GameType, HandHeader, HandOutcome, Platform, Player, PlayerAdjustment, PotDeduction, PotWinner,
    Street, StreetAction,
};
use crate::domain::card::Card;
use crate::domain::money::{CurrencyCode, decimal_to_micro_units, micro_units_to_decimal};
use crate::parser::base::{ParseError, validate_hand};
use crate::storage::schema::{APPLICATION_ID, MIGRATIONS, PRAGMA_TUNING, SCHEMA_VERSION};

/// How long a write waits for another connection's lock before failing.
pub const DEFAULT_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Timestamp format stored in `hands.timestamp` (fractional seconds only when present).
const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.f";

/// Errors produced by the SQLite persistence layer.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("invalid canonical hand: {0}")]
    InvalidHand(#[from] ParseError),

    #[error(
        "database schema version {found} is newer than this build supports ({expected}); upgrade poker_tracker"
    )]
    UnsupportedSchemaVersion { found: i64, expected: i64 },

    #[error("{detail}")]
    NotAPokerTrackerDatabase { detail: String },

    #[error("no database exists at '{path}'")]
    DatabaseNotFound { path: String },

    #[error(
        "hand '{site_hand_id}' is already stored in {stored}; re-importing it as {incoming} was refused (delete the earlier import run to correct it)"
    )]
    CurrencyConflict {
        site_hand_id: String,
        stored: String,
        incoming: String,
    },

    #[error(
        "hand '{site_hand_id}' is already stored with different content; the re-import was refused (delete the earlier import run to replace it)"
    )]
    ContentConflict { site_hand_id: String },

    #[error("import run {run_id} does not exist")]
    UnknownImportRun { run_id: Uuid },

    #[error("{hands} hands were passed with {sources} sources")]
    SourceCountMismatch { hands: usize, sources: usize },

    #[error(
        "monetary value {value} in field '{field}' cannot be losslessly converted to micro-units"
    )]
    LossyMonetaryConversion { field: &'static str, value: Decimal },

    #[error("player '{player_name}' referenced in hand '{hand_id}' is not seated")]
    MissingPlayerId {
        hand_id: String,
        player_name: String,
    },

    #[error("stored row in '{table}' cannot be read back: {detail}")]
    CorruptRow { table: &'static str, detail: String },
}

impl StorageError {
    /// `true` if the error concerns one hand only, so the rest of its batch can still be stored.
    /// Lock, I/O and other database-wide failures are not hand-specific and abort the batch.
    #[must_use]
    pub fn is_hand_specific(&self) -> bool {
        match self {
            Self::Sqlite(e) => e.sqlite_error_code() == Some(ErrorCode::ConstraintViolation),
            Self::InvalidHand(_)
            | Self::CurrencyConflict { .. }
            | Self::ContentConflict { .. }
            | Self::LossyMonetaryConversion { .. }
            | Self::MissingPlayerId { .. }
            | Self::CorruptRow { .. } => true,
            _ => false,
        }
    }
}

/// Where a hand was imported from. Stored with the hand so an import can be traced, undone or
/// re-parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandSource {
    pub import_run_id: Uuid,
    pub file_path: String,
    /// 1-based line in `file_path` where the hand starts.
    pub start_line: usize,
    /// The hand block exactly as it appears in the file.
    pub raw_text: String,
}

/// A hand an import could not store, kept with its raw text until a later import (for example
/// after a parser update) stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuarantinedHand {
    pub import_run_id: Uuid,
    /// Parser that handled the file (see [`crate::parser::HandParser::describe`]).
    pub parser: String,
    pub file_path: String,
    /// 1-based line in `file_path` where the hand starts.
    pub start_line: usize,
    pub site_hand_id: Option<String>,
    /// Pipeline stage that rejected the hand (e.g. `"parse"` or `"storage"`).
    pub stage: String,
    pub error: String,
    pub raw_text: String,
}

/// A hand from a batch that was not stored. The rest of the batch is unaffected.
#[derive(Debug)]
pub struct RejectedHand {
    /// Index of the hand in the slice passed to [`SqliteRepository::insert_hands_batch`].
    pub batch_index: usize,
    pub site_hand_id: String,
    pub error: StorageError,
}

/// Summary returned after committing a batch of hands.
#[derive(Debug, Default)]
pub struct BatchInsertStats {
    pub hands_inserted: usize,
    pub duplicates_skipped: usize,
    pub rejected: Vec<RejectedHand>,
}

/// What [`SqliteRepository::delete_import_run`] removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeletedImportRun {
    /// Hands that no other import run contained, deleted with all their rows.
    pub hands_deleted: usize,
    /// Hands this run contained that another import run also contained; they are kept.
    pub hands_kept: usize,
    /// Quarantined hands recorded by this run.
    pub quarantined_deleted: usize,
}

enum InsertOutcome {
    Inserted,
    Duplicate,
}

/// A hand block with line endings unified and trailing whitespace and blank lines removed: the
/// form [`hand_text_sha256`] fingerprints, so re-exporting with CRLF or stripped trailing spaces
/// is still recognised as the same hand.
#[must_use]
pub fn normalized_hand_text(raw: &str) -> String {
    raw.strip_prefix('\u{feff}')
        .unwrap_or(raw)
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Lower-case hex SHA-256 of [`normalized_hand_text`].
#[must_use]
pub fn hand_text_sha256(raw: &str) -> String {
    let digest = Sha256::digest(normalized_hand_text(raw).as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

fn to_micros(field: &'static str, value: Decimal) -> Result<i64, StorageError> {
    decimal_to_micro_units(value).ok_or(StorageError::LossyMonetaryConversion { field, value })
}

fn card_code(card: Option<Card>) -> Option<i64> {
    card.map(|c| i64::from(c.raw()))
}

fn index_i64(index: usize) -> i64 {
    i64::try_from(index).unwrap_or(i64::MAX)
}

fn invariant(hand: &CanonicalHand, detail: &str) -> StorageError {
    StorageError::InvalidHand(ParseError::InvariantViolation {
        hand_id: hand.header.site_hand_id.clone(),
        detail: detail.to_string(),
    })
}

fn now_utc() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn register_run(conn: &Connection, run_id: Uuid, started_at: &str) -> Result<(), StorageError> {
    conn.prepare_cached(
        "INSERT INTO import_runs (id, started_at, tool_version) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO NOTHING",
    )?
    .execute(params![
        run_id.to_string(),
        started_at,
        env!("CARGO_PKG_VERSION")
    ])?;
    Ok(())
}

fn record_sighting(
    conn: &Connection,
    hand_db_id: i64,
    src: &HandSource,
) -> Result<(), StorageError> {
    conn.prepare_cached(
        "INSERT INTO hand_sightings (hand_id, import_run_id, source_file, source_line)
         VALUES (?1, ?2, ?3, ?4) ON CONFLICT DO NOTHING",
    )?
    .execute(params![
        hand_db_id,
        src.import_run_id.to_string(),
        src.file_path,
        index_i64(src.start_line)
    ])?;
    Ok(())
}

/// Inserts one validated hand. Must run inside a transaction/savepoint owned by the caller.
fn insert_one(
    conn: &Connection,
    hand: &CanonicalHand,
    source: Option<&HandSource>,
) -> Result<InsertOutcome, StorageError> {
    let h = &hand.header;
    let o = &hand.outcome;
    let platform = h.platform.as_str();
    let raw_sha = source.map(|src| hand_text_sha256(&src.raw_text));
    let deduction = |kind: DeductionKind, field: &'static str| -> Result<i64, StorageError> {
        let total = o
            .checked_deductions_of(kind)
            .ok_or_else(|| invariant(hand, "deduction overflow"))?;
        to_micros(field, total)
    };

    let changes = conn
        .prepare_cached(
            r"
            INSERT INTO hands (
                platform, site_hand_id, game_type, betting_limit, currency,
                small_blind_micros, big_blind_micros, timestamp, utc_offset_seconds, table_name,
                max_seats, button_seat, run_count,
                total_pot_micros, rake_micros, jackpot_micros, other_deductions_micros,
                uncalled_returned_micros, import_run_id, source_file, source_line,
                raw_text, raw_sha256
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                      ?19, ?20, ?21, ?22, ?23)
            ON CONFLICT(platform, site_hand_id) DO NOTHING
            ",
        )?
        .execute(params![
            platform,
            h.site_hand_id,
            h.game_type.as_str(),
            h.limit.as_str(),
            h.currency.as_str(),
            to_micros("small_blind", h.small_blind)?,
            to_micros("big_blind", h.big_blind)?,
            h.timestamp.format(TIMESTAMP_FORMAT).to_string(),
            h.utc_offset_seconds,
            h.table_name,
            i64::from(h.max_seats),
            i64::from(h.button_seat),
            index_i64(hand.run_count()),
            to_micros("total_pot", o.total_pot)?,
            deduction(DeductionKind::Rake, "rake")?,
            deduction(DeductionKind::Jackpot, "jackpot")?,
            deduction(DeductionKind::Other, "other_deductions")?,
            to_micros("uncalled_returned", o.uncalled_returned)?,
            source.map(|src| src.import_run_id.to_string()),
            source.map(|src| src.file_path.as_str()),
            source.map(|src| index_i64(src.start_line)),
            source.map(|src| src.raw_text.as_str()),
            raw_sha.as_deref(),
        ])?;
    if changes == 0 {
        // Already stored: a duplicate only if it is the same hand in the same currency.
        let (stored_id, stored_currency, stored_sha): (i64, String, Option<String>) = conn
            .prepare_cached(
                "SELECT id, currency, raw_sha256 FROM hands WHERE platform = ?1 AND site_hand_id = ?2",
            )?
            .query_row(params![platform, h.site_hand_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
        if stored_currency != h.currency.as_str() {
            return Err(StorageError::CurrencyConflict {
                site_hand_id: h.site_hand_id.clone(),
                stored: stored_currency,
                incoming: h.currency.to_string(),
            });
        }
        let same = match (stored_sha.as_deref(), raw_sha.as_deref()) {
            (Some(stored), Some(incoming)) => stored == incoming,
            _ => load_hand_by_id(conn, stored_id)? == *hand,
        };
        if !same {
            return Err(StorageError::ContentConflict {
                site_hand_id: h.site_hand_id.clone(),
            });
        }
        if let Some(src) = source {
            record_sighting(conn, stored_id, src)?;
            conn.prepare_cached(
                "UPDATE hands SET raw_text = ?1, raw_sha256 = ?2 WHERE id = ?3 AND raw_sha256 IS NULL",
            )?
            .execute(params![src.raw_text, raw_sha.as_deref(), stored_id])?;
        }
        return Ok(InsertOutcome::Duplicate);
    }
    let hand_db_id = conn.last_insert_rowid();
    if let Some(src) = source {
        record_sighting(conn, hand_db_id, src)?;
    }

    let results = hand
        .checked_player_results()
        .ok_or_else(|| invariant(hand, "player result overflow"))?;

    let mut player_ids: HashMap<&str, i64> = HashMap::with_capacity(hand.players.len());
    for (player, result) in hand.players.iter().zip(&results) {
        conn.prepare_cached(
            "INSERT INTO players (platform, screen_name) VALUES (?1, ?2)
             ON CONFLICT(platform, screen_name) DO NOTHING",
        )?
        .execute(params![platform, player.name])?;
        let player_id: i64 = conn
            .prepare_cached("SELECT id FROM players WHERE platform = ?1 AND screen_name = ?2")?
            .query_row(params![platform, player.name], |row| row.get(0))?;
        player_ids.insert(player.name.as_str(), player_id);

        conn.prepare_cached(
            r"
            INSERT INTO hand_players (
                hand_id, player_id, seat, starting_stack_micros, hole_card_1, hole_card_2,
                is_hero, invested_micros, won_micros, adjustments_micros, net_micros
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ",
        )?
        .execute(params![
            hand_db_id,
            player_id,
            i64::from(player.seat),
            to_micros("starting_stack", player.starting_stack)?,
            card_code(player.hole_cards[0]),
            card_code(player.hole_cards[1]),
            i64::from(u8::from(player.is_hero)),
            to_micros("invested", result.invested)?,
            to_micros("won", result.won)?,
            to_micros("adjustments", result.adjustments)?,
            to_micros("net", result.net)?,
        ])?;
    }

    let player_id = |name: &str| -> Result<i64, StorageError> {
        player_ids
            .get(name)
            .copied()
            .ok_or_else(|| StorageError::MissingPlayerId {
                hand_id: h.site_hand_id.clone(),
                player_name: name.to_string(),
            })
    };

    for act in &hand.actions {
        conn.prepare_cached(
            r"
            INSERT INTO actions (
                hand_id, player_id, sequence, street, action_type,
                amount_micros, street_total_after_micros, is_all_in
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
        )?
        .execute(params![
            hand_db_id,
            player_id(&act.player_name)?,
            i64::from(act.sequence),
            i64::from(act.street.code()),
            act.action_type.as_str(),
            to_micros("action_amount", act.amount)?,
            to_micros("street_total_after", act.street_total_after)?,
            i64::from(u8::from(act.is_all_in)),
        ])?;
    }

    for board in &hand.boards {
        let flop = board.flop.map_or([None, None, None], |f| f.map(Some));
        conn.prepare_cached(
            r"
            INSERT INTO board_cards (hand_id, run_index, flop_1, flop_2, flop_3, turn, river)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
        )?
        .execute(params![
            hand_db_id,
            i64::from(board.run_index),
            card_code(flop[0]),
            card_code(flop[1]),
            card_code(flop[2]),
            card_code(board.turn),
            card_code(board.river),
        ])?;
    }

    for (seq, winner) in o.winners.iter().enumerate() {
        conn.prepare_cached(
            r"
            INSERT INTO pot_winners (hand_id, player_id, pot_sequence, run_index, amount_micros)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ",
        )?
        .execute(params![
            hand_db_id,
            player_id(&winner.player_name)?,
            index_i64(seq),
            i64::from(winner.run_index),
            to_micros("pot_winner_amount", winner.amount)?,
        ])?;
    }

    for (seq, d) in o.deductions.iter().enumerate() {
        conn.prepare_cached(
            r"
            INSERT INTO pot_deductions (hand_id, deduction_sequence, kind, site_label, amount_micros)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ",
        )?
        .execute(params![
            hand_db_id,
            index_i64(seq),
            d.kind.as_str(),
            d.label,
            to_micros("deduction_amount", d.amount)?,
        ])?;
    }

    for (seq, a) in o.adjustments.iter().enumerate() {
        conn.prepare_cached(
            r"
            INSERT INTO player_adjustments (
                hand_id, player_id, adjustment_sequence, kind, street, amount_micros
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ",
        )?
        .execute(params![
            hand_db_id,
            player_id(&a.player_name)?,
            index_i64(seq),
            a.kind.as_str(),
            i64::from(a.street.code()),
            to_micros("adjustment_amount", a.amount)?,
        ])?;
    }

    Ok(InsertOutcome::Inserted)
}

// ---------------------------------------------------------------------------------------------
// Reading hands back
// ---------------------------------------------------------------------------------------------

fn corrupt(table: &'static str, detail: impl Into<String>) -> StorageError {
    StorageError::CorruptRow {
        table,
        detail: detail.into(),
    }
}

fn small_int(table: &'static str, value: i64) -> Result<u8, StorageError> {
    u8::try_from(value).map_err(|_| corrupt(table, format!("{value} is out of range")))
}

fn sequence_int(table: &'static str, value: i64) -> Result<u16, StorageError> {
    u16::try_from(value).map_err(|_| corrupt(table, format!("{value} is out of range")))
}

fn stored<T: FromStr>(table: &'static str, value: &str) -> Result<T, StorageError> {
    value
        .parse()
        .map_err(|_| corrupt(table, format!("unknown identifier '{value}'")))
}

fn stored_card(table: &'static str, value: Option<i64>) -> Result<Option<Card>, StorageError> {
    value
        .map(|v| {
            u8::try_from(v)
                .ok()
                .and_then(Card::from_raw)
                .ok_or_else(|| corrupt(table, format!("invalid card code {v}")))
        })
        .transpose()
}

fn stored_street(table: &'static str, value: i64) -> Result<Street, StorageError> {
    Street::from_code(small_int(table, value)?)
        .ok_or_else(|| corrupt(table, format!("invalid street code {value}")))
}

fn collect_rows<T>(
    conn: &Connection,
    sql: &str,
    hand_db_id: i64,
    mut map: impl FnMut(&Row<'_>) -> Result<T, StorageError>,
) -> Result<Vec<T>, StorageError> {
    let mut stmt = conn.prepare_cached(sql)?;
    let mut rows = stmt.query(params![hand_db_id])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map(row)?);
    }
    Ok(out)
}

/// Rebuilds the canonical hand stored under `hands.id = hand_db_id`.
fn load_hand_by_id(conn: &Connection, hand_db_id: i64) -> Result<CanonicalHand, StorageError> {
    type HeaderRow = (
        String,
        String,
        String,
        String,
        String,
        i64,
        i64,
        String,
        Option<i32>,
        String,
        i64,
        i64,
        i64,
        i64,
    );
    let row: HeaderRow = conn
        .prepare_cached(
            r"
            SELECT platform, site_hand_id, game_type, betting_limit, currency,
                   small_blind_micros, big_blind_micros, timestamp, utc_offset_seconds,
                   table_name, max_seats, button_seat, total_pot_micros, uncalled_returned_micros
            FROM hands WHERE id = ?1
            ",
        )?
        .query_row(params![hand_db_id], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
                r.get(11)?,
                r.get(12)?,
                r.get(13)?,
            ))
        })?;
    let header = HandHeader {
        platform: stored::<Platform>("hands", &row.0)?,
        site_hand_id: row.1,
        game_type: stored::<GameType>("hands", &row.2)?,
        limit: stored::<BettingLimit>("hands", &row.3)?,
        currency: stored::<CurrencyCode>("hands", &row.4)?,
        small_blind: micro_units_to_decimal(row.5),
        big_blind: micro_units_to_decimal(row.6),
        timestamp: NaiveDateTime::parse_from_str(&row.7, TIMESTAMP_FORMAT)
            .map_err(|_| corrupt("hands", format!("invalid timestamp '{}'", row.7)))?,
        utc_offset_seconds: row.8,
        table_name: row.9,
        max_seats: small_int("hands", row.10)?,
        button_seat: small_int("hands", row.11)?,
    };

    let players = collect_rows(
        conn,
        r"
        SELECT p.screen_name, hp.seat, hp.starting_stack_micros, hp.hole_card_1, hp.hole_card_2,
               hp.is_hero
        FROM hand_players hp JOIN players p ON p.id = hp.player_id
        WHERE hp.hand_id = ?1 ORDER BY hp.id
        ",
        hand_db_id,
        |r| {
            Ok(Player {
                name: r.get(0)?,
                seat: small_int("hand_players", r.get(1)?)?,
                starting_stack: micro_units_to_decimal(r.get(2)?),
                hole_cards: [
                    stored_card("hand_players", r.get(3)?)?,
                    stored_card("hand_players", r.get(4)?)?,
                ],
                is_hero: r.get::<_, i64>(5)? == 1,
            })
        },
    )?;

    let actions = collect_rows(
        conn,
        r"
        SELECT p.screen_name, a.sequence, a.street, a.action_type, a.amount_micros,
               a.street_total_after_micros, a.is_all_in
        FROM actions a JOIN players p ON p.id = a.player_id
        WHERE a.hand_id = ?1 ORDER BY a.sequence
        ",
        hand_db_id,
        |r| {
            Ok(StreetAction {
                player_name: r.get(0)?,
                sequence: sequence_int("actions", r.get(1)?)?,
                street: stored_street("actions", r.get(2)?)?,
                action_type: stored::<ActionType>("actions", &r.get::<_, String>(3)?)?,
                amount: micro_units_to_decimal(r.get(4)?),
                street_total_after: micro_units_to_decimal(r.get(5)?),
                is_all_in: r.get::<_, i64>(6)? == 1,
            })
        },
    )?;

    let boards = collect_rows(
        conn,
        r"
        SELECT run_index, flop_1, flop_2, flop_3, turn, river
        FROM board_cards WHERE hand_id = ?1 ORDER BY id
        ",
        hand_db_id,
        |r| {
            let flop = match (
                stored_card("board_cards", r.get(1)?)?,
                stored_card("board_cards", r.get(2)?)?,
                stored_card("board_cards", r.get(3)?)?,
            ) {
                (Some(a), Some(b), Some(c)) => Some([a, b, c]),
                (None, None, None) => None,
                _ => return Err(corrupt("board_cards", "partial flop")),
            };
            Ok(CommunityCards {
                run_index: small_int("board_cards", r.get(0)?)?,
                flop,
                turn: stored_card("board_cards", r.get(4)?)?,
                river: stored_card("board_cards", r.get(5)?)?,
            })
        },
    )?;

    let winners = collect_rows(
        conn,
        r"
        SELECT p.screen_name, w.amount_micros, w.run_index
        FROM pot_winners w JOIN players p ON p.id = w.player_id
        WHERE w.hand_id = ?1 ORDER BY w.pot_sequence
        ",
        hand_db_id,
        |r| {
            Ok(PotWinner {
                player_name: r.get(0)?,
                amount: micro_units_to_decimal(r.get(1)?),
                run_index: small_int("pot_winners", r.get(2)?)?,
            })
        },
    )?;

    let deductions = collect_rows(
        conn,
        r"
        SELECT kind, site_label, amount_micros
        FROM pot_deductions WHERE hand_id = ?1 ORDER BY deduction_sequence
        ",
        hand_db_id,
        |r| {
            Ok(PotDeduction {
                kind: stored::<DeductionKind>("pot_deductions", &r.get::<_, String>(0)?)?,
                label: r.get(1)?,
                amount: micro_units_to_decimal(r.get(2)?),
            })
        },
    )?;

    let adjustments = collect_rows(
        conn,
        r"
        SELECT p.screen_name, a.kind, a.street, a.amount_micros
        FROM player_adjustments a JOIN players p ON p.id = a.player_id
        WHERE a.hand_id = ?1 ORDER BY a.adjustment_sequence
        ",
        hand_db_id,
        |r| {
            Ok(PlayerAdjustment {
                player_name: r.get(0)?,
                kind: stored::<AdjustmentKind>("player_adjustments", &r.get::<_, String>(1)?)?,
                street: stored_street("player_adjustments", r.get(2)?)?,
                amount: micro_units_to_decimal(r.get(3)?),
            })
        },
    )?;

    Ok(CanonicalHand {
        header,
        players,
        actions,
        boards,
        outcome: HandOutcome {
            total_pot: micro_units_to_decimal(row.12),
            deductions,
            uncalled_returned: micro_units_to_decimal(row.13),
            winners,
            adjustments,
        },
    })
}

// ---------------------------------------------------------------------------------------------
// Repository
// ---------------------------------------------------------------------------------------------

/// What the file at hand is, judged before anything is written to it.
enum DatabaseKind {
    /// No tables and no identity yet: a new database.
    New,
    /// A `poker_tracker` database at the given schema version.
    PokerTracker(i64),
}

fn identify(conn: &Connection) -> Result<DatabaseKind, StorageError> {
    let app_id: i64 = conn.query_row("PRAGMA application_id;", [], |row| row.get(0))?;
    let version: i64 = conn.query_row("PRAGMA user_version;", [], |row| row.get(0))?;
    let table_exists = |name: &str| -> Result<bool, StorageError> {
        Ok(conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            params![name],
            |row| row.get(0),
        )?)
    };
    let has_schema_objects: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name NOT LIKE 'sqlite_%')",
        [],
        |row| row.get(0),
    )?;
    if app_id == APPLICATION_ID {
        return Ok(DatabaseKind::PokerTracker(version));
    }
    if app_id == 0 && version == 0 && !has_schema_objects {
        return Ok(DatabaseKind::New);
    }
    // v3.4 databases predate the application id: schema version 1 with its tables.
    if app_id == 0 && version == 1 && table_exists("hands")? && table_exists("import_runs")? {
        return Ok(DatabaseKind::PokerTracker(1));
    }
    let detail = if app_id == 0 {
        format!(
            "this file is not a poker_tracker v3.4+ database (schema version {version}); a database \
             from poker_tracker v3.3 or earlier must be deleted and its hands re-imported, and any \
             other SQLite file is left untouched"
        )
    } else {
        format!(
            "this SQLite file belongs to another application (application_id {app_id:#x}); it was \
             left untouched"
        )
    };
    Err(StorageError::NotAPokerTrackerDatabase { detail })
}

/// SQLite-backed repository managing schema initialization and batch ingestion.
pub struct SqliteRepository {
    conn: Connection,
}

impl SqliteRepository {
    /// Opens or creates a SQLite database at `path`, identifies it, applies pragmas, and runs
    /// any pending migrations.
    ///
    /// # Errors
    /// Returns [`StorageError`] if opening the file or a migration fails, if the file is not a
    /// `poker_tracker` database ([`StorageError::NotAPokerTrackerDatabase`], nothing is changed),
    /// or if it was created by a newer build.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let conn = Connection::open(path)?;
        Self::initialize(conn)
    }

    /// Opens an existing database without ever creating one (e.g. to undo an import).
    ///
    /// # Errors
    /// [`StorageError::DatabaseNotFound`] if no file exists at `path`; otherwise as
    /// [`Self::open`].
    pub fn open_existing<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let path = path.as_ref();
        if !path.is_file() {
            return Err(StorageError::DatabaseNotFound {
                path: path.display().to_string(),
            });
        }
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Self::initialize(conn)
    }

    /// Creates an in-memory SQLite repository suitable for testing.
    ///
    /// # Errors
    /// Returns [`StorageError`] if schema initialization fails.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        Self::initialize(conn)
    }

    fn initialize(mut conn: Connection) -> Result<Self, StorageError> {
        conn.busy_timeout(DEFAULT_BUSY_TIMEOUT)?;
        // Identify the file before changing anything about it.
        let current = match identify(&conn)? {
            DatabaseKind::New => 0,
            DatabaseKind::PokerTracker(version) => version,
        };
        if !(0..=SCHEMA_VERSION).contains(&current) {
            return Err(StorageError::UnsupportedSchemaVersion {
                found: current,
                expected: SCHEMA_VERSION,
            });
        }
        conn.execute_batch(PRAGMA_TUNING)?;
        // Apply pending migrations in order, each atomically with its version bump. The version
        // is re-read under the write lock, so a concurrent open never applies a step twice.
        loop {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let version: i64 = tx.query_row("PRAGMA user_version;", [], |row| row.get(0))?;
            let next = usize::try_from(version).unwrap_or(usize::MAX);
            let Some(sql) = MIGRATIONS.get(next) else {
                tx.commit()?;
                break;
            };
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "application_id", APPLICATION_ID)?;
            tx.pragma_update(None, "user_version", index_i64(next + 1))?;
            tx.commit()?;
        }
        Ok(Self { conn })
    }

    /// Changes how long a write waits for another connection's lock ([`DEFAULT_BUSY_TIMEOUT`]
    /// by default) before failing.
    ///
    /// # Errors
    /// Returns [`StorageError::Sqlite`] if the timeout cannot be set.
    pub fn set_busy_timeout(&self, timeout: Duration) -> Result<(), StorageError> {
        Ok(self.conn.busy_timeout(timeout)?)
    }

    /// Schema version of the open database (`PRAGMA user_version`).
    ///
    /// # Errors
    /// Returns [`StorageError::Sqlite`] if the pragma cannot be read.
    pub fn schema_version(&self) -> Result<i64, StorageError> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version;", [], |row| row.get(0))?)
    }

    /// Records an import run and the time it started. Idempotent; batches and quarantine writes
    /// that reference an unregistered run register it with the current time instead.
    ///
    /// # Errors
    /// Returns [`StorageError::Sqlite`] if the write fails.
    pub fn register_import_run(
        &mut self,
        run_id: Uuid,
        started_at: DateTime<Utc>,
    ) -> Result<(), StorageError> {
        let started = started_at.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        register_run(&tx, run_id, &started)?;
        tx.commit()?;
        Ok(())
    }

    /// Undoes an import run: deletes the hands this run stored that no other import run
    /// contained (with all their rows), keeps every other hand the run contained (moving the
    /// provenance of hands it stored to the earliest remaining run), and removes the run's
    /// quarantine entries and the run record. Hands stored without a source are never deleted.
    ///
    /// # Errors
    /// [`StorageError::UnknownImportRun`] if no such run exists, or [`StorageError::Sqlite`] if
    /// the deletion fails; nothing is deleted in either case.
    pub fn delete_import_run(&mut self, run_id: Uuid) -> Result<DeletedImportRun, StorageError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = run_id.to_string();
        let known: Option<i64> = tx
            .query_row(
                "SELECT 1 FROM import_runs WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        if known.is_none() {
            return Err(StorageError::UnknownImportRun { run_id });
        }
        let hands_deleted = tx.execute(
            r"
            DELETE FROM hands
            WHERE import_run_id = ?1
              AND NOT EXISTS (
                  SELECT 1 FROM hand_sightings o
                  WHERE o.hand_id = hands.id AND o.import_run_id != ?1)
            ",
            params![id],
        )?;
        let hands_kept: i64 = tx.query_row(
            "SELECT COUNT(*) FROM hand_sightings WHERE import_run_id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        tx.execute(
            r"
            UPDATE hands SET (import_run_id, source_file, source_line) = (
                SELECT s.import_run_id, s.source_file, s.source_line
                FROM hand_sightings s JOIN import_runs r ON r.id = s.import_run_id
                WHERE s.hand_id = hands.id AND s.import_run_id != ?1
                ORDER BY r.started_at, s.import_run_id LIMIT 1)
            WHERE import_run_id = ?1
            ",
            params![id],
        )?;
        let quarantined_deleted = tx.execute(
            "DELETE FROM quarantined_hands WHERE import_run_id = ?1",
            params![id],
        )?;
        tx.execute("DELETE FROM import_runs WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(DeletedImportRun {
            hands_deleted,
            hands_kept: usize::try_from(hands_kept).unwrap_or(0),
            quarantined_deleted,
        })
    }

    /// Reads a stored hand back into the canonical model, or `None` if it is not stored.
    ///
    /// # Errors
    /// [`StorageError::Sqlite`] on a query failure, or [`StorageError::CorruptRow`] if a stored
    /// value cannot be represented in the model.
    pub fn load_hand(
        &self,
        platform: Platform,
        site_hand_id: &str,
    ) -> Result<Option<CanonicalHand>, StorageError> {
        let id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM hands WHERE platform = ?1 AND site_hand_id = ?2",
                params![platform.as_str(), site_hand_id],
                |row| row.get(0),
            )
            .optional()?;
        id.map(|id| load_hand_by_id(&self.conn, id)).transpose()
    }

    /// Borrows the underlying [`rusqlite::Connection`].
    #[must_use]
    pub const fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Validates and inserts hands without source information (see
    /// [`Self::insert_sourced_hands_batch`] for the semantics). Without raw text, a hand already
    /// stored under the same id is compared by its canonical content.
    ///
    /// # Errors
    /// As [`Self::insert_sourced_hands_batch`].
    pub fn insert_hands_batch(
        &mut self,
        hands: &[CanonicalHand],
    ) -> Result<BatchInsertStats, StorageError> {
        self.insert_batch(hands, None)
    }

    /// Validates and inserts a slice of hands (from any site, in any currency) in one SQLite
    /// transaction, recording for each hand its raw text and the import run, file and line it
    /// came from. `sources[i]` describes `hands[i]`.
    ///
    /// Each hand is written inside its own savepoint: a hand that fails validation, conversion
    /// or a constraint is rolled back individually and reported in
    /// [`BatchInsertStats::rejected`], while the rest of the batch is committed. A hand whose
    /// `(platform, site_hand_id)` already exists is a duplicate only if it is the same hand:
    /// same currency (else [`StorageError::CurrencyConflict`]) and same content (else
    /// [`StorageError::ContentConflict`]), compared by the SHA-256 of the normalized raw text
    /// when both imports carry it, otherwise by the canonical hand. Every import run that
    /// contains a hand is recorded for it; a quarantined text that gets stored leaves the
    /// `quarantine_pending` view.
    ///
    /// # Errors
    /// Returns [`StorageError`] for failures that affect the whole transaction (taking the write
    /// lock within the busy timeout, savepoint management, I/O, commit), in which case nothing
    /// from this batch is stored, or [`StorageError::SourceCountMismatch`] if the slices differ
    /// in length.
    pub fn insert_sourced_hands_batch(
        &mut self,
        hands: &[CanonicalHand],
        sources: &[HandSource],
    ) -> Result<BatchInsertStats, StorageError> {
        if hands.len() != sources.len() {
            return Err(StorageError::SourceCountMismatch {
                hands: hands.len(),
                sources: sources.len(),
            });
        }
        self.insert_batch(hands, Some(sources))
    }

    fn insert_batch(
        &mut self,
        hands: &[CanonicalHand],
        sources: Option<&[HandSource]>,
    ) -> Result<BatchInsertStats, StorageError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut stats = BatchInsertStats::default();

        // Register every import run referenced by this batch.
        let mut runs: Vec<Uuid> = sources
            .unwrap_or(&[])
            .iter()
            .map(|src| src.import_run_id)
            .collect();
        runs.sort_unstable();
        runs.dedup();
        let now = now_utc();
        for run in runs {
            register_run(&tx, run, &now)?;
        }

        for (batch_index, hand) in hands.iter().enumerate() {
            let reject = |error: StorageError| RejectedHand {
                batch_index,
                site_hand_id: hand.header.site_hand_id.clone(),
                error,
            };
            if let Err(e) = validate_hand(hand) {
                stats.rejected.push(reject(StorageError::InvalidHand(e)));
                continue;
            }
            tx.execute_batch("SAVEPOINT hand_insert")?;
            let source = sources.and_then(|all| all.get(batch_index));
            match insert_one(&tx, hand, source) {
                Ok(outcome) => {
                    tx.execute_batch("RELEASE hand_insert")?;
                    match outcome {
                        InsertOutcome::Inserted => stats.hands_inserted += 1,
                        InsertOutcome::Duplicate => stats.duplicates_skipped += 1,
                    }
                }
                Err(e) if e.is_hand_specific() => {
                    tx.execute_batch("ROLLBACK TO hand_insert; RELEASE hand_insert")?;
                    stats.rejected.push(reject(e));
                }
                // Lock, I/O and other database-wide errors: the whole batch is rolled back.
                Err(e) => return Err(e),
            }
        }

        tx.commit()?;
        Ok(stats)
    }

    /// Stores rejected hands with their raw text, once per import run and text. Returns the
    /// number of new quarantine entries. Entries stay until their run is deleted; the
    /// `quarantine_pending` view lists those whose text is not stored as a hand.
    ///
    /// # Errors
    /// Returns [`StorageError::Sqlite`] if the transaction fails; nothing is stored then.
    pub fn quarantine_hands(&mut self, hands: &[QuarantinedHand]) -> Result<usize, StorageError> {
        if hands.is_empty() {
            return Ok(0);
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = now_utc();
        let mut added = 0;
        for q in hands {
            register_run(&tx, q.import_run_id, &now)?;
            added += tx
                .prepare_cached(
                    r"
                    INSERT INTO quarantined_hands (
                        import_run_id, raw_sha256, raw_text, parser, site_hand_id, stage, error,
                        source_file, source_line
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                    ON CONFLICT(import_run_id, raw_sha256) DO NOTHING
                    ",
                )?
                .execute(params![
                    q.import_run_id.to_string(),
                    hand_text_sha256(&q.raw_text),
                    q.raw_text,
                    q.parser,
                    q.site_hand_id,
                    q.stage,
                    q.error,
                    q.file_path,
                    index_i64(q.start_line.max(1)),
                ])?;
        }
        tx.commit()?;
        Ok(added)
    }
}
```

---

## FILE: tests/storage.rs · Milestone 4

```rust
//! SQLite repository tests: idempotency, per-hand savepoints, reconciliation queries,
//! multi-currency storage, and schema versioning.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use pretty_assertions::assert_eq;
use rust_decimal_macros::dec;
use tempfile::NamedTempFile;

use common::{
    EV_CASHOUT, ONE_CARD_SHOW_AFTER_FOLD, REAL_SESSION, RIT_THREE_TIMES_PREFLOP, WALK, gg,
};
use poker_tracker::domain::{CanonicalHand, CurrencyCode, Platform};
use poker_tracker::parser::{GgPokerParser, HandParser};
use poker_tracker::storage::{
    APPLICATION_ID, DeletedImportRun, HandSource, MIGRATIONS, QuarantinedHand, SCHEMA_VERSION,
    SqliteRepository, StorageError, hand_text_sha256,
};
use uuid::Uuid;

fn parse(raw: &str) -> CanonicalHand {
    gg().parse_hand(raw).unwrap()
}

fn real_hands() -> Vec<CanonicalHand> {
    gg().parse_session(REAL_SESSION)
        .map(|(_, r)| r.unwrap())
        .collect()
}

fn scalar(repo: &SqliteRepository, sql: &str) -> i64 {
    repo.connection()
        .query_row(sql, [], |row| row.get(0))
        .unwrap()
}

fn rows<T: rusqlite::types::FromSql, U: rusqlite::types::FromSql>(
    repo: &SqliteRepository,
    sql: &str,
) -> Vec<(T, U)> {
    let mut stmt = repo.connection().prepare(sql).unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn real_session_inserts_once_and_reconciles_in_sql() {
    let hands = real_hands();
    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();

    let first = repo.insert_hands_batch(&hands).unwrap();
    assert_eq!((first.hands_inserted, first.duplicates_skipped), (294, 0));
    assert!(first.rejected.is_empty());
    let second = repo.insert_hands_batch(&hands).unwrap();
    assert_eq!((second.hands_inserted, second.duplicates_skipped), (0, 294));

    // Hero's net for the session, per currency: +2.86 CAD.
    let hero: Vec<(String, i64)> = rows(
        &repo,
        "SELECT h.currency, SUM(hp.net_micros) FROM hand_players hp
         JOIN hands h ON h.id = hp.hand_id WHERE hp.is_hero = 1 GROUP BY h.currency",
    );
    assert_eq!(hero, vec![("CAD".to_string(), 2_860_000)]);

    // Money conservation: all nets == adjustments - deductions.
    let nets = scalar(&repo, "SELECT SUM(net_micros) FROM hand_players");
    let deductions = scalar(&repo, "SELECT SUM(amount_micros) FROM pot_deductions");
    let adjustments = scalar(&repo, "SELECT SUM(amount_micros) FROM player_adjustments");
    assert_eq!(nets, adjustments - deductions);
    // Aggregates on `hands` agree with the detail table.
    assert_eq!(
        scalar(
            &repo,
            "SELECT SUM(rake_micros + jackpot_micros + other_deductions_micros) FROM hands"
        ),
        deductions
    );
    // Chip conservation per hand, checked independently in SQL.
    assert_eq!(
        scalar(
            &repo,
            "SELECT COUNT(*) FROM hands h WHERE h.total_pot_micros != (
                SELECT COALESCE(SUM(CASE
                    WHEN a.action_type = 'UncalledBetReturned' THEN -a.amount_micros
                    WHEN a.action_type IN ('Fold', 'Check') THEN 0
                    ELSE a.amount_micros END), 0)
                FROM actions a WHERE a.hand_id = h.id)"
        ),
        0
    );
}

#[test]
fn hands_in_different_currencies_are_stored_and_reported_separately() {
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let cad = GgPokerParser::new(CurrencyCode::CAD)
        .parse_hand(EV_CASHOUT)
        .unwrap();
    let usd = GgPokerParser::new(CurrencyCode::USD)
        .parse_hand(RIT_THREE_TIMES_PREFLOP)
        .unwrap();
    repo.insert_hands_batch(&[cad, usd]).unwrap();

    let rake_by_currency: Vec<(String, i64)> = rows(
        &repo,
        "SELECT currency, SUM(rake_micros) FROM hands GROUP BY currency ORDER BY currency",
    );
    assert_eq!(
        rake_by_currency,
        vec![("CAD".to_string(), 140_000), ("USD".to_string(), 960_000)]
    );
}

#[test]
fn multi_run_winners_partial_cards_deductions_and_adjustments_are_persisted() {
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let hands = vec![
        parse(RIT_THREE_TIMES_PREFLOP),
        parse(ONE_CARD_SHOW_AFTER_FOLD),
        parse(EV_CASHOUT),
    ];
    repo.insert_hands_batch(&hands).unwrap();

    let runs: Vec<(i64, i64)> = rows(
        &repo,
        "SELECT w.run_index, w.amount_micros FROM pot_winners w
         JOIN hands h ON h.id = w.hand_id WHERE h.site_hand_id = 'HD140645134'
         ORDER BY w.pot_sequence",
    );
    assert_eq!(runs, vec![(1, 6_230_000), (2, 6_230_000), (3, 6_230_000)]);
    assert_eq!(
        scalar(
            &repo,
            "SELECT run_count FROM hands WHERE site_hand_id = 'HD140645134'"
        ),
        3
    );

    let labels: Vec<(String, String)> = rows(
        &repo,
        "SELECT d.kind, d.site_label FROM pot_deductions d JOIN hands h ON h.id = d.hand_id
         WHERE h.site_hand_id = 'HD140645134' ORDER BY d.deduction_sequence",
    );
    assert_eq!(
        labels,
        [
            ("Rake", "Rake"),
            ("Jackpot", "Jackpot"),
            ("Other", "Bingo"),
            ("Other", "Fortune"),
            ("Other", "Tax")
        ]
        .map(|(k, l)| (k.to_string(), l.to_string()))
        .to_vec()
    );

    assert_eq!(
        scalar(
            &repo,
            "SELECT COUNT(*) FROM hand_players WHERE hole_card_1 IS NOT NULL AND hole_card_2 IS NULL"
        ),
        1
    );
    let adjustment: Vec<(String, i64)> =
        rows(&repo, "SELECT kind, amount_micros FROM player_adjustments");
    assert_eq!(
        adjustment,
        vec![("EvCashoutRiskPaid".to_string(), -480_000)]
    );
    assert_eq!(
        scalar(
            &repo,
            "SELECT hp.net_micros FROM hand_players hp JOIN players p ON p.id = hp.player_id
             JOIN hands h ON h.id = hp.hand_id
             WHERE h.site_hand_id = 'HD140642980' AND p.screen_name = '1020a9fa'"
        ),
        830_000
    );
}

#[test]
fn one_bad_hand_is_rejected_without_losing_the_rest_of_the_batch() {
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let good_a = parse(WALK);
    let good_b = parse(EV_CASHOUT);
    let mut bad = parse(RIT_THREE_TIMES_PREFLOP);
    bad.outcome.total_pot = dec!(999.99);

    let stats = repo.insert_hands_batch(&[good_a, bad, good_b]).unwrap();
    assert_eq!(stats.hands_inserted, 2);
    assert_eq!(stats.rejected.len(), 1);
    assert_eq!(stats.rejected[0].batch_index, 1);
    assert_eq!(stats.rejected[0].site_hand_id, "HD140645134");
    assert!(matches!(
        stats.rejected[0].error,
        StorageError::InvalidHand(_)
    ));
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM hands"), 2);
    assert_eq!(
        scalar(
            &repo,
            "SELECT COUNT(*) FROM actions WHERE hand_id NOT IN (SELECT id FROM hands)"
        ),
        0
    );
}

#[test]
fn migrations_bring_new_databases_up_to_date_and_refuse_newer_ones() {
    let temp_db = NamedTempFile::new().unwrap();
    {
        let repo = SqliteRepository::open(temp_db.path()).unwrap();
        assert_eq!(repo.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(usize::try_from(SCHEMA_VERSION).unwrap(), MIGRATIONS.len());
    }
    // Re-opening an up-to-date database applies nothing and keeps the data.
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    repo.insert_hands_batch(&[parse(WALK)]).unwrap();
    drop(repo);
    let repo = SqliteRepository::open(temp_db.path()).unwrap();
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM hands"), 1);
    drop(repo);

    // A database written by a newer build is refused rather than misread.
    {
        let conn = rusqlite::Connection::open(temp_db.path()).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
    }
    assert!(matches!(
        SqliteRepository::open(temp_db.path()),
        Err(StorageError::UnsupportedSchemaVersion { found, expected })
            if found == SCHEMA_VERSION + 1 && expected == SCHEMA_VERSION
    ));
}

#[test]
fn reimporting_in_a_different_currency_is_rejected_not_skipped() {
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let as_usd = GgPokerParser::new(CurrencyCode::USD)
        .parse_hand(WALK)
        .unwrap();
    repo.insert_hands_batch(&[as_usd]).unwrap();

    let as_cad = parse(WALK);
    let stats = repo.insert_hands_batch(&[as_cad]).unwrap();
    assert_eq!((stats.hands_inserted, stats.duplicates_skipped), (0, 0));
    assert_eq!(stats.rejected.len(), 1);
    assert!(matches!(
        &stats.rejected[0].error,
        StorageError::CurrencyConflict { stored, incoming, .. } if stored == "USD" && incoming == "CAD"
    ));

    // The same hand in the same currency is still an ordinary duplicate.
    let again = GgPokerParser::new(CurrencyCode::USD)
        .parse_hand(WALK)
        .unwrap();
    let stats = repo.insert_hands_batch(&[again]).unwrap();
    assert_eq!(stats.duplicates_skipped, 1);
    assert!(stats.rejected.is_empty());
}

#[test]
fn sources_are_recorded_and_an_import_run_can_be_deleted() {
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let run = Uuid::now_v7();
    let other_run = Uuid::now_v7();
    let hands = vec![parse(WALK), parse(EV_CASHOUT)];
    let sources = vec![
        HandSource {
            import_run_id: run,
            file_path: "a.txt".into(),
            start_line: 1,
            raw_text: WALK.into(),
        },
        HandSource {
            import_run_id: other_run,
            file_path: "b.txt".into(),
            start_line: 42,
            raw_text: EV_CASHOUT.into(),
        },
    ];
    let stats = repo.insert_sourced_hands_batch(&hands, &sources).unwrap();
    assert_eq!(stats.hands_inserted, 2);

    let recorded: Vec<(String, i64)> = rows(
        &repo,
        "SELECT source_file, source_line FROM hands ORDER BY source_line",
    );
    assert_eq!(
        recorded,
        vec![("a.txt".to_string(), 1), ("b.txt".to_string(), 42)]
    );
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM import_runs"), 2);

    // Undo one run: its hands and all their child rows go; the other run is untouched.
    assert_eq!(repo.delete_import_run(other_run).unwrap().hands_deleted, 1);
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM hands"), 1);
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM player_adjustments"), 0);
    assert_eq!(
        scalar(
            &repo,
            "SELECT COUNT(*) FROM actions WHERE hand_id NOT IN (SELECT id FROM hands)"
        ),
        0
    );

    assert!(matches!(
        repo.insert_sourced_hands_batch(&hands, &sources[..1]),
        Err(StorageError::SourceCountMismatch { .. })
    ));
}

// ---------------------------------------------------------------------------------------------
// v3.5 review fixes
// ---------------------------------------------------------------------------------------------

fn source(run: Uuid, file: &str, line: usize, raw: &str) -> HandSource {
    HandSource {
        import_run_id: run,
        file_path: file.into(),
        start_line: line,
        raw_text: raw.into(),
    }
}

#[test]
fn every_real_hand_round_trips_through_sqlite_in_true_micro_units() {
    let hands = real_hands();
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    assert_eq!(repo.insert_hands_batch(&hands).unwrap().hands_inserted, 294);
    for h in &hands {
        let stored = repo
            .load_hand(Platform::GGPoker, &h.header.site_hand_id)
            .unwrap()
            .unwrap();
        assert_eq!(&stored, h);
    }
    assert!(repo.load_hand(Platform::GGPoker, "HD0").unwrap().is_none());
    // One micro-unit is one millionth: the big blind of 0.10 is stored as 100,000.
    assert_eq!(
        scalar(&repo, "SELECT MAX(big_blind_micros) FROM hands"),
        100_000
    );
}

#[test]
fn different_content_under_the_same_hand_id_is_a_conflict_not_a_duplicate() {
    let changed = WALK.replace("Dealt to Hero [Ts 6d]", "Dealt to Hero [Ts 7d]");
    let run = Uuid::now_v7();

    // With raw text (every pipeline import): compared by the normalised text's SHA-256.
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    repo.insert_sourced_hands_batch(&[parse(WALK)], &[source(run, "a.txt", 1, WALK)])
        .unwrap();
    let stats = repo
        .insert_sourced_hands_batch(&[parse(&changed)], &[source(run, "b.txt", 1, &changed)])
        .unwrap();
    assert_eq!((stats.hands_inserted, stats.duplicates_skipped), (0, 0));
    assert!(matches!(
        &stats.rejected[0].error,
        StorageError::ContentConflict { site_hand_id } if site_hand_id == "HD140646565"
    ));
    // CRLF line endings and trailing spaces do not make a different hand.
    let reformatted = WALK.replace('\n', "  \r\n");
    assert_eq!(hand_text_sha256(&reformatted), hand_text_sha256(WALK));
    let stats = repo
        .insert_sourced_hands_batch(
            &[parse(&reformatted)],
            &[source(run, "c.txt", 1, &reformatted)],
        )
        .unwrap();
    assert_eq!(stats.duplicates_skipped, 1);
    assert_eq!(
        scalar(
            &repo,
            "SELECT COUNT(*) FROM hands WHERE raw_text IS NOT NULL"
        ),
        1
    );

    // Without raw text: compared by the canonical hand read back from the database.
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    repo.insert_hands_batch(&[parse(WALK)]).unwrap();
    let mut restacked = parse(WALK);
    restacked.players[0].starting_stack += rust_decimal::Decimal::ONE;
    for different in [parse(&changed), restacked] {
        let stats = repo.insert_hands_batch(&[different]).unwrap();
        assert!(matches!(
            stats.rejected[0].error,
            StorageError::ContentConflict { .. }
        ));
    }
    assert_eq!(
        repo.insert_hands_batch(&[parse(WALK)])
            .unwrap()
            .duplicates_skipped,
        1
    );
}

#[test]
fn deleting_a_run_keeps_hands_that_another_run_also_contained() {
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let (first, second) = (Uuid::now_v7(), Uuid::now_v7());
    repo.insert_sourced_hands_batch(
        &[parse(WALK), parse(EV_CASHOUT)],
        &[
            source(first, "a/s.txt", 1, WALK),
            source(first, "a/s.txt", 30, EV_CASHOUT),
        ],
    )
    .unwrap();
    // The second run re-imports WALK only: a duplicate, but it is recorded as a sighting.
    let stats = repo
        .insert_sourced_hands_batch(&[parse(WALK)], &[source(second, "b/s.txt", 7, WALK)])
        .unwrap();
    assert_eq!(stats.duplicates_skipped, 1);
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM hand_sightings"), 3);

    assert_eq!(
        repo.delete_import_run(first).unwrap(),
        DeletedImportRun {
            hands_deleted: 1,
            hands_kept: 1,
            quarantined_deleted: 0
        }
    );
    let kept: Vec<(String, i64)> = rows(
        &repo,
        "SELECT source_file, source_line FROM hands WHERE site_hand_id = 'HD140646565'",
    );
    assert_eq!(kept, vec![("b/s.txt".to_string(), 7)]);
    assert_eq!(
        scalar(
            &repo,
            &format!("SELECT COUNT(*) FROM hands WHERE import_run_id = '{second}'")
        ),
        1
    );

    // A mistyped run id is an error, not a silent "0 deleted".
    assert!(matches!(
        repo.delete_import_run(first),
        Err(StorageError::UnknownImportRun { run_id }) if run_id == first
    ));
    assert_eq!(repo.delete_import_run(second).unwrap().hands_deleted, 1);
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM hands"), 0);

    // A hand stored without a source existed before any run that later contained it.
    repo.insert_hands_batch(&[parse(WALK)]).unwrap();
    let third = Uuid::now_v7();
    repo.insert_sourced_hands_batch(&[parse(WALK)], &[source(third, "c/s.txt", 1, WALK)])
        .unwrap();
    let undone = repo.delete_import_run(third).unwrap();
    assert_eq!((undone.hands_deleted, undone.hands_kept), (0, 1));
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM hands"), 1);
}

#[test]
fn quarantined_hands_keep_their_raw_text_until_an_import_stores_them() {
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let run = Uuid::now_v7();
    let entry = QuarantinedHand {
        import_run_id: run,
        parser: "GGPoker (CAD)".into(),
        file_path: "s.txt".into(),
        start_line: 1,
        site_hand_id: Some("HD140646565".into()),
        stage: "parse".into(),
        error: "unrecognized line".into(),
        raw_text: WALK.into(),
    };
    assert_eq!(
        repo.quarantine_hands(&[entry.clone(), entry.clone()])
            .unwrap(),
        1
    );
    let stored: Vec<(String, String)> =
        rows(&repo, "SELECT stage, raw_text FROM quarantined_hands");
    assert_eq!(stored, vec![("parse".to_string(), WALK.to_string())]);

    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM quarantine_pending"), 1);

    // A later import that stores the same text takes it off the pending list; undoing that
    // import puts it back, because the quarantine entry itself is kept.
    let later = Uuid::now_v7();
    repo.insert_sourced_hands_batch(&[parse(WALK)], &[source(later, "s.txt", 1, WALK)])
        .unwrap();
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM quarantine_pending"), 0);
    repo.delete_import_run(later).unwrap();
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM quarantine_pending"), 1);

    // Undoing a run removes its quarantine entries.
    repo.quarantine_hands(&[QuarantinedHand {
        raw_text: EV_CASHOUT.into(),
        ..entry
    }])
    .unwrap();
    assert_eq!(repo.delete_import_run(run).unwrap().quarantined_deleted, 2);
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM quarantined_hands"), 0);
}

/// Builds a v3.4 database by hand: migration 1 only, one hand, amounts in units of 0.0001.
fn write_v3_4_database(path: &std::path::Path) {
    let run = Uuid::now_v7().to_string();
    {
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(MIGRATIONS[0]).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute(
            "INSERT INTO import_runs (id, started_at, tool_version) VALUES (?1, '2026-08-21T20:00:00Z', '0.4.0')",
            [&run],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO hands (platform, site_hand_id, game_type, betting_limit, currency,
                small_blind_micros, big_blind_micros, timestamp, table_name, max_seats,
                button_seat, run_count, total_pot_micros, rake_micros, jackpot_micros,
                other_deductions_micros, uncalled_returned_micros, import_run_id, source_file,
                source_line)
             VALUES ('GGPoker', 'HD1', 'Cash', 'NL', 'CAD', 500, 1000, '2026-08-21T17:00:00',
                'T', 6, 1, 0, 1000, 0, 0, 0, 500, ?1, 's.txt', 1)",
            [&run],
        )
        .unwrap();
    }
}

#[test]
fn a_v3_4_database_is_upgraded_in_place() {
    let temp_db = NamedTempFile::new().unwrap();
    write_v3_4_database(temp_db.path());
    let repo = SqliteRepository::open(temp_db.path()).unwrap();
    assert_eq!(repo.schema_version().unwrap(), SCHEMA_VERSION);
    assert_eq!(scalar(&repo, "PRAGMA application_id"), APPLICATION_ID);
    let blinds: Vec<(i64, i64)> = rows(
        &repo,
        "SELECT small_blind_micros, big_blind_micros FROM hands",
    );
    assert_eq!(blinds, vec![(50_000, 100_000)]);
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM hand_sightings"), 1);
}

#[test]
fn concurrent_opens_apply_each_migration_once() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("v34.sqlite");
    write_v3_4_database(&path);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let opens: Vec<_> = (0..4)
        .map(|_| {
            let (path, barrier) = (path.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                SqliteRepository::open(&path).map(|_| ())
            })
        })
        .collect();
    for open in opens {
        open.join().unwrap().unwrap();
    }
    let repo = SqliteRepository::open(&path).unwrap();
    let blinds: Vec<(i64, i64)> = rows(
        &repo,
        "SELECT small_blind_micros, big_blind_micros FROM hands",
    );
    assert_eq!(
        blinds,
        vec![(50_000, 100_000)],
        "the rescale ran exactly once"
    );
}

#[test]
fn files_that_are_not_poker_tracker_databases_are_left_untouched() {
    // An unrelated SQLite file.
    let other = NamedTempFile::new().unwrap();
    {
        let conn = rusqlite::Connection::open(other.path()).unwrap();
        conn.execute_batch("CREATE TABLE notes (body TEXT);")
            .unwrap();
    }
    // A poker_tracker v3.3 database (schema version 3, no application id).
    let old = NamedTempFile::new().unwrap();
    {
        let conn = rusqlite::Connection::open(old.path()).unwrap();
        conn.execute_batch("CREATE TABLE hands (id INTEGER PRIMARY KEY);")
            .unwrap();
        conn.pragma_update(None, "user_version", 3).unwrap();
    }
    for (file, hint) in [(&other, "left untouched"), (&old, "v3.3 or earlier")] {
        let err = SqliteRepository::open(file.path()).err().unwrap();
        assert!(
            matches!(&err, StorageError::NotAPokerTrackerDatabase { detail } if detail.contains(hint)),
            "{err}"
        );
        let conn = rusqlite::Connection::open(file.path()).unwrap();
        let journal: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            journal, "delete",
            "the journal mode must not be switched to WAL"
        );
        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 1, "no poker_tracker tables may be added");
    }
    // Opening an existing database never creates one.
    let missing = other.path().with_extension("missing.sqlite");
    assert!(matches!(
        SqliteRepository::open_existing(&missing),
        Err(StorageError::DatabaseNotFound { .. })
    ));
    assert!(!missing.exists());
}

#[test]
fn a_locked_database_fails_the_batch_instead_of_rejecting_its_hands() {
    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    repo.set_busy_timeout(std::time::Duration::from_millis(50))
        .unwrap();
    let other = rusqlite::Connection::open(temp_db.path()).unwrap();
    other.execute_batch("BEGIN IMMEDIATE;").unwrap();

    let err = repo.insert_hands_batch(&[parse(WALK), parse(EV_CASHOUT)]);
    assert!(
        matches!(&err, Err(StorageError::Sqlite(e)) if e.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseBusy)),
        "{err:?}"
    );
    other.execute_batch("ROLLBACK;").unwrap();
    let stats = repo
        .insert_hands_batch(&[parse(WALK), parse(EV_CASHOUT)])
        .unwrap();
    assert_eq!(stats.hands_inserted, 2);
}
```

---

## FILE: src/pipeline/mod.rs · Milestone 5

```rust
//! Resilient directory ingestion pipeline, diagnostic reporting and cross-hand checks.

pub mod continuity;
pub mod ingester;

pub use continuity::{PlayerStacks, StackSnapshot, StackWarning, check_stack_continuity};
pub use ingester::{
    DEFAULT_BATCH_SIZE, DirectoryIngester, FailureStage, HandFailure, IngestionReport,
    PipelineError,
};
```

---

## FILE: src/pipeline/continuity.rs · Milestone 5

```rust
//! Stack continuity between consecutive hands at a table, a site-neutral cross-check that no
//! single hand can provide: a player's stack can grow between hands (a top-up or rebuy) but
//! never shrink. A shrinking stack means a misparsed or corrupted hand on one side of the gap.

use std::fmt;
use std::path::PathBuf;

use chrono::NaiveDateTime;
use rust_decimal::Decimal;

use crate::domain::canonical::{CanonicalHand, Platform};

/// Starting and ending stacks of one parsed hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackSnapshot {
    pub platform: Platform,
    pub table_name: String,
    pub timestamp: NaiveDateTime,
    pub hand_id: String,
    /// 1-based line in the source file where the hand starts.
    pub block_start_line: usize,
    pub players: Vec<PlayerStacks>,
}

/// One player's stack at the start and the end of a hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerStacks {
    pub name: String,
    pub is_hero: bool,
    pub starting: Decimal,
    /// `starting + net` (net includes off-pot adjustments).
    pub ending: Decimal,
}

impl StackSnapshot {
    /// The stacks of `hand`, or `None` if a result overflows (validated hands never do).
    #[must_use]
    pub fn of(hand: &CanonicalHand, block_start_line: usize) -> Option<Self> {
        let results = hand.checked_player_results()?;
        let players = hand
            .players
            .iter()
            .zip(results)
            .map(|(p, r)| {
                Some(PlayerStacks {
                    name: p.name.clone(),
                    is_hero: p.is_hero,
                    starting: p.starting_stack,
                    ending: p.starting_stack.checked_add(r.net)?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            platform: hand.header.platform,
            table_name: hand.header.table_name.clone(),
            timestamp: hand.header.timestamp,
            hand_id: hand.header.site_hand_id.clone(),
            block_start_line,
            players,
        })
    }
}

/// A player who starts a hand with less than they ended the previous hand at the same table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackWarning {
    pub file_path: PathBuf,
    pub table_name: String,
    pub player_name: String,
    pub is_hero: bool,
    pub previous_hand_id: String,
    pub hand_id: String,
    /// 1-based line in `file_path` where `hand_id` starts.
    pub block_start_line: usize,
    /// The player's stack at the end of `previous_hand_id`.
    pub previous_ending: Decimal,
    /// The player's stack at the start of `hand_id`.
    pub starting: Decimal,
}

impl fmt::Display for StackWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{} hand {}: '{}'{} starts with {} at table '{}' but ended hand {} with {}; one of \
             the two hands is probably misrecorded",
            self.file_path.display(),
            self.block_start_line,
            self.hand_id,
            self.player_name,
            if self.is_hero { " (hero)" } else { "" },
            self.starting,
            self.table_name,
            self.previous_hand_id,
            self.previous_ending,
        )
    }
}

/// Checks every pair of consecutive hands (by timestamp) at the same table in one file: each
/// player seated in both must start the later hand with at least the stack they ended the
/// earlier one with.
///
/// A pair is skipped when its order is unknown (identical timestamps) or when a block that did
/// not parse lies between the two hands in the file (`unparsed_lines` are those blocks' start
/// lines), because the missing hand may explain the change.
#[must_use]
pub fn check_stack_continuity(
    file_path: &std::path::Path,
    snapshots: &mut [StackSnapshot],
    unparsed_lines: &[usize],
) -> Vec<StackWarning> {
    snapshots.sort_by(|a, b| {
        (a.platform.as_str(), a.table_name.as_str(), a.timestamp).cmp(&(
            b.platform.as_str(),
            b.table_name.as_str(),
            b.timestamp,
        ))
    });
    let mut warnings = Vec::new();
    for pair in snapshots.windows(2) {
        let [prev, next] = pair else { continue };
        let (low, high) = if prev.block_start_line < next.block_start_line {
            (prev.block_start_line, next.block_start_line)
        } else {
            (next.block_start_line, prev.block_start_line)
        };
        if prev.platform != next.platform
            || prev.table_name != next.table_name
            || prev.timestamp == next.timestamp
            || unparsed_lines.iter().any(|l| (low..high).contains(l))
        {
            continue;
        }
        for p in &next.players {
            let Some(before) = prev.players.iter().find(|q| q.name == p.name) else {
                continue;
            };
            if p.starting < before.ending {
                warnings.push(StackWarning {
                    file_path: file_path.to_path_buf(),
                    table_name: next.table_name.clone(),
                    player_name: p.name.clone(),
                    is_hero: p.is_hero,
                    previous_hand_id: prev.hand_id.clone(),
                    hand_id: next.hand_id.clone(),
                    block_start_line: next.block_start_line,
                    previous_ending: before.ending,
                    starting: p.starting,
                });
            }
        }
    }
    warnings
}
```

---

## FILE: src/pipeline/ingester.rs · Milestone 5

```rust
//! Deterministic directory ingestion pipeline with per-file parser detection, per-hand failure
//! diagnostics, a quarantine for rejected hands, and stack-continuity warnings.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::domain::canonical::CanonicalHand;
use crate::parser::base::ParseError;
use crate::parser::registry::ParserRegistry;
use crate::pipeline::continuity::{StackSnapshot, StackWarning, check_stack_continuity};
use crate::storage::repository::{HandSource, QuarantinedHand, SqliteRepository, StorageError};

/// Default number of hands buffered before committing a SQLite transaction.
pub const DEFAULT_BATCH_SIZE: usize = 1_000;

/// Fatal pipeline-level errors. Carries the report accumulated before the failure; hands from
/// the failed batch are not counted as ingested.
#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("storage transaction failed: {source}")]
    Storage {
        source: StorageError,
        partial_report: Box<IngestionReport>,
    },
}

/// Stage at which a hand was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureStage {
    Parse(ParseError),
    /// Rejected by the storage layer (message of the underlying [`StorageError`]).
    Storage(String),
}

impl FailureStage {
    /// Stage name stored with a quarantined hand (`"parse"` or `"storage"`).
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Parse(_) => "parse",
            Self::Storage(_) => "storage",
        }
    }
}

impl fmt::Display for FailureStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "{e}"),
            Self::Storage(msg) => write!(f, "rejected by storage: {msg}"),
        }
    }
}

/// Diagnostic record for a single hand that could not be ingested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandFailure {
    pub file_path: PathBuf,
    /// Parser that handled the file (see [`crate::parser::HandParser::describe`]).
    pub parser: String,
    /// 1-based line in `file_path` where the hand block starts.
    pub block_start_line: usize,
    /// Site hand id, if the header line could be read.
    pub hand_id: Option<String>,
    pub stage: FailureStage,
}

impl HandFailure {
    /// Best line number in the source file for this failure.
    #[must_use]
    pub fn file_line(&self) -> usize {
        let relative = match &self.stage {
            FailureStage::Parse(
                ParseError::IncompleteStreet { line, .. }
                | ParseError::IllegalAction { line, .. }
                | ParseError::UnrecognizedLine { line, .. }
                | ParseError::UnsupportedAdjustment { line, .. },
            ) => *line,
            _ => 0,
        };
        self.block_start_line + relative.saturating_sub(1)
    }
}

impl fmt::Display for HandFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let id = self.hand_id.as_deref().unwrap_or("<unknown id>");
        write!(
            f,
            "{}:{} [{}] hand {id}: {}",
            self.file_path.display(),
            self.file_line(),
            self.parser,
            self.stage
        )
    }
}

/// Telemetry report produced by a directory ingestion run.
#[derive(Debug, Clone)]
pub struct IngestionReport {
    /// Import run id; recorded for every hand this run contained (see
    /// [`SqliteRepository::delete_import_run`]).
    pub batch_run_id: Uuid,
    /// When the run started (stored in `import_runs.started_at`).
    pub started_at: DateTime<Utc>,
    pub files_processed: usize,
    /// Number of files handled by each parser (keyed by [`crate::parser::HandParser::describe`]).
    pub files_by_parser: BTreeMap<String, usize>,
    pub hands_ingested: usize,
    pub duplicates_skipped: usize,
    pub hands_failed: usize,
    /// Rejected hands stored with their raw text in `quarantined_hands` by this run.
    pub hands_quarantined: usize,
    /// Unreadable files, files no single parser recognized, and an input with no files.
    pub file_errors: Vec<String>,
    pub hand_failures: Vec<HandFailure>,
    /// Stacks that shrank between consecutive hands at a table (warnings, not failures).
    pub stack_warnings: Vec<StackWarning>,
}

impl IngestionReport {
    fn new() -> Self {
        Self {
            batch_run_id: Uuid::now_v7(),
            started_at: Utc::now(),
            files_processed: 0,
            files_by_parser: BTreeMap::new(),
            hands_ingested: 0,
            duplicates_skipped: 0,
            hands_failed: 0,
            hands_quarantined: 0,
            file_errors: Vec::new(),
            hand_failures: Vec::new(),
            stack_warnings: Vec::new(),
        }
    }

    fn record_failure(&mut self, failure: HandFailure) {
        self.hands_failed += 1;
        self.hand_failures.push(failure);
    }

    /// `true` if every file was recognized and every hand was stored or was a duplicate.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.file_errors.is_empty() && self.hands_failed == 0
    }
}

/// Where a buffered hand came from, for diagnostics, provenance and quarantine.
#[derive(Debug, Clone)]
struct Origin {
    file_path: PathBuf,
    parser: String,
    block_start_line: usize,
    hand_id: String,
    raw_text: String,
}

/// Hands and rejections waiting to be written in one batch.
#[derive(Default)]
struct Pending {
    hands: Vec<CanonicalHand>,
    origins: Vec<Origin>,
    quarantine: Vec<QuarantinedHand>,
    run_registered: bool,
}

impl Pending {
    fn len(&self) -> usize {
        self.hands.len() + self.quarantine.len()
    }
}

/// Coordinates recursive file discovery, per-file parser selection, streaming parsing, and
/// batched SQLite persistence.
#[derive(Debug, Clone, Copy)]
pub struct DirectoryIngester {
    batch_size: usize,
}

impl Default for DirectoryIngester {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_BATCH_SIZE,
        }
    }
}

fn quarantine_entry(report: &IngestionReport, failure: &HandFailure, raw: &str) -> QuarantinedHand {
    QuarantinedHand {
        import_run_id: report.batch_run_id,
        parser: failure.parser.clone(),
        file_path: failure.file_path.display().to_string(),
        start_line: failure.block_start_line,
        site_hand_id: failure.hand_id.clone(),
        stage: failure.stage.as_str().to_string(),
        error: failure.stage.to_string(),
        raw_text: raw.to_string(),
    }
}

impl DirectoryIngester {
    /// Creates a [`DirectoryIngester`] with a custom transaction batch size (minimum 1).
    #[must_use]
    pub const fn with_batch_size(batch_size: usize) -> Self {
        let effective = if batch_size == 0 { 1 } else { batch_size };
        Self {
            batch_size: effective,
        }
    }

    fn flush(
        repo: &mut SqliteRepository,
        pending: &mut Pending,
        report: &mut IngestionReport,
    ) -> Result<(), PipelineError> {
        if pending.len() == 0 {
            return Ok(());
        }
        let fail = |source: StorageError, report: &IngestionReport| PipelineError::Storage {
            source,
            partial_report: Box::new(report.clone()),
        };
        if !pending.run_registered {
            repo.register_import_run(report.batch_run_id, report.started_at)
                .map_err(|e| fail(e, report))?;
            pending.run_registered = true;
        }
        let sources: Vec<HandSource> = pending
            .origins
            .iter()
            .map(|o| HandSource {
                import_run_id: report.batch_run_id,
                file_path: o.file_path.display().to_string(),
                start_line: o.block_start_line,
                raw_text: o.raw_text.clone(),
            })
            .collect();
        let stats = repo
            .insert_sourced_hands_batch(&pending.hands, &sources)
            .map_err(|e| fail(e, report))?;
        report.hands_ingested += stats.hands_inserted;
        report.duplicates_skipped += stats.duplicates_skipped;
        for rejected in stats.rejected {
            let origin = pending.origins.get(rejected.batch_index);
            let failure = HandFailure {
                file_path: origin.map(|o| o.file_path.clone()).unwrap_or_default(),
                parser: origin.map(|o| o.parser.clone()).unwrap_or_default(),
                block_start_line: origin.map_or(0, |o| o.block_start_line),
                hand_id: Some(origin.map_or(rejected.site_hand_id, |o| o.hand_id.clone())),
                stage: FailureStage::Storage(rejected.error.to_string()),
            };
            if let Some(o) = origin {
                let entry = quarantine_entry(report, &failure, &o.raw_text);
                pending.quarantine.push(entry);
            }
            report.record_failure(failure);
        }
        report.hands_quarantined += repo
            .quarantine_hands(&pending.quarantine)
            .map_err(|e| fail(e, report))?;
        pending.hands.clear();
        pending.origins.clear();
        pending.quarantine.clear();
        Ok(())
    }

    /// Recursively discovers `.txt` files under `root_dir` (following symbolic links) and
    /// processes them in sorted path order. For each file, the registry selects the one parser
    /// that recognizes it; every hand block is parsed, valid hands are committed in batches, and
    /// rejected hands are quarantined with their raw text. Paths are recorded as absolute paths.
    ///
    /// # Errors
    /// Returns [`PipelineError`] (with the partial report) if a SQLite transaction fails.
    /// Unreadable or unrecognized files, an input without `.txt` files, and per-hand
    /// parse/storage rejections are recorded in the report, as are stack-continuity warnings.
    pub fn ingest_directory<P: AsRef<Path>>(
        &self,
        root_dir: P,
        registry: &ParserRegistry,
        repo: &mut SqliteRepository,
    ) -> Result<IngestionReport, PipelineError> {
        let mut report = IngestionReport::new();
        let root = std::path::absolute(root_dir.as_ref())
            .unwrap_or_else(|_| root_dir.as_ref().to_path_buf());
        let mut txt_files: Vec<PathBuf> = Vec::new();

        for entry_res in WalkDir::new(&root).follow_links(true) {
            match entry_res {
                Ok(entry) => {
                    let path = entry.path();
                    if entry.file_type().is_file()
                        && let Some(ext) = path.extension()
                        && ext.eq_ignore_ascii_case("txt")
                    {
                        txt_files.push(path.to_path_buf());
                    }
                }
                Err(walk_err) => report.file_errors.push(walk_err.to_string()),
            }
        }
        txt_files.sort();
        if txt_files.is_empty() && report.file_errors.is_empty() {
            report.file_errors.push(format!(
                "{}: no hand-history files (*.txt) found",
                root.display()
            ));
        }

        let mut pending = Pending::default();
        for file_path in txt_files {
            let raw_content = match fs::read_to_string(&file_path) {
                Ok(content) => content,
                Err(io_err) => {
                    report
                        .file_errors
                        .push(format!("{}: {io_err}", file_path.display()));
                    continue;
                }
            };
            let parser = match registry.detect(&raw_content) {
                Ok(parser) => parser,
                Err(detect_err) => {
                    report
                        .file_errors
                        .push(format!("{}: {detect_err}", file_path.display()));
                    continue;
                }
            };
            let parser_name = parser.describe();
            report.files_processed += 1;
            *report
                .files_by_parser
                .entry(parser_name.clone())
                .or_insert(0) += 1;

            let mut snapshots: Vec<StackSnapshot> = Vec::new();
            let mut unparsed_lines: Vec<usize> = Vec::new();
            for (block, result) in parser.parse_session(&raw_content) {
                match result {
                    Ok(hand) => {
                        snapshots.extend(StackSnapshot::of(&hand, block.start_line));
                        pending.origins.push(Origin {
                            file_path: file_path.clone(),
                            parser: parser_name.clone(),
                            block_start_line: block.start_line,
                            hand_id: hand.header.site_hand_id.clone(),
                            raw_text: block.text.to_string(),
                        });
                        pending.hands.push(hand);
                    }
                    Err(parse_err) => {
                        unparsed_lines.push(block.start_line);
                        let failure = HandFailure {
                            file_path: file_path.clone(),
                            parser: parser_name.clone(),
                            block_start_line: block.start_line,
                            hand_id: parser.peek_hand_id(block.text),
                            stage: FailureStage::Parse(parse_err),
                        };
                        pending
                            .quarantine
                            .push(quarantine_entry(&report, &failure, block.text));
                        report.record_failure(failure);
                    }
                }
                if pending.len() >= self.batch_size {
                    Self::flush(repo, &mut pending, &mut report)?;
                }
            }
            report.stack_warnings.extend(check_stack_continuity(
                &file_path,
                &mut snapshots,
                &unparsed_lines,
            ));
        }
        Self::flush(repo, &mut pending, &mut report)?;
        Ok(report)
    }
}
```

---

## FILE: src/main.rs · Milestone 5

```rust
//! CLI entrypoint for `poker_tracker`.
//!
//! - Import: `poker_tracker <input-dir> --ggpoker-currency <CODE> [--db <path>]`
//! - Undo an import run: `poker_tracker --delete-run <RUN-ID> [--db <path>]`
//!
//! Each supported site is enabled by its own option (given at most once). Sites whose exports
//! do not state the account currency (GGPoker) take it as the option value, in any letter case.
//! The parser for each file is detected automatically.
//!
//! Exit codes: `0` success (and `--help`), `1` fatal error, `2` usage error (including a missing
//! input directory), `3` import completed but some files or hands were rejected.

use std::env;
use std::path::Path;
use std::process::ExitCode;

use poker_tracker::domain::CurrencyCode;
use poker_tracker::parser::{GgPokerParser, ParserRegistry};
use poker_tracker::pipeline::{DirectoryIngester, IngestionReport, PipelineError};
use poker_tracker::storage::SqliteRepository;
use uuid::Uuid;

const USAGE: &str = "\
Usage:
  poker_tracker <input-dir> [site options] [--db <path>]   Import hand histories
  poker_tracker --delete-run <RUN-ID> [--db <path>]        Undo one import run
  poker_tracker --help                                     Show this help

Site options (enable at least one when importing; each at most once):
  --ggpoker-currency <CODE>   Parse GGPoker exports; amounts are in CODE (e.g. CAD, USD).
                              GGPoker prints `$` for every account currency, so it must be given.

Other options:
  --db <path>                 SQLite database (default: poker_tracker.sqlite)
  --delete-run <RUN-ID>       Undo an import (the Batch Run ID printed by that import). Hands
                              that another import also contained are kept.

Exit codes: 0 success, 1 fatal error, 2 usage error, 3 some files or hands were rejected.";

enum Command {
    Import {
        input_dir: String,
        registry: ParserRegistry,
    },
    DeleteRun(Uuid),
}

struct CliArgs {
    db_path: String,
    command: Command,
}

enum Parsed {
    Run(CliArgs),
    Help,
}

fn take_value(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
    seen: &mut Vec<String>,
) -> Result<String, String> {
    if seen.iter().any(|f| f == flag) {
        return Err(format!("{flag} given more than once"));
    }
    seen.push(flag.to_string());
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Parsed, String> {
    let mut input_dir = None;
    let mut db_path = "poker_tracker.sqlite".to_string();
    let mut registry = ParserRegistry::new();
    let mut delete_run = None;
    let mut seen: Vec<String> = Vec::new();
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => db_path = take_value(&mut args, "--db", &mut seen)?,
            "--ggpoker-currency" => {
                let raw = take_value(&mut args, "--ggpoker-currency", &mut seen)?;
                let currency: CurrencyCode = raw
                    .to_ascii_uppercase()
                    .parse()
                    .map_err(|e| format!("{e}"))?;
                registry.register(GgPokerParser::new(currency));
            }
            "--delete-run" => {
                let raw = take_value(&mut args, "--delete-run", &mut seen)?;
                let id = Uuid::parse_str(&raw).map_err(|e| format!("invalid run id: {e}"))?;
                delete_run = Some(id);
            }
            "-h" | "--help" => return Ok(Parsed::Help),
            flag if flag.starts_with("--") => return Err(format!("unknown option '{flag}'")),
            _ if input_dir.is_none() => input_dir = Some(arg),
            _ => return Err(format!("unexpected argument '{arg}'")),
        }
    }

    let command = match (delete_run, input_dir) {
        (Some(run), None) if registry.is_empty() => Command::DeleteRun(run),
        (Some(_), _) => {
            return Err("--delete-run cannot be combined with an import".to_string());
        }
        (None, Some(input_dir)) => {
            if registry.is_empty() {
                return Err("no site enabled; pass e.g. --ggpoker-currency CAD".to_string());
            }
            if !Path::new(&input_dir).is_dir() {
                return Err(format!(
                    "input directory '{input_dir}' does not exist or is not a directory"
                ));
            }
            Command::Import {
                input_dir,
                registry,
            }
        }
        (None, None) => return Err("missing <input-dir>".to_string()),
    };
    Ok(Parsed::Run(CliArgs { db_path, command }))
}

fn print_report(report: &IngestionReport) {
    println!("=== Poker Tracker Ingestion Summary ===");
    println!("Batch Run ID       : {}", report.batch_run_id);
    println!("Files Processed    : {}", report.files_processed);
    for (parser, files) in &report.files_by_parser {
        println!("  {parser:<17}: {files} file(s)");
    }
    println!("Hands Ingested     : {}", report.hands_ingested);
    println!("Duplicates Skipped : {}", report.duplicates_skipped);
    println!("Hands Failed       : {}", report.hands_failed);
    println!("Hands Quarantined  : {}", report.hands_quarantined);
    println!("File Errors        : {}", report.file_errors.len());
    println!("Stack Warnings     : {}", report.stack_warnings.len());
    for file_err in &report.file_errors {
        eprintln!("File error: {file_err}");
    }
    for failure in &report.hand_failures {
        eprintln!("Hand failure: {failure}");
    }
    for warning in &report.stack_warnings {
        eprintln!("Stack warning: {warning}");
    }
}

fn delete_run(db_path: &str, run: Uuid) -> ExitCode {
    // Never create a database just to delete from it.
    let mut repo = match SqliteRepository::open_existing(db_path) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("Failed to open SQLite database at '{db_path}': {err}");
            return ExitCode::from(1);
        }
    };
    match repo.delete_import_run(run) {
        Ok(deleted) => {
            println!(
                "Deleted {} hand(s) from import run {run}; kept {} hand(s) that another import \
                 also contained; removed {} quarantined hand(s)",
                deleted.hands_deleted, deleted.hands_kept, deleted.quarantined_deleted
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Failed to delete import run {run}: {err}");
            ExitCode::from(1)
        }
    }
}

fn import(db_path: &str, input_dir: &str, registry: &ParserRegistry) -> ExitCode {
    let mut repo = match SqliteRepository::open(db_path) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("Failed to open SQLite database at '{db_path}': {err}");
            return ExitCode::from(1);
        }
    };
    match DirectoryIngester::default().ingest_directory(input_dir, registry, &mut repo) {
        Ok(report) => {
            print_report(&report);
            if report.is_complete() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(3)
            }
        }
        Err(PipelineError::Storage {
            source,
            partial_report,
        }) => {
            print_report(&partial_report);
            eprintln!("Fatal storage error (the batch in progress was not saved): {source}");
            ExitCode::from(1)
        }
    }
}

fn main() -> ExitCode {
    let args = match parse_args(env::args().skip(1)) {
        Ok(Parsed::Run(a)) => a,
        Ok(Parsed::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(msg) => {
            eprintln!("Error: {msg}\n");
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match &args.command {
        Command::DeleteRun(run) => delete_run(&args.db_path, *run),
        Command::Import {
            input_dir,
            registry,
        } => import(&args.db_path, input_dir, registry),
    }
}
```

---

## FILE: tests/pipeline.rs · Milestone 5

```rust
//! End-to-end directory ingestion tests, including per-file parser detection.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;

use tempfile::TempDir;

use common::{REAL_SESSION, SAMPLE_SESSION, WALK, gg};
use poker_tracker::domain::CurrencyCode;
use poker_tracker::parser::{GgPokerParser, HandParser, ParseError, ParserRegistry};
use poker_tracker::pipeline::{DirectoryIngester, FailureStage};
use poker_tracker::storage::SqliteRepository;

fn registry() -> ParserRegistry {
    ParserRegistry::new().with(gg())
}

#[test]
fn ingests_real_session_with_no_failures() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("session.txt"), REAL_SESSION).unwrap();
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let report = DirectoryIngester::default()
        .ingest_directory(dir.path(), &registry(), &mut repo)
        .unwrap();
    assert_eq!(report.files_processed, 1);
    assert_eq!(report.files_by_parser.get("GGPoker (CAD)"), Some(&1));
    assert_eq!(report.hands_ingested, 294);
    assert_eq!(report.hands_failed, 0);
    assert!(report.file_errors.is_empty());

    // Every stored hand records the run, file and line it came from.
    let (with_source, runs): (i64, i64) = repo
        .connection()
        .query_row(
            "SELECT COUNT(*), COUNT(DISTINCT import_run_id) FROM hands
             WHERE source_file LIKE '%session.txt' AND source_line >= 1 AND import_run_id = ?1",
            [report.batch_run_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((with_source, runs), (294, 1));
    let first_line: i64 = repo
        .connection()
        .query_row(
            "SELECT source_line FROM hands WHERE site_hand_id = 'HD140647837'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(first_line, 1);
}

#[test]
fn unrecognized_and_ambiguous_files_are_reported_not_guessed() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a_gg.txt"), SAMPLE_SESSION).unwrap();
    fs::write(
        dir.path().join("b_other_site.txt"),
        "PokerStars Hand #254000000000: Hold'em No Limit ($0.05/$0.10 USD)\n",
    )
    .unwrap();

    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let report = DirectoryIngester::default()
        .ingest_directory(dir.path(), &registry(), &mut repo)
        .unwrap();
    assert_eq!(report.files_processed, 1);
    assert_eq!(report.hands_ingested, 2);
    assert_eq!(report.file_errors.len(), 1);
    assert!(report.file_errors[0].contains("b_other_site.txt"));
    assert!(report.file_errors[0].contains("no registered parser"));

    // Two sources claiming the same format is a configuration error, never a guess.
    let ambiguous = ParserRegistry::new()
        .with(GgPokerParser::new(CurrencyCode::CAD))
        .with(GgPokerParser::new(CurrencyCode::USD));
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let report = DirectoryIngester::default()
        .ingest_directory(dir.path(), &ambiguous, &mut repo)
        .unwrap();
    assert_eq!(report.hands_ingested, 0);
    assert!(
        report
            .file_errors
            .iter()
            .any(|e| e.contains("GGPoker (CAD)") && e.contains("GGPoker (USD)"))
    );
}

#[test]
fn sorts_files_skips_duplicates_and_pinpoints_failures() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a_folder");
    let b = dir.path().join("b_folder");
    fs::create_dir_all(&a).unwrap();
    fs::create_dir_all(&b).unwrap();
    fs::write(a.join("session1.txt"), SAMPLE_SESSION).unwrap();
    // Same two hands again; the first one is corrupted.
    let broken = SAMPLE_SESSION.replace("Dealt to Hero [5h 2s]", "Dealt to Hero [1h 2s]");
    fs::write(b.join("session2.txt"), &broken).unwrap();
    fs::write(b.join("notes.md"), "ignored").unwrap();

    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let report = DirectoryIngester::with_batch_size(1)
        .ingest_directory(dir.path(), &registry(), &mut repo)
        .unwrap();

    assert_eq!(report.files_processed, 2);
    assert_eq!(report.hands_ingested, 2);
    assert_eq!(report.duplicates_skipped, 1);
    assert_eq!(report.hands_failed, 1);

    let failure = &report.hand_failures[0];
    assert!(failure.file_path.ends_with("b_folder/session2.txt"));
    assert_eq!(failure.parser, "GGPoker (CAD)");
    assert_eq!(failure.hand_id.as_deref(), Some("HD140650645"));
    assert_eq!(failure.block_start_line, 1);
    assert!(matches!(
        failure.stage,
        FailureStage::Parse(ParseError::InvalidCardSyntax { .. })
    ));
    let message = failure.to_string();
    assert!(
        message.contains("session2.txt:1 [GGPoker (CAD)] hand HD140650645"),
        "{message}"
    );
}

#[test]
fn failure_line_points_into_the_source_file() {
    let dir = TempDir::new().unwrap();
    let broken = SAMPLE_SESSION.replace(
        "Hero: checks\n24eaa482: checks",
        "Hero: dances\n24eaa482: checks",
    );
    fs::write(dir.path().join("s.txt"), &broken).unwrap();
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let report = DirectoryIngester::default()
        .ingest_directory(dir.path(), &registry(), &mut repo)
        .unwrap();
    assert_eq!(report.hands_failed, 1);
    let expected_line = broken.lines().position(|l| l == "Hero: dances").unwrap() + 1;
    assert_eq!(report.hand_failures[0].file_line(), expected_line);
    assert_eq!(
        report.hand_failures[0].hand_id.as_deref(),
        Some("HD140650460")
    );
}

// ---------------------------------------------------------------------------------------------
// v3.5 review fixes
// ---------------------------------------------------------------------------------------------

fn count(repo: &SqliteRepository, sql: &str) -> i64 {
    repo.connection().query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn rejected_hands_are_quarantined_with_their_raw_text() {
    let dir = TempDir::new().unwrap();
    let broken = SAMPLE_SESSION.replace("Dealt to Hero [5h 2s]", "Dealt to Hero [1h 2s]");
    fs::write(dir.path().join("a.txt"), &broken).unwrap();
    // WALK already stored in USD, so the CAD import of it is a storage rejection.
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    repo.insert_hands_batch(&[GgPokerParser::new(CurrencyCode::USD)
        .parse_hand(WALK)
        .unwrap()])
        .unwrap();
    fs::write(dir.path().join("b.txt"), WALK).unwrap();

    let report = DirectoryIngester::default()
        .ingest_directory(dir.path(), &registry(), &mut repo)
        .unwrap();
    assert_eq!(report.hands_ingested, 1);
    assert_eq!(report.hands_failed, 2);
    assert_eq!(report.hands_quarantined, 2);
    assert!(!report.is_complete());
    let quarantined: Vec<(String, String, String)> = {
        let mut stmt = repo
            .connection()
            .prepare("SELECT stage, site_hand_id, raw_text FROM quarantined_hands ORDER BY stage")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(quarantined[0].0, "parse");
    assert_eq!(quarantined[0].1, "HD140650645");
    assert!(quarantined[0].2.contains("Dealt to Hero [1h 2s]"));
    assert_eq!(quarantined[1].0, "storage");
    assert_eq!(quarantined[1].2, WALK);
    // The stored hand keeps its raw text and an absolute source path.
    assert_eq!(
        count(
            &repo,
            "SELECT COUNT(*) FROM hands WHERE raw_text LIKE 'Poker Hand #HD140650460%'"
        ),
        1
    );
    let file: String = repo
        .connection()
        .query_row(
            "SELECT source_file FROM hands WHERE site_hand_id = 'HD140650460'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(std::path::Path::new(&file).is_absolute(), "{file}");
    // The run's start time is recorded, not the time of its first write.
    let started: String = repo
        .connection()
        .query_row(
            "SELECT started_at FROM import_runs WHERE id = ?1",
            [report.batch_run_id.to_string()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        started,
        report.started_at.format("%Y-%m-%dT%H:%M:%SZ").to_string()
    );
}

#[test]
fn a_stack_that_shrinks_between_consecutive_hands_is_reported() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("session.txt"), REAL_SESSION).unwrap();
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let report = DirectoryIngester::default()
        .ingest_directory(dir.path(), &registry(), &mut repo)
        .unwrap();
    assert!(
        report.stack_warnings.is_empty(),
        "{:?}",
        report.stack_warnings
    );

    // The newest hand's starting stack lowered by 0.01: still a valid hand on its own.
    let edited = REAL_SESSION.replacen(
        "Seat 2: Hero ($18.85 in chips)",
        "Seat 2: Hero ($18.84 in chips)",
        1,
    );
    assert_ne!(edited, REAL_SESSION);
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("session.txt"), &edited).unwrap();
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let report = DirectoryIngester::default()
        .ingest_directory(dir.path(), &registry(), &mut repo)
        .unwrap();
    assert_eq!(report.hands_ingested, 294);
    assert_eq!(report.stack_warnings.len(), 1);
    let w = &report.stack_warnings[0];
    assert!(w.is_hero);
    assert_eq!(
        (w.previous_hand_id.as_str(), w.hand_id.as_str()),
        ("HD140647808", "HD140647837")
    );
    assert!(
        w.to_string().contains("'Hero' (hero) starts with 18.84"),
        "{w}"
    );
    assert!(report.is_complete(), "a warning is not a failure");

    // A hand that fails to parse leaves a gap that is not reported as a stack change.
    let broken = REAL_SESSION.replacen("*** FIRST SHOWDOWN ***", "*** FIRST SHOWDOWN ** *", 1);
    assert_ne!(broken, REAL_SESSION);
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("session.txt"), &broken).unwrap();
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let report = DirectoryIngester::default()
        .ingest_directory(dir.path(), &registry(), &mut repo)
        .unwrap();
    assert_eq!(report.hands_failed, 1);
    assert!(
        report.stack_warnings.is_empty(),
        "{:?}",
        report.stack_warnings
    );
}

#[test]
fn symlinked_exports_are_followed_and_an_empty_input_is_reported() {
    let empty = TempDir::new().unwrap();
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let report = DirectoryIngester::default()
        .ingest_directory(empty.path(), &registry(), &mut repo)
        .unwrap();
    assert_eq!(report.files_processed, 0);
    assert!(report.file_errors[0].contains("no hand-history files"));
    assert!(!report.is_complete());
    assert_eq!(
        count(&repo, "SELECT COUNT(*) FROM import_runs"),
        0,
        "no empty run recorded"
    );

    #[cfg(unix)]
    {
        let exports = TempDir::new().unwrap();
        fs::write(exports.path().join("session.txt"), SAMPLE_SESSION).unwrap();
        let linked = TempDir::new().unwrap();
        std::os::unix::fs::symlink(exports.path(), linked.path().join("exports")).unwrap();
        let report = DirectoryIngester::default()
            .ingest_directory(linked.path(), &registry(), &mut repo)
            .unwrap();
        assert_eq!((report.files_processed, report.hands_ingested), (1, 2));
    }
}
```

---

## FILE: tests/cli.rs · Milestone 5

```rust
//! End-to-end tests of the `poker_tracker` binary: exit codes, messages and side effects.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

use common::SAMPLE_SESSION;

fn run(cwd: &Path, args: &[&str]) -> (i32, String, String) {
    let Output {
        status,
        stdout,
        stderr,
    } = Command::new(env!("CARGO_BIN_EXE_poker_tracker"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    (
        status.code().unwrap(),
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
    )
}

fn run_id(stdout: &str) -> String {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix("Batch Run ID       : "))
        .unwrap()
        .to_string()
}

#[test]
fn help_and_usage_errors_exit_with_their_codes_and_create_nothing() {
    let cwd = TempDir::new().unwrap();
    let (code, out, _) = run(cwd.path(), &["--help"]);
    assert_eq!(code, 0);
    assert!(out.contains("Usage:"));
    for args in [
        &["missing_dir", "--ggpoker-currency", "CAD"][..],
        &[
            ".",
            "--ggpoker-currency",
            "CAD",
            "--ggpoker-currency",
            "USD",
        ],
        &[".", "--bogus"],
        &["."],
        &[".", "--ggpoker-currency", "C$"],
    ] {
        let (code, _, err) = run(cwd.path(), args);
        assert_eq!(code, 2, "{args:?}: {err}");
    }
    let (_, _, err) = run(cwd.path(), &["missing_dir", "--ggpoker-currency", "CAD"]);
    assert!(err.contains("does not exist"), "{err}");
    assert!(
        fs::read_dir(cwd.path()).unwrap().next().is_none(),
        "no database may be created by a usage error"
    );
}

#[test]
fn imports_reimports_and_undoes_runs_from_the_command_line() {
    let cwd = TempDir::new().unwrap();
    let exports = cwd.path().join("exports");
    fs::create_dir(&exports).unwrap();
    fs::write(exports.join("s.txt"), SAMPLE_SESSION).unwrap();

    // The currency code is accepted in any letter case.
    let (code, out, err) = run(cwd.path(), &["exports", "--ggpoker-currency", "cad"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Hands Ingested     : 2"), "{out}");
    let first = run_id(&out);
    let (code, out, _) = run(cwd.path(), &["exports", "--ggpoker-currency", "CAD"]);
    assert_eq!(code, 0);
    assert!(out.contains("Duplicates Skipped : 2"), "{out}");
    let second = run_id(&out);

    // Undoing the first run keeps the hands the second run also contained.
    let (code, out, _) = run(cwd.path(), &["--delete-run", &first]);
    assert_eq!(code, 0);
    assert!(
        out.contains("Deleted 0 hand(s)") && out.contains("kept 2 hand(s)"),
        "{out}"
    );
    let (code, _, err) = run(cwd.path(), &["--delete-run", &first]);
    assert_eq!(code, 1);
    assert!(err.contains("does not exist"), "{err}");
    let (code, out, _) = run(cwd.path(), &["--delete-run", &second]);
    assert_eq!(code, 0);
    assert!(out.contains("Deleted 2 hand(s)"), "{out}");

    // Deleting from a database that does not exist neither succeeds nor creates it.
    let (code, _, _) = run(
        cwd.path(),
        &["--delete-run", &second, "--db", "other.sqlite"],
    );
    assert_eq!(code, 1);
    assert!(!cwd.path().join("other.sqlite").exists());

    // A partial import and an empty input both exit 3.
    let broken = SAMPLE_SESSION.replace("Dealt to Hero [5h 2s]", "Dealt to Hero [1h 2s]");
    fs::write(exports.join("s.txt"), broken).unwrap();
    let (code, out, _) = run(cwd.path(), &["exports", "--ggpoker-currency", "CAD"]);
    assert_eq!(code, 3);
    assert!(out.contains("Hands Quarantined  : 1"), "{out}");
    let empty = cwd.path().join("empty");
    fs::create_dir(&empty).unwrap();
    let (code, _, err) = run(cwd.path(), &["empty", "--ggpoker-currency", "CAD"]);
    assert_eq!(code, 3);
    assert!(err.contains("no hand-history files"), "{err}");
}
```
