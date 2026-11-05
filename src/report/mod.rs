//! CSV reporting module for results, positions, sessions, and statistics (SPEC_v3.7 §6).

pub mod format;
pub mod positions;
pub mod results;
pub(crate) mod selection;
pub mod sessions;
pub mod stats;

pub use crate::domain::money::Stakes;
pub use crate::stats::PositionLabel;
pub use positions::PositionsRow;
pub use results::ResultsRow;
pub(crate) use selection::selection;
pub use sessions::SessionsRow;
pub use stats::StatsRow;

use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::str::FromStr;

use rusqlite::OptionalExtension;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::storage::{SqliteRepository, StorageError};

/// The four supported report kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

/// Container for a complete typed report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    #[serde(flatten)]
    pub rows: ReportRows,
    pub unmatched_stakes: Vec<Stakes>,
}

/// Typed rows by report kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "rows", rename_all = "snake_case")]
pub enum ReportRows {
    Results(Vec<ResultsRow>),
    Positions(Vec<PositionsRow>),
    Sessions(Vec<SessionsRow>),
    Stats(Vec<StatsRow>),
}

/// Errors during report generation.
#[derive(Debug, Error)]
pub enum ReportError {
    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("arithmetic overflow computing {figure}")]
    ArithmeticOverflow { figure: &'static str },
}

impl From<rusqlite::Error> for ReportError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Storage(StorageError::Sqlite(err))
    }
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// Exact rational number represented by an integer numerator and positive denominator in lowest terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rational {
    num: i128,
    den: i128,
}

impl Rational {
    pub(crate) const ZERO: Self = Self { num: 0, den: 1 };

    pub(crate) fn new(num: i128, den: i64, figure: &'static str) -> Result<Self, ReportError> {
        if den == 0 {
            return Err(ReportError::ArithmeticOverflow { figure });
        }
        let (num_128, den_128) = if den < 0 {
            (
                num.checked_neg()
                    .ok_or(ReportError::ArithmeticOverflow { figure })?,
                -i128::from(den),
            )
        } else {
            (num, i128::from(den))
        };
        let g = i128::try_from(gcd_u128(num_128.unsigned_abs(), den_128.unsigned_abs()))
            .map_err(|_| ReportError::ArithmeticOverflow { figure })?;
        let g = if g == 0 { 1 } else { g };
        Ok(Self {
            num: num_128 / g,
            den: den_128 / g,
        })
    }

    pub(crate) fn checked_add(
        self,
        other: Self,
        figure: &'static str,
    ) -> Result<Self, ReportError> {
        let g = i128::try_from(gcd_u128(self.den.unsigned_abs(), other.den.unsigned_abs()))
            .map_err(|_| ReportError::ArithmeticOverflow { figure })?;
        let d1_div_g = self.den / g;
        let d2_div_g = other.den / g;

        let lcm = self
            .den
            .checked_mul(d2_div_g)
            .ok_or(ReportError::ArithmeticOverflow { figure })?;
        let term1 = self
            .num
            .checked_mul(d2_div_g)
            .ok_or(ReportError::ArithmeticOverflow { figure })?;
        let term2 = other
            .num
            .checked_mul(d1_div_g)
            .ok_or(ReportError::ArithmeticOverflow { figure })?;
        let num = term1
            .checked_add(term2)
            .ok_or(ReportError::ArithmeticOverflow { figure })?;

        let res_g = i128::try_from(gcd_u128(num.unsigned_abs(), lcm.unsigned_abs()))
            .map_err(|_| ReportError::ArithmeticOverflow { figure })?;
        let res_g = if res_g == 0 { 1 } else { res_g };

        Ok(Self {
            num: num / res_g,
            den: lcm / res_g,
        })
    }

    pub(crate) fn checked_per_100(
        self,
        hands: i64,
        figure: &'static str,
    ) -> Result<Self, ReportError> {
        if hands <= 0 {
            return Err(ReportError::ArithmeticOverflow { figure });
        }
        let num = self
            .num
            .checked_mul(100)
            .ok_or(ReportError::ArithmeticOverflow { figure })?;
        let den = self
            .den
            .checked_mul(i128::from(hands))
            .ok_or(ReportError::ArithmeticOverflow { figure })?;
        let g = i128::try_from(gcd_u128(num.unsigned_abs(), den.unsigned_abs()))
            .map_err(|_| ReportError::ArithmeticOverflow { figure })?;
        let g = if g == 0 { 1 } else { g };
        Ok(Self {
            num: num / g,
            den: den / g,
        })
    }

    pub(crate) fn round_half_even_2dp(self, figure: &'static str) -> Result<Decimal, ReportError> {
        if self.num == 0 {
            return Ok(Decimal::new(0, 2));
        }
        let is_neg = self.num < 0;
        let num_times_100 = self
            .num
            .checked_mul(100)
            .ok_or(ReportError::ArithmeticOverflow { figure })?;
        let abs_num = num_times_100.unsigned_abs();
        let d = self.den.unsigned_abs();

        let abs_q = abs_num / d;
        let abs_r = abs_num % d;

        let round_up = match abs_r.checked_mul(2) {
            Some(r2) => match r2.cmp(&d) {
                std::cmp::Ordering::Less => false,
                std::cmp::Ordering::Greater => true,
                std::cmp::Ordering::Equal => !abs_q.is_multiple_of(2),
            },
            None => true,
        };

        let rounded_cents = if round_up {
            abs_q
                .checked_add(1)
                .ok_or(ReportError::ArithmeticOverflow { figure })?
        } else {
            abs_q
        };

        if rounded_cents == 0 {
            return Ok(Decimal::new(0, 2));
        }

        let signed_cents = if is_neg {
            let neg_i128 = i128::try_from(rounded_cents)
                .map_err(|_| ReportError::ArithmeticOverflow { figure })?;
            -neg_i128
        } else {
            i128::try_from(rounded_cents).map_err(|_| ReportError::ArithmeticOverflow { figure })?
        };

        let mut dec = Decimal::try_from_i128_with_scale(signed_cents, 2)
            .map_err(|_| ReportError::ArithmeticOverflow { figure })?;

        if dec.is_zero() && dec.is_sign_negative() {
            dec = Decimal::new(0, 2);
        }
        Ok(dec)
    }
}

/// Big blinds won over a set of hands (`docs/stats.md` §6.1): the net is summed per big-blind
/// value and divided once per value, then the quotients are added as exact rationals.
#[derive(Debug, Default, Clone)]
pub(crate) struct BbWon {
    net_by_big_blind: BTreeMap<i64, Option<i128>>,
}

impl BbWon {
    /// Adds `net_micros` won at `big_blind_micros` (positive by the schema's `CHECK`). The sum
    /// is an `i128`; an overflow is kept as `None` and reported by `total_rational`.
    pub(crate) fn add(&mut self, big_blind_micros: i64, net_micros: i64) {
        let net = self
            .net_by_big_blind
            .entry(big_blind_micros)
            .or_insert(Some(0));
        *net = net.and_then(|n| n.checked_add(i128::from(net_micros)));
    }

    /// `Σ net / big_blind` per big-blind value, added as an exact rational.
    pub(crate) fn total_rational(&self, figure: &'static str) -> Result<Rational, ReportError> {
        let mut total = Rational::ZERO;
        for (&big_blind_micros, &net_micros) in &self.net_by_big_blind {
            let net_micros = net_micros.ok_or(ReportError::ArithmeticOverflow { figure })?;
            let r = Rational::new(net_micros, big_blind_micros, figure)?;
            total = total.checked_add(r, figure)?;
        }
        Ok(total)
    }

    pub(crate) fn bb_won(&self, figure: &'static str) -> Result<Decimal, ReportError> {
        self.total_rational(figure)?.round_half_even_2dp(figure)
    }

    pub(crate) fn bb_per_100(
        &self,
        hands: i64,
        figure: &'static str,
    ) -> Result<Decimal, ReportError> {
        self.total_rational(figure)?
            .checked_per_100(hands, figure)?
            .round_half_even_2dp(figure)
    }
}

/// Builds a typed [`Report`] for the given kind and options.
///
/// Runs entirely in one read transaction. The first read is the statistics cache
/// version guard (SPEC_v3.7 §2 step 5); if the cache is missing or stale,
/// returns [`ReportError::Storage`] with [`StorageError::StaleDerivedCache`].
///
/// # Errors
/// Returns [`ReportError`] on cache version mismatch, SQLite failure, or arithmetic overflow.
pub fn build_report(
    repo: &SqliteRepository,
    kind: ReportKind,
    options: &ReportOptions,
) -> Result<Report, ReportError> {
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
            for s in stakes {
                let sel = selection(Some(&[*s]));
                let sql = format!("SELECT 1 {} LIMIT 1", sel.sql);
                let mut stmt = tx.prepare_cached(&sql)?;
                let rusqlite_params: Vec<&dyn rusqlite::ToSql> = sel
                    .params
                    .iter()
                    .map(|v| v as &dyn rusqlite::ToSql)
                    .collect();
                let exists: bool = stmt.exists(rusqlite_params.as_slice())?;
                if !exists {
                    unmatched.push(*s);
                }
            }
            unmatched
        }
        None => Vec::new(),
    };

    let stakes_slice = dedup_stakes.as_deref();

    let rows = match kind {
        ReportKind::Results => ReportRows::Results(results::build_results_rows(&tx, stakes_slice)?),
        ReportKind::Positions => {
            ReportRows::Positions(positions::build_positions_rows(&tx, stakes_slice)?)
        }
        ReportKind::Sessions => {
            ReportRows::Sessions(sessions::build_sessions_rows(&tx, stakes_slice)?)
        }
        ReportKind::Stats => ReportRows::Stats(stats::build_stats_rows(&tx, stakes_slice)?),
    };

    tx.finish()?;

    Ok(Report {
        rows,
        unmatched_stakes,
    })
}

/// Formats a [`Report`] as CSV and writes it to `out`.
///
/// # Errors
/// Returns [`ReportError::Io`] on write failure.
pub fn write_csv(report: &Report, out: &mut impl Write) -> Result<(), ReportError> {
    match &report.rows {
        ReportRows::Results(rows) => {
            writeln!(out, "{}", results::RESULTS_HEADER)?;
            for r in rows {
                writeln!(out, "{}", r.to_csv_line())?;
            }
        }
        ReportRows::Positions(rows) => {
            writeln!(out, "{}", positions::POSITIONS_HEADER)?;
            for r in rows {
                writeln!(out, "{}", r.to_csv_line())?;
            }
        }
        ReportRows::Sessions(rows) => {
            writeln!(out, "{}", sessions::SESSIONS_HEADER)?;
            for r in rows {
                writeln!(out, "{}", r.to_csv_line())?;
            }
        }
        ReportRows::Stats(rows) => {
            writeln!(out, "{}", stats::STATS_HEADER)?;
            for r in rows {
                writeln!(out, "{}", r.to_csv_line())?;
            }
        }
    }
    Ok(())
}

/// Generates a CSV report and writes it to `out`.
///
/// Builds a typed report with [`build_report`] and formats it with [`write_csv`].
///
/// # Errors
/// Returns [`ReportError`] on cache version mismatch, SQLite failure, or I/O failure.
pub fn write_report(
    repo: &SqliteRepository,
    kind: ReportKind,
    options: &ReportOptions,
    out: &mut impl Write,
) -> Result<ReportOutcome, ReportError> {
    let report = build_report(repo, kind, options)?;
    write_csv(&report, out)?;
    Ok(ReportOutcome {
        unmatched_stakes: report.unmatched_stakes,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn bb_won_exact_rational_aggregation() {
        // Case 1: (3_000_000, -29_990_000), (6_000_000, 23_590_000) -> -6.06; over 2 hands -> -303.25
        let mut bb1 = BbWon::default();
        bb1.add(3_000_000, -29_990_000);
        bb1.add(6_000_000, 23_590_000);
        assert_eq!(bb1.bb_won("bb_won").unwrap().to_string(), "-6.06");
        assert_eq!(
            bb1.bb_per_100(2, "bb_per_100").unwrap().to_string(),
            "-303.25"
        );

        // Case 2: Negated -> 6.06; over 2 hands -> 303.25
        let mut bb2 = BbWon::default();
        bb2.add(3_000_000, 29_990_000);
        bb2.add(6_000_000, -23_590_000);
        assert_eq!(bb2.bb_won("bb_won").unwrap().to_string(), "6.06");
        assert_eq!(
            bb2.bb_per_100(2, "bb_per_100").unwrap().to_string(),
            "303.25"
        );

        // Case 3: (240_000, -10_000), (480_000, -20_000), (960_000, -40_000) -> -0.12; over 4 hands -> -3.12
        let mut bb3 = BbWon::default();
        bb3.add(240_000, -10_000);
        bb3.add(480_000, -20_000);
        bb3.add(960_000, -40_000);
        assert_eq!(bb3.bb_won("bb_won").unwrap().to_string(), "-0.12");
        assert_eq!(
            bb3.bb_per_100(4, "bb_per_100").unwrap().to_string(),
            "-3.12"
        );

        // Case 4: (100_000, -100) -> -0.001 -> 0.00
        let mut bb4 = BbWon::default();
        bb4.add(100_000, -100);
        assert_eq!(bb4.bb_won("bb_won").unwrap().to_string(), "0.00");

        // Case 5: Overflow path returns ArithmeticOverflow
        let mut bb_ov = BbWon::default();
        bb_ov.add(1_000_000_000_000_000_003, 1);
        bb_ov.add(1_000_000_000_000_009, 1);
        bb_ov.add(1_000_000_000_000_021, 1);
        assert!(matches!(
            bb_ov.bb_won("bb_won"),
            Err(ReportError::ArithmeticOverflow { figure: "bb_won" })
        ));
    }

    #[test]
    fn bb_won_sums_the_nets_of_one_big_blind_without_saturating() {
        // 2 × 5e18 micro-units at a big blind of 1.00: 1e19 / 1e6 = 1e13 bb; over 2 hands 5e14.
        let mut bb = BbWon::default();
        bb.add(1_000_000, 5_000_000_000_000_000_000);
        bb.add(1_000_000, 5_000_000_000_000_000_000);
        assert_eq!(
            bb.bb_won("bb_won").unwrap().to_string(),
            "10000000000000.00"
        );
        assert_eq!(
            bb.bb_per_100(2, "bb_per_100").unwrap().to_string(),
            "500000000000000.00"
        );
    }

    #[test]
    fn bb_won_reduces_its_fraction_across_ten_big_blinds() {
        // Σ net/bb = -61.5 + 91.2 - 78.9 + 61.7 - 22.68 + 17.8 - 23.45 + 33.945 - 9.99666… + 3.93166…
        // = 241/20 = 12.05 exactly; over 10 hands 120.50. Without reducing, the denominator
        // (the product of the ten big blinds) overflows i128.
        let mut bb = BbWon::default();
        for (big_blind, net) in [
            (20_000, -1_230_000),
            (50_000, 4_560_000),
            (100_000, -7_890_000),
            (200_000, 12_340_000),
            (250_000, -5_670_000),
            (500_000, 8_900_000),
            (1_000_000, -23_450_000),
            (2_000_000, 67_890_000),
            (3_000_000, -29_990_000),
            (6_000_000, 23_590_000),
        ] {
            bb.add(big_blind, net);
        }
        assert_eq!(bb.bb_won("bb_won").unwrap().to_string(), "12.05");
        assert_eq!(
            bb.bb_per_100(10, "bb_per_100").unwrap().to_string(),
            "120.50"
        );
    }

    #[test]
    fn bb_won_no_negative_zero() {
        let mut b = BbWon::default();
        b.add(100_000, -100);
        let val = b.bb_won("bb_won").unwrap();
        assert_eq!(val, Decimal::ZERO);
        assert_eq!(val.scale(), 2);
        assert!(!val.is_sign_negative());
    }
}
