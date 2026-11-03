//! Positions report generation (SPEC_v3.7 §6).

use std::collections::BTreeMap;

use rusqlite::Transaction;
use rust_decimal::Decimal;

use super::format::{format_exact_money, format_fixed2, format_pct};
use super::{BbWon, ReportError, Stakes};
use crate::stats::position_label;

pub const POSITIONS_HEADER: &str = "currency,position,hands,net,bb_per_100,vpip,pfr,rfi";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionsRow {
    pub currency: String,
    pub position: String,
    pub hands: i64,
    pub net: String,
    pub bb_per_100: String,
    pub vpip: String,
    pub pfr: String,
    pub rfi: String,
}

impl PositionsRow {
    #[must_use]
    pub fn to_csv_line(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{}",
            self.currency,
            self.position,
            self.hands,
            self.net,
            self.bb_per_100,
            self.vpip,
            self.pfr,
            self.rfi
        )
    }
}

#[derive(Default)]
struct PositionAgg {
    hands: i64,
    net_micros: i64,
    bb_won: BbWon,
    p_walk: i64,
    p_vpip: i64,
    p_pfr_opp: i64,
    p_pfr: i64,
    p_rfi_opp: i64,
    p_rfi: i64,
}

const ORDERED_LABELS: &[&str] = &["EP", "MP", "CO", "BTN", "SB", "BB", "?"];

pub(crate) fn build_positions_rows(
    tx: &Transaction,
    stakes_filter: Option<&[Stakes]>,
) -> Result<Vec<PositionsRow>, ReportError> {
    let (clause, params) = super::stakes_clause(stakes_filter);
    let sql = format!(
        "SELECT \
            h.currency, \
            hps.position, \
            h.max_seats, \
            h.big_blind_micros, \
            COUNT(*), \
            SUM(hp.net_micros), \
            SUM(hps.p_walk), \
            SUM(hps.p_vpip), \
            SUM(hps.p_pfr_opp), \
            SUM(hps.p_pfr), \
            SUM(hps.p_rfi_opp), \
            SUM(hps.p_rfi) \
         FROM hands h \
         JOIN hand_players hp ON hp.hand_id = h.id AND hp.is_hero = 1 \
         JOIN hand_player_stats hps ON hps.hand_id = hp.hand_id AND hps.player_id = hp.player_id \
         WHERE 1 = 1 {clause} \
         GROUP BY h.currency, hps.position, h.max_seats, h.big_blind_micros \
         ORDER BY h.currency ASC"
    );

    let mut stmt = tx.prepare(&sql)?;
    let rusqlite_params: Vec<&dyn rusqlite::ToSql> =
        params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

    let mut agg_by_currency: BTreeMap<String, BTreeMap<&'static str, PositionAgg>> =
        BTreeMap::new();
    let mut query_rows = stmt.query(rusqlite_params.as_slice())?;

    while let Some(row) = query_rows.next()? {
        let currency: String = row.get(0)?;
        let pos_opt: Option<u8> = row.get(1)?;
        let max_seats: u8 = row.get(2)?;
        let bb_micros: i64 = row.get(3)?;
        let hands: i64 = row.get(4)?;
        let net_micros: i64 = row.get(5)?;
        let p_walk: i64 = row.get(6)?;
        let p_vpip: i64 = row.get(7)?;
        let p_pfr_opp: i64 = row.get(8)?;
        let p_pfr: i64 = row.get(9)?;
        let p_rfi_opp: i64 = row.get(10)?;
        let p_rfi: i64 = row.get(11)?;

        let label = match pos_opt {
            Some(code) => position_label(code, max_seats),
            None => "?",
        };

        let entry = agg_by_currency
            .entry(currency)
            .or_default()
            .entry(label)
            .or_default();

        entry.hands = entry.hands.saturating_add(hands);
        entry.net_micros = entry.net_micros.saturating_add(net_micros);
        entry.bb_won.add(bb_micros, net_micros);
        entry.p_walk = entry.p_walk.saturating_add(p_walk);
        entry.p_vpip = entry.p_vpip.saturating_add(p_vpip);
        entry.p_pfr_opp = entry.p_pfr_opp.saturating_add(p_pfr_opp);
        entry.p_pfr = entry.p_pfr.saturating_add(p_pfr);
        entry.p_rfi_opp = entry.p_rfi_opp.saturating_add(p_rfi_opp);
        entry.p_rfi = entry.p_rfi.saturating_add(p_rfi);
    }

    let mut rows = Vec::new();

    for (currency, pos_map) in agg_by_currency {
        for &label in ORDERED_LABELS {
            if let Some(agg) = pos_map.get(label) {
                if agg.hands <= 0 {
                    continue;
                }
                let bb_per_100_dec =
                    (agg.bb_won.total() / Decimal::from(agg.hands)) * Decimal::from(100);
                let vpip_den = agg.hands.saturating_sub(agg.p_walk);

                rows.push(PositionsRow {
                    currency: currency.clone(),
                    position: label.to_string(),
                    hands: agg.hands,
                    net: format_exact_money(agg.net_micros),
                    bb_per_100: format_fixed2(bb_per_100_dec),
                    vpip: format_pct(agg.p_vpip, vpip_den),
                    pfr: format_pct(agg.p_pfr, agg.p_pfr_opp),
                    rfi: format_pct(agg.p_rfi, agg.p_rfi_opp),
                });
            }
        }
    }

    Ok(rows)
}
