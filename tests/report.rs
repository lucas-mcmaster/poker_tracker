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
    ActionType, CanonicalHand, Card, CommunityCards, CurrencyCode, PotWinner, Rank, Street, Suit,
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
        "CAD,0.05/0.1,294,2.86,0.97,28.60,9.73,2.195316,0.353832,8.3,0,3,2.48105,8.44"
    );
}

/// Heads-up hand in which Hero posts the small blind and folds to the big blind, so Hero's net
/// is minus the small blind and `bb_won` is `-sb / bb`.
fn hero_sb_folds(id: &str, sb: Decimal, bb: Decimal) -> CanonicalHand {
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
            sb,
            sb,
            false,
        ),
        make_action(
            1,
            Street::Preflop,
            "Villain",
            ActionType::PostBigBlind,
            bb,
            bb,
            false,
        ),
        make_action(
            2,
            Street::Preflop,
            "Hero",
            ActionType::Fold,
            dec!(0),
            sb,
            false,
        ),
    ];
    let mut h = valid_hand(make_canonical_hand(1, p, a, vec![], vec![]));
    h.header.site_hand_id = id.to_string();
    h.header.small_blind = sb;
    h.header.big_blind = bb;
    h
}

fn report_text(repo: &SqliteRepository, kind: ReportKind, options: &ReportOptions) -> String {
    let mut buf = Vec::new();
    write_report(repo, kind, options, &mut buf).unwrap();
    String::from_utf8(buf).unwrap()
}

fn stakes_options(items: &[&str]) -> ReportOptions {
    ReportOptions {
        stakes: Some(items.iter().map(|s| Stakes::from_str(s).unwrap()).collect()),
    }
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
        "1_0/2_0",
        "1e-2/0.1",
        "+0.05/0.1",
        ".05/.1",
        "0.05/0.1/0.2",
    ] {
        assert!(
            Stakes::from_str(malformed).is_err(),
            "expected '{malformed}' to be rejected"
        );
    }

    // Valid stakes formatting and Display
    let s = Stakes::from_str("0.05/0.10").unwrap();
    assert_eq!(s.to_string(), "0.05/0.1");
    assert_eq!(
        Stakes::from_str(" 0.1 / 0.25 ").unwrap().to_string(),
        "0.1/0.25"
    );

    // Hero folds the small blind in every hand. The two CAD stakes have different `bb_won` per
    // hand (-0.5 and -0.4) and different hand counts (2 and 1), so an ALL row computed as
    // Σ net / Σ bb, or as the mean of the stakes rows, would differ from the correct one.
    let mut h1 = hero_sb_folds("H1", dec!(0.05), dec!(0.10));
    h1.header.currency = CurrencyCode::CAD;
    let mut h1b = hero_sb_folds("H1B", dec!(0.05), dec!(0.10));
    h1b.header.currency = CurrencyCode::CAD;
    let mut h2 = hero_sb_folds("H2", dec!(0.10), dec!(0.25));
    h2.header.currency = CurrencyCode::CAD;
    let mut h3 = hero_sb_folds("H3", dec!(0.05), dec!(0.10));
    h3.header.currency = CurrencyCode::USD;

    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    let stored = repo.insert_hands_batch(&[h1, h1b, h2, h3]).unwrap();
    assert!(stored.rejected.is_empty(), "{:?}", stored.rejected);

    let header = "currency,stakes,hands,net,net_per_100,bb_won,bb_per_100,rake,fees,blinds,straddles,allin_hands,ev_net,ev_bb_per_100";
    // 2 hands; net -0.10; $/100 -0.10 / 2 × 100; bb_won -0.10 / 0.10; bb/100 -1.00 / 2 × 100
    let cad_small = "CAD,0.05/0.1,2,-0.1,-5.00,-1.00,-50.00,0,0,0.1,0,0,-0.1,-50.00";
    // net -0.10; $/100 -10.00; bb_won -0.10 / 0.25 = -0.40; bb/100 -40.00; blinds 0.1
    let cad_big = "CAD,0.1/0.25,1,-0.1,-10.00,-0.40,-40.00,0,0,0.1,0,0,-0.1,-40.00";
    // 3 hands; net -0.20; $/100 -0.20 / 3 × 100 = -6.67; bb_won -1.00 + -0.40 = -1.40;
    // bb/100 -1.40 / 3 × 100 = -46.67 (the mean of the rows would be -45.00; Σ net / Σ bb gives
    // bb_won -0.44)
    let cad_all = "CAD,ALL,3,-0.2,-6.67,-1.40,-46.67,0,0,0.2,0,0,-0.2,-46.67";
    // net -0.05; $/100 -0.05 × 100; bb_won -0.05 / 0.10; bb/100 -0.5 × 100; blinds the SB
    let usd_small = "USD,0.05/0.1,1,-0.05,-5.00,-0.50,-50.00,0,0,0.05,0,0,-0.05,-50.00";

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
    assert_eq!(
        String::from_utf8(buf).unwrap(),
        format!("{header}\n{cad_small}\n{cad_big}\n{cad_all}\n{usd_small}\n")
    );

    // 2. Filter to 0.05/0.1: drops CAD's 0.1/0.25 row and its ALL row
    assert_eq!(
        report_text(&repo, ReportKind::Results, &stakes_options(&["0.05/0.1"])),
        format!("{header}\n{cad_small}\n{usd_small}\n")
    );

    // 3. Both CAD stakes, in the other order and with 0.10-style spellings, restore ALL
    assert_eq!(
        report_text(
            &repo,
            ReportKind::Results,
            &stakes_options(&["0.10/0.250", "0.05/0.10"])
        ),
        format!("{header}\n{cad_small}\n{cad_big}\n{cad_all}\n{usd_small}\n")
    );

    // 4. Unmatched stake reported in outcome
    let mut buf = Vec::new();
    let outcome = write_report(
        &repo,
        ReportKind::Results,
        &stakes_options(&["0.05/0.1", "1.0/2.0"]),
        &mut buf,
    )
    .unwrap();
    assert_eq!(outcome.unmatched_stakes.len(), 1);
    assert_eq!(outcome.unmatched_stakes[0].to_string(), "1/2");

    // 5. Restricts positions, sessions, and stats: only H2 (CAD 0.1/0.25) remains
    let only_big = stakes_options(&["0.1/0.25"]);
    // Hero is the heads-up SB: net -0.1; bb/100 -0.40 × 100; VPIP 0/1; PFR 0/1; RFI 0/1
    assert_eq!(
        report_text(&repo, ReportKind::Positions, &only_big),
        "currency,position,hands,net,bb_per_100,vpip,pfr,rfi\n\
         CAD,SB,1,-0.1,-40.00,0.00,0.00,0.00\n"
    );
    assert_eq!(
        report_text(&repo, ReportKind::Sessions, &only_big),
        "currency,platform,table,start,end,duration_seconds,hands,net,bb_per_100,ev_net\n\
         CAD,GGPoker,TestTable,2026-08-21T17:00:00,2026-08-21T17:00:00,0,1,-0.1,-40.00,-0.1\n"
    );
    let stats_out = report_text(&repo, ReportKind::Stats, &only_big);
    let stats_lines: Vec<&str> = stats_out.lines().collect();
    assert_eq!(stats_lines.len(), 40, "header and 39 stats, CAD only");
    assert_eq!(stats_lines[1], "CAD,vpip,0,1,0.00");
    assert!(stats_lines[1..].iter().all(|l| l.starts_with("CAD,")));
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

/// `hero_sb_folds` at 0.05/0.10 on `table` at `ts` (`%Y-%m-%d %H:%M:%S`), with an optional UTC
/// offset. Hero's net is -0.05 and `bb_won` -0.5 in every such hand.
fn session_hand(id: &str, table: &str, ts: &str, offset: Option<i32>) -> CanonicalHand {
    let mut h = hero_sb_folds(id, dec!(0.05), dec!(0.10));
    h.header.table_name = table.to_string();
    h.header.timestamp = NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S").unwrap();
    h.header.utc_offset_seconds = offset;
    h
}

#[test]
fn sessions_duration_gaps_and_dst() {
    let hands = vec![
        // A one-hand session, and another table's one-hand session at the same start
        session_hand("H1", "TableSingle", "2026-08-21 12:00:00", None),
        session_hand("H0", "TableA", "2026-08-21 12:00:00", None),
        // Two hands 30:00 apart: one session
        session_hand("H2A", "Table30Min", "2026-08-21 13:00:00", None),
        session_hand("H2B", "Table30Min", "2026-08-21 13:30:00", None),
        // Two hands 30:01 apart: two sessions
        session_hand("H3A", "Table3001", "2026-08-21 14:00:00", None),
        session_hand("H3B", "Table3001", "2026-08-21 14:30:01", None),
        // Daylight-saving change, both offsets stated: 01:50 at -4 h (05:50 UTC), then 01:10 at
        // -5 h (06:10 UTC)
        session_hand("H4A", "TableDST", "2026-11-01 01:50:00", Some(-14_400)),
        session_hand("H4B", "TableDST", "2026-11-01 01:10:00", Some(-18_000)),
        // Not every hand states an offset: as printed, 10:00 then 10:05
        session_hand("H5A", "TableMixed", "2026-11-02 10:00:00", Some(-14_400)),
        session_hand("H5B", "TableMixed", "2026-11-02 10:05:00", None),
    ];
    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    let stored = repo.insert_hands_batch(&hands).unwrap();
    assert!(stored.rejected.is_empty(), "{:?}", stored.rejected);

    // Listed by currency, start time, table, then platform. Net is -0.05 per hand and bb/100
    // is -0.5 × 100 in every session.
    let expected = "\
currency,platform,table,start,end,duration_seconds,hands,net,bb_per_100,ev_net
CAD,GGPoker,TableA,2026-08-21T12:00:00,2026-08-21T12:00:00,0,1,-0.05,-50.00,-0.05
CAD,GGPoker,TableSingle,2026-08-21T12:00:00,2026-08-21T12:00:00,0,1,-0.05,-50.00,-0.05
CAD,GGPoker,Table30Min,2026-08-21T13:00:00,2026-08-21T13:30:00,1800,2,-0.1,-50.00,-0.1
CAD,GGPoker,Table3001,2026-08-21T14:00:00,2026-08-21T14:00:00,0,1,-0.05,-50.00,-0.05
CAD,GGPoker,Table3001,2026-08-21T14:30:01,2026-08-21T14:30:01,0,1,-0.05,-50.00,-0.05
CAD,GGPoker,TableDST,2026-11-01T01:50:00,2026-11-01T01:10:00,1200,2,-0.1,-50.00,-0.1
CAD,GGPoker,TableMixed,2026-11-02T10:00:00,2026-11-02T10:05:00,300,2,-0.1,-50.00,-0.1
";
    assert_eq!(
        report_text(&repo, ReportKind::Sessions, &ReportOptions::default()),
        expected
    );
}

#[test]
fn version_guard_rejects_stale_cache_and_writes_nothing() {
    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    let stored = repo
        .insert_hands_batch(&[hero_sb_folds("H_VG", dec!(0.05), dec!(0.10))])
        .unwrap();
    assert!(stored.rejected.is_empty(), "{:?}", stored.rejected);
    let expected = poker_tracker::stats::STATS_VERSION;

    // 1. Another connection marks the cache as built by another version
    {
        let conn_b = rusqlite::Connection::open(temp_db.path()).unwrap();
        conn_b
            .execute(
                "UPDATE derived_cache SET version = ?1 WHERE name = 'hand_player_stats'",
                [expected + 1],
            )
            .unwrap();
    }
    for kind in [
        ReportKind::Results,
        ReportKind::Positions,
        ReportKind::Sessions,
        ReportKind::Stats,
    ] {
        let mut buf = Vec::new();
        let err = write_report(&repo, kind, &ReportOptions::default(), &mut buf).unwrap_err();
        assert!(
            matches!(
                &err,
                ReportError::Storage(StorageError::StaleDerivedCache { found: Some(v), expected: e })
                    if *v == expected + 1 && *e == expected
            ),
            "{kind:?}: expected StaleDerivedCache, got {err:?}"
        );
        assert!(buf.is_empty(), "{kind:?}: a stale cache must write nothing");
    }

    // 2. With the derived_cache row deleted instead, found is None
    {
        let conn_b = rusqlite::Connection::open(temp_db.path()).unwrap();
        conn_b
            .execute(
                "DELETE FROM derived_cache WHERE name = 'hand_player_stats'",
                [],
            )
            .unwrap();
    }
    let mut buf = Vec::new();
    let err = write_report(
        &repo,
        ReportKind::Results,
        &ReportOptions::default(),
        &mut buf,
    )
    .unwrap_err();
    assert!(
        matches!(
            &err,
            ReportError::Storage(StorageError::StaleDerivedCache { found: None, expected: e })
                if *e == expected
        ),
        "expected StaleDerivedCache with found None, got {err:?}"
    );
    assert!(buf.is_empty(), "a missing cache row must write nothing");
}

#[test]
fn sessions_fail_closed_on_an_unreadable_timestamp() {
    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    let stored = repo
        .insert_hands_batch(&[
            session_hand("T1", "Table", "2026-08-21 12:00:00", None),
            session_hand("T2", "Table", "2026-08-21 12:01:00", None),
        ])
        .unwrap();
    assert!(stored.rejected.is_empty(), "{:?}", stored.rejected);

    {
        let conn_b = rusqlite::Connection::open(temp_db.path()).unwrap();
        conn_b
            .execute(
                "UPDATE hands SET timestamp = '21/08/2026 17:30:00' WHERE site_hand_id = 'T2'",
                [],
            )
            .unwrap();
    }

    let mut buf = Vec::new();
    let err = write_report(
        &repo,
        ReportKind::Sessions,
        &ReportOptions::default(),
        &mut buf,
    )
    .unwrap_err();
    assert!(
        matches!(
            &err,
            ReportError::Storage(StorageError::CorruptRow { table: "hands", detail })
                if detail.contains("21/08/2026 17:30:00")
        ),
        "expected CorruptRow naming the timestamp, got {err:?}"
    );
    assert!(buf.is_empty(), "a corrupt row must write nothing");
}

/// Heads-up turn all-in at 0.05/0.10 that Hero loses: Hero (button, SB) holds Ah Ad, Villain
/// Kh Kd, board 2c 7d 9h Ks, river 3s. On the turn only Ac and As win for Hero, so Hero's
/// equity is 2/44 = 1/22 of the 20.00 pot (no deductions).
fn hero_turn_all_in_with_one_out_in_22(id: &str) -> CanonicalHand {
    let card = |s: &str| Card::from_str(s).unwrap();
    let mut p = vec![
        make_player(1, "Hero", dec!(10), true),
        make_player(2, "Villain", dec!(10), false),
    ];
    p[0].hole_cards = [Some(card("Ah")), Some(card("Ad"))];
    p[1].hole_cards = [Some(card("Kh")), Some(card("Kd"))];
    let act = |seq, street, who, kind, amount, total, all_in| {
        make_action(seq, street, who, kind, amount, total, all_in)
    };
    let a = vec![
        act(
            0,
            Street::Preflop,
            "Hero",
            ActionType::PostSmallBlind,
            dec!(0.05),
            dec!(0.05),
            false,
        ),
        act(
            1,
            Street::Preflop,
            "Villain",
            ActionType::PostBigBlind,
            dec!(0.10),
            dec!(0.10),
            false,
        ),
        act(
            2,
            Street::Preflop,
            "Hero",
            ActionType::Call,
            dec!(0.05),
            dec!(0.10),
            false,
        ),
        act(
            3,
            Street::Preflop,
            "Villain",
            ActionType::Check,
            dec!(0),
            dec!(0.10),
            false,
        ),
        act(
            4,
            Street::Flop,
            "Villain",
            ActionType::Check,
            dec!(0),
            dec!(0),
            false,
        ),
        act(
            5,
            Street::Flop,
            "Hero",
            ActionType::Check,
            dec!(0),
            dec!(0),
            false,
        ),
        act(
            6,
            Street::Turn,
            "Villain",
            ActionType::Bet,
            dec!(9.90),
            dec!(9.90),
            true,
        ),
        act(
            7,
            Street::Turn,
            "Hero",
            ActionType::Call,
            dec!(9.90),
            dec!(9.90),
            true,
        ),
    ];
    let board = CommunityCards {
        run_index: 0,
        flop: Some([card("2c"), card("7d"), card("9h")]),
        turn: Some(card("Ks")),
        river: Some(card("3s")),
    };
    let mut h = make_canonical_hand(1, p, a, vec![board], vec![("Villain", dec!(20.00))]);
    h.outcome.total_pot = dec!(20.00);
    h.header.site_hand_id = id.to_string();
    h
}

/// Three stakes in one currency, every hand by Hero from the SB at one table and one time:
/// - `M1` at 0.05/0.1, the turn all-in above: net -10, equity 1/22, so `ev_net` =
///   round(20.00 / 22) - 10 = 0.909091 - 10 = -9.090909;
/// - `M2` at 0.1/0.2 and `M3` at 0.05/0.25, `hero_sb_folds`: nets -0.1 and -0.05.
///
/// Ordered by big blind the rows are 0.05/0.1, 0.1/0.2, 0.05/0.25 (by small blind first they
/// would be 0.05/0.1, 0.05/0.25, 0.1/0.2). `bb_won` per big blind: -100, -0.5, -0.2, total
/// -100.70; `ev_bb_won`: -90.90909, -0.5, -0.2, total -91.60909.
#[test]
fn multi_stakes_rows_combine_per_big_blind_and_m16_columns() {
    let m1 = hero_turn_all_in_with_one_out_in_22("M1");
    let m2 = hero_sb_folds("M2", dec!(0.10), dec!(0.20));
    let m3 = hero_sb_folds("M3", dec!(0.05), dec!(0.25));
    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    let stored = repo.insert_hands_batch(&[m1, m2, m3]).unwrap();
    assert!(stored.rejected.is_empty(), "{:?}", stored.rejected);

    assert_eq!(
        report_text(&repo, ReportKind::Results, &ReportOptions::default()),
        "currency,stakes,hands,net,net_per_100,bb_won,bb_per_100,rake,fees,blinds,straddles,allin_hands,ev_net,ev_bb_per_100\n\
         CAD,0.05/0.1,1,-10,-1000.00,-100.00,-10000.00,0,0,0.05,0,1,-9.090909,-9090.91\n\
         CAD,0.1/0.2,1,-0.1,-10.00,-0.50,-50.00,0,0,0.1,0,0,-0.1,-50.00\n\
         CAD,0.05/0.25,1,-0.05,-5.00,-0.20,-20.00,0,0,0.05,0,0,-0.05,-20.00\n\
         CAD,ALL,3,-10.15,-338.33,-100.70,-3356.67,0,0,0.2,0,1,-9.240909,-3053.64\n"
    );
    // One SB row across three big blinds: bb/100 = -100.70 / 3 × 100
    assert_eq!(
        report_text(&repo, ReportKind::Positions, &ReportOptions::default()),
        "currency,position,hands,net,bb_per_100,vpip,pfr,rfi\n\
         CAD,SB,3,-10.15,-3356.67,33.33,0.00,0.00\n"
    );
    // One session across three big blinds (same table, same start)
    assert_eq!(
        report_text(&repo, ReportKind::Sessions, &ReportOptions::default()),
        "currency,platform,table,start,end,duration_seconds,hands,net,bb_per_100,ev_net\n\
         CAD,GGPoker,TestTable,2026-08-21T17:00:00,2026-08-21T17:00:00,0,3,-10.15,-3356.67,-9.240909\n"
    );
}

#[test]
fn stakes_allow_exactly_six_decimal_places() {
    // Six decimal places, on either side, are accepted (SPEC_v3.7 M15.3)
    for six in ["0.000001/0.1", "0.05/0.100000"] {
        assert!(
            Stakes::from_str(six).is_ok(),
            "expected '{six}' to be accepted"
        );
    }
    // Seven are rejected even when the extra digit is a trailing zero that micro-units could hold
    for seven in ["0.0500000/0.1", "0.05/0.1000000"] {
        assert!(
            Stakes::from_str(seven).is_err(),
            "expected '{seven}' to be rejected"
        );
    }
}

#[test]
fn report_kinds_parse_from_their_cli_names() {
    // The CLI tests run only `--report results`; every kind must parse to its own variant.
    for (name, kind) in [
        ("results", ReportKind::Results),
        ("positions", ReportKind::Positions),
        ("sessions", ReportKind::Sessions),
        ("stats", ReportKind::Stats),
    ] {
        assert_eq!(ReportKind::from_str(name), Ok(kind), "{name}");
    }
    assert!(ReportKind::from_str("summary").is_err());
}

fn make_hu_hand(
    id: &str,
    sb: Decimal,
    bb: Decimal,
    x: Decimal,
    hero_cards: (&str, &str),
    villain_cards: (&str, &str),
    hero_wins: bool,
) -> CanonicalHand {
    let card = |s: &str| Card::from_str(s).unwrap();
    let mut p = vec![
        make_player(1, "Hero", dec!(100), true),
        make_player(2, "Villain", dec!(100), false),
    ];
    p[0].hole_cards = [Some(card(hero_cards.0)), Some(card(hero_cards.1))];
    p[1].hole_cards = [Some(card(villain_cards.0)), Some(card(villain_cards.1))];
    let act = |seq, street, who, kind, amount, total, all_in| {
        make_action(seq, street, who, kind, amount, total, all_in)
    };
    let pot = (bb + x) * dec!(2);
    let a = vec![
        act(
            0,
            Street::Preflop,
            "Hero",
            ActionType::PostSmallBlind,
            sb,
            sb,
            false,
        ),
        act(
            1,
            Street::Preflop,
            "Villain",
            ActionType::PostBigBlind,
            bb,
            bb,
            false,
        ),
        act(
            2,
            Street::Preflop,
            "Hero",
            ActionType::Call,
            bb - sb,
            bb,
            false,
        ),
        act(
            3,
            Street::Preflop,
            "Villain",
            ActionType::Check,
            Decimal::ZERO,
            bb,
            false,
        ),
        act(
            4,
            Street::Flop,
            "Villain",
            ActionType::Check,
            Decimal::ZERO,
            Decimal::ZERO,
            false,
        ),
        act(5, Street::Flop, "Hero", ActionType::Bet, x, x, false),
        act(6, Street::Flop, "Villain", ActionType::Call, x, x, false),
        act(
            7,
            Street::Turn,
            "Villain",
            ActionType::Check,
            Decimal::ZERO,
            Decimal::ZERO,
            false,
        ),
        act(
            8,
            Street::Turn,
            "Hero",
            ActionType::Check,
            Decimal::ZERO,
            Decimal::ZERO,
            false,
        ),
        act(
            9,
            Street::River,
            "Villain",
            ActionType::Check,
            Decimal::ZERO,
            Decimal::ZERO,
            false,
        ),
        act(
            10,
            Street::River,
            "Hero",
            ActionType::Check,
            Decimal::ZERO,
            Decimal::ZERO,
            false,
        ),
    ];
    let board = CommunityCards {
        run_index: 0,
        flop: Some([card("2c"), card("7d"), card("9h")]),
        turn: Some(card("Ks")),
        river: Some(card("3s")),
    };
    let winner_name = if hero_wins { "Hero" } else { "Villain" };
    let mut h = make_canonical_hand(1, p, a, vec![board], vec![(winner_name, pot)]);
    h.header.site_hand_id = id.to_string();
    h.header.small_blind = sb;
    h.header.big_blind = bb;
    h.outcome.total_pot = pot;
    h
}

#[test]
fn bb_won_aggregation_exact_rational_results_positions_sessions() {
    let l1 = make_hu_hand(
        "L1",
        dec!(1.5),
        dec!(3),
        dec!(26.99),
        ("Ah", "Ad"),
        ("Kh", "Kd"),
        false,
    );
    let w1 = make_hu_hand(
        "W1",
        dec!(3),
        dec!(6),
        dec!(17.59),
        ("Kh", "Kd"),
        ("Ah", "Ad"),
        true,
    );

    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    let stored = repo.insert_hands_batch(&[l1, w1]).unwrap();
    assert!(stored.rejected.is_empty(), "{:?}", stored.rejected);

    assert_eq!(
        report_text(&repo, ReportKind::Results, &ReportOptions::default()),
        "currency,stakes,hands,net,net_per_100,bb_won,bb_per_100,rake,fees,blinds,straddles,allin_hands,ev_net,ev_bb_per_100\n\
         CAD,1.5/3,1,-29.99,-2999.00,-10.00,-999.67,0,0,1.5,0,0,-29.99,-999.67\n\
         CAD,3/6,1,23.59,2359.00,3.93,393.17,0,0,3,0,0,23.59,393.17\n\
         CAD,ALL,2,-6.4,-320.00,-6.06,-303.25,0,0,4.5,0,0,-6.4,-303.25\n"
    );

    assert_eq!(
        report_text(&repo, ReportKind::Positions, &ReportOptions::default()),
        "currency,position,hands,net,bb_per_100,vpip,pfr,rfi\n\
         CAD,SB,2,-6.4,-303.25,100.00,0.00,0.00\n"
    );

    assert_eq!(
        report_text(&repo, ReportKind::Sessions, &ReportOptions::default()),
        "currency,platform,table,start,end,duration_seconds,hands,net,bb_per_100,ev_net\n\
         CAD,GGPoker,TestTable,2026-08-21T17:00:00,2026-08-21T17:00:00,0,2,-6.4,-303.25,-6.4\n"
    );
}

#[test]
fn csv_text_fields_quoted_when_needed() {
    let mut h = make_hu_hand(
        "Q1",
        dec!(0.05),
        dec!(0.10),
        dec!(1.00),
        ("Ah", "Ad"),
        ("Kh", "Kd"),
        true,
    );
    h.header.table_name = "Table, \"A\"".to_string();
    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    repo.insert_hands_batch(&[h]).unwrap();

    let text = report_text(&repo, ReportKind::Sessions, &ReportOptions::default());
    assert!(
        text.contains("CAD,GGPoker,\"Table, \"\"A\"\"\","),
        "expected quoted table name in sessions report, got:\n{text}"
    );
}

/// `Some([])` selects no hand (SPEC_v3.8 M17.1.4: `<stakes>` is `0`): every report is its header
/// line, and nothing is unmatched.
#[test]
fn an_empty_stakes_list_selects_no_hand() {
    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    let stored = repo
        .insert_hands_batch(&[hero_sb_folds("H_EMPTY", dec!(0.05), dec!(0.10))])
        .unwrap();
    assert!(stored.rejected.is_empty(), "{:?}", stored.rejected);

    let empty = ReportOptions {
        stakes: Some(Vec::new()),
    };
    for kind in [
        ReportKind::Results,
        ReportKind::Positions,
        ReportKind::Sessions,
        ReportKind::Stats,
    ] {
        let all = report_text(&repo, kind, &ReportOptions::default());
        assert!(all.lines().count() > 1, "{kind:?}: {all}");
        let mut buf = Vec::new();
        let outcome = write_report(&repo, kind, &empty, &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert_eq!(
            text.lines().collect::<Vec<_>>(),
            all.lines().take(1).collect::<Vec<_>>(),
            "{kind:?}"
        );
        assert!(outcome.unmatched_stakes.is_empty(), "{kind:?}");
    }
}

/// Two thousand stakes items select as one: the stakes condition does not grow with the list
/// (an `OR` chain of about 994 pairs exceeds SQLite's expression depth; SPEC_v3.8 amendment A1).
#[test]
fn two_thousand_stakes_items_report_like_one() {
    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    let stored = repo
        .insert_hands_batch(&[hero_sb_folds("H_MANY", dec!(0.05), dec!(0.10))])
        .unwrap();
    assert!(stored.rejected.is_empty(), "{:?}", stored.rejected);

    let mut items: Vec<String> = (1..2000).map(|i| format!("0.05/{i}")).collect();
    items.push("0.05/0.1".to_string());
    let refs: Vec<&str> = items.iter().map(String::as_str).collect();
    let many = stakes_options(&refs);
    let one = stakes_options(&["0.05/0.1"]);
    for kind in [
        ReportKind::Results,
        ReportKind::Positions,
        ReportKind::Sessions,
        ReportKind::Stats,
    ] {
        let mut buf = Vec::new();
        let outcome = write_report(&repo, kind, &many, &mut buf).unwrap();
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            report_text(&repo, kind, &one),
            "{kind:?}"
        );
        assert_eq!(outcome.unmatched_stakes.len(), 1999, "{kind:?}");
    }
}
