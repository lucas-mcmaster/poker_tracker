//! Transactional SQLite repository for batch-inserting canonical hands from any site.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::{
    Connection, ErrorCode, OpenFlags, OptionalExtension, Row, TransactionBehavior, params,
};
use rust_decimal::Decimal;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::canonical::{
    ActionType, AdjustmentKind, BettingLimit, CanonicalHand, CommunityCards, DeductionKind,
    GameType, HandHeader, HandOutcome, Platform, Player, PlayerAdjustment, PotDeduction, PotWinner,
    Street, StreetAction,
};
use crate::domain::card::Card;
use crate::domain::money::{CurrencyCode, decimal_to_micro_units, micro_units_to_decimal};
use crate::parser::base::{ParseError, validate_hand};
use crate::storage::schema::{APPLICATION_ID, MIGRATIONS, PRAGMA_TUNING, SCHEMA_VERSION};

/// How long a write waits for another connection's lock before failing.
pub const DEFAULT_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Timestamp format stored in `hands.timestamp` (fractional seconds only when present).
pub(crate) const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.f";

/// Errors produced by the SQLite persistence layer.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("invalid canonical hand: {0}")]
    InvalidHand(#[from] ParseError),

    #[error(
        "database schema version {found} is newer than this build supports ({expected}); upgrade poker_tracker"
    )]
    UnsupportedSchemaVersion { found: i64, expected: i64 },

    #[error("{detail}")]
    NotAPokerTrackerDatabase { detail: String },

    #[error("no database exists at '{path}'")]
    DatabaseNotFound { path: String },

    #[error(
        "hand '{site_hand_id}' is already stored in {stored}; re-importing it as {incoming} was refused (delete the earlier import run to correct it)"
    )]
    CurrencyConflict {
        site_hand_id: String,
        stored: String,
        incoming: String,
    },

    #[error(
        "hand '{site_hand_id}' is already stored with different content; the re-import was refused (delete the earlier import run to replace it)"
    )]
    ContentConflict { site_hand_id: String },

    #[error("import run {run_id} does not exist")]
    UnknownImportRun { run_id: Uuid },

    #[error("{hands} hands were passed with {sources} sources")]
    SourceCountMismatch { hands: usize, sources: usize },

    #[error(
        "monetary value {value} in field '{field}' cannot be losslessly converted to micro-units"
    )]
    LossyMonetaryConversion { field: &'static str, value: Decimal },

    #[error("player '{player_name}' referenced in hand '{hand_id}' is not seated")]
    MissingPlayerId {
        hand_id: String,
        player_name: String,
    },

    #[error("stored row in '{table}' cannot be read back: {detail}")]
    CorruptRow { table: &'static str, detail: String },

    #[error(
        "cannot rebuild the statistics cache: {platform} hand {site_hand_id} cannot be loaded: {source}"
    )]
    DerivedRebuildFailed {
        platform: String,
        site_hand_id: String,
        #[source]
        source: Box<StorageError>,
    },

    #[error(
        "the statistics cache was changed by another build (cache version {}, this build expected {expected}); run the command again",
        format_cache_version(*.found)
    )]
    StaleDerivedCache { found: Option<i64>, expected: i64 },
}

fn format_cache_version(v: Option<i64>) -> String {
    match v {
        Some(n) => n.to_string(),
        None => "none".to_string(),
    }
}

impl StorageError {
    /// `true` if the error concerns one hand only, so the rest of its batch can still be stored.
    /// Lock, I/O and other database-wide failures are not hand-specific and abort the batch.
    #[must_use]
    pub fn is_hand_specific(&self) -> bool {
        match self {
            Self::Sqlite(e) => e.sqlite_error_code() == Some(ErrorCode::ConstraintViolation),
            Self::InvalidHand(_)
            | Self::CurrencyConflict { .. }
            | Self::ContentConflict { .. }
            | Self::LossyMonetaryConversion { .. }
            | Self::MissingPlayerId { .. }
            | Self::CorruptRow { .. } => true,
            _ => false,
        }
    }
}

/// Where a hand was imported from. Stored with the hand so an import can be traced, undone or
/// re-parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandSource {
    pub import_run_id: Uuid,
    pub file_path: String,
    /// 1-based line in `file_path` where the hand starts.
    pub start_line: usize,
    /// The hand block exactly as it appears in the file.
    pub raw_text: String,
}

/// A hand an import could not store, kept with its raw text until a later import (for example
/// after a parser update) stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuarantinedHand {
    pub import_run_id: Uuid,
    /// Parser that handled the file (see [`crate::parser::HandParser::describe`]).
    pub parser: String,
    pub file_path: String,
    /// 1-based line in `file_path` where the hand starts.
    pub start_line: usize,
    pub site_hand_id: Option<String>,
    /// Pipeline stage that rejected the hand (e.g. `"parse"` or `"storage"`).
    pub stage: String,
    pub error: String,
    pub raw_text: String,
}

/// A hand from a batch that was not stored. The rest of the batch is unaffected.
#[derive(Debug)]
pub struct RejectedHand {
    /// Index of the hand in the slice passed to [`SqliteRepository::insert_hands_batch`].
    pub batch_index: usize,
    pub site_hand_id: String,
    pub error: StorageError,
}

/// Summary returned after committing a batch of hands.
#[derive(Debug, Default)]
pub struct BatchInsertStats {
    pub hands_inserted: usize,
    pub duplicates_skipped: usize,
    pub rejected: Vec<RejectedHand>,
}

/// What [`SqliteRepository::delete_import_run`] removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeletedImportRun {
    /// Hands that no other import run contained, deleted with all their rows.
    pub hands_deleted: usize,
    /// Hands this run contained that another import run also contained or that were stored without a source; they are kept.
    pub hands_kept: usize,
    /// Quarantined hands recorded by this run.
    pub quarantined_deleted: usize,
}

/// Summary of a rebuild of the derived statistics cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerivedRebuild {
    pub hands: usize,
    pub version: i64,
}

enum InsertOutcome {
    Inserted,
    Duplicate,
}

/// A hand block with line endings unified and trailing whitespace and blank lines removed: the
/// form [`hand_text_sha256`] fingerprints, so re-exporting with CRLF or stripped trailing spaces
/// is still recognised as the same hand.
#[must_use]
pub fn normalized_hand_text(raw: &str) -> String {
    raw.strip_prefix('\u{feff}')
        .unwrap_or(raw)
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Lower-case hex SHA-256 of [`normalized_hand_text`].
#[must_use]
pub fn hand_text_sha256(raw: &str) -> String {
    let digest = Sha256::digest(normalized_hand_text(raw).as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

fn to_micros(field: &'static str, value: Decimal) -> Result<i64, StorageError> {
    decimal_to_micro_units(value).ok_or(StorageError::LossyMonetaryConversion { field, value })
}

fn card_code(card: Option<Card>) -> Option<i64> {
    card.map(|c| i64::from(c.raw()))
}

fn index_i64(index: usize) -> i64 {
    i64::try_from(index).unwrap_or(i64::MAX)
}

fn invariant(hand: &CanonicalHand, detail: &str) -> StorageError {
    StorageError::InvalidHand(ParseError::InvariantViolation {
        hand_id: hand.header.site_hand_id.clone(),
        detail: detail.to_string(),
    })
}

fn now_utc() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn register_run(conn: &Connection, run_id: Uuid, started_at: &str) -> Result<(), StorageError> {
    conn.prepare_cached(
        "INSERT INTO import_runs (id, started_at, tool_version) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO NOTHING",
    )?
    .execute(params![
        run_id.to_string(),
        started_at,
        env!("CARGO_PKG_VERSION")
    ])?;
    Ok(())
}

fn record_sighting(
    conn: &Connection,
    hand_db_id: i64,
    src: &HandSource,
) -> Result<(), StorageError> {
    conn.prepare_cached(
        "INSERT INTO hand_sightings (hand_id, import_run_id, source_file, source_line)
         VALUES (?1, ?2, ?3, ?4) ON CONFLICT DO NOTHING",
    )?
    .execute(params![
        hand_db_id,
        src.import_run_id.to_string(),
        src.file_path,
        index_i64(src.start_line)
    ])?;
    Ok(())
}

/// Inserts one validated hand. Must run inside a transaction/savepoint owned by the caller.
fn insert_one(
    conn: &Connection,
    hand: &CanonicalHand,
    source: Option<&HandSource>,
) -> Result<InsertOutcome, StorageError> {
    let h = &hand.header;
    let o = &hand.outcome;
    let platform = h.platform.as_str();
    let raw_sha = source.map(|src| hand_text_sha256(&src.raw_text));
    let deduction = |kind: DeductionKind, field: &'static str| -> Result<i64, StorageError> {
        let total = o
            .checked_deductions_of(kind)
            .ok_or_else(|| invariant(hand, "deduction overflow"))?;
        to_micros(field, total)
    };

    let changes = conn
        .prepare_cached(
            r"
            INSERT INTO hands (
                platform, site_hand_id, game_type, betting_limit, currency,
                small_blind_micros, big_blind_micros, timestamp, utc_offset_seconds, table_name,
                max_seats, button_seat, run_count,
                total_pot_micros, rake_micros, jackpot_micros, other_deductions_micros,
                uncalled_returned_micros, import_run_id, source_file, source_line,
                raw_text, raw_sha256
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                      ?19, ?20, ?21, ?22, ?23)
            ON CONFLICT(platform, site_hand_id) DO NOTHING
            ",
        )?
        .execute(params![
            platform,
            h.site_hand_id,
            h.game_type.as_str(),
            h.limit.as_str(),
            h.currency.as_str(),
            to_micros("small_blind", h.small_blind)?,
            to_micros("big_blind", h.big_blind)?,
            h.timestamp.format(TIMESTAMP_FORMAT).to_string(),
            h.utc_offset_seconds,
            h.table_name,
            i64::from(h.max_seats),
            i64::from(h.button_seat),
            index_i64(hand.run_count()),
            to_micros("total_pot", o.total_pot)?,
            deduction(DeductionKind::Rake, "rake")?,
            deduction(DeductionKind::Jackpot, "jackpot")?,
            deduction(DeductionKind::Other, "other_deductions")?,
            to_micros("uncalled_returned", o.uncalled_returned)?,
            source.map(|src| src.import_run_id.to_string()),
            source.map(|src| src.file_path.as_str()),
            source.map(|src| index_i64(src.start_line)),
            source.map(|src| src.raw_text.as_str()),
            raw_sha.as_deref(),
        ])?;
    if changes == 0 {
        // Already stored: a duplicate only if it is the same hand in the same currency.
        let (stored_id, stored_currency, stored_sha): (i64, String, Option<String>) = conn
            .prepare_cached(
                "SELECT id, currency, raw_sha256 FROM hands WHERE platform = ?1 AND site_hand_id = ?2",
            )?
            .query_row(params![platform, h.site_hand_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
        if stored_currency != h.currency.as_str() {
            return Err(StorageError::CurrencyConflict {
                site_hand_id: h.site_hand_id.clone(),
                stored: stored_currency,
                incoming: h.currency.to_string(),
            });
        }
        let same = match (stored_sha.as_deref(), raw_sha.as_deref()) {
            (Some(stored), Some(incoming)) => stored == incoming,
            _ => load_hand_by_id(conn, stored_id)? == *hand,
        };
        if !same {
            return Err(StorageError::ContentConflict {
                site_hand_id: h.site_hand_id.clone(),
            });
        }
        if let Some(src) = source {
            record_sighting(conn, stored_id, src)?;
            conn.prepare_cached(
                "UPDATE hands SET raw_text = ?1, raw_sha256 = ?2 WHERE id = ?3 AND raw_sha256 IS NULL",
            )?
            .execute(params![src.raw_text, raw_sha.as_deref(), stored_id])?;
        }
        return Ok(InsertOutcome::Duplicate);
    }
    let hand_db_id = conn.last_insert_rowid();
    if let Some(src) = source {
        record_sighting(conn, hand_db_id, src)?;
    }

    let results = hand
        .checked_player_results()
        .ok_or_else(|| invariant(hand, "player result overflow"))?;

    let mut player_ids: HashMap<&str, i64> = HashMap::with_capacity(hand.players.len());
    for (player, result) in hand.players.iter().zip(&results) {
        conn.prepare_cached(
            "INSERT INTO players (platform, screen_name) VALUES (?1, ?2)
             ON CONFLICT(platform, screen_name) DO NOTHING",
        )?
        .execute(params![platform, player.name])?;
        let player_id: i64 = conn
            .prepare_cached("SELECT id FROM players WHERE platform = ?1 AND screen_name = ?2")?
            .query_row(params![platform, player.name], |row| row.get(0))?;
        player_ids.insert(player.name.as_str(), player_id);

        conn.prepare_cached(
            r"
            INSERT INTO hand_players (
                hand_id, player_id, seat, starting_stack_micros, hole_card_1, hole_card_2,
                is_hero, invested_micros, won_micros, adjustments_micros, net_micros
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ",
        )?
        .execute(params![
            hand_db_id,
            player_id,
            i64::from(player.seat),
            to_micros("starting_stack", player.starting_stack)?,
            card_code(player.hole_cards[0]),
            card_code(player.hole_cards[1]),
            i64::from(u8::from(player.is_hero)),
            to_micros("invested", result.invested)?,
            to_micros("won", result.won)?,
            to_micros("adjustments", result.adjustments)?,
            to_micros("net", result.net)?,
        ])?;
    }

    let player_id = |name: &str| -> Result<i64, StorageError> {
        player_ids
            .get(name)
            .copied()
            .ok_or_else(|| StorageError::MissingPlayerId {
                hand_id: h.site_hand_id.clone(),
                player_name: name.to_string(),
            })
    };

    for act in &hand.actions {
        conn.prepare_cached(
            r"
            INSERT INTO actions (
                hand_id, player_id, sequence, street, action_type,
                amount_micros, street_total_after_micros, is_all_in
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
        )?
        .execute(params![
            hand_db_id,
            player_id(&act.player_name)?,
            i64::from(act.sequence),
            i64::from(act.street.code()),
            act.action_type.as_str(),
            to_micros("action_amount", act.amount)?,
            to_micros("street_total_after", act.street_total_after)?,
            i64::from(u8::from(act.is_all_in)),
        ])?;
    }

    for board in &hand.boards {
        let flop = board.flop.map_or([None, None, None], |f| f.map(Some));
        conn.prepare_cached(
            r"
            INSERT INTO board_cards (hand_id, run_index, flop_1, flop_2, flop_3, turn, river)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
        )?
        .execute(params![
            hand_db_id,
            i64::from(board.run_index),
            card_code(flop[0]),
            card_code(flop[1]),
            card_code(flop[2]),
            card_code(board.turn),
            card_code(board.river),
        ])?;
    }

    for (seq, winner) in o.winners.iter().enumerate() {
        conn.prepare_cached(
            r"
            INSERT INTO pot_winners (hand_id, player_id, pot_sequence, run_index, amount_micros)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ",
        )?
        .execute(params![
            hand_db_id,
            player_id(&winner.player_name)?,
            index_i64(seq),
            i64::from(winner.run_index),
            to_micros("pot_winner_amount", winner.amount)?,
        ])?;
    }

    for (seq, d) in o.deductions.iter().enumerate() {
        conn.prepare_cached(
            r"
            INSERT INTO pot_deductions (hand_id, deduction_sequence, kind, site_label, amount_micros)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ",
        )?
        .execute(params![
            hand_db_id,
            index_i64(seq),
            d.kind.as_str(),
            d.label,
            to_micros("deduction_amount", d.amount)?,
        ])?;
    }

    for (seq, a) in o.adjustments.iter().enumerate() {
        conn.prepare_cached(
            r"
            INSERT INTO player_adjustments (
                hand_id, player_id, adjustment_sequence, kind, street, amount_micros
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ",
        )?
        .execute(params![
            hand_db_id,
            player_id(&a.player_name)?,
            index_i64(seq),
            a.kind.as_str(),
            i64::from(a.street.code()),
            to_micros("adjustment_amount", a.amount)?,
        ])?;
    }

    let derived = crate::stats::derive_hand_stats(hand);
    for st in &derived {
        let pid = player_id(&st.player_name)?;
        insert_player_stats(conn, hand_db_id, pid, st)?;
    }

    Ok(InsertOutcome::Inserted)
}

fn create_hand_player_stats_sql() -> String {
    let mut sql = String::from(
        "CREATE TABLE hand_player_stats (\n\
         hand_id INTEGER NOT NULL REFERENCES hands(id) ON DELETE CASCADE,\n\
         player_id INTEGER NOT NULL REFERENCES players(id) ON DELETE RESTRICT,\n\
         position INTEGER CHECK (position IS NULL OR (position BETWEEN 0 AND 9)),\n",
    );
    for spec in crate::stats::COLUMN_SPECS {
        let col = spec.name;
        if spec.signed {
            sql.push_str(&format!("{col} INTEGER NOT NULL,\n"));
        } else {
            sql.push_str(&format!("{col} INTEGER NOT NULL CHECK ({col} >= 0),\n"));
        }
    }
    sql.push_str(
        "PRIMARY KEY (hand_id, player_id)\n\
         ) STRICT, WITHOUT ROWID;\n\
         CREATE INDEX idx_hand_player_stats_player ON hand_player_stats(player_id, hand_id);",
    );
    sql
}

fn insert_player_stats(
    conn: &Connection,
    hand_db_id: i64,
    player_id: i64,
    stats: &crate::stats::PlayerHandStats,
) -> Result<(), StorageError> {
    static SQL: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        let cols = crate::stats::COLUMNS.join(", ");
        let placeholders = vec!["?"; 3 + crate::stats::COLUMNS.len()].join(", ");
        format!(
            "INSERT INTO hand_player_stats (hand_id, player_id, position, {cols}) VALUES ({placeholders})"
        )
    });

    let mut values: Vec<rusqlite::types::Value> =
        Vec::with_capacity(3 + crate::stats::COLUMNS.len());
    values.push(rusqlite::types::Value::Integer(hand_db_id));
    values.push(rusqlite::types::Value::Integer(player_id));
    values.push(match stats.position {
        Some(p) => rusqlite::types::Value::Integer(i64::from(p)),
        None => rusqlite::types::Value::Null,
    });
    for col in crate::stats::COLUMNS {
        values.push(rusqlite::types::Value::Integer(stats.get(col).unwrap_or(0)));
    }
    conn.prepare_cached(&SQL)?
        .execute(rusqlite::params_from_iter(values))?;
    Ok(())
}

fn rebuild_derived_in_tx(conn: &Connection) -> Result<DerivedRebuild, StorageError> {
    conn.execute_batch("DROP TABLE IF EXISTS hand_player_stats;")?;
    let create_sql = create_hand_player_stats_sql();
    conn.execute_batch(&create_sql)?;

    let mut stmt = conn.prepare("SELECT id, platform, site_hand_id FROM hands ORDER BY id")?;
    let hands_meta: Vec<(i64, String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<Result<_, _>>()?;
    drop(stmt);

    let count = hands_meta.len();
    for (hand_id, platform_str, site_hand_id_str) in hands_meta {
        let hand = match load_hand_by_id(conn, hand_id) {
            Ok(h) => h,
            Err(e) => {
                return Err(StorageError::DerivedRebuildFailed {
                    platform: platform_str,
                    site_hand_id: site_hand_id_str,
                    source: Box::new(e),
                });
            }
        };

        let wrap_err = |e: StorageError| StorageError::DerivedRebuildFailed {
            platform: platform_str.clone(),
            site_hand_id: site_hand_id_str.clone(),
            source: Box::new(e),
        };

        let mut p_stmt = conn
            .prepare_cached(
                "SELECT p.screen_name, hp.player_id FROM hand_players hp
                 JOIN players p ON p.id = hp.player_id WHERE hp.hand_id = ?1",
            )
            .map_err(StorageError::from)
            .map_err(wrap_err)?;
        let p_rows: Vec<(String, i64)> = p_stmt
            .query_map(params![hand_id], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(StorageError::from)
            .map_err(wrap_err)?
            .collect::<Result<_, _>>()
            .map_err(StorageError::from)
            .map_err(wrap_err)?;
        let p_map: HashMap<String, i64> = p_rows.into_iter().collect();

        let derived = crate::stats::derive_hand_stats(&hand);
        for st in &derived {
            let pid = p_map.get(&st.player_name).copied().ok_or_else(|| {
                wrap_err(StorageError::MissingPlayerId {
                    hand_id: site_hand_id_str.clone(),
                    player_name: st.player_name.clone(),
                })
            })?;
            insert_player_stats(conn, hand_id, pid, st).map_err(wrap_err)?;
        }
    }

    let now = now_utc();
    conn.prepare_cached(
        "INSERT INTO derived_cache (name, version, built_at) VALUES ('hand_player_stats', ?1, ?2)
         ON CONFLICT(name) DO UPDATE SET version = excluded.version, built_at = excluded.built_at",
    )?
    .execute(params![crate::stats::STATS_VERSION, now])?;

    Ok(DerivedRebuild {
        hands: count,
        version: crate::stats::STATS_VERSION,
    })
}

pub(crate) fn rebuild_if_stale(
    conn: &mut Connection,
) -> Result<Option<DerivedRebuild>, StorageError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let locked_version: Option<i64> = tx
        .query_row(
            "SELECT version FROM derived_cache WHERE name = 'hand_player_stats'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if locked_version != Some(crate::stats::STATS_VERSION) {
        let rebuild = rebuild_derived_in_tx(&tx)?;
        tx.commit()?;
        Ok(Some(rebuild))
    } else {
        tx.commit()?;
        Ok(None)
    }
}

// ---------------------------------------------------------------------------------------------
// Reading hands back
// ---------------------------------------------------------------------------------------------

fn corrupt(table: &'static str, detail: impl Into<String>) -> StorageError {
    StorageError::CorruptRow {
        table,
        detail: detail.into(),
    }
}

fn small_int(table: &'static str, value: i64) -> Result<u8, StorageError> {
    u8::try_from(value).map_err(|_| corrupt(table, format!("{value} is out of range")))
}

fn sequence_int(table: &'static str, value: i64) -> Result<u16, StorageError> {
    u16::try_from(value).map_err(|_| corrupt(table, format!("{value} is out of range")))
}

fn stored<T: FromStr>(table: &'static str, value: &str) -> Result<T, StorageError> {
    value
        .parse()
        .map_err(|_| corrupt(table, format!("unknown identifier '{value}'")))
}

fn stored_card(table: &'static str, value: Option<i64>) -> Result<Option<Card>, StorageError> {
    value
        .map(|v| {
            u8::try_from(v)
                .ok()
                .and_then(Card::from_raw)
                .ok_or_else(|| corrupt(table, format!("invalid card code {v}")))
        })
        .transpose()
}

fn stored_street(table: &'static str, value: i64) -> Result<Street, StorageError> {
    Street::from_code(small_int(table, value)?)
        .ok_or_else(|| corrupt(table, format!("invalid street code {value}")))
}

fn collect_rows<T>(
    conn: &Connection,
    sql: &str,
    hand_db_id: i64,
    mut map: impl FnMut(&Row<'_>) -> Result<T, StorageError>,
) -> Result<Vec<T>, StorageError> {
    let mut stmt = conn.prepare_cached(sql)?;
    let mut rows = stmt.query(params![hand_db_id])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map(row)?);
    }
    Ok(out)
}

/// Rebuilds the canonical hand stored under `hands.id = hand_db_id`.
fn load_hand_by_id(conn: &Connection, hand_db_id: i64) -> Result<CanonicalHand, StorageError> {
    type HeaderRow = (
        String,
        String,
        String,
        String,
        String,
        i64,
        i64,
        String,
        Option<i32>,
        String,
        i64,
        i64,
        i64,
        i64,
    );
    let row: HeaderRow = conn
        .prepare_cached(
            r"
            SELECT platform, site_hand_id, game_type, betting_limit, currency,
                   small_blind_micros, big_blind_micros, timestamp, utc_offset_seconds,
                   table_name, max_seats, button_seat, total_pot_micros, uncalled_returned_micros
            FROM hands WHERE id = ?1
            ",
        )?
        .query_row(params![hand_db_id], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
                r.get(11)?,
                r.get(12)?,
                r.get(13)?,
            ))
        })?;
    let header = HandHeader {
        platform: stored::<Platform>("hands", &row.0)?,
        site_hand_id: row.1,
        game_type: stored::<GameType>("hands", &row.2)?,
        limit: stored::<BettingLimit>("hands", &row.3)?,
        currency: stored::<CurrencyCode>("hands", &row.4)?,
        small_blind: micro_units_to_decimal(row.5),
        big_blind: micro_units_to_decimal(row.6),
        timestamp: NaiveDateTime::parse_from_str(&row.7, TIMESTAMP_FORMAT)
            .map_err(|_| corrupt("hands", format!("invalid timestamp '{}'", row.7)))?,
        utc_offset_seconds: row.8,
        table_name: row.9,
        max_seats: small_int("hands", row.10)?,
        button_seat: small_int("hands", row.11)?,
    };

    let players = collect_rows(
        conn,
        r"
        SELECT p.screen_name, hp.seat, hp.starting_stack_micros, hp.hole_card_1, hp.hole_card_2,
               hp.is_hero
        FROM hand_players hp JOIN players p ON p.id = hp.player_id
        WHERE hp.hand_id = ?1 ORDER BY hp.id
        ",
        hand_db_id,
        |r| {
            Ok(Player {
                name: r.get(0)?,
                seat: small_int("hand_players", r.get(1)?)?,
                starting_stack: micro_units_to_decimal(r.get(2)?),
                hole_cards: [
                    stored_card("hand_players", r.get(3)?)?,
                    stored_card("hand_players", r.get(4)?)?,
                ],
                is_hero: r.get::<_, i64>(5)? == 1,
            })
        },
    )?;

    let actions = collect_rows(
        conn,
        r"
        SELECT p.screen_name, a.sequence, a.street, a.action_type, a.amount_micros,
               a.street_total_after_micros, a.is_all_in
        FROM actions a JOIN players p ON p.id = a.player_id
        WHERE a.hand_id = ?1 ORDER BY a.sequence
        ",
        hand_db_id,
        |r| {
            Ok(StreetAction {
                player_name: r.get(0)?,
                sequence: sequence_int("actions", r.get(1)?)?,
                street: stored_street("actions", r.get(2)?)?,
                action_type: stored::<ActionType>("actions", &r.get::<_, String>(3)?)?,
                amount: micro_units_to_decimal(r.get(4)?),
                street_total_after: micro_units_to_decimal(r.get(5)?),
                is_all_in: r.get::<_, i64>(6)? == 1,
            })
        },
    )?;

    let boards = collect_rows(
        conn,
        r"
        SELECT run_index, flop_1, flop_2, flop_3, turn, river
        FROM board_cards WHERE hand_id = ?1 ORDER BY id
        ",
        hand_db_id,
        |r| {
            let flop = match (
                stored_card("board_cards", r.get(1)?)?,
                stored_card("board_cards", r.get(2)?)?,
                stored_card("board_cards", r.get(3)?)?,
            ) {
                (Some(a), Some(b), Some(c)) => Some([a, b, c]),
                (None, None, None) => None,
                _ => return Err(corrupt("board_cards", "partial flop")),
            };
            Ok(CommunityCards {
                run_index: small_int("board_cards", r.get(0)?)?,
                flop,
                turn: stored_card("board_cards", r.get(4)?)?,
                river: stored_card("board_cards", r.get(5)?)?,
            })
        },
    )?;

    let winners = collect_rows(
        conn,
        r"
        SELECT p.screen_name, w.amount_micros, w.run_index
        FROM pot_winners w JOIN players p ON p.id = w.player_id
        WHERE w.hand_id = ?1 ORDER BY w.pot_sequence
        ",
        hand_db_id,
        |r| {
            Ok(PotWinner {
                player_name: r.get(0)?,
                amount: micro_units_to_decimal(r.get(1)?),
                run_index: small_int("pot_winners", r.get(2)?)?,
            })
        },
    )?;

    let deductions = collect_rows(
        conn,
        r"
        SELECT kind, site_label, amount_micros
        FROM pot_deductions WHERE hand_id = ?1 ORDER BY deduction_sequence
        ",
        hand_db_id,
        |r| {
            Ok(PotDeduction {
                kind: stored::<DeductionKind>("pot_deductions", &r.get::<_, String>(0)?)?,
                label: r.get(1)?,
                amount: micro_units_to_decimal(r.get(2)?),
            })
        },
    )?;

    let adjustments = collect_rows(
        conn,
        r"
        SELECT p.screen_name, a.kind, a.street, a.amount_micros
        FROM player_adjustments a JOIN players p ON p.id = a.player_id
        WHERE a.hand_id = ?1 ORDER BY a.adjustment_sequence
        ",
        hand_db_id,
        |r| {
            Ok(PlayerAdjustment {
                player_name: r.get(0)?,
                kind: stored::<AdjustmentKind>("player_adjustments", &r.get::<_, String>(1)?)?,
                street: stored_street("player_adjustments", r.get(2)?)?,
                amount: micro_units_to_decimal(r.get(3)?),
            })
        },
    )?;

    Ok(CanonicalHand {
        header,
        players,
        actions,
        boards,
        outcome: HandOutcome {
            total_pot: micro_units_to_decimal(row.12),
            deductions,
            uncalled_returned: micro_units_to_decimal(row.13),
            winners,
            adjustments,
        },
    })
}

// ---------------------------------------------------------------------------------------------
// Repository
// ---------------------------------------------------------------------------------------------

/// What the file at hand is, judged before anything is written to it.
enum DatabaseKind {
    /// No tables and no identity yet: a new database.
    New,
    /// A `poker_tracker` database at the given schema version.
    PokerTracker(i64),
}

fn identify(conn: &Connection) -> Result<DatabaseKind, StorageError> {
    let app_id: i64 = conn.query_row("PRAGMA application_id;", [], |row| row.get(0))?;
    let version: i64 = conn.query_row("PRAGMA user_version;", [], |row| row.get(0))?;
    let table_exists = |name: &str| -> Result<bool, StorageError> {
        Ok(conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            params![name],
            |row| row.get(0),
        )?)
    };
    let has_schema_objects: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name NOT LIKE 'sqlite_%')",
        [],
        |row| row.get(0),
    )?;
    if app_id == APPLICATION_ID {
        return Ok(DatabaseKind::PokerTracker(version));
    }
    if app_id == 0 && version == 0 && !has_schema_objects {
        return Ok(DatabaseKind::New);
    }
    // v3.4 databases predate the application id: schema version 1 with its tables.
    if app_id == 0 && version == 1 && table_exists("hands")? && table_exists("import_runs")? {
        return Ok(DatabaseKind::PokerTracker(1));
    }
    let detail = if app_id == 0 {
        format!(
            "this file is not a poker_tracker v3.4+ database (schema version {version}); a database \
             from poker_tracker v3.3 or earlier must be deleted and its hands re-imported, and any \
             other SQLite file is left untouched"
        )
    } else {
        format!(
            "this SQLite file belongs to another application (application_id {app_id:#x}); it was \
             left untouched"
        )
    };
    Err(StorageError::NotAPokerTrackerDatabase { detail })
}

/// [`identify`] inside one deferred read transaction (a consistent snapshot of the header and
/// the schema).
fn identify_snapshot(conn: &mut Connection) -> Result<DatabaseKind, StorageError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Deferred)?;
    let kind = identify(&tx)?;
    tx.commit()?;
    Ok(kind)
}

/// SQLite-backed repository managing schema initialization and batch ingestion.
pub struct SqliteRepository {
    conn: Connection,
    rebuilt_on_open: Option<DerivedRebuild>,
}

impl SqliteRepository {
    /// Opens or creates a SQLite database at `path`, identifies it, applies pragmas, and runs
    /// any pending migrations.
    ///
    /// # Errors
    /// Returns [`StorageError`] if opening the file or a migration fails, if the file is not a
    /// `poker_tracker` database ([`StorageError::NotAPokerTrackerDatabase`], nothing is changed),
    /// or if it was created by a newer build.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let conn = Connection::open(path)?;
        Self::initialize(conn)
    }

    /// Opens an existing database without ever creating one (e.g. to undo an import).
    ///
    /// # Errors
    /// [`StorageError::DatabaseNotFound`] if no file exists at `path`; otherwise as
    /// [`Self::open`].
    pub fn open_existing<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let path = path.as_ref();
        if !path.is_file() {
            return Err(StorageError::DatabaseNotFound {
                path: path.display().to_string(),
            });
        }
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Self::initialize(conn)
    }

    /// Creates an in-memory SQLite repository suitable for testing.
    ///
    /// # Errors
    /// Returns [`StorageError`] if schema initialization fails.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        Self::initialize(conn)
    }

    fn initialize(mut conn: Connection) -> Result<Self, StorageError> {
        conn.busy_timeout(DEFAULT_BUSY_TIMEOUT)?;
        let start = std::time::Instant::now();
        let busy = |e: &rusqlite::Error| {
            e.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseBusy)
                && start.elapsed() < DEFAULT_BUSY_TIMEOUT
        };
        // Identify the file before changing anything about it. The reads run in one read
        // transaction, so they see a single committed state: a concurrent open that is
        // migrating the file can never make it look foreign. Only a busy lock is retried.
        let current = loop {
            match identify_snapshot(&mut conn) {
                Ok(DatabaseKind::New) => break 0,
                Ok(DatabaseKind::PokerTracker(version)) => break version,
                Err(StorageError::Sqlite(e)) if busy(&e) => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => return Err(e),
            }
        };
        if !(0..=SCHEMA_VERSION).contains(&current) {
            return Err(StorageError::UnsupportedSchemaVersion {
                found: current,
                expected: SCHEMA_VERSION,
            });
        }
        loop {
            // Switching to WAL can report a busy lock without calling the busy handler.
            match conn.execute_batch(PRAGMA_TUNING) {
                Ok(()) => break,
                Err(e) if busy(&e) => std::thread::sleep(Duration::from_millis(5)),
                Err(e) => return Err(StorageError::Sqlite(e)),
            }
        }
        // Apply pending migrations in order, each atomically with its version bump. The version
        // is re-read under the write lock, so a concurrent open never applies a step twice.
        loop {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let version: i64 = tx.query_row("PRAGMA user_version;", [], |row| row.get(0))?;
            let next = usize::try_from(version).unwrap_or(usize::MAX);
            let Some(sql) = MIGRATIONS.get(next) else {
                tx.commit()?;
                break;
            };
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "application_id", APPLICATION_ID)?;
            tx.pragma_update(None, "user_version", index_i64(next + 1))?;
            tx.commit()?;
        }

        let cached_version: Option<i64> = conn
            .query_row(
                "SELECT version FROM derived_cache WHERE name = 'hand_player_stats'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let mut rebuilt_on_open = None;
        if cached_version != Some(crate::stats::STATS_VERSION) {
            rebuilt_on_open = rebuild_if_stale(&mut conn)?;
        }

        Ok(Self {
            conn,
            rebuilt_on_open,
        })
    }

    /// Rebuild summary if the statistics cache was rebuilt when the database was opened.
    #[must_use]
    pub fn rebuilt_on_open(&self) -> Option<DerivedRebuild> {
        self.rebuilt_on_open
    }

    /// Unconditionally rebuilds the derived statistics cache for every stored hand.
    ///
    /// # Errors
    /// Returns [`StorageError`] if a hand cannot be loaded or a database write fails.
    pub fn rebuild_derived(&mut self) -> Result<DerivedRebuild, StorageError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let rebuild = rebuild_derived_in_tx(&tx)?;
        tx.commit()?;
        Ok(rebuild)
    }

    /// Changes how long a write waits for another connection's lock ([`DEFAULT_BUSY_TIMEOUT`]
    /// by default) before failing.
    ///
    /// # Errors
    /// Returns [`StorageError::Sqlite`] if the timeout cannot be set.
    pub fn set_busy_timeout(&self, timeout: Duration) -> Result<(), StorageError> {
        Ok(self.conn.busy_timeout(timeout)?)
    }

    /// Schema version of the open database (`PRAGMA user_version`).
    ///
    /// # Errors
    /// Returns [`StorageError::Sqlite`] if the pragma cannot be read.
    pub fn schema_version(&self) -> Result<i64, StorageError> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version;", [], |row| row.get(0))?)
    }

    /// Records an import run and the time it started. Idempotent; batches and quarantine writes
    /// that reference an unregistered run register it with the current time instead.
    ///
    /// # Errors
    /// Returns [`StorageError::Sqlite`] if the write fails.
    pub fn register_import_run(
        &mut self,
        run_id: Uuid,
        started_at: DateTime<Utc>,
    ) -> Result<(), StorageError> {
        let started = started_at.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        register_run(&tx, run_id, &started)?;
        tx.commit()?;
        Ok(())
    }

    /// Undoes an import run: deletes the hands this run stored that no other import run
    /// contained (with all their rows), keeps every other hand the run contained (moving the
    /// provenance of hands it stored to the earliest remaining run), and removes the run's
    /// quarantine entries and the run record. Hands stored without a source are never deleted.
    ///
    /// # Errors
    /// [`StorageError::UnknownImportRun`] if no such run exists, or [`StorageError::Sqlite`] if
    /// the deletion fails; nothing is deleted in either case.
    pub fn delete_import_run(&mut self, run_id: Uuid) -> Result<DeletedImportRun, StorageError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = run_id.to_string();
        let known: Option<i64> = tx
            .query_row(
                "SELECT 1 FROM import_runs WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        if known.is_none() {
            return Err(StorageError::UnknownImportRun { run_id });
        }
        let hands_deleted = tx.execute(
            r"
            DELETE FROM hands
            WHERE import_run_id = ?1
              AND NOT EXISTS (
                  SELECT 1 FROM hand_sightings o
                  WHERE o.hand_id = hands.id AND o.import_run_id != ?1)
            ",
            params![id],
        )?;
        let hands_kept: i64 = tx.query_row(
            "SELECT COUNT(*) FROM hand_sightings WHERE import_run_id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        tx.execute(
            r"
            UPDATE hands SET (import_run_id, source_file, source_line) = (
                SELECT s.import_run_id, s.source_file, s.source_line
                FROM hand_sightings s JOIN import_runs r ON r.id = s.import_run_id
                WHERE s.hand_id = hands.id AND s.import_run_id != ?1
                ORDER BY r.started_at, s.import_run_id LIMIT 1)
            WHERE import_run_id = ?1
            ",
            params![id],
        )?;
        let quarantined_deleted = tx.execute(
            "DELETE FROM quarantined_hands WHERE import_run_id = ?1",
            params![id],
        )?;
        tx.execute("DELETE FROM import_runs WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(DeletedImportRun {
            hands_deleted,
            hands_kept: usize::try_from(hands_kept).unwrap_or(0),
            quarantined_deleted,
        })
    }

    /// Reads a stored hand back into the canonical model, or `None` if it is not stored.
    ///
    /// # Errors
    /// [`StorageError::Sqlite`] on a query failure, or [`StorageError::CorruptRow`] if a stored
    /// value cannot be represented in the model.
    pub fn load_hand(
        &self,
        platform: Platform,
        site_hand_id: &str,
    ) -> Result<Option<CanonicalHand>, StorageError> {
        let id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM hands WHERE platform = ?1 AND site_hand_id = ?2",
                params![platform.as_str(), site_hand_id],
                |row| row.get(0),
            )
            .optional()?;
        id.map(|id| load_hand_by_id(&self.conn, id)).transpose()
    }

    /// Borrows the underlying [`rusqlite::Connection`].
    #[must_use]
    pub const fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Validates and inserts hands without source information (see
    /// [`Self::insert_sourced_hands_batch`] for the semantics). Without raw text, a hand already
    /// stored under the same id is compared by its canonical content.
    ///
    /// # Errors
    /// As [`Self::insert_sourced_hands_batch`].
    pub fn insert_hands_batch(
        &mut self,
        hands: &[CanonicalHand],
    ) -> Result<BatchInsertStats, StorageError> {
        self.insert_batch(hands, None, None)
    }

    /// Validates and inserts a slice of hands (from any site, in any currency) in one SQLite
    /// transaction, recording for each hand its raw text and the import run, file and line it
    /// came from. `sources[i]` describes `hands[i]`.
    ///
    /// Each hand is written inside its own savepoint: a hand that fails validation, conversion
    /// or a constraint is rolled back individually and reported in
    /// [`BatchInsertStats::rejected`], while the rest of the batch is committed. A hand whose
    /// `(platform, site_hand_id)` already exists is a duplicate only if it is the same hand:
    /// same currency (else [`StorageError::CurrencyConflict`]) and same content (else
    /// [`StorageError::ContentConflict`]), compared by the SHA-256 of the normalized raw text
    /// when both imports carry it, otherwise by the canonical hand. Every import run that
    /// contains a hand is recorded for it; a quarantined text that gets stored leaves the
    /// `quarantine_pending` view.
    ///
    /// # Errors
    /// Returns [`StorageError`] for failures that affect the whole transaction (taking the write
    /// lock within the busy timeout, savepoint management, I/O, commit), in which case nothing
    /// from this batch is stored, or [`StorageError::SourceCountMismatch`] if the slices differ
    /// in length.
    pub fn insert_sourced_hands_batch(
        &mut self,
        hands: &[CanonicalHand],
        sources: &[HandSource],
    ) -> Result<BatchInsertStats, StorageError> {
        self.insert_sourced_hands_batch_at(hands, sources, None)
    }

    /// Validates and inserts a slice of hands in one SQLite transaction, recording provenance and
    /// registering referenced import runs with `started_at` (or current time if `None`).
    ///
    /// # Errors
    /// As [`Self::insert_sourced_hands_batch`].
    pub fn insert_sourced_hands_batch_at(
        &mut self,
        hands: &[CanonicalHand],
        sources: &[HandSource],
        started_at: Option<DateTime<Utc>>,
    ) -> Result<BatchInsertStats, StorageError> {
        if hands.len() != sources.len() {
            return Err(StorageError::SourceCountMismatch {
                hands: hands.len(),
                sources: sources.len(),
            });
        }
        self.insert_batch(hands, Some(sources), started_at)
    }

    fn insert_batch(
        &mut self,
        hands: &[CanonicalHand],
        sources: Option<&[HandSource]>,
        started_at: Option<DateTime<Utc>>,
    ) -> Result<BatchInsertStats, StorageError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let found_version: Option<i64> = tx
            .query_row(
                "SELECT version FROM derived_cache WHERE name = 'hand_player_stats'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if found_version != Some(crate::stats::STATS_VERSION) {
            return Err(StorageError::StaleDerivedCache {
                found: found_version,
                expected: crate::stats::STATS_VERSION,
            });
        }

        let mut stats = BatchInsertStats::default();

        // Register every import run referenced by this batch.
        let mut runs: Vec<Uuid> = sources
            .unwrap_or(&[])
            .iter()
            .map(|src| src.import_run_id)
            .collect();
        runs.sort_unstable();
        runs.dedup();
        let default_time = now_utc();
        let run_started = match started_at {
            Some(dt) => dt.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            None => default_time,
        };
        for run in runs {
            register_run(&tx, run, &run_started)?;
        }

        for (batch_index, hand) in hands.iter().enumerate() {
            let reject = |error: StorageError| RejectedHand {
                batch_index,
                site_hand_id: hand.header.site_hand_id.clone(),
                error,
            };
            if let Err(e) = validate_hand(hand) {
                stats.rejected.push(reject(StorageError::InvalidHand(e)));
                continue;
            }
            tx.execute_batch("SAVEPOINT hand_insert")?;
            let source = sources.and_then(|all| all.get(batch_index));
            match insert_one(&tx, hand, source) {
                Ok(outcome) => {
                    tx.execute_batch("RELEASE hand_insert")?;
                    match outcome {
                        InsertOutcome::Inserted => stats.hands_inserted += 1,
                        InsertOutcome::Duplicate => stats.duplicates_skipped += 1,
                    }
                }
                Err(e) if e.is_hand_specific() => {
                    tx.execute_batch("ROLLBACK TO hand_insert; RELEASE hand_insert")?;
                    stats.rejected.push(reject(e));
                }
                // Lock, I/O and other database-wide errors: the whole batch is rolled back.
                Err(e) => return Err(e),
            }
        }

        tx.commit()?;
        Ok(stats)
    }

    /// Stores rejected hands with their raw text, once per import run and text. Returns the
    /// number of new quarantine entries. Entries stay until their run is deleted; the
    /// `quarantine_pending` view lists those whose text is not stored as a hand.
    ///
    /// # Errors
    /// Returns [`StorageError::Sqlite`] if the transaction fails; nothing is stored then.
    pub fn quarantine_hands(&mut self, hands: &[QuarantinedHand]) -> Result<usize, StorageError> {
        self.quarantine_hands_at(hands, None)
    }

    /// Stores rejected hands with their raw text, registering the referenced import run with
    /// `started_at` (or current time if `None`).
    ///
    /// # Errors
    /// As [`Self::quarantine_hands`].
    pub fn quarantine_hands_at(
        &mut self,
        hands: &[QuarantinedHand],
        started_at: Option<DateTime<Utc>>,
    ) -> Result<usize, StorageError> {
        if hands.is_empty() {
            return Ok(0);
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let default_time = now_utc();
        let run_started = match started_at {
            Some(dt) => dt.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            None => default_time,
        };
        let mut added = 0;
        for q in hands {
            register_run(&tx, q.import_run_id, &run_started)?;
            added += tx
                .prepare_cached(
                    r"
                    INSERT INTO quarantined_hands (
                        import_run_id, raw_sha256, raw_text, parser, site_hand_id, stage, error,
                        source_file, source_line
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                    ON CONFLICT(import_run_id, raw_sha256) DO NOTHING
                    ",
                )?
                .execute(params![
                    q.import_run_id.to_string(),
                    hand_text_sha256(&q.raw_text),
                    q.raw_text,
                    q.parser,
                    q.site_hand_id,
                    q.stage,
                    q.error,
                    q.file_path,
                    index_i64(q.start_line.max(1)),
                ])?;
        }
        tx.commit()?;
        Ok(added)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn rebuild_if_stale_skips_rebuild_when_locked_version_is_current() {
        let mut repo = SqliteRepository::open_in_memory().unwrap();
        let built_at: String = repo
            .conn
            .query_row(
                "SELECT built_at FROM derived_cache WHERE name = 'hand_player_stats'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        // An open whose unlocked read saw a stale version, but whose locked read sees
        // STATS_VERSION, does not rebuild: Ok(None) returned, built_at unchanged.
        let result = rebuild_if_stale(&mut repo.conn).unwrap();
        assert_eq!(result, None);

        let built_at_after: String = repo
            .conn
            .query_row(
                "SELECT built_at FROM derived_cache WHERE name = 'hand_player_stats'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(built_at, built_at_after);
    }
    #[test]
    fn rebuild_if_stale_rebuilds_only_when_the_locked_version_is_stale() {
        use crate::parser::{GgPokerParser, HandParser};
        let text = include_str!("../../tests/fixtures/ggpoker/NLHRed15_2026-08-21.txt");
        let parser = GgPokerParser::new(crate::domain::CurrencyCode::CAD);
        let block = parser.split_blocks(text).into_iter().next().unwrap();
        let hand = parser.parse_hand(block.text).unwrap();
        let mut repo = SqliteRepository::open_in_memory().unwrap();
        repo.insert_hands_batch(&[hand]).unwrap();
        let sentinel = "2000-01-01T00:00:00Z";
        let cache = |conn: &Connection| -> (i64, String, Vec<(i64, i64, i64)>) {
            let (version, built_at) = conn
                .query_row(
                    "SELECT version, built_at FROM derived_cache WHERE name = 'hand_player_stats'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            let rows = conn
                .prepare("SELECT hand_id, player_id, p_vpip FROM hand_player_stats ORDER BY 1, 2")
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            (version, built_at, rows)
        };

        // Current under the lock: nothing is rebuilt; the sentinel and a doctored row survive.
        repo.conn
            .execute_batch(&format!(
                "UPDATE derived_cache SET built_at = '{sentinel}';
                 UPDATE hand_player_stats SET p_vpip = 1 - p_vpip WHERE player_id = (SELECT MIN(player_id) FROM hand_player_stats);"
            ))
            .unwrap();
        let before = cache(&repo.conn);
        assert_eq!(before.1, sentinel);
        assert_eq!(rebuild_if_stale(&mut repo.conn).unwrap(), None);
        assert_eq!(cache(&repo.conn), before);

        // Stale under the lock: one hand is rebuilt, the version and the doctored row restored.
        repo.conn
            .execute(
                "UPDATE derived_cache SET version = ?1",
                [crate::stats::STATS_VERSION + 1],
            )
            .unwrap();
        let rebuild = rebuild_if_stale(&mut repo.conn).unwrap().unwrap();
        assert_eq!(rebuild.hands, 1);
        let after = cache(&repo.conn);
        assert_eq!(after.0, crate::stats::STATS_VERSION);
        assert_ne!(after.1, sentinel);
        assert_ne!(after.2, before.2);
    }
}
