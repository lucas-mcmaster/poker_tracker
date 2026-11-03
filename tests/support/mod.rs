//! Shared test builder helpers for canonical hand construction.

#![allow(dead_code)]

use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use poker_tracker::domain::{
    ActionType, BettingLimit, CanonicalHand, CommunityCards, CurrencyCode, GameType, HandHeader,
    HandOutcome, Platform, Player, PotWinner, Street, StreetAction,
};

pub fn make_player(seat: u8, name: &str, stack: Decimal, is_hero: bool) -> Player {
    Player {
        seat,
        name: name.to_string(),
        starting_stack: stack,
        hole_cards: [None, None],
        is_hero,
    }
}

pub fn make_action(
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

pub fn make_canonical_hand(
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
        .map(|(name, amount)| PotWinner {
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
