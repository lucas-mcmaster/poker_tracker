//! Tests for derived statistics: positions, counters, near misses, and worked examples.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::HashMap;
use std::str::FromStr;

use chrono::NaiveDateTime;
use pretty_assertions::assert_eq;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use common::{
    HU_PREFLOP_FOLD, SAMPLE_SESSION, STRADDLE_BB_WALK, STRADDLE_UTG_OPTION, SUMMARY_SHOW_TWO_CARDS,
    WALK, gg,
};
use poker_tracker::domain::{
    ActionType, BettingLimit, CanonicalHand, CommunityCards, CurrencyCode, GameType, HandHeader,
    HandOutcome, Platform, Player, Street, StreetAction,
};
use poker_tracker::parser::HandParser;
use poker_tracker::stats::{
    COLUMNS, STATS_VERSION, assign_positions, derive_hand_stats, position_label,
};

fn make_player(seat: u8, name: &str, stack: Decimal, is_hero: bool) -> Player {
    Player {
        seat,
        name: name.to_string(),
        starting_stack: stack,
        hole_cards: [None, None],
        is_hero,
    }
}

fn make_action(
    seq: u16,
    street: Street,
    player: &str,
    act: ActionType,
    amount: Decimal,
    total_after: Decimal,
    is_all_in: bool,
) -> StreetAction {
    StreetAction {
        sequence: seq,
        street,
        player_name: player.to_string(),
        action_type: act,
        amount,
        street_total_after: total_after,
        is_all_in,
    }
}

fn make_canonical_hand(
    button_seat: u8,
    players: Vec<Player>,
    actions: Vec<StreetAction>,
    boards: Vec<CommunityCards>,
    winners: Vec<(&str, Decimal)>,
) -> CanonicalHand {
    let header = HandHeader {
        platform: Platform::GGPoker,
        site_hand_id: "HD_TEST".to_string(),
        game_type: GameType::Cash,
        limit: BettingLimit::NoLimit,
        currency: CurrencyCode::CAD,
        small_blind: dec!(0.05),
        big_blind: dec!(0.10),
        timestamp: NaiveDateTime::parse_from_str("2026-08-21 17:00:00", "%Y-%m-%d %H:%M:%S")
            .unwrap(),
        utc_offset_seconds: None,
        table_name: "TestTable".to_string(),
        max_seats: 6,
        button_seat,
    };
    let pot_winners = winners
        .into_iter()
        .map(|(name, amount)| poker_tracker::domain::PotWinner {
            player_name: name.to_string(),
            amount,
            run_index: 0,
        })
        .collect();
    CanonicalHand {
        header,
        players,
        actions,
        boards,
        outcome: HandOutcome {
            total_pot: dec!(1.0),
            deductions: Vec::new(),
            uncalled_returned: Decimal::ZERO,
            winners: pot_winners,
            adjustments: Vec::new(),
        },
    }
}

#[test]
fn positions_across_table_sizes_and_layouts() {
    assert_eq!(STATS_VERSION, 1);
    assert_eq!(COLUMNS.len(), 32);

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

    // Worked Example 1: HD140650557 (SUMMARY_SHOW_TWO_CARDS)
    let h1 = parser.parse_hand(SUMMARY_SHOW_TWO_CARDS).unwrap();
    let s1 = derive_hand_stats(&h1);
    let map1: HashMap<String, _> = s1.into_iter().map(|s| (s.player_name.clone(), s)).collect();

    let co = &map1["6570ba77"];
    assert_eq!(co.position, Some(1));
    assert_eq!(co.hands, 1);
    assert_eq!(co.p_pfr_opp, 1);
    assert_eq!(co.p_folds, 1);
    assert_eq!(co.p_vpip, 0);
    assert_eq!(co.p_pfr, 0);

    let btn = &map1["4af6a555"];
    assert_eq!(btn.position, Some(0));
    assert_eq!(btn.hands, 1);
    assert_eq!(btn.p_pfr_opp, 1);
    assert_eq!(btn.p_folds, 1);
    assert_eq!(btn.p_vpip, 0);

    let hero = &map1["Hero"];
    assert_eq!(hero.position, Some(9));
    assert_eq!(hero.hands, 1);
    assert_eq!(hero.p_vpip, 1);
    assert_eq!(hero.p_pfr_opp, 1);
    assert_eq!(hero.p_pfr, 1);
    assert_eq!(hero.p_raises, 1);
    assert_eq!(hero.f_saw, 1);
    assert_eq!(hero.f_bets, 1);
    assert_eq!(hero.f_calls, 1);
    assert_eq!(hero.t_saw, 1);
    assert_eq!(hero.t_checks, 1);
    assert_eq!(hero.t_calls, 1);
    assert_eq!(hero.r_saw, 1);
    assert_eq!(hero.r_checks, 1);
    assert_eq!(hero.r_folds, 1);

    let bb = &map1["2d5ce8a9"];
    assert_eq!(bb.position, Some(8));
    assert_eq!(bb.hands, 1);
    assert_eq!(bb.p_vpip, 1);
    assert_eq!(bb.p_pfr_opp, 1);
    assert_eq!(bb.p_calls, 1);
    assert_eq!(bb.f_saw, 1);
    assert_eq!(bb.f_raises, 1);
    assert_eq!(bb.t_saw, 1);
    assert_eq!(bb.t_bets, 1);
    assert_eq!(bb.r_saw, 1);
    assert_eq!(bb.r_bets, 1);
    assert_eq!(bb.won_hand, 1);
    assert_eq!(bb.f_saw_won, 1);

    // Worked Example 2: HD145385563 (STRADDLE_UTG_OPTION)
    let h2 = parser.parse_hand(STRADDLE_UTG_OPTION).unwrap();
    let s2 = derive_hand_stats(&h2);
    let map2: HashMap<String, _> = s2.into_iter().map(|s| (s.player_name.clone(), s)).collect();

    let sb2 = &map2["494b476f"];
    assert_eq!(sb2.position, Some(9));
    assert_eq!(sb2.hands, 1);
    assert_eq!(sb2.p_pfr_opp, 1);
    assert_eq!(sb2.p_folds, 1);

    let bb2 = &map2["c89b4da3"];
    assert_eq!(bb2.position, Some(8));
    assert_eq!(bb2.hands, 1);
    assert_eq!(bb2.p_pfr_opp, 1);
    assert_eq!(bb2.p_folds, 1);

    let straddler = &map2["5daacec9"];
    assert_eq!(straddler.position, Some(3)); // EP
    assert_eq!(straddler.hands, 1);
    assert_eq!(straddler.p_pfr_opp, 1);
    assert_eq!(straddler.p_checks, 1);
    assert_eq!(straddler.f_saw, 1);
    assert_eq!(straddler.f_bets, 1);
    assert_eq!(straddler.f_folds, 1);

    let mp = &map2["adbf678c"];
    assert_eq!(mp.position, Some(2)); // MP
    assert_eq!(mp.hands, 1);
    assert_eq!(mp.p_vpip, 1);
    assert_eq!(mp.p_pfr_opp, 1);
    assert_eq!(mp.p_calls, 1);
    assert_eq!(mp.f_saw, 1);
    assert_eq!(mp.f_raises, 1);
    assert_eq!(mp.won_hand, 1);
    assert_eq!(mp.f_saw_won, 1);

    let hero2 = &map2["Hero"];
    assert_eq!(hero2.position, Some(1)); // CO
    assert_eq!(hero2.hands, 1);
    assert_eq!(hero2.p_pfr_opp, 1);
    assert_eq!(hero2.p_folds, 1);

    let btn2 = &map2["4e268376"];
    assert_eq!(btn2.position, Some(0)); // BTN
    assert_eq!(btn2.hands, 1);
    assert_eq!(btn2.p_pfr_opp, 1);
    assert_eq!(btn2.p_folds, 1);

    // Worked Example 3: HD140650645 (first hand of SAMPLE_SESSION)
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
    assert_eq!(bb3.hands, 1);
    assert_eq!(bb3.p_pfr_opp, 1);
    assert_eq!(bb3.p_checks, 1);
    assert_eq!(bb3.f_saw, 1);
    assert_eq!(bb3.f_checks, 1);
    assert_eq!(bb3.t_saw, 1);
    assert_eq!(bb3.t_checks, 1);
    assert_eq!(bb3.r_saw, 1);
    assert_eq!(bb3.r_checks, 1);
    assert_eq!(bb3.sd_went, 1);
    assert_eq!(bb3.sd_won, 0);

    let hero3 = &map3["Hero"];
    assert_eq!(hero3.position, Some(0)); // BTN
    assert_eq!(hero3.hands, 1);
    assert_eq!(hero3.p_pfr_opp, 1);
    assert_eq!(hero3.p_folds, 1);

    let sb3 = &map3["2d5ce8a9"];
    assert_eq!(sb3.position, Some(9)); // SB
    assert_eq!(sb3.hands, 1);
    assert_eq!(sb3.p_vpip, 1);
    assert_eq!(sb3.p_pfr_opp, 1);
    assert_eq!(sb3.p_calls, 1);
    assert_eq!(sb3.f_saw, 1);
    assert_eq!(sb3.f_checks, 1);
    assert_eq!(sb3.t_saw, 1);
    assert_eq!(sb3.t_checks, 1);
    assert_eq!(sb3.r_saw, 1);
    assert_eq!(sb3.r_checks, 1);
    assert_eq!(sb3.sd_went, 1);
    assert_eq!(sb3.sd_won, 1);
    assert_eq!(sb3.won_hand, 1);
    assert_eq!(sb3.f_saw_won, 1);
}
