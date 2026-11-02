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
  - Up to two straddles. Each straddle is posted before the deal in turn order clockwise from the big blind (UTG straddle first, then double straddle / UTG+1 straddle).
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
    - Otherwise, a raise must exceed the faced bet by at least the last full bet or raise (initially the big blind), or complete a smaller all-in to the minimum raise, unless all-in.
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
  - Single-run and unsplit pots only.
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
  - A blind or straddle that uses the poster's whole stack is recorded as all-in (GGPoker prints no suffix there).
- **Hole cards:**
  - Exactly one `Dealt to` line per seated player, in seat order, directly after `*** HOLE CARDS ***`.
  - Only Hero's line carries cards (`Dealt to Hero [c1 c2]`); opponent lines end with a trailing space (`Dealt to <name> `).
- **Action lines:**
  - Check, call, bet, raise, straddle legality; `raises $x to $y` requires `facing + x = y`.
  - Straddle lines use the observed syntax `<name>: straddle $x`.
  - EV Cashout:
    - Chosen on flop or turn (`<name>: Chooses to EV Cashout`), at most once per player.
    - Settled on the river by `<name>: Pays Cashout Risk ($x)` (winner) or `<name>: Receives Cashout ($x)` (loser).
    - Single-run, unsplit pots only; cashout in multi-run or split pots is quarantined as `UnsupportedAdjustment`.
- **Showdown headers:** come in run order (`*** FIRST/SECOND/THIRD SHOWDOWN ***`).
- **Summary section:**
  - Per-seat winnings and cashout settlements: body `Pays Cashout Risk ($x)` ⇔ summary `, Cashout Risk ($x)`; body `Receives Cashout ($x)` ⇔ summary `, EV Cashout ($x)`, with equal amounts.
  - Board lines match dealt cards (for multi-run hands, a suffix of each runout).
  - The stated run count (`Hand was run two/three times`) matches.
  - Every seat line follows the observed grammar:
    - `folded before Flop [(didn't bet)]`
    - `folded on the <street>`
    - `won|collected ($x)`
    - `showed [c1 c2] and won|collected ($x)` (uncontested winner, no description)
    - `showed [c1 c2] and <outcome>[, and <outcome>]…[, Cashout Risk ($x) | , EV Cashout ($x)]` where outcome is `won ($x)` or `lost`, optionally followed by `with <description>`
  - **`won` vs `collected`:** `won` appears if and only if a flop was dealt; `collected` appears if and only if no flop was dealt, on plain and `showed` lines alike.
  - **Summary position labels:**
    - `(button)`, `(small blind)` and `(big blind)` match the button seat, small blind poster, and first big blind poster.
    - **Heads-up rule:** in a heads-up hand where the button posted the small blind, the button's summary line carries `(small blind)` only (the `(button)` label is omitted by GGPoker).
  - **Shown cards:**
    - Shown cards equal the player's known cards; for uncontested summary shows, cards must agree with any previously known cards for that player.
    - An uncontested winner may show 1 or 2 cards, or not show.
    - A player still in a contested hand must show, with one outcome per runout.

