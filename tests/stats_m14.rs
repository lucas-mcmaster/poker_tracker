//! Tests for Milestone 14: opportunity counters (RFI, 3-bet, 4-bet, cold call,
//! steal, c-bets, check-raise) and column coverage.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
mod support;

use pretty_assertions::assert_eq;
use rust_decimal_macros::dec;

use poker_tracker::domain::{ActionType, CommunityCards, Street};
use poker_tracker::stats::{COLUMNS, PlayerHandStats, derive_hand_stats};
use support::{make_action, make_canonical_hand, make_player};

#[test]
fn player_hand_stats_get_covers_all_columns() {
    let s = PlayerHandStats::new("Hero".to_string(), Some(0));
    assert_eq!(COLUMNS.len(), 76);
    for &col in COLUMNS {
        assert!(
            s.get(col).is_some(),
            "column {col} must return Some from get()"
        );
    }
    assert_eq!(s.get("unknown_column"), None);
    assert_eq!(s.get(""), None);
}

#[test]
fn canonical_rfi_open_raise_limper_short_stack() {
    // 1. Open raise is p_rfi
    let p1 = vec![
        make_player(1, "BTN", dec!(10), true),
        make_player(2, "SB", dec!(10), false),
        make_player(3, "BB", dec!(10), false),
    ];
    let a1 = vec![
        make_action(
            0,
            Street::Preflop,
            "SB",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "BB",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "BTN",
            ActionType::Raise,
            dec!(0.30),
            dec!(0.30),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "SB",
            ActionType::Fold,
            dec!(0),
            dec!(0.05),
            false,
        ),
        make_action(
            4,
            Street::Preflop,
            "BB",
            ActionType::Fold,
            dec!(0),
            dec!(0.10),
            false,
        ),
    ];
    let h1 = make_canonical_hand(1, p1, a1, vec![], vec![("BTN", dec!(0.45))]);
    let s1 = derive_hand_stats(&h1);
    assert_eq!(s1[0].p_rfi_opp, 1);
    assert_eq!(s1[0].p_rfi, 1);

    // 2. Limper before player removes p_rfi_opp
    let p2 = vec![
        make_player(1, "UTG", dec!(10), false),
        make_player(2, "BTN", dec!(10), true),
        make_player(3, "SB", dec!(10), false),
        make_player(4, "BB", dec!(10), false),
    ];
    let a2 = vec![
        make_action(
            0,
            Street::Preflop,
            "SB",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "BB",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "UTG",
            ActionType::Call,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "BTN",
            ActionType::Raise,
            dec!(0.40),
            dec!(0.40),
            false,
        ),
        make_action(
            4,
            Street::Preflop,
            "SB",
            ActionType::Fold,
            dec!(0),
            dec!(0.05),
            false,
        ),
        make_action(
            5,
            Street::Preflop,
            "BB",
            ActionType::Fold,
            dec!(0),
            dec!(0.10),
            false,
        ),
        make_action(
            6,
            Street::Preflop,
            "UTG",
            ActionType::Fold,
            dec!(0),
            dec!(0.10),
            false,
        ),
    ];
    let h2 = make_canonical_hand(2, p2, a2, vec![], vec![("BTN", dec!(0.65))]);
    let s2 = derive_hand_stats(&h2);
    assert_eq!(s2[1].p_rfi_opp, 0);
    assert_eq!(s2[1].p_rfi, 0);

    // 3. Player whose stack does not exceed big blind has no p_rfi_opp
    let p3 = vec![
        make_player(1, "BTN", dec!(0.10), true), // stack = BB
        make_player(2, "SB", dec!(10), false),
        make_player(3, "BB", dec!(10), false),
    ];
    let a3 = vec![
        make_action(
            0,
            Street::Preflop,
            "SB",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "BB",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "BTN",
            ActionType::Call,
            dec!(0.10),
            dec!(0.10),
            true,
        ),
        make_action(
            3,
            Street::Preflop,
            "SB",
            ActionType::Fold,
            dec!(0),
            dec!(0.05),
            false,
        ),
        make_action(
            4,
            Street::Preflop,
            "BB",
            ActionType::Check,
            dec!(0),
            dec!(0.10),
            false,
        ),
    ];
    let h3 = make_canonical_hand(1, p3, a3, vec![], vec![("BB", dec!(0.25))]);
    let s3 = derive_hand_stats(&h3);
    assert_eq!(s3[0].p_rfi_opp, 0);
    assert_eq!(s3[0].p_rfi, 0);
}

#[test]
fn canonical_3bet_and_4bet() {
    // 1. Facing one raise gives p_3bet_opp
    let p1 = vec![
        make_player(1, "CO", dec!(10), false),
        make_player(2, "BTN", dec!(10), true),
        make_player(3, "SB", dec!(10), false),
        make_player(4, "BB", dec!(10), false),
    ];
    let a1 = vec![
        make_action(
            0,
            Street::Preflop,
            "SB",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "BB",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "CO",
            ActionType::Raise,
            dec!(0.25),
            dec!(0.25),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "BTN",
            ActionType::Raise,
            dec!(0.75),
            dec!(0.75),
            false,
        ),
        make_action(
            4,
            Street::Preflop,
            "SB",
            ActionType::Fold,
            dec!(0),
            dec!(0.05),
            false,
        ),
        make_action(
            5,
            Street::Preflop,
            "BB",
            ActionType::Fold,
            dec!(0),
            dec!(0.10),
            false,
        ),
        make_action(
            6,
            Street::Preflop,
            "CO",
            ActionType::Fold,
            dec!(0),
            dec!(0.25),
            false,
        ),
    ];
    let h1 = make_canonical_hand(2, p1, a1, vec![], vec![("BTN", dec!(1.15))]);
    let s1 = derive_hand_stats(&h1);
    assert_eq!(s1[1].p_3bet_opp, 1);
    assert_eq!(s1[1].p_3bet, 1);

    // 2. Facing an open and a 3-bet gives p_3bet_faced and p_4bet_opp
    // In h1, CO faced the 3-bet (raises == 2):
    assert_eq!(s1[0].p_3bet_faced, 1);
    assert_eq!(s1[0].p_3bet_fold, 1);
    assert_eq!(s1[0].p_4bet_opp, 1);

    // Cold 4-bet scenario:
    let p2 = vec![
        make_player(1, "EP", dec!(10), false),
        make_player(2, "MP", dec!(10), false),
        make_player(3, "CO", dec!(10), true), // Hero
        make_player(4, "BTN", dec!(10), false),
        make_player(5, "SB", dec!(10), false),
        make_player(6, "BB", dec!(10), false),
    ];
    let a2 = vec![
        make_action(
            0,
            Street::Preflop,
            "SB",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "BB",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "EP",
            ActionType::Raise,
            dec!(0.25),
            dec!(0.25),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "MP",
            ActionType::Raise,
            dec!(0.80),
            dec!(0.80),
            false,
        ),
        make_action(
            4,
            Street::Preflop,
            "CO",
            ActionType::Fold,
            dec!(0),
            dec!(0),
            false,
        ),
        make_action(
            5,
            Street::Preflop,
            "BTN",
            ActionType::Fold,
            dec!(0),
            dec!(0),
            false,
        ),
        make_action(
            6,
            Street::Preflop,
            "SB",
            ActionType::Fold,
            dec!(0),
            dec!(0.05),
            false,
        ),
        make_action(
            7,
            Street::Preflop,
            "BB",
            ActionType::Fold,
            dec!(0),
            dec!(0.10),
            false,
        ),
        make_action(
            8,
            Street::Preflop,
            "EP",
            ActionType::Fold,
            dec!(0),
            dec!(0.25),
            false,
        ),
    ];
    let h2 = make_canonical_hand(4, p2, a2, vec![], vec![("MP", dec!(1.20))]);
    let s2 = derive_hand_stats(&h2);
    // CO faces open and 3-bet
    assert_eq!(s2[2].p_3bet_faced, 1);
    assert_eq!(s2[2].p_3bet_fold, 1);
    assert_eq!(s2[2].p_4bet_opp, 1);

    // 3. 4-bet then fold to a 5-bet counts in p_4bet_fold
    let p3 = vec![
        make_player(1, "BTN", dec!(10), true),
        make_player(2, "SB", dec!(10), false),
        make_player(3, "BB", dec!(10), false),
    ];
    let a3 = vec![
        make_action(
            0,
            Street::Preflop,
            "SB",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "BB",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "BTN",
            ActionType::Raise,
            dec!(0.25),
            dec!(0.25),
            false,
        ), // 1st raise (open)
        make_action(
            3,
            Street::Preflop,
            "SB",
            ActionType::Raise,
            dec!(0.80),
            dec!(0.80),
            false,
        ), // 2nd raise (3-bet)
        make_action(
            4,
            Street::Preflop,
            "BB",
            ActionType::Fold,
            dec!(0),
            dec!(0.10),
            false,
        ),
        make_action(
            5,
            Street::Preflop,
            "BTN",
            ActionType::Raise,
            dec!(2.20),
            dec!(2.20),
            false,
        ), // 3rd raise (4-bet)
        make_action(
            6,
            Street::Preflop,
            "SB",
            ActionType::Raise,
            dec!(5.50),
            dec!(5.50),
            false,
        ), // 4th raise (5-bet)
        make_action(
            7,
            Street::Preflop,
            "BTN",
            ActionType::Fold,
            dec!(0),
            dec!(2.20),
            false,
        ), // BTN folds to 5-bet
    ];
    let h3 = make_canonical_hand(1, p3, a3, vec![], vec![("SB", dec!(7.80))]);
    let s3 = derive_hand_stats(&h3);
    assert_eq!(s3[0].p_4bet_opp, 1);
    assert_eq!(s3[0].p_4bet, 1);
    assert_eq!(s3[0].p_4bet_faced, 1);
    assert_eq!(s3[0].p_4bet_fold, 1);
}

#[test]
fn canonical_cold_call() {
    let p = vec![
        make_player(1, "CO", dec!(10), false),
        make_player(2, "BTN", dec!(10), true),
        make_player(3, "SB", dec!(10), false),
        make_player(4, "BB", dec!(10), false),
    ];
    let a = vec![
        make_action(
            0,
            Street::Preflop,
            "SB",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "BB",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "CO",
            ActionType::Raise,
            dec!(0.30),
            dec!(0.30),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "BTN",
            ActionType::Call,
            dec!(0.30),
            dec!(0.30),
            false,
        ), // BTN cold calls
        make_action(
            4,
            Street::Preflop,
            "SB",
            ActionType::Call,
            dec!(0.25),
            dec!(0.30),
            false,
        ), // SB calls (not a cold call!)
        make_action(
            5,
            Street::Preflop,
            "BB",
            ActionType::Fold,
            dec!(0),
            dec!(0.10),
            false,
        ),
    ];
    let h = make_canonical_hand(2, p, a, vec![], vec![("CO", dec!(1.00))]);
    let s = derive_hand_stats(&h);

    // BTN cold called
    assert_eq!(s[1].p_cc_opp, 1);
    assert_eq!(s[1].p_cc, 1);

    // SB posted blind, so calling a raise is not p_cc_opp
    assert_eq!(s[2].p_cc_opp, 0);
    assert_eq!(s[2].p_cc, 0);
}

#[test]
fn canonical_steal_and_defence() {
    // 1. Open from CO/BTN/SB is p_steal; from MP it is not
    let p1 = vec![
        make_player(1, "MP", dec!(10), false),
        make_player(2, "CO", dec!(10), false),
        make_player(3, "BTN", dec!(10), true),
        make_player(4, "SB", dec!(10), false),
        make_player(5, "BB", dec!(10), false),
    ];
    let a1 = vec![
        make_action(
            0,
            Street::Preflop,
            "SB",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "BB",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "MP",
            ActionType::Raise,
            dec!(0.30),
            dec!(0.30),
            false,
        ), // MP opens
        make_action(
            3,
            Street::Preflop,
            "CO",
            ActionType::Fold,
            dec!(0),
            dec!(0),
            false,
        ),
        make_action(
            4,
            Street::Preflop,
            "BTN",
            ActionType::Fold,
            dec!(0),
            dec!(0),
            false,
        ),
        make_action(
            5,
            Street::Preflop,
            "SB",
            ActionType::Fold,
            dec!(0),
            dec!(0.05),
            false,
        ),
        make_action(
            6,
            Street::Preflop,
            "BB",
            ActionType::Fold,
            dec!(0),
            dec!(0.10),
            false,
        ),
    ];
    let h1 = make_canonical_hand(3, p1, a1, vec![], vec![("MP", dec!(0.45))]);
    let s1 = derive_hand_stats(&h1);
    assert_eq!(s1[0].p_rfi_opp, 1);
    assert_eq!(s1[0].p_steal_opp, 0); // MP is not steal opp!
    assert_eq!(s1[0].p_steal, 0);

    // 2. Hand with a straddle has no steal opportunity
    let p2 = vec![
        make_player(1, "BTN", dec!(10), true),
        make_player(2, "SB", dec!(10), false),
        make_player(3, "BB", dec!(10), false),
    ];
    let a2 = vec![
        make_action(
            0,
            Street::Preflop,
            "SB",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "BB",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "BTN",
            ActionType::PostStraddle,
            dec!(0.20),
            dec!(0.20),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "SB",
            ActionType::Fold,
            dec!(0),
            dec!(0.05),
            false,
        ),
        make_action(
            4,
            Street::Preflop,
            "BB",
            ActionType::Fold,
            dec!(0),
            dec!(0.10),
            false,
        ),
    ];
    let h2 = make_canonical_hand(1, p2, a2, vec![], vec![("BTN", dec!(0.35))]);
    let s2 = derive_hand_stats(&h2);
    assert_eq!(s2[0].p_steal_opp, 0);

    // 3. Caller between steal and BB removes p_bb_steal_faced
    let p3 = vec![
        make_player(1, "CO", dec!(10), false),
        make_player(2, "BTN", dec!(10), false),
        make_player(3, "SB", dec!(10), false),
        make_player(4, "BB", dec!(10), true),
    ];
    let a3 = vec![
        make_action(
            0,
            Street::Preflop,
            "SB",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "BB",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "CO",
            ActionType::Raise,
            dec!(0.30),
            dec!(0.30),
            false,
        ), // CO steals
        make_action(
            3,
            Street::Preflop,
            "BTN",
            ActionType::Call,
            dec!(0.30),
            dec!(0.30),
            false,
        ), // BTN calls
        make_action(
            4,
            Street::Preflop,
            "SB",
            ActionType::Fold,
            dec!(0),
            dec!(0.05),
            false,
        ),
        make_action(
            5,
            Street::Preflop,
            "BB",
            ActionType::Fold,
            dec!(0),
            dec!(0.10),
            false,
        ),
    ];
    let h3 = make_canonical_hand(2, p3, a3, vec![], vec![("CO", dec!(0.75))]);
    let s3 = derive_hand_stats(&h3);
    assert_eq!(s3[0].p_steal_opp, 1);
    assert_eq!(s3[0].p_steal, 1);
    // BB does NOT face steal because BTN called between CO and BB
    assert_eq!(s3[3].p_bb_steal_faced, 0);

    // 4. Heads-up: SB's open is a steal and BB faces it
    let p4 = vec![
        make_player(1, "SB", dec!(10), false), // button is SB in HU
        make_player(2, "BB", dec!(10), true),
    ];
    let a4 = vec![
        make_action(
            0,
            Street::Preflop,
            "SB",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "BB",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "SB",
            ActionType::Raise,
            dec!(0.25),
            dec!(0.30),
            false,
        ), // SB steals
        make_action(
            3,
            Street::Preflop,
            "BB",
            ActionType::Fold,
            dec!(0),
            dec!(0.10),
            false,
        ), // BB folds
    ];
    let h4 = make_canonical_hand(1, p4, a4, vec![], vec![("SB", dec!(0.40))]);
    let s4 = derive_hand_stats(&h4);
    assert_eq!(s4[0].p_steal_opp, 1);
    assert_eq!(s4[0].p_steal, 1);
    assert_eq!(s4[1].p_bb_steal_faced, 1);
    assert_eq!(s4[1].p_bb_steal_fold, 1);
}

#[test]
fn canonical_cbet_opportunities_and_faced() {
    // 1. All-in preflop aggressor has no f_cbet_opp
    let p1 = vec![
        make_player(1, "Hero", dec!(1.00), true),
        make_player(2, "SB", dec!(10), false),
        make_player(3, "BB", dec!(10), false),
    ];
    let a1 = vec![
        make_action(
            0,
            Street::Preflop,
            "SB",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "BB",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Hero",
            ActionType::Raise,
            dec!(1.00),
            dec!(1.00),
            true,
        ), // Hero all-in
        make_action(
            3,
            Street::Preflop,
            "SB",
            ActionType::Fold,
            dec!(0),
            dec!(0.05),
            false,
        ),
        make_action(
            4,
            Street::Preflop,
            "BB",
            ActionType::Call,
            dec!(0.90),
            dec!(1.00),
            false,
        ),
    ];
    let board = vec![CommunityCards {
        run_index: 0,
        flop: Some([
            poker_tracker::domain::Card::new(
                poker_tracker::domain::Rank::Ace,
                poker_tracker::domain::Suit::Spades,
            ),
            poker_tracker::domain::Card::new(
                poker_tracker::domain::Rank::King,
                poker_tracker::domain::Suit::Hearts,
            ),
            poker_tracker::domain::Card::new(
                poker_tracker::domain::Rank::Two,
                poker_tracker::domain::Suit::Clubs,
            ),
        ]),
        turn: None,
        river: None,
    }];
    let h1 = make_canonical_hand(1, p1, a1, board, vec![("BB", dec!(2.05))]);
    let s1 = derive_hand_stats(&h1);
    assert_eq!(s1[0].f_saw, 1);
    assert_eq!(s1[0].f_cbet_opp, 0); // Hero is all in, cannot bet!

    // 2. Raise between c-bet and player's decision removes f_cbet_faced
    let p2 = vec![
        make_player(1, "Hero", dec!(10), true), // Preflop aggressor & c-bettor
        make_player(2, "P2", dec!(10), false),
        make_player(3, "P3", dec!(10), false),
        make_player(4, "BB", dec!(10), false),
    ];
    let a2 = vec![
        make_action(
            0,
            Street::Preflop,
            "P3",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "BB",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Hero",
            ActionType::Raise,
            dec!(0.30),
            dec!(0.30),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "P2",
            ActionType::Call,
            dec!(0.30),
            dec!(0.30),
            false,
        ),
        make_action(
            4,
            Street::Preflop,
            "P3",
            ActionType::Fold,
            dec!(0),
            dec!(0.05),
            false,
        ),
        make_action(
            5,
            Street::Preflop,
            "BB",
            ActionType::Call,
            dec!(0.20),
            dec!(0.30),
            false,
        ),
        // Flop
        make_action(
            6,
            Street::Flop,
            "BB",
            ActionType::Check,
            dec!(0),
            dec!(0),
            false,
        ),
        make_action(
            7,
            Street::Flop,
            "Hero",
            ActionType::Bet,
            dec!(0.50),
            dec!(0.50),
            false,
        ), // Hero c-bets
        make_action(
            8,
            Street::Flop,
            "P2",
            ActionType::Raise,
            dec!(1.50),
            dec!(1.50),
            false,
        ), // P2 raises c-bet
        make_action(
            9,
            Street::Flop,
            "BB",
            ActionType::Fold,
            dec!(0),
            dec!(0),
            false,
        ), // BB decides after raise!
        make_action(
            10,
            Street::Flop,
            "Hero",
            ActionType::Fold,
            dec!(0),
            dec!(0.50),
            false,
        ),
    ];
    let board2 = vec![CommunityCards {
        run_index: 0,
        flop: Some([
            poker_tracker::domain::Card::new(
                poker_tracker::domain::Rank::Ace,
                poker_tracker::domain::Suit::Spades,
            ),
            poker_tracker::domain::Card::new(
                poker_tracker::domain::Rank::King,
                poker_tracker::domain::Suit::Hearts,
            ),
            poker_tracker::domain::Card::new(
                poker_tracker::domain::Rank::Two,
                poker_tracker::domain::Suit::Clubs,
            ),
        ]),
        turn: None,
        river: None,
    }];
    let h2 = make_canonical_hand(1, p2, a2, board2, vec![("P2", dec!(2.95))]);
    let s2 = derive_hand_stats(&h2);
    // P2 faced c-bet and raised
    assert_eq!(s2[1].f_cbet_faced, 1);
    assert_eq!(s2[1].f_cbet_raise, 1);
    // BB had a raise between c-bet and BB's decision, so BB does NOT face c-bet!
    assert_eq!(s2[3].f_cbet_faced, 0);

    // 3. t_cbet_opp requires a flop c-bet by the same player
    // If Hero was preflop aggressor, but checked flop, Hero has no t_cbet_opp on turn
    let p3 = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "BB", dec!(10), false),
    ];
    let a3 = vec![
        make_action(
            0,
            Street::Preflop,
            "Hero",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "BB",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Hero",
            ActionType::Raise,
            dec!(0.25),
            dec!(0.30),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "BB",
            ActionType::Call,
            dec!(0.20),
            dec!(0.30),
            false,
        ),
        // Flop: both check
        make_action(
            4,
            Street::Flop,
            "BB",
            ActionType::Check,
            dec!(0),
            dec!(0),
            false,
        ),
        make_action(
            5,
            Street::Flop,
            "Hero",
            ActionType::Check,
            dec!(0),
            dec!(0),
            false,
        ),
        // Turn: both check
        make_action(
            6,
            Street::Turn,
            "BB",
            ActionType::Check,
            dec!(0),
            dec!(0),
            false,
        ),
        make_action(
            7,
            Street::Turn,
            "Hero",
            ActionType::Check,
            dec!(0),
            dec!(0),
            false,
        ),
    ];
    let board3 = vec![CommunityCards {
        run_index: 0,
        flop: Some([
            poker_tracker::domain::Card::new(
                poker_tracker::domain::Rank::Ace,
                poker_tracker::domain::Suit::Spades,
            ),
            poker_tracker::domain::Card::new(
                poker_tracker::domain::Rank::King,
                poker_tracker::domain::Suit::Hearts,
            ),
            poker_tracker::domain::Card::new(
                poker_tracker::domain::Rank::Two,
                poker_tracker::domain::Suit::Clubs,
            ),
        ]),
        turn: Some(poker_tracker::domain::Card::new(
            poker_tracker::domain::Rank::Three,
            poker_tracker::domain::Suit::Diamonds,
        )),
        river: None,
    }];
    let h3 = make_canonical_hand(1, p3, a3, board3, vec![("BB", dec!(0.60))]);
    let s3 = derive_hand_stats(&h3);
    assert_eq!(s3[0].f_cbet_opp, 1);
    assert_eq!(s3[0].f_cbet, 0); // Hero checked
    assert_eq!(s3[0].t_cbet_opp, 0); // No t_cbet_opp because Hero did not flop c-bet!
}

#[test]
fn canonical_check_raise() {
    let board = vec![CommunityCards {
        run_index: 0,
        flop: Some([
            poker_tracker::domain::Card::new(
                poker_tracker::domain::Rank::Ace,
                poker_tracker::domain::Suit::Spades,
            ),
            poker_tracker::domain::Card::new(
                poker_tracker::domain::Rank::King,
                poker_tracker::domain::Suit::Hearts,
            ),
            poker_tracker::domain::Card::new(
                poker_tracker::domain::Rank::Two,
                poker_tracker::domain::Suit::Clubs,
            ),
        ]),
        turn: None,
        river: None,
    }];

    // 1. Check, face a bet, raise is f_xr
    let p1 = vec![
        make_player(1, "P1", dec!(10), false),
        make_player(2, "P2", dec!(10), false),
    ];
    let a1 = vec![
        make_action(
            0,
            Street::Preflop,
            "P1",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "P2",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "P1",
            ActionType::Call,
            dec!(0.05),
            dec!(0.10),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "P2",
            ActionType::Check,
            dec!(0),
            dec!(0.10),
            false,
        ),
        // Flop
        make_action(
            4,
            Street::Flop,
            "P2",
            ActionType::Check,
            dec!(0),
            dec!(0),
            false,
        ),
        make_action(
            5,
            Street::Flop,
            "P1",
            ActionType::Bet,
            dec!(0.15),
            dec!(0.15),
            false,
        ),
        make_action(
            6,
            Street::Flop,
            "P2",
            ActionType::Raise,
            dec!(0.45),
            dec!(0.45),
            false,
        ), // P2 check-raises!
        make_action(
            7,
            Street::Flop,
            "P1",
            ActionType::Fold,
            dec!(0),
            dec!(0.15),
            false,
        ),
    ];
    let h1 = make_canonical_hand(1, p1, a1, board.clone(), vec![("P2", dec!(0.50))]);
    let s1 = derive_hand_stats(&h1);
    assert_eq!(s1[1].f_xr_opp, 1);
    assert_eq!(s1[1].f_xr, 1);

    // 2. Check, face a bet, call is f_xr_opp only
    let p2 = vec![
        make_player(1, "P1", dec!(10), false),
        make_player(2, "P2", dec!(10), false),
    ];
    let a2 = vec![
        make_action(
            0,
            Street::Preflop,
            "P1",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "P2",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "P1",
            ActionType::Call,
            dec!(0.05),
            dec!(0.10),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "P2",
            ActionType::Check,
            dec!(0),
            dec!(0.10),
            false,
        ),
        // Flop
        make_action(
            4,
            Street::Flop,
            "P2",
            ActionType::Check,
            dec!(0),
            dec!(0),
            false,
        ),
        make_action(
            5,
            Street::Flop,
            "P1",
            ActionType::Bet,
            dec!(0.15),
            dec!(0.15),
            false,
        ),
        make_action(
            6,
            Street::Flop,
            "P2",
            ActionType::Call,
            dec!(0.15),
            dec!(0.15),
            false,
        ), // P2 calls
    ];
    let h2 = make_canonical_hand(1, p2, a2, board, vec![("P1", dec!(0.50))]);
    let s2 = derive_hand_stats(&h2);
    assert_eq!(s2[1].f_xr_opp, 1);
    assert_eq!(s2[1].f_xr, 0);
}

/// Heads-up: A (button, SB) opens, B calls; A c-bets the flop and B calls; on the turn B checks,
/// A bets again (a turn c-bet) and B makes `turn_reply` (chips added, B's turn total after).
fn turn_cbet_hand(
    turn_reply: ActionType,
    amount: rust_decimal::Decimal,
) -> poker_tracker::domain::CanonicalHand {
    let p = vec![
        make_player(1, "A", dec!(10), false),
        make_player(2, "B", dec!(10), true),
    ];
    let act = |seq, street, who, kind, amount, total| {
        make_action(seq, street, who, kind, amount, total, false)
    };
    let mut a = vec![
        act(
            0,
            Street::Preflop,
            "A",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
        ),
        act(
            1,
            Street::Preflop,
            "B",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
        ),
        act(
            2,
            Street::Preflop,
            "A",
            ActionType::Raise,
            dec!(0.25),
            dec!(0.30),
        ),
        act(
            3,
            Street::Preflop,
            "B",
            ActionType::Call,
            dec!(0.20),
            dec!(0.30),
        ),
        act(4, Street::Flop, "B", ActionType::Check, dec!(0), dec!(0)),
        act(
            5,
            Street::Flop,
            "A",
            ActionType::Bet,
            dec!(0.30),
            dec!(0.30),
        ),
        act(
            6,
            Street::Flop,
            "B",
            ActionType::Call,
            dec!(0.30),
            dec!(0.30),
        ),
        act(7, Street::Turn, "B", ActionType::Check, dec!(0), dec!(0)),
        act(
            8,
            Street::Turn,
            "A",
            ActionType::Bet,
            dec!(0.60),
            dec!(0.60),
        ),
        act(9, Street::Turn, "B", turn_reply, amount, amount),
    ];
    if turn_reply == ActionType::Raise {
        a.push(act(
            10,
            Street::Turn,
            "A",
            ActionType::Fold,
            dec!(0),
            dec!(0.60),
        ));
    }
    let board = CommunityCards {
        run_index: 0,
        flop: Some([
            "2c".parse().unwrap(),
            "7d".parse().unwrap(),
            "9h".parse().unwrap(),
        ]),
        turn: Some("Ks".parse().unwrap()),
        river: None,
    };
    make_canonical_hand(1, p, a, vec![board], vec![])
}

#[test]
fn turn_cbet_faced_fold_and_raise_are_counted() {
    // docs/stats.md §4.2: facing a turn c-bet, the reply is counted as fold, call or raise.
    // Hero's corpus values of t_cbet_fold and t_cbet_raise are 0, so only these hands set them.
    for (reply, amount, expected) in [
        (ActionType::Fold, dec!(0), [1, 0, 0]),
        (ActionType::Call, dec!(0.60), [0, 1, 0]),
        (ActionType::Raise, dec!(1.80), [0, 0, 1]),
    ] {
        let stats = derive_hand_stats(&turn_cbet_hand(reply, amount));
        let (a, b) = (&stats[0], &stats[1]);
        assert_eq!((a.t_cbet_opp, a.t_cbet), (1, 1), "{reply:?}");
        assert_eq!(b.t_cbet_faced, 1, "{reply:?}");
        assert_eq!(
            [b.t_cbet_fold, b.t_cbet_call, b.t_cbet_raise],
            expected,
            "{reply:?}"
        );
    }
}

#[test]
fn a_raise_after_the_turn_cbet_removes_turn_cbet_faced() {
    // docs/stats.md §4.2: t_cbet_faced needs no Raise between the c-bet and the decision (as on
    // the flop). Three-handed: A (button) c-bets flop and turn, B raises the turn c-bet, C folds.
    let p = vec![
        make_player(1, "A", dec!(10), false),
        make_player(2, "B", dec!(10), false),
        make_player(3, "C", dec!(10), true),
    ];
    let act = |seq, street, who, kind, amount, total| {
        make_action(seq, street, who, kind, amount, total, false)
    };
    let a = vec![
        act(
            0,
            Street::Preflop,
            "B",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
        ),
        act(
            1,
            Street::Preflop,
            "C",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
        ),
        act(
            2,
            Street::Preflop,
            "A",
            ActionType::Raise,
            dec!(0.30),
            dec!(0.30),
        ),
        act(
            3,
            Street::Preflop,
            "B",
            ActionType::Call,
            dec!(0.25),
            dec!(0.30),
        ),
        act(
            4,
            Street::Preflop,
            "C",
            ActionType::Call,
            dec!(0.20),
            dec!(0.30),
        ),
        act(5, Street::Flop, "B", ActionType::Check, dec!(0), dec!(0)),
        act(6, Street::Flop, "C", ActionType::Check, dec!(0), dec!(0)),
        act(
            7,
            Street::Flop,
            "A",
            ActionType::Bet,
            dec!(0.45),
            dec!(0.45),
        ),
        act(
            8,
            Street::Flop,
            "B",
            ActionType::Call,
            dec!(0.45),
            dec!(0.45),
        ),
        act(
            9,
            Street::Flop,
            "C",
            ActionType::Call,
            dec!(0.45),
            dec!(0.45),
        ),
        act(10, Street::Turn, "B", ActionType::Check, dec!(0), dec!(0)),
        act(11, Street::Turn, "C", ActionType::Check, dec!(0), dec!(0)),
        act(
            12,
            Street::Turn,
            "A",
            ActionType::Bet,
            dec!(1.00),
            dec!(1.00),
        ),
        act(
            13,
            Street::Turn,
            "B",
            ActionType::Raise,
            dec!(3.00),
            dec!(3.00),
        ),
        act(14, Street::Turn, "C", ActionType::Fold, dec!(0), dec!(0)),
        act(15, Street::Turn, "A", ActionType::Fold, dec!(0), dec!(1.00)),
    ];
    let board = CommunityCards {
        run_index: 0,
        flop: Some([
            "2c".parse().unwrap(),
            "7d".parse().unwrap(),
            "9h".parse().unwrap(),
        ]),
        turn: Some("Ks".parse().unwrap()),
        river: None,
    };
    let stats = derive_hand_stats(&make_canonical_hand(1, p, a, vec![board], vec![]));
    let (a, b, c) = (&stats[0], &stats[1], &stats[2]);
    assert_eq!((a.t_cbet_opp, a.t_cbet), (1, 1));
    assert_eq!((b.t_cbet_faced, b.t_cbet_raise), (1, 1));
    assert_eq!(
        (c.t_cbet_faced, c.t_cbet_fold),
        (0, 0),
        "B's raise came between"
    );
}

#[test]
fn facing_a_four_bet_a_fold_is_not_a_call() {
    // docs/stats.md §4.1: p_4bet_call counts a call of the 4-bet; a fold counts in p_4bet_fold
    // only. Hero's corpus counts cannot tell (10 faced = 5 calls + 5 other replies).
    for (reply, amount, total, expected) in [
        (ActionType::Call, dec!(1.80), dec!(2.70), (1, 1, 0)),
        (ActionType::Fold, dec!(0), dec!(0.90), (1, 0, 1)),
    ] {
        let p = vec![
            make_player(1, "A", dec!(10), false),
            make_player(2, "B", dec!(10), true),
        ];
        let act = |seq, who, kind, amount, total| {
            make_action(seq, Street::Preflop, who, kind, amount, total, false)
        };
        let a = vec![
            act(0, "A", ActionType::PostSmallBlind, dec!(0.05), dec!(0.05)),
            act(1, "B", ActionType::PostBigBlind, dec!(0.10), dec!(0.10)),
            act(2, "A", ActionType::Raise, dec!(0.25), dec!(0.30)),
            act(3, "B", ActionType::Raise, dec!(0.80), dec!(0.90)),
            act(4, "A", ActionType::Raise, dec!(2.40), dec!(2.70)),
            act(5, "B", reply, amount, total),
        ];
        let stats = derive_hand_stats(&make_canonical_hand(1, p, a, vec![], vec![]));
        let b = &stats[1];
        assert_eq!(
            (b.p_4bet_faced, b.p_4bet_call, b.p_4bet_fold),
            expected,
            "{reply:?}"
        );
    }
}
