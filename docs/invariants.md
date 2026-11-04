# `poker_tracker` Invariants

Living rules for `poker_tracker`: the site-neutral validator (§1), the GGPoker format (§2), and
the derived cache and reports (§3). It started from `docs/history/BLUEPRINT.md` §6.

---

## 1. Domain invariants (enforced by `validate_hand`, site-independent)

Validation is mandatory: every parser returns hands only after `validate_hand` succeeds, and
storage re-validates before inserting. No invariant is ever weakened or skipped.

### Header

- Non-empty site hand ID and table name.
- `2 ≤ max_seats ≤ 10`, and the button seat is within the table.
- `0 < small_blind ≤ big_blind`.
- If a UTC offset is present, it is within ±18 h (`utc_offset_seconds: Option<i32>`).

### Players

- Between 2 and `max_seats` players.
- Unique seats and unique player names.
- Positive starting stacks (`starting_stack > 0`).
- Hole cards in canonical slot order (`[Some(c1), Some(c2)]`, `[Some(c1), None]`, or `[None, None]`; `[None, Some(_)]` is invalid).
- **Exactly one hero**, holding two known cards.

### Actions

- Action sequences are numbered `0..n` consecutively in order.
- Streets never go backwards (`Preflop -> Flop -> Turn -> River`), and no player action is recorded on `Showdown`.
- `Fold` and `Check` carry zero amount; every other action carries a positive amount (`amount > 0`).
- The `street_total_after` chain holds:
  - blinds, straddles, calls, bets and raises add their amount;
  - an uncalled-bet return subtracts its amount;
  - dead blinds, folds and checks leave `street_total_after` unchanged.
- A player never commits more than their starting stack in total across the hand.
- A contribution is flagged all-in (`is_all_in == true`) **exactly** when it commits the player's whole remaining stack.
- No player acts after folding.

### Betting replay (No-Limit; seats clockwise by seat number)

- **Forced posts:**
  - Small blind, big blind(s), dead blinds, then up to two straddles (`ActionType::PostStraddle`); straddles are the last forced posts (the order of the other posts is not checked).
  - At most one small blind and at least one big blind.
  - A live small or big blind equals the stake, or less only when the poster is all-in.
  - Dead money (`PostDeadBlind`) is at most one big blind.
  - Up to two straddles, by different players, after every blind. Any seat may straddle, including a blind poster (observed: SB, BB, UTG, button), and the order is not tied to seat position (HD145385574: the BB straddles, then the button re-straddles).
  - Each straddle must be exactly 2× the highest commitment before it (less only when all-in).
- **Action order:**
  - Preflop action starts left of the last straddler (or left of the first big blind poster if no straddle).
  - Postflop streets start left of the button.
  - Each action belongs to the next player clockwise who still owes an action. A player who folded or is all-in owes none.
  - A betting round closes only when everyone who can act has acted. A bet or raise makes every other player who can act owe an action again. A lone player who can still act and already matches the highest commitment owes nothing (e.g. the big blind after a short all-in call).
- **Betting legality:**
  - `Check` only when not facing a bet (`street_bet == 0` or player already matches `street_bet`).
  - `Call` matches the bet faced, or puts the player all-in for less.
  - `Bet` only when no bet is faced, of at least the big blind unless all-in.
  - `Raise` only when facing a bet:
    - Preflop after straddle(s), the first raise must add at least the last straddle's total commitment (standard rule), or less only when all-in.
    - Otherwise, a raise must exceed the faced bet by at least the last full bet or raise (initially the big blind), or complete a smaller all-in to the big blind, unless all-in.
  - A bet or raise is legal only when at least one other active opponent has chips remaining.
  - A player who already acted on the street may raise again only when facing at least a full raise since then; an all-in for less than a full raise does not reopen the betting (a check preserves the right to raise).
- **Uncalled bet returns:**
  - Returned only after betting has closed on the street, to the player with the largest commitment, and equals exactly the unmatched portion of that player's bet.
- **Street progression:**
  - Actions occur only on streets whose community cards were dealt.
  - No street is dealt after all but one player has folded.
  - A hand still contested by two or more players reaches the river.

### Outcome

- **Summary arithmetic:** `total_pot − Σ deductions = Σ payouts`.
- **Chip conservation:** `Σ contributions − Σ uncalled returns = total_pot`.
- **Side pots:** for every winner's live contribution `L`, the players whose live contribution is at most `L` are paid at most `Σ_all players min(live contribution, L) + Σ dead money`.
- Deductions are nonnegative and labeled (`Rake`, `Jackpot`, or `Other`).
- `uncalled_returned` equals the sum of all `UncalledBetReturned` actions.
- A pot larger than zero has a winner.
- Every payout is positive and awarded to a player who did not fold.
- Single-board hands pay on `run_index = 0`. Multi-run hands pay on runs `1..=n`, and every runout pays at least once.
- **Adjustments:**
  - An adjustment may be triggered on any street before the showdown, preflop included; a site's parser may allow fewer (§2).
  - Reference seated players.
  - At most one adjustment per player.
  - Are nonzero, representable, and carry the sign required by their kind:
    - `EvCashoutRiskPaid`: negative (debit).
    - `EvCashoutReceived`: positive (credit).
  - Are never triggered at showdown.
  - `EvCashoutReceived` only for a non-folded player with zero payout (`won == 0`).
  - `EvCashoutRiskPaid` only for a player with a positive payout (`won > 0`).
  - (The GGPoker parser additionally refuses any cashout in a multi-run hand or a hand with more than one payout line; see §2. `validate_hand` does not.)
- Every player's net converts losslessly to integer micro-units (`1.00 = 1_000_000`, `i64`).

### Boards

- Turn requires a flop; river requires a turn (otherwise `IncompleteStreet`, with `line: 0`, meaning no line).
- Multi-run boards are runs `1..=n` with `2 ≤ n ≤ MAX_RUNOUTS` (3), each dealt to the river.
- In a multi-run hand, every runout has the first runout's cards on each street up to and including the street of the last action that is not `UncalledBetReturned`.
- No card appears twice among the hole cards and any single runout.
- No card is dealt twice across runouts: the cards every runout shares (the streets bet on, above) are counted once, and every street dealt after the betting ended is dealt anew in each runout, so its cards are new.

---

## 2. GGPoker parser format rules (`src/parser/ggpoker.rs`)

Site-specific text format rules observed in real exports:

- **Amounts and cards, on every line:** an amount is exact and has at most two decimals (it is never rounded); a card token has the observed case (ranks `23456789TJQKA`, suits `cdhs`).
- **Seat lines:** come first, in ascending seat order.
- **Blinds and straddles:**
  - Small blind is the first blind posted.
  - Blinds and straddles may be posted only before `*** HOLE CARDS ***`.
  - A blind that uses the poster's whole stack is recorded as all-in (GGPoker prints no suffix there).
  - Refused as never observed: a live blind for less than the stake, a missed blind in a heads-up hand, a heads-up straddle, a second straddle by the same player, and an all-in straddle (whatever its size).
- **Hole cards:**
  - Exactly one `Dealt to` line per seated player, in seat order, directly after `*** HOLE CARDS ***`.
  - Only Hero's line carries cards (`Dealt to Hero [c1 c2]`); opponent lines are printed with a trailing space (`Dealt to <name> `); the parser does not depend on it.
- **Action lines:**
  - Check, call, bet, raise, straddle legality; `raises $x to $y` requires `facing + x = y`.
  - Straddle lines use the observed syntax `<name>: straddle $x`.
  - EV Cashout:
    - Chosen on flop or turn (`<name>: Chooses to EV Cashout`), at most once per player (refused preflop or river).
    - Settled on the river by `<name>: Pays Cashout Risk ($x)` (winner) or `<name>: Receives Cashout ($x)` (loser).
    - Single-run hands with exactly one payout line only. A cashout in a multi-run hand, or in any hand with two or more `collected` lines (a split pot, but also one player collecting a main and a side pot), is quarantined as `UnsupportedAdjustment`.
  - Multi-run headers: `*** SECOND/THIRD FLOP/TURN ***` headers are accepted only when no action took place on that street.
  - Shows lines: body `shows` lines allow one non-empty parenthesised description without nested parentheses and nothing after it.
- **Showdown headers:** come in run order (`*** FIRST/SECOND/THIRD SHOWDOWN ***`).
- **Summary section:**
  - Per-seat winnings and cashout settlements: body `Pays Cashout Risk ($x)` ⇔ summary `, Cashout Risk ($x)`; body `Receives Cashout ($x)` ⇔ summary `, EV Cashout ($x)`, with equal amounts.
  - Board lines match dealt cards (for multi-run hands, a suffix of each runout). A summary `Board` line whose run already had one is refused.
  - The stated run count (`Hand was run two/three times`) matches.
  - Every seat line follows the observed grammar:
    - `folded before Flop [(didn't bet)]`
    - `folded on the <street>`
    - `won|collected ($x)`
    - `showed [c1] and won|collected ($x)` or `showed [c1 c2] and won|collected ($x)` (uncontested winner, no description; tried before the general `showed` shape)
    - `showed [c1 c2] and <outcome>[, and <outcome>]…[, Cashout Risk ($x) | , EV Cashout ($x)]` where outcome is `won ($x)` or `lost`, optionally followed by `with <description>`
  - **`won` vs `collected`:** `won` appears if and only if a flop was dealt; `collected` appears if and only if no flop was dealt, on plain and `showed` lines alike.
  - **Summary position labels:**
    - `(button)`, `(small blind)` and `(big blind)` match the button seat, small blind poster, and first big blind poster. Duplicate position labels on a single seat line are refused.
    - **Heads-up rule:** in a heads-up hand a player must sit at the button seat and post the small blind (any other arrangement is refused), and the button's summary line carries `(small blind)` only (GGPoker omits `(button)`).
  - **Shown cards:**
    - On the general `showed` shape, shown cards equal the player's known cards.
    - On the uncontested shape, an opponent's summary cards include every card they showed in the body (a second card may be added); Hero's summary cards are among Hero's dealt cards. A card that collides with the board or another player is `ConflictingHoleCards` or `DuplicateCard`.
    - An uncontested winner may show 1 or 2 cards, or not show.
    - A player still in a contested hand must show, with one outcome per runout.

---

## 3. Derived data and reports (`src/stats/`, `src/report/`, `hand_player_stats`)

`docs/stats.md` defines every position, counter, money column, session and equity figure. This
section holds the rules it does not state. The module, report and CLI contracts are in
`.agents/rules/ARCHITECTURE.md` §7–§8.

### Cache lifecycle

- **A cache only.** `hand_player_stats` is computed only from stored canonical hands. Validation,
  ingestion and duplicate detection never read it, and dropping it loses nothing.
- **Shape.** One row per `hand_players` row: `(hand_id, player_id)` primary key (`hand_id`
  references `hands(id)` `ON DELETE CASCADE`, `player_id` references `players(id)`
  `ON DELETE RESTRICT`), a nullable
  `position` code (0–9), and one `INTEGER NOT NULL` column per entry of `stats::COLUMNS`, in that
  order. Every column carries `CHECK (… >= 0)` except `ev_net_micros`, which is signed. The table
  (`STRICT, WITHOUT ROWID`) is created by the rebuild, never by `MIGRATIONS`.
- **Versioning.** `derived_cache(name, version, built_at)` (migration 3) holds one row,
  `name = 'hand_player_stats'`, with the `STATS_VERSION` the cache was built with. When that
  version is bumped: `AGENTS.md` §3.10.
- **Insert.** A newly stored hand's rows are written in its own savepoint; a duplicate writes
  none.
- **Rebuild on open.** After migrations, if the `derived_cache` row is missing or its version
  differs from `STATS_VERSION`, the open rebuilds the whole cache in one `BEGIN IMMEDIATE`
  transaction: it drops the table, creates it from `COLUMNS`, derives every stored hand and
  upserts `derived_cache`. It re-reads the version under the lock, so concurrent opens rebuild
  once. A new, empty database is rebuilt instantly. `rebuilt_on_open()` reports it.
  `rebuild_derived()` (CLI `--rebuild-stats`) rebuilds unconditionally.
- **Waiting.** Every open takes the write lock briefly (the migration loop). An open that starts
  while another connection rebuilds waits up to the 5 s busy timeout, then fails with the usual
  lock error (CLI exit `1`); it is not retried. The concurrency test therefore uses a small
  database (a few hands), so its rebuild stays far inside the timeout in a debug build.
- **Unloadable hand.** If a stored hand cannot be loaded, or an error occurs while processing it
  during a rebuild (e.g. `MissingPlayerId`), the rebuild and the open fail with
  `StorageError::DerivedRebuildFailed { platform, site_hand_id, source }` and nothing changes.
  `platform` and `site_hand_id` are read from `hands` as stored text, so the error names the hand
  even when the platform column is the corrupt value. The message names the hand and the load
  error (`cannot rebuild the statistics cache: GGPoker hand HD140650557 cannot be loaded: …`).
  This fails closed on purpose: such a database cannot be opened by this build, not even for
  `--delete-run`, until the row is repaired outside `poker_tracker`.
- **Version guard.** A connection can outlive the cache it opened: another build may rebuild it
  meanwhile. So `insert_hands_batch` and `insert_sourced_hands_batch` re-read
  `derived_cache.version` under their write lock before the first savepoint, and
  `report::write_report` makes the same check as the first read of its one read transaction. A
  missing or different version is `StorageError::StaleDerivedCache { found: Option<i64>,
  expected: i64 }`.
  - For an insert, this is a batch-level error (like a lock error): it stores and quarantines
    nothing, the pipeline returns `PipelineError::Storage`, and the CLI exits `1`. It never
    surfaces as per-hand rejections.
  - For a report, it writes nothing to its output.
  - The message says that another `poker_tracker` build changed the statistics cache (cache
    version `found`, this build `expected`) and to run the command again. The next open
    rebuilds the cache.
  - The guard never rebuilds by itself. A mismatch means two builds share the database, which
    must be visible, and a rebuild inside an import's write lock can take seconds.
  - `rebuild_derived` and `delete_import_run` need no guard.
- **Delete.** Rows go with their hand (`ON DELETE CASCADE` on `hand_id`).

### Positions

Defined in `docs/stats.md` §2, including its standard layout check (when it fails, every
position of the hand is `NULL`). Heads-up, the button is the SB, so its open is a steal attempt
that the BB faces (`docs/stats.md` §4.1).

### Fail-closed derived values

`derive_hand_stats` is infallible. An amount it cannot represent in micro-units, and a sum that
overflows, never become a plausible value. For hands storage can hold this never happens: every
stored amount is an `i64` of micro-units, and storage computes the sums itself. So no definition
in `docs/stats.md` depends on this policy.

- **Money:**
  - A blind or straddle sum that overflows makes that column `0`.
  - If any player's `invested` cannot be represented, every rake and fee part of the hand is `0`.
  - The largest-remainder split computes its products, weights and parts in `i128`, with
    negative weights clamped to 0 in both the products and the sum.
- **All-in equity:** the hand does not qualify (`ev_qualified = 0`, equity `0/1`,
  `ev_net_micros = net_micros`) when any of these cannot be represented or overflows:
  - a live player's `invested`, `PostDeadBlind` amounts, adjustments or net;
  - the deductions sum;
  - the pot after deductions;
  - a live player's `ev_net_micros`.

### Reports

- **Hero only, per currency.** Every row covers the hero's hands in one currency. Currencies are
  listed alphabetically, and amounts are never added across them.
- **Reading.** One read transaction, after the version guard. Reports read only integer columns,
  integer `SUM`s and text columns; arithmetic and display follow `docs/stats.md` §1.2.
- **Exact rates.** `bb_won`, `bb/100` and `ev_bb_per_100` (`docs/stats.md` §6.1–§6.2) use checked
  `i128` fractions. A figure that does not fit fails the report with
  `ReportError::ArithmeticOverflow { figure }`, naming the figure. Nothing is written, and the
  CLI exits `1`.
- **CSV.** A header line, then the rows, with LF endings. A database with no hands prints the
  header only. Text fields (`currency`, `stakes`, `platform`, `table`, `position`, `stat`) are
  quoted per RFC 4180 when they contain `,`, `"`, CR or LF (quotes doubled); every other field
  is unquoted.
- **Columns and rows**, in this order:

  | Report | Columns | Rows |
  | --- | --- | --- |
  | `results` | `currency,stakes,hands,net,net_per_100,bb_won,bb_per_100,rake,fees,blinds,straddles,allin_hands,ev_net,ev_bb_per_100` | Grouped by currency. Within a currency there is one row per stakes among the selected hands, ordered by big blind then small blind. `stakes` is `<sb>/<bb>`, each amount printed exactly (`0.05/0.1`). Then comes one `stakes = ALL` row combining all of that currency's selected stakes, every column included, as the last row of the block, present only when the block has more than one stakes row |
  | `positions` | `currency,position,hands,net,bb_per_100,vpip,pfr,rfi` | One per currency and position label, in the order `EP, MP, CO, BTN, SB, BB`, then `?` for `NULL` positions if any |
  | `sessions` | `currency,platform,table,start,end,duration_seconds,hands,net,bb_per_100,ev_net` | One per session (`docs/stats.md` §6.1), ordered by currency, `start` as printed, table, then platform. `platform` is `Platform::as_str()` (the session key includes it, so two sites' tables of the same name stay apart). `start`/`end` are the first and last hands' timestamps as stored (`2026-08-21T16:54:10`). `duration_seconds` is an integer |
  | `stats` | `currency,stat,numerator,denominator,value` | One per stat of `docs/stats.md` §5, in that table's order (`af_*` and `afq_*` last). For `vpip`, the denominator printed is `Σ hands − Σ p_walk` |

- **Stakes selection** (`--stakes`, `ReportOptions::stakes`):
  - Each item is a small blind then a big blind. Each amount is a positive plain decimal: ASCII
    digits with at most one `.`, which has digits on both sides, and at most 6 decimal places.
    Surrounding spaces are ignored.
  - Items are compared numerically with the stored blinds (`0.05/0.10` selects the stakes
    printed `0.05/0.1`), in every currency, and currencies stay separate.
  - A repeated item counts once. An item that matches no hero hand is reported, not an error.
  - Every report covers only the selected hands, and sessions are formed from them.
  - Together with the `results` rows per stakes and the `ALL` rows, this gives results by stakes,
    for all stakes combined, and for any combination of stakes.
- **Corrupt rows fail closed.** A stored timestamp that does not parse is
  `StorageError::CorruptRow`, as when the hand is loaded; it never becomes a default value.
