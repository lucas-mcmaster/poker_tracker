//! SQLite repository tests: idempotency, per-hand savepoints, reconciliation queries,
//! multi-currency storage, and schema versioning.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use pretty_assertions::assert_eq;
use rust_decimal_macros::dec;
use tempfile::NamedTempFile;

use common::{
    EV_CASHOUT, EV_CASHOUT_RECEIVED, ONE_CARD_SHOW_AFTER_FOLD, REAL_SESSION,
    RIT_THREE_TIMES_PREFLOP, SAMPLE_SESSION, WALK, gg,
};
use poker_tracker::domain::{CanonicalHand, CurrencyCode, Platform};
use poker_tracker::parser::{GgPokerParser, HandParser, ParserRegistry};
use poker_tracker::pipeline::{DirectoryIngester, PipelineError};
use poker_tracker::stats::{COLUMNS, STATS_VERSION};
use poker_tracker::storage::{
    APPLICATION_ID, DeletedImportRun, DerivedRebuild, HandSource, MIGRATIONS, QuarantinedHand,
    SCHEMA_VERSION, SqliteRepository, StorageError, hand_text_sha256,
};
use uuid::Uuid;

fn parse(raw: &str) -> CanonicalHand {
    gg().parse_hand(raw).unwrap()
}

fn real_hands() -> Vec<CanonicalHand> {
    gg().parse_session(REAL_SESSION)
        .map(|(_, r)| r.unwrap())
        .collect()
}

fn scalar(repo: &SqliteRepository, sql: &str) -> i64 {
    repo.connection()
        .query_row(sql, [], |row| row.get(0))
        .unwrap()
}

fn rows<T: rusqlite::types::FromSql, U: rusqlite::types::FromSql>(
    repo: &SqliteRepository,
    sql: &str,
) -> Vec<(T, U)> {
    let mut stmt = repo.connection().prepare(sql).unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn real_session_inserts_once_and_reconciles_in_sql() {
    let hands = real_hands();
    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();

    let first = repo.insert_hands_batch(&hands).unwrap();
    assert_eq!((first.hands_inserted, first.duplicates_skipped), (294, 0));
    assert!(first.rejected.is_empty());
    let second = repo.insert_hands_batch(&hands).unwrap();
    assert_eq!((second.hands_inserted, second.duplicates_skipped), (0, 294));

    // Hero's net for the session, per currency: +2.86 CAD.
    let hero: Vec<(String, i64)> = rows(
        &repo,
        "SELECT h.currency, SUM(hp.net_micros) FROM hand_players hp
         JOIN hands h ON h.id = hp.hand_id WHERE hp.is_hero = 1 GROUP BY h.currency",
    );
    assert_eq!(hero, vec![("CAD".to_string(), 2_860_000)]);

    // Money conservation: all nets == adjustments - deductions.
    let nets = scalar(&repo, "SELECT SUM(net_micros) FROM hand_players");
    let deductions = scalar(&repo, "SELECT SUM(amount_micros) FROM pot_deductions");
    let adjustments = scalar(&repo, "SELECT SUM(amount_micros) FROM player_adjustments");
    assert_eq!(nets, adjustments - deductions);
    // Aggregates on `hands` agree with the detail table.
    assert_eq!(
        scalar(
            &repo,
            "SELECT SUM(rake_micros + jackpot_micros + other_deductions_micros) FROM hands"
        ),
        deductions
    );
    // Chip conservation per hand, checked independently in SQL.
    assert_eq!(
        scalar(
            &repo,
            "SELECT COUNT(*) FROM hands h WHERE h.total_pot_micros != (
                SELECT COALESCE(SUM(CASE
                    WHEN a.action_type = 'UncalledBetReturned' THEN -a.amount_micros
                    WHEN a.action_type IN ('Fold', 'Check') THEN 0
                    ELSE a.amount_micros END), 0)
                FROM actions a WHERE a.hand_id = h.id)"
        ),
        0
    );
}

#[test]
fn hands_in_different_currencies_are_stored_and_reported_separately() {
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let cad = GgPokerParser::new(CurrencyCode::CAD)
        .parse_hand(EV_CASHOUT)
        .unwrap();
    let usd = GgPokerParser::new(CurrencyCode::USD)
        .parse_hand(RIT_THREE_TIMES_PREFLOP)
        .unwrap();
    repo.insert_hands_batch(&[cad, usd]).unwrap();

    let rake_by_currency: Vec<(String, i64)> = rows(
        &repo,
        "SELECT currency, SUM(rake_micros) FROM hands GROUP BY currency ORDER BY currency",
    );
    assert_eq!(
        rake_by_currency,
        vec![("CAD".to_string(), 140_000), ("USD".to_string(), 960_000)]
    );
}

#[test]
fn multi_run_winners_partial_cards_deductions_and_adjustments_are_persisted() {
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let hands = vec![
        parse(RIT_THREE_TIMES_PREFLOP),
        parse(ONE_CARD_SHOW_AFTER_FOLD),
        parse(EV_CASHOUT),
    ];
    repo.insert_hands_batch(&hands).unwrap();

    let runs: Vec<(i64, i64)> = rows(
        &repo,
        "SELECT w.run_index, w.amount_micros FROM pot_winners w
         JOIN hands h ON h.id = w.hand_id WHERE h.site_hand_id = 'HD140645134'
         ORDER BY w.pot_sequence",
    );
    assert_eq!(runs, vec![(1, 6_230_000), (2, 6_230_000), (3, 6_230_000)]);
    assert_eq!(
        scalar(
            &repo,
            "SELECT run_count FROM hands WHERE site_hand_id = 'HD140645134'"
        ),
        3
    );

    let labels: Vec<(String, String)> = rows(
        &repo,
        "SELECT d.kind, d.site_label FROM pot_deductions d JOIN hands h ON h.id = d.hand_id
         WHERE h.site_hand_id = 'HD140645134' ORDER BY d.deduction_sequence",
    );
    assert_eq!(
        labels,
        [
            ("Rake", "Rake"),
            ("Jackpot", "Jackpot"),
            ("Other", "Bingo"),
            ("Other", "Fortune"),
            ("Other", "Tax")
        ]
        .map(|(k, l)| (k.to_string(), l.to_string()))
        .to_vec()
    );

    assert_eq!(
        scalar(
            &repo,
            "SELECT COUNT(*) FROM hand_players WHERE hole_card_1 IS NOT NULL AND hole_card_2 IS NULL"
        ),
        1
    );
    let adjustment: Vec<(String, i64)> =
        rows(&repo, "SELECT kind, amount_micros FROM player_adjustments");
    assert_eq!(
        adjustment,
        vec![("EvCashoutRiskPaid".to_string(), -480_000)]
    );
    assert_eq!(
        scalar(
            &repo,
            "SELECT hp.net_micros FROM hand_players hp JOIN players p ON p.id = hp.player_id
             JOIN hands h ON h.id = hp.hand_id
             WHERE h.site_hand_id = 'HD140642980' AND p.screen_name = '1020a9fa'"
        ),
        830_000
    );
}

#[test]
fn one_bad_hand_is_rejected_without_losing_the_rest_of_the_batch() {
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let good_a = parse(WALK);
    let good_b = parse(EV_CASHOUT);
    let mut bad = parse(RIT_THREE_TIMES_PREFLOP);
    bad.outcome.total_pot = dec!(999.99);

    let stats = repo.insert_hands_batch(&[good_a, bad, good_b]).unwrap();
    assert_eq!(stats.hands_inserted, 2);
    assert_eq!(stats.rejected.len(), 1);
    assert_eq!(stats.rejected[0].batch_index, 1);
    assert_eq!(stats.rejected[0].site_hand_id, "HD140645134");
    assert!(matches!(
        stats.rejected[0].error,
        StorageError::InvalidHand(_)
    ));
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM hands"), 2);
    assert_eq!(
        scalar(
            &repo,
            "SELECT COUNT(*) FROM actions WHERE hand_id NOT IN (SELECT id FROM hands)"
        ),
        0
    );
}

#[test]
fn migrations_bring_new_databases_up_to_date_and_refuse_newer_ones() {
    let temp_db = NamedTempFile::new().unwrap();
    {
        let repo = SqliteRepository::open(temp_db.path()).unwrap();
        assert_eq!(repo.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(usize::try_from(SCHEMA_VERSION).unwrap(), MIGRATIONS.len());
    }
    // Re-opening an up-to-date database applies nothing and keeps the data.
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    repo.insert_hands_batch(&[parse(WALK)]).unwrap();
    drop(repo);
    let repo = SqliteRepository::open(temp_db.path()).unwrap();
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM hands"), 1);
    drop(repo);

    // A database written by a newer build is refused rather than misread.
    {
        let conn = rusqlite::Connection::open(temp_db.path()).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
    }
    assert!(matches!(
        SqliteRepository::open(temp_db.path()),
        Err(StorageError::UnsupportedSchemaVersion { found, expected })
            if found == SCHEMA_VERSION + 1 && expected == SCHEMA_VERSION
    ));
}

#[test]
fn reimporting_in_a_different_currency_is_rejected_not_skipped() {
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let as_usd = GgPokerParser::new(CurrencyCode::USD)
        .parse_hand(WALK)
        .unwrap();
    repo.insert_hands_batch(&[as_usd]).unwrap();

    let as_cad = parse(WALK);
    let stats = repo.insert_hands_batch(&[as_cad]).unwrap();
    assert_eq!((stats.hands_inserted, stats.duplicates_skipped), (0, 0));
    assert_eq!(stats.rejected.len(), 1);
    assert!(matches!(
        &stats.rejected[0].error,
        StorageError::CurrencyConflict { stored, incoming, .. } if stored == "USD" && incoming == "CAD"
    ));

    // The same hand in the same currency is still an ordinary duplicate.
    let again = GgPokerParser::new(CurrencyCode::USD)
        .parse_hand(WALK)
        .unwrap();
    let stats = repo.insert_hands_batch(&[again]).unwrap();
    assert_eq!(stats.duplicates_skipped, 1);
    assert!(stats.rejected.is_empty());
}

#[test]
fn sources_are_recorded_and_an_import_run_can_be_deleted() {
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let run = Uuid::now_v7();
    let other_run = Uuid::now_v7();
    let hands = vec![parse(WALK), parse(EV_CASHOUT)];
    let sources = vec![
        HandSource {
            import_run_id: run,
            file_path: "a.txt".into(),
            start_line: 1,
            raw_text: WALK.into(),
        },
        HandSource {
            import_run_id: other_run,
            file_path: "b.txt".into(),
            start_line: 42,
            raw_text: EV_CASHOUT.into(),
        },
    ];
    let stats = repo.insert_sourced_hands_batch(&hands, &sources).unwrap();
    assert_eq!(stats.hands_inserted, 2);

    let recorded: Vec<(String, i64)> = rows(
        &repo,
        "SELECT source_file, source_line FROM hands ORDER BY source_line",
    );
    assert_eq!(
        recorded,
        vec![("a.txt".to_string(), 1), ("b.txt".to_string(), 42)]
    );
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM import_runs"), 2);

    // Undo one run: its hands and all their child rows go; the other run is untouched.
    assert_eq!(repo.delete_import_run(other_run).unwrap().hands_deleted, 1);
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM hands"), 1);
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM player_adjustments"), 0);
    assert_eq!(
        scalar(
            &repo,
            "SELECT COUNT(*) FROM actions WHERE hand_id NOT IN (SELECT id FROM hands)"
        ),
        0
    );

    assert!(matches!(
        repo.insert_sourced_hands_batch(&hands, &sources[..1]),
        Err(StorageError::SourceCountMismatch { .. })
    ));
}

// ---------------------------------------------------------------------------------------------
// v3.5 review fixes
// ---------------------------------------------------------------------------------------------

fn source(run: Uuid, file: &str, line: usize, raw: &str) -> HandSource {
    HandSource {
        import_run_id: run,
        file_path: file.into(),
        start_line: line,
        raw_text: raw.into(),
    }
}

#[test]
fn every_real_hand_round_trips_through_sqlite_in_true_micro_units() {
    let hands = real_hands();
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    assert_eq!(repo.insert_hands_batch(&hands).unwrap().hands_inserted, 294);
    for h in &hands {
        let stored = repo
            .load_hand(Platform::GGPoker, &h.header.site_hand_id)
            .unwrap()
            .unwrap();
        assert_eq!(&stored, h);
    }
    assert!(repo.load_hand(Platform::GGPoker, "HD0").unwrap().is_none());
    // One micro-unit is one millionth: the big blind of 0.10 is stored as 100,000.
    assert_eq!(
        scalar(&repo, "SELECT MAX(big_blind_micros) FROM hands"),
        100_000
    );
}

#[test]
fn different_content_under_the_same_hand_id_is_a_conflict_not_a_duplicate() {
    let changed = WALK.replace("Dealt to Hero [Ts 6d]", "Dealt to Hero [Ts 7d]");
    let run = Uuid::now_v7();

    // With raw text (every pipeline import): compared by the normalised text's SHA-256.
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    repo.insert_sourced_hands_batch(&[parse(WALK)], &[source(run, "a.txt", 1, WALK)])
        .unwrap();
    let stats = repo
        .insert_sourced_hands_batch(&[parse(&changed)], &[source(run, "b.txt", 1, &changed)])
        .unwrap();
    assert_eq!((stats.hands_inserted, stats.duplicates_skipped), (0, 0));
    assert!(matches!(
        &stats.rejected[0].error,
        StorageError::ContentConflict { site_hand_id } if site_hand_id == "HD140646565"
    ));
    // CRLF line endings and trailing spaces do not make a different hand.
    let reformatted = WALK.replace('\n', "  \r\n");
    assert_eq!(hand_text_sha256(&reformatted), hand_text_sha256(WALK));
    let stats = repo
        .insert_sourced_hands_batch(
            &[parse(&reformatted)],
            &[source(run, "c.txt", 1, &reformatted)],
        )
        .unwrap();
    assert_eq!(stats.duplicates_skipped, 1);
    assert_eq!(
        scalar(
            &repo,
            "SELECT COUNT(*) FROM hands WHERE raw_text IS NOT NULL"
        ),
        1
    );

    // Without raw text: compared by the canonical hand read back from the database.
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    repo.insert_hands_batch(&[parse(WALK)]).unwrap();
    let mut restacked = parse(WALK);
    restacked.players[0].starting_stack += rust_decimal::Decimal::ONE;
    for different in [parse(&changed), restacked] {
        let stats = repo.insert_hands_batch(&[different]).unwrap();
        assert!(matches!(
            stats.rejected[0].error,
            StorageError::ContentConflict { .. }
        ));
    }
    assert_eq!(
        repo.insert_hands_batch(&[parse(WALK)])
            .unwrap()
            .duplicates_skipped,
        1
    );
}

#[test]
fn deleting_a_run_keeps_hands_that_another_run_also_contained() {
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let (first, second) = (Uuid::now_v7(), Uuid::now_v7());
    repo.insert_sourced_hands_batch(
        &[parse(WALK), parse(EV_CASHOUT)],
        &[
            source(first, "a/s.txt", 1, WALK),
            source(first, "a/s.txt", 30, EV_CASHOUT),
        ],
    )
    .unwrap();
    // The second run re-imports WALK only: a duplicate, but it is recorded as a sighting.
    let stats = repo
        .insert_sourced_hands_batch(&[parse(WALK)], &[source(second, "b/s.txt", 7, WALK)])
        .unwrap();
    assert_eq!(stats.duplicates_skipped, 1);
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM hand_sightings"), 3);

    assert_eq!(
        repo.delete_import_run(first).unwrap(),
        DeletedImportRun {
            hands_deleted: 1,
            hands_kept: 1,
            quarantined_deleted: 0
        }
    );
    let kept: Vec<(String, i64)> = rows(
        &repo,
        "SELECT source_file, source_line FROM hands WHERE site_hand_id = 'HD140646565'",
    );
    assert_eq!(kept, vec![("b/s.txt".to_string(), 7)]);
    assert_eq!(
        scalar(
            &repo,
            &format!("SELECT COUNT(*) FROM hands WHERE import_run_id = '{second}'")
        ),
        1
    );

    // A mistyped run id is an error, not a silent "0 deleted".
    assert!(matches!(
        repo.delete_import_run(first),
        Err(StorageError::UnknownImportRun { run_id }) if run_id == first
    ));
    assert_eq!(repo.delete_import_run(second).unwrap().hands_deleted, 1);
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM hands"), 0);

    // A hand stored without a source existed before any run that later contained it.
    repo.insert_hands_batch(&[parse(WALK)]).unwrap();
    let third = Uuid::now_v7();
    repo.insert_sourced_hands_batch(&[parse(WALK)], &[source(third, "c/s.txt", 1, WALK)])
        .unwrap();
    let undone = repo.delete_import_run(third).unwrap();
    assert_eq!((undone.hands_deleted, undone.hands_kept), (0, 1));
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM hands"), 1);
}

#[test]
fn quarantined_hands_keep_their_raw_text_until_an_import_stores_them() {
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let run = Uuid::now_v7();
    let entry = QuarantinedHand {
        import_run_id: run,
        parser: "GGPoker (CAD)".into(),
        file_path: "s.txt".into(),
        start_line: 1,
        site_hand_id: Some("HD140646565".into()),
        stage: "parse".into(),
        error: "unrecognized line".into(),
        raw_text: WALK.into(),
    };
    assert_eq!(
        repo.quarantine_hands(&[entry.clone(), entry.clone()])
            .unwrap(),
        1
    );
    let stored: Vec<(String, String)> =
        rows(&repo, "SELECT stage, raw_text FROM quarantined_hands");
    assert_eq!(stored, vec![("parse".to_string(), WALK.to_string())]);

    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM quarantine_pending"), 1);

    // A later import that stores the same text takes it off the pending list; undoing that
    // import puts it back, because the quarantine entry itself is kept.
    let later = Uuid::now_v7();
    repo.insert_sourced_hands_batch(&[parse(WALK)], &[source(later, "s.txt", 1, WALK)])
        .unwrap();
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM quarantine_pending"), 0);
    repo.delete_import_run(later).unwrap();
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM quarantine_pending"), 1);

    // Undoing a run removes its quarantine entries.
    repo.quarantine_hands(&[QuarantinedHand {
        raw_text: EV_CASHOUT.into(),
        ..entry
    }])
    .unwrap();
    assert_eq!(repo.delete_import_run(run).unwrap().quarantined_deleted, 2);
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM quarantined_hands"), 0);
}

/// Builds a v3.4 database by hand: migration 1 only, one hand, amounts in units of 0.0001.
fn write_v3_4_database(path: &std::path::Path) {
    let run = Uuid::now_v7().to_string();
    {
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(MIGRATIONS[0]).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute(
            "INSERT INTO import_runs (id, started_at, tool_version) VALUES (?1, '2026-08-21T20:00:00Z', '0.4.0')",
            [&run],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO hands (platform, site_hand_id, game_type, betting_limit, currency,
                small_blind_micros, big_blind_micros, timestamp, table_name, max_seats,
                button_seat, run_count, total_pot_micros, rake_micros, jackpot_micros,
                other_deductions_micros, uncalled_returned_micros, import_run_id, source_file,
                source_line)
             VALUES ('GGPoker', 'HD1', 'Cash', 'NL', 'CAD', 500, 1000, '2026-08-21T17:00:00',
                'T', 6, 1, 0, 1000, 0, 0, 0, 500, ?1, 's.txt', 1)",
            [&run],
        )
        .unwrap();
    }
}

#[test]
fn a_v3_4_database_is_upgraded_in_place() {
    let temp_db = NamedTempFile::new().unwrap();
    write_v3_4_database(temp_db.path());
    let repo = SqliteRepository::open(temp_db.path()).unwrap();
    assert_eq!(repo.schema_version().unwrap(), SCHEMA_VERSION);
    assert_eq!(scalar(&repo, "PRAGMA application_id"), APPLICATION_ID);
    let blinds: Vec<(i64, i64)> = rows(
        &repo,
        "SELECT small_blind_micros, big_blind_micros FROM hands",
    );
    assert_eq!(blinds, vec![(50_000, 100_000)]);
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM hand_sightings"), 1);
}

#[test]
fn concurrent_opens_apply_each_migration_once() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("v34.sqlite");
    write_v3_4_database(&path);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let opens: Vec<_> = (0..4)
        .map(|_| {
            let (path, barrier) = (path.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                SqliteRepository::open(&path).map(|_| ())
            })
        })
        .collect();
    for open in opens {
        open.join().unwrap().unwrap();
    }
    let repo = SqliteRepository::open(&path).unwrap();
    let blinds: Vec<(i64, i64)> = rows(
        &repo,
        "SELECT small_blind_micros, big_blind_micros FROM hands",
    );
    assert_eq!(
        blinds,
        vec![(50_000, 100_000)],
        "the rescale ran exactly once"
    );
}

#[test]
fn files_that_are_not_poker_tracker_databases_are_left_untouched() {
    // An unrelated SQLite file.
    let other = NamedTempFile::new().unwrap();
    {
        let conn = rusqlite::Connection::open(other.path()).unwrap();
        conn.execute_batch("CREATE TABLE notes (body TEXT);")
            .unwrap();
    }
    // A poker_tracker v3.3 database (schema version 3, no application id).
    let old = NamedTempFile::new().unwrap();
    {
        let conn = rusqlite::Connection::open(old.path()).unwrap();
        conn.execute_batch("CREATE TABLE hands (id INTEGER PRIMARY KEY);")
            .unwrap();
        conn.pragma_update(None, "user_version", 3).unwrap();
    }
    for (file, hint) in [(&other, "left untouched"), (&old, "v3.3 or earlier")] {
        let err = SqliteRepository::open(file.path()).err().unwrap();
        assert!(
            matches!(&err, StorageError::NotAPokerTrackerDatabase { detail } if detail.contains(hint)),
            "{err}"
        );
        let conn = rusqlite::Connection::open(file.path()).unwrap();
        let journal: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            journal, "delete",
            "the journal mode must not be switched to WAL"
        );
        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 1, "no poker_tracker tables may be added");
    }
    // Opening an existing database never creates one.
    let missing = other.path().with_extension("missing.sqlite");
    assert!(matches!(
        SqliteRepository::open_existing(&missing),
        Err(StorageError::DatabaseNotFound { .. })
    ));
    assert!(!missing.exists());
}

#[test]
fn a_locked_database_fails_the_batch_instead_of_rejecting_its_hands() {
    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    repo.set_busy_timeout(std::time::Duration::from_millis(50))
        .unwrap();
    let other = rusqlite::Connection::open(temp_db.path()).unwrap();
    other.execute_batch("BEGIN IMMEDIATE;").unwrap();

    let err = repo.insert_hands_batch(&[parse(WALK), parse(EV_CASHOUT)]);
    assert!(
        matches!(&err, Err(StorageError::Sqlite(e)) if e.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseBusy)),
        "{err:?}"
    );
    other.execute_batch("ROLLBACK;").unwrap();
    let stats = repo
        .insert_hands_batch(&[parse(WALK), parse(EV_CASHOUT)])
        .unwrap();
    assert_eq!(stats.hands_inserted, 2);
}

#[test]
fn ev_cashout_received_round_trips_through_sqlite() {
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let hand = parse(EV_CASHOUT_RECEIVED);
    assert_eq!(
        repo.insert_hands_batch(std::slice::from_ref(&hand))
            .unwrap()
            .hands_inserted,
        1
    );
    let adj: Vec<(String, i64)> = rows(&repo, "SELECT kind, amount_micros FROM player_adjustments");
    assert_eq!(adj, vec![("EvCashoutReceived".to_string(), 6_870_000)]);
    let loaded = repo
        .load_hand(Platform::GGPoker, &hand.header.site_hand_id)
        .unwrap()
        .unwrap();
    assert_eq!(loaded, hand);
}

#[test]
fn a_foreign_database_is_refused_without_waiting_for_the_busy_timeout() {
    let other = NamedTempFile::new().unwrap();
    rusqlite::Connection::open(other.path())
        .unwrap()
        .execute_batch("CREATE TABLE notes (body TEXT);")
        .unwrap();
    let start = std::time::Instant::now();
    assert!(SqliteRepository::open(other.path()).is_err());
    assert!(
        start.elapsed() < std::time::Duration::from_secs(1),
        "refusal took {:?}",
        start.elapsed()
    );
}

#[test]
fn rows_written_on_insert_equal_rows_after_rebuild_for_all_corpus_hands() {
    let mut all_hands = real_hands();
    let v36_dir = std::path::Path::new("tests/fixtures/ggpoker/v3_6");
    let mut paths: Vec<_> = std::fs::read_dir(v36_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "txt"))
        .collect();
    paths.sort();
    for p in paths {
        let content = std::fs::read_to_string(p).unwrap();
        for (_, res) in gg().parse_session(&content) {
            all_hands.push(res.unwrap());
        }
    }
    assert_eq!(all_hands.len(), 1283);

    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    let stats = repo.insert_hands_batch(&all_hands).unwrap();
    assert_eq!(stats.hands_inserted, 1283);

    let query_rows = |repo: &SqliteRepository| -> Vec<Vec<i64>> {
        let sql = format!(
            "SELECT hand_id, player_id, COALESCE(position, -1), {} FROM hand_player_stats ORDER BY hand_id, player_id",
            COLUMNS.join(", ")
        );
        let mut stmt = repo.connection().prepare(&sql).unwrap();
        let num_cols = 3 + COLUMNS.len();
        stmt.query_map([], |row| {
            let mut v = Vec::with_capacity(num_cols);
            for i in 0..num_cols {
                v.push(row.get::<_, i64>(i)?);
            }
            Ok(v)
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
    };

    let before = query_rows(&repo);
    assert_eq!(before.len(), 6840);

    let rebuild = repo.rebuild_derived().unwrap();
    assert_eq!(rebuild.hands, 1283);
    assert_eq!(rebuild.version, STATS_VERSION);

    let after = query_rows(&repo);
    assert_eq!(before, after);
}

#[test]
fn deleting_derived_cache_row_and_reopening_rebuilds_and_reports() {
    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    repo.insert_hands_batch(&[parse(WALK), parse(EV_CASHOUT)])
        .unwrap();

    repo.connection()
        .execute(
            "DELETE FROM derived_cache WHERE name = 'hand_player_stats'",
            [],
        )
        .unwrap();
    drop(repo);

    let repo2 = SqliteRepository::open(temp_db.path()).unwrap();
    let rebuild = repo2.rebuilt_on_open();
    assert_eq!(
        rebuild,
        Some(DerivedRebuild {
            hands: 2,
            version: STATS_VERSION,
        })
    );
}

#[test]
fn delete_import_run_removes_derived_cache_rows() {
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let run = Uuid::now_v7();
    repo.insert_sourced_hands_batch(
        &[parse(WALK), parse(EV_CASHOUT)],
        &[
            source(run, "f.txt", 1, WALK),
            source(run, "f.txt", 50, EV_CASHOUT),
        ],
    )
    .unwrap();
    assert!(scalar(&repo, "SELECT COUNT(*) FROM hand_player_stats") > 0);

    repo.delete_import_run(run).unwrap();
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM hand_player_stats"), 0);
}

#[test]
fn v3_4_database_cache_is_built_on_upgrade() {
    let temp_db = NamedTempFile::new().unwrap();
    write_v3_4_database(temp_db.path());
    let repo = SqliteRepository::open(temp_db.path()).unwrap();
    assert_eq!(
        repo.rebuilt_on_open(),
        Some(DerivedRebuild {
            hands: 1,
            version: STATS_VERSION,
        })
    );
    assert_eq!(
        scalar(
            &repo,
            "SELECT version FROM derived_cache WHERE name = 'hand_player_stats'"
        ),
        STATS_VERSION
    );
}

#[test]
fn four_concurrent_opens_of_stale_database_rebuild_exactly_once() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("stale.sqlite");
    {
        let mut repo = SqliteRepository::open(&path).unwrap();
        repo.insert_hands_batch(&[parse(WALK), parse(EV_CASHOUT)])
            .unwrap();
        repo.connection()
            .execute(
                "DELETE FROM derived_cache WHERE name = 'hand_player_stats'",
                [],
            )
            .unwrap();
    }

    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let opens: Vec<_> = (0..4)
        .map(|_| {
            let (path, barrier) = (path.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                SqliteRepository::open(&path).map(|repo| repo.rebuilt_on_open())
            })
        })
        .collect();

    let mut rebuild_count = 0;
    for open in opens {
        let rebuilt = open.join().unwrap().unwrap();
        if rebuilt.is_some() {
            rebuild_count += 1;
        }
    }
    assert_eq!(
        rebuild_count, 1,
        "exactly one concurrent open performed the rebuild"
    );

    let repo = SqliteRepository::open(&path).unwrap();
    assert_eq!(
        scalar(
            &repo,
            "SELECT version FROM derived_cache WHERE name = 'hand_player_stats'"
        ),
        STATS_VERSION
    );
    assert!(scalar(&repo, "SELECT COUNT(*) FROM hand_player_stats") > 0);
}

#[test]
fn version_guard_rejects_stale_cache_and_pipeline_quarantines_nothing() {
    let temp_db = NamedTempFile::new().unwrap();
    let mut repo_a = SqliteRepository::open(temp_db.path()).unwrap();
    let run = Uuid::now_v7();
    let hands = vec![parse(WALK)];
    let sources = vec![source(run, "file.txt", 1, WALK)];

    // 1. Through a second connection set version to STATS_VERSION + 1
    {
        let conn_b = rusqlite::Connection::open(temp_db.path()).unwrap();
        conn_b
            .execute(
                "UPDATE derived_cache SET version = ?1 WHERE name = 'hand_player_stats'",
                [STATS_VERSION + 1],
            )
            .unwrap();
    }

    let err1 = repo_a
        .insert_sourced_hands_batch(&hands, &sources)
        .err()
        .unwrap();
    assert!(
        matches!(
            &err1,
            StorageError::StaleDerivedCache {
                found: Some(v),
                expected
            } if *v == STATS_VERSION + 1 && *expected == STATS_VERSION
        ),
        "expected StaleDerivedCache with found Some, got {err1:?}"
    );
    assert_eq!(scalar(&repo_a, "SELECT COUNT(*) FROM hands"), 0);
    assert_eq!(scalar(&repo_a, "SELECT COUNT(*) FROM players"), 0);
    assert_eq!(scalar(&repo_a, "SELECT COUNT(*) FROM hand_sightings"), 0);
    assert_eq!(scalar(&repo_a, "SELECT COUNT(*) FROM import_runs"), 0);
    assert_eq!(scalar(&repo_a, "SELECT COUNT(*) FROM quarantined_hands"), 0);

    // 2. With derived_cache row deleted instead, found is None
    {
        let conn_b = rusqlite::Connection::open(temp_db.path()).unwrap();
        conn_b
            .execute(
                "DELETE FROM derived_cache WHERE name = 'hand_player_stats'",
                [],
            )
            .unwrap();
    }
    let err2 = repo_a
        .insert_sourced_hands_batch(&hands, &sources)
        .err()
        .unwrap();
    assert!(
        matches!(
            &err2,
            StorageError::StaleDerivedCache {
                found: None,
                expected
            } if *expected == STATS_VERSION
        ),
        "expected StaleDerivedCache with found None, got {err2:?}"
    );

    // 3. Reopening rebuilds, and the same insert then succeeds
    drop(repo_a);
    let mut repo_reopened = SqliteRepository::open(temp_db.path()).unwrap();
    let stats = repo_reopened
        .insert_sourced_hands_batch(&hands, &sources)
        .unwrap();
    assert_eq!(stats.hands_inserted, 1);
    assert_eq!(scalar(&repo_reopened, "SELECT COUNT(*) FROM hands"), 1);

    // 4. Through the pipeline, a stale insert is a PipelineError::Storage and quarantines nothing
    {
        let conn_b = rusqlite::Connection::open(temp_db.path()).unwrap();
        conn_b
            .execute(
                "UPDATE derived_cache SET version = ?1 WHERE name = 'hand_player_stats'",
                [99],
            )
            .unwrap();
    }
    let exports_dir = tempfile::TempDir::new().unwrap();
    std::fs::write(exports_dir.path().join("s.txt"), SAMPLE_SESSION).unwrap();
    let mut reg = ParserRegistry::new();
    reg.register(GgPokerParser::new(CurrencyCode::CAD));
    let pipe_res =
        DirectoryIngester::default().ingest_directory(exports_dir.path(), &reg, &mut repo_reopened);
    assert!(
        matches!(pipe_res, Err(PipelineError::Storage { .. })),
        "pipeline must fail with Storage error, got {pipe_res:?}"
    );
    assert_eq!(
        scalar(&repo_reopened, "SELECT COUNT(*) FROM quarantined_hands"),
        0
    );
}

#[test]
fn rebuild_failure_on_unloadable_hand_fails_closed_and_leaves_database_untouched() {
    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    repo.insert_hands_batch(&[parse(WALK)]).unwrap();

    let plain_conn = rusqlite::Connection::open(temp_db.path()).unwrap();
    let count_before: i64 = plain_conn
        .query_row("SELECT COUNT(*) FROM hand_player_stats", [], |r| r.get(0))
        .unwrap();
    assert!(count_before > 0);

    // Corrupt one action and delete the derived_cache row
    repo.connection()
        .execute(
            "UPDATE actions SET action_type = 'UnrecognizedActionType' WHERE id = 1",
            [],
        )
        .unwrap();
    repo.connection()
        .execute(
            "DELETE FROM derived_cache WHERE name = 'hand_player_stats'",
            [],
        )
        .unwrap();
    drop(repo);

    let err = SqliteRepository::open(temp_db.path()).err().unwrap();
    match &err {
        StorageError::DerivedRebuildFailed {
            platform,
            site_hand_id,
            source,
        } => {
            assert_eq!(platform, "GGPoker");
            assert_eq!(site_hand_id, "HD140646565");
            let msg = err.to_string();
            assert!(
                msg.contains("HD140646565"),
                "message must contain hand id: {msg}"
            );
            assert!(matches!(**source, StorageError::CorruptRow { .. }));
        }
        _ => panic!("expected DerivedRebuildFailed, got {err:?}"),
    }

    // Inspect with plain rusqlite connection: cache table and derived_cache are as before the open
    let cache_rows: i64 = plain_conn
        .query_row(
            "SELECT COUNT(*) FROM derived_cache WHERE name = 'hand_player_stats'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        cache_rows, 0,
        "derived_cache row was deleted and not recreated"
    );
    let count_after: i64 = plain_conn
        .query_row("SELECT COUNT(*) FROM hand_player_stats", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        count_after, count_before,
        "hand_player_stats was preserved via rollback"
    );
}

// ---------------------------------------------------------------------------------------------
// v3.7 review: batch-level errors, mixed raw text, and the stale-cache import
// ---------------------------------------------------------------------------------------------

fn counts(repo: &SqliteRepository) -> (i64, i64, i64) {
    (
        scalar(repo, "SELECT COUNT(*) FROM hands"),
        scalar(repo, "SELECT COUNT(*) FROM hand_sightings"),
        scalar(repo, "SELECT COUNT(*) FROM import_runs"),
    )
}

#[test]
fn a_database_wide_error_after_a_duplicate_stores_nothing_from_the_batch() {
    // ARCHITECTURE §6: lock, I/O and other database-wide errors return Err and nothing from the
    // batch is stored. The batch is [a duplicate, a new hand]; the new hand's cache rows fail
    // with "no such table" (not a constraint error), after the duplicate's sighting and the
    // batch's run were written in the same transaction.
    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    let first = Uuid::now_v7();
    repo.insert_sourced_hands_batch(&[parse(WALK)], &[source(first, "a.txt", 1, WALK)])
        .unwrap();
    let before = counts(&repo);
    assert_eq!(before, (1, 1, 1));

    rusqlite::Connection::open(temp_db.path())
        .unwrap()
        .execute_batch("DROP TABLE hand_player_stats;")
        .unwrap();

    let second = Uuid::now_v7();
    let result = repo.insert_sourced_hands_batch(
        &[parse(WALK), parse(EV_CASHOUT)],
        &[
            source(second, "b.txt", 1, WALK),
            source(second, "b.txt", 30, EV_CASHOUT),
        ],
    );
    let err = result.expect_err("a database-wide error must fail the batch");
    assert!(!err.is_hand_specific(), "{err:?}");
    assert_eq!(
        counts(&repo),
        before,
        "no hand, sighting or run from the failed batch"
    );
}

#[test]
fn duplicates_with_raw_text_on_one_side_only_are_compared_by_content() {
    // One side has raw text and the other does not: the stored hand is loaded and compared
    // (ARCHITECTURE §6), in both directions.
    let changed = WALK.replace("Dealt to Hero [Ts 6d]", "Dealt to Hero [Ts 7d]");
    let run = Uuid::now_v7();

    // Stored without raw text, re-imported with it
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    repo.insert_hands_batch(&[parse(WALK)]).unwrap();
    let stats = repo
        .insert_sourced_hands_batch(&[parse(&changed)], &[source(run, "b.txt", 1, &changed)])
        .unwrap();
    assert!(
        matches!(&stats.rejected[..], [r] if matches!(r.error, StorageError::ContentConflict { .. })),
        "{stats:?}"
    );

    // Stored with raw text, re-imported without it
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    repo.insert_sourced_hands_batch(&[parse(WALK)], &[source(run, "a.txt", 1, WALK)])
        .unwrap();
    let stats = repo.insert_hands_batch(&[parse(&changed)]).unwrap();
    assert!(
        matches!(&stats.rejected[..], [r] if matches!(r.error, StorageError::ContentConflict { .. })),
        "{stats:?}"
    );
}

#[test]
fn a_duplicate_with_raw_text_fills_in_a_hand_stored_without_it() {
    // ARCHITECTURE §6: a duplicate records a sighting and fills in missing raw text.
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    repo.insert_hands_batch(&[parse(WALK)]).unwrap();
    assert_eq!(
        scalar(&repo, "SELECT COUNT(*) FROM hands WHERE raw_sha256 IS NULL"),
        1
    );
    let run = Uuid::now_v7();
    let stats = repo
        .insert_sourced_hands_batch(&[parse(WALK)], &[source(run, "a.txt", 7, WALK)])
        .unwrap();
    assert_eq!(stats.duplicates_skipped, 1);
    let filled: Vec<(String, String)> = rows(&repo, "SELECT raw_text, raw_sha256 FROM hands");
    assert_eq!(filled, vec![(WALK.to_string(), hand_text_sha256(WALK))]);
    assert_eq!(
        rows::<String, i64>(&repo, "SELECT source_file, source_line FROM hand_sightings"),
        vec![("a.txt".to_string(), 7)]
    );
}

#[test]
fn a_stale_cache_import_quarantines_nothing_even_with_a_broken_hand() {
    // SPEC_v3.7 §2 step 5: a stale insert is a batch-level error; nothing is quarantined. The
    // file has one hand that fails to parse, which a normal import would quarantine.
    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    rusqlite::Connection::open(temp_db.path())
        .unwrap()
        .execute(
            "UPDATE derived_cache SET version = ?1 WHERE name = 'hand_player_stats'",
            [STATS_VERSION + 1],
        )
        .unwrap();
    let exports = tempfile::TempDir::new().unwrap();
    let broken = SAMPLE_SESSION.replace("Dealt to Hero [5h 2s]", "Dealt to Hero [1h 2s]");
    std::fs::write(exports.path().join("s.txt"), broken).unwrap();
    let mut reg = ParserRegistry::new();
    reg.register(GgPokerParser::new(CurrencyCode::CAD));
    let result = DirectoryIngester::default().ingest_directory(exports.path(), &reg, &mut repo);
    assert!(
        matches!(result, Err(PipelineError::Storage { .. })),
        "{result:?}"
    );
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM quarantined_hands"), 0);
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM hands"), 0);
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM import_runs"), 0);
}

#[test]
fn a_constraint_error_rejects_only_that_hand() {
    // ARCHITECTURE §6: conflict and constraint errors reject that hand; the rest of the batch is
    // stored. A trigger added by another connection makes SQLite refuse one hand with a
    // constraint error (RAISE(ABORT) is SQLITE_CONSTRAINT_TRIGGER).
    let temp_db = NamedTempFile::new().unwrap();
    let mut repo = SqliteRepository::open(temp_db.path()).unwrap();
    rusqlite::Connection::open(temp_db.path())
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER refuse_walk BEFORE INSERT ON hands \
             WHEN NEW.site_hand_id = 'HD140646565' \
             BEGIN SELECT RAISE(ABORT, 'refused by the test'); END;",
        )
        .unwrap();
    let stats = repo
        .insert_hands_batch(&[parse(WALK), parse(EV_CASHOUT)])
        .unwrap();
    assert_eq!(stats.hands_inserted, 1);
    assert!(
        matches!(&stats.rejected[..], [r] if r.site_hand_id == "HD140646565"
            && matches!(r.error, StorageError::Sqlite(_))),
        "{stats:?}"
    );
    assert_eq!(scalar(&repo, "SELECT COUNT(*) FROM hands"), 1);
}

#[test]
fn duplicates_with_raw_text_on_both_sides_are_compared_by_text() {
    // ARCHITECTURE §6: with raw text on both sides, the SHA-256 of the normalised text decides.
    // A different showdown description parses to the same canonical hand but is different text.
    let original = common::HU_SHOWDOWN;
    let reworded = original.replace(
        "445e6868: shows [4c Tc] (a pair of Fours)",
        "445e6868: shows [4c Tc] (one pair, Fours)",
    );
    assert_ne!(reworded, original);
    assert_eq!(parse(&reworded), parse(original), "same canonical hand");
    let run = Uuid::now_v7();
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    repo.insert_sourced_hands_batch(&[parse(original)], &[source(run, "a.txt", 1, original)])
        .unwrap();
    let stats = repo
        .insert_sourced_hands_batch(&[parse(&reworded)], &[source(run, "b.txt", 1, &reworded)])
        .unwrap();
    assert!(
        matches!(&stats.rejected[..], [r] if matches!(r.error, StorageError::ContentConflict { .. })),
        "{stats:?}"
    );
}

#[test]
fn stale_cache_error_message_names_both_versions() {
    // SPEC_v3.7 §2 step 5: the message names the cache's version (or none) and this build's.
    let missing = StorageError::StaleDerivedCache {
        found: None,
        expected: STATS_VERSION,
    }
    .to_string();
    assert!(missing.contains("cache version none"), "{missing}");
    let newer = StorageError::StaleDerivedCache {
        found: Some(STATS_VERSION + 1),
        expected: STATS_VERSION,
    }
    .to_string();
    assert!(
        newer.contains(&format!("cache version {}", STATS_VERSION + 1))
            && newer.contains(&format!("this build expected {STATS_VERSION}"))
            && newer.contains("run the command again"),
        "{newer}"
    );
}

#[test]
fn stored_build_and_run_times_are_utc_timestamps() {
    // derived_cache.built_at and a batch-registered import run's started_at are UTC times in
    // `%Y-%m-%dT%H:%M:%SZ`.
    let mut repo = SqliteRepository::open_in_memory().unwrap();
    let run = Uuid::now_v7();
    repo.insert_sourced_hands_batch(&[parse(WALK)], &[source(run, "a.txt", 1, WALK)])
        .unwrap();
    let times: Vec<(String, String)> = rows(
        &repo,
        "SELECT (SELECT built_at FROM derived_cache), started_at FROM import_runs",
    );
    assert_eq!(times.len(), 1);
    for t in [&times[0].0, &times[0].1] {
        assert!(
            chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%SZ").is_ok(),
            "not a UTC timestamp: {t:?}"
        );
    }
}

#[test]
fn only_a_v3_4_shape_is_taken_for_a_v3_4_database() {
    // ARCHITECTURE §6: id 0 + version 1 + both `hands` and `import_runs` is a v3.4 database;
    // anything else that is not ours is refused and left untouched.
    let cases: [(&str, i64, i64, &str); 5] = [
        (
            "hands only",
            0,
            1,
            "CREATE TABLE hands (id INTEGER PRIMARY KEY);",
        ),
        (
            "import_runs only",
            0,
            1,
            "CREATE TABLE import_runs (id TEXT PRIMARY KEY);",
        ),
        (
            "schema version 2",
            0,
            2,
            "CREATE TABLE hands (id INTEGER PRIMARY KEY); CREATE TABLE import_runs (id TEXT);",
        ),
        (
            "another application id",
            0x1234,
            1,
            "CREATE TABLE hands (id INTEGER PRIMARY KEY); CREATE TABLE import_runs (id TEXT);",
        ),
        ("version 1, no tables", 0, 1, ""),
    ];
    for (label, app_id, version, ddl) in cases {
        let file = NamedTempFile::new().unwrap();
        {
            let conn = rusqlite::Connection::open(file.path()).unwrap();
            conn.execute_batch(ddl).unwrap();
            conn.pragma_update(None, "application_id", app_id).unwrap();
            conn.pragma_update(None, "user_version", version).unwrap();
        }
        let before = std::fs::read(file.path()).unwrap();
        let err = SqliteRepository::open(file.path()).err();
        assert!(
            matches!(err, Some(StorageError::NotAPokerTrackerDatabase { .. })),
            "{label}: {err:?}"
        );
        assert_eq!(
            std::fs::read(file.path()).unwrap(),
            before,
            "{label}: file changed"
        );
    }
}

#[test]
fn hand_player_stats_schema_and_index_sql_hash() {
    let repo = SqliteRepository::open_in_memory().unwrap();
    let table_sql: String = repo
        .connection()
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'hand_player_stats'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(table_sql.len(), 4242);
    use sha2::{Digest, Sha256};
    let table_digest = format!("{:x}", Sha256::digest(table_sql.as_bytes()));
    assert_eq!(
        table_digest,
        "d15e06c9474da75be98319f2091430b7e207f3f26bf792e4d8cf456f80a0d6fb"
    );

    let index_sql: String = repo
        .connection()
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = 'idx_hand_player_stats_player'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let index_digest = format!("{:x}", Sha256::digest(index_sql.as_bytes()));
    assert_eq!(
        index_digest,
        "b909b540d7db74d79c2d46ea9c999058e2e6ddbcc321b4178aafe406a79501e2"
    );
}
