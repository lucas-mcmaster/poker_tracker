//! Results report generation (SPEC_v3.7 §6, §7).

use std::collections::BTreeMap;

use rusqlite::Transaction;
use rust_decimal::Decimal;

use super::format::{format_exact_money, format_fixed2, quote_csv_text};
use super::{BbWon, ReportError, Stakes};

pub const RESULTS_HEADER: &str = "currency,stakes,hands,net,net_per_100,bb_won,bb_per_100,rake,fees,blinds,straddles,allin_hands,ev_net,ev_bb_per_100";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultsRow {
    pub currency: String,
    pub stakes: String,
    pub hands: i64,
    pub net: String,
    pub net_per_100: String,
    pub bb_won: String,
    pub bb_per_100: String,
    pub rake: String,
    pub fees: String,
    pub blinds: String,
    pub straddles: String,
    pub allin_hands: i64,
    pub ev_net: String,
    pub ev_bb_per_100: String,
}

impl ResultsRow {
    #[must_use]
    pub fn to_csv_line(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            quote_csv_text(&self.currency),
            quote_csv_text(&self.stakes),
            self.hands,
            self.net,
            self.net_per_100,
            self.bb_won,
            self.bb_per_100,
            self.rake,
            self.fees,
            self.blinds,
            self.straddles,
            self.allin_hands,
            self.ev_net,
            self.ev_bb_per_100
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StakeGroup {
    pub small_blind_micros: i64,
    pub big_blind_micros: i64,
    pub hands: i64,
    pub net_micros: i64,
    pub rake_micros: i64,
    pub fees_micros: i64,
    pub blinds_micros: i64,
    pub straddles_micros: i64,
    pub allin_hands: i64,
    pub ev_net_micros: i64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResultTotals {
    pub hands: i64,
    pub net_micros: i64,
    pub rake_micros: i64,
    pub fees_micros: i64,
    pub blinds_micros: i64,
    pub straddles_micros: i64,
    pub allin_hands: i64,
    pub ev_net_micros: i64,
}

pub(crate) fn aggregate_result_totals(groups: &[StakeGroup]) -> Result<ResultTotals, ReportError> {
    let mut totals = ResultTotals::default();
    for g in groups {
        totals.hands = totals
            .hands
            .checked_add(g.hands)
            .ok_or(ReportError::ArithmeticOverflow { figure: "hands" })?;
        totals.net_micros = totals
            .net_micros
            .checked_add(g.net_micros)
            .ok_or(ReportError::ArithmeticOverflow { figure: "net" })?;
        totals.rake_micros = totals
            .rake_micros
            .checked_add(g.rake_micros)
            .ok_or(ReportError::ArithmeticOverflow { figure: "rake" })?;
        totals.fees_micros = totals
            .fees_micros
            .checked_add(g.fees_micros)
            .ok_or(ReportError::ArithmeticOverflow { figure: "fees" })?;
        totals.blinds_micros = totals
            .blinds_micros
            .checked_add(g.blinds_micros)
            .ok_or(ReportError::ArithmeticOverflow { figure: "blinds" })?;
        totals.straddles_micros = totals
            .straddles_micros
            .checked_add(g.straddles_micros)
            .ok_or(ReportError::ArithmeticOverflow {
                figure: "straddles",
            })?;
        totals.allin_hands = totals.allin_hands.checked_add(g.allin_hands).ok_or(
            ReportError::ArithmeticOverflow {
                figure: "allin_hands",
            },
        )?;
        totals.ev_net_micros = totals
            .ev_net_micros
            .checked_add(g.ev_net_micros)
            .ok_or(ReportError::ArithmeticOverflow { figure: "ev_net" })?;
    }
    Ok(totals)
}

pub(crate) fn build_results_rows(
    tx: &Transaction,
    stakes_filter: Option<&[Stakes]>,
) -> Result<Vec<ResultsRow>, ReportError> {
    let sel = super::selection(stakes_filter);
    let sql = format!(
        "SELECT \
            h.currency, \
            h.small_blind_micros, \
            h.big_blind_micros, \
            COUNT(*), \
            SUM(hp.net_micros), \
            SUM(hps.rake_attributed_micros), \
            SUM(hps.fees_attributed_micros), \
            SUM(hps.blinds_posted_micros), \
            SUM(hps.straddles_posted_micros), \
            SUM(hps.ev_qualified), \
            SUM(hps.ev_net_micros) \
         {} \
         GROUP BY h.currency, h.big_blind_micros, h.small_blind_micros \
         ORDER BY h.currency ASC, h.big_blind_micros ASC, h.small_blind_micros ASC",
        sel.sql
    );

    let mut stmt = tx.prepare(&sql)?;
    let rusqlite_params: Vec<&dyn rusqlite::ToSql> = sel
        .params
        .iter()
        .map(|v| v as &dyn rusqlite::ToSql)
        .collect();

    let mut rows_by_currency: BTreeMap<String, Vec<StakeGroup>> = BTreeMap::new();
    let mut query_rows = stmt.query(rusqlite_params.as_slice())?;

    while let Some(row) = query_rows.next()? {
        let currency: String = row.get(0)?;
        let sb: i64 = row.get(1)?;
        let bb: i64 = row.get(2)?;
        let hands: i64 = row.get(3)?;
        let net: i64 = row.get(4)?;
        let rake: i64 = row.get(5)?;
        let fees: i64 = row.get(6)?;
        let blinds: i64 = row.get(7)?;
        let straddles: i64 = row.get(8)?;
        let allin_hands: i64 = row.get(9)?;
        let ev_net_micros: i64 = row.get(10)?;

        rows_by_currency
            .entry(currency)
            .or_default()
            .push(StakeGroup {
                small_blind_micros: sb,
                big_blind_micros: bb,
                hands,
                net_micros: net,
                rake_micros: rake,
                fees_micros: fees,
                blinds_micros: blinds,
                straddles_micros: straddles,
                allin_hands,
                ev_net_micros,
            });
    }

    let mut result_rows = Vec::new();

    for (currency, groups) in rows_by_currency {
        let totals = aggregate_result_totals(&groups)?;
        let mut total_bb_won = BbWon::default();
        let mut total_ev_bb_won = BbWon::default();

        for g in &groups {
            let net_dec = Decimal::new(g.net_micros, 6);
            let net_per_100_dec = (net_dec / Decimal::from(g.hands)) * Decimal::from(100);

            let mut row_bb_won = BbWon::default();
            row_bb_won.add(g.big_blind_micros, g.net_micros);
            let bb_won_str = row_bb_won.format_bb_won("bb_won")?;
            let bb_per_100_str = row_bb_won.format_bb_per_100(g.hands, "bb_per_100")?;
            total_bb_won.add(g.big_blind_micros, g.net_micros);

            let mut row_ev_bb_won = BbWon::default();
            row_ev_bb_won.add(g.big_blind_micros, g.ev_net_micros);
            let ev_bb_per_100_str = row_ev_bb_won.format_bb_per_100(g.hands, "ev_bb_per_100")?;
            total_ev_bb_won.add(g.big_blind_micros, g.ev_net_micros);

            let stakes_str = Stakes {
                small_blind_micros: g.small_blind_micros,
                big_blind_micros: g.big_blind_micros,
            }
            .to_string();

            result_rows.push(ResultsRow {
                currency: currency.clone(),
                stakes: stakes_str,
                hands: g.hands,
                net: format_exact_money(g.net_micros),
                net_per_100: format_fixed2(net_per_100_dec),
                bb_won: bb_won_str,
                bb_per_100: bb_per_100_str,
                rake: format_exact_money(g.rake_micros),
                fees: format_exact_money(g.fees_micros),
                blinds: format_exact_money(g.blinds_micros),
                straddles: format_exact_money(g.straddles_micros),
                allin_hands: g.allin_hands,
                ev_net: format_exact_money(g.ev_net_micros),
                ev_bb_per_100: ev_bb_per_100_str,
            });
        }

        if groups.len() > 1 && totals.hands > 0 {
            let net_dec = Decimal::new(totals.net_micros, 6);
            let net_per_100_dec = (net_dec / Decimal::from(totals.hands)) * Decimal::from(100);
            let total_bb_won_str = total_bb_won.format_bb_won("bb_won")?;
            let bb_per_100_str = total_bb_won.format_bb_per_100(totals.hands, "bb_per_100")?;
            let ev_bb_per_100_str =
                total_ev_bb_won.format_bb_per_100(totals.hands, "ev_bb_per_100")?;

            result_rows.push(ResultsRow {
                currency,
                stakes: "ALL".to_string(),
                hands: totals.hands,
                net: format_exact_money(totals.net_micros),
                net_per_100: format_fixed2(net_per_100_dec),
                bb_won: total_bb_won_str,
                bb_per_100: bb_per_100_str,
                rake: format_exact_money(totals.rake_micros),
                fees: format_exact_money(totals.fees_micros),
                blinds: format_exact_money(totals.blinds_micros),
                straddles: format_exact_money(totals.straddles_micros),
                allin_hands: totals.allin_hands,
                ev_net: format_exact_money(totals.ev_net_micros),
                ev_bb_per_100: ev_bb_per_100_str,
            });
        }
    }

    Ok(result_rows)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn results_total_net_overflow_returns_error() {
        let g1 = StakeGroup {
            small_blind_micros: 50_000,
            big_blind_micros: 100_000,
            hands: 1,
            net_micros: i64::MAX,
            rake_micros: 0,
            fees_micros: 0,
            blinds_micros: 0,
            straddles_micros: 0,
            allin_hands: 0,
            ev_net_micros: 0,
        };
        let g2 = StakeGroup {
            small_blind_micros: 50_000,
            big_blind_micros: 100_000,
            hands: 1,
            net_micros: 1,
            rake_micros: 0,
            fees_micros: 0,
            blinds_micros: 0,
            straddles_micros: 0,
            allin_hands: 0,
            ev_net_micros: 0,
        };
        let err = aggregate_result_totals(&[g1, g2]).unwrap_err();
        assert!(matches!(
            err,
            ReportError::ArithmeticOverflow { figure: "net" }
        ));
    }

    fn field_of_totals<'a>(t: &'a mut ResultTotals, figure: &str) -> &'a mut i64 {
        match figure {
            "hands" => &mut t.hands,
            "net" => &mut t.net_micros,
            "rake" => &mut t.rake_micros,
            "fees" => &mut t.fees_micros,
            "blinds" => &mut t.blinds_micros,
            "straddles" => &mut t.straddles_micros,
            "allin_hands" => &mut t.allin_hands,
            "ev_net" => &mut t.ev_net_micros,
            _ => panic!("unknown figure {figure}"),
        }
    }

    /// Every total names the CSV column it feeds, and sums its own field.
    #[test]
    fn every_results_total_overflow_names_its_column() {
        let zero = StakeGroup {
            small_blind_micros: 50_000,
            big_blind_micros: 100_000,
            hands: 0,
            net_micros: 0,
            rake_micros: 0,
            fees_micros: 0,
            blinds_micros: 0,
            straddles_micros: 0,
            allin_hands: 0,
            ev_net_micros: 0,
        };
        type Field = fn(&mut StakeGroup) -> &mut i64;
        let fields: [(&str, Field); 8] = [
            ("hands", |g| &mut g.hands),
            ("net", |g| &mut g.net_micros),
            ("rake", |g| &mut g.rake_micros),
            ("fees", |g| &mut g.fees_micros),
            ("blinds", |g| &mut g.blinds_micros),
            ("straddles", |g| &mut g.straddles_micros),
            ("allin_hands", |g| &mut g.allin_hands),
            ("ev_net", |g| &mut g.ev_net_micros),
        ];
        for (figure, field) in fields {
            let (mut g1, mut g2) = (zero, zero);
            *field(&mut g1) = i64::MAX;
            *field(&mut g2) = 1;
            let err = aggregate_result_totals(&[g1, g2]).unwrap_err();
            assert!(
                matches!(err, ReportError::ArithmeticOverflow { figure: f } if f == figure),
                "{figure}: got {err:?}"
            );
            assert_eq!(aggregate_result_totals(&[g1]).unwrap(), {
                let mut t = ResultTotals::default();
                *field_of_totals(&mut t, figure) = i64::MAX;
                t
            });
        }
    }
}
