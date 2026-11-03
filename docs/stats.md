# `poker_tracker` Statistics Reference

Living definitions of every derived statistic. Introduced for v3.7 (`docs/SPEC_v3.7.md`). The
stat list starts from the PokerTracker 3 statistical reference (names and formulas kept so that
results can later be compared with PT3/PT4); VPIP and PFR use PokerTracker 4's denominators
(§5). Every definition below is restated in terms of the canonical model so it can be computed
identically for any site.

**This file is authoritative.** Code (`src/stats/`) and the independent oracle
(`tools/oracles/v3_7_oracle.py`) implement it. If they disagree with this file, they have a bug.
A change to any definition here bumps `STATS_VERSION` (§1.3) in the same commit.

---

## 1. Model

### 1.1 Counters and stats

- A **counter** is a non-negative integer computed for one player in one hand (a *player-hand*)
  from the `CanonicalHand` alone. Most counters are flags (`0`/`1`); action counts can be
  larger. Counters are stored in the derived table `hand_player_stats` (§1.3).
- A **stat** is a ratio of two counter sums over a set of player-hands:
  `stat = Σ numerator / Σ denominator`. Percent stats are multiplied by 100. Counter names that
  end in `_opp` or `_faced` are denominators ("opportunities"). One denominator is a difference
  of two sums: VPIP's `Σ hands − Σ p_walk` (§5).
- A stat whose denominator sum is `0` has no value (an empty CSV cell), never `0`.

### 1.2 Arithmetic and display

- No floating point anywhere (`AGENTS.md` invariant 1). Counters are integers; money is integer
  micro-units; equities are exact fractions of integers.
- Ratios are computed with `rust_decimal::Decimal` from integer sums, never in SQL. Counters and
  money are read only from `INTEGER` columns (`SUM(...)` of integers; never a `REAL`). Text
  columns (currency, table, timestamp) are read as text.
- **Display rounding** (reports only, never stored): percent stats, `bb/100`, `$/100`, `bb_won`
  and aggregation factors are rounded **half-even to 2 decimals** and always printed with 2
  decimals (`53.125 → 53.12`, `-208.1 → -208.10`). Exact money amounts are printed exactly with
  trailing zeros removed (`-25.79`, `14.23115`, `-1`).

### 1.3 The derived cache

- `hand_player_stats` holds one row per `hand_players` row: `(hand_id, player_id)`, the
  `position` code (§2, nullable), and one `INTEGER NOT NULL` column per counter of §4, per
  `*_micros` column of §6.1, and per `ev_*` column of §6.2. Figures such as `net`, `bb_won` and
  `bb/100` are computed at report time, not stored.
- It is a **cache**: everything in it is recomputed from the stored canonical hands. It is never
  read by validation, ingestion or duplicate detection, and dropping it loses nothing.
- `STATS_VERSION` (an integer constant in `src/stats/`) identifies the definitions. The table
  `derived_cache(name, version, built_at)` records the version the cache was built with. When
  the row is missing or its version differs, opening the database rebuilds the cache
  (SPEC_v3.7 M13).
- Every column added by a milestone bumps `STATS_VERSION`: 1 = M13, 2 = M14, 3 = M15, 4 = M16.

---

## 2. Positions

Positions use PokerTracker's numbering: the number of seats **counter-clockwise from the button**
among the players dealt in, with the blinds fixed.

| Code | Label (`max_seats ≤ 6`) | Label (`max_seats ≥ 7`) | Player |
| --- | --- | --- | --- |
| 9 | SB | SB | The `PostSmallBlind` poster |
| 8 | BB | BB | The first `PostBigBlind` poster |
| 0 | BTN | BTN | The player in `button_seat` (unless they are the SB) |
| 1 | CO | CO | The next player counter-clockwise from the button |
| 2 | MP | MP | The next one |
| 3 | EP | MP | … |
| 4 | EP | MP | … |
| 5–7 | EP | EP | … |

Rules:

- Codes 0, 1, 2, … are assigned to the players other than the SB and BB, starting with the
  button and moving counter-clockwise (decreasing seat number, wrapping). At 6-max with six
  players: BTN 0, CO 1, MP 2, EP 3 (the first player to act preflop).
- **Heads-up:** the button posts the small blind and is code **9 (SB)**; the other player is
  **8 (BB)**. There is no code 0.
- A player who posts a missed blind (`PostDeadBlind`) or an extra big blind keeps the position of
  their seat; only the first big blind poster is BB.
- A straddler keeps the position of their seat.
- **Standard layout check.** Positions are assigned only when: the button seat is dealt in;
  exactly one small blind is posted; in a hand of 3+ players the SB is the first player clockwise
  after the button and the BB the first player clockwise after the SB; heads-up, the button is the
  SB. Otherwise every player of the hand gets `position = NULL`, and the position-dependent
  counters (steal and steal defence, §4.1) are `0` for the hand. All 1,283 real hands in the
  fixtures have the standard layout.

The label is derived at report time from `position` and the hand's `max_seats`; only the code is
stored.

---

## 3. Terms used by the definitions

- **Decision:** a player's action on a street other than a forced post (`PostSmallBlind`,
  `PostBigBlind`, `PostDeadBlind`, `PostStraddle`) and other than `UncalledBetReturned`: one of
  `Fold`, `Check`, `Call`, `Bet`, `Raise`.
- **State at a decision** (computed by replaying the street up to, not including, the decision):
  - `raises`: number of `Raise` actions earlier on this street (forced posts are not raises; on
    preflop the first `Raise` is the open raise, the second a 3-bet, the third a 4-bet);
  - `calls`: number of `Call` actions earlier on this street;
  - `bets`: number of `Bet` actions earlier on this street (postflop only);
  - `to_call`: highest street commitment minus the player's own street commitment;
  - `remaining`: starting stack minus everything the player has put in so far (gross);
  - `can_raise`: `remaining > to_call` and at least one other player is neither folded nor all-in;
  - `can_bet`: `to_call == 0`, `remaining > 0` and at least one other player is neither folded
    nor all-in.
- **Unopened:** `raises == 0` and `calls == 0` at the decision (only forced posts so far).
- **Money in the pot:** the player made any forced post in this hand (blinds, a missed blind or a
  straddle).
- **Walk:** every other player folded preflop and the player made no decision in the hand. This
  is the big blind when everyone folds to them, and equally a straddler to whom everyone folds
  (5 such hands in the corpus); in every walk the player wins the pot without acting.
- **Preflop aggressor:** the player who made the last preflop `Raise`; none if nobody raised.
- **Steal attempt:** the first preflop `Raise` of the hand, if it was made at a steal
  opportunity (`p_steal_opp`, §4.1).
- **Live at the end:** the player never folded in this hand.
- **Street dealt:** the flop (turn, river) is dealt when at least one board has a flop (turn,
  river).
- **Saw a street:** the street was dealt and the player had not folded on an earlier street.
- **Went to showdown:** the player is live at the end and at least two players are live at the
  end (this includes all-in hands that were run out before the river).

Opportunity rules below say "the player's first decision with …" (`p_pfr_opp` alone says "at
least one decision"). Each opportunity is counted at most once per player-hand. The approximation is deliberate: an opportunity does not check whether
an earlier all-in for less than a full raise reopened the betting.

---

## 4. Counters

### 4.1 Preflop

| Counter | Milestone | Definition |
| --- | --- | --- |
| `hands` | M13 | Always `1`. |
| `p_walk` | M13 | The player had a walk (§3). PT4's `cnt_walks`. |
| `p_vpip` | M13 | The player made at least one preflop `Call` or `Raise`. Forced posts and checks never count. Never `1` in a walk. |
| `p_pfr_opp` | M13 | The player has at least one preflop decision at which `can_raise` holds (PT4's `cnt_pfr_opp`: a preflop raise was possible). It is `0` in a walk (no decision); when every preflop decision faces a bet that takes all of the player's remaining stack (they can only call all-in or fold); and when every other player has folded or is all-in at each of the player's decisions. |
| `p_pfr` | M13 | The player made at least one preflop `Raise`. A legal raise needs `can_raise`, so `p_pfr = 1` implies `p_pfr_opp = 1`. |
| `p_rfi_opp` | M14 | The player's first preflop decision is unopened and `can_raise`. |
| `p_rfi` | M14 | `p_rfi_opp` and that decision is a `Raise`. |
| `p_3bet_opp` | M14 | The player's first preflop decision with `raises == 1` is `can_raise`. |
| `p_3bet` | M14 | `p_3bet_opp` and that decision is a `Raise`. |
| `p_3bet_faced` | M14 | The player has a preflop decision with `raises == 2` (whatever they did before). |
| `p_3bet_fold` / `p_3bet_call` | M14 | `p_3bet_faced` and the first such decision is a `Fold` / `Call`. |
| `p_4bet_opp` | M14 | The player's first preflop decision with `raises >= 2` is `can_raise`. |
| `p_4bet` | M14 | `p_4bet_opp` and that decision is a `Raise` (4-bets, 5-bets and higher). |
| `p_4bet_faced` | M14 | The player has a preflop decision with `raises >= 3`. |
| `p_4bet_fold` | M14 | The player folds preflop at a decision with `raises >= 3` (so a 4-bet then a fold to the 5-bet counts, and so does a 5-bet then a fold to the 6-bet, as PT3 specifies). |
| `p_4bet_call` | M14 | `p_4bet_faced` and the first decision with `raises >= 3` is a `Call`. |
| `p_cc_opp` | M14 | The player's first preflop decision has `raises >= 1` and the player has no money in the pot. |
| `p_cc` | M14 | `p_cc_opp` and that decision is a `Call` (a cold call). |
| `p_steal_opp` | M14 | `p_rfi_opp`, the player's position is CO (1), BTN (0) or SB (9), and the hand has no `PostStraddle`. |
| `p_steal` | M14 | `p_steal_opp` and that decision is a `Raise`. |
| `p_sb_steal_faced` | M14 | The player is SB (9), there was a steal attempt by another player, and the player's first decision has `raises == 1` and `calls == 0`. |
| `p_sb_steal_fold` / `p_sb_steal_raise` | M14 | `p_sb_steal_faced` and that decision is a `Fold` / `Raise`. |
| `p_bb_steal_faced` | M14 | As `p_sb_steal_faced`, for the BB (8). |
| `p_bb_steal_fold` / `p_bb_steal_raise` | M14 | `p_bb_steal_faced` and that decision is a `Fold` / `Raise`. |

### 4.2 Postflop

| Counter | Milestone | Definition |
| --- | --- | --- |
| `f_saw`, `t_saw`, `r_saw` | M13 | The player saw the flop / turn / river (§3). |
| `f_cbet_opp` | M14 | The player is the preflop aggressor, saw the flop, and their first flop decision has `bets == 0` and `can_bet`. |
| `f_cbet` | M14 | `f_cbet_opp` and that decision is a `Bet` (that bet is the flop c-bet). |
| `f_cbet_faced` | M14 | Another player made a flop c-bet, and this player has a flop decision after it with no `Raise` between the c-bet and that decision. |
| `f_cbet_fold` / `f_cbet_call` / `f_cbet_raise` | M14 | `f_cbet_faced` and the first such decision is a `Fold` / `Call` / `Raise`. |
| `t_cbet_opp`, `t_cbet` | M14 | As `f_cbet_opp` / `f_cbet` on the turn, for the player who made the flop c-bet. |
| `t_cbet_faced`, `t_cbet_fold`, `t_cbet_call`, `t_cbet_raise` | M14 | As the flop counters, facing the turn c-bet. |
| `f_xr_opp` | M14 | The player's first flop decision is a `Check` and they have a later flop decision with `to_call > 0`. |
| `f_xr` | M14 | `f_xr_opp` and the first such later decision is a `Raise` (a check-raise). |

### 4.3 Action counts and outcome

| Counter | Milestone | Definition |
| --- | --- | --- |
| `{s}_bets`, `{s}_raises`, `{s}_calls`, `{s}_checks`, `{s}_folds` for `s` in `p`, `f`, `t`, `r` | M13 | Number of the player's decisions of that type on preflop / flop / turn / river. `p_bets` is always `0`. |
| `sd_went` | M13 | The player went to showdown (§3). |
| `sd_won` | M13 | `sd_went` and the player's payout (`won`) is positive, including a split share smaller than what they put in. |
| `won_hand` | M13 | The player's payout is positive (walks and split pots included). |
| `f_saw_won` | M13 | `f_saw` and `won_hand`. |

---

## 5. Stats (PT3 names)

| Stat (report id) | PT3 name | Formula | Milestone |
| --- | --- | --- | --- |
| `vpip` | VP$IP | `p_vpip / (hands − p_walk)` (PT4: `cnt_vpip / (cnt_hands − cnt_walks)`) | M13 |
| `pfr` | PFR | `p_pfr / p_pfr_opp` (PT4: `cnt_pfr / cnt_pfr_opp`) | M13 |
| `saw_flop` | Saw Flop % | `f_saw / hands` | M13 |
| `wtsd` | WTSD | `sd_went / f_saw` | M13 |
| `wsd` | W$SD | `sd_won / sd_went` | M13 |
| `wwsf` | W$WSF | `f_saw_won / f_saw` | M13 |
| `win` | Win % | `won_hand / hands` | M13 |
| `rfi` | RFI | `p_rfi / p_rfi_opp` | M14 |
| `three_bet_pf` | 3Bet PF | `p_3bet / p_3bet_opp` | M14 |
| `fold_to_three_bet_pf` | Fold to PF 3Bet | `p_3bet_fold / p_3bet_faced` | M14 |
| `call_three_bet_pf` | Call PF 3Bet | `p_3bet_call / p_3bet_faced` | M14 |
| `four_bet_pf` | 4Bet PF | `p_4bet / p_4bet_opp` | M14 |
| `fold_to_four_bet_pf` | Fold to PF 4Bet | `p_4bet_fold / p_4bet_faced` | M14 |
| `call_four_bet_pf` | Call PF 4Bet | `p_4bet_call / p_4bet_faced` | M14 |
| `cold_call_pf` | CCPF | `p_cc / p_cc_opp` | M14 |
| `att_to_steal` | Att To Steal | `p_steal / p_steal_opp` | M14 |
| `fold_sb_to_steal` | Fold SB to Steal | `p_sb_steal_fold / p_sb_steal_faced` | M14 |
| `raise_sb_steal` | Raise SB Steal Att | `p_sb_steal_raise / p_sb_steal_faced` | M14 |
| `fold_bb_to_steal` | Fold BB to Steal | `p_bb_steal_fold / p_bb_steal_faced` | M14 |
| `raise_bb_steal` | Raise BB Steal Att | `p_bb_steal_raise / p_bb_steal_faced` | M14 |
| `cbet_flop` | CBet Flop | `f_cbet / f_cbet_opp` | M14 |
| `fold_to_cbet_flop` | Fold to F CBet | `f_cbet_fold / f_cbet_faced` | M14 |
| `call_cbet_flop` | Call F CBet | `f_cbet_call / f_cbet_faced` | M14 |
| `raise_cbet_flop` | Raise F CBet | `f_cbet_raise / f_cbet_faced` | M14 |
| `cbet_turn` | CBet Turn | `t_cbet / t_cbet_opp` | M14 |
| `fold_to_cbet_turn` | Fold to T CBet | `t_cbet_fold / t_cbet_faced` | M14 |
| `call_cbet_turn` | Call T CBet | `t_cbet_call / t_cbet_faced` | M14 |
| `raise_cbet_turn` | Raise T CBet | `t_cbet_raise / t_cbet_faced` | M14 |
| `check_raise_flop` | Check Raise Flop | `f_xr / f_xr_opp` | M14 |
| `af_pf`, `af_flop`, `af_turn`, `af_river` | [Street] AF | `(bets + raises) / calls` on that street (a factor, not a percent) | M13 |
| `af_total` | Total AF | The same over flop, turn and river together | M13 |
| `afq_pf`, `afq_flop`, `afq_turn`, `afq_river` | [Street] AFq | `(bets + raises) / (bets + raises + calls + folds)` on that street | M13 |
| `afq_total` | Total AFq | The same over flop, turn and river together | M13 |

Every stat above is a percent except the `af_*` factors. The `stats` report lists them in this
table's order. **VP$IP SB** is `vpip` in the positions
report's SB row.

---

## 6. Money (M15) and all-in equity (M16)

All money is per player-hand in integer micro-units of the hand's currency, and is aggregated
only within one currency.

### 6.1 Results (M15)

| Column / figure | Definition |
| --- | --- |
| Amount Won (`net`) | `hand_players.net_micros` (`won − invested + adjustments`), unchanged. |
| `bb_won` | `net / big_blind` of the same hand, summed over hands: compute `Σ net / big_blind` per big-blind value with `Decimal`, then add the groups. Big blinds, as PT4 uses for No-Limit (**not** PT3's "big bets" = 2 big blinds). |
| `bb/100` | `Σ bb_won / hands × 100`. |
| `$/100` (`net_per_100`) | `Σ net / hands × 100`. |
| `blinds_posted_micros` | Sum of the player's `PostSmallBlind`, `PostBigBlind` and `PostDeadBlind` amounts (PT3 Amount Blind). |
| `straddles_posted_micros` | Sum of the player's `PostStraddle` amounts. |
| `rake_attributed_micros` | The hand's `Rake` deductions split among all players in proportion to `invested` (the "contributed" method), by the largest-remainder rule below. |
| `fees_attributed_micros` | One split, by the same rule, of the **sum** of all the hand's other deductions (`Jackpot` and `Other`: GGPoker's Jackpot, Bingo, Fortune, Tax). |

**Largest-remainder rule.** To split an integer total `T` (micro-units) by weights `w_i`
(`invested_i`, summing to `W = total_pot`): each player gets `floor(T × w_i / W)`; the leftover
micro-units go one each to the players with the largest remainders `(T × w_i) mod W`, ties to the
lower seat number. The parts always sum to exactly `T`. Use `i128` for the products. If `W` or
`T` is `0`, every part is `0`.

**Sessions.** A table session is a maximal run of the hero's hands at the same `(platform,
currency, table_name)` (a *table*) in which consecutive hands start at most **30 minutes** apart
(`SESSION_GAP`, inclusive). Sessions are computed at report time, not cached, from the hands the
report selects.

- **Time basis.** All of a table's selected hands use one basis. It is **UTC** when every one of
  them states a UTC offset (`utc_offset_seconds`; a hand's UTC time is its timestamp minus its
  offset), so a session across a daylight-saving change is measured correctly. Otherwise it is
  the timestamps **as printed** (GGPoker states no offset). With one basis per table, no gap or
  duration is negative.
- **Order and elapsed time.** A table's hands are ordered by their time on the table's basis,
  then by site hand id. The elapsed time between two hands is the difference of their times on
  that basis, in whole seconds. The gap rule and the duration both use it.
- **Duration** (`duration_seconds`) is the elapsed time from the session's first hand to its
  last hand. Hand histories record when a hand starts, not when it ends, so the duration
  understates the time at the table by about one hand (30–40 s at GGPoker 6-max); a one-hand
  session lasts `0`. The definition uses only start timestamps, which every site provides.

### 6.2 All-in equity (M16)

Following PokerTracker 4 ("All-in Equity"): replace the result of an all-in by its expectation at
the moment the money went in.

**A hand qualifies** when all of these hold:

1. at least two players are live at the end;
2. the last action that is not an `UncalledBetReturned` is on the preflop, flop or turn (cards
   were still to come); that street is the **all-in street**;
3. every live player's two hole cards are known;
4. every live player has the same **live contribution** (`invested` minus their `PostDeadBlind`
   amounts): a single pot. Multiway all-ins with side pots are excluded, as PT4 does.

**Equity.** For each live player, the exact share of the pot over every completion of the board
dealt by the all-in street (preflop: no board cards; flop: 3; turn: 4; for multi-run hands, the
first run's cards up to that street). Only the board cards and the **live players'** hole cards
are removed from the deck; a folded player's cards stay in it even when they are known (for
example Hero's), so a hand's equity never depends on what Hero happened to hold. A runout won by
`k` players gives each `1/k`. Equity is the fraction `ev_equity_num / ev_equity_den` in lowest terms
(integers; no floats). Running it twice or three times does not change the expectation, so
multi-run hands use the same rule.

**Pot.** The pot after **all** deductions: `total_pot − Σ deductions` (= Σ payouts). GGPoker's own
EV Cashout offers are 98.6–99.0% of `equity × (pot after all deductions)` on all four real cashout
hands (versus 95–98% after rake alone and 91–93% of the gross pot), so this is the basis GGPoker
itself uses.

**Per player-hand columns:**

| Column | Definition |
| --- | --- |
| `ev_qualified` | `1` if the hand qualifies and the player is live at the end. |
| `ev_equity_num`, `ev_equity_den` | The equity fraction when `ev_qualified`, else `0` and `1`. |
| `ev_net_micros` | When `ev_qualified` and the player did not take EV Cashout: `round_half_even(num × pot_after / den) − invested + adjustments` (one rounding, to micro-units; `i128` products). Otherwise `net_micros`. |

A player who took **EV Cashout** keeps their actual net: GGPoker already insured their result, so
it carries no all-in luck. Their opponents are adjusted normally.

**Report figures:** `ev_net` = `Σ ev_net_micros`; `ev_bb_per_100` uses `ev_net / big_blind`
per hand as for `bb/100`; `allin_hands` = number of player-hands with `ev_qualified = 1`.

---

## 7. Worked examples

Expected non-zero counters, from real fixtures. Tests assert these exactly.

**HD140650557** (`SUMMARY_SHOW_TWO_CARDS`, button seat 3): folded to Hero in the SB, who opens;
the BB calls; Hero c-bets the flop and calls a raise; Hero checks and calls the turn, then
checks and folds the river.

| Player | Position | Non-zero counters |
| --- | --- | --- |
| 6570ba77 | CO | `hands`, `p_pfr_opp`, `p_folds`, `p_rfi_opp`, `p_steal_opp` |
| 4af6a555 | BTN | `hands`, `p_pfr_opp`, `p_folds`, `p_rfi_opp`, `p_steal_opp` |
| Hero | SB | `hands`, `p_vpip`, `p_pfr_opp`, `p_pfr`, `p_raises`, `p_rfi_opp`, `p_rfi`, `p_steal_opp`, `p_steal`, `f_saw`, `f_cbet_opp`, `f_cbet`, `f_bets`, `f_calls`, `t_saw`, `t_cbet_opp`, `t_checks`, `t_calls`, `r_saw`, `r_checks`, `r_folds` |
| 2d5ce8a9 | BB | `hands`, `p_vpip`, `p_pfr_opp`, `p_calls`, `p_3bet_opp`, `p_bb_steal_faced`, `f_saw`, `f_cbet_faced`, `f_cbet_raise`, `f_raises`, `t_saw`, `t_bets`, `r_saw`, `r_bets`, `won_hand`, `f_saw_won` |

Rake $0.24 and jackpot $0.10 split equally between Hero and the BB (each invested $2.50):
`rake_attributed_micros` 120,000 and `fees_attributed_micros` 50,000 each.

**HD145385563** (`STRADDLE_UTG_OPTION`): UTG straddles; the next player limps; everyone else
folds; the straddler checks.

| Player | Position | Non-zero counters |
| --- | --- | --- |
| 494b476f | SB | `hands`, `p_pfr_opp`, `p_folds` |
| c89b4da3 | BB | `hands`, `p_pfr_opp`, `p_folds` |
| 5daacec9 (straddler) | EP | `hands`, `p_pfr_opp`, `p_checks`, `f_saw`, `f_bets`, `f_folds` |
| adbf678c | MP | `hands`, `p_vpip`, `p_pfr_opp`, `p_calls`, `p_rfi_opp`, `f_saw`, `f_raises`, `won_hand`, `f_saw_won` |
| Hero | CO | `hands`, `p_pfr_opp`, `p_folds` |
| 4e268376 | BTN | `hands`, `p_pfr_opp`, `p_folds` |

No steal opportunities (the hand has a straddle), no c-bet (nobody raised preflop), and Hero has
no RFI opportunity (a limper is in). Every player could have raised, so every player has
`p_pfr_opp`. Rake $0.04 by largest remainder: SB 2,151, BB 4,301,
straddler 16,774, adbf678c 16,774 micro-units (the SB's remainder is the largest).

**HD140650645** (first hand of `SAMPLE_SESSION`): folded to the SB, who completes; the BB checks;
checked down to showdown.

| Player | Position | Non-zero counters |
| --- | --- | --- |
| 4af6a555 | BB | `hands`, `p_pfr_opp`, `p_checks`, `f_saw`, `f_checks`, `t_saw`, `t_checks`, `r_saw`, `r_checks`, `sd_went` |
| Hero | BTN | `hands`, `p_pfr_opp`, `p_folds`, `p_rfi_opp`, `p_steal_opp` |
| 2d5ce8a9 | SB | `hands`, `p_vpip`, `p_pfr_opp`, `p_calls`, `p_rfi_opp`, `p_steal_opp`, `f_saw`, `f_checks`, `t_saw`, `t_checks`, `r_saw`, `r_checks`, `sd_went`, `sd_won`, `won_hand`, `f_saw_won` |

---

## 8. PT3 stats not in v3.7

Planned for later milestones (their counters follow the same patterns):

- **Preflop:** Called PFR; LWPC; Att To Steal & Fold; Steal Success; Steal WTSD / W SD / W w/o SD;
  Fold Steal to Blind Reraise; Fold BB to SB Steal; SB/BB Steal Def WTSD / W SD / W w/o SD;
  Raise PF 3Bet / 4Bet as separate stats.
- **Postflop:** CBet River and the river c-bet responses; Bet / Call / Fold / Raise facing
  F/T/R bets; Call / Fold to F/T/R Raise; Raise F/T/R Total; Check Raise Turn / River / Total;
  Donk and Float (turn/river) with their responses; 3Bet / 4Bet on F/T/R and their responses;
  Fold F/T/R (folded when saw street); WSDWRT/R.
- **Results:** MGR (rake per player dealt in); Without Blind (`net + blinds + straddles`).
- **Dimensions** (report grouping rather than stats): HoleCards, Final Hand, Limit (stakes is the
  results report's grouping already), Site.

Not applicable: Blinds (tournaments), Buddy, Player Icon, Site Icon (PT3 interface features).
