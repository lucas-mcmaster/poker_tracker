//! End-to-end directory ingestion tests, including per-file parser detection.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;

use tempfile::TempDir;

use common::{REAL_SESSION, SAMPLE_SESSION, WALK, gg};
use poker_tracker::domain::CurrencyCode;
use poker_tracker::parser::{GgPokerParser, HandParser, ParseError, ParserRegistry};
use poker_tracker::pipeline::{DirectoryIngester, FailureStage};
use poker_tracker::storage::SqliteRepository;

fn registry() -> ParserRegistry {
    ParserRegistry::new().with(gg())
}

#[test]
fn ingests_real_session_with_no_failures() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("session.txt"), REAL_SESSION).unwrap();
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let report = DirectoryIngester::default()
        .ingest_directory(dir.path(), &registry(), &mut repo)
        .unwrap();
    assert_eq!(report.files_processed, 1);
    assert_eq!(report.files_by_parser.get("GGPoker (CAD)"), Some(&1));
    assert_eq!(report.hands_ingested, 294);
    assert_eq!(report.hands_failed, 0);
    assert!(report.file_errors.is_empty());

    // Every stored hand records the run, file and line it came from.
    let (with_source, runs): (i64, i64) = repo
        .connection()
        .query_row(
            "SELECT COUNT(*), COUNT(DISTINCT import_run_id) FROM hands
             WHERE source_file LIKE '%session.txt' AND source_line >= 1 AND import_run_id = ?1",
            [report.batch_run_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((with_source, runs), (294, 1));
    let first_line: i64 = repo
        .connection()
        .query_row(
            "SELECT source_line FROM hands WHERE site_hand_id = 'HD140647837'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(first_line, 1);
}

#[test]
fn unrecognized_and_ambiguous_files_are_reported_not_guessed() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a_gg.txt"), SAMPLE_SESSION).unwrap();
    fs::write(
        dir.path().join("b_other_site.txt"),
        "PokerStars Hand #254000000000: Hold'em No Limit ($0.05/$0.10 USD)\n",
    )
    .unwrap();

    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let report = DirectoryIngester::default()
        .ingest_directory(dir.path(), &registry(), &mut repo)
        .unwrap();
    assert_eq!(report.files_processed, 1);
    assert_eq!(report.hands_ingested, 2);
    assert_eq!(report.file_errors.len(), 1);
    assert!(report.file_errors[0].contains("b_other_site.txt"));
    assert!(report.file_errors[0].contains("no registered parser"));

    // Two sources claiming the same format is a configuration error, never a guess.
    let ambiguous = ParserRegistry::new()
        .with(GgPokerParser::new(CurrencyCode::CAD))
        .with(GgPokerParser::new(CurrencyCode::USD));
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let report = DirectoryIngester::default()
        .ingest_directory(dir.path(), &ambiguous, &mut repo)
        .unwrap();
    assert_eq!(report.hands_ingested, 0);
    assert!(
        report
            .file_errors
            .iter()
            .any(|e| e.contains("GGPoker (CAD)") && e.contains("GGPoker (USD)"))
    );
}

#[test]
fn sorts_files_skips_duplicates_and_pinpoints_failures() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a_folder");
    let b = dir.path().join("b_folder");
    fs::create_dir_all(&a).unwrap();
    fs::create_dir_all(&b).unwrap();
    fs::write(a.join("session1.txt"), SAMPLE_SESSION).unwrap();
    // Same two hands again; the first one is corrupted.
    let broken = SAMPLE_SESSION.replace("Dealt to Hero [5h 2s]", "Dealt to Hero [1h 2s]");
    fs::write(b.join("session2.txt"), &broken).unwrap();
    fs::write(b.join("notes.md"), "ignored").unwrap();

    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let report = DirectoryIngester::with_batch_size(1)
        .ingest_directory(dir.path(), &registry(), &mut repo)
        .unwrap();

    assert_eq!(report.files_processed, 2);
    assert_eq!(report.hands_ingested, 2);
    assert_eq!(report.duplicates_skipped, 1);
    assert_eq!(report.hands_failed, 1);

    let failure = &report.hand_failures[0];
    assert!(failure.file_path.ends_with("b_folder/session2.txt"));
    assert_eq!(failure.parser, "GGPoker (CAD)");
    assert_eq!(failure.hand_id.as_deref(), Some("HD140650645"));
    assert_eq!(failure.block_start_line, 1);
    assert!(matches!(
        failure.stage,
        FailureStage::Parse(ParseError::InvalidCardSyntax { .. })
    ));
    let message = failure.to_string();
    assert!(
        message.contains("session2.txt:1 [GGPoker (CAD)] hand HD140650645"),
        "{message}"
    );
}

#[test]
fn failure_line_points_into_the_source_file() {
    let dir = TempDir::new().unwrap();
    let broken = SAMPLE_SESSION.replace(
        "Hero: checks\n24eaa482: checks",
        "Hero: dances\n24eaa482: checks",
    );
    fs::write(dir.path().join("s.txt"), &broken).unwrap();
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let report = DirectoryIngester::default()
        .ingest_directory(dir.path(), &registry(), &mut repo)
        .unwrap();
    assert_eq!(report.hands_failed, 1);
    let expected_line = broken.lines().position(|l| l == "Hero: dances").unwrap() + 1;
    assert_eq!(report.hand_failures[0].file_line(), expected_line);
    assert_eq!(
        report.hand_failures[0].hand_id.as_deref(),
        Some("HD140650460")
    );
}

// ---------------------------------------------------------------------------------------------
// v3.5 review fixes
// ---------------------------------------------------------------------------------------------

fn count(repo: &SqliteRepository, sql: &str) -> i64 {
    repo.connection().query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn rejected_hands_are_quarantined_with_their_raw_text() {
    let dir = TempDir::new().unwrap();
    let broken = SAMPLE_SESSION.replace("Dealt to Hero [5h 2s]", "Dealt to Hero [1h 2s]");
    fs::write(dir.path().join("a.txt"), &broken).unwrap();
    // WALK already stored in USD, so the CAD import of it is a storage rejection.
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    repo.insert_hands_batch(&[GgPokerParser::new(CurrencyCode::USD)
        .parse_hand(WALK)
        .unwrap()])
        .unwrap();
    fs::write(dir.path().join("b.txt"), WALK).unwrap();

    let report = DirectoryIngester::default()
        .ingest_directory(dir.path(), &registry(), &mut repo)
        .unwrap();
    assert_eq!(report.hands_ingested, 1);
    assert_eq!(report.hands_failed, 2);
    assert_eq!(report.hands_quarantined, 2);
    assert!(!report.is_complete());
    let quarantined: Vec<(String, String, String)> = {
        let mut stmt = repo
            .connection()
            .prepare("SELECT stage, site_hand_id, raw_text FROM quarantined_hands ORDER BY stage")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(quarantined[0].0, "parse");
    assert_eq!(quarantined[0].1, "HD140650645");
    assert!(quarantined[0].2.contains("Dealt to Hero [1h 2s]"));
    assert_eq!(quarantined[1].0, "storage");
    assert_eq!(quarantined[1].2, WALK);
    // The stored hand keeps its raw text and an absolute source path.
    assert_eq!(
        count(
            &repo,
            "SELECT COUNT(*) FROM hands WHERE raw_text LIKE 'Poker Hand #HD140650460%'"
        ),
        1
    );
    let file: String = repo
        .connection()
        .query_row(
            "SELECT source_file FROM hands WHERE site_hand_id = 'HD140650460'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(std::path::Path::new(&file).is_absolute(), "{file}");
    // The run's start time is recorded, not the time of its first write.
    let started: String = repo
        .connection()
        .query_row(
            "SELECT started_at FROM import_runs WHERE id = ?1",
            [report.batch_run_id.to_string()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        started,
        report.started_at.format("%Y-%m-%dT%H:%M:%SZ").to_string()
    );
}

#[test]
fn a_stack_that_shrinks_between_consecutive_hands_is_reported() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("session.txt"), REAL_SESSION).unwrap();
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let report = DirectoryIngester::default()
        .ingest_directory(dir.path(), &registry(), &mut repo)
        .unwrap();
    assert!(
        report.stack_warnings.is_empty(),
        "{:?}",
        report.stack_warnings
    );

    // The newest hand's starting stack lowered by 0.01: still a valid hand on its own.
    let edited = REAL_SESSION.replacen(
        "Seat 2: Hero ($18.85 in chips)",
        "Seat 2: Hero ($18.84 in chips)",
        1,
    );
    assert_ne!(edited, REAL_SESSION);
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("session.txt"), &edited).unwrap();
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let report = DirectoryIngester::default()
        .ingest_directory(dir.path(), &registry(), &mut repo)
        .unwrap();
    assert_eq!(report.hands_ingested, 294);
    assert_eq!(report.stack_warnings.len(), 1);
    let w = &report.stack_warnings[0];
    assert!(w.is_hero);
    assert_eq!(
        (w.previous_hand_id.as_str(), w.hand_id.as_str()),
        ("HD140647808", "HD140647837")
    );
    assert!(
        w.to_string().contains("'Hero' (hero) starts with 18.84"),
        "{w}"
    );
    assert!(report.is_complete(), "a warning is not a failure");

    // A hand that fails to parse leaves a gap that is not reported as a stack change.
    let broken = REAL_SESSION.replacen("*** FIRST SHOWDOWN ***", "*** FIRST SHOWDOWN ** *", 1);
    assert_ne!(broken, REAL_SESSION);
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("session.txt"), &broken).unwrap();
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let report = DirectoryIngester::default()
        .ingest_directory(dir.path(), &registry(), &mut repo)
        .unwrap();
    assert_eq!(report.hands_failed, 1);
    assert!(
        report.stack_warnings.is_empty(),
        "{:?}",
        report.stack_warnings
    );
}

#[test]
fn symlinked_exports_are_followed_and_an_empty_input_is_reported() {
    let empty = TempDir::new().unwrap();
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let report = DirectoryIngester::default()
        .ingest_directory(empty.path(), &registry(), &mut repo)
        .unwrap();
    assert_eq!(report.files_processed, 0);
    assert!(report.file_errors[0].contains("no hand-history files"));
    assert!(!report.is_complete());
    assert_eq!(
        count(&repo, "SELECT COUNT(*) FROM import_runs"),
        0,
        "no empty run recorded"
    );

    #[cfg(unix)]
    {
        let exports = TempDir::new().unwrap();
        fs::write(exports.path().join("session.txt"), SAMPLE_SESSION).unwrap();
        let linked = TempDir::new().unwrap();
        std::os::unix::fs::symlink(exports.path(), linked.path().join("exports")).unwrap();
        let report = DirectoryIngester::default()
            .ingest_directory(linked.path(), &registry(), &mut repo)
            .unwrap();
        assert_eq!((report.files_processed, report.hands_ingested), (1, 2));
    }
}

/// A one-player snapshot for the stack-continuity unit tests below.
fn snapshot(
    table: &str,
    time: &str,
    line: usize,
    starting: rust_decimal::Decimal,
    ending: rust_decimal::Decimal,
) -> poker_tracker::pipeline::StackSnapshot {
    poker_tracker::pipeline::StackSnapshot {
        platform: poker_tracker::domain::Platform::GGPoker,
        table_name: table.to_string(),
        timestamp: chrono::NaiveDateTime::parse_from_str(time, "%Y-%m-%d %H:%M:%S").unwrap(),
        hand_id: format!("{table}-{line}"),
        block_start_line: line,
        players: vec![poker_tracker::pipeline::PlayerStacks {
            name: "P".to_string(),
            is_hero: false,
            starting,
            ending,
        }],
    }
}

#[test]
fn stack_continuity_compares_only_one_table_and_skips_across_unparsed_blocks() {
    use poker_tracker::pipeline::check_stack_continuity;
    use rust_decimal_macros::dec;
    let path = std::path::Path::new("f.txt");

    // Control: one table, consecutive hands, the stack shrinks from 10 to 5: one warning.
    let mut one_table = vec![
        snapshot("A", "2026-08-21 12:00:00", 1, dec!(10), dec!(10)),
        snapshot("A", "2026-08-21 12:01:00", 40, dec!(5), dec!(5)),
    ];
    assert_eq!(check_stack_continuity(path, &mut one_table, &[]).len(), 1);

    // Two tables in one file (another site's export could mix them): never compared.
    let mut two_tables = vec![
        snapshot("A", "2026-08-21 12:00:00", 1, dec!(10), dec!(10)),
        snapshot("B", "2026-08-21 12:01:00", 40, dec!(5), dec!(5)),
    ];
    assert!(check_stack_continuity(path, &mut two_tables, &[]).is_empty());

    // Oldest hand first in the file, with a block that did not parse between them: skipped.
    let mut gap = vec![
        snapshot("A", "2026-08-21 12:00:00", 1, dec!(10), dec!(10)),
        snapshot("A", "2026-08-21 12:01:00", 40, dec!(5), dec!(5)),
    ];
    assert!(check_stack_continuity(path, &mut gap, &[20]).is_empty());
}

#[test]
fn a_storage_failure_is_described_as_such() {
    assert_eq!(
        FailureStage::Storage("duplicate content".to_string()).to_string(),
        "rejected by storage: duplicate content"
    );
}

#[test]
fn ingest_directory_at_records_exact_started_at() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("session.txt"), SAMPLE_SESSION).unwrap();
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let fixed_time = chrono::DateTime::parse_from_rfc3339("2026-11-03T14:30:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let report = DirectoryIngester::default()
        .ingest_directory_at(dir.path(), &registry(), &mut repo, fixed_time)
        .unwrap();
    let stored_started: String = repo
        .connection()
        .query_row(
            "SELECT started_at FROM import_runs WHERE id = ?1",
            [report.batch_run_id.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored_started, "2026-11-03T14:30:00Z");
}
