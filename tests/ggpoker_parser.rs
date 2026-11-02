//! GGPoker parser tests built on verbatim real hands, plus fault injection on those hands.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::str::FromStr;

use chrono::NaiveDate;
use pretty_assertions::assert_eq;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use common::{
    BODY_AND_SUMMARY_ONE_CARD, DEAD_MISSED_BLIND, DOUBLE_STRADDLE, EV_CASHOUT, EV_CASHOUT_CALLER,
    EV_CASHOUT_FLOP, EV_CASHOUT_RECEIVED, EXTRA_BIG_BLIND, EXTRA_BIG_BLIND_2, FIXTURE_CURRENCY,
    FOLDED_ONE_CARD_SHOW, FOUR_WAY_ALL_IN, HERO_SUMMARY_SHOW_ONE_CARD, HERO_SUMMARY_SHOW_TWO_CARDS,
    HERO_SUMMARY_SHOW_TWO_CARDS_2, HU_FLOP, HU_PREFLOP_FOLD, HU_SHOWDOWN, MISSED_AND_BIG_BLIND,
    MISSED_AND_BIG_BLIND_2, MISSED_BLIND_ONLY, MISSED_BLIND_ONLY_2, ONE_CARD_SHOW_AFTER_FOLD,
    RIT_THREE_TIMES_PREFLOP, RIT_TWICE_FLOP_ALL_IN, RIT_TWICE_FLOP_REAL, RIT_TWICE_TURN,
    SAMPLE_SESSION, STRADDLE_BB_WALK, STRADDLE_BUTTON, STRADDLE_SB_ALL_IN_POT, STRADDLE_UTG_OPTION,
    SUMMARY_SHOW_COLLECTED, SUMMARY_SHOW_ONE_CARD, SUMMARY_SHOW_TWO_CARDS,
    THREE_WAY_ALL_IN_SIDE_POT, UNCALLED_ALL_IN_SPLIT, WALK, gg,
};
use poker_tracker::domain::{
    ActionType, AdjustmentKind, CanonicalHand, Card, CommunityCards, CurrencyCode, DeductionKind,
    Platform, Player, Street,
};
use poker_tracker::parser::{GgPokerParser, HandParser, ParseError};

fn card(s: &str) -> Card {
    Card::from_str(s).unwrap()
}

fn parse(raw: &str) -> CanonicalHand {
    gg().parse_hand(raw).expect("real hand must parse")
}

fn parse_err(raw: &str) -> ParseError {
    gg().parse_hand(raw)
        .expect_err("corrupted hand must be rejected")
}

fn net_of(hand: &CanonicalHand, name: &str) -> Decimal {
    hand.checked_player_results()
        .unwrap()
        .into_iter()
        .find(|r| r.player_name == name)
        .unwrap()
        .net
}

fn first_hand(session: &str) -> &str {
    session.split("\n\n").next().unwrap()
}

// ---------------------------------------------------------------------------------------------
// Real hands
// ---------------------------------------------------------------------------------------------

#[test]
fn parses_sample_session_with_street_total_after_accuracy() {
    let parser = gg();
    let hands: Vec<_> = parser
        .parse_session(SAMPLE_SESSION)
        .map(|(_, r)| r)
        .collect::<Result<Vec<_>, _>>()
        .expect("both sample hands should parse cleanly");
    assert_eq!(hands.len(), 2);

    let h1 = &hands[0];
    assert_eq!(h1.header.platform, Platform::GGPoker);
    assert_eq!(h1.header.site_hand_id, "HD140650645");
    assert_eq!(h1.header.currency, FIXTURE_CURRENCY);
    assert_eq!(h1.header.utc_offset_seconds, None);
    assert_eq!(h1.header.small_blind, dec!(0.05));
    assert_eq!(h1.header.big_blind, dec!(0.1));
    assert_eq!(
        h1.header.timestamp,
        NaiveDate::from_ymd_opt(2026, 8, 21)
            .unwrap()
            .and_hms_opt(17, 5, 3)
            .unwrap()
    );
    let hero = h1.players.iter().find(|p| p.is_hero).unwrap();
    assert_eq!(hero.name, "Hero");
    assert_eq!(hero.hole_cards, [Some(card("5h")), Some(card("2s"))]);
    let villain = h1.players.iter().find(|p| p.name == "2d5ce8a9").unwrap();
    assert_eq!(villain.hole_cards, [Some(card("4d")), Some(card("8d"))]);

    assert_eq!(h1.actions[0].action_type, ActionType::PostSmallBlind);
    assert_eq!(h1.actions[0].street_total_after, dec!(0.05));
    assert_eq!(h1.actions[3].action_type, ActionType::Call);
    assert_eq!(h1.actions[3].amount, dec!(0.05));
    assert_eq!(h1.actions[3].street_total_after, dec!(0.10));
    assert_eq!(h1.outcome.total_pot, dec!(0.2));
    assert_eq!(h1.outcome.winners[0].player_name, "2d5ce8a9");
    assert_eq!(h1.outcome.winners[0].run_index, 0);
    assert_eq!(net_of(h1, "2d5ce8a9"), dec!(0.09));

    let h2 = &hands[1];
    let flop: Vec<_> = h2
        .actions
        .iter()
        .filter(|a| a.street == Street::Flop)
        .collect();
    assert_eq!(flop.len(), 8);
    // Hero checked, then "raises $1.74 to $2.22": incremental 2.22.
    assert_eq!(flop[3].amount, dec!(2.22));
    assert_eq!(flop[3].street_total_after, dec!(2.22));
    // 9f78ead0 bet 0.48, then "raises $7.08 to $9.3": incremental 8.82.
    assert_eq!(flop[5].amount, dec!(8.82));
    assert_eq!(flop[5].street_total_after, dec!(9.3));
    // Hero "raises $0.4 to $9.7 and is all-in": a short all-in raise.
    assert_eq!(flop[6].amount, dec!(7.48));
    assert!(flop[6].is_all_in);
    assert_eq!(h2.outcome.checked_total_collected(), Some(dec!(20.95)));
    assert_eq!(net_of(h2, "Hero"), dec!(10.95));
}

#[test]
fn summary_fees_become_labeled_site_neutral_deductions() {
    let h = parse(RIT_THREE_TIMES_PREFLOP);
    let deductions: Vec<_> = h
        .outcome
        .deductions
        .iter()
        .map(|d| (d.kind, d.label.as_str(), d.amount))
        .collect();
    assert_eq!(
        deductions,
        vec![
            (DeductionKind::Rake, "Rake", dec!(0.96)),
            (DeductionKind::Jackpot, "Jackpot", dec!(0.1)),
            (DeductionKind::Other, "Bingo", dec!(0)),
            (DeductionKind::Other, "Fortune", dec!(0)),
            (DeductionKind::Other, "Tax", dec!(0)),
        ]
    );
    assert_eq!(
        h.outcome.checked_deductions_of(DeductionKind::Rake),
        Some(dec!(0.96))
    );
}

#[test]
fn run_it_twice_after_flop_all_in() {
    let h = parse(RIT_TWICE_FLOP_ALL_IN);
    assert_eq!(h.run_count(), 2);
    let flop = Some([card("As"), card("Tc"), card("Js")]);
    assert_eq!(h.boards[0].run_index, 1);
    assert_eq!(h.boards[0].flop, flop);
    assert_eq!(h.boards[0].turn, Some(card("Qc")));
    assert_eq!(h.boards[0].river, Some(card("7s")));
    assert_eq!(h.boards[1].run_index, 2);
    assert_eq!(h.boards[1].flop, flop, "second run shares the flop");
    assert_eq!(h.boards[1].turn, Some(card("Ks")));
    assert_eq!(h.boards[1].river, Some(card("3h")));

    let runs: Vec<_> = h
        .outcome
        .winners
        .iter()
        .map(|w| (w.run_index, w.amount))
        .collect();
    assert_eq!(runs, vec![(1, dec!(1.28)), (2, dec!(1.27))]);
    let shove = h.actions.iter().find(|a| a.is_all_in).unwrap();
    assert_eq!((shove.street, shove.amount), (Street::Flop, dec!(0.06)));
    assert_eq!(net_of(&h, "Hero"), dec!(1.29));
    assert_eq!(net_of(&h, "45745f0e"), dec!(-1.26));
}

#[test]
fn run_it_three_times_after_preflop_all_in() {
    let h = parse(RIT_THREE_TIMES_PREFLOP);
    assert_eq!(h.run_count(), 3);
    assert!(h.boards.iter().all(|b| b.river.is_some()));
    assert_eq!(h.boards[2].flop, Some([card("8d"), card("5s"), card("Ks")]));
    let runs: Vec<_> = h
        .outcome
        .winners
        .iter()
        .map(|w| (w.run_index, w.player_name.as_str()))
        .collect();
    assert_eq!(
        runs,
        vec![(1, "3fe4ea4e"), (2, "345644aa"), (3, "345644aa")]
    );
    assert_eq!(h.outcome.uncalled_returned, dec!(14.56));
    assert_eq!(net_of(&h, "3fe4ea4e"), dec!(-3.57));
    assert_eq!(net_of(&h, "345644aa"), dec!(2.66));
}

#[test]
fn dead_missed_blind_enters_pot_but_not_street_commitment() {
    let h = parse(DEAD_MISSED_BLIND);
    let types: Vec<_> = h.actions.iter().take(4).map(|a| a.action_type).collect();
    assert_eq!(
        types,
        vec![
            ActionType::PostSmallBlind,
            ActionType::PostBigBlind,
            ActionType::PostDeadBlind,
            ActionType::PostBigBlind,
        ]
    );
    let dead = &h.actions[2];
    assert_eq!(
        (dead.player_name.as_str(), dead.amount),
        ("f618b25c", dec!(0.05))
    );
    assert_eq!(dead.street_total_after, dec!(0));
    let call = h
        .actions
        .iter()
        .find(|a| a.player_name == "f618b25c" && a.action_type == ActionType::Call)
        .unwrap();
    assert_eq!(
        (call.amount, call.street_total_after),
        (dec!(0.1), dec!(0.1))
    );
    assert_eq!(h.checked_total_contributed(), Some(dec!(1.65)));
    assert_eq!(net_of(&h, "f618b25c"), dec!(-0.15));
}

#[test]
fn ev_cashout_becomes_a_debit_adjustment() {
    let h = parse(EV_CASHOUT);
    assert_eq!(h.outcome.adjustments.len(), 1);
    let a = &h.outcome.adjustments[0];
    assert_eq!(
        (a.player_name.as_str(), a.kind, a.street, a.amount),
        (
            "1020a9fa",
            AdjustmentKind::EvCashoutRiskPaid,
            Street::Turn,
            dec!(-0.48)
        )
    );
    let r = h
        .checked_player_results()
        .unwrap()
        .into_iter()
        .find(|r| r.player_name == "1020a9fa")
        .unwrap();
    assert_eq!(r.invested, dec!(1.5));
    assert_eq!(r.won, dec!(2.81));
    assert_eq!(r.adjustments, dec!(-0.48));
    // Verified against the export: this player's next hand starts at 10.83 from 10.00.
    assert_eq!(r.net, dec!(0.83));
    // Pot accounting itself is unaffected by the adjustment.
    assert_eq!(h.outcome.total_pot, dec!(3.05));
}

#[test]
fn ev_cashout_received_becomes_a_credit_adjustment() {
    let h = parse(EV_CASHOUT_RECEIVED);
    assert_eq!(h.outcome.adjustments.len(), 1);
    let a = &h.outcome.adjustments[0];
    assert_eq!(
        (a.player_name.as_str(), a.kind, a.street, a.amount),
        (
            "81ae8569",
            AdjustmentKind::EvCashoutReceived,
            Street::Flop,
            dec!(6.87)
        )
    );
    assert_eq!(net_of(&h, "a2939c73"), dec!(4.93));
    assert_eq!(net_of(&h, "81ae8569"), dec!(1.87));
    assert_eq!(net_of(&h, "f729835e"), dec!(-0.40));
    assert_eq!(net_of(&h, "ab03e00b"), dec!(-0.05));
    assert_eq!(net_of(&h, "27a6d6a"), dec!(-0.10));
    assert_eq!(net_of(&h, "Hero"), dec!(0.00));
}

#[test]
fn ev_cashout_flop_and_caller_hands_parse() {
    let h_flop = parse(EV_CASHOUT_FLOP);
    assert_eq!(h_flop.outcome.adjustments.len(), 1);
    let a_flop = &h_flop.outcome.adjustments[0];
    assert_eq!(
        (
            a_flop.player_name.as_str(),
            a_flop.kind,
            a_flop.street,
            a_flop.amount
        ),
        (
            "62688fe9",
            AdjustmentKind::EvCashoutRiskPaid,
            Street::Flop,
            dec!(-0.55)
        )
    );
    assert_eq!(net_of(&h_flop, "62688fe9"), dec!(1.27));
    assert_eq!(net_of(&h_flop, "dc7d0eb4"), dec!(-1.96));

    let h_caller = parse(EV_CASHOUT_CALLER);
    assert_eq!(h_caller.outcome.adjustments.len(), 1);
    let a_caller = &h_caller.outcome.adjustments[0];
    assert_eq!(
        (
            a_caller.player_name.as_str(),
            a_caller.kind,
            a_caller.street,
            a_caller.amount
        ),
        (
            "c7bdd1e4",
            AdjustmentKind::EvCashoutRiskPaid,
            Street::Turn,
            dec!(-0.66)
        )
    );
    assert_eq!(net_of(&h_caller, "c7bdd1e4"), dec!(1.53));
    assert_eq!(net_of(&h_caller, "28501331"), dec!(-2.27));
}

#[test]
fn one_card_show_after_fold_is_recorded_as_partial_hole_cards() {
    let h = parse(ONE_CARD_SHOW_AFTER_FOLD);
    let p = h.players.iter().find(|p| p.name == "ff6f7609").unwrap();
    assert_eq!(p.hole_cards, [Some(card("Jc")), None]);
    let hero = h.players.iter().find(|p| p.is_hero).unwrap();
    assert_eq!(hero.hole_cards, [Some(card("As")), Some(card("3s"))]);
}

#[test]
fn three_way_all_in_with_side_pot_is_unlabeled_by_ggpoker() {
    let h = parse(THREE_WAY_ALL_IN_SIDE_POT);
    assert_eq!(h.outcome.total_pot, dec!(22.76));
    assert!(h.outcome.winners.iter().all(|w| w.run_index == 0));
    assert_eq!(net_of(&h, "c8f1ba23"), dec!(-10));
    assert_eq!(net_of(&h, "3fe4ea4e"), dec!(3.85));
    assert_eq!(net_of(&h, "6bf8f391"), dec!(5.11));
}

#[test]
fn uncalled_over_shove_and_split_pot() {
    let h = parse(UNCALLED_ALL_IN_SPLIT);
    let returned = h
        .actions
        .iter()
        .find(|a| a.action_type == ActionType::UncalledBetReturned)
        .unwrap();
    assert_eq!(
        (
            returned.player_name.as_str(),
            returned.amount,
            returned.street_total_after
        ),
        ("99a2b579", dec!(5.18), dec!(4.82))
    );
    assert_eq!(h.outcome.winners.len(), 2);
}

#[test]
fn heads_up_hands_parse_with_button_order_and_nets() {
    // 1. HU_PREFLOP_FOLD (HD145412494): Hero is button on seat 3, posts SB.
    let h1 = parse(HU_PREFLOP_FOLD);
    assert_eq!(h1.header.button_seat, 3);
    assert_eq!(h1.players.len(), 2);
    let preflop_vol1: Vec<_> = h1
        .actions
        .iter()
        .filter(|a| {
            a.street == Street::Preflop
                && !matches!(
                    a.action_type,
                    ActionType::PostSmallBlind
                        | ActionType::PostBigBlind
                        | ActionType::PostDeadBlind
                )
        })
        .collect();
    assert_eq!(preflop_vol1[0].player_name, "Hero");
    assert_eq!(preflop_vol1[0].action_type, ActionType::Fold);
    assert_eq!(net_of(&h1, "Hero"), dec!(-0.05));
    assert_eq!(net_of(&h1, "445e6868"), dec!(0.05));

    // 2. HU_SHOWDOWN (HD145412514): 445e6868 is button on seat 6, posts SB.
    let h2 = parse(HU_SHOWDOWN);
    assert_eq!(h2.header.button_seat, 6);
    assert_eq!(h2.players.len(), 2);
    let preflop_vol2: Vec<_> = h2
        .actions
        .iter()
        .filter(|a| {
            a.street == Street::Preflop
                && !matches!(
                    a.action_type,
                    ActionType::PostSmallBlind
                        | ActionType::PostBigBlind
                        | ActionType::PostDeadBlind
                )
        })
        .collect();
    assert_eq!(preflop_vol2[0].player_name, "445e6868");
    assert_eq!(preflop_vol2[0].action_type, ActionType::Call);
    assert_eq!(preflop_vol2[1].player_name, "Hero");
    assert_eq!(preflop_vol2[1].action_type, ActionType::Check);
    // BB acts first postflop
    for street in [Street::Flop, Street::Turn, Street::River] {
        let street_actions: Vec<_> = h2.actions.iter().filter(|a| a.street == street).collect();
        assert_eq!(street_actions[0].player_name, "Hero");
    }
    assert_eq!(net_of(&h2, "Hero"), dec!(-0.20));
    assert_eq!(net_of(&h2, "445e6868"), dec!(0.18));

    // 3. HU_FLOP (HD140650285): 9f78ead0 is button on seat 1, posts SB.
    let h3 = parse(HU_FLOP);
    assert_eq!(h3.header.button_seat, 1);
    assert_eq!(h3.players.len(), 2);
    let preflop_vol3: Vec<_> = h3
        .actions
        .iter()
        .filter(|a| {
            a.street == Street::Preflop
                && !matches!(
                    a.action_type,
                    ActionType::PostSmallBlind
                        | ActionType::PostBigBlind
                        | ActionType::PostDeadBlind
                )
        })
        .collect();
    assert_eq!(preflop_vol3[0].player_name, "9f78ead0");
    assert_eq!(preflop_vol3[0].action_type, ActionType::Call);
    assert_eq!(preflop_vol3[1].player_name, "Hero");
    assert_eq!(preflop_vol3[1].action_type, ActionType::Check);
    // BB acts first on flop
    let flop_actions: Vec<_> = h3
        .actions
        .iter()
        .filter(|a| a.street == Street::Flop)
        .collect();
    assert_eq!(flop_actions[0].player_name, "Hero");
    assert_eq!(net_of(&h3, "Hero"), dec!(0.18));
    assert_eq!(net_of(&h3, "9f78ead0"), dec!(-0.20));
}

#[test]
fn heads_up_summary_label_refusals_and_ring_regression() {
    // M6.4: Add (button) to seat 6's summary line
    let with_button = HU_SHOWDOWN.replace(
        "Seat 6: 445e6868 (small blind)",
        "Seat 6: 445e6868 (button) (small blind)",
    );
    assert!(matches!(
        parse_err(&with_button),
        ParseError::SummaryMismatch { .. }
    ));
    let with_button_after = HU_SHOWDOWN.replace(
        "Seat 6: 445e6868 (small blind)",
        "Seat 6: 445e6868 (small blind) (button)",
    );
    assert!(matches!(
        parse_err(&with_button_after),
        ParseError::SummaryMismatch { .. }
    ));

    // M6.4: Swap (small blind) and (big blind)
    let swapped_blinds = HU_SHOWDOWN
        .replace("Hero (big blind)", "Hero (small blind)")
        .replace("445e6868 (small blind)", "445e6868 (big blind)");
    assert!(matches!(
        parse_err(&swapped_blinds),
        ParseError::SummaryMismatch { .. }
    ));

    // M6.4: Change the table line to Seat #3 is the button
    let wrong_button = HU_SHOWDOWN.replace("Seat #6 is the button", "Seat #3 is the button");
    assert!(matches!(
        parse_err(&wrong_button),
        ParseError::SummaryMismatch { .. }
    ));

    // M6.5 Ring regression: a ring-game hand whose button line has (button) removed is still refused
    let ring_hand = first_hand(SAMPLE_SESSION);
    assert!(ring_hand.contains("Seat 4: Hero (button)"));
    let missing_btn = ring_hand.replace("Seat 4: Hero (button)", "Seat 4: Hero");
    assert!(matches!(
        parse_err(&missing_btn),
        ParseError::SummaryMismatch { .. }
    ));
}

#[test]
fn uncontested_summary_shows_parse_known_cards_and_nets() {
    let p_of = |hand: &CanonicalHand, name: &str| -> Player {
        hand.players
            .iter()
            .find(|p| p.name == name)
            .cloned()
            .unwrap()
    };

    // 1. SUMMARY_SHOW_ONE_CARD (HD140656328): opponent shows [2c] and won ($3.66)
    let h1 = parse(SUMMARY_SHOW_ONE_CARD);
    assert_eq!(p_of(&h1, "76fd4643").hole_cards, [Some(card("2c")), None]);
    assert_eq!(
        p_of(&h1, "Hero").hole_cards,
        [Some(card("2s")), Some(card("Qs"))]
    );
    assert_eq!(p_of(&h1, "9f795b5b").hole_cards, [None, None]);
    assert_eq!(p_of(&h1, "24a1a9e0").hole_cards, [None, None]);
    assert_eq!(p_of(&h1, "b29fca73").hole_cards, [None, None]);
    assert_eq!(net_of(&h1, "Hero"), dec!(0.00));
    assert_eq!(net_of(&h1, "76fd4643"), dec!(1.76));
    assert_eq!(net_of(&h1, "9f795b5b"), dec!(-1.90));
    assert_eq!(net_of(&h1, "24a1a9e0"), dec!(-0.05));
    assert_eq!(net_of(&h1, "b29fca73"), dec!(-0.10));

    // 2. HERO_SUMMARY_SHOW_ONE_CARD (HD140647731): Hero shows [Ts] and won ($1.25)
    let h2 = parse(HERO_SUMMARY_SHOW_ONE_CARD);
    assert_eq!(
        p_of(&h2, "Hero").hole_cards,
        [Some(card("Ts")), Some(card("Tc"))]
    );
    assert_eq!(p_of(&h2, "62688fe9").hole_cards, [None, None]);
    assert_eq!(p_of(&h2, "dc7d0eb4").hole_cards, [None, None]);
    assert_eq!(p_of(&h2, "1fba9f17").hole_cards, [None, None]);
    assert_eq!(p_of(&h2, "aa56bdcc").hole_cards, [None, None]);
    assert_eq!(p_of(&h2, "f0d370b7").hole_cards, [None, None]);
    assert_eq!(net_of(&h2, "Hero"), dec!(0.67));
    assert_eq!(net_of(&h2, "62688fe9"), dec!(-0.05));
    assert_eq!(net_of(&h2, "dc7d0eb4"), dec!(-0.10));
    assert_eq!(net_of(&h2, "1fba9f17"), dec!(0.00));
    assert_eq!(net_of(&h2, "aa56bdcc"), dec!(0.00));
    assert_eq!(net_of(&h2, "f0d370b7"), dec!(-0.58));

    // 3. BODY_AND_SUMMARY_ONE_CARD (HD140646055): opponent shows [6s] in body and summary
    let h3 = parse(BODY_AND_SUMMARY_ONE_CARD);
    assert_eq!(p_of(&h3, "dcd1bbfb").hole_cards, [Some(card("6s")), None]);
    assert_eq!(
        p_of(&h3, "Hero").hole_cards,
        [Some(card("Qd")), Some(card("Qc"))]
    );
    assert_eq!(p_of(&h3, "174d6de3").hole_cards, [None, None]);
    assert_eq!(p_of(&h3, "53be55c0").hole_cards, [None, None]);
    assert_eq!(p_of(&h3, "ed5884a").hole_cards, [None, None]);
    assert_eq!(p_of(&h3, "a2886689").hole_cards, [None, None]);
    assert_eq!(net_of(&h3, "Hero"), dec!(-1.20));
    assert_eq!(net_of(&h3, "174d6de3"), dec!(0.00));
    assert_eq!(net_of(&h3, "53be55c0"), dec!(-0.05));
    assert_eq!(net_of(&h3, "ed5884a"), dec!(-0.10));
    assert_eq!(net_of(&h3, "a2886689"), dec!(0.00));
    assert_eq!(net_of(&h3, "dcd1bbfb"), dec!(1.23));

    // 4. SUMMARY_SHOW_TWO_CARDS (HD140650557): opponent shows [Qs Jh] and won ($4.66)
    let h4 = parse(SUMMARY_SHOW_TWO_CARDS);
    assert_eq!(
        p_of(&h4, "2d5ce8a9").hole_cards,
        [Some(card("Qs")), Some(card("Jh"))]
    );
    assert_eq!(
        p_of(&h4, "Hero").hole_cards,
        [Some(card("Js")), Some(card("Ac"))]
    );
    assert_eq!(p_of(&h4, "6570ba77").hole_cards, [None, None]);
    assert_eq!(p_of(&h4, "4af6a555").hole_cards, [None, None]);
    assert_eq!(net_of(&h4, "Hero"), dec!(-2.50));
    assert_eq!(net_of(&h4, "2d5ce8a9"), dec!(2.16));
    assert_eq!(net_of(&h4, "6570ba77"), dec!(0.00));
    assert_eq!(net_of(&h4, "4af6a555"), dec!(0.00));

    // 5. SUMMARY_SHOW_COLLECTED (HD140652448): opponent shows [Qs Ah] and collected ($0.53)
    let h5 = parse(SUMMARY_SHOW_COLLECTED);
    assert_eq!(
        p_of(&h5, "2ff4bf29").hole_cards,
        [Some(card("Qs")), Some(card("Ah"))]
    );
    assert_eq!(
        p_of(&h5, "Hero").hole_cards,
        [Some(card("Kh")), Some(card("3c"))]
    );
    assert_eq!(p_of(&h5, "f82517").hole_cards, [None, None]);
    assert_eq!(p_of(&h5, "2ad6949d").hole_cards, [None, None]);
    assert_eq!(p_of(&h5, "c7bdd1e4").hole_cards, [None, None]);
    assert_eq!(p_of(&h5, "edba49df").hole_cards, [None, None]);
    assert_eq!(net_of(&h5, "2ff4bf29"), dec!(0.33));
    assert_eq!(net_of(&h5, "f82517"), dec!(-0.05));
    assert_eq!(net_of(&h5, "2ad6949d"), dec!(-0.10));
    assert_eq!(net_of(&h5, "c7bdd1e4"), dec!(0.00));
    assert_eq!(net_of(&h5, "edba49df"), dec!(-0.20));
    assert_eq!(net_of(&h5, "Hero"), dec!(0.00));
}

#[test]
fn uncontested_summary_shows_refusals() {
    // 1. A summary card that is on the board -> ConflictingHoleCards or DuplicateCard
    // Board is [6h 7h Qd]; seat 2 shows [6h] instead of [2c]
    let board_conflict = SUMMARY_SHOW_ONE_CARD.replace(
        "Seat 2: 76fd4643 showed [2c] and won ($3.66)",
        "Seat 2: 76fd4643 showed [6h] and won ($3.66)",
    );
    assert!(matches!(
        parse_err(&board_conflict),
        ParseError::ConflictingHoleCards { .. } | ParseError::DuplicateCard { .. }
    ));

    // 2. A Hero summary card Hero was not dealt -> ConflictingHoleCards
    // Hero was dealt [Ts Tc]; summary shows [Ah]
    let hero_conflict = HERO_SUMMARY_SHOW_ONE_CARD.replace(
        "Seat 4: Hero showed [Ts] and won ($1.25)",
        "Seat 4: Hero showed [Ah] and won ($1.25)",
    );
    assert!(matches!(
        parse_err(&hero_conflict),
        ParseError::ConflictingHoleCards { ref player_name } if player_name == "Hero"
    ));

    // 3. ` with a pair of Aces` appended to a one-card summary show -> InvalidCardSyntax or UnrecognizedLine
    let with_desc = SUMMARY_SHOW_ONE_CARD.replace(
        "Seat 2: 76fd4643 showed [2c] and won ($3.66)",
        "Seat 2: 76fd4643 showed [2c] and won ($3.66) with a pair of Aces",
    );
    assert!(matches!(
        parse_err(&with_desc),
        ParseError::InvalidCardSyntax { .. } | ParseError::UnrecognizedLine { .. }
    ));

    // 4. `collected` in a hand with a flop, or `won` in a hand without one, on both a showed line and a plain line -> SummaryMismatch
    // 4a. Showed line with flop, using `collected`
    let flop_showed_collected = SUMMARY_SHOW_ONE_CARD.replace(
        "Seat 2: 76fd4643 showed [2c] and won ($3.66)",
        "Seat 2: 76fd4643 showed [2c] and collected ($3.66)",
    );
    assert!(matches!(
        parse_err(&flop_showed_collected),
        ParseError::SummaryMismatch { .. }
    ));

    // 4b. Showed line without flop, using `won`
    let preflop_showed_won = SUMMARY_SHOW_COLLECTED.replace(
        "Seat 1: 2ff4bf29 (button) showed [Qs Ah] and collected ($0.53)",
        "Seat 1: 2ff4bf29 (button) showed [Qs Ah] and won ($0.53)",
    );
    assert!(matches!(
        parse_err(&preflop_showed_won),
        ParseError::SummaryMismatch { .. }
    ));

    // 4c. Plain line with flop, using `collected`
    let flop_plain_collected = HU_FLOP.replace(
        "Seat 4: Hero (big blind) won ($0.38)",
        "Seat 4: Hero (big blind) collected ($0.38)",
    );
    assert!(matches!(
        parse_err(&flop_plain_collected),
        ParseError::SummaryMismatch { .. }
    ));

    // 4d. Plain line without flop, using `won`
    let preflop_plain_won = HU_PREFLOP_FOLD.replace(
        "Seat 6: 445e6868 (big blind) collected ($0.1)",
        "Seat 6: 445e6868 (big blind) won ($0.1)",
    );
    assert!(matches!(
        parse_err(&preflop_plain_won),
        ParseError::SummaryMismatch { .. }
    ));

    // 5. Three cards -> refused (InvalidCardSyntax)
    let three_cards_1 = SUMMARY_SHOW_ONE_CARD.replace(
        "Seat 2: 76fd4643 showed [2c] and won ($3.66)",
        "Seat 2: 76fd4643 showed [2c 3c 4c] and won ($3.66)",
    );
    assert!(matches!(
        parse_err(&three_cards_1),
        ParseError::InvalidCardSyntax { .. }
    ));
    let three_cards_2 = SUMMARY_SHOW_TWO_CARDS.replace(
        "Seat 5: 2d5ce8a9 (big blind) showed [Qs Jh] and won ($4.66)",
        "Seat 5: 2d5ce8a9 (big blind) showed [Qs Jh 2d] and won ($4.66)",
    );
    assert!(matches!(
        parse_err(&three_cards_2),
        ParseError::InvalidCardSyntax { .. }
    ));

    // 6. The uncontested form (ShowedUncontested) on a player in a contested showdown -> SummaryMismatch
    let uncontested_in_showdown = HU_SHOWDOWN.replace(
        "Seat 6: 445e6868 (small blind) showed [4c Tc] and won ($0.38) with a pair of Fours",
        "Seat 6: 445e6868 (small blind) showed [4c Tc] and won ($0.38)",
    );
    assert!(matches!(
        parse_err(&uncontested_in_showdown),
        ParseError::SummaryMismatch { .. }
    ));
}

#[test]
fn straddle_hands_parse_actions_and_nets() {
    let check_actions =
        |hand: &CanonicalHand, expected: &[(&str, ActionType, Decimal, Decimal)]| {
            assert_eq!(hand.actions.len(), expected.len(), "action count mismatch");
            for (i, (act, (name, atype, amt, total))) in
                hand.actions.iter().zip(expected.iter()).enumerate()
            {
                assert_eq!(
                    (
                        act.player_name.as_str(),
                        act.action_type,
                        act.amount,
                        act.street_total_after
                    ),
                    (*name, *atype, *amt, *total),
                    "action {i} mismatch"
                );
            }
        };

    // 1. STRADDLE_UTG_OPTION (HD145385563)
    let h1 = parse(STRADDLE_UTG_OPTION);
    check_actions(
        &h1,
        &[
            (
                "494b476f",
                ActionType::PostSmallBlind,
                dec!(0.05),
                dec!(0.05),
            ),
            ("c89b4da3", ActionType::PostBigBlind, dec!(0.1), dec!(0.1)),
            ("5daacec9", ActionType::PostStraddle, dec!(0.2), dec!(0.2)),
            ("adbf678c", ActionType::Call, dec!(0.2), dec!(0.2)),
            ("Hero", ActionType::Fold, dec!(0), dec!(0)),
            ("4e268376", ActionType::Fold, dec!(0), dec!(0)),
            ("494b476f", ActionType::Fold, dec!(0), dec!(0.05)),
            ("c89b4da3", ActionType::Fold, dec!(0), dec!(0.1)),
            ("5daacec9", ActionType::Check, dec!(0), dec!(0.2)),
            ("5daacec9", ActionType::Bet, dec!(0.19), dec!(0.19)),
            ("adbf678c", ActionType::Raise, dec!(0.82), dec!(0.82)),
            ("5daacec9", ActionType::Fold, dec!(0), dec!(0.19)),
            (
                "adbf678c",
                ActionType::UncalledBetReturned,
                dec!(0.63),
                dec!(0.19),
            ),
        ],
    );
    assert_eq!(net_of(&h1, "494b476f"), dec!(-0.05));
    assert_eq!(net_of(&h1, "c89b4da3"), dec!(-0.10));
    assert_eq!(net_of(&h1, "5daacec9"), dec!(-0.39));
    assert_eq!(net_of(&h1, "adbf678c"), dec!(0.50));
    assert_eq!(net_of(&h1, "Hero"), dec!(0.00));
    assert_eq!(net_of(&h1, "4e268376"), dec!(0.00));

    // 2. STRADDLE_SB_ALL_IN_POT (HD145385586)
    let h2 = parse(STRADDLE_SB_ALL_IN_POT);
    check_actions(
        &h2,
        &[
            (
                "5daacec9",
                ActionType::PostSmallBlind,
                dec!(0.05),
                dec!(0.05),
            ),
            ("adbf678c", ActionType::PostBigBlind, dec!(0.1), dec!(0.1)),
            ("5daacec9", ActionType::PostStraddle, dec!(0.15), dec!(0.2)),
            ("adbf678c", ActionType::Call, dec!(0.1), dec!(0.2)),
            ("Hero", ActionType::Fold, dec!(0), dec!(0)),
            ("4e268376", ActionType::Raise, dec!(0.75), dec!(0.75)),
            ("494b476f", ActionType::Fold, dec!(0), dec!(0)),
            ("c89b4da3", ActionType::Fold, dec!(0), dec!(0)),
            ("5daacec9", ActionType::Call, dec!(0.55), dec!(0.75)),
            ("adbf678c", ActionType::Fold, dec!(0), dec!(0.2)),
            ("5daacec9", ActionType::Check, dec!(0), dec!(0)),
            ("4e268376", ActionType::Bet, dec!(1.15), dec!(1.15)),
            ("5daacec9", ActionType::Call, dec!(1.15), dec!(1.15)),
            ("5daacec9", ActionType::Check, dec!(0), dec!(0)),
            ("4e268376", ActionType::Bet, dec!(2.65), dec!(2.65)),
            ("5daacec9", ActionType::Raise, dec!(11.15), dec!(11.15)),
            ("4e268376", ActionType::Call, dec!(8.5), dec!(11.15)),
        ],
    );
    assert_eq!(h2.actions[15].is_all_in, true);
    assert_eq!(net_of(&h2, "494b476f"), dec!(0.00));
    assert_eq!(net_of(&h2, "c89b4da3"), dec!(0.00));
    assert_eq!(net_of(&h2, "5daacec9"), dec!(12.15));
    assert_eq!(net_of(&h2, "adbf678c"), dec!(-0.20));
    assert_eq!(net_of(&h2, "Hero"), dec!(0.00));
    assert_eq!(net_of(&h2, "4e268376"), dec!(-13.05));

    // 3. STRADDLE_BB_WALK (HD145385508)
    let h3 = parse(STRADDLE_BB_WALK);
    check_actions(
        &h3,
        &[
            (
                "c89b4da3",
                ActionType::PostSmallBlind,
                dec!(0.05),
                dec!(0.05),
            ),
            ("5daacec9", ActionType::PostBigBlind, dec!(0.1), dec!(0.1)),
            ("5daacec9", ActionType::PostStraddle, dec!(0.1), dec!(0.2)),
            ("adbf678c", ActionType::Fold, dec!(0), dec!(0)),
            ("Hero", ActionType::Fold, dec!(0), dec!(0)),
            ("4e268376", ActionType::Fold, dec!(0), dec!(0)),
            ("494b476f", ActionType::Fold, dec!(0), dec!(0)),
            ("c89b4da3", ActionType::Fold, dec!(0), dec!(0.05)),
            (
                "5daacec9",
                ActionType::UncalledBetReturned,
                dec!(0.15),
                dec!(0.05),
            ),
        ],
    );
    assert_eq!(net_of(&h3, "494b476f"), dec!(0.00));
    assert_eq!(net_of(&h3, "c89b4da3"), dec!(-0.05));
    assert_eq!(net_of(&h3, "5daacec9"), dec!(0.05));
    assert_eq!(net_of(&h3, "adbf678c"), dec!(0.00));
    assert_eq!(net_of(&h3, "Hero"), dec!(0.00));
    assert_eq!(net_of(&h3, "4e268376"), dec!(0.00));

    // 4. STRADDLE_BUTTON (HD145385522)
    let h4 = parse(STRADDLE_BUTTON);
    check_actions(
        &h4,
        &[
            (
                "adbf678c",
                ActionType::PostSmallBlind,
                dec!(0.05),
                dec!(0.05),
            ),
            ("Hero", ActionType::PostBigBlind, dec!(0.1), dec!(0.1)),
            ("5daacec9", ActionType::PostStraddle, dec!(0.2), dec!(0.2)),
            ("adbf678c", ActionType::Raise, dec!(0.41), dec!(0.46)),
            ("Hero", ActionType::Fold, dec!(0), dec!(0.1)),
            ("4e268376", ActionType::Fold, dec!(0), dec!(0)),
            ("494b476f", ActionType::Call, dec!(0.46), dec!(0.46)),
            ("c89b4da3", ActionType::Call, dec!(0.46), dec!(0.46)),
            ("5daacec9", ActionType::Call, dec!(0.26), dec!(0.46)),
            ("adbf678c", ActionType::Check, dec!(0), dec!(0)),
            ("494b476f", ActionType::Check, dec!(0), dec!(0)),
            ("c89b4da3", ActionType::Check, dec!(0), dec!(0)),
            ("5daacec9", ActionType::Check, dec!(0), dec!(0)),
            ("adbf678c", ActionType::Bet, dec!(1.3), dec!(1.3)),
            ("494b476f", ActionType::Fold, dec!(0), dec!(0)),
            ("c89b4da3", ActionType::Fold, dec!(0), dec!(0)),
            ("5daacec9", ActionType::Fold, dec!(0), dec!(0)),
            (
                "adbf678c",
                ActionType::UncalledBetReturned,
                dec!(1.3),
                dec!(0),
            ),
        ],
    );
    assert_eq!(net_of(&h4, "494b476f"), dec!(-0.46));
    assert_eq!(net_of(&h4, "c89b4da3"), dec!(-0.46));
    assert_eq!(net_of(&h4, "5daacec9"), dec!(-0.46));
    assert_eq!(net_of(&h4, "adbf678c"), dec!(1.39));
    assert_eq!(net_of(&h4, "Hero"), dec!(-0.10));
    assert_eq!(net_of(&h4, "4e268376"), dec!(0.00));

    // 5. DOUBLE_STRADDLE (HD145385574)
    let h5 = parse(DOUBLE_STRADDLE);
    check_actions(
        &h5,
        &[
            (
                "c89b4da3",
                ActionType::PostSmallBlind,
                dec!(0.05),
                dec!(0.05),
            ),
            ("5daacec9", ActionType::PostBigBlind, dec!(0.1), dec!(0.1)),
            ("5daacec9", ActionType::PostStraddle, dec!(0.1), dec!(0.2)),
            ("494b476f", ActionType::PostStraddle, dec!(0.4), dec!(0.4)),
            ("c89b4da3", ActionType::Fold, dec!(0), dec!(0.05)),
            ("5daacec9", ActionType::Call, dec!(0.2), dec!(0.4)),
            ("adbf678c", ActionType::Call, dec!(0.4), dec!(0.4)),
            ("Hero", ActionType::Raise, dec!(1.8), dec!(1.8)),
            ("4e268376", ActionType::Fold, dec!(0), dec!(0)),
            ("494b476f", ActionType::Fold, dec!(0), dec!(0.4)),
            ("5daacec9", ActionType::Fold, dec!(0), dec!(0.4)),
            ("adbf678c", ActionType::Fold, dec!(0), dec!(0.4)),
            (
                "Hero",
                ActionType::UncalledBetReturned,
                dec!(1.4),
                dec!(0.4),
            ),
        ],
    );
    assert_eq!(net_of(&h5, "494b476f"), dec!(-0.40));
    assert_eq!(net_of(&h5, "c89b4da3"), dec!(-0.05));
    assert_eq!(net_of(&h5, "5daacec9"), dec!(-0.40));
    assert_eq!(net_of(&h5, "adbf678c"), dec!(-0.40));
    assert_eq!(net_of(&h5, "Hero"), dec!(1.17));
    assert_eq!(net_of(&h5, "4e268376"), dec!(0.00));
}

#[test]
fn walk_returns_small_blind_and_awards_big_blind() {
    let h = parse(WALK);
    assert!(h.boards.is_empty());
    assert_eq!(h.run_count(), 0);
    assert_eq!(net_of(&h, "10a37d12"), dec!(0.05));
    assert_eq!(net_of(&h, "99a2b579"), dec!(-0.05));
}

#[test]
fn crlf_session_parses_and_reports_block_start_lines() {
    let crlf = SAMPLE_SESSION.replace('\n', "\r\n");
    let parser = gg();
    let blocks: Vec<_> = parser.parse_session(&crlf).collect();
    assert_eq!(blocks.len(), 2);
    assert!(blocks.iter().all(|(_, r)| r.is_ok()));
    assert_eq!(blocks[0].0.start_line, 1);
    let expected_second = SAMPLE_SESSION
        .lines()
        .position(|l| l.starts_with("Poker Hand #HD140650460"))
        .unwrap()
        + 1;
    assert_eq!(blocks[1].0.start_line, expected_second);
    assert_eq!(
        parser.peek_hand_id(blocks[1].0.text).as_deref(),
        Some("HD140650460")
    );
}

#[test]
fn currency_comes_from_the_configured_source_not_the_symbol() {
    for currency in [CurrencyCode::CAD, CurrencyCode::USD, CurrencyCode::EUR] {
        let parser = GgPokerParser::new(currency);
        assert_eq!(parser.currency(), currency);
        assert_eq!(parser.parse_hand(WALK).unwrap().header.currency, currency);
    }
    assert_eq!(gg().describe(), "GGPoker (CAD)");
}

#[test]
fn recognizes_ggpoker_exports_only() {
    let parser = gg();
    assert!(parser.recognizes(SAMPLE_SESSION));
    assert!(parser.recognizes(&format!("\u{feff}\n\n{WALK}")));
    // Unsupported GGPoker games are still claimed (and rejected per hand).
    assert!(parser.recognizes(&WALK.replace("Hold'em No Limit", "PLO")));
    assert!(!parser.recognizes(
        "PokerStars Hand #254000000000: Hold'em No Limit ($0.05/$0.10 USD) - 2026/08/21 17:05:03 ET"
    ));
    assert!(!parser.recognizes("random notes\nPoker Hand #HD1: ..."));
    assert!(!parser.recognizes(""));
}

#[test]
fn v3_6_single_hand_regression_fixtures() {
    let parse_cards = |s: &str| -> Vec<Card> { s.split_whitespace().map(card).collect() };
    let board_cards = |b: &CommunityCards| -> Vec<Card> { b.cards().collect() };

    // 1. RIT_TWICE_TURN (HD140644794) - Run it twice from the turn
    let h1 = parse(RIT_TWICE_TURN);
    assert_eq!(net_of(&h1, "Hero"), dec!(0.00));
    assert_eq!(net_of(&h1, "174d6de3"), dec!(10.76));
    assert_eq!(net_of(&h1, "53be55c0"), dec!(-11.26));
    assert_eq!(net_of(&h1, "9b8067e5"), dec!(-0.50));
    assert_eq!(net_of(&h1, "a2886689"), dec!(-0.10));
    assert_eq!(net_of(&h1, "6e766d6a"), dec!(0.00));
    assert_eq!(h1.boards.len(), 2);
    assert_eq!(board_cards(&h1.boards[0]), parse_cards("9h Kc 6d Js Ks"));
    assert_eq!(board_cards(&h1.boards[1]), parse_cards("9h Kc 6d Js Kh"));

    // 2. RIT_TWICE_FLOP_REAL (HD145411928) - Run it twice from the flop
    let h2 = parse(RIT_TWICE_FLOP_REAL);
    assert_eq!(net_of(&h2, "f6bdaa79"), dec!(0.00));
    assert_eq!(net_of(&h2, "7dfadc4b"), dec!(-0.41));
    assert_eq!(net_of(&h2, "Hero"), dec!(-0.42));
    assert_eq!(net_of(&h2, "d1a82b6a"), dec!(-0.05));
    assert_eq!(net_of(&h2, "7e0ec17"), dec!(-0.10));
    assert_eq!(h2.boards.len(), 2);
    assert_eq!(board_cards(&h2.boards[0]), parse_cards("4s 3c As Kd Kc"));
    assert_eq!(board_cards(&h2.boards[1]), parse_cards("4s 3c As Ac Qd"));

    // 3. FOUR_WAY_ALL_IN (HD145383901) - Four-way all-in, one player collects twice
    let h3 = parse(FOUR_WAY_ALL_IN);
    assert_eq!(net_of(&h3, "c4aace87"), dec!(0.00));
    assert_eq!(net_of(&h3, "a021b3ae"), dec!(2.27));
    assert_eq!(net_of(&h3, "6f9cac4b"), dec!(-0.10));
    assert_eq!(net_of(&h3, "267294a2"), dec!(0.00));
    assert_eq!(net_of(&h3, "Hero"), dec!(-6.87));
    assert_eq!(net_of(&h3, "6f724592"), dec!(3.65));
    assert_eq!(h3.boards.len(), 1);
    assert_eq!(board_cards(&h3.boards[0]), parse_cards("7d As 4c 3c 7s"));

    // 4. MISSED_AND_BIG_BLIND (HD140647389) - One player posts missed blind + big blind
    let h4 = parse(MISSED_AND_BIG_BLIND);
    assert_eq!(net_of(&h4, "5257a38"), dec!(-0.15));
    assert_eq!(net_of(&h4, "1fba9f17"), dec!(0.00));
    assert_eq!(net_of(&h4, "Hero"), dec!(-0.05));
    assert_eq!(net_of(&h4, "aa56bdcc"), dec!(0.19));
    assert_eq!(net_of(&h4, "f0d370b7"), dec!(0.00));
    assert_eq!(h4.boards.len(), 1);
    assert_eq!(board_cards(&h4.boards[0]), parse_cards("Ts Ah 4s 7d"));

    // 5. MISSED_AND_BIG_BLIND_2 (HD140645359) - Same, second sample
    let h5 = parse(MISSED_AND_BIG_BLIND_2);
    assert_eq!(net_of(&h5, "Hero"), dec!(-0.50));
    assert_eq!(net_of(&h5, "174d6de3"), dec!(-0.05));
    assert_eq!(net_of(&h5, "53be55c0"), dec!(0.64));
    assert_eq!(net_of(&h5, "ed5884a"), dec!(0.00));
    assert_eq!(net_of(&h5, "a2886689"), dec!(0.00));
    assert_eq!(net_of(&h5, "dcd1bbfb"), dec!(-0.15));
    assert!(h5.boards.is_empty());

    // 6. MISSED_BLIND_ONLY (HD145411947) - Missed blind alone
    let h6 = parse(MISSED_BLIND_ONLY);
    assert_eq!(net_of(&h6, "f6bdaa79"), dec!(-0.10));
    assert_eq!(net_of(&h6, "7dfadc4b"), dec!(7.70));
    assert_eq!(net_of(&h6, "Hero"), dec!(0.00));
    assert_eq!(net_of(&h6, "1105877e"), dec!(-8.49));
    assert_eq!(net_of(&h6, "d1a82b6a"), dec!(0.00));
    assert_eq!(net_of(&h6, "7e0ec17"), dec!(-0.05));
    assert_eq!(h6.boards.len(), 1);
    assert_eq!(board_cards(&h6.boards[0]), parse_cards("2d Ad 2h Ah Kc"));

    // 7. MISSED_BLIND_ONLY_2 (HD140654659) - Missed blind alone, second sample
    let h7 = parse(MISSED_BLIND_ONLY_2);
    assert_eq!(net_of(&h7, "Hero"), dec!(-0.49));
    assert_eq!(net_of(&h7, "8786307e"), dec!(0.00));
    assert_eq!(net_of(&h7, "64955198"), dec!(-0.05));
    assert_eq!(net_of(&h7, "dcc19732"), dec!(0.64));
    assert_eq!(net_of(&h7, "32cff61e"), dec!(-0.05));
    assert_eq!(net_of(&h7, "fe18d44c"), dec!(-0.10));
    assert_eq!(h7.boards.len(), 1);
    assert_eq!(board_cards(&h7.boards[0]), parse_cards("6s Kd 9d 4s 4c"));

    // 8. EXTRA_BIG_BLIND (HD140653739) - Extra big blind
    let h8 = parse(EXTRA_BIG_BLIND);
    assert_eq!(net_of(&h8, "3c71a362"), dec!(0.00));
    assert_eq!(net_of(&h8, "d43e85b9"), dec!(-0.05));
    assert_eq!(net_of(&h8, "Hero"), dec!(-0.10));
    assert_eq!(net_of(&h8, "34eb6ca7"), dec!(0.00));
    assert_eq!(net_of(&h8, "3724b5ab"), dec!(0.00));
    assert_eq!(net_of(&h8, "f2c7beb1"), dec!(0.14));
    assert_eq!(h8.boards.len(), 1);
    assert_eq!(board_cards(&h8.boards[0]), parse_cards("Qd 7h As Ad 5c"));

    // 9. EXTRA_BIG_BLIND_2 (HD140653777) - Extra big blind, second sample
    let h9 = parse(EXTRA_BIG_BLIND_2);
    assert_eq!(net_of(&h9, "3c71a362"), dec!(0.00));
    assert_eq!(net_of(&h9, "82ad4a3e"), dec!(0.33));
    assert_eq!(net_of(&h9, "Hero"), dec!(0.00));
    assert_eq!(net_of(&h9, "34eb6ca7"), dec!(-0.05));
    assert_eq!(net_of(&h9, "3724b5ab"), dec!(-0.10));
    assert_eq!(net_of(&h9, "f2c7beb1"), dec!(-0.20));
    assert_eq!(h9.boards.len(), 1);
    assert_eq!(board_cards(&h9.boards[0]), parse_cards("7s As 8d 3s"));

    // 10. FOLDED_ONE_CARD_SHOW (HD145383947) - Folded one-card show plus duplicate shows
    let h10 = parse(FOLDED_ONE_CARD_SHOW);
    assert_eq!(net_of(&h10, "c4aace87"), dec!(0.61));
    assert_eq!(net_of(&h10, "a021b3ae"), dec!(0.00));
    assert_eq!(net_of(&h10, "6f9cac4b"), dec!(-0.51));
    assert_eq!(net_of(&h10, "267294a2"), dec!(-0.05));
    assert_eq!(net_of(&h10, "Hero"), dec!(-0.10));
    assert_eq!(net_of(&h10, "6f724592"), dec!(0.00));
    assert_eq!(h10.boards.len(), 1);
    assert_eq!(board_cards(&h10.boards[0]), parse_cards("Td Ad 6d 3s 2d"));

    // 11. HERO_SUMMARY_SHOW_TWO_CARDS (HD140645051) - Hero shows both cards in the summary only
    let h11 = parse(HERO_SUMMARY_SHOW_TWO_CARDS);
    assert_eq!(net_of(&h11, "Hero"), dec!(1.08));
    assert_eq!(net_of(&h11, "174d6de3"), dec!(0.00));
    assert_eq!(net_of(&h11, "53be55c0"), dec!(-0.05));
    assert_eq!(net_of(&h11, "9b8067e5"), dec!(-0.10));
    assert_eq!(net_of(&h11, "a2886689"), dec!(0.00));
    assert_eq!(net_of(&h11, "dcd1bbfb"), dec!(-1.04));
    assert_eq!(h11.boards.len(), 1);
    assert_eq!(board_cards(&h11.boards[0]), parse_cards("6h Qd As"));

    // 12. HERO_SUMMARY_SHOW_TWO_CARDS_2 (HD145383504) - Same, second sample
    let h12 = parse(HERO_SUMMARY_SHOW_TWO_CARDS_2);
    assert_eq!(net_of(&h12, "c4aace87"), dec!(-0.05));
    assert_eq!(net_of(&h12, "a021b3ae"), dec!(-1.10));
    assert_eq!(net_of(&h12, "be5746f6"), dec!(-0.50));
    assert_eq!(net_of(&h12, "2ee85b67"), dec!(0.00));
    assert_eq!(net_of(&h12, "Hero"), dec!(1.90));
    assert_eq!(net_of(&h12, "6f724592"), dec!(-0.50));
    assert_eq!(h12.boards.len(), 1);
    assert_eq!(board_cards(&h12.boards[0]), parse_cards("8h 4c 9h 5d Ah"));
}

// ---------------------------------------------------------------------------------------------
// Fault injection on real hands
// ---------------------------------------------------------------------------------------------

#[test]
fn rejects_summary_arithmetic_mismatch() {
    let bad = first_hand(SAMPLE_SESSION).replace("Total pot $0.2 |", "Total pot $0.25 |");
    assert!(matches!(
        parse_err(&bad),
        ParseError::MismatchedPotAccounting { .. }
    ));
}

#[test]
fn rejects_pot_that_disagrees_with_actions() {
    let bad = first_hand(SAMPLE_SESSION).replace(
        "Total pot $0.2 | Rake $0.01",
        "Total pot $0.25 | Rake $0.06",
    );
    assert!(matches!(
        parse_err(&bad),
        ParseError::PotContributionMismatch { .. }
    ));
}

#[test]
fn rejects_invalid_card_syntax() {
    let bad = first_hand(SAMPLE_SESSION).replace("Dealt to Hero [5h 2s]", "Dealt to Hero [1h 2s]");
    assert!(matches!(
        parse_err(&bad),
        ParseError::InvalidCardSyntax { .. }
    ));
}

#[test]
fn rejects_duplicate_physical_card() {
    let bad = first_hand(SAMPLE_SESSION).replace("3c 9h 7h", "5h 9h 7h");
    assert_eq!(
        parse_err(&bad),
        ParseError::DuplicateCard {
            card: "5h".to_string()
        }
    );
}

#[test]
fn rejects_missing_street_header() {
    let bad = first_hand(SAMPLE_SESSION).replace("*** FLOP *** [3c 9h 7h]\n", "");
    assert!(matches!(
        parse_err(&bad),
        ParseError::IncompleteStreet { .. }
    ));
}

#[test]
fn rejects_truncated_hand() {
    let truncated = first_hand(SAMPLE_SESSION)
        .split("*** SUMMARY ***")
        .next()
        .unwrap();
    assert!(matches!(
        parse_err(truncated),
        ParseError::IncompleteStreet { .. }
    ));
}

#[test]
fn rejects_inconsistent_raise_arithmetic() {
    let second = SAMPLE_SESSION.split("\n\n").nth(1).unwrap();
    let bad = second.replace("raises $1.74 to $2.22", "raises $1.74 to $5.00");
    assert!(matches!(parse_err(&bad), ParseError::IllegalAction { .. }));
}

#[test]
fn rejects_check_while_facing_a_bet() {
    let bad = first_hand(SAMPLE_SESSION).replace("Hero: folds", "Hero: checks");
    assert!(matches!(parse_err(&bad), ParseError::IllegalAction { .. }));
}

#[test]
fn rejects_blind_posted_after_hole_cards() {
    let bad = WALK.replace("888ffeb2: folds", "888ffeb2: posts big blind $0.1");
    assert!(matches!(parse_err(&bad), ParseError::IllegalAction { .. }));
}

#[test]
fn rejects_unrecognized_line() {
    let bad = first_hand(SAMPLE_SESSION).replace("Hero: folds", "Hero: performs a barrel roll");
    assert!(matches!(
        parse_err(&bad),
        ParseError::UnrecognizedLine { .. }
    ));
}

#[test]
fn rejects_conflicting_shown_cards() {
    let bad = first_hand(SAMPLE_SESSION).replace(
        "2d5ce8a9: shows [4d 8d]",
        "Hero: shows [As Ks]\n2d5ce8a9: shows [4d 8d]",
    );
    assert_eq!(
        parse_err(&bad),
        ParseError::ConflictingHoleCards {
            player_name: "Hero".to_string()
        }
    );
}

#[test]
fn rejects_summary_winnings_or_board_that_disagree_with_body() {
    let bad = first_hand(SAMPLE_SESSION).replace("won ($0.19)", "won ($0.18)");
    assert!(matches!(
        parse_err(&bad),
        ParseError::SummaryMismatch { .. }
    ));
    let bad =
        first_hand(SAMPLE_SESSION).replace("Board [3c 9h 7h Qs 9c]", "Board [3c 9h 7h Qs 9d]");
    assert!(matches!(
        parse_err(&bad),
        ParseError::SummaryMismatch { .. }
    ));
    let bad = RIT_TWICE_FLOP_ALL_IN.replace("run two times", "run three times");
    assert!(matches!(
        parse_err(&bad),
        ParseError::SummaryMismatch { .. }
    ));
}

#[test]
fn rejects_unsupported_games_and_stakes_formats() {
    let plo = WALK.replace("Hold'em No Limit", "Omaha Pot Limit");
    assert!(matches!(
        parse_err(&plo),
        ParseError::UnsupportedGame { .. }
    ));
    let euro = WALK.replace("($0.05/$0.1)", "(€0.05/€0.1)");
    assert!(matches!(
        parse_err(&euro),
        ParseError::UnsupportedGame { .. }
    ));
}

#[test]
fn rejects_unknown_or_unsettled_cashout_formats() {
    let unknown = EV_CASHOUT.replace("Pays Cashout Risk ($0.48)\n", "Receives Cashout ($1.20)\n");
    assert!(matches!(
        parse_err(&unknown),
        ParseError::SummaryMismatch { .. }
    ));
    let unknown_wording =
        EV_CASHOUT.replace("Pays Cashout Risk ($0.48)\n", "Takes Cashout ($1.20)\n");
    assert!(matches!(
        parse_err(&unknown_wording),
        ParseError::UnsupportedAdjustment { .. }
    ));
    let unsettled = EV_CASHOUT
        .replace("1020a9fa: Pays Cashout Risk ($0.48)\n", "")
        .replace(", Cashout Risk ($0.48)", "");
    assert!(matches!(
        parse_err(&unsettled),
        ParseError::UnsupportedAdjustment { .. }
    ));
}

#[test]
fn ev_cashout_received_refusals() {
    // Amount differs between body and summary
    let bad_amt =
        EV_CASHOUT_RECEIVED.replace("Receives Cashout ($6.87)", "Receives Cashout ($5.00)");
    assert!(matches!(
        parse_err(&bad_amt),
        ParseError::SummaryMismatch { .. }
    ));

    // Summary suffix changed to Cashout Risk ($6.87)
    let wrong_suffix =
        EV_CASHOUT_RECEIVED.replace(", EV Cashout ($6.87)", ", Cashout Risk ($6.87)");
    assert!(matches!(
        parse_err(&wrong_suffix),
        ParseError::SummaryMismatch { .. }
    ));

    // Receives Cashout line deleted
    let deleted_settlement =
        EV_CASHOUT_RECEIVED.replace("81ae8569: Receives Cashout ($6.87)\n", "");
    assert!(matches!(
        parse_err(&deleted_settlement),
        ParseError::UnsupportedAdjustment { .. }
    ));

    // The line moved before the turn (e.g. into flop)
    let line_before_turn = EV_CASHOUT_RECEIVED
        .replace("81ae8569: Receives Cashout ($6.87)\n", "")
        .replace(
            "81ae8569: Chooses to EV Cashout\n",
            "81ae8569: Chooses to EV Cashout\n81ae8569: Receives Cashout ($6.87)\n",
        );
    assert!(matches!(
        parse_err(&line_before_turn),
        ParseError::UnsupportedAdjustment { .. }
    ));

    // The loser's outcome changed to won ($6.87)
    let loser_won = EV_CASHOUT_RECEIVED.replace(
        "lost with two pair, Tens and Eights, EV Cashout ($6.87)",
        "won ($6.87) with two pair, Tens and Eights, EV Cashout ($6.87)",
    );
    assert!(matches!(
        parse_err(&loser_won),
        ParseError::SummaryMismatch { .. }
    ));

    // Cashout in split pot -> UnsupportedAdjustment
    let split_pot = EV_CASHOUT_RECEIVED.replace(
        "a2939c73 collected $9.93 from pot\n",
        "a2939c73 collected $4.96 from pot\n27a6d6a collected $4.97 from pot\n",
    );
    assert!(matches!(
        parse_err(&split_pot),
        ParseError::UnsupportedAdjustment { .. }
    ));
}

#[test]
fn rejects_missing_repeated_or_skipped_actions() {
    let hand = first_hand(SAMPLE_SESSION);
    // A deleted fold: the summary still says Hero folded before the flop.
    let deleted_fold = hand.replace("Hero: folds\n", "");
    assert!(matches!(
        parse_err(&deleted_fold),
        ParseError::SummaryMismatch { .. } | ParseError::InvariantViolation { .. }
    ));
    // The same player checking twice in a row.
    let double_check = hand.replace(
        "*** FLOP *** [3c 9h 7h]\n2d5ce8a9: checks\n",
        "*** FLOP *** [3c 9h 7h]\n2d5ce8a9: checks\n2d5ce8a9: checks\n",
    );
    assert!(matches!(
        parse_err(&double_check),
        ParseError::InvariantViolation { .. }
    ));
    // A flop on which nobody acts although two players can.
    let empty_flop = hand.replace(
        "*** FLOP *** [3c 9h 7h]\n2d5ce8a9: checks\n4af6a555: checks\n",
        "*** FLOP *** [3c 9h 7h]\n",
    );
    assert!(matches!(
        parse_err(&empty_flop),
        ParseError::InvariantViolation { .. }
    ));
}

#[test]
fn rejects_summary_labels_folds_and_shown_cards_that_disagree_with_the_body() {
    let hand = first_hand(SAMPLE_SESSION);
    for (from, to) in [
        // Position labels.
        ("Seat 4: Hero (button)", "Seat 4: Hero"),
        ("2d5ce8a9 (small blind) showed", "2d5ce8a9 showed"),
        ("4af6a555 (big blind)", "4af6a555 (button)"),
        // Fold street and "(didn't bet)".
        (
            "folded before Flop (didn't bet)",
            "folded on the Flop (didn't bet)",
        ),
        ("folded before Flop (didn't bet)", "folded before Flop"),
        // Shown cards.
        ("showed [4d 8d]", "showed [Kd Kh]"),
        // A player who reached showdown reported as winning without showing.
        (
            "showed [4d 8d] and won ($0.19) with a pair of Nines",
            "won ($0.19)",
        ),
        // A line format never observed.
        ("folded before Flop (didn't bet)", "mucked [Ah Ad]"),
    ] {
        assert!(hand.contains(from), "fixture changed: {from}");
        let bad = hand.replace(from, to);
        assert!(
            matches!(
                parse_err(&bad),
                ParseError::SummaryMismatch { .. } | ParseError::UnrecognizedLine { .. }
            ),
            "{from} -> {to} was not rejected"
        );
    }
}

#[test]
fn rejects_stray_amounts_and_lines_before_the_deal() {
    let hand = first_hand(SAMPLE_SESSION);
    for (from, to) in [
        ("Hero: folds", "Hero: folds $7.5"),
        (
            "4af6a555: checks\n*** FLOP",
            "4af6a555: checks $0.1\n*** FLOP",
        ),
        ("2d5ce8a9: calls $0.05", "2d5ce8a9: calls $0.05 to $9"),
        ("2d5ce8a9: calls $0.05", "2d5ce8a9: calls"),
        (
            "*** HOLE CARDS ***",
            "Hero: shows [5h 2s]\n*** HOLE CARDS ***",
        ),
        (
            "*** HOLE CARDS ***",
            "Hero: Chooses to EV Cashout\n*** HOLE CARDS ***",
        ),
        (
            "4af6a555: posts big blind $0.1",
            "4af6a555: posts big blind $0.1 and is all-in",
        ),
    ] {
        assert!(hand.contains(from), "fixture changed: {from}");
        let bad = hand.replace(from, to);
        assert!(
            matches!(parse_err(&bad), ParseError::UnrecognizedLine { .. }),
            "{to:?} was not rejected as unrecognized"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// v3.5 review fixes
// ---------------------------------------------------------------------------------------------

/// The real-session hand with the given id, verbatim.
fn real_hand(id: &str) -> &'static str {
    common::REAL_SESSION
        .split("\n\n")
        .map(str::trim)
        .find(|block| block.starts_with(&format!("Poker Hand #{id}:")))
        .unwrap()
}

/// `hand` with its lines rearranged by `edit` (line endings and trailing spaces preserved).
fn edit_lines(hand: &str, edit: impl FnOnce(&mut Vec<&str>)) -> String {
    let mut lines: Vec<&str> = hand.lines().collect();
    edit(&mut lines);
    lines.join("\n")
}

#[test]
fn requires_exactly_one_dealt_line_per_seat_in_seat_order() {
    let hand = first_hand(SAMPLE_SESSION);
    let dealt: Vec<usize> = hand
        .lines()
        .enumerate()
        .filter(|(_, l)| l.starts_with("Dealt to "))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(dealt.len(), 3);
    let misplaced = [
        // Hero's cards moved onto another seated player's line.
        hand.replace("Dealt to Hero [5h 2s]", "Dealt to 2d5ce8a9 [5h 2s]"),
        edit_lines(hand, |l| l.insert(dealt[0], l[dealt[0]])),
        edit_lines(hand, |l| l.swap(dealt[0], dealt[2])),
        // A deal line after the first action.
        edit_lines(hand, |l| l.swap(dealt[2], dealt[2] + 1)),
    ];
    for bad in &misplaced {
        assert!(
            matches!(parse_err(bad), ParseError::UnrecognizedLine { .. }),
            "{bad}"
        );
    }
    for missing in [
        edit_lines(hand, |l| {
            l.remove(dealt[2]);
        }),
        hand.replace("Dealt to Hero [5h 2s]\n", ""),
    ] {
        assert!(
            matches!(
                parse_err(&missing),
                ParseError::InvariantViolation { ref detail, .. } if detail.contains("'Dealt to'")
            ),
            "{missing}"
        );
    }
}

#[test]
fn a_blind_that_uses_the_whole_stack_is_an_all_in() {
    // GGPoker prints no "and is all-in" on a blind (the suffix itself is still refused).
    let short = WALK.replace(
        "Seat 4: 10a37d12 ($8.95 in chips)",
        "Seat 4: 10a37d12 ($0.1 in chips)",
    );
    let h = parse(&short);
    let post = h
        .actions
        .iter()
        .find(|a| a.action_type == ActionType::PostBigBlind)
        .unwrap();
    assert!(post.is_all_in);
    assert_eq!(net_of(&h, "10a37d12"), dec!(0.05));
}

#[test]
fn seat_and_blind_lines_follow_the_observed_order() {
    let hand = first_hand(SAMPLE_SESSION);
    let swapped_seats = hand.replace(
        "Seat 3: 4af6a555 ($9.55 in chips)\nSeat 4: Hero ($18.6 in chips)",
        "Seat 4: Hero ($18.6 in chips)\nSeat 3: 4af6a555 ($9.55 in chips)",
    );
    let swapped_blinds = hand.replace(
        "2d5ce8a9: posts small blind $0.05\n4af6a555: posts big blind $0.1",
        "4af6a555: posts big blind $0.1\n2d5ce8a9: posts small blind $0.05",
    );
    for bad in [swapped_seats, swapped_blinds] {
        assert_ne!(bad, hand);
        assert!(
            matches!(parse_err(&bad), ParseError::UnrecognizedLine { .. }),
            "{bad}"
        );
    }
    // An EV Cashout chosen after the river is dealt cannot happen.
    let late = EV_CASHOUT.replace(
        "1020a9fa: Chooses to EV Cashout\n*** RIVER *** [8s 2c 7c Qh] [7s]\n",
        "*** RIVER *** [8s 2c 7c Qh] [7s]\n1020a9fa: Chooses to EV Cashout\n",
    );
    assert_ne!(late, EV_CASHOUT);
    assert!(matches!(
        parse_err(&late),
        ParseError::UnsupportedAdjustment { .. }
    ));
}

#[test]
fn summary_show_lines_follow_the_observed_grammar() {
    let hand = first_hand(SAMPLE_SESSION);
    for (from, to) in [
        // Text that could carry money after the hand description.
        (
            "won ($0.19) with a pair of Nines",
            "won ($0.19) with a pair of Nines, plus $0.50",
        ),
        (
            "lost with a pair of Nines",
            "lost with a pair of Nines (2 outs)",
        ),
        ("and lost with", "and lost to"),
    ] {
        assert!(hand.contains(from), "fixture changed: {from}");
        let bad = hand.replace(from, to);
        assert!(
            matches!(parse_err(&bad), ParseError::UnrecognizedLine { .. }),
            "{to:?} was not rejected as unrecognized"
        );
    }
    // One outcome per runout: an extra outcome on a single board disagrees with the body.
    let extra = hand.replace(
        "won ($0.19) with a pair of Nines",
        "won ($0.19) with a pair of Nines, and lost",
    );
    assert!(matches!(
        parse_err(&extra),
        ParseError::SummaryMismatch { .. }
    ));
    // Every observed shape still parses: three runouts, a bare `lost`, and a cashout suffix.
    for raw in [RIT_THREE_TIMES_PREFLOP, RIT_TWICE_FLOP_ALL_IN, EV_CASHOUT] {
        parse(raw);
    }
}

#[test]
fn an_uncontested_winner_may_show_cards() {
    // The big blind collects uncontested and shows both cards; GGPoker keeps `collected`.
    let shown = WALK.replace(
        "Uncalled bet ($0.05) returned to 10a37d12\n",
        "Uncalled bet ($0.05) returned to 10a37d12\n10a37d12: shows [Ah Kh]\n",
    );
    let h = parse(&shown);
    let bb = h.players.iter().find(|p| p.name == "10a37d12").unwrap();
    assert_eq!(bb.hole_cards, [Some(card("Ah")), Some(card("Kh"))]);
    assert_eq!(net_of(&h, "10a37d12"), dec!(0.05));
    // A contested player summarised as winning without a show is still refused (see above).
}

#[test]
fn rejects_blinds_and_payouts_the_stakes_and_side_pots_do_not_allow() {
    // A 0.06 small blind whose difference the later `raises $0.8 to $1` total would absorb.
    let bad = real_hand("HD140647049").replace(
        "Hero: posts small blind $0.05",
        "Hero: posts small blind $0.06",
    );
    assert!(matches!(
        parse_err(&bad),
        ParseError::InvariantViolation { ref detail, .. } if detail.contains("stakes")
    ));
    // The 2.71 all-in stack paid the whole pot: it can win only the main pot.
    let bad = THREE_WAY_ALL_IN_SIDE_POT
        .replace(
            "6bf8f391 collected $7.82 from pot\n3fe4ea4e collected $13.85 from pot",
            "6bf8f391 collected $21.67 from pot",
        )
        .replace("and won ($7.82)", "and won ($21.67)")
        .replace(
            "showed [As Qc] and won ($13.85) with",
            "showed [As Qc] and lost with",
        );
    assert!(matches!(
        parse_err(&bad),
        ParseError::InvariantViolation { ref detail, .. } if detail.contains("could win")
    ));
}

#[test]
fn straddle_refusal_tests() {
    // 1. straddle $0.3 (not 2x)
    let not_2x = STRADDLE_UTG_OPTION.replace("5daacec9: straddle $0.2", "5daacec9: straddle $0.3");
    assert!(matches!(
        parse_err(&not_2x),
        ParseError::IllegalAction { ref reason, .. } if reason.contains("2x highest previous commitment")
    ));

    // 2. A third straddle (straddle $0.8 added after straddle $0.4 in DOUBLE_STRADDLE)
    let third_straddle = DOUBLE_STRADDLE.replace(
        "494b476f: straddle $0.4\n",
        "494b476f: straddle $0.4\nadbf678c: straddle $0.8\n",
    );
    assert!(matches!(
        parse_err(&third_straddle),
        ParseError::IllegalAction { ref reason, .. } if reason.contains("at most two straddles")
    ));

    // 3a. Straddle moved after *** HOLE CARDS ***
    let straddle_after_hole_cards = STRADDLE_UTG_OPTION
        .replace("5daacec9: straddle $0.2\n", "")
        .replace(
            "*** HOLE CARDS ***\n",
            "*** HOLE CARDS ***\n5daacec9: straddle $0.2\n",
        );
    assert!(matches!(
        parse_err(&straddle_after_hole_cards),
        ParseError::IllegalAction { ref reason, .. } if reason.contains("forced post after hole cards")
    ));

    // 3b. Straddle moved before blinds
    let straddle_before_blinds = STRADDLE_UTG_OPTION
        .replace("5daacec9: straddle $0.2\n", "")
        .replace(
            "494b476f: posts small blind $0.05\n",
            "5daacec9: straddle $0.2\n494b476f: posts small blind $0.05\n",
        );
    assert!(matches!(
        parse_err(&straddle_before_blinds),
        ParseError::UnrecognizedLine { .. } | ParseError::IllegalAction { .. }
    ));

    // 4. First preflop actor changed to the seat left of the big blind
    // In STRADDLE_BUTTON, seat 3 is button/straddler, seat 4 is SB, seat 5 is BB (Hero), seat 6 is 4e268376.
    // Seat left of BB is seat 6. But first actor should be seat 4 (left of straddler).
    let out_of_turn = STRADDLE_BUTTON.replace("4e268376: folds\n", "").replace(
        "adbf678c: raises $0.26 to $0.46\n",
        "4e268376: folds\nadbf678c: raises $0.26 to $0.46\n",
    );
    assert!(matches!(
        parse_err(&out_of_turn),
        ParseError::InvariantViolation { ref detail, .. } if detail.contains("acts out of turn")
    ));

    // 5. The straddler's final `checks` deleted
    let no_check = STRADDLE_UTG_OPTION.replace("5daacec9: checks\n", "");
    assert!(matches!(
        parse_err(&no_check),
        ParseError::InvariantViolation { ref detail, .. } if detail.contains("betting ended before 1 player(s) acted")
    ));

    // 6. `raises $0.1 to $0.3` facing a 0.2 straddle
    let small_raise =
        STRADDLE_UTG_OPTION.replace("adbf678c: calls $0.2", "adbf678c: raises $0.1 to $0.3");
    assert!(matches!(
        parse_err(&small_raise),
        ParseError::IllegalAction { .. } | ParseError::InvariantViolation { .. }
    ));

    // 7. `straddle $0.2 and is all-in`
    let all_in_suffix = STRADDLE_UTG_OPTION.replace(
        "5daacec9: straddle $0.2",
        "5daacec9: straddle $0.2 and is all-in",
    );
    assert!(matches!(
        parse_err(&all_in_suffix),
        ParseError::UnrecognizedLine { .. }
    ));
}
