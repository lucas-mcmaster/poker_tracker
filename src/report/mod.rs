//! CSV reporting module for results, positions, sessions, and statistics (SPEC_v3.7 §6).

pub mod format;
pub mod positions;
pub mod results;
pub mod sessions;
pub mod stats;

use std::collections::HashSet;
use std::io::Write;
use std::str::FromStr;

use rusqlite::OptionalExtension;
use rust_decimal::Decimal;
use thiserror::Error;

use crate::domain::money::{decimal_to_micro_units, micro_units_to_decimal};
use crate::storage::{SqliteRepository, StorageError};

/// The four supported report kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportKind {
    Results,
    Positions,
    Sessions,
    Stats,
}

impl FromStr for ReportKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "results" => Ok(Self::Results),
            "positions" => Ok(Self::Positions),
            "sessions" => Ok(Self::Sessions),
            "stats" => Ok(Self::Stats),
            _ => Err(format!(
                "unknown report kind '{s}': expected results, positions, sessions, or stats"
            )),
        }
    }
}

/// Blinds pair identifying a stake level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Stakes {
    pub small_blind_micros: i64,
    pub big_blind_micros: i64,
}

impl FromStr for Stakes {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 2 {
            return Err(format!("invalid stakes '{s}': expected <sb>/<bb>"));
        }
        let sb_str = parts[0].trim();
        let bb_str = parts[1].trim();
        if sb_str.is_empty() || bb_str.is_empty() {
            return Err(format!("invalid stakes '{s}': empty blind amount"));
        }

        let sb: Decimal = sb_str
            .parse()
            .map_err(|e| format!("invalid small blind '{sb_str}': {e}"))?;
        let bb: Decimal = bb_str
            .parse()
            .map_err(|e| format!("invalid big blind '{bb_str}': {e}"))?;

        if sb <= Decimal::ZERO || bb <= Decimal::ZERO {
            return Err(format!("invalid stakes '{s}': amounts must be positive"));
        }
        if sb.scale() > 6 || bb.scale() > 6 {
            return Err(format!("invalid stakes '{s}': at most 6 decimal places"));
        }

        let small_blind_micros =
            decimal_to_micro_units(sb).ok_or_else(|| format!("invalid small blind '{sb_str}'"))?;
        let big_blind_micros =
            decimal_to_micro_units(bb).ok_or_else(|| format!("invalid big blind '{bb_str}'"))?;

        Ok(Self {
            small_blind_micros,
            big_blind_micros,
        })
    }
}

impl std::fmt::Display for Stakes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}/{}",
            micro_units_to_decimal(self.small_blind_micros),
            micro_units_to_decimal(self.big_blind_micros)
        )
    }
}

/// Filter options for report generation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReportOptions {
    pub stakes: Option<Vec<Stakes>>,
}

/// Summary outcome of report generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportOutcome {
    pub unmatched_stakes: Vec<Stakes>,
}

/// Errors during report generation.
#[derive(Debug, Error)]
pub enum ReportError {
    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<rusqlite::Error> for ReportError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Storage(StorageError::Sqlite(err))
    }
}

pub(crate) fn stakes_clause(stakes: Option<&[Stakes]>) -> (String, Vec<rusqlite::types::Value>) {
    match stakes {
        None => (String::new(), Vec::new()),
        Some([]) => (" AND 1 = 0".to_string(), Vec::new()),
        Some(list) => {
            let mut clause = String::from(" AND (");
            let mut params = Vec::with_capacity(list.len() * 2);
            for (i, s) in list.iter().enumerate() {
                if i > 0 {
                    clause.push_str(" OR ");
                }
                clause.push_str("(h.small_blind_micros = ? AND h.big_blind_micros = ?)");
                params.push(rusqlite::types::Value::Integer(s.small_blind_micros));
                params.push(rusqlite::types::Value::Integer(s.big_blind_micros));
            }
            clause.push(')');
            (clause, params)
        }
    }
}

/// Generates a CSV report and writes it to `out`.
///
/// Runs entirely in one read transaction. The first read is the statistics cache
/// version guard (SPEC_v3.7 §2 step 5); if the cache is missing or stale,
/// returns [`ReportError::Storage`] with [`StorageError::StaleDerivedCache`] and
/// writes nothing to `out`.
///
/// # Errors
/// Returns [`ReportError`] on cache version mismatch, SQLite failure, or I/O failure.
pub fn write_report(
    repo: &SqliteRepository,
    kind: ReportKind,
    options: &ReportOptions,
    out: &mut impl Write,
) -> Result<ReportOutcome, ReportError> {
    let tx = repo.connection().unchecked_transaction()?;

    // Version guard (§2 step 5)
    let found_version: Option<i64> = tx
        .query_row(
            "SELECT version FROM derived_cache WHERE name = 'hand_player_stats'",
            [],
            |row| row.get(0),
        )
        .optional()?;

    if found_version != Some(crate::stats::STATS_VERSION) {
        return Err(ReportError::Storage(StorageError::StaleDerivedCache {
            found: found_version,
            expected: crate::stats::STATS_VERSION,
        }));
    }

    // Deduplicate stakes while preserving order
    let dedup_stakes = options.stakes.as_ref().map(|list| {
        let mut seen = HashSet::new();
        let mut out_list = Vec::new();
        for s in list {
            if seen.insert(*s) {
                out_list.push(*s);
            }
        }
        out_list
    });

    let unmatched_stakes = match &dedup_stakes {
        Some(stakes) => {
            let mut unmatched = Vec::new();
            let mut stmt = tx.prepare_cached(
                "SELECT 1 FROM hands h \
                 JOIN hand_players hp ON hp.hand_id = h.id AND hp.is_hero = 1 \
                 WHERE h.small_blind_micros = ?1 AND h.big_blind_micros = ?2 LIMIT 1",
            )?;
            for s in stakes {
                let exists: bool = stmt.exists([s.small_blind_micros, s.big_blind_micros])?;
                if !exists {
                    unmatched.push(*s);
                }
            }
            unmatched
        }
        None => Vec::new(),
    };

    let stakes_slice = dedup_stakes.as_deref();

    match kind {
        ReportKind::Results => {
            let rows = results::build_results_rows(&tx, stakes_slice)?;
            writeln!(out, "{}", results::RESULTS_HEADER)?;
            for r in rows {
                writeln!(out, "{}", r.to_csv_line())?;
            }
        }
        ReportKind::Positions => {
            let rows = positions::build_positions_rows(&tx, stakes_slice)?;
            writeln!(out, "{}", positions::POSITIONS_HEADER)?;
            for r in rows {
                writeln!(out, "{}", r.to_csv_line())?;
            }
        }
        ReportKind::Sessions => {
            let rows = sessions::build_sessions_rows(&tx, stakes_slice)?;
            writeln!(out, "{}", sessions::SESSIONS_HEADER)?;
            for r in rows {
                writeln!(out, "{}", r.to_csv_line())?;
            }
        }
        ReportKind::Stats => {
            let rows = stats::build_stats_rows(&tx, stakes_slice)?;
            writeln!(out, "{}", stats::STATS_HEADER)?;
            for r in rows {
                writeln!(out, "{}", r.to_csv_line())?;
            }
        }
    }

    tx.finish()?;

    Ok(ReportOutcome { unmatched_stakes })
}
