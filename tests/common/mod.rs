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

/// Heads-up preflop fold. (HD145412494)
pub const HU_PREFLOP_FOLD: &str = r#"Poker Hand #HD145412494: Hold'em No Limit ($0.05/$0.1) - 2026/09/25 02:35:58
Table 'NLHRed3' 6-max Seat #3 is the button
Seat 3: Hero ($13.57 in chips)
Seat 6: 445e6868 ($11.96 in chips)
Hero: posts small blind $0.05
445e6868: posts big blind $0.1
*** HOLE CARDS ***
Dealt to Hero [2d 7s]
Dealt to 445e6868 
Hero: folds
Uncalled bet ($0.05) returned to 445e6868
*** SHOWDOWN ***
445e6868 collected $0.1 from pot
*** SUMMARY ***
Total pot $0.1 | Rake $0 | Jackpot $0 | Bingo $0 | Fortune $0 | Tax $0
Seat 3: Hero (small blind) folded before Flop
Seat 6: 445e6868 (big blind) collected ($0.1)"#;

/// Heads-up single-board showdown. (HD145412514)
pub const HU_SHOWDOWN: &str = r#"Poker Hand #HD145412514: Hold'em No Limit ($0.05/$0.1) - 2026/09/25 02:36:41
Table 'NLHRed3' 6-max Seat #6 is the button
Seat 3: Hero ($13.8 in chips)
Seat 6: 445e6868 ($11.71 in chips)
445e6868: posts small blind $0.05
Hero: posts big blind $0.1
*** HOLE CARDS ***
Dealt to Hero [2d Jh]
Dealt to 445e6868 
445e6868: calls $0.05
Hero: checks
*** FLOP *** [2s 9d As]
Hero: checks
445e6868: checks
*** TURN *** [2s 9d As] [4h]
Hero: bets $0.1
445e6868: calls $0.1
*** RIVER *** [2s 9d As 4h] [5c]
Hero: checks
445e6868: checks
445e6868: shows [4c Tc] (a pair of Fours)
Hero: shows [2d Jh] (a pair of Twos)
*** SHOWDOWN ***
445e6868 collected $0.38 from pot
*** SUMMARY ***
Total pot $0.4 | Rake $0.02 | Jackpot $0 | Bingo $0 | Fortune $0 | Tax $0
Board [2s 9d As 4h 5c]
Seat 3: Hero (big blind) showed [2d Jh] and lost with a pair of Twos
Seat 6: 445e6868 (small blind) showed [4c Tc] and won ($0.38) with a pair of Fours"#;

/// Heads-up flop action ending in fold. (HD140650285)
pub const HU_FLOP: &str = r#"Poker Hand #HD140650285: Hold'em No Limit ($0.05/$0.1) - 2026/08/21 16:55:45
Table 'NLHRed14' 6-max Seat #1 is the button
Seat 1: 9f78ead0 ($2 in chips)
Seat 4: Hero ($10.52 in chips)
9f78ead0: posts small blind $0.05
Hero: posts big blind $0.1
*** HOLE CARDS ***
Dealt to 9f78ead0 
Dealt to Hero [8c Th]
9f78ead0: calls $0.05
Hero: checks
*** FLOP *** [2d 8d 9s]
Hero: checks
9f78ead0: bets $0.1
Hero: raises $0.3 to $0.4
9f78ead0: folds
Uncalled bet ($0.3) returned to Hero
*** SHOWDOWN ***
Hero collected $0.38 from pot
*** SUMMARY ***
Total pot $0.4 | Rake $0.02 | Jackpot $0 | Bingo $0 | Fortune $0 | Tax $0
Board [2d 8d 9s]
Seat 1: 9f78ead0 (small blind) folded on the Flop
Seat 4: Hero (big blind) won ($0.38)"#;
