# `poker_tracker` Invariants

Living domain, validator and site-parser rules for `poker_tracker` v3.6+.
Derived from `docs/BLUEPRINT.md` §6 with the v3.6 extensions for heads-up hands, summary shows,
straddles, and EV Cashout on both sides.

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
  - Small blind, big blind(s), dead blinds, then up to two straddles (`ActionType::PostStraddle`).
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
- Every payout is positive and awarded to a player who did not fold.
- Single-board hands pay on `run_index = 0`. Multi-run hands pay on runs `1..=n`, and every runout pays at least once.
- **Adjustments:**
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

- Turn requires a flop; river requires a turn.
- Multi-run boards are runs `1..=n` with `2 ≤ n ≤ MAX_RUNOUTS` (3), each dealt to the river.
- No card appears twice among the hole cards and any single runout.
- No card is dealt twice across runouts; cards shared with the first runout are counted once.

---

## 2. GGPoker parser format rules (`src/parser/ggpoker.rs`)

Site-specific text format rules observed in real exports:

- **Seat lines:** come first, in ascending seat order.
- **Blinds and straddles:**
  - Small blind is the first blind posted.
  - Blinds and straddles may be posted only before `*** HOLE CARDS ***`.
  - A blind that uses the poster's whole stack is recorded as all-in (GGPoker prints no suffix there).
  - Refused as never observed: a live blind for less than the stake, a missed blind in a heads-up hand, a heads-up straddle, a second straddle by the same player, and an all-in straddle (whatever its size).
- **Hole cards:**
  - Exactly one `Dealt to` line per seated player, in seat order, directly after `*** HOLE CARDS ***`.
  - Only Hero's line carries cards (`Dealt to Hero [c1 c2]`); opponent lines end with a trailing space (`Dealt to <name> `).
- **Action lines:**
  - Check, call, bet, raise, straddle legality; `raises $x to $y` requires `facing + x = y`.
  - Straddle lines use the observed syntax `<name>: straddle $x`.
  - EV Cashout:
    - Chosen on flop or turn (`<name>: Chooses to EV Cashout`), at most once per player.
    - Settled on the river by `<name>: Pays Cashout Risk ($x)` (winner) or `<name>: Receives Cashout ($x)` (loser).
    - Single-run hands with exactly one payout line only. A cashout in a multi-run hand, or in any hand with two or more `collected` lines (a split pot, but also one player collecting a main and a side pot), is quarantined as `UnsupportedAdjustment`.
- **Showdown headers:** come in run order (`*** FIRST/SECOND/THIRD SHOWDOWN ***`).
- **Summary section:**
  - Per-seat winnings and cashout settlements: body `Pays Cashout Risk ($x)` ⇔ summary `, Cashout Risk ($x)`; body `Receives Cashout ($x)` ⇔ summary `, EV Cashout ($x)`, with equal amounts.
  - Board lines match dealt cards (for multi-run hands, a suffix of each runout).
  - The stated run count (`Hand was run two/three times`) matches.
  - Every seat line follows the observed grammar:
    - `folded before Flop [(didn't bet)]`
    - `folded on the <street>`
    - `won|collected ($x)`
    - `showed [c1] and won|collected ($x)` or `showed [c1 c2] and won|collected ($x)` (uncontested winner, no description; tried before the general `showed` shape)
    - `showed [c1 c2] and <outcome>[, and <outcome>]…[, Cashout Risk ($x) | , EV Cashout ($x)]` where outcome is `won ($x)` or `lost`, optionally followed by `with <description>`
  - **`won` vs `collected`:** `won` appears if and only if a flop was dealt; `collected` appears if and only if no flop was dealt, on plain and `showed` lines alike.
  - **Summary position labels:**
    - `(button)`, `(small blind)` and `(big blind)` match the button seat, small blind poster, and first big blind poster.
    - **Heads-up rule:** in a heads-up hand the button posts the small blind (any other arrangement is refused), and the button's summary line carries `(small blind)` only (GGPoker omits `(button)`).
  - **Shown cards:**
    - On the general `showed` shape, shown cards equal the player's known cards.
    - On the uncontested shape, an opponent's summary cards include every card they showed in the body (a second card may be added); Hero's summary cards are among Hero's dealt cards. A card that collides with the board or another player is `ConflictingHoleCards` or `DuplicateCard`.
    - An uncontested winner may show 1 or 2 cards, or not show.
    - A player still in a contested hand must show, with one outcome per runout.

---

## 3. Derived data (`src/stats/`, `src/report/`, `hand_player_stats`)

Rules for the derived statistics cache (v3.7, M13–M15) and the reports built on it (M15).
Definitions of every position, counter, money column and session are in `docs/stats.md`; the
lifecycle contract is `docs/SPEC_v3.7.md` §2 and the report contract §6.

### Cache lifecycle

- **A cache only.** `hand_player_stats` is computed only from stored canonical hands. Validation,
  ingestion and duplicate detection never read it, and dropping it loses nothing.
- **Shape.** One row per `hand_players` row: `(hand_id, player_id)` primary key, a nullable
  `position` code (0–9), and one `INTEGER NOT NULL CHECK (… >= 0)` column per entry of
  `stats::COLUMNS`, in that order. The table is created by the rebuild, never by `MIGRATIONS`.
- **Versioning.** `derived_cache(name, version, built_at)` (migration 3) holds one row,
  `name = 'hand_player_stats'`, with the `STATS_VERSION` the cache was built with. As
  `AGENTS.md` §3.10 says, changing the definition of any stored column (position, counter, money
  or equity column) bumps `STATS_VERSION` in the same commit; figures computed at report time
  (stat ratios, `bb_won`, `bb/100`, sessions) do not.
- **Insert.** A newly stored hand's rows are written in its own savepoint; a duplicate writes
  none.
- **Rebuild on open.** After migrations, if the `derived_cache` row is missing or its version
  differs from `STATS_VERSION`, the open rebuilds the whole cache in one `BEGIN IMMEDIATE`
  transaction, re-reading the version under the lock so concurrent opens rebuild once.
  `rebuilt_on_open()` reports it. `rebuild_derived()` (CLI `--rebuild-stats`) rebuilds
  unconditionally.
- **Unloadable hand.** If a stored hand cannot be loaded, the rebuild and the open fail with
  `StorageError::DerivedRebuildFailed { platform, site_hand_id, source }` (stored text, so it
  names the hand even when the platform column is the corrupt value) and nothing changes.
- **Version guard.** `insert_hands_batch` and `insert_sourced_hands_batch` re-read
  `derived_cache.version` under their write lock before any write. A missing or different
  version is `StorageError::StaleDerivedCache { found, expected }`: a batch-level error that
  stores and quarantines nothing. They never rebuild by themselves. `report::write_report` makes
  the same check as the first read of its one read transaction and, on a mismatch, writes
  nothing to its output.
- **Delete.** Rows go with their hand (`ON DELETE CASCADE` on `hand_id`).

### Positions (`docs/stats.md` §2)

- PokerTracker numbering, counted **counter-clockwise from the button** among the players dealt
  in, with the blinds fixed: `SB` = 9, `BB` = 8, then `BTN` = 0, `CO` = 1, and 2, 3, … for the
  next players counter-clockwise.
- Labels are derived at report time from the code and `max_seats`: with `max_seats ≤ 6`, code 2
  is `MP` and codes 3–7 are `EP`; with `max_seats ≥ 7`, codes 2–4 are `MP` and 5–7 are `EP`.
  Only the code is stored.
- **Heads-up:** the button posts the small blind and is `SB` (9); the other player is `BB` (8).
  There is no code 0.
- The SB is the single `PostSmallBlind` poster and the BB the first `PostBigBlind` poster. A
  player who posts a missed blind or an extra big blind, or who straddles, keeps their seat's
  position.
- **Standard layout check.** Positions are assigned only when the button seat is dealt in,
  exactly one small blind is posted, and (3+ players) the SB is the first player clockwise after
  the button and the BB the first player clockwise after the SB, or (heads-up) the button is the
  SB. Otherwise every player in the hand gets `NULL`.


### Opportunity counters (`docs/stats.md` §3, §4.1–§4.2)

- **State at a decision:** computed by replaying the street up to, not including, the decision.
- **Single evaluation per player-hand:** opportunities are counted at the player's first
  qualifying decision, with two exceptions: `p_4bet_fold` counts any fold at `raises >= 3`,
  and `p_pfr_opp` counts any decision with `can_raise`.
- **Cold call:** `p_cc_opp` requires no money in the pot (no forced post of any kind).
- **Steal:** `p_steal_opp` requires `p_rfi_opp`, seat position CO (1), BTN (0), or SB (9), and
  the hand has no `PostStraddle`. Heads-up, the SB's open is a steal attempt that the BB faces.
- **C-bets:** `f_cbet_faced` and `t_cbet_faced` require no `Raise` between the c-bet and the
  decision; `t_cbet_opp` is available only to the player who made the flop c-bet.

### Money (`docs/stats.md` §6.1)

- `blinds_posted_micros` sums the player's `PostSmallBlind`, `PostBigBlind` and `PostDeadBlind`
  amounts; `straddles_posted_micros` sums their `PostStraddle` amounts.
- **Contributed method.** `rake_attributed_micros` splits the hand's `Rake` deductions, and
  `fees_attributed_micros` one sum of all its other deductions (`Jackpot` and `Other`), among all
  players in proportion to `invested`.
- **Largest remainder.** Each player gets `floor(T × invested / W)` (`W` = the invested total =
  `total_pot`), with `i128` products; the leftover micro-units go one each to the largest
  remainders `(T × invested) mod W`, ties to the lower seat number. The parts sum exactly to the
  hand's `Rake` and to its other deductions (a corpus test checks every hand). If `T` or `W` is
  `0`, every part is `0`.

### Reports (`src/report/`, `docs/SPEC_v3.7.md` §6)

- **Hero only, per currency.** Every row covers the hero's hands in one currency; currencies are
  listed alphabetically and amounts are never added across them.
- **Integers in, `Decimal` out.** Only integer columns, integer `SUM`s and text columns are read
  from SQLite, and nothing is divided in SQL. Ratios are computed with `Decimal`. Percents,
  factors, `bb_won`, `bb/100` and `$/100` are rounded half-even to 2 decimals and printed with 2;
  money is printed exactly, trailing zeros removed; a zero denominator prints an empty cell.
- **`bb_won`** sums the net per big-blind value, divides once per value with `Decimal`, and adds
  the quotients (`docs/stats.md` §6.1), in every report and row, `ALL` rows included. It is never
  `Σ net / Σ big_blind`, and never divided hand by hand.
- **Stakes selection.** Each `--stakes` amount is a positive plain decimal: ASCII digits with at
  most one `.`, which has digits on both sides, at most 6 decimal places, surrounding spaces
  ignored. Amounts are compared numerically with the stored blinds in every currency. A repeated
  item counts once; an item that matches no hero hand is reported, not an error.
- **Corrupt rows fail closed.** A stored timestamp that does not parse is
  `StorageError::CorruptRow`, as when the hand is loaded; it never becomes a default value.

### Sessions (`docs/stats.md` §6.1)

- A session is a maximal run of the hero's hands at one table, the same `(platform, currency,
  table_name)`, whose consecutive hands start at most 30 minutes apart (inclusive). Sessions are
  computed at report time from the selected hands and never cached.
- **One time basis per table:** UTC (timestamp minus `utc_offset_seconds`) when every selected
  hand of the table states an offset, otherwise the timestamps as printed. Hands are ordered by
  that time, then site hand id. Gaps and `duration_seconds` (first to last hand start, whole
  seconds) use it, so neither is negative.
- Sessions are listed by currency, `start` as printed, table, then platform.
