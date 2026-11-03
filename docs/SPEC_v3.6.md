# poker_tracker — v3.6 implementation spec (§8 limitations)

Nov 2, 2026 · @Lucas

## 0. Agent brief

This spec moves `poker_tracker` from blueprint v3.5 to v3.6. It closes four §8 limitations using real GGPoker data: heads-up hands, uncontested winners who show cards in the summary, straddles, and the EV Cashout receiving side. All five v3.5 milestones are already implemented. The new work is milestones M6–M11 below.

**How to work through it:**

1. Read `docs/BLUEPRINT.md` (sections 0–8 only), especially §3 (format-coverage policy), §6 (invariants) and §8 (known limitations). This spec names functions and regexes from it.
2. Do the milestones in order, M6 to M11. Each one says what to change, in which files, and which tests to add, then ends with a **Done when** checklist. Do not start a milestone until the previous one's checklist is fully ticked and committed.
3. Every product decision is already made (see **Decisions (final)** at the end). Do not reopen them. If the code disagrees with this spec, stop and report the difference.
4. Do not change behaviour that a milestone does not name. In particular, keep every v3.5 refusal not listed here.

**Expected result:** all 989 hands in the 10 new exports, plus HD145319803, import with 0 quarantined. Every existing test and mutation still passes.

## 1. Ground rules and gates

**Source of truth.** From M6 on, the repository code and this spec (`docs/SPEC_v3.6.md`) are the source of truth, not `docs/BLUEPRINT.md`. `tools/blueprint check` will report drift after the first change; that is expected, so **do not run `tools/blueprint check` or `extract` in M6–M11**. `docs/BLUEPRINT.md` stays frozen at v3.5 as history. M11 moves its living sections into `docs/`.

**Rules carried over from the blueprint (§3, §2):**

- Support a line shape only if it was seen in a real export. Anything else stays an error (`UnrecognizedLine`, `UnsupportedAdjustment`, …) and is quarantined, never skipped.
- Order for every new shape: (1) add the real hand verbatim to `tests/common/mod.rs`; (2) write a failing test that asserts the parsed result; (3) change the parser or validator; (4) add refusal tests for near-miss edits.
- Site-neutral rules go in `validate_hand` (`src/parser/base.rs`). GGPoker text rules go in `src/parser/ggpoker.rs`.
- Money is `Decimal` in memory and micro-units in SQLite, with no floats.
- A new enum variant must be added to its `stored_identifier!` list (and the variant count in `stored_identifiers_round_trip_and_unknown_ones_are_rejected` in `src/domain/canonical.rs` updated accordingly). Kind columns are free text, so no migration is needed. Never edit a released migration.

**Gates for every milestone** (all must pass before ticking **Done when**):

```
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

- The test count only goes up (M6–M10) and stays equal in M11 (docs-only). v3.5 ends at 104 tests. Record the new count at the end of each milestone in the commit message and report.
- If the repository has the single-line mutation sweep from blueprint §0 (delete, duplicate, swap, ±0.01, rename), run it over the new fixtures. Report survivors that change a result. None are allowed.

## 2. Data and fixtures

The new data is 10 GGPoker exports (989 unique hands, $0.05/$0.10 6-max) plus one pasted hand, HD145319803. An independent check found chip conservation, summary arithmetic and stack continuity hold on every hand. The only body line v3.5 has never seen is `straddle`. v3.5 would quarantine 59 of the 989 hands, and no hand is blocked for more than one reason.

**Copy the exports byte-for-byte** into `tests/fixtures/ggpoker/v3_6/`, named `<YYYYMMDD-HHMM>_<table>.txt` (for example `20260821-2054_NLHRed14.txt`). Then verify them with `sha256sum -c` (or `shasum -a 256`) against:

```
befb8e27ceaa028d93c17732ac84541c1a9fe5f411831c5da499114ce2d56f86  20260821-2054_NLHRed14.txt
4c28f0b7035e819ad99c271f9c19f3cfb5cbd06bfac07f1d8f1a926ef2b7478b  20260821-2054_NLHRed18.txt
e9125ba5a5712af971df86c2b71f635176eafa36397bdbe775434128d821b053  20260821-2106_NLHRed4.txt
b06d5914fb879555b4ec002e57a7e7ba33c53cbede7bcc8d29026ff3c9519336  20260821-2207_NLHRed10.txt
3cf349c3694e04582d23362bf132fc7809fa8018261d83dce1c902e6853db3f9  20260821-2247_NLHRed14.txt
81302080e47308953b5357cf0f6367a9fc02f5aac424b5d9d7472d6ffd689d31  20260821-2252_NLHRed19.txt
8ffe6652bb90e707cfd915459f334ee46153d8f0e63c6f84afa5e11510448660  20260821-2254_NLHRed7.txt
9f8de2fac1d3e421f2635d96523ce6a6d58b7c188dde037b0371a4d29a8e1bb1  20260925-0500_NLHRed21.txt
0a0cb40fbd4c49e8d5ef94cfe3153612c58106eeb1462835dae1f3ef46f32d63  20260925-0539_NLHRed3.txt
cce897605f1bb5f8fca5a834ba085a64e1a3951248d5282d716c8f47aef13c0b  20260925-0618_NLHRed15.txt
```

| File | Bytes | Hands | M6 heads-up | M7 summary shows | M8 straddle | Blocked in v3.5 | Hero net ($) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 20260821-2054_NLHRed14 | 20,657 | 24 | 7 | 1 | 0 | 8 | +7.75 |
| 20260821-2054_NLHRed18 | 204,951 | 167 | 0 | 3 | 0 | 3 | −17.71 |
| 20260821-2106_NLHRed4 | 202,035 | 182 | 1 | 4 | 0 | 5 | −3.03 |
| 20260821-2207_NLHRed10 | 96,525 | 87 | 4 | 0 | 0 | 4 | +3.78 |
| 20260821-2247_NLHRed14 | 105,665 | 90 | 0 | 5 | 0 | 5 | −1.00 |
| 20260821-2252_NLHRed19 | 119,476 | 102 | 0 | 1 | 0 | 1 | +9.41 |
| 20260821-2254_NLHRed7 | 97,832 | 84 | 0 | 1 | 0 | 1 | −17.48 |
| 20260925-0500_NLHRed21 | 114,214 | 99 | 0 | 2 | 0 | 2 | −3.25 |
| 20260925-0539_NLHRed3 | 101,613 | 100 | 14 | 1 | 0 | 15 | +3.45 |
| 20260925-0618_NLHRed15 | 67,774 | 54 | 0 | 1 | 14 | 15 | −7.71 |
| **Total** | **1,130,742** | **989** | **26** | **19** | **14** | **59** | **−25.79** |

Hero net = payouts − contributions + uncalled returns − cashout risk paid, summed over every hand in the file. It was computed outside the Rust code, so it is an independent check.

**Extracting a single-hand fixture.** Hands are separated by blank lines. Copy the block from `Poker Hand #<ID>:` to the line before the next blank line. Paste it unchanged as a `pub const` raw string in `tests/common/mod.rs`, with LF endings and the same trailing-newline convention as the existing constants (preserving trailing spaces on opponent `Dealt to ` lines). Name it after the feature, for example `HU_SHOWDOWN`. Put the hand id in a doc comment, as `EV_CASHOUT` does.

## 3. M6 — Heads-up summary labels

**Goal:** the 26 heads-up hands import. Only the summary label check refuses them today; the betting replay already handles heads-up.

**Observed format** (26/26 hands): the button posts the small blind, and its summary line carries `(small blind)` but not `(button)`. The button/SB acts first preflop and the BB acts first postflop. Example, with the button on seat 6:

```
Table 'NLHRed3' 6-max Seat #6 is the button
Seat 3: Hero ($13.8 in chips)
Seat 6: 445e6868 ($11.71 in chips)
445e6868: posts small blind $0.05
Hero: posts big blind $0.1
…
Seat 3: Hero (big blind) showed [2d Jh] and lost with a pair of Twos
Seat 6: 445e6868 (small blind) showed [4c Tc] and won ($0.38) with a pair of Fours
```

**Tasks:**

1. **M6.1 Fixtures.** Add verbatim to `tests/common/mod.rs`: `HU_PREFLOP_FOLD` = HD145412494 and `HU_SHOWDOWN` = HD145412514 (both `20260925-0539_NLHRed3.txt`), and `HU_FLOP` = HD140650285 (`20260821-2054_NLHRed14.txt`).
2. **M6.2 Failing tests** in `tests/ggpoker_parser.rs`: each fixture parses. Assert the button seat, the action order (button/SB first preflop, BB first postflop) and each player's net.
3. **M6.3 Change** `check_summary` in `src/parser/ggpoker.rs`. Narrow rule (decided): drop the `(button)` label only in a two-player hand where the button posted the small blind.

```rust
let heads_up = self.players.len() == 2;
let expect_button = p.seat == header.button_seat && !(heads_up && posted_small);
if seat_line.button != expect_button
    || seat_line.small_blind != posted_small
    || seat_line.big_blind != (first_big_blind == Some(p.name.as_str()))
```

4. **M6.4 Refusal tests:** each edit of `HU_SHOWDOWN` must fail with `SummaryMismatch`:
   - add `(button)` to seat 6's summary line;
   - swap `(small blind)` and `(big blind)`;
   - change the table line to `Seat #3 is the button`.
5. **M6.5 Ring regression:** a ring-game hand whose button line has `(button)` removed is still refused.

**Done when:**

- [ ] M6.1–M6.5 merged; gates green.
- [ ] All 26 heads-up hands in the 4 affected files parse. Run a scratch import of `20260925-0539_NLHRed3.txt`: quarantined = 1, the M7 hand HD145412146 (clean up any scratch sqlite file afterwards).

## 4. M7 — Uncontested winners who show in the summary

**Goal:** the 19 hands where an uncontested winner shows one or two cards import. In most of them the cards appear only on the summary line. Today `check_summary` requires `shown.len() == 2 && shown == known`, so the hand is refused.

**Observed shapes** (none has a `with …` description, and all are single-board):

| Shape | Hands | Why v3.5 refuses | Fixture to add |
| --- | --- | --- | --- |
| `showed [c] and won ($x)` (one card) | 9: 6 opponents with no body `shows` line, 2 Hero, 1 opponent with a body `shows [6s]` | Needs 2 cards | `SUMMARY_SHOW_ONE_CARD` = HD140656328 (`20260821-2247_NLHRed14.txt`); `HERO_SUMMARY_SHOW_ONE_CARD` = HD140647731 (`20260821-2254_NLHRed7.txt`); `BODY_AND_SUMMARY_ONE_CARD` = HD140646055 (`20260821-2054_NLHRed18.txt`) |
| `showed [c1 c2] and won ($x)`, opponent, no body `shows` line | 8 | Opponent's known cards are empty | `SUMMARY_SHOW_TWO_CARDS` = HD140650557 (`20260821-2054_NLHRed14.txt`) |
| `showed [c1 c2] and collected ($x)`, no flop | 2 | `collected` is not an outcome | `SUMMARY_SHOW_COLLECTED` = HD140652448 (`20260821-2106_NLHRed4.txt`) |

Other affected hands include HD140643529, HD140654126, HD140654960, HD140643699 (Hero), HD140654247, HD140652240, HD140655064 and HD145412146 (`collected`). Hero two-card summary-only shows (HD140645051, HD145383504) already pass and must keep passing.

**Tasks:**

1. **M7.1 Fixtures and failing tests** for the five constants above. Assert the shown cards are stored as the player's known hole cards: one slot for a one-card show, two slots for a two-card show. Assert each player's net too.
2. **M7.2 Grammar.** Add a new pattern, tried *after* `RE_SEAT_SHOWED` with `RE_SEAT_RUN_OUTCOME` has failed. It produces a new `SeatResult::ShowedUncontested { cards: Vec<Card>, verb: PayoutVerb }` with `PayoutVerb::{Won, Collected}`. Leave `RE_SEAT_RUN_OUTCOME` unchanged.

```
static RE_SEAT_SHOWED_UNCONTESTED = r"^ showed \[(?P<cards>[^\]]+)\] and (?P<verb>won|collected) \(\$(?P<amt>AMT)\)$"
```

3. **M7.3 Known cards.** Before the cross-checks in `check_summary`, call the existing `record_shown_cards(name, cards)` (or record each `Card` on the player) for each `ShowedUncontested` seat. This gives the same conflict handling as body shows: a card on the board, in Hero's hand or in another show is `ConflictingHoleCards` (or `DuplicateCard` in `validate_hand`). For Hero, every summary card must already be one of Hero's dealt cards (`record_known_card` returns `false` if a third distinct card is recorded, producing `ConflictingHoleCards`).
4. **M7.4 Acceptance rule** in `check_summary`. Keep the `SeatResult::Showed` rule (contested or uncontested two-card show with valid run outcomes, `shown.len() == 2 && shown == known && *outcomes == runs`) as it is. Accept `ShowedUncontested` only if:

```
folded_on.is_none() && !contested && runs == 1
    && (1..=2).contains(&cards.len())
    && cards.iter().all(|c| p.known_cards().any(|k| k == *c))
    && (verb == PayoutVerb::Won) == flop_dealt
```
(where `flop_dealt = self.boards.iter().any(|b| b.flop.is_some())`, and `cards` itself contains no duplicate card when `cards.len() == 2`).

5. **M7.5 `won` vs `collected` on plain lines** (decided: yes). `RE_SEAT_WON` keeps matching both verbs. Capture `(?P<verb>won|collected)` into `SeatResult::Won { verb: PayoutVerb }`, and in `check_summary` require `(verb == PayoutVerb::Won) == flop_dealt` alongside `folded_on.is_none() && !contested`. This held on 801/801 plain lines and on every `showed` line.
6. **M7.6 Refusal tests:** each edit must be refused:
   - a summary card that is on the board → `ConflictingHoleCards` or `DuplicateCard`;
   - a Hero summary card Hero was not dealt → `ConflictingHoleCards`;
   - ` with a pair of Aces` appended to a one-card summary show → `InvalidCardSyntax` or `UnrecognizedLine`;
   - `collected` in a hand with a flop, or `won` in a hand without one, on both a showed line and a plain line → `SummaryMismatch`;
   - three cards → refused (`InvalidCardSyntax`);
   - the uncontested form (`ShowedUncontested`) on a player in a contested showdown → `SummaryMismatch`.

No domain, storage or schema change. Do not add a provenance column (decided).

**Done when:**

- [ ] M7.1–M7.6 merged; gates green.
- [ ] All 19 hands parse, and every contested showdown in the 10 exports parses as before.

## 5. M8 — Straddles

**Goal:** the 14 straddle hands in `20260925-0618_NLHRed15.txt` import with exact nets. Straddles become a site-neutral forced post that the betting replay understands.

**Observed format** (all 14 hands):

- **Line:** `<name>: straddle $X`. It comes after the blind posts and before `*** HOLE CARDS ***`. There is no all-in suffix and no summary label.
- **X is the poster's total commitment** after the straddle ("to" semantics). An SB straddling to 0.2 adds 0.15 (HD145385586); a BB straddling to 0.2 adds 0.1 (HD145385508). Chip conservation holds on all 14 hands only under this reading.
- **Size:** each straddle is exactly 2× the highest commitment before it: 0.1 → 0.2, and a re-straddle 0.2 → 0.4 (HD145385574). At most 2 straddles per hand have been observed; 3 or more straddles must be refused (§9).
- **Who:** any seat can straddle, including the SB (3 hands), the BB (3), UTG (6) and the button (2). A player can post a blind *and* straddle.
- **Order:** preflop action starts with the seat left of the *last* straddler (14/14). The straddler acts last and has an option: they check in HD145385563, HD145385539 and HD145385473, and raise in HD145385465.
- **Labels:** `(big blind)` stays on the first big-blind poster. A straddler is never `(didn't bet)`.
- **Uncalled bet:** the unmatched part of a straddle is returned as usual, for example `Uncalled bet ($0.15) returned to 5daacec9` in HD145385508.

Example (HD145385574, double straddle):

```
c89b4da3: posts small blind $0.05
5daacec9: posts big blind $0.1
5daacec9: straddle $0.2
494b476f: straddle $0.4
*** HOLE CARDS ***
…
c89b4da3: folds
5daacec9: calls $0.2
adbf678c: calls $0.4
Hero: raises $1.4 to $1.8
```

**Tasks:**

1. **M8.1 Fixtures** from `20260925-0618_NLHRed15.txt` with failing tests that assert every action and every net:
   - `STRADDLE_UTG_OPTION` = HD145385563;
   - `STRADDLE_SB_ALL_IN_POT` = HD145385586 (pot $26.30);
   - `STRADDLE_BB_WALK` = HD145385508 ($0.15 returned);
   - `STRADDLE_BUTTON` = HD145385522 (SB acts first);
   - `DOUBLE_STRADDLE` = HD145385574.
2. **M8.2 Domain** (`src/domain/canonical.rs`):
   - add `ActionType::PostStraddle`, stored as `"PostStraddle"`, and add it to the `stored_identifier!` list;
   - update `stored_identifiers_round_trip_and_unknown_ones_are_rejected` in `src/domain/canonical.rs` so `assert_eq!(round_trip(ActionType::ALL, ActionType::as_str), 10);` (was `9`);
   - `is_contribution()` and `adds_to_street_commitment()` both return `true` for `ActionType::PostStraddle`;
   - `amount` is the incremental chips added by the straddle and `street_total_after` is `X` (note: because `amount` is incremental and `action_type = 'PostStraddle'`, existing SQL chip-conservation queries in `tests/storage.rs` work without change).
3. **M8.3 Parser** (`src/parser/ggpoker.rs`):
   - add `RE_STRADDLE = ^(?P<name>.+?): straddle \$(?P<amt>AMT)$`;
   - accept it only in `SeatingAndBlinds` (before `*** HOLE CARDS ***`), after at least one `PostBigBlind` action, and reject any blind post (`RE_POST`) that appears *after* a `PostStraddle`;
   - reject if 2 `PostStraddle` actions have already been recorded in the hand (3 or more straddles are out of scope per §9);
   - `total_after = X`; `increment = X − current_street_commitment`, which must be `> 0`; furthermore, require `X == 2 * max_street_commitment_before` (so `straddle $0.3` or a short straddle fails immediately in the parser);
   - mark `is_all_in = true` when the poster's cumulative preflop posted amount (`posted_before + increment`) equals `starting_stack` (GGPoker prints no suffix on forced posts).
4. **M8.4 Validator** (`validate_hand` in `src/parser/base.rs`, site-neutral):
   1. Forced posts come first: small blind, big blind(s)/dead blinds, then 0, 1, or 2 straddles (`ActionType::PostStraddle`). No blind may follow a straddle, at most 2 straddles are allowed, and nothing forced may follow a voluntary action.
   2. Each straddle's `street_total_after` must equal `2 * highest_live_commitment_before_it` (or less only when `is_all_in` is true, though the GGPoker parser refuses short all-in straddles per §9).
   3. The preflop `anchor` is the seat of the *last* straddler (so `next_clockwise` starts with the seat left of the last straddler), or `big_blind_seat` if there is no straddle.
   4. A straddle is a forced post (not in `acted`), so the straddler still owes an action when action reaches them, giving them the option by the same mechanism as the big blind.
   5. **Minimum raise** (decided: straddle total). On `Street::Preflop`, initialize `min_raise` to the last straddle's `street_total_after` (or `big_blind` if there is no straddle). Over a `0.2` straddle the minimum raise is to `0.4` (`min_raise = 0.2`); over a `0.4` re-straddle it is to `0.8` (`min_raise = 0.4`). After that, the usual last-full-raise rule applies.
   6. Straddles are live money for side-pot caps (`validate_side_pots`), not dead money.
5. **M8.5 Validator unit tests** in `src/parser/base.rs`, built from canonical actions with no GGPoker text, covering: anchor shift, straddler option, 2× size requirement, maximum of 2 straddles, and minimum raise over a straddle.
6. **M8.6 Refusal tests** in `tests/ggpoker_parser.rs`: each edit of the fixtures must be refused:
   - `straddle $0.3` (not 2×);
   - a third straddle (`straddle $0.8` added after `straddle $0.4` in `DOUBLE_STRADDLE`);
   - the straddle line moved after `*** HOLE CARDS ***`, or before the blinds;
   - first preflop actor changed to the seat left of the big blind;
   - the straddler's final `checks` deleted;
   - `raises $0.1 to $0.3` facing a `0.2` straddle;
   - `straddle $0.2 and is all-in`.

**Stats note** (for the later stats layer, nothing to build now): a straddle is forced, so VPIP and PFR must not count it.

**Done when:**

- [ ] M8.1–M8.6 merged; gates green.
- [ ] A scratch import of `20260925-0618_NLHRed15.txt` gives 54 ingested, 0 quarantined, and Hero net −7.71 (clean up any scratch sqlite file afterwards).

## 6. M9 — EV Cashout, receiving side

**Goal:** a player who takes EV Cashout and then loses the pot is credited the cashout. Today the hand is refused with `UnsupportedAdjustment`. The first real sample is HD145319803 (NLHRed18, 2026-09-24): 81ae8569 cashes out on the flop with Ts Tc, loses to a flush, and receives $6.87.

**Fixture** — add verbatim as `EV_CASHOUT_RECEIVED` in `tests/common/mod.rs`:

```
Poker Hand #HD145319803: Hold'em No Limit ($0.05/$0.1) - 2026/09/24 18:12:47
Table 'NLHRed18' 6-max Seat #4 is the button
Seat 1: a2939c73 ($5 in chips)
Seat 2: f729835e ($10.37 in chips)
Seat 3: Hero ($10.11 in chips)
Seat 4: 81ae8569 ($30.71 in chips)
Seat 5: ab03e00b ($10 in chips)
Seat 6: 27a6d6a ($12.06 in chips)
ab03e00b: posts small blind $0.05
27a6d6a: posts big blind $0.1
*** HOLE CARDS ***
Dealt to a2939c73 
Dealt to f729835e 
Dealt to Hero [7s 9s]
Dealt to 81ae8569 
Dealt to ab03e00b 
Dealt to 27a6d6a 
a2939c73: raises $0.3 to $0.4
f729835e: calls $0.4
Hero: folds
81ae8569: calls $0.4
ab03e00b: folds
27a6d6a: folds
*** FLOP *** [8d 7d 8h]
a2939c73: bets $4.6 and is all-in
f729835e: folds
81ae8569: calls $4.6
a2939c73: shows [2d 3d] (a pair of Eights)
81ae8569: shows [Ts Tc] (two pair, Tens and Eights)
81ae8569: Chooses to EV Cashout
*** TURN *** [8d 7d 8h] [Kd]
*** RIVER *** [8d 7d 8h Kd] [2h]
81ae8569: Receives Cashout ($6.87)
*** SHOWDOWN ***
a2939c73 collected $9.93 from pot
*** SUMMARY ***
Total pot $10.55 | Rake $0.52 | Jackpot $0.1 | Bingo $0 | Fortune $0 | Tax $0
Board [8d 7d 8h Kd 2h]
Seat 1: a2939c73 showed [2d 3d] and won ($9.93) with a flush King high
Seat 2: f729835e folded on the Flop
Seat 3: Hero folded before Flop (didn't bet)
Seat 4: 81ae8569 (button) showed [Ts Tc] and lost with two pair, Tens and Eights, EV Cashout ($6.87)
Seat 5: ab03e00b (small blind) folded before Flop
Seat 6: 27a6d6a (big blind) folded before Flop
```

`Dealt to` lines for opponents end with a trailing space in the export. Keep it.

**What the sample establishes** (checked by hand):

- **Body line:** `<name>: Receives Cashout ($x)`, after the river is dealt and before `*** SHOWDOWN ***`. This is the same place as `Pays Cashout Risk ($x)`.
- **Summary suffix:** `, EV Cashout ($x)` after a `lost` outcome. The pay side uses `, Cashout Risk ($x)` instead. The description before it may contain its one comma (`two pair, Tens and Eights`).
- **Pot accounting is unchanged.** The cashout is paid from outside the pot: contributions 5.00 + 0.40 + 5.00 + 0.05 + 0.10 = total pot $10.55; $10.55 − $0.52 rake − $0.10 jackpot = $9.93 collected.
- **The cashing player need not be all-in.** 81ae8569 called the all-in and covered it. The pay-side HD140653161 is the same.
- **Expected nets:** a2939c73 +4.93; 81ae8569 +1.87 (−5.00 + 6.87); f729835e −0.40; ab03e00b −0.05; 27a6d6a −0.10; Hero 0.00. The nets sum to +6.25, which is the cashout minus pot deductions.

**Tasks:**

1. **M9.1 Fixtures and failing tests:**
   - `EV_CASHOUT_RECEIVED` (above): assert the nets above and one adjustment `{ player_name: "81ae8569", kind: EvCashoutReceived, street: Flop, amount: +6.87 }`;
   - `EV_CASHOUT_FLOP` = HD140647649 (`20260821-2254_NLHRed7.txt`): risk $0.55, chosen on the flop after a preflop all-in;
   - `EV_CASHOUT_CALLER` = HD140653161 (`20260821-2106_NLHRed4.txt`): risk $0.66, chosen on the turn by the player who called the all-in.
2. **M9.2 Domain** (`src/domain/canonical.rs`):
   - add `AdjustmentKind::EvCashoutReceived`, doc comment `"EV Cashout taken, pot lost afterwards: the player is paid the cashout (GGPoker \`Receives Cashout ($x)\`). Always a credit."`;
   - `as_str` returns `"EvCashoutReceived"`, and `is_credit()` returns `Some(true)`;
   - add it to `stored_identifier!(AdjustmentKind, …)` and update `stored_identifiers_round_trip_and_unknown_ones_are_rejected` in `src/domain/canonical.rs` so `assert_eq!(round_trip(AdjustmentKind::ALL, AdjustmentKind::as_str), 2);` (was `1`). No migration is needed.
3. **M9.3 Parser body** (`src/parser/ggpoker.rs`):
   - add `RE_EV_RECEIVES = ^(?P<name>.+?): Receives Cashout \(\$(?P<amt>AMT)\)$`;
   - replace `PendingCashout.risk_paid: Option<Decimal>` with `settlement: Option<CashoutSettlement>`, where `enum CashoutSettlement { RiskPaid(Decimal), Received(Decimal) }`;
   - require both `RE_EV_PAYS` and `RE_EV_RECEIVES` to appear on `Street::River` (after the river is dealt);
   - either line settles the player's unsettled pending cashout. A second settlement, or one with no pending cashout, or one before the river, is `UnsupportedAdjustment`;
   - keep the `line.contains("Cashout")` fallback for any other wording.
4. **M9.4 Parser summary:**
   - add `RE_SEAT_EV_CASHOUT_SUFFIX = ^(?P<body>.+), EV Cashout \(\$(?P<amt>AMT)\)$` next to `RE_SEAT_CASHOUT_SUFFIX`;
   - at most one of the two suffixes per line (if after stripping one suffix `body.contains("Cashout")`, return `UnsupportedAdjustment`);
   - store the amounts in `seat_cashout_received: HashMap<u8, Decimal>` beside `seat_cashout_risk`.
5. **M9.5 `check_summary`:**
   - body `Received(x)` ⇔ summary `EV Cashout ($x)`, with equal amounts (`SummaryMismatch` on disagreement), and body `RiskPaid(x)` ⇔ summary `Cashout Risk ($x)` (`SummaryMismatch` on disagreement);
   - `Received` only for a `Showed` seat whose outcomes are all `lost`, and whose body payout is `0`;
   - `RiskPaid` only for a seat with a body payout `> 0`;
   - a split pot (`self.winners.len() > 1`) or multi-run hand (`multi`) with any cashout is unobserved, so refuse it with `UnsupportedAdjustment`.
6. **M9.6 Adjustments** (the loop after `check_summary`): `Received(x)` becomes `EvCashoutReceived` with amount `+x`; `RiskPaid(x)` becomes `EvCashoutRiskPaid` with amount `-x`. An unsettled pending cashout is still `UnsupportedAdjustment`. Update the message to name both settlement lines.
7. **M9.7 Validator** (`validate_hand` in `src/parser/base.rs`, site-neutral): at most one adjustment per player. `EvCashoutReceived` only for a player who did not fold and has zero payout (`won == 0`). `EvCashoutRiskPaid` only for a player with a positive payout (`won > 0`).
8. **M9.8 Tests to update and add:**
   - in `tests/ggpoker_parser.rs`, `rejects_unknown_or_unsettled_cashout_formats` puts `Receives Cashout ($1.20)` into `EV_CASHOUT` (a hand the cashing player *won*). It must still be refused, now with `SummaryMismatch` (the summary still says `Cashout Risk ($0.48)`). Update that assertion to expect `SummaryMismatch`, add an unknown wording case (`" Takes Cashout ($1.20)\n"` → `UnsupportedAdjustment`), and keep the unsettled case as `UnsupportedAdjustment`;
   - new refusals on `EV_CASHOUT_RECEIVED`: amount differs between body and summary (`SummaryMismatch`); summary suffix changed to `Cashout Risk ($6.87)` (`SummaryMismatch`); `Receives Cashout` line deleted (`UnsupportedAdjustment`); the line moved before the turn (`UnsupportedAdjustment`); the loser's outcome changed to `won ($6.87)` (`SummaryMismatch`);
   - storage round trip in `tests/storage.rs`: `EV_CASHOUT_RECEIVED` stores the adjustment with kind `"EvCashoutReceived"` and `+6_870_000` micros, and `load_hand` reads it back unchanged.

**Done when:**

- [ ] M9.1–M9.8 merged; gates green.
- [ ] Importing `EV_CASHOUT_RECEIVED` gives 81ae8569 a net of +1.87.

## 7. M10 — Regression fixtures and whole-folder test

**Goal:** lock in shapes v3.5 already accepts, using real hands from the new exports, and prove the whole set imports. M6–M9 touch the summary checks and the preflop replay, so these hands guard against regressions.

**M10.1 Single-hand fixtures** (all should pass with no code change; if one fails, stop and report it):

| Constant | Hand | File | Covers |
| --- | --- | --- | --- |
| `RIT_TWICE_TURN` | HD140644794 | `20260821-2054_NLHRed18.txt` | Run it twice from the turn |
| `RIT_TWICE_FLOP_REAL` | HD145411928 | `20260925-0539_NLHRed3.txt` | Run it twice from the flop |
| `FOUR_WAY_ALL_IN` | HD145383901 | `20260925-0500_NLHRed21.txt` | Four-way all-in, one player collects twice |
| `MISSED_AND_BIG_BLIND` | HD140647389 | `20260821-2254_NLHRed7.txt` | One player posts missed blind + big blind |
| `MISSED_AND_BIG_BLIND_2` | HD140645359 | `20260821-2054_NLHRed18.txt` | Same, second sample |
| `MISSED_BLIND_ONLY` | HD145411947 | `20260925-0539_NLHRed3.txt` | Missed blind alone |
| `MISSED_BLIND_ONLY_2` | HD140654659 | `20260821-2252_NLHRed19.txt` | Missed blind alone, second sample |
| `EXTRA_BIG_BLIND` | HD140653739 | `20260821-2207_NLHRed10.txt` | Extra big blind (19 such hands in the set) |
| `EXTRA_BIG_BLIND_2` | HD140653777 | `20260821-2207_NLHRed10.txt` | Extra big blind, second sample |
| `FOLDED_ONE_CARD_SHOW` | HD145383947 | `20260925-0500_NLHRed21.txt` | Folded one-card show plus duplicate shows |
| `HERO_SUMMARY_SHOW_TWO_CARDS` | HD140645051 | `20260821-2054_NLHRed18.txt` | Hero shows both cards in the summary only |
| `HERO_SUMMARY_SHOW_TWO_CARDS_2` | HD145383504 | `20260925-0500_NLHRed21.txt` | Same, second sample |

For each fixture, assert every player's net and the board(s). If an existing constant already covers the same shape, keep both: the new one is a second real sample.

**M10.2 Whole-folder integration test** (`tests/real_session_v3_6.rs`), loading the 10 files from `tests/fixtures/ggpoker/v3_6/`:

1. Assert each file's SHA-256 and byte length (section 2).
2. Each file parses with 0 errors, and the hand count per file matches the section 2 table.
3. Chip conservation holds on every hand, and there are 0 stack warnings.
4. Hero net per file matches the section 2 table exactly (total `dec!(-25.79)`).
5. Import `tests/fixtures/ggpoker/v3_6/` into a temporary SQLite database via `DirectoryIngester`: 10 files processed, 989 hands ingested, 0 failed, 0 quarantined, 0 stack warnings. A second import of the folder gives 0 ingested and 989 duplicates.
6. 2 hands in `20260821-2054_NLHRed14.txt` are also in `SAMPLE_SESSION` (`HD140650645` and `HD140650460`). Verify that importing `SAMPLE_SESSION` after that file counts them as 2 duplicates.

**M10.3 Opponent identity check:** across the 10 files, no opponent hash appears at two distinct files/table sessions (101 unique opponent names excluding `"Hero"`, 101 file appearances). Assert it as a test in `tests/real_session_v3_6.rs`, because §8 relies on it.

**Done when:**

- [ ] M10.1–M10.3 merged; gates green.
- [ ] The CLI import of `tests/fixtures/ggpoker/v3_6/` into a scratch DB exits with code `0` (clean up the scratch DB afterwards).

## 8. M11 — Documentation moves out of the blueprint

**Goal:** the project's living rules live in `docs/`, updated for v3.6. `docs/BLUEPRINT.md` stays as a frozen v3.5 record.

**Tasks:**

1. **M11.1** Create `docs/invariants.md` from `docs/BLUEPRINT.md` §6. Add these rules:
   - **Forced posts:** small blind, big blind(s), dead blinds, then up to two straddles. Each straddle is 2× the highest commitment before it (less only all-in). Preflop starts left of the last straddler, and the first raise must add at least the last straddle's total.
   - **GGPoker labels:** in a heads-up hand the button posts the small blind, and its summary line carries `(small blind)` only.
   - **GGPoker summary:** an uncontested winner may read `showed [1–2 cards] and won|collected ($x)` with no description; the cards must agree with the player's known cards. `won` appears iff a flop was dealt, `collected` iff not, on plain and `showed` lines alike.
   - **EV Cashout:** settled by `Pays Cashout Risk ($x)` (winner, summary `, Cashout Risk ($x)`) or `Receives Cashout ($x)` (loser, summary `, EV Cashout ($x)`). At most one per player, single-run and unsplit pots only.
2. **M11.2** Create `docs/known_limitations.md` from `docs/BLUEPRINT.md` §8 with these edits:
   - **delete** the "Heads-up hands" bullet;
   - **replace** the EV Cashout bullet with: "*EV Cashout* is supported on both sides (`EvCashoutRiskPaid`, `EvCashoutReceived`). A cashout in a split or multi-run pot has not been observed and is quarantined with `UnsupportedAdjustment`.";
   - **edit** "Not yet observed on GGPoker": change "straddles, antes or bomb pots" to "antes, bomb pots, all-in or short straddles, three or more straddles", and keep the other items;
   - **replace** the GGPoker hash sub-bullet under *Player identity* with: "GGPoker opponent hashes are per table session. Across 10 tables (101 hashes) none recurred, even at the same table name on the same day, so opponent-level features cannot span sessions.";
   - **add** under *Derived statistics*: "Straddles are forced posts: exclude them from VPIP and PFR. Heads-up has two positions (BTN/SB, BB)."
3. **M11.3** Add a `v3.6` entry at the top of `docs/CHANGELOG.md`. List M6–M10 with one line each, the new identifiers `PostStraddle` and `EvCashoutReceived`, and the final test count.
4. **M11.4** Add a blockquote note at the very top of `docs/BLUEPRINT.md`:
   `> **Note:** Frozen at v3.5. For v3.6+ specification and current living rules, see \`docs/SPEC_v3.6.md\`, \`docs/invariants.md\`, and \`docs/known_limitations.md\`.`

**Done when:**

- [ ] M11.1–M11.4 merged; gates green.
- [ ] Every rule added in M6–M9 appears in `docs/invariants.md`.

## 9. Out of scope — must stay refused

None of these appears in the new data. Each must still produce an error and be quarantined after v3.6. Do not add speculative support.

- EV Cashout in a split pot or a multi-run hand;
- antes, bomb pots;
- `mucks hand`, seat-change and sit-out lines;
- non-`$` stakes, Omaha, Pot-Limit or Fixed-Limit;
- all-in blinds for less than the stake;
- all-in or short straddles, a straddle with an `and is all-in` suffix, and three or more straddles;
- heads-up hands with a missed or dead blind.

When a real sample of any of these turns up, add it as a new milestone in the same format: fixture first, then the failing test, then the change, then refusals.

## 10. Decisions (final)

| # | Decision | Options | Final |
| --- | --- | --- | --- |
| 1 | Minimum raise after a straddle | Straddle total (standard rule, stricter) or big blind (v3.5 default, more lenient). All 14 samples pass either way. | Straddle total (owner decision) |
| 2 | Heads-up label rule | Narrow (heads-up and button posted SB) or general (any button that posted SB) | Narrow (owner decision) |
| 3 | Require `won` when a flop was dealt and `collected` when none was, on plain summary payout lines too | Yes or no | Yes (Claude's call). It held on 801/801 lines, and a wrong guess only quarantines a hand, which a re-import after a fix recovers |
| 4 | Record whether shown cards came from the body or the summary | Add a provenance column, or don't | No provenance column (owner decision) |

## 11. Errata (v3.6.1)

Found by the audit of `1af1f57`; implemented in M12 (`docs/SPEC_v3.6.1.md`).

1. **M7.2:** `RE_SEAT_SHOWED_UNCONTESTED` is tried *before* `RE_SEAT_SHOWED`. In the original order, `showed [c1 c2] and won ($x)` matches the general shape first and cannot import.
2. **M8.3:** a straddle that uses the poster's whole stack is refused by the GGPoker parser (`IllegalAction`), as §9 requires. The validator still allows it site-neutrally.
3. **§9 also refuses:** a second straddle by the same player, a heads-up straddle, and a heads-up hand where the button does not post the small blind (owner decision, 2026-11-02).
4. **M8.6:** refusal 6 is tested with a consistent edit (the straddler calls; pot and payout +$0.20), so only the minimum-raise rule can refuse it.
5. **Storage:** `SqliteRepository::initialize` reads the database identity in one read transaction and retries only a busy lock.
