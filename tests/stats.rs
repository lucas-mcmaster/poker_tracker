//! Tests for derived statistics: positions, counters, near misses, and worked examples.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
mod support;

use std::collections::HashMap;
use std::str::FromStr;

use pretty_assertions::assert_eq;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use common::{
    HU_PREFLOP_FOLD, SAMPLE_SESSION, STRADDLE_BB_WALK, STRADDLE_UTG_OPTION, SUMMARY_SHOW_TWO_CARDS,
    WALK, gg,
};
use poker_tracker::domain::{ActionType, CommunityCards, Player, Street};
use poker_tracker::parser::HandParser;
use poker_tracker::stats::{
    COLUMNS, PlayerHandStats, STATS_VERSION, assign_positions, derive_hand_stats, position_label,
};
use support::{make_action, make_canonical_hand, make_player};

#[test]
fn positions_across_table_sizes_and_layouts() {
    assert_eq!(STATS_VERSION, 4);
    assert_eq!(COLUMNS.len(), 76);

    // 6 players: seats 1..=6, button 1, SB seat 2, BB seat 3
    let p6: Vec<Player> = (1..=6)
        .map(|s| make_player(s, &format!("P{s}"), dec!(10), s == 1))
        .collect();
    let a6 = vec![
        make_action(
            0,
            Street::Preflop,
            "P2",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "P3",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
    ];
    let h6 = make_canonical_hand(1, p6, a6, vec![], vec![("P3", dec!(0.15))]);
    let pos6 = assign_positions(&h6);
    assert_eq!(pos6.get(&1), Some(&0)); // BTN
    assert_eq!(pos6.get(&6), Some(&1)); // CO
    assert_eq!(pos6.get(&5), Some(&2)); // MP
    assert_eq!(pos6.get(&4), Some(&3)); // EP
    assert_eq!(pos6.get(&3), Some(&8)); // BB
    assert_eq!(pos6.get(&2), Some(&9)); // SB

    assert_eq!(position_label(0, 6), "BTN");
    assert_eq!(position_label(1, 6), "CO");
    assert_eq!(position_label(2, 6), "MP");
    assert_eq!(position_label(3, 6), "EP");
    assert_eq!(position_label(8, 6), "BB");
    assert_eq!(position_label(9, 6), "SB");

    // 5 players: seats 1..=5, button 1, SB seat 2, BB seat 3
    let p5: Vec<Player> = (1..=5)
        .map(|s| make_player(s, &format!("P{s}"), dec!(10), s == 1))
        .collect();
    let a5 = vec![
        make_action(
            0,
            Street::Preflop,
            "P2",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "P3",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
    ];
    let h5 = make_canonical_hand(1, p5, a5, vec![], vec![("P3", dec!(0.15))]);
    let pos5 = assign_positions(&h5);
    assert_eq!(pos5.get(&1), Some(&0)); // BTN
    assert_eq!(pos5.get(&5), Some(&1)); // CO
    assert_eq!(pos5.get(&4), Some(&2)); // MP
    assert_eq!(pos5.get(&3), Some(&8)); // BB
    assert_eq!(pos5.get(&2), Some(&9)); // SB

    // 4 players: seats 1..=4, button 1, SB seat 2, BB seat 3
    let p4: Vec<Player> = (1..=4)
        .map(|s| make_player(s, &format!("P{s}"), dec!(10), s == 1))
        .collect();
    let a4 = vec![
        make_action(
            0,
            Street::Preflop,
            "P2",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "P3",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
    ];
    let h4 = make_canonical_hand(1, p4, a4, vec![], vec![("P3", dec!(0.15))]);
    let pos4 = assign_positions(&h4);
    assert_eq!(pos4.get(&1), Some(&0)); // BTN
    assert_eq!(pos4.get(&4), Some(&1)); // CO
    assert_eq!(pos4.get(&3), Some(&8)); // BB
    assert_eq!(pos4.get(&2), Some(&9)); // SB

    // 3 players: seats 1..=3, button 1, SB seat 2, BB seat 3
    let p3: Vec<Player> = (1..=3)
        .map(|s| make_player(s, &format!("P{s}"), dec!(10), s == 1))
        .collect();
    let a3 = vec![
        make_action(
            0,
            Street::Preflop,
            "P2",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "P3",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
    ];
    let h3 = make_canonical_hand(1, p3, a3, vec![], vec![("P3", dec!(0.15))]);
    let pos3 = assign_positions(&h3);
    assert_eq!(pos3.get(&1), Some(&0)); // BTN
    assert_eq!(pos3.get(&3), Some(&8)); // BB
    assert_eq!(pos3.get(&2), Some(&9)); // SB

    // 2 players (heads-up): seats 1 and 2, button 1
    // Button posts SB (code 9), other posts BB (code 8)
    let p2: Vec<Player> = (1..=2)
        .map(|s| make_player(s, &format!("P{s}"), dec!(10), s == 1))
        .collect();
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
    ];
    let h2 = make_canonical_hand(1, p2, a2, vec![], vec![("P2", dec!(0.15))]);
    let pos2 = assign_positions(&h2);
    assert_eq!(pos2.get(&1), Some(&9)); // SB
    assert_eq!(pos2.get(&2), Some(&8)); // BB

    // Table with 9 seats: code 3 and 4 are MP
    assert_eq!(position_label(3, 9), "MP");
    assert_eq!(position_label(4, 9), "MP");
    assert_eq!(position_label(5, 9), "EP");
    assert_eq!(position_label(99, 6), "?");
}

#[test]
fn positions_with_extra_blinds_straddles_and_non_standard_layout() {
    // Extra big-blind poster keeps their seat's position (seat 4)
    let p6: Vec<Player> = (1..=6)
        .map(|s| make_player(s, &format!("P{s}"), dec!(10), s == 1))
        .collect();
    let a_extra_bb = vec![
        make_action(
            0,
            Street::Preflop,
            "P2",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "P3",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "P4",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
    ];
    let h_extra = make_canonical_hand(1, p6.clone(), a_extra_bb, vec![], vec![("P3", dec!(0.25))]);
    let pos_extra = assign_positions(&h_extra);
    assert_eq!(pos_extra.get(&4), Some(&3)); // Keeps seat 4's position (EP)

    // Missed-blind poster keeps their seat's position (seat 4)
    let a_dead = vec![
        make_action(
            0,
            Street::Preflop,
            "P2",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "P3",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "P4",
            ActionType::PostDeadBlind,
            dec!(0.10),
            dec!(0.00),
            false,
        ),
    ];
    let h_dead = make_canonical_hand(1, p6.clone(), a_dead, vec![], vec![("P3", dec!(0.25))]);
    let pos_dead = assign_positions(&h_dead);
    assert_eq!(pos_dead.get(&4), Some(&3));

    // Straddler keeps their seat's position (seat 4)
    let a_straddle = vec![
        make_action(
            0,
            Street::Preflop,
            "P2",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "P3",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "P4",
            ActionType::PostStraddle,
            dec!(0.20),
            dec!(0.20),
            false,
        ),
    ];
    let h_straddle =
        make_canonical_hand(1, p6.clone(), a_straddle, vec![], vec![("P3", dec!(0.35))]);
    let pos_straddle = assign_positions(&h_straddle);
    assert_eq!(pos_straddle.get(&4), Some(&3));

    // Non-standard layout: SB is posted by seat 4 (not seat 2 directly after button seat 1)
    let a_non_std = vec![
        make_action(
            0,
            Street::Preflop,
            "P4",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "P5",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
    ];
    let h_non_std = make_canonical_hand(1, p6, a_non_std, vec![], vec![("P5", dec!(0.15))]);
    let pos_non_std = assign_positions(&h_non_std);
    assert!(pos_non_std.is_empty());
    let stats = derive_hand_stats(&h_non_std);
    for s in stats {
        assert_eq!(s.position, None);
    }
}

#[test]
fn facts_and_counters_canonical() {
    let p = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "Villain", dec!(10), false),
    ];

    // VPIP ignores posts and checks
    let a_check = vec![
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
            "Villain",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Hero",
            ActionType::Call,
            dec!(0.05),
            dec!(0.10),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "Villain",
            ActionType::Check,
            Decimal::ZERO,
            dec!(0.10),
            false,
        ),
    ];
    let h_check = make_canonical_hand(1, p.clone(), a_check, vec![], vec![("Hero", dec!(0.20))]);
    let stats_check = derive_hand_stats(&h_check);
    assert_eq!(stats_check[0].p_vpip, 1); // Hero called
    assert_eq!(stats_check[1].p_vpip, 0); // Villain checked, VPIP is 0

    // Fold on the turn: t_saw = 1, r_saw = 0
    let a_turn_fold = vec![
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
            "Villain",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Hero",
            ActionType::Call,
            dec!(0.05),
            dec!(0.10),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "Villain",
            ActionType::Check,
            Decimal::ZERO,
            dec!(0.10),
            false,
        ),
        make_action(
            4,
            Street::Flop,
            "Villain",
            ActionType::Check,
            Decimal::ZERO,
            Decimal::ZERO,
            false,
        ),
        make_action(
            5,
            Street::Flop,
            "Hero",
            ActionType::Check,
            Decimal::ZERO,
            Decimal::ZERO,
            false,
        ),
        make_action(
            6,
            Street::Turn,
            "Villain",
            ActionType::Bet,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            7,
            Street::Turn,
            "Hero",
            ActionType::Fold,
            Decimal::ZERO,
            Decimal::ZERO,
            false,
        ),
    ];
    let card = poker_tracker::domain::Card::from_str("Ah").unwrap();
    let board = vec![CommunityCards {
        run_index: 0,
        flop: Some([card, card, card]),
        turn: Some(card),
        river: Some(card),
    }];
    let h_turn_fold = make_canonical_hand(
        1,
        p.clone(),
        a_turn_fold,
        board.clone(),
        vec![("Villain", dec!(0.30))],
    );
    let stats_turn_fold = derive_hand_stats(&h_turn_fold);
    assert_eq!(stats_turn_fold[0].f_saw, 1);
    assert_eq!(stats_turn_fold[0].t_saw, 1);
    assert_eq!(stats_turn_fold[0].r_saw, 0);
    assert_eq!(stats_turn_fold[1].r_saw, 1);

    // Preflop all-in run out to river: sd_went = 1 for both
    let a_all_in = vec![
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
            "Villain",
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
            dec!(9.95),
            dec!(10.00),
            true,
        ),
        make_action(
            3,
            Street::Preflop,
            "Villain",
            ActionType::Call,
            dec!(9.90),
            dec!(10.00),
            true,
        ),
    ];
    let h_all_in = make_canonical_hand(1, p.clone(), a_all_in, board, vec![("Hero", dec!(20.00))]);
    let stats_all_in = derive_hand_stats(&h_all_in);
    assert_eq!(stats_all_in[0].sd_went, 1);
    assert_eq!(stats_all_in[0].sd_won, 1);
    assert_eq!(stats_all_in[1].sd_went, 1);
    assert_eq!(stats_all_in[1].sd_won, 0);

    // Straddler walk: everyone folds to the straddler
    let p3 = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "SB", dec!(10), false),
        make_player(3, "Straddler", dec!(10), false),
    ];
    let a_straddle_walk = vec![
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
            "Straddler",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Hero",
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
            Decimal::ZERO,
            dec!(0.05),
            false,
        ),
        make_action(
            4,
            Street::Preflop,
            "Straddler",
            ActionType::Fold,
            Decimal::ZERO,
            dec!(0.10),
            false,
        ),
    ];
    let h_straddle_walk =
        make_canonical_hand(1, p3, a_straddle_walk, vec![], vec![("Hero", dec!(0.35))]);
    let stats_straddle_walk = derive_hand_stats(&h_straddle_walk);
    assert_eq!(stats_straddle_walk[0].p_walk, 1);
    assert_eq!(stats_straddle_walk[0].p_pfr_opp, 0);
    assert_eq!(stats_straddle_walk[0].won_hand, 1);
    assert_eq!(stats_straddle_walk[0].sd_went, 0);
}

#[test]
fn walks_and_pfr_opportunities_near_misses() {
    let p = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "Villain", dec!(10), false),
    ];

    // 1. BB checks an unraised pot: NOT a walk, and has p_pfr_opp = 1
    let a_bb_check = vec![
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
            "Villain",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Hero",
            ActionType::Call,
            dec!(0.05),
            dec!(0.10),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "Villain",
            ActionType::Check,
            Decimal::ZERO,
            dec!(0.10),
            false,
        ),
    ];
    let h_bb_check = make_canonical_hand(
        1,
        p.clone(),
        a_bb_check,
        vec![],
        vec![("Villain", dec!(0.20))],
    );
    let s_bb_check = derive_hand_stats(&h_bb_check);
    assert_eq!(s_bb_check[1].p_walk, 0, "BB checking is not a walk");
    assert_eq!(s_bb_check[1].p_pfr_opp, 1, "BB had raise opportunity");

    // 2. BB raises and opponent folds: NOT a walk
    let a_bb_raise = vec![
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
            "Villain",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Hero",
            ActionType::Call,
            dec!(0.05),
            dec!(0.10),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "Villain",
            ActionType::Raise,
            dec!(0.20),
            dec!(0.30),
            false,
        ),
        make_action(
            4,
            Street::Preflop,
            "Hero",
            ActionType::Fold,
            Decimal::ZERO,
            dec!(0.10),
            false,
        ),
    ];
    let h_bb_raise = make_canonical_hand(
        1,
        p.clone(),
        a_bb_raise,
        vec![],
        vec![("Villain", dec!(0.40))],
    );
    let s_bb_raise = derive_hand_stats(&h_bb_raise);
    assert_eq!(s_bb_raise[1].p_walk, 0, "BB raising is not a walk");
    assert_eq!(s_bb_raise[1].p_pfr, 1);
    assert_eq!(s_bb_raise[1].p_pfr_opp, 1);

    // 3. Player whose every preflop decision faces a bet taking all remaining stack: p_pfr_opp = 0
    let p_short = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "Short", dec!(1), false),
    ];
    let a_short = vec![
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
            "Short",
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
            dec!(4.95),
            dec!(5.00),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "Short",
            ActionType::Call,
            dec!(0.90),
            dec!(1.00),
            true,
        ),
    ];
    let h_short = make_canonical_hand(1, p_short, a_short, vec![], vec![("Hero", dec!(2.00))]);
    let s_short = derive_hand_stats(&h_short);
    assert_eq!(
        s_short[1].p_pfr_opp, 0,
        "facing an all-in bet leaves no raise opportunity"
    );

    // 4. Decision comes when every other player has folded or is all-in: p_pfr_opp = 0 even with chips behind
    let p_deep = vec![
        make_player(1, "AllIn1", dec!(1), false),
        make_player(2, "AllIn2", dec!(1), false),
        make_player(3, "Deep", dec!(50), true),
    ];
    let a_deep = vec![
        make_action(
            0,
            Street::Preflop,
            "AllIn1",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "AllIn2",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "AllIn1",
            ActionType::Raise,
            dec!(0.95),
            dec!(1.00),
            true,
        ),
        make_action(
            3,
            Street::Preflop,
            "AllIn2",
            ActionType::Call,
            dec!(0.90),
            dec!(1.00),
            true,
        ),
        make_action(
            4,
            Street::Preflop,
            "Deep",
            ActionType::Call,
            dec!(1.00),
            dec!(1.00),
            false,
        ),
    ];
    let h_deep = make_canonical_hand(1, p_deep, a_deep, vec![], vec![("Deep", dec!(3.00))]);
    let s_deep = derive_hand_stats(&h_deep);
    assert_eq!(
        s_deep[2].p_pfr_opp, 0,
        "no other active opponent has chips, so cannot raise"
    );

    // 5. Player who folds with a raise available: p_pfr_opp = 1
    let a_fold_raise = vec![
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
            "Villain",
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
            "Villain",
            ActionType::Fold,
            Decimal::ZERO,
            dec!(0.10),
            false,
        ),
    ];
    let h_fold_raise = make_canonical_hand(1, p, a_fold_raise, vec![], vec![("Hero", dec!(0.40))]);
    let s_fold_raise = derive_hand_stats(&h_fold_raise);
    assert_eq!(
        s_fold_raise[1].p_pfr_opp, 1,
        "fold with chips behind and raise available has p_pfr_opp = 1"
    );

    // Invariants check: p_pfr <= p_pfr_opp and p_vpip <= hands - p_walk
    for s in s_bb_check
        .iter()
        .chain(&s_bb_raise)
        .chain(&s_short)
        .chain(&s_deep)
        .chain(&s_fold_raise)
    {
        assert!(s.p_pfr <= s.p_pfr_opp, "p_pfr must be <= p_pfr_opp");
        assert!(
            s.p_vpip <= s.hands - s.p_walk,
            "p_vpip must be <= hands - p_walk"
        );
    }
}

#[test]
fn real_walks_from_fixtures() {
    let parser = gg();

    // 1. WALK (HD140646565): the BB 10a37d12
    let hand_walk = parser.parse_hand(WALK).unwrap();
    let stats_walk = derive_hand_stats(&hand_walk);
    let mut walk_count = 0;
    for s in &stats_walk {
        if s.player_name == "10a37d12" {
            assert_eq!(s.p_walk, 1);
            assert_eq!(s.hands, 1);
            assert_eq!(s.won_hand, 1);
            assert_eq!(s.p_vpip, 0);
            assert_eq!(s.p_pfr_opp, 0);
            assert_eq!(s.p_pfr, 0);
            assert_eq!(s.sd_went, 0);
            assert_eq!(s.f_saw, 0);
            walk_count += 1;
        } else {
            assert_eq!(s.p_walk, 0);
            assert_eq!(s.p_pfr_opp, 1, "opponents in WALK had raise opportunity");
        }
    }
    assert_eq!(walk_count, 1);

    // 2. HU_PREFLOP_FOLD (HD145412494): the BB 445e6868
    let hand_hu = parser.parse_hand(HU_PREFLOP_FOLD).unwrap();
    let stats_hu = derive_hand_stats(&hand_hu);
    let mut hu_walk_count = 0;
    for s in &stats_hu {
        if s.player_name == "445e6868" {
            assert_eq!(s.p_walk, 1);
            assert_eq!(s.hands, 1);
            assert_eq!(s.won_hand, 1);
            assert_eq!(s.p_vpip, 0);
            assert_eq!(s.p_pfr_opp, 0);
            hu_walk_count += 1;
        } else {
            assert_eq!(s.p_walk, 0);
            assert_eq!(s.p_pfr_opp, 1);
        }
    }
    assert_eq!(hu_walk_count, 1);

    // 3. STRADDLE_BB_WALK (HD145385508): BB and straddler 5daacec9
    let hand_straddle = parser.parse_hand(STRADDLE_BB_WALK).unwrap();
    let stats_straddle = derive_hand_stats(&hand_straddle);
    let mut straddle_walk_count = 0;
    for s in &stats_straddle {
        if s.player_name == "5daacec9" {
            assert_eq!(s.p_walk, 1);
            assert_eq!(s.hands, 1);
            assert_eq!(s.won_hand, 1);
            assert_eq!(s.p_vpip, 0);
            assert_eq!(s.p_pfr_opp, 0);
            straddle_walk_count += 1;
        } else {
            assert_eq!(s.p_walk, 0);
            assert_eq!(s.p_pfr_opp, 1);
        }
    }
    assert_eq!(straddle_walk_count, 1);
}

#[test]
fn worked_examples_from_stats_spec() {
    let parser = gg();

    fn assert_exact_non_zero(stats: &PlayerHandStats, expected: &[(&str, i64)]) {
        let expected_map: HashMap<&str, i64> = expected.iter().copied().collect();
        for &col in COLUMNS {
            let val = stats.get(col).unwrap();
            if let Some(&exp) = expected_map.get(col) {
                assert_eq!(
                    val, exp,
                    "player {} column {} expected {} got {}",
                    stats.player_name, col, exp, val
                );
            } else {
                assert_eq!(
                    val, 0,
                    "player {} column {} expected 0 got {}",
                    stats.player_name, col, val
                );
            }
        }
    }

    // Worked Example 1: HD140650557 (SUMMARY_SHOW_TWO_CARDS). Money (stats.md §7): Hero and the
    // BB each invested $2.50, so rake $0.24 and jackpot $0.10 split equally between them.
    let h1 = parser.parse_hand(SUMMARY_SHOW_TWO_CARDS).unwrap();
    let s1 = derive_hand_stats(&h1);
    let map1: HashMap<String, _> = s1.into_iter().map(|s| (s.player_name.clone(), s)).collect();

    let co = &map1["6570ba77"];
    assert_eq!(co.position, Some(1));
    assert_exact_non_zero(
        co,
        &[
            ("hands", 1),
            ("p_pfr_opp", 1),
            ("p_folds", 1),
            ("p_rfi_opp", 1),
            ("p_steal_opp", 1),
            ("ev_equity_den", 1),
        ],
    );

    let btn = &map1["4af6a555"];
    assert_eq!(btn.position, Some(0));
    assert_exact_non_zero(
        btn,
        &[
            ("hands", 1),
            ("p_pfr_opp", 1),
            ("p_folds", 1),
            ("p_rfi_opp", 1),
            ("p_steal_opp", 1),
            ("ev_equity_den", 1),
        ],
    );

    let hero = &map1["Hero"];
    assert_eq!(hero.position, Some(9));
    assert_exact_non_zero(
        hero,
        &[
            ("hands", 1),
            ("p_vpip", 1),
            ("p_pfr_opp", 1),
            ("p_pfr", 1),
            ("p_raises", 1),
            ("p_rfi_opp", 1),
            ("p_rfi", 1),
            ("p_steal_opp", 1),
            ("p_steal", 1),
            ("f_saw", 1),
            ("f_cbet_opp", 1),
            ("f_cbet", 1),
            ("f_bets", 1),
            ("f_calls", 1),
            ("t_saw", 1),
            ("t_cbet_opp", 1),
            ("t_checks", 1),
            ("t_calls", 1),
            ("r_saw", 1),
            ("r_checks", 1),
            ("r_folds", 1),
            ("rake_attributed_micros", 120_000), // $0.24 / 2
            ("fees_attributed_micros", 50_000),  // $0.10 / 2
            ("blinds_posted_micros", 50_000),    // posted the $0.05 SB
            ("ev_equity_den", 1),
            ("ev_net_micros", -2_500_000),
        ],
    );

    let bb = &map1["2d5ce8a9"];
    assert_eq!(bb.position, Some(8));
    assert_exact_non_zero(
        bb,
        &[
            ("hands", 1),
            ("p_vpip", 1),
            ("p_pfr_opp", 1),
            ("p_calls", 1),
            ("p_3bet_opp", 1),
            ("p_bb_steal_faced", 1),
            ("f_saw", 1),
            ("f_cbet_faced", 1),
            ("f_cbet_raise", 1),
            ("f_raises", 1),
            ("t_saw", 1),
            ("t_bets", 1),
            ("r_saw", 1),
            ("r_bets", 1),
            ("won_hand", 1),
            ("f_saw_won", 1),
            ("rake_attributed_micros", 120_000), // $0.24 / 2
            ("fees_attributed_micros", 50_000),  // $0.10 / 2
            ("blinds_posted_micros", 100_000),   // posted the $0.10 BB
            ("ev_equity_den", 1),
            ("ev_net_micros", 2_160_000),
        ],
    );

    // Worked Example 2: HD145385563 (STRADDLE_UTG_OPTION). Money (stats.md §7): rake $0.04 by
    // largest remainder over invested 0.05 / 0.10 / 0.39 / 0.39; the SB's remainder is largest.
    let h2 = parser.parse_hand(STRADDLE_UTG_OPTION).unwrap();
    let s2 = derive_hand_stats(&h2);
    let map2: HashMap<String, _> = s2.into_iter().map(|s| (s.player_name.clone(), s)).collect();

    let sb2 = &map2["494b476f"];
    assert_eq!(sb2.position, Some(9));
    assert_exact_non_zero(
        sb2,
        &[
            ("hands", 1),
            ("p_pfr_opp", 1),
            ("p_folds", 1),
            ("rake_attributed_micros", 2_151), // floor 2,150 + the leftover micro-unit
            ("blinds_posted_micros", 50_000),  // posted the $0.05 SB
            ("ev_equity_den", 1),
            ("ev_net_micros", -50_000),
        ],
    );

    let bb2 = &map2["c89b4da3"];
    assert_eq!(bb2.position, Some(8));
    assert_exact_non_zero(
        bb2,
        &[
            ("hands", 1),
            ("p_pfr_opp", 1),
            ("p_folds", 1),
            ("rake_attributed_micros", 4_301), // floor(40,000 × 0.10 / 0.93)
            ("blinds_posted_micros", 100_000), // posted the $0.10 BB
            ("ev_equity_den", 1),
            ("ev_net_micros", -100_000),
        ],
    );

    let straddler = &map2["5daacec9"];
    assert_eq!(straddler.position, Some(3)); // EP
    assert_exact_non_zero(
        straddler,
        &[
            ("hands", 1),
            ("p_pfr_opp", 1),
            ("p_checks", 1),
            ("f_saw", 1),
            ("f_bets", 1),
            ("f_folds", 1),
            ("rake_attributed_micros", 16_774), // floor(40,000 × 0.39 / 0.93)
            ("straddles_posted_micros", 200_000), // posted the $0.20 straddle
            ("ev_equity_den", 1),
            ("ev_net_micros", -390_000),
        ],
    );

    let mp = &map2["adbf678c"];
    assert_eq!(mp.position, Some(2)); // MP
    assert_exact_non_zero(
        mp,
        &[
            ("hands", 1),
            ("p_vpip", 1),
            ("p_pfr_opp", 1),
            ("p_calls", 1),
            ("p_rfi_opp", 1),
            ("f_saw", 1),
            ("f_raises", 1),
            ("won_hand", 1),
            ("f_saw_won", 1),
            ("rake_attributed_micros", 16_774), // floor(40,000 × 0.39 / 0.93)
            ("ev_equity_den", 1),
            ("ev_net_micros", 500_000),
        ],
    );

    let hero2 = &map2["Hero"];
    assert_eq!(hero2.position, Some(1)); // CO
    assert_exact_non_zero(
        hero2,
        &[
            ("hands", 1),
            ("p_pfr_opp", 1),
            ("p_folds", 1),
            ("ev_equity_den", 1),
        ],
    );

    let btn2 = &map2["4e268376"];
    assert_eq!(btn2.position, Some(0)); // BTN
    assert_exact_non_zero(
        btn2,
        &[
            ("hands", 1),
            ("p_pfr_opp", 1),
            ("p_folds", 1),
            ("ev_equity_den", 1),
        ],
    );

    // Worked Example 3: HD140650645 (first hand of SAMPLE_SESSION). Money: the SB and BB each
    // invested $0.10, so the $0.01 rake splits 5,000 / 5,000.
    let first_sample_hand = parser
        .parse_session(SAMPLE_SESSION)
        .next()
        .unwrap()
        .1
        .unwrap();
    let s3 = derive_hand_stats(&first_sample_hand);
    let map3: HashMap<String, _> = s3.into_iter().map(|s| (s.player_name.clone(), s)).collect();

    let bb3 = &map3["4af6a555"];
    assert_eq!(bb3.position, Some(8));
    assert_exact_non_zero(
        bb3,
        &[
            ("hands", 1),
            ("p_pfr_opp", 1),
            ("p_checks", 1),
            ("f_saw", 1),
            ("f_checks", 1),
            ("t_saw", 1),
            ("t_checks", 1),
            ("r_saw", 1),
            ("r_checks", 1),
            ("sd_went", 1),
            ("rake_attributed_micros", 5_000), // $0.01 / 2
            ("blinds_posted_micros", 100_000), // posted the $0.10 BB
            ("ev_equity_den", 1),
            ("ev_net_micros", -100_000),
        ],
    );

    let hero3 = &map3["Hero"];
    assert_eq!(hero3.position, Some(0)); // BTN
    assert_exact_non_zero(
        hero3,
        &[
            ("hands", 1),
            ("p_pfr_opp", 1),
            ("p_folds", 1),
            ("p_rfi_opp", 1),
            ("p_steal_opp", 1),
            ("ev_equity_den", 1),
        ],
    );

    let sb3 = &map3["2d5ce8a9"];
    assert_eq!(sb3.position, Some(9)); // SB
    assert_exact_non_zero(
        sb3,
        &[
            ("hands", 1),
            ("p_vpip", 1),
            ("p_pfr_opp", 1),
            ("p_calls", 1),
            ("p_rfi_opp", 1),
            ("p_steal_opp", 1),
            ("f_saw", 1),
            ("f_checks", 1),
            ("t_saw", 1),
            ("t_checks", 1),
            ("r_saw", 1),
            ("r_checks", 1),
            ("sd_went", 1),
            ("sd_won", 1),
            ("won_hand", 1),
            ("f_saw_won", 1),
            ("rake_attributed_micros", 5_000), // $0.01 / 2
            ("blinds_posted_micros", 50_000),  // posted the $0.05 SB
            ("ev_equity_den", 1),
            ("ev_net_micros", 90_000),
        ],
    );
}

#[test]
fn attributed_money_parts_sum_exactly_to_hand_deductions_over_corpus() {
    use poker_tracker::domain::{DeductionKind, decimal_to_micro_units};

    let parser = gg();
    let mut all_hands = Vec::new();

    let v35_path = std::path::Path::new("tests/fixtures/ggpoker/NLHRed15_2026-08-21.txt");
    if v35_path.exists() {
        let content = std::fs::read_to_string(v35_path).unwrap();
        for (_, res) in parser.parse_session(&content) {
            all_hands.push(res.unwrap());
        }
    }

    let v36_dir = std::path::Path::new("tests/fixtures/ggpoker/v3_6");
    if v36_dir.exists() {
        let mut paths: Vec<_> = std::fs::read_dir(v36_dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "txt"))
            .collect();
        paths.sort();
        for p in paths {
            let content = std::fs::read_to_string(p).unwrap();
            for (_, res) in parser.parse_session(&content) {
                all_hands.push(res.unwrap());
            }
        }
    }

    assert_eq!(all_hands.len(), 1283);

    for hand in &all_hands {
        let stats = derive_hand_stats(hand);

        let hand_rake_dec = hand
            .outcome
            .checked_deductions_of(DeductionKind::Rake)
            .unwrap_or(Decimal::ZERO);
        let hand_rake_micros = decimal_to_micro_units(hand_rake_dec).unwrap();

        let hand_fees_dec = hand
            .outcome
            .deductions
            .iter()
            .filter(|d| d.kind != DeductionKind::Rake)
            .try_fold(Decimal::ZERO, |acc, d| acc.checked_add(d.amount))
            .unwrap_or(Decimal::ZERO);
        let hand_fees_micros = decimal_to_micro_units(hand_fees_dec).unwrap();

        let sum_rake_attr: i64 = stats.iter().map(|s| s.rake_attributed_micros).sum();
        let sum_fees_attr: i64 = stats.iter().map(|s| s.fees_attributed_micros).sum();

        assert_eq!(
            sum_rake_attr, hand_rake_micros,
            "hand {} rake attribution sum mismatch: expected {}, got {}",
            hand.header.site_hand_id, hand_rake_micros, sum_rake_attr
        );
        assert_eq!(
            sum_fees_attr, hand_fees_micros,
            "hand {} fees attribution sum mismatch: expected {}, got {}",
            hand.header.site_hand_id, hand_fees_micros, sum_fees_attr
        );
    }
}

#[test]
fn position_labels_switch_between_six_and_seven_seats() {
    // docs/stats.md §2: with max_seats <= 6, code 2 is MP and codes 3-7 are EP; with
    // max_seats >= 7, codes 2-4 are MP and 5-7 are EP. The blinds, BTN and CO never change.
    for max_seats in 2..=10u8 {
        let expected_3_and_4 = if max_seats <= 6 { "EP" } else { "MP" };
        let labels: Vec<&str> = (0..=9)
            .map(|code| position_label(code, max_seats))
            .collect();
        assert_eq!(
            labels,
            [
                "BTN",
                "CO",
                "MP",
                expected_3_and_4,
                expected_3_and_4,
                "EP",
                "EP",
                "EP",
                "BB",
                "SB"
            ],
            "max_seats {max_seats}"
        );
    }
}

#[test]
fn jackpot_and_other_deductions_are_split_once_as_one_sum() {
    use poker_tracker::domain::{DeductionKind, PotDeduction};

    // Three players invest 0.10 each; the hand has a Jackpot of 1 micro-unit and an Other
    // deduction of 1 micro-unit (docs/stats.md §6.1: one split of their sum). Splitting the sum
    // T = 2 by equal weights gives floor(2/3) = 0 each and two leftover micro-units, which go to
    // the two lowest seats: [1, 1, 0]. Two separate splits would give [2, 0, 0].
    let p: Vec<Player> = (1..=3)
        .map(|s| make_player(s, &format!("P{s}"), dec!(10), s == 1))
        .collect();
    let a = vec![
        make_action(
            0,
            Street::Preflop,
            "P2",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "P3",
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
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            3,
            Street::Preflop,
            "P2",
            ActionType::Call,
            dec!(0.05),
            dec!(0.10),
            false,
        ),
        make_action(
            4,
            Street::Preflop,
            "P3",
            ActionType::Check,
            dec!(0),
            dec!(0.10),
            false,
        ),
    ];
    let mut h = make_canonical_hand(1, p, a, vec![], vec![("P1", dec!(0.299998))]);
    h.outcome.total_pot = dec!(0.30);
    h.outcome.deductions = vec![
        PotDeduction {
            kind: DeductionKind::Jackpot,
            label: "Jackpot".to_string(),
            amount: dec!(0.000001),
        },
        PotDeduction {
            kind: DeductionKind::Other,
            label: "Other".to_string(),
            amount: dec!(0.000001),
        },
    ];
    let stats = derive_hand_stats(&h);
    let fees: Vec<i64> = stats.iter().map(|s| s.fees_attributed_micros).collect();
    let rake: Vec<i64> = stats.iter().map(|s| s.rake_attributed_micros).collect();
    assert_eq!(fees, [1, 1, 0]);
    assert_eq!(rake, [0, 0, 0]);
}

#[test]
fn a_hand_with_two_small_blind_posts_has_no_positions() {
    // docs/invariants.md §3 Positions: positions need exactly one small blind; otherwise every
    // player gets NULL. Seat 2 (left of the button) posts the small blind and seat 3 the big
    // blind, so the layout check alone would pass; seat 4 posts a second small blind.
    let p: Vec<Player> = (1..=4)
        .map(|s| make_player(s, &format!("P{s}"), dec!(10), s == 1))
        .collect();
    let post = |seq, who, kind, amount| {
        make_action(seq, Street::Preflop, who, kind, amount, amount, false)
    };
    let a = vec![
        post(0, "P2", ActionType::PostSmallBlind, dec!(0.05)),
        post(1, "P3", ActionType::PostBigBlind, dec!(0.10)),
        post(2, "P4", ActionType::PostSmallBlind, dec!(0.05)),
        make_action(
            3,
            Street::Preflop,
            "P4",
            ActionType::Fold,
            dec!(0),
            dec!(0.05),
            false,
        ),
        make_action(
            4,
            Street::Preflop,
            "P1",
            ActionType::Fold,
            dec!(0),
            dec!(0),
            false,
        ),
        make_action(
            5,
            Street::Preflop,
            "P2",
            ActionType::Fold,
            dec!(0),
            dec!(0.05),
            false,
        ),
    ];
    let h = make_canonical_hand(1, p, a, vec![], vec![("P3", dec!(0.20))]);
    assert!(assign_positions(&h).is_empty());
    assert!(derive_hand_stats(&h).iter().all(|s| s.position.is_none()));
}

#[test]
fn an_all_in_big_blind_who_never_acts_is_not_a_walk() {
    // docs/stats.md §3: a walk needs every other player to fold. Here the big blind's post is
    // their whole stack, so they never decide, but the small blind calls: not a walk.
    let p = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "Villain", dec!(0.10), false),
    ];
    let a = vec![
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
            "Villain",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            true,
        ),
        make_action(
            2,
            Street::Preflop,
            "Hero",
            ActionType::Call,
            dec!(0.05),
            dec!(0.10),
            false,
        ),
    ];
    let board = CommunityCards {
        run_index: 0,
        flop: Some([
            "2c".parse().unwrap(),
            "7d".parse().unwrap(),
            "9h".parse().unwrap(),
        ]),
        turn: Some("Ks".parse().unwrap()),
        river: Some("3s".parse().unwrap()),
    };
    let h = make_canonical_hand(1, p, a, vec![board], vec![("Hero", dec!(0.20))]);
    let stats = derive_hand_stats(&h);
    assert_eq!(stats.iter().map(|s| s.p_walk).collect::<Vec<_>>(), [0, 0]);
}
