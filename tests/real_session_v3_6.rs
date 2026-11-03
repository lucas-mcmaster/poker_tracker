//! Whole-folder integration and reconciliation tests against the 10 v3.6 real GGPoker exports
//! from a CAD account (`tests/fixtures/ggpoker/v3_6/`, 989 hands).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use pretty_assertions::assert_eq;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use common::{FIXTURE_CURRENCY, SAMPLE_SESSION, gg};
use poker_tracker::domain::{CanonicalHand, CurrencyCode};
use poker_tracker::parser::{GgPokerParser, HandParser, ParserRegistry};
use poker_tracker::pipeline::{DirectoryIngester, StackSnapshot, check_stack_continuity};
use poker_tracker::stats::position_label;
use poker_tracker::storage::SqliteRepository;

struct SessionFileInfo {
    file_name: &'static str,
    byte_len: usize,
    sha256: &'static str,
    hand_count: usize,
    hero_net: Decimal,
}

const V3_6_FILES: &[SessionFileInfo] = &[
    SessionFileInfo {
        file_name: "GG20260821-2054 - NLHRed14 - 0.05 - 0.1 - 6max.txt",
        byte_len: 20_657,
        sha256: "befb8e27ceaa028d93c17732ac84541c1a9fe5f411831c5da499114ce2d56f86",
        hand_count: 24,
        hero_net: dec!(7.75),
    },
    SessionFileInfo {
        file_name: "GG20260821-2054 - NLHRed18 - 0.05 - 0.1 - 6max.txt",
        byte_len: 204_951,
        sha256: "4c28f0b7035e819ad99c271f9c19f3cfb5cbd06bfac07f1d8f1a926ef2b7478b",
        hand_count: 167,
        hero_net: dec!(-17.71),
    },
    SessionFileInfo {
        file_name: "GG20260821-2106 - NLHRed4 - 0.05 - 0.1 - 6max.txt",
        byte_len: 202_035,
        sha256: "e9125ba5a5712af971df86c2b71f635176eafa36397bdbe775434128d821b053",
        hand_count: 182,
        hero_net: dec!(-3.03),
    },
    SessionFileInfo {
        file_name: "GG20260821-2207 - NLHRed10 - 0.05 - 0.1 - 6max.txt",
        byte_len: 96_525,
        sha256: "b06d5914fb879555b4ec002e57a7e7ba33c53cbede7bcc8d29026ff3c9519336",
        hand_count: 87,
        hero_net: dec!(3.78),
    },
    SessionFileInfo {
        file_name: "GG20260821-2247 - NLHRed14 - 0.05 - 0.1 - 6max.txt",
        byte_len: 105_665,
        sha256: "3cf349c3694e04582d23362bf132fc7809fa8018261d83dce1c902e6853db3f9",
        hand_count: 90,
        hero_net: dec!(-1.00),
    },
    SessionFileInfo {
        file_name: "GG20260821-2252 - NLHRed19 - 0.05 - 0.1 - 6max.txt",
        byte_len: 119_476,
        sha256: "81302080e47308953b5357cf0f6367a9fc02f5aac424b5d9d7472d6ffd689d31",
        hand_count: 102,
        hero_net: dec!(9.41),
    },
    SessionFileInfo {
        file_name: "GG20260821-2254 - NLHRed7 - 0.05 - 0.1 - 6max.txt",
        byte_len: 97_832,
        sha256: "8ffe6652bb90e707cfd915459f334ee46153d8f0e63c6f84afa5e11510448660",
        hand_count: 84,
        hero_net: dec!(-17.48),
    },
    SessionFileInfo {
        file_name: "GG20260925-0500 - NLHRed21 - 0.05 - 0.1 - 6max.txt",
        byte_len: 114_214,
        sha256: "9f8de2fac1d3e421f2635d96523ce6a6d58b7c188dde037b0371a4d29a8e1bb1",
        hand_count: 99,
        hero_net: dec!(-3.25),
    },
    SessionFileInfo {
        file_name: "GG20260925-0539 - NLHRed3 - 0.05 - 0.1 - 6max.txt",
        byte_len: 101_613,
        sha256: "0a0cb40fbd4c49e8d5ef94cfe3153612c58106eeb1462835dae1f3ef46f32d63",
        hand_count: 100,
        hero_net: dec!(3.45),
    },
    SessionFileInfo {
        file_name: "GG20260925-0618 - NLHRed15 - 0.05 - 0.1 - 6max.txt",
        byte_len: 67_774,
        sha256: "cce897605f1bb5f8fca5a834ba085a64e1a3951248d5282d716c8f47aef13c0b",
        hand_count: 54,
        hero_net: dec!(-7.71),
    },
];

fn registry() -> ParserRegistry {
    ParserRegistry::new().with(GgPokerParser::new(CurrencyCode::CAD))
}

fn parse_file_hands(path: &Path) -> Vec<(usize, CanonicalHand)> {
    let content = fs::read_to_string(path).unwrap();
    let parser = gg();
    let mut hands = Vec::new();
    for (block, result) in parser.parse_session(&content) {
        match result {
            Ok(h) => hands.push((block.start_line, h)),
            Err(e) => panic!(
                "file {} hand {:?} at line {} failed: {e}",
                path.display(),
                parser.peek_hand_id(block.text),
                block.start_line
            ),
        }
    }
    hands
}

#[test]
fn file_digests_and_lengths_match_spec() {
    let dir = Path::new("tests/fixtures/ggpoker/v3_6");
    let mut total_bytes = 0;
    for info in V3_6_FILES {
        let path = dir.join(info.file_name);
        let bytes =
            fs::read(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        total_bytes += bytes.len();
        assert_eq!(
            bytes.len(),
            info.byte_len,
            "byte length mismatch for {}",
            info.file_name
        );
        let digest = Sha256::digest(&bytes);
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, info.sha256, "SHA-256 mismatch for {}", info.file_name);
    }
    assert_eq!(total_bytes, 1_130_742);
}

#[test]
fn all_v3_6_files_parse_with_chip_conservation_and_expected_nets() {
    let dir = Path::new("tests/fixtures/ggpoker/v3_6");
    let mut total_hands = 0;
    let mut total_hero_net = Decimal::ZERO;

    for info in V3_6_FILES {
        let path = dir.join(info.file_name);
        let hands = parse_file_hands(&path);
        assert_eq!(
            hands.len(),
            info.hand_count,
            "hand count mismatch for {}",
            info.file_name
        );
        total_hands += hands.len();

        let mut file_hero_net = Decimal::ZERO;
        let mut snapshots = Vec::new();

        for (line, h) in &hands {
            assert_eq!(h.header.currency, FIXTURE_CURRENCY);

            // Chip conservation: sum(results.net) == adjustments - deductions
            let mut hand_net = Decimal::ZERO;
            for r in h.checked_player_results().unwrap() {
                hand_net += r.net;
                if r.player_name == "Hero" {
                    file_hero_net += r.net;
                }
            }
            let deductions = h.outcome.checked_total_deductions().unwrap();
            let adjustments = h.outcome.checked_total_adjustments().unwrap();
            assert_eq!(
                hand_net,
                adjustments - deductions,
                "chip conservation failed for hand {} in {}",
                h.header.site_hand_id,
                info.file_name
            );

            snapshots.push(StackSnapshot::of(h, *line).unwrap());
        }

        // Stack continuity: 0 warnings
        let warnings = check_stack_continuity(&path, &mut snapshots, &[]);
        assert!(
            warnings.is_empty(),
            "unexpected stack warnings in {}: {:?}",
            info.file_name,
            warnings
        );

        // Hero net per file
        assert_eq!(
            file_hero_net, info.hero_net,
            "Hero net mismatch for {}",
            info.file_name
        );
        total_hero_net += file_hero_net;
    }

    assert_eq!(total_hands, 989);
    assert_eq!(total_hero_net, dec!(-25.79));
}

#[test]
fn directory_ingester_imports_v3_6_with_idempotency_and_duplicates() {
    let fixtures_dir = Path::new("tests/fixtures/ggpoker/v3_6");
    let ingester = DirectoryIngester::default();
    let mut repo = SqliteRepository::open_in_memory().unwrap();

    // 1. First import: 10 files processed, 989 ingested, 0 failed, 0 quarantined, 0 stack warnings
    let report1 = ingester
        .ingest_directory(fixtures_dir, &registry(), &mut repo)
        .unwrap();
    assert_eq!(report1.files_processed, 10);
    assert_eq!(report1.hands_ingested, 989);
    assert_eq!(report1.duplicates_skipped, 0);
    assert_eq!(report1.hands_failed, 0);
    assert_eq!(report1.hands_quarantined, 0);
    assert!(report1.file_errors.is_empty());
    assert!(report1.stack_warnings.is_empty());

    // 2. Second import of the same folder: 0 ingested, 989 duplicates
    let report2 = ingester
        .ingest_directory(fixtures_dir, &registry(), &mut repo)
        .unwrap();
    assert_eq!(report2.files_processed, 10);
    assert_eq!(report2.hands_ingested, 0);
    assert_eq!(report2.duplicates_skipped, 989);
    assert_eq!(report2.hands_failed, 0);
    assert_eq!(report2.hands_quarantined, 0);
    assert!(report2.file_errors.is_empty());
    assert!(report2.stack_warnings.is_empty());

    // 3. 2 hands in 20260821-2054_NLHRed14.txt are also in SAMPLE_SESSION (HD140650645 and HD140650460).
    // Verify that importing SAMPLE_SESSION after that file counts them as 2 duplicates.
    let sample_dir = TempDir::new().unwrap();
    fs::write(sample_dir.path().join("sample.txt"), SAMPLE_SESSION).unwrap();
    let report_sample = ingester
        .ingest_directory(sample_dir.path(), &registry(), &mut repo)
        .unwrap();
    assert_eq!(report_sample.files_processed, 1);
    assert_eq!(report_sample.hands_ingested, 0);
    assert_eq!(report_sample.duplicates_skipped, 2);
    assert_eq!(report_sample.hands_failed, 0);
    assert_eq!(report_sample.hands_quarantined, 0);
    assert!(report_sample.file_errors.is_empty());
}

#[test]
fn sample_session_hands_are_duplicates_after_single_file_import() {
    // Specifically test importing 20260821-2054_NLHRed14 alone, then importing SAMPLE_SESSION
    let ingester = DirectoryIngester::default();
    let mut repo = SqliteRepository::open_in_memory().unwrap();

    let first_file_dir = TempDir::new().unwrap();
    fs::copy(
        Path::new("tests/fixtures/ggpoker/v3_6")
            .join("GG20260821-2054 - NLHRed14 - 0.05 - 0.1 - 6max.txt"),
        first_file_dir.path().join("20260821-2054_NLHRed14.txt"),
    )
    .unwrap();

    let report_file = ingester
        .ingest_directory(first_file_dir.path(), &registry(), &mut repo)
        .unwrap();
    assert_eq!(report_file.files_processed, 1);
    assert_eq!(report_file.hands_ingested, 24);
    assert_eq!(report_file.duplicates_skipped, 0);

    let sample_dir = TempDir::new().unwrap();
    fs::write(sample_dir.path().join("sample.txt"), SAMPLE_SESSION).unwrap();
    let report_sample = ingester
        .ingest_directory(sample_dir.path(), &registry(), &mut repo)
        .unwrap();
    assert_eq!(report_sample.files_processed, 1);
    assert_eq!(report_sample.hands_ingested, 0);
    assert_eq!(report_sample.duplicates_skipped, 2);
    assert_eq!(report_sample.hands_failed, 0);
}

#[test]
fn opponent_hashes_are_isolated_to_single_sessions() {
    // M10.3: across the 10 files, no opponent hash appears at two distinct files/table sessions
    // (101 unique opponent names excluding "Hero", 101 file appearances).
    let dir = Path::new("tests/fixtures/ggpoker/v3_6");
    let mut file_opponents: Vec<(&'static str, HashSet<String>)> = Vec::new();
    let mut all_opponents = HashSet::new();
    let mut total_appearances = 0;

    for info in V3_6_FILES {
        let path = dir.join(info.file_name);
        let hands = parse_file_hands(&path);
        let mut opponents_in_file = HashSet::new();
        for (_, h) in &hands {
            for p in &h.players {
                if !p.is_hero {
                    opponents_in_file.insert(p.name.clone());
                }
            }
        }
        total_appearances += opponents_in_file.len();
        for opp in &opponents_in_file {
            all_opponents.insert(opp.clone());
        }
        file_opponents.push((info.file_name, opponents_in_file));
    }

    assert_eq!(file_opponents.len(), 10);
    assert_eq!(total_appearances, 101);
    assert_eq!(all_opponents.len(), 101);

    // Pairwise disjoint: no opponent hash appears in more than one file
    for i in 0..file_opponents.len() {
        for j in (i + 1)..file_opponents.len() {
            let (f1, opps1) = &file_opponents[i];
            let (f2, opps2) = &file_opponents[j];
            let shared: Vec<_> = opps1.intersection(opps2).collect();
            assert!(
                shared.is_empty(),
                "opponent hashes {:?} appear in both {} and {}",
                shared,
                f1,
                f2
            );
        }
    }
}

#[test]
fn hero_positions_and_m13_counters_match_pinned_tables() {
    let fixtures_dir = Path::new("tests/fixtures/ggpoker/v3_6");
    let ingester = DirectoryIngester::default();
    let mut repo = SqliteRepository::open_in_memory().unwrap();

    let report = ingester
        .ingest_directory(fixtures_dir, &registry(), &mut repo)
        .unwrap();
    assert_eq!(report.hands_ingested, 989);

    // Verify positions for Hero across the 989 hands
    let mut stmt = repo
        .connection()
        .prepare(
            "SELECT hps.position, h.max_seats FROM hand_player_stats hps
             JOIN hand_players hp ON hp.hand_id = hps.hand_id AND hp.player_id = hps.player_id
             JOIN hands h ON h.id = hps.hand_id
             WHERE hp.is_hero = 1",
        )
        .unwrap();
    let hero_pos_rows: Vec<(Option<u8>, u8)> = stmt
        .query_map([], |row| {
            let pos_code: Option<i64> = row.get(0)?;
            let max_seats: i64 = row.get(1)?;
            Ok((pos_code.map(|c| c as u8), max_seats as u8))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert_eq!(hero_pos_rows.len(), 989);
    let mut pos_counts: std::collections::HashMap<&'static str, usize> =
        std::collections::HashMap::new();
    let mut null_positions = 0;
    for (pos_code, max_seats) in hero_pos_rows {
        match pos_code {
            Some(code) => {
                let label = position_label(code, max_seats);
                *pos_counts.entry(label).or_insert(0) += 1;
            }
            None => null_positions += 1,
        }
    }

    assert_eq!(
        null_positions, 0,
        "No player-hand in the corpus has a NULL position"
    );
    assert_eq!(pos_counts.get("EP"), Some(&96));
    assert_eq!(pos_counts.get("MP"), Some(&139));
    assert_eq!(pos_counts.get("CO"), Some(&178));
    assert_eq!(pos_counts.get("BTN"), Some(&185));
    assert_eq!(pos_counts.get("SB"), Some(&197));
    assert_eq!(pos_counts.get("BB"), Some(&194));

    // Verify all 6,840 player-hands in corpus have no NULL positions
    let null_all: i64 = repo
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM hand_player_stats WHERE position IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(null_all, 0);

    // Sum Hero's counters for v3.6 folder
    let hero_sums_v36 = |col: &str| -> i64 {
        let sql = format!(
            "SELECT SUM(hps.{col}) FROM hand_player_stats hps
             JOIN hand_players hp ON hp.hand_id = hps.hand_id AND hp.player_id = hps.player_id
             WHERE hp.is_hero = 1"
        );
        repo.connection().query_row(&sql, [], |r| r.get(0)).unwrap()
    };

    assert_eq!(hero_sums_v36("hands"), 989);
    assert_eq!(hero_sums_v36("p_walk"), 35);
    assert_eq!(hero_sums_v36("p_vpip"), 258);
    assert_eq!(hero_sums_v36("p_pfr_opp"), 954);
    assert_eq!(hero_sums_v36("p_pfr"), 224);
    assert_eq!(hero_sums_v36("f_saw"), 198);
    assert_eq!(hero_sums_v36("t_saw"), 133);
    assert_eq!(hero_sums_v36("r_saw"), 98);
    assert_eq!(hero_sums_v36("sd_went"), 55);
    assert_eq!(hero_sums_v36("sd_won"), 26);
    assert_eq!(hero_sums_v36("won_hand"), 193);
    assert_eq!(hero_sums_v36("f_saw_won"), 87);

    // Action counts for Hero in v3.6 folder
    assert_eq!(hero_sums_v36("p_bets"), 0);
    assert_eq!(hero_sums_v36("p_raises"), 228);
    assert_eq!(hero_sums_v36("p_calls"), 47);
    assert_eq!(hero_sums_v36("p_checks"), 30);
    assert_eq!(hero_sums_v36("p_folds"), 685);

    assert_eq!(hero_sums_v36("f_bets"), 60);
    assert_eq!(hero_sums_v36("f_raises"), 13);
    assert_eq!(hero_sums_v36("f_calls"), 19);
    assert_eq!(hero_sums_v36("f_checks"), 107);
    assert_eq!(hero_sums_v36("f_folds"), 36);

    assert_eq!(hero_sums_v36("t_bets"), 35);
    assert_eq!(hero_sums_v36("t_raises"), 7);
    assert_eq!(hero_sums_v36("t_calls"), 25);
    assert_eq!(hero_sums_v36("t_checks"), 55);
    assert_eq!(hero_sums_v36("t_folds"), 22);

    assert_eq!(hero_sums_v36("r_bets"), 23);
    assert_eq!(hero_sums_v36("r_raises"), 11);
    assert_eq!(hero_sums_v36("r_calls"), 6);
    assert_eq!(hero_sums_v36("r_checks"), 35);
    assert_eq!(hero_sums_v36("r_folds"), 24);

    // Now test v3.5 file (294 hands)
    let mut repo35 = SqliteRepository::open_in_memory().unwrap();
    let v35_hands: Vec<CanonicalHand> = gg()
        .parse_session(common::REAL_SESSION)
        .map(|(_, r)| r.unwrap())
        .collect();
    assert_eq!(v35_hands.len(), 294);
    repo35.insert_hands_batch(&v35_hands).unwrap();

    let hero_sums_v35 = |col: &str| -> i64 {
        let sql = format!(
            "SELECT SUM(hps.{col}) FROM hand_player_stats hps
             JOIN hand_players hp ON hp.hand_id = hps.hand_id AND hp.player_id = hps.player_id
             WHERE hp.is_hero = 1"
        );
        repo35
            .connection()
            .query_row(&sql, [], |r| r.get(0))
            .unwrap()
    };

    assert_eq!(hero_sums_v35("hands"), 294);
    assert_eq!(hero_sums_v35("p_walk"), 15);
    assert_eq!(hero_sums_v35("p_vpip"), 67);
    assert_eq!(hero_sums_v35("p_pfr_opp"), 279);
    assert_eq!(hero_sums_v35("p_pfr"), 57);
    assert_eq!(hero_sums_v35("f_saw"), 50);
    assert_eq!(hero_sums_v35("t_saw"), 29);
    assert_eq!(hero_sums_v35("r_saw"), 23);
    assert_eq!(hero_sums_v35("sd_went"), 16);
    assert_eq!(hero_sums_v35("sd_won"), 8);
    assert_eq!(hero_sums_v35("won_hand"), 65);
    assert_eq!(hero_sums_v35("f_saw_won"), 28);
}
