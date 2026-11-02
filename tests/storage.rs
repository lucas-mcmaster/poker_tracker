//! SQLite repository tests: idempotency, per-hand savepoints, reconciliation queries,
//! multi-currency storage, and schema versioning.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use pretty_assertions::assert_eq;
use rust_decimal_macros::dec;
use tempfile::NamedTempFile;

use common::{
    EV_CASHOUT, EV_CASHOUT_RECEIVED, ONE_CARD_SHOW_AFTER_FOLD, REAL_SESSION,
    RIT_THREE_TIMES_PREFLOP, WALK, gg,
};
use poker_tracker::domain::{CanonicalHand, CurrencyCode, Platform};
use poker_tracker::parser::{GgPokerParser, HandParser};
use poker_tracker::storage::{
    APPLICATION_ID, DeletedImportRun, HandSource, MIGRATIONS, QuarantinedHand, SCHEMA_VERSION,
    SqliteRepository, StorageError, hand_text_sha256,
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
