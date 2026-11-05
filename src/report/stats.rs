//! Statistics report generation (SPEC_v3.7 §6).

use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;

use rusqlite::Transaction;
use rust_decimal::Decimal;
use serde::Serialize;

use super::format::{quote_csv_text, round_factor, round_pct};
use super::{ReportError, Stakes};
use crate::domain::money::CurrencyCode;
use crate::storage::StorageError;

pub const STATS_HEADER: &str = "currency,stat,numerator,denominator,value";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatsRow {
    pub currency: CurrencyCode,
    pub stat: &'static str,
    pub numerator: i64,
    pub denominator: i64,
    pub value: Option<Decimal>,
}

impl StatsRow {
    #[must_use]
    pub fn to_csv_line(&self) -> String {
        let val_str = match self.value {
            Some(v) => v.to_string(),
            None => String::new(),
        };
        format!(
            "{},{},{},{},{}",
            quote_csv_text(self.currency.as_str()),
            quote_csv_text(self.stat),
            self.numerator,
            self.denominator,
            val_str,
        )
    }
}

/// How a stat's denominator is formed from the column sums.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Denominator {
    /// The sum of one column.
    Column(&'static str),
    /// The sum of the first column minus the sum of the second (VPIP: hands minus walks).
    Difference(&'static str, &'static str),
}

use Denominator::{Column, Difference};

/// `(stat, numerator column, denominator)` in `docs/stats.md` §5 order.
pub(crate) const STAT_TABLE: &[(&str, &str, Denominator)] = &[
    ("vpip", "p_vpip", Difference("hands", "p_walk")),
    ("pfr", "p_pfr", Column("p_pfr_opp")),
    ("saw_flop", "f_saw", Column("hands")),
    ("wtsd", "sd_went", Column("f_saw")),
    ("wsd", "sd_won", Column("sd_went")),
    ("wwsf", "f_saw_won", Column("f_saw")),
    ("win", "won_hand", Column("hands")),
    ("rfi", "p_rfi", Column("p_rfi_opp")),
    ("three_bet_pf", "p_3bet", Column("p_3bet_opp")),
    (
        "fold_to_three_bet_pf",
        "p_3bet_fold",
        Column("p_3bet_faced"),
    ),
    ("call_three_bet_pf", "p_3bet_call", Column("p_3bet_faced")),
    ("four_bet_pf", "p_4bet", Column("p_4bet_opp")),
    ("fold_to_four_bet_pf", "p_4bet_fold", Column("p_4bet_faced")),
    ("call_four_bet_pf", "p_4bet_call", Column("p_4bet_faced")),
    ("cold_call_pf", "p_cc", Column("p_cc_opp")),
    ("att_to_steal", "p_steal", Column("p_steal_opp")),
    (
        "fold_sb_to_steal",
        "p_sb_steal_fold",
        Column("p_sb_steal_faced"),
    ),
    (
        "raise_sb_steal",
        "p_sb_steal_raise",
        Column("p_sb_steal_faced"),
    ),
    (
        "fold_bb_to_steal",
        "p_bb_steal_fold",
        Column("p_bb_steal_faced"),
    ),
    (
        "raise_bb_steal",
        "p_bb_steal_raise",
        Column("p_bb_steal_faced"),
    ),
    ("cbet_flop", "f_cbet", Column("f_cbet_opp")),
    ("fold_to_cbet_flop", "f_cbet_fold", Column("f_cbet_faced")),
    ("call_cbet_flop", "f_cbet_call", Column("f_cbet_faced")),
    ("raise_cbet_flop", "f_cbet_raise", Column("f_cbet_faced")),
    ("cbet_turn", "t_cbet", Column("t_cbet_opp")),
    ("fold_to_cbet_turn", "t_cbet_fold", Column("t_cbet_faced")),
    ("call_cbet_turn", "t_cbet_call", Column("t_cbet_faced")),
    ("raise_cbet_turn", "t_cbet_raise", Column("t_cbet_faced")),
    ("check_raise_flop", "f_xr", Column("f_xr_opp")),
];

/// One street's action-count columns for AF and AFq (`docs/stats.md` §5).
pub(crate) struct AggressionStreet {
    pub af: &'static str,
    pub afq: &'static str,
    pub bets: &'static str,
    pub raises: &'static str,
    pub calls: &'static str,
    pub folds: &'static str,
    /// Counted in `af_total` and `afq_total` (flop, turn and river).
    pub postflop: bool,
}

/// The streets of AF and AFq, in report order; `af_total` and `afq_total` follow them.
pub(crate) const AGGRESSION_STREETS: &[AggressionStreet] = &[
    AggressionStreet {
        af: "af_pf",
        afq: "afq_pf",
        bets: "p_bets",
        raises: "p_raises",
        calls: "p_calls",
        folds: "p_folds",
        postflop: false,
    },
    AggressionStreet {
        af: "af_flop",
        afq: "afq_flop",
        bets: "f_bets",
        raises: "f_raises",
        calls: "f_calls",
        folds: "f_folds",
        postflop: true,
    },
    AggressionStreet {
        af: "af_turn",
        afq: "afq_turn",
        bets: "t_bets",
        raises: "t_raises",
        calls: "t_calls",
        folds: "t_folds",
        postflop: true,
    },
    AggressionStreet {
        af: "af_river",
        afq: "afq_river",
        bets: "r_bets",
        raises: "r_raises",
        calls: "r_calls",
        folds: "r_folds",
        postflop: true,
    },
];

type StatRatioRow = (&'static str, i64, i64);
type AggressionStats = (Vec<StatRatioRow>, Vec<StatRatioRow>);

/// The sum of `col`. Every name the report uses is a `Flag` or `Count` column, and the map holds
/// every such column (`stat_table_columns_exist_and_are_flags_or_counts`).
fn sum_of(counts: &HashMap<&str, i64>, col: &str) -> i64 {
    counts.get(col).copied().unwrap_or(0)
}

fn overflow(figure: &'static str) -> ReportError {
    ReportError::ArithmeticOverflow { figure }
}

/// AF and AFq per street, then over flop, turn and river. An overflow names the AF figure for
/// `bets + raises` and the total calls, and the AFq figure for the rest.
pub(crate) fn compute_aggression_stats(
    counts: &HashMap<&str, i64>,
) -> Result<AggressionStats, ReportError> {
    let get = |col: &str| sum_of(counts, col);

    let mut aggression = Vec::with_capacity(AGGRESSION_STREETS.len());
    for street in AGGRESSION_STREETS {
        let agg = get(street.bets)
            .checked_add(get(street.raises))
            .ok_or(overflow(street.af))?;
        aggression.push(agg);
    }

    let mut postflop_agg: i64 = 0;
    let mut postflop_calls: i64 = 0;
    for (street, &agg) in AGGRESSION_STREETS.iter().zip(&aggression) {
        if street.postflop {
            postflop_agg = postflop_agg.checked_add(agg).ok_or(overflow("af_total"))?;
            postflop_calls = postflop_calls
                .checked_add(get(street.calls))
                .ok_or(overflow("af_total"))?;
        }
    }

    let mut af_definitions: Vec<StatRatioRow> = AGGRESSION_STREETS
        .iter()
        .zip(&aggression)
        .map(|(street, &agg)| (street.af, agg, get(street.calls)))
        .collect();
    af_definitions.push(("af_total", postflop_agg, postflop_calls));

    let mut afq_definitions = Vec::with_capacity(AGGRESSION_STREETS.len() + 1);
    for (street, &agg) in AGGRESSION_STREETS.iter().zip(&aggression) {
        let den = agg
            .checked_add(get(street.calls))
            .and_then(|v| v.checked_add(get(street.folds)))
            .ok_or(overflow(street.afq))?;
        afq_definitions.push((street.afq, agg, den));
    }

    let mut postflop_folds: i64 = 0;
    for street in AGGRESSION_STREETS.iter().filter(|s| s.postflop) {
        postflop_folds = postflop_folds
            .checked_add(get(street.folds))
            .ok_or(overflow("afq_total"))?;
    }
    let total_afq_den = postflop_agg
        .checked_add(postflop_calls)
        .and_then(|v| v.checked_add(postflop_folds))
        .ok_or(overflow("afq_total"))?;
    afq_definitions.push(("afq_total", postflop_agg, total_afq_den));

    Ok((af_definitions, afq_definitions))
}

pub(crate) fn build_stats_rows(
    tx: &Transaction,
    stakes_filter: Option<&[Stakes]>,
) -> Result<Vec<StatsRow>, ReportError> {
    let sel = super::selection(stakes_filter);

    let stat_cols: Vec<&'static str> = crate::stats::COLUMN_SPECS
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                crate::stats::ColumnKind::Flag | crate::stats::ColumnKind::Count
            )
        })
        .map(|s| s.name)
        .collect();

    let sum_parts: Vec<String> = stat_cols
        .iter()
        .map(|col| format!("SUM(hps.{col}) AS {col}"))
        .collect();
    let sums_sql = sum_parts.join(", ");

    let sql = format!(
        "SELECT h.currency, {sums_sql} \
         {} \
         GROUP BY h.currency \
         ORDER BY h.currency ASC",
        sel.sql
    );

    let mut stmt = tx.prepare(&sql)?;
    let rusqlite_params: Vec<&dyn rusqlite::ToSql> = sel
        .params
        .iter()
        .map(|v| v as &dyn rusqlite::ToSql)
        .collect();

    let mut by_currency: BTreeMap<String, HashMap<&'static str, i64>> = BTreeMap::new();
    let mut query_rows = stmt.query(rusqlite_params.as_slice())?;

    while let Some(row) = query_rows.next()? {
        let currency: String = row.get(0)?;
        let mut counts = HashMap::with_capacity(stat_cols.len());
        for &col in &stat_cols {
            counts.insert(col, row.get::<_, i64>(col)?);
        }
        by_currency.insert(currency, counts);
    }

    let mut rows = Vec::new();

    for (currency, counts) in by_currency {
        let currency_code = CurrencyCode::from_str(&currency).map_err(|_| {
            ReportError::Storage(StorageError::CorruptRow {
                table: "hands",
                detail: format!("invalid currency '{currency}'"),
            })
        })?;
        for &(stat, num_col, den) in STAT_TABLE {
            let num = sum_of(&counts, num_col);
            let den = match den {
                Column(col) => sum_of(&counts, col),
                Difference(minuend, subtrahend) => sum_of(&counts, minuend)
                    .checked_sub(sum_of(&counts, subtrahend))
                    .ok_or(overflow(stat))?,
            };

            rows.push(StatsRow {
                currency: currency_code,
                stat,
                numerator: num,
                denominator: den,
                value: round_pct(num, den),
            });
        }

        let (af_definitions, afq_definitions) = compute_aggression_stats(&counts)?;

        for (name, num, den) in af_definitions {
            rows.push(StatsRow {
                currency: currency_code,
                stat: name,
                numerator: num,
                denominator: den,
                value: round_factor(num, den),
            });
        }

        for (name, num, den) in afq_definitions {
            rows.push(StatsRow {
                currency: currency_code,
                stat: name,
                numerator: num,
                denominator: den,
                value: round_pct(num, den),
            });
        }
    }

    Ok(rows)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::stats::{ColumnKind, column_spec};

    #[test]
    fn stats_af_pf_overflow_returns_error() {
        let mut counts = HashMap::new();
        counts.insert("p_bets", i64::MAX);
        counts.insert("p_raises", 1);
        let err = compute_aggression_stats(&counts).unwrap_err();
        assert!(matches!(
            err,
            ReportError::ArithmeticOverflow { figure: "af_pf" }
        ));
    }

    /// Each pair of columns whose sums overflow names the figure it feeds.
    #[test]
    fn every_aggression_overflow_names_its_figure() {
        let cases: &[(&str, &str, &str)] = &[
            ("p_bets", "p_raises", "af_pf"),
            ("f_bets", "f_raises", "af_flop"),
            ("t_bets", "t_raises", "af_turn"),
            ("r_bets", "r_raises", "af_river"),
            ("f_bets", "t_bets", "af_total"),
            ("f_calls", "r_calls", "af_total"),
            ("p_calls", "p_folds", "afq_pf"),
            ("f_calls", "f_folds", "afq_flop"),
            ("t_calls", "t_folds", "afq_turn"),
            ("r_calls", "r_folds", "afq_river"),
            ("f_folds", "r_folds", "afq_total"),
            ("f_calls", "t_folds", "afq_total"),
        ];
        for &(big, one, figure) in cases {
            let counts = HashMap::from([(big, i64::MAX), (one, 1)]);
            let err = compute_aggression_stats(&counts).unwrap_err();
            assert!(
                matches!(err, ReportError::ArithmeticOverflow { figure: f } if f == figure),
                "{big} + {one}: expected {figure}, got {err:?}"
            );
        }
    }

    /// Every column of a street feeds that street's figures, and only flop, turn and river feed
    /// the totals: distinct powers of two make every sum identify its inputs.
    #[test]
    fn aggression_figures_read_their_own_columns() {
        let mut counts = HashMap::new();
        let mut bit = 1_i64;
        for street in AGGRESSION_STREETS {
            for col in [street.bets, street.raises, street.calls, street.folds] {
                counts.insert(col, bit);
                bit <<= 1;
            }
        }
        let (af, afq) = compute_aggression_stats(&counts).unwrap();
        // Street i: bets 1, raises 2, calls 4 and folds 8, times 16^i.
        let street = |i: u32| {
            let base = 1_i64 << (4 * i);
            (base + 2 * base, 4 * base, 8 * base)
        };
        let mut expected_af = Vec::new();
        let mut expected_afq = Vec::new();
        for (i, s) in AGGRESSION_STREETS.iter().enumerate() {
            let (agg, calls, folds) = street(u32::try_from(i).unwrap());
            expected_af.push((s.af, agg, calls));
            expected_afq.push((s.afq, agg, agg + calls + folds));
        }
        let post = |k: usize| -> i64 {
            (1..4)
                .map(|i| {
                    let t = street(i);
                    [t.0, t.1, t.2][k]
                })
                .sum()
        };
        expected_af.push(("af_total", post(0), post(1)));
        expected_afq.push(("afq_total", post(0), post(0) + post(1) + post(2)));
        assert_eq!(af, expected_af);
        assert_eq!(afq, expected_afq);
    }

    /// Every name the report reads is a `Flag` or `Count` column, so a misspelled name fails here
    /// instead of reading as 0.
    #[test]
    fn stat_table_columns_exist_and_are_flags_or_counts() {
        let mut names: Vec<(&str, &str)> = Vec::new();
        for &(stat, num, den) in STAT_TABLE {
            names.push((stat, num));
            match den {
                Denominator::Column(col) => names.push((stat, col)),
                Denominator::Difference(a, b) => {
                    names.push((stat, a));
                    names.push((stat, b));
                }
            }
        }
        for s in AGGRESSION_STREETS {
            for col in [s.bets, s.raises, s.calls, s.folds] {
                names.push((s.af, col));
            }
        }
        assert_eq!(names.len(), 29 * 2 + 1 + 4 * 4);
        for (stat, col) in names {
            let spec =
                column_spec(col).unwrap_or_else(|| panic!("stat {stat}: unknown column {col}"));
            assert!(
                matches!(spec.kind, ColumnKind::Flag | ColumnKind::Count),
                "stat {stat}: column {col} has kind {:?}",
                spec.kind
            );
        }
        assert!(column_spec("nonexistent_stat").is_none());
    }

    #[test]
    fn vpip_divides_by_hands_minus_walks() {
        assert_eq!(
            STAT_TABLE[0],
            ("vpip", "p_vpip", Denominator::Difference("hands", "p_walk"))
        );
        assert!(
            STAT_TABLE[1..]
                .iter()
                .all(|(_, _, d)| matches!(d, Denominator::Column(_)))
        );
    }
}
