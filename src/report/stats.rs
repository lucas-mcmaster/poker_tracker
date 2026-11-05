//! Statistics report generation (SPEC_v3.7 §6).

use std::collections::BTreeMap;

use rusqlite::Transaction;

use super::format::{format_factor, format_pct, quote_csv_text};
use super::{ReportError, Stakes};

pub const STATS_HEADER: &str = "currency,stat,numerator,denominator,value";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsRow {
    pub currency: String,
    pub stat: String,
    pub numerator: i64,
    pub denominator: i64,
    pub value: String,
}

impl StatsRow {
    #[must_use]
    pub fn to_csv_line(&self) -> String {
        format!(
            "{},{},{},{},{}",
            quote_csv_text(&self.currency),
            quote_csv_text(&self.stat),
            self.numerator,
            self.denominator,
            self.value
        )
    }
}

use std::collections::HashMap;

pub(crate) const STAT_TABLE: &[(&str, &str, &str)] = &[
    ("vpip", "p_vpip", "hands"),
    ("pfr", "p_pfr", "p_pfr_opp"),
    ("saw_flop", "f_saw", "hands"),
    ("wtsd", "sd_went", "f_saw"),
    ("wsd", "sd_won", "sd_went"),
    ("wwsf", "f_saw_won", "f_saw"),
    ("win", "won_hand", "hands"),
    ("rfi", "p_rfi", "p_rfi_opp"),
    ("three_bet_pf", "p_3bet", "p_3bet_opp"),
    ("fold_to_three_bet_pf", "p_3bet_fold", "p_3bet_faced"),
    ("call_three_bet_pf", "p_3bet_call", "p_3bet_faced"),
    ("four_bet_pf", "p_4bet", "p_4bet_opp"),
    ("fold_to_four_bet_pf", "p_4bet_fold", "p_4bet_faced"),
    ("call_four_bet_pf", "p_4bet_call", "p_4bet_faced"),
    ("cold_call_pf", "p_cc", "p_cc_opp"),
    ("att_to_steal", "p_steal", "p_steal_opp"),
    ("fold_sb_to_steal", "p_sb_steal_fold", "p_sb_steal_faced"),
    ("raise_sb_steal", "p_sb_steal_raise", "p_sb_steal_faced"),
    ("fold_bb_to_steal", "p_bb_steal_fold", "p_bb_steal_faced"),
    ("raise_bb_steal", "p_bb_steal_raise", "p_bb_steal_faced"),
    ("cbet_flop", "f_cbet", "f_cbet_opp"),
    ("fold_to_cbet_flop", "f_cbet_fold", "f_cbet_faced"),
    ("call_cbet_flop", "f_cbet_call", "f_cbet_faced"),
    ("raise_cbet_flop", "f_cbet_raise", "f_cbet_faced"),
    ("cbet_turn", "t_cbet", "t_cbet_opp"),
    ("fold_to_cbet_turn", "t_cbet_fold", "t_cbet_faced"),
    ("call_cbet_turn", "t_cbet_call", "t_cbet_faced"),
    ("raise_cbet_turn", "t_cbet_raise", "t_cbet_faced"),
    ("check_raise_flop", "f_xr", "f_xr_opp"),
];

type StatRatioRow = (&'static str, i64, i64);
type AggressionStats = (Vec<StatRatioRow>, Vec<StatRatioRow>);

pub(crate) fn compute_aggression_stats(
    counts: &HashMap<&str, i64>,
) -> Result<AggressionStats, ReportError> {
    let get = |col: &str| counts.get(col).copied().unwrap_or(0);

    let p_agg = get("p_bets")
        .checked_add(get("p_raises"))
        .ok_or(ReportError::ArithmeticOverflow { figure: "af_pf" })?;
    let f_agg = get("f_bets")
        .checked_add(get("f_raises"))
        .ok_or(ReportError::ArithmeticOverflow { figure: "af_flop" })?;
    let t_agg = get("t_bets")
        .checked_add(get("t_raises"))
        .ok_or(ReportError::ArithmeticOverflow { figure: "af_turn" })?;
    let r_agg = get("r_bets")
        .checked_add(get("r_raises"))
        .ok_or(ReportError::ArithmeticOverflow { figure: "af_river" })?;

    let postflop_agg = f_agg
        .checked_add(t_agg)
        .ok_or(ReportError::ArithmeticOverflow { figure: "af_total" })?
        .checked_add(r_agg)
        .ok_or(ReportError::ArithmeticOverflow { figure: "af_total" })?;

    let postflop_calls = get("f_calls")
        .checked_add(get("t_calls"))
        .ok_or(ReportError::ArithmeticOverflow { figure: "af_total" })?
        .checked_add(get("r_calls"))
        .ok_or(ReportError::ArithmeticOverflow { figure: "af_total" })?;

    let af_definitions = vec![
        ("af_pf", p_agg, get("p_calls")),
        ("af_flop", f_agg, get("f_calls")),
        ("af_turn", t_agg, get("t_calls")),
        ("af_river", r_agg, get("r_calls")),
        ("af_total", postflop_agg, postflop_calls),
    ];

    let p_afq_den = p_agg
        .checked_add(get("p_calls"))
        .ok_or(ReportError::ArithmeticOverflow { figure: "afq_pf" })?
        .checked_add(get("p_folds"))
        .ok_or(ReportError::ArithmeticOverflow { figure: "afq_pf" })?;

    let f_afq_den = f_agg
        .checked_add(get("f_calls"))
        .ok_or(ReportError::ArithmeticOverflow { figure: "afq_flop" })?
        .checked_add(get("f_folds"))
        .ok_or(ReportError::ArithmeticOverflow { figure: "afq_flop" })?;

    let t_afq_den = t_agg
        .checked_add(get("t_calls"))
        .ok_or(ReportError::ArithmeticOverflow { figure: "afq_turn" })?
        .checked_add(get("t_folds"))
        .ok_or(ReportError::ArithmeticOverflow { figure: "afq_turn" })?;

    let r_afq_den = r_agg
        .checked_add(get("r_calls"))
        .ok_or(ReportError::ArithmeticOverflow {
            figure: "afq_river",
        })?
        .checked_add(get("r_folds"))
        .ok_or(ReportError::ArithmeticOverflow {
            figure: "afq_river",
        })?;

    let postflop_folds = get("f_folds")
        .checked_add(get("t_folds"))
        .ok_or(ReportError::ArithmeticOverflow {
            figure: "afq_total",
        })?
        .checked_add(get("r_folds"))
        .ok_or(ReportError::ArithmeticOverflow {
            figure: "afq_total",
        })?;

    let total_afq_den = postflop_agg
        .checked_add(postflop_calls)
        .ok_or(ReportError::ArithmeticOverflow {
            figure: "afq_total",
        })?
        .checked_add(postflop_folds)
        .ok_or(ReportError::ArithmeticOverflow {
            figure: "afq_total",
        })?;

    let afq_definitions = vec![
        ("afq_pf", p_agg, p_afq_den),
        ("afq_flop", f_agg, f_afq_den),
        ("afq_turn", t_agg, t_afq_den),
        ("afq_river", r_agg, r_afq_den),
        ("afq_total", postflop_agg, total_afq_den),
    ];

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
            let val: Option<i64> = row.get(col)?;
            counts.insert(col, val.unwrap_or(0));
        }
        by_currency.insert(currency, counts);
    }

    let mut rows = Vec::new();

    for (currency, counts) in by_currency {
        for &(stat, num_col, den_col) in STAT_TABLE {
            let num = counts.get(num_col).copied().unwrap_or(0);
            let den = if stat == "vpip" {
                let hands = counts.get("hands").copied().unwrap_or(0);
                let p_walk = counts.get("p_walk").copied().unwrap_or(0);
                hands
                    .checked_sub(p_walk)
                    .ok_or(ReportError::ArithmeticOverflow { figure: "vpip" })?
            } else {
                counts.get(den_col).copied().unwrap_or(0)
            };

            rows.push(StatsRow {
                currency: currency.clone(),
                stat: stat.to_string(),
                numerator: num,
                denominator: den,
                value: format_pct(num, den),
            });
        }

        let (af_definitions, afq_definitions) = compute_aggression_stats(&counts)?;

        for (name, num, den) in af_definitions {
            rows.push(StatsRow {
                currency: currency.clone(),
                stat: name.to_string(),
                numerator: num,
                denominator: den,
                value: format_factor(num, den),
            });
        }

        for (name, num, den) in afq_definitions {
            rows.push(StatsRow {
                currency: currency.clone(),
                stat: name.to_string(),
                numerator: num,
                denominator: den,
                value: format_pct(num, den),
            });
        }
    }

    Ok(rows)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

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

    #[test]
    fn stat_table_columns_exist_and_are_flags_or_counts() {
        use crate::stats::{ColumnKind, column_spec};

        for &(stat, num_col, den_col) in STAT_TABLE {
            let num_spec = column_spec(num_col)
                .unwrap_or_else(|| panic!("stat {stat}: unknown numerator column {num_col}"));
            assert!(
                matches!(num_spec.kind, ColumnKind::Flag | ColumnKind::Count),
                "stat {stat}: numerator column {num_col} has unexpected kind {:?}",
                num_spec.kind
            );
            let den_spec = column_spec(den_col)
                .unwrap_or_else(|| panic!("stat {stat}: unknown denominator column {den_col}"));
            assert!(
                matches!(den_spec.kind, ColumnKind::Flag | ColumnKind::Count),
                "stat {stat}: denominator column {den_col} has unexpected kind {:?}",
                den_spec.kind
            );
        }

        let p_walk_spec = column_spec("p_walk").expect("p_walk exists");
        assert!(matches!(
            p_walk_spec.kind,
            ColumnKind::Flag | ColumnKind::Count
        ));

        for col in [
            "p_bets", "p_raises", "p_calls", "p_folds", "f_bets", "f_raises", "f_calls", "f_folds",
            "t_bets", "t_raises", "t_calls", "t_folds", "r_bets", "r_raises", "r_calls", "r_folds",
        ] {
            let spec =
                column_spec(col).unwrap_or_else(|| panic!("missing action count column {col}"));
            assert!(matches!(spec.kind, ColumnKind::Flag | ColumnKind::Count));
        }

        assert!(column_spec("nonexistent_stat").is_none());
    }
}
