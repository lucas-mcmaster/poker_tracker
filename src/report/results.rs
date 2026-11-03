//! Results report generation (SPEC_v3.7 §6).

use std::collections::BTreeMap;

use rusqlite::Transaction;
use rust_decimal::Decimal;

use super::format::{format_exact_money, format_fixed2};
use super::{ReportError, Stakes};

pub const RESULTS_HEADER: &str =
    "currency,stakes,hands,net,net_per_100,bb_won,bb_per_100,rake,fees,blinds,straddles";

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
}

impl ResultsRow {
    #[must_use]
    pub fn to_csv_line(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{},{},{},{}",
            self.currency,
            self.stakes,
            self.hands,
            self.net,
            self.net_per_100,
            self.bb_won,
            self.bb_per_100,
            self.rake,
            self.fees,
            self.blinds,
            self.straddles
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
            SUM(hps.straddles_posted_micros) \
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
        let mut total_bb_won = Decimal::ZERO;

        for g in &groups {
            total_hands = total_hands.saturating_add(g.hands);
            total_net = total_net.saturating_add(g.net_micros);
            total_rake = total_rake.saturating_add(g.rake_micros);
            total_fees = total_fees.saturating_add(g.fees_micros);
            total_blinds = total_blinds.saturating_add(g.blinds_micros);
            total_straddles = total_straddles.saturating_add(g.straddles_micros);

            let net_dec = Decimal::new(g.net_micros, 6);
            let net_per_100_dec = (net_dec / Decimal::from(g.hands)) * Decimal::from(100);
            let bb_won_dec = Decimal::from(g.net_micros) / Decimal::from(g.big_blind_micros);
            total_bb_won += bb_won_dec;

            let bb_per_100_dec = (bb_won_dec / Decimal::from(g.hands)) * Decimal::from(100);

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
                bb_won: format_fixed2(bb_won_dec),
                bb_per_100: format_fixed2(bb_per_100_dec),
                rake: format_exact_money(g.rake_micros),
                fees: format_exact_money(g.fees_micros),
                blinds: format_exact_money(g.blinds_micros),
                straddles: format_exact_money(g.straddles_micros),
            });
        }

        if groups.len() > 1 && total_hands > 0 {
            let net_dec = Decimal::new(total_net, 6);
            let net_per_100_dec = (net_dec / Decimal::from(total_hands)) * Decimal::from(100);
            let bb_per_100_dec = (total_bb_won / Decimal::from(total_hands)) * Decimal::from(100);

            result_rows.push(ResultsRow {
                currency,
                stakes: "ALL".to_string(),
                hands: total_hands,
                net: format_exact_money(total_net),
                net_per_100: format_fixed2(net_per_100_dec),
                bb_won: format_fixed2(total_bb_won),
                bb_per_100: format_fixed2(bb_per_100_dec),
                rake: format_exact_money(total_rake),
                fees: format_exact_money(total_fees),
                blinds: format_exact_money(total_blinds),
                straddles: format_exact_money(total_straddles),
            });
        }
    }

    Ok(result_rows)
}
