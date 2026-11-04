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

struct StakeGroup {
    small_blind_micros: i64,
    big_blind_micros: i64,
    hands: i64,
    net_micros: i64,
    rake_micros: i64,
    fees_micros: i64,
    blinds_micros: i64,
    straddles_micros: i64,
    allin_hands: i64,
    ev_net_micros: i64,
}

pub(crate) fn build_results_rows(
    tx: &Transaction,
    stakes_filter: Option<&[Stakes]>,
) -> Result<Vec<ResultsRow>, ReportError> {
    let (clause, params) = super::stakes_clause(stakes_filter);
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
         FROM hands h \
         JOIN hand_players hp ON hp.hand_id = h.id AND hp.is_hero = 1 \
         JOIN hand_player_stats hps ON hps.hand_id = hp.hand_id AND hps.player_id = hp.player_id \
         WHERE 1 = 1 {clause} \
         GROUP BY h.currency, h.big_blind_micros, h.small_blind_micros \
         ORDER BY h.currency ASC, h.big_blind_micros ASC, h.small_blind_micros ASC"
    );

    let mut stmt = tx.prepare(&sql)?;
    let rusqlite_params: Vec<&dyn rusqlite::ToSql> =
        params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

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
        let mut total_hands: i64 = 0;
        let mut total_net: i64 = 0;
        let mut total_rake: i64 = 0;
        let mut total_fees: i64 = 0;
        let mut total_blinds: i64 = 0;
        let mut total_straddles: i64 = 0;
        let mut total_allin_hands: i64 = 0;
        let mut total_ev_net: i64 = 0;
        // The ALL row sums net and ev_net per big blind, so two stakes with one big blind divide once.
        let mut total_bb_won = BbWon::default();
        let mut total_ev_bb_won = BbWon::default();

        for g in &groups {
            total_hands = total_hands.saturating_add(g.hands);
            total_net = total_net.saturating_add(g.net_micros);
            total_rake = total_rake.saturating_add(g.rake_micros);
            total_fees = total_fees.saturating_add(g.fees_micros);
            total_blinds = total_blinds.saturating_add(g.blinds_micros);
            total_straddles = total_straddles.saturating_add(g.straddles_micros);
            total_allin_hands = total_allin_hands.saturating_add(g.allin_hands);
            total_ev_net = total_ev_net.saturating_add(g.ev_net_micros);

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

        if groups.len() > 1 && total_hands > 0 {
            let net_dec = Decimal::new(total_net, 6);
            let net_per_100_dec = (net_dec / Decimal::from(total_hands)) * Decimal::from(100);
            let total_bb_won_str = total_bb_won.format_bb_won("bb_won")?;
            let bb_per_100_str = total_bb_won.format_bb_per_100(total_hands, "bb_per_100")?;
            let ev_bb_per_100_str =
                total_ev_bb_won.format_bb_per_100(total_hands, "ev_bb_per_100")?;

            result_rows.push(ResultsRow {
                currency,
                stakes: "ALL".to_string(),
                hands: total_hands,
                net: format_exact_money(total_net),
                net_per_100: format_fixed2(net_per_100_dec),
                bb_won: total_bb_won_str,
                bb_per_100: bb_per_100_str,
                rake: format_exact_money(total_rake),
                fees: format_exact_money(total_fees),
                blinds: format_exact_money(total_blinds),
                straddles: format_exact_money(total_straddles),
                allin_hands: total_allin_hands,
                ev_net: format_exact_money(total_ev_net),
                ev_bb_per_100: ev_bb_per_100_str,
            });
        }
    }

    Ok(result_rows)
}
