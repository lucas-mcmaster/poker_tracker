//! Whole-session reconciliation against a real GGPoker export from a CAD account
//! (`tests/fixtures/ggpoker/NLHRed15_2026-08-21.txt`, 294 hands).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::HashMap;

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sha2::{Digest, Sha256};

use common::{FIXTURE_CURRENCY, REAL_SESSION, gg};
use poker_tracker::domain::{ActionType, CanonicalHand, DeductionKind};
use poker_tracker::parser::HandParser;

fn parse_all() -> Vec<CanonicalHand> {
    let parser = gg();
    let mut hands = Vec::new();
    for (block, result) in parser.parse_session(REAL_SESSION) {
        match result {
            Ok(h) => hands.push(h),
            Err(e) => panic!(
                "hand {:?} at line {} failed: {e}",
                parser.peek_hand_id(block.text),
                block.start_line
            ),
        }
    }
    hands.sort_by_key(|h| h.header.timestamp);
    hands
}

#[test]
fn every_hand_in_the_real_session_parses() {
    let hands = parse_all();
    assert_eq!(hands.len(), 294);
    assert!(hands.iter().all(|h| h.header.currency == FIXTURE_CURRENCY));
    assert_eq!(hands.iter().filter(|h| h.run_count() >= 2).count(), 4);
    assert_eq!(
        hands
            .iter()
            .filter(|h| !h.outcome.adjustments.is_empty())
            .count(),
        1
    );
    assert_eq!(
        hands
            .iter()
            .flat_map(|h| &h.actions)
            .filter(|a| a.action_type == ActionType::PostDeadBlind)
            .count(),
        1
    );
    assert_eq!(
        hands
            .iter()
            .flat_map(|h| &h.players)
            .filter(|p| p.hole_cards[0].is_some() && p.hole_cards[1].is_none())
            .count(),
        1
    );
}

#[test]
fn money_is_conserved_across_the_whole_session() {
    // Sum of every player's net == -(all pot deductions) + (all off-pot adjustments).
    let hands = parse_all();
    let mut total_net = Decimal::ZERO;
    let mut deductions = Decimal::ZERO;
    let mut rake = Decimal::ZERO;
    let mut adjustments = Decimal::ZERO;
    for h in &hands {
        for r in h.checked_player_results().unwrap() {
            total_net += r.net;
        }
        deductions += h.outcome.checked_total_deductions().unwrap();
        rake += h
            .outcome
            .checked_deductions_of(DeductionKind::Rake)
            .unwrap();
        adjustments += h.outcome.checked_total_adjustments().unwrap();
    }
    assert_eq!(total_net, adjustments - deductions);
    assert_eq!(deductions, dec!(17.35));
    assert_eq!(adjustments, dec!(-0.48));
    assert!(rake > Decimal::ZERO && rake < deductions);
}

#[test]
fn stacks_reconcile_between_consecutive_hands() {
    // A player's next starting stack equals their previous ending stack plus any top-up.
    // In this session no stack ever drops between hands, and Hero's results reconcile with
    // the observed stack change: 10.00 -> 18.85 = +2.86 net + 5.99 of auto top-ups.
    let hands = parse_all();
    let mut ending: HashMap<String, Decimal> = HashMap::new();
    let mut hero_top_ups = Decimal::ZERO;
    let mut hero_net = Decimal::ZERO;
    for h in &hands {
        for p in &h.players {
            if let Some(prev_end) = ending.get(&p.name) {
                let gap = p.starting_stack - *prev_end;
                assert!(
                    gap >= Decimal::ZERO,
                    "{} lost {gap} between hands before {}",
                    p.name,
                    h.header.site_hand_id
                );
                if p.name == "Hero" {
                    hero_top_ups += gap;
                }
            }
        }
        for (p, r) in h.players.iter().zip(h.checked_player_results().unwrap()) {
            ending.insert(p.name.clone(), p.starting_stack + r.net);
            if p.is_hero {
                hero_net += r.net;
            }
        }
    }
    assert_eq!(hero_net, dec!(2.86));
    assert_eq!(hero_top_ups, dec!(5.99));
    assert_eq!(ending["Hero"], dec!(18.85));
}

#[test]
fn the_fixture_is_the_reference_export_byte_for_byte() {
    // Guards against editors or Git rewriting line endings or trailing spaces in the export.
    let digest = Sha256::digest(REAL_SESSION.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(REAL_SESSION.len(), 331_441);
    assert_eq!(
        hex,
        "78d1e036752c92c3064154596bd168d3bb92871c54cd8aa2626d181e627b05e4"
    );
}

#[test]
fn every_real_hand_survives_a_json_round_trip() {
    for h in parse_all() {
        let json = serde_json::to_string(&h).unwrap();
        let back: CanonicalHand = serde_json::from_str(&json).unwrap();
        assert_eq!(back, h);
    }
}
