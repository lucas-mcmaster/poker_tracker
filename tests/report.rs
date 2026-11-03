//! Report integration and synthetic tests (SPEC_v3.7 §6, M15.5).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
mod support;

use std::fs;
use std::path::Path;
use std::str::FromStr;

use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tempfile::NamedTempFile;

use common::gg;
use poker_tracker::domain::{
    ActionType, CanonicalHand, Card, CurrencyCode, PotWinner, Rank, Street, Suit,
};
use poker_tracker::parser::HandParser;
use poker_tracker::report::{ReportError, ReportKind, ReportOptions, Stakes, write_report};
use poker_tracker::storage::{SqliteRepository, StorageError};
use support::{make_action, make_canonical_hand, make_player};

fn valid_hand(mut h: CanonicalHand) -> CanonicalHand {
    if let Some(hero) = h.players.iter_mut().find(|p| p.is_hero)
        && hero.hole_cards[0].is_none()
    {
        hero.hole_cards = [
            Some(Card::new(Rank::Ace, Suit::Spades)),
            Some(Card::new(Rank::King, Suit::Hearts)),
        ];
    }
    let folded: std::collections::HashSet<String> = h
        .actions
        .iter()
        .filter(|a| a.action_type == ActionType::Fold)
        .map(|a| a.player_name.clone())
        .collect();
    let pot = h.checked_total_contributed().unwrap_or(Decimal::ZERO);
    let deductions = h
        .outcome
        .checked_total_deductions()
        .unwrap_or(Decimal::ZERO);
    h.outcome.total_pot = pot;
    let net_pot = pot - deductions;
    let winner = h
        .players
        .iter()
        .find(|p| !folded.contains(&p.name))
        .map(|p| p.name.clone())
        .unwrap_or_else(|| h.players[0].name.clone());
    h.outcome.winners = vec![PotWinner {
        player_name: winner,
        amount: net_pot,
        run_index: 0,
    }];
    h
}

#[test]
fn golden_v3_6_reports_match_byte_for_byte() {
    let v36_dir = Path::new("tests/fixtures/ggpoker/v3_6");
    let mut paths: Vec<_> = fs::read_dir(v36_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "txt"))
        .collect();
    paths.sort();

    let parser = gg();
    let mut all_hands = Vec::new();
    for p in paths {
        let content = fs::read_to_string(p).unwrap();
        for (_, res) in parser.parse_session(&content) {
            all_hands.push(res.unwrap());
        }
    }
    assert_eq!(all_hands.len(), 989);

    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    repo.insert_hands_batch(&all_hands).unwrap();

    let options = ReportOptions::default();

    // 1. Results
    let mut buf = Vec::new();
    write_report(&repo, ReportKind::Results, &options, &mut buf).unwrap();
    let golden_results = fs::read("tests/golden/v3_6_results.csv").unwrap();
    assert_eq!(buf, golden_results, "results CSV differs from golden");

    // 2. Positions
    let mut buf = Vec::new();
    write_report(&repo, ReportKind::Positions, &options, &mut buf).unwrap();
    let golden_positions = fs::read("tests/golden/v3_6_positions.csv").unwrap();
    assert_eq!(buf, golden_positions, "positions CSV differs from golden");

    // 3. Sessions
    let mut buf = Vec::new();
    write_report(&repo, ReportKind::Sessions, &options, &mut buf).unwrap();
    let golden_sessions = fs::read("tests/golden/v3_6_sessions.csv").unwrap();
    assert_eq!(buf, golden_sessions, "sessions CSV differs from golden");

    // 4. Stats
    let mut buf = Vec::new();
    write_report(&repo, ReportKind::Stats, &options, &mut buf).unwrap();
    let golden_stats = fs::read("tests/golden/v3_6_stats.csv").unwrap();
    assert_eq!(buf, golden_stats, "stats CSV differs from golden");
}

#[test]
fn golden_v3_5_results_row() {
    let v35_path = Path::new("tests/fixtures/ggpoker/NLHRed15_2026-08-21.txt");
    let content = fs::read_to_string(v35_path).unwrap();
    let parser = gg();
    let mut hands = Vec::new();
    for (_, res) in parser.parse_session(&content) {
        hands.push(res.unwrap());
    }
    assert_eq!(hands.len(), 294);

    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    repo.insert_hands_batch(&hands).unwrap();

    let mut buf = Vec::new();
    write_report(
        &repo,
        ReportKind::Results,
        &ReportOptions::default(),
        &mut buf,
    )
    .unwrap();
    let out_str = String::from_utf8(buf).unwrap();
    let lines: Vec<&str> = out_str.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[1],
        "CAD,0.05/0.1,294,2.86,0.97,28.60,9.73,2.195316,0.353832,8.3,0"
    );
}

#[test]
fn stakes_synthetic_and_parsing() {
    // Malformed stakes rejected by Stakes::from_str
    for malformed in [
        "0.05",
        "0.05/",
        "/0.1",
        "a/b",
        "0.0000001/0.1",
        "-0.05/0.1",
        "0/0.1",
        "0.05/0",
        "",
    ] {
        assert!(
            Stakes::from_str(malformed).is_err(),
            "expected '{malformed}' to be rejected"
        );
    }

    // Valid stakes formatting and Display
    let s = Stakes::from_str("0.05/0.10").unwrap();
    assert_eq!(s.to_string(), "0.05/0.1");

    let p = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "Villain", dec!(10), false),
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
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Hero",
            ActionType::Fold,
            dec!(0),
            dec!(0.05),
            false,
        ),
    ];

    // Hand 1: CAD 0.05/0.10
    let mut h1 = valid_hand(make_canonical_hand(1, p.clone(), a.clone(), vec![], vec![]));
    h1.header.site_hand_id = "H1".to_string();
    h1.header.currency = CurrencyCode::CAD;
    h1.header.small_blind = dec!(0.05);
    h1.header.big_blind = dec!(0.10);

    // Hand 2: CAD 0.10/0.20
    let a2 = vec![
        make_action(
            0,
            Street::Preflop,
            "Hero",
            ActionType::PostSmallBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "Villain",
            ActionType::PostBigBlind,
            dec!(0.20),
            dec!(0.20),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Hero",
            ActionType::Fold,
            dec!(0),
            dec!(0.10),
            false,
        ),
    ];
    let mut h2 = valid_hand(make_canonical_hand(1, p.clone(), a2, vec![], vec![]));
    h2.header.site_hand_id = "H2".to_string();
    h2.header.currency = CurrencyCode::CAD;
    h2.header.small_blind = dec!(0.10);
    h2.header.big_blind = dec!(0.20);

    // Hand 3: USD 0.05/0.10
    let mut h3 = valid_hand(make_canonical_hand(1, p, a, vec![], vec![]));
    h3.header.site_hand_id = "H3".to_string();
    h3.header.currency = CurrencyCode::USD;
    h3.header.small_blind = dec!(0.05);
    h3.header.big_blind = dec!(0.10);

    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    repo.insert_hands_batch(&[h1, h2, h3]).unwrap();

    // 1. All stakes: CAD has two stakes then ALL; USD has one stake and no ALL
    let mut buf = Vec::new();
    let outcome = write_report(
        &repo,
        ReportKind::Results,
        &ReportOptions::default(),
        &mut buf,
    )
    .unwrap();
    assert!(outcome.unmatched_stakes.is_empty());
    let out = String::from_utf8(buf).unwrap();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines[0],
        "currency,stakes,hands,net,net_per_100,bb_won,bb_per_100,rake,fees,blinds,straddles"
    );
    assert!(lines[1].starts_with("CAD,0.05/0.1,"));
    assert!(lines[2].starts_with("CAD,0.1/0.2,"));
    assert!(lines[3].starts_with("CAD,ALL,2,"));
    assert!(lines[4].starts_with("USD,0.05/0.1,1,"));
    assert_eq!(lines.len(), 5);

    // 2. Filter to 0.05/0.1: drops CAD's 0.1/0.2 and drops ALL
    let mut buf = Vec::new();
    let opt = ReportOptions {
        stakes: Some(vec![Stakes::from_str("0.05/0.1").unwrap()]),
    };
    write_report(&repo, ReportKind::Results, &opt, &mut buf).unwrap();
    let out = String::from_utf8(buf).unwrap();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[1].starts_with("CAD,0.05/0.1,1,"));
    assert!(lines[2].starts_with("USD,0.05/0.1,1,"));

    // 3. Filter with both CAD stakes restores ALL row, even with 0.10 spelling
    let mut buf = Vec::new();
    let opt = ReportOptions {
        stakes: Some(vec![
            Stakes::from_str("0.10/0.20").unwrap(),
            Stakes::from_str("0.05/0.10").unwrap(),
        ]),
    };
    write_report(&repo, ReportKind::Results, &opt, &mut buf).unwrap();
    let out = String::from_utf8(buf).unwrap();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 5);
    assert!(lines[3].starts_with("CAD,ALL,2,"));

    // 4. Unmatched stake reported in outcome
    let mut buf = Vec::new();
    let opt = ReportOptions {
        stakes: Some(vec![
            Stakes::from_str("0.05/0.1").unwrap(),
            Stakes::from_str("1.0/2.0").unwrap(),
        ]),
    };
    let outcome = write_report(&repo, ReportKind::Results, &opt, &mut buf).unwrap();
    assert_eq!(outcome.unmatched_stakes.len(), 1);
    assert_eq!(outcome.unmatched_stakes[0].to_string(), "1/2");

    // 5. Restricts positions, sessions, and stats
    let opt_single = ReportOptions {
        stakes: Some(vec![Stakes::from_str("0.1/0.2").unwrap()]),
    };
    let mut buf = Vec::new();
    write_report(&repo, ReportKind::Positions, &opt_single, &mut buf).unwrap();
    let pos_out = String::from_utf8(buf).unwrap();
    // Only CAD has 0.1/0.2, USD has none
    assert!(pos_out.contains("CAD,"));
    assert!(!pos_out.contains("USD,"));

    let mut buf = Vec::new();
    write_report(&repo, ReportKind::Sessions, &opt_single, &mut buf).unwrap();
    let sess_out = String::from_utf8(buf).unwrap();
    assert!(sess_out.contains("CAD,"));
    assert!(!sess_out.contains("USD,"));

    let mut buf = Vec::new();
    write_report(&repo, ReportKind::Stats, &opt_single, &mut buf).unwrap();
    let stats_out = String::from_utf8(buf).unwrap();
    assert!(stats_out.contains("CAD,"));
    assert!(!stats_out.contains("USD,"));
}

#[test]
fn positions_null_gives_question_mark_row() {
    // 2 players: button on seat 1, but SB is posted by seat 2 and BB by seat 1.
    // In heads-up, the button must be the SB poster. Since it is not, assign_positions
    // returns an empty map and every player gets NULL position.
    let p = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "Villain", dec!(10), false),
    ];
    let a = vec![
        make_action(
            0,
            Street::Preflop,
            "Villain",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "Hero",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Villain",
            ActionType::Fold,
            dec!(0),
            dec!(0.05),
            false,
        ),
    ];
    let mut h = valid_hand(make_canonical_hand(1, p, a, vec![], vec![]));
    h.header.site_hand_id = "H_NULL_POS".to_string();

    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    repo.insert_hands_batch(&[h]).unwrap();

    let mut buf = Vec::new();
    write_report(
        &repo,
        ReportKind::Positions,
        &ReportOptions::default(),
        &mut buf,
    )
    .unwrap();
    let out = String::from_utf8(buf).unwrap();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines[0],
        "currency,position,hands,net,bb_per_100,vpip,pfr,rfi"
    );
    assert_eq!(lines.len(), 2);
    assert!(lines[1].starts_with("CAD,?,1,"));
}

#[test]
fn sessions_duration_gaps_and_dst() {
    let p = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "Villain", dec!(10), false),
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
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Hero",
            ActionType::Fold,
            dec!(0),
            dec!(0.05),
            false,
        ),
    ];

    // Case 1: single hand session has duration_seconds = 0
    let mut h1 = valid_hand(make_canonical_hand(1, p.clone(), a.clone(), vec![], vec![]));
    h1.header.site_hand_id = "H1".to_string();
    h1.header.table_name = "TableSingle".to_string();
    h1.header.timestamp =
        NaiveDateTime::parse_from_str("2026-08-21 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap();

    // Case 2: two hands 30:00 apart are one session (duration 1800)
    let mut h2a = valid_hand(make_canonical_hand(1, p.clone(), a.clone(), vec![], vec![]));
    h2a.header.site_hand_id = "H2A".to_string();
    h2a.header.table_name = "Table30Min".to_string();
    h2a.header.timestamp =
        NaiveDateTime::parse_from_str("2026-08-21 13:00:00", "%Y-%m-%d %H:%M:%S").unwrap();

    let mut h2b = valid_hand(make_canonical_hand(1, p.clone(), a.clone(), vec![], vec![]));
    h2b.header.site_hand_id = "H2B".to_string();
    h2b.header.table_name = "Table30Min".to_string();
    h2b.header.timestamp =
        NaiveDateTime::parse_from_str("2026-08-21 13:30:00", "%Y-%m-%d %H:%M:%S").unwrap();

    // Case 3: two hands 30:01 apart are two sessions
    let mut h3a = valid_hand(make_canonical_hand(1, p.clone(), a.clone(), vec![], vec![]));
    h3a.header.site_hand_id = "H3A".to_string();
    h3a.header.table_name = "Table3001".to_string();
    h3a.header.timestamp =
        NaiveDateTime::parse_from_str("2026-08-21 14:00:00", "%Y-%m-%d %H:%M:%S").unwrap();

    let mut h3b = valid_hand(make_canonical_hand(1, p.clone(), a.clone(), vec![], vec![]));
    h3b.header.site_hand_id = "H3B".to_string();
    h3b.header.table_name = "Table3001".to_string();
    h3b.header.timestamp =
        NaiveDateTime::parse_from_str("2026-08-21 14:30:01", "%Y-%m-%d %H:%M:%S").unwrap();

    // Case 4: daylight-saving change: 01:50 at -4h (-14400s), 01:10 at -5h (-18000s) = 20 min (1200s) later in UTC
    let mut h4a = valid_hand(make_canonical_hand(1, p.clone(), a.clone(), vec![], vec![]));
    h4a.header.site_hand_id = "H4A".to_string();
    h4a.header.table_name = "TableDST".to_string();
    h4a.header.timestamp =
        NaiveDateTime::parse_from_str("2026-11-01 01:50:00", "%Y-%m-%d %H:%M:%S").unwrap();
    h4a.header.utc_offset_seconds = Some(-14400);

    let mut h4b = valid_hand(make_canonical_hand(1, p, a, vec![], vec![]));
    h4b.header.site_hand_id = "H4B".to_string();
    h4b.header.table_name = "TableDST".to_string();
    h4b.header.timestamp =
        NaiveDateTime::parse_from_str("2026-11-01 01:10:00", "%Y-%m-%d %H:%M:%S").unwrap();
    h4b.header.utc_offset_seconds = Some(-18000);

    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    repo.insert_hands_batch(&[h1, h2a, h2b, h3a, h3b, h4a, h4b])
        .unwrap();

    let mut buf = Vec::new();
    write_report(
        &repo,
        ReportKind::Sessions,
        &ReportOptions::default(),
        &mut buf,
    )
    .unwrap();
    let out = String::from_utf8(buf).unwrap();

    // TableSingle: duration 0, 1 hand
    assert!(out.contains(",TableSingle,2026-08-21T12:00:00,2026-08-21T12:00:00,0,1,"));

    // Table30Min: duration 1800, 2 hands
    assert!(out.contains(",Table30Min,2026-08-21T13:00:00,2026-08-21T13:30:00,1800,2,"));

    // Table3001: 2 separate sessions
    assert!(out.contains(",Table3001,2026-08-21T14:00:00,2026-08-21T14:00:00,0,1,"));
    assert!(out.contains(",Table3001,2026-08-21T14:30:01,2026-08-21T14:30:01,0,1,"));

    // TableDST: 1 session, duration 1200
    assert!(out.contains(",TableDST,2026-11-01T01:50:00,2026-11-01T01:10:00,1200,2,"));
}

#[test]
fn version_guard_rejects_stale_cache_and_writes_nothing() {
    let p = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "Villain", dec!(10), false),
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
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Hero",
            ActionType::Fold,
            dec!(0),
            dec!(0.05),
            false,
        ),
    ];
    let mut h = valid_hand(make_canonical_hand(1, p, a, vec![], vec![]));
    h.header.site_hand_id = "H_VG".to_string();

    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    let res = repo.insert_hands_batch(&[h]).unwrap();
    eprintln!("positions rejected: {:?}", res.rejected);

    // Mark cache stale via direct SQL
    repo.connection()
        .execute(
            "UPDATE derived_cache SET version = 999 WHERE name = 'hand_player_stats'",
            [],
        )
        .unwrap();

    let mut buf = Vec::new();
    let err = write_report(
        &repo,
        ReportKind::Results,
        &ReportOptions::default(),
        &mut buf,
    )
    .unwrap_err();

    match err {
        ReportError::Storage(StorageError::StaleDerivedCache { found, expected }) => {
            assert_eq!(found, Some(999));
            assert_eq!(expected, poker_tracker::stats::STATS_VERSION);
        }
        other => panic!("expected StaleDerivedCache, got: {other}"),
    }

    assert!(buf.is_empty(), "stale cache must write nothing to output");
}
