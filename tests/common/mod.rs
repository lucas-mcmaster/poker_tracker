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

/// Opponent shows one card in summary after uncontested win. (HD140656328)
pub const SUMMARY_SHOW_ONE_CARD: &str = r#"Poker Hand #HD140656328: Hold'em No Limit ($0.05/$0.1) - 2026/08/21 19:45:04
Table 'NLHRed14' 6-max Seat #3 is the button
Seat 1: Hero ($10.75 in chips)
Seat 2: 76fd4643 ($10.15 in chips)
Seat 3: 9f795b5b ($10.15 in chips)
Seat 4: 24a1a9e0 ($19.72 in chips)
Seat 6: b29fca73 ($20.5 in chips)
24a1a9e0: posts small blind $0.05
b29fca73: posts big blind $0.1
*** HOLE CARDS ***
Dealt to Hero [2s Qs]
Dealt to 76fd4643 
Dealt to 9f795b5b 
Dealt to 24a1a9e0 
Dealt to b29fca73 
Hero: folds
76fd4643: raises $0.1 to $0.2
9f795b5b: raises $0.65 to $0.85
24a1a9e0: folds
b29fca73: folds
76fd4643: calls $0.65
*** FLOP *** [6h 7h Qd]
76fd4643: checks
9f795b5b: bets $1.05
76fd4643: raises $1.98 to $3.03
9f795b5b: folds
Uncalled bet ($1.98) returned to 76fd4643
*** SHOWDOWN ***
76fd4643 collected $3.66 from pot
*** SUMMARY ***
Total pot $3.95 | Rake $0.19 | Jackpot $0.1 | Bingo $0 | Fortune $0 | Tax $0
Board [6h 7h Qd]
Seat 1: Hero folded before Flop (didn't bet)
Seat 2: 76fd4643 showed [2c] and won ($3.66)
Seat 3: 9f795b5b (button) folded on the Flop
Seat 4: 24a1a9e0 (small blind) folded before Flop
Seat 6: b29fca73 (big blind) folded before Flop"#;

/// Hero shows one card in summary after uncontested win. (HD140647731)
pub const HERO_SUMMARY_SHOW_ONE_CARD: &str = r#"Poker Hand #HD140647731: Hold'em No Limit ($0.05/$0.1) - 2026/08/21 19:40:24
Table 'NLHRed7' 6-max Seat #6 is the button
Seat 1: 62688fe9 ($4.65 in chips)
Seat 2: dc7d0eb4 ($10.15 in chips)
Seat 3: 1fba9f17 ($12.73 in chips)
Seat 4: Hero ($10.7 in chips)
Seat 5: aa56bdcc ($8.4 in chips)
Seat 6: f0d370b7 ($10.6 in chips)
62688fe9: posts small blind $0.05
dc7d0eb4: posts big blind $0.1
*** HOLE CARDS ***
Dealt to 62688fe9 
Dealt to dc7d0eb4 
Dealt to 1fba9f17 
Dealt to Hero [Ts Tc]
Dealt to aa56bdcc 
Dealt to f0d370b7 
1fba9f17: folds
Hero: raises $0.15 to $0.25
aa56bdcc: folds
f0d370b7: calls $0.25
62688fe9: folds
dc7d0eb4: folds
*** FLOP *** [8s Jd 9h]
Hero: checks
f0d370b7: bets $0.33
Hero: raises $1.09 to $1.42
f0d370b7: folds
Uncalled bet ($1.09) returned to Hero
*** SHOWDOWN ***
Hero collected $1.25 from pot
*** SUMMARY ***
Total pot $1.31 | Rake $0.06 | Jackpot $0 | Bingo $0 | Fortune $0 | Tax $0
Board [8s Jd 9h]
Seat 1: 62688fe9 (small blind) folded before Flop
Seat 2: dc7d0eb4 (big blind) folded before Flop
Seat 3: 1fba9f17 folded before Flop (didn't bet)
Seat 4: Hero showed [Ts] and won ($1.25)
Seat 5: aa56bdcc folded before Flop (didn't bet)
Seat 6: f0d370b7 (button) folded on the Flop"#;

/// Opponent shows one card in body and again in summary. (HD140646055)
pub const BODY_AND_SUMMARY_ONE_CARD: &str = r#"Poker Hand #HD140646055: Hold'em No Limit ($0.05/$0.1) - 2026/08/21 18:44:27
Table 'NLHRed18' 6-max Seat #2 is the button
Seat 1: Hero ($10.79 in chips)
Seat 2: 174d6de3 ($22.51 in chips)
Seat 3: 53be55c0 ($10.63 in chips)
Seat 4: ed5884a ($11.63 in chips)
Seat 5: a2886689 ($9.1 in chips)
Seat 6: dcd1bbfb ($12.21 in chips)
53be55c0: posts small blind $0.05
ed5884a: posts big blind $0.1
*** HOLE CARDS ***
Dealt to Hero [Qd Qc]
Dealt to 174d6de3 
Dealt to 53be55c0 
Dealt to ed5884a 
Dealt to a2886689 
Dealt to dcd1bbfb 
a2886689: folds
dcd1bbfb: raises $0.3 to $0.4
Hero: raises $0.8 to $1.2
174d6de3: folds
53be55c0: folds
ed5884a: folds
dcd1bbfb: calls $0.8
*** FLOP *** [2d Ks 4h]
dcd1bbfb: bets $1.28
Hero: folds
Uncalled bet ($1.28) returned to dcd1bbfb
Hero: shows [Qd Qc] (a pair of Queens)
dcd1bbfb: shows [6s]
*** SHOWDOWN ***
dcd1bbfb collected $2.43 from pot
*** SUMMARY ***
Total pot $2.55 | Rake $0.12 | Jackpot $0 | Bingo $0 | Fortune $0 | Tax $0
Board [2d Ks 4h]
Seat 1: Hero folded on the Flop
Seat 2: 174d6de3 (button) folded before Flop (didn't bet)
Seat 3: 53be55c0 (small blind) folded before Flop
Seat 4: ed5884a (big blind) folded before Flop
Seat 5: a2886689 folded before Flop (didn't bet)
Seat 6: dcd1bbfb showed [6s] and won ($2.43)"#;

/// Opponent shows two cards in summary with no body show. (HD140650557)
pub const SUMMARY_SHOW_TWO_CARDS: &str = r#"Poker Hand #HD140650557: Hold'em No Limit ($0.05/$0.1) - 2026/08/21 17:02:53
Table 'NLHRed14' 6-max Seat #3 is the button
Seat 2: 6570ba77 ($10.05 in chips)
Seat 3: 4af6a555 ($9.7 in chips)
Seat 4: Hero ($20.95 in chips)
Seat 5: 2d5ce8a9 ($10.52 in chips)
Hero: posts small blind $0.05
2d5ce8a9: posts big blind $0.1
*** HOLE CARDS ***
Dealt to 6570ba77 
Dealt to 4af6a555 
Dealt to Hero [Js Ac]
Dealt to 2d5ce8a9 
6570ba77: folds
4af6a555: folds
Hero: raises $0.2 to $0.3
2d5ce8a9: calls $0.2
*** FLOP *** [Kd Ks 5c]
Hero: bets $0.2
2d5ce8a9: raises $0.75 to $0.95
Hero: calls $0.75
*** TURN *** [Kd Ks 5c] [6d]
Hero: checks
2d5ce8a9: bets $1.25
Hero: calls $1.25
*** RIVER *** [Kd Ks 5c 6d] [9d]
Hero: checks
2d5ce8a9: bets $5
Hero: folds
Uncalled bet ($5) returned to 2d5ce8a9
*** SHOWDOWN ***
2d5ce8a9 collected $4.66 from pot
*** SUMMARY ***
Total pot $5 | Rake $0.24 | Jackpot $0.1 | Bingo $0 | Fortune $0 | Tax $0
Board [Kd Ks 5c 6d 9d]
Seat 2: 6570ba77 folded before Flop (didn't bet)
Seat 3: 4af6a555 (button) folded before Flop (didn't bet)
Seat 4: Hero (small blind) folded on the River
Seat 5: 2d5ce8a9 (big blind) showed [Qs Jh] and won ($4.66)"#;

/// Opponent shows two cards in summary and collected (no flop). (HD140652448)
pub const SUMMARY_SHOW_COLLECTED: &str = r#"Poker Hand #HD140652448: Hold'em No Limit ($0.05/$0.1) - 2026/08/21 17:53:23
Table 'NLHRed4' 6-max Seat #1 is the button
Seat 1: 2ff4bf29 ($1.03 in chips)
Seat 2: f82517 ($17.82 in chips)
Seat 3: 2ad6949d ($10.25 in chips)
Seat 4: c7bdd1e4 ($16.32 in chips)
Seat 5: edba49df ($9.5 in chips)
Seat 6: Hero ($12.67 in chips)
f82517: posts small blind $0.05
2ad6949d: posts big blind $0.1
*** HOLE CARDS ***
Dealt to 2ff4bf29 
Dealt to f82517 
Dealt to 2ad6949d 
Dealt to c7bdd1e4 
Dealt to edba49df 
Dealt to Hero [Kh 3c]
c7bdd1e4: folds
edba49df: raises $0.1 to $0.2
Hero: folds
2ff4bf29: raises $0.28 to $0.48
f82517: folds
2ad6949d: folds
edba49df: folds
Uncalled bet ($0.28) returned to 2ff4bf29
*** SHOWDOWN ***
2ff4bf29 collected $0.53 from pot
*** SUMMARY ***
Total pot $0.55 | Rake $0.02 | Jackpot $0 | Bingo $0 | Fortune $0 | Tax $0
Seat 1: 2ff4bf29 (button) showed [Qs Ah] and collected ($0.53)
Seat 2: f82517 (small blind) folded before Flop
Seat 3: 2ad6949d (big blind) folded before Flop
Seat 4: c7bdd1e4 folded before Flop (didn't bet)
Seat 5: edba49df folded before Flop
Seat 6: Hero folded before Flop (didn't bet)"#;

/// UTG straddle with the straddler exercising the preflop option by checking. (HD145385563)
pub const STRADDLE_UTG_OPTION: &str = r#"Poker Hand #HD145385563: Hold'em No Limit ($0.05/$0.1) - 2026/09/25 02:55:02
Table 'NLHRed15' 6-max Seat #6 is the button
Seat 1: 494b476f ($22.98 in chips)
Seat 2: c89b4da3 ($15.02 in chips)
Seat 3: 5daacec9 ($13.84 in chips)
Seat 4: adbf678c ($21.34 in chips)
Seat 5: Hero ($10 in chips)
Seat 6: 4e268376 ($16.74 in chips)
494b476f: posts small blind $0.05
c89b4da3: posts big blind $0.1
5daacec9: straddle $0.2
*** HOLE CARDS ***
Dealt to 494b476f 
Dealt to c89b4da3 
Dealt to 5daacec9 
Dealt to adbf678c 
Dealt to Hero [9c 5c]
Dealt to 4e268376 
adbf678c: calls $0.2
Hero: folds
4e268376: folds
494b476f: folds
c89b4da3: folds
5daacec9: checks
*** FLOP *** [Jd 9s 6s]
5daacec9: bets $0.19
adbf678c: raises $0.63 to $0.82
5daacec9: folds
Uncalled bet ($0.63) returned to adbf678c
*** SHOWDOWN ***
adbf678c collected $0.89 from pot
*** SUMMARY ***
Total pot $0.93 | Rake $0.04 | Jackpot $0 | Bingo $0 | Fortune $0 | Tax $0
Board [Jd 9s 6s]
Seat 1: 494b476f (small blind) folded before Flop
Seat 2: c89b4da3 (big blind) folded before Flop
Seat 3: 5daacec9 folded on the Flop
Seat 4: adbf678c won ($0.89)
Seat 5: Hero folded before Flop (didn't bet)
Seat 6: 4e268376 (button) folded before Flop (didn't bet)"#;

/// Small blind straddle ending in an all-in confrontation ($26.30 pot). (HD145385586)
pub const STRADDLE_SB_ALL_IN_POT: &str = r#"Poker Hand #HD145385586: Hold'em No Limit ($0.05/$0.1) - 2026/09/25 02:57:07
Table 'NLHRed15' 6-max Seat #2 is the button
Seat 1: 494b476f ($22.53 in chips)
Seat 2: c89b4da3 ($14.87 in chips)
Seat 3: 5daacec9 ($13.05 in chips)
Seat 4: adbf678c ($21.44 in chips)
Seat 5: Hero ($11.17 in chips)
Seat 6: 4e268376 ($16.74 in chips)
5daacec9: posts small blind $0.05
adbf678c: posts big blind $0.1
5daacec9: straddle $0.2
*** HOLE CARDS ***
Dealt to 494b476f 
Dealt to c89b4da3 
Dealt to 5daacec9 
Dealt to adbf678c 
Dealt to Hero [4d Qc]
Dealt to 4e268376 
adbf678c: calls $0.1
Hero: folds
4e268376: raises $0.55 to $0.75
494b476f: folds
c89b4da3: folds
5daacec9: calls $0.55
adbf678c: folds
*** FLOP *** [2h 9d Qh]
5daacec9: checks
4e268376: bets $1.15
5daacec9: calls $1.15
*** TURN *** [2h 9d Qh] [5c]
5daacec9: checks
4e268376: bets $2.65
5daacec9: raises $8.5 to $11.15 and is all-in
4e268376: calls $8.5
5daacec9: shows [Ad Ac] (a pair of Aces)
4e268376: shows [Qd Kc] (a pair of Queens)
*** RIVER *** [2h 9d Qh 5c] [8d]
*** SHOWDOWN ***
5daacec9 collected $25.2 from pot
*** SUMMARY ***
Total pot $26.3 | Rake $1 | Jackpot $0.1 | Bingo $0 | Fortune $0 | Tax $0
Board [2h 9d Qh 5c 8d]
Seat 1: 494b476f folded before Flop (didn't bet)
Seat 2: c89b4da3 (button) folded before Flop (didn't bet)
Seat 3: 5daacec9 (small blind) showed [Ad Ac] and won ($25.2) with a pair of Aces
Seat 4: adbf678c (big blind) folded before Flop
Seat 5: Hero folded before Flop (didn't bet)
Seat 6: 4e268376 showed [Qd Kc] and lost with a pair of Queens"#;

/// Big blind straddle walk with $0.15 uncalled bet returned. (HD145385508)
pub const STRADDLE_BB_WALK: &str = r#"Poker Hand #HD145385508: Hold'em No Limit ($0.05/$0.1) - 2026/09/25 02:51:36
Table 'NLHRed15' 6-max Seat #1 is the button
Seat 1: 494b476f ($23.54 in chips)
Seat 2: c89b4da3 ($15.53 in chips)
Seat 3: 5daacec9 ($13.56 in chips)
Seat 4: adbf678c ($20.05 in chips)
Seat 5: Hero ($10.08 in chips)
Seat 6: 4e268376 ($17.22 in chips)
c89b4da3: posts small blind $0.05
5daacec9: posts big blind $0.1
5daacec9: straddle $0.2
*** HOLE CARDS ***
Dealt to 494b476f 
Dealt to c89b4da3 
Dealt to 5daacec9 
Dealt to adbf678c 
Dealt to Hero [5h Qs]
Dealt to 4e268376 
adbf678c: folds
Hero: folds
4e268376: folds
494b476f: folds
c89b4da3: folds
Uncalled bet ($0.15) returned to 5daacec9
*** SHOWDOWN ***
5daacec9 collected $0.1 from pot
*** SUMMARY ***
Total pot $0.1 | Rake $0 | Jackpot $0 | Bingo $0 | Fortune $0 | Tax $0
Seat 1: 494b476f (button) folded before Flop (didn't bet)
Seat 2: c89b4da3 (small blind) folded before Flop
Seat 3: 5daacec9 (big blind) collected ($0.1)
Seat 4: adbf678c folded before Flop (didn't bet)
Seat 5: Hero folded before Flop (didn't bet)
Seat 6: 4e268376 folded before Flop (didn't bet)"#;

/// Button straddle where the small blind acts first. (HD145385522)
pub const STRADDLE_BUTTON: &str = r#"Poker Hand #HD145385522: Hold'em No Limit ($0.05/$0.1) - 2026/09/25 02:52:30
Table 'NLHRed15' 6-max Seat #3 is the button
Seat 1: 494b476f ($23.54 in chips)
Seat 2: c89b4da3 ($15.48 in chips)
Seat 3: 5daacec9 ($13.71 in chips)
Seat 4: adbf678c ($19.95 in chips)
Seat 5: Hero ($10.08 in chips)
Seat 6: 4e268376 ($17.22 in chips)
adbf678c: posts small blind $0.05
Hero: posts big blind $0.1
5daacec9: straddle $0.2
*** HOLE CARDS ***
Dealt to 494b476f 
Dealt to c89b4da3 
Dealt to 5daacec9 
Dealt to adbf678c 
Dealt to Hero [4d 7h]
Dealt to 4e268376 
adbf678c: raises $0.26 to $0.46
Hero: folds
4e268376: folds
494b476f: calls $0.46
c89b4da3: calls $0.46
5daacec9: calls $0.26
*** FLOP *** [As Th Kh]
adbf678c: checks
494b476f: checks
c89b4da3: checks
5daacec9: checks
*** TURN *** [As Th Kh] [9d]
adbf678c: bets $1.3
494b476f: folds
c89b4da3: folds
5daacec9: folds
Uncalled bet ($1.3) returned to adbf678c
*** SHOWDOWN ***
adbf678c collected $1.85 from pot
*** SUMMARY ***
Total pot $1.94 | Rake $0.09 | Jackpot $0 | Bingo $0 | Fortune $0 | Tax $0
Board [As Th Kh 9d]
Seat 1: 494b476f folded on the Turn
Seat 2: c89b4da3 folded on the Turn
Seat 3: 5daacec9 (button) folded on the Turn
Seat 4: adbf678c (small blind) won ($1.85)
Seat 5: Hero (big blind) folded before Flop
Seat 6: 4e268376 folded before Flop (didn't bet)"#;

/// Double straddle ($0.20 then $0.40). (HD145385574)
pub const DOUBLE_STRADDLE: &str = r#"Poker Hand #HD145385574: Hold'em No Limit ($0.05/$0.1) - 2026/09/25 02:56:16
Table 'NLHRed15' 6-max Seat #1 is the button
Seat 1: 494b476f ($22.93 in chips)
Seat 2: c89b4da3 ($14.92 in chips)
Seat 3: 5daacec9 ($13.45 in chips)
Seat 4: adbf678c ($21.84 in chips)
Seat 5: Hero ($10 in chips)
Seat 6: 4e268376 ($16.74 in chips)
c89b4da3: posts small blind $0.05
5daacec9: posts big blind $0.1
5daacec9: straddle $0.2
494b476f: straddle $0.4
*** HOLE CARDS ***
Dealt to 494b476f 
Dealt to c89b4da3 
Dealt to 5daacec9 
Dealt to adbf678c 
Dealt to Hero [Ks Qh]
Dealt to 4e268376 
c89b4da3: folds
5daacec9: calls $0.2
adbf678c: calls $0.4
Hero: raises $1.4 to $1.8
4e268376: folds
494b476f: folds
5daacec9: folds
adbf678c: folds
Uncalled bet ($1.4) returned to Hero
*** SHOWDOWN ***
Hero collected $1.57 from pot
*** SUMMARY ***
Total pot $1.65 | Rake $0.08 | Jackpot $0 | Bingo $0 | Fortune $0 | Tax $0
Seat 1: 494b476f (button) folded before Flop
Seat 2: c89b4da3 (small blind) folded before Flop
Seat 3: 5daacec9 (big blind) folded before Flop
Seat 4: adbf678c folded before Flop
Seat 5: Hero collected ($1.57)
Seat 6: 4e268376 folded before Flop (didn't bet)"#;

/// EV Cashout chosen on the flop; player loses showdown and receives cashout. (HD145319803)
pub const EV_CASHOUT_RECEIVED: &str = r#"Poker Hand #HD145319803: Hold'em No Limit ($0.05/$0.1) - 2026/09/24 18:12:47
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
Seat 6: 27a6d6a (big blind) folded before Flop"#;

/// EV Cashout chosen on the flop after preflop all-in; cashout risk paid after winning. (HD140647649)
pub const EV_CASHOUT_FLOP: &str = r#"Poker Hand #HD140647649: Hold'em No Limit ($0.05/$0.1) - 2026/08/21 19:37:47
Table 'NLHRed7' 6-max Seat #2 is the button
Seat 1: 62688fe9 ($1.96 in chips)
Seat 2: dc7d0eb4 ($10.59 in chips)
Seat 3: 1fba9f17 ($12.78 in chips)
Seat 4: Hero ($10.85 in chips)
Seat 5: aa56bdcc ($8.75 in chips)
Seat 6: f0d370b7 ($10.75 in chips)
1fba9f17: posts small blind $0.05
Hero: posts big blind $0.1
*** HOLE CARDS ***
Dealt to 62688fe9 
Dealt to dc7d0eb4 
Dealt to 1fba9f17 
Dealt to Hero [Th 8d]
Dealt to aa56bdcc 
Dealt to f0d370b7 
aa56bdcc: folds
f0d370b7: folds
62688fe9: raises $1.86 to $1.96 and is all-in
dc7d0eb4: calls $1.96
1fba9f17: folds
Hero: folds
62688fe9: shows [Ad Kd]
dc7d0eb4: shows [Qh Kh]
*** FLOP *** [Kc Jd 4s]
62688fe9: Chooses to EV Cashout
*** TURN *** [Kc Jd 4s] [3c]
*** RIVER *** [Kc Jd 4s 3c] [Td]
62688fe9: Pays Cashout Risk ($0.55)
*** SHOWDOWN ***
62688fe9 collected $3.78 from pot
*** SUMMARY ***
Total pot $4.07 | Rake $0.19 | Jackpot $0.1 | Bingo $0 | Fortune $0 | Tax $0
Board [Kc Jd 4s 3c Td]
Seat 1: 62688fe9 showed [Ad Kd] and won ($3.78) with a pair of Kings, Cashout Risk ($0.55)
Seat 2: dc7d0eb4 (button) showed [Qh Kh] and lost with a pair of Kings
Seat 3: 1fba9f17 (small blind) folded before Flop
Seat 4: Hero (big blind) folded before Flop
Seat 5: aa56bdcc folded before Flop (didn't bet)
Seat 6: f0d370b7 folded before Flop (didn't bet)"#;

/// EV Cashout chosen on the turn by caller; cashout risk paid after winning. (HD140653161)
pub const EV_CASHOUT_CALLER: &str = r#"Poker Hand #HD140653161: Hold'em No Limit ($0.05/$0.1) - 2026/08/21 18:15:05
Table 'NLHRed4' 6-max Seat #1 is the button
Seat 1: 28501331 ($2.27 in chips)
Seat 2: f82517 ($20.59 in chips)
Seat 3: de4e416a ($8.13 in chips)
Seat 4: c7bdd1e4 ($14.7 in chips)
Seat 6: Hero ($13.74 in chips)
f82517: posts small blind $0.05
de4e416a: posts big blind $0.1
*** HOLE CARDS ***
Dealt to 28501331 
Dealt to f82517 
Dealt to de4e416a 
Dealt to c7bdd1e4 
Dealt to Hero [3h 6c]
c7bdd1e4: raises $0.1 to $0.2
Hero: folds
28501331: calls $0.2
f82517: folds
de4e416a: calls $0.1
*** FLOP *** [7s 2c Jd]
de4e416a: checks
c7bdd1e4: bets $0.65
28501331: calls $0.65
de4e416a: folds
*** TURN *** [7s 2c Jd] [Qs]
c7bdd1e4: bets $1.37
28501331: raises $0.05 to $1.42 and is all-in
c7bdd1e4: calls $0.05
28501331: shows [Kc 9c] (King high)
c7bdd1e4: shows [Th Jh] (a pair of Jacks)
c7bdd1e4: Chooses to EV Cashout
*** RIVER *** [7s 2c Jd Qs] [4s]
c7bdd1e4: Pays Cashout Risk ($0.66)
*** SHOWDOWN ***
c7bdd1e4 collected $4.46 from pot
*** SUMMARY ***
Total pot $4.79 | Rake $0.23 | Jackpot $0.1 | Bingo $0 | Fortune $0 | Tax $0
Board [7s 2c Jd Qs 4s]
Seat 1: 28501331 (button) showed [Kc 9c] and lost with King high
Seat 2: f82517 (small blind) folded before Flop
Seat 3: de4e416a (big blind) folded on the Flop
Seat 4: c7bdd1e4 showed [Th Jh] and won ($4.46) with a pair of Jacks, Cashout Risk ($0.66)
Seat 6: Hero folded before Flop (didn't bet)"#;
