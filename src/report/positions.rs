//! Positions report generation (SPEC_v3.7 §6).

use std::collections::BTreeMap;
use std::str::FromStr;

use rusqlite::Transaction;
use rust_decimal::Decimal;
use serde::Serialize;

use super::format::{quote_csv_text, round_pct};
use super::{BbWon, ReportError, Stakes};
use crate::domain::money::{CurrencyCode, micro_units_to_decimal};
use crate::stats::PositionLabel;
use crate::storage::StorageError;

pub const POSITIONS_HEADER: &str = "currency,position,hands,net,bb_per_100,vpip,pfr,rfi";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PositionsRow {
    pub currency: CurrencyCode,
    pub position: PositionLabel,
    pub hands: i64,
    pub net: Decimal,
    pub bb_per_100: Decimal,
    pub vpip: Option<Decimal>,
    pub pfr: Option<Decimal>,
    pub rfi: Option<Decimal>,
}

impl PositionsRow {
    #[must_use]
    pub fn to_csv_line(&self) -> String {
        let fmt_opt = |opt: Option<Decimal>| match opt {
            Some(v) => v.to_string(),
            None => String::new(),
        };
        format!(
            "{},{},{},{},{},{},{},{}",
            quote_csv_text(self.currency.as_str()),
            quote_csv_text(self.position.as_str()),
            self.hands,
            self.net,
            self.bb_per_100,
            fmt_opt(self.vpip),
            fmt_opt(self.pfr),
            fmt_opt(self.rfi),
        )
    }
}

#[derive(Default)]
pub(crate) struct PositionAgg {
    pub hands: i64,
    pub net_micros: i64,
    pub bb_won: BbWon,
    pub p_walk: i64,
    pub p_vpip: i64,
    pub p_pfr_opp: i64,
    pub p_pfr: i64,
    pub p_rfi_opp: i64,
    pub p_rfi: i64,
}

impl PositionAgg {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add_group(
        &mut self,
        hands: i64,
        net_micros: i64,
        bb_micros: i64,
        p_walk: i64,
        p_vpip: i64,
        p_pfr_opp: i64,
        p_pfr: i64,
        p_rfi_opp: i64,
        p_rfi: i64,
    ) -> Result<(), ReportError> {
        self.hands = self
            .hands
            .checked_add(hands)
            .ok_or(ReportError::ArithmeticOverflow { figure: "hands" })?;
        self.net_micros = self
            .net_micros
            .checked_add(net_micros)
            .ok_or(ReportError::ArithmeticOverflow { figure: "net" })?;
        self.bb_won.add(bb_micros, net_micros);
        self.p_walk = self
            .p_walk
            .checked_add(p_walk)
            .ok_or(ReportError::ArithmeticOverflow { figure: "vpip" })?;
        self.p_vpip = self
            .p_vpip
            .checked_add(p_vpip)
            .ok_or(ReportError::ArithmeticOverflow { figure: "vpip" })?;
        self.p_pfr_opp = self
            .p_pfr_opp
            .checked_add(p_pfr_opp)
            .ok_or(ReportError::ArithmeticOverflow { figure: "pfr" })?;
        self.p_pfr = self
            .p_pfr
            .checked_add(p_pfr)
            .ok_or(ReportError::ArithmeticOverflow { figure: "pfr" })?;
        self.p_rfi_opp = self
            .p_rfi_opp
            .checked_add(p_rfi_opp)
            .ok_or(ReportError::ArithmeticOverflow { figure: "rfi" })?;
        self.p_rfi = self
            .p_rfi
            .checked_add(p_rfi)
            .ok_or(ReportError::ArithmeticOverflow { figure: "rfi" })?;
        Ok(())
    }
}

pub(crate) fn build_positions_rows(
    tx: &Transaction,
    stakes_filter: Option<&[Stakes]>,
) -> Result<Vec<PositionsRow>, ReportError> {
    let sel = super::selection(stakes_filter);
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
         {} \
         GROUP BY h.currency, hps.position, h.max_seats, h.big_blind_micros \
         ORDER BY h.currency ASC",
        sel.sql
    );

    let mut stmt = tx.prepare(&sql)?;
    let rusqlite_params: Vec<&dyn rusqlite::ToSql> = sel
        .params
        .iter()
        .map(|v| v as &dyn rusqlite::ToSql)
        .collect();

    let mut agg_by_currency: BTreeMap<String, BTreeMap<PositionLabel, PositionAgg>> =
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

        let label = PositionLabel::of(pos_opt, max_seats);

        let entry = agg_by_currency
            .entry(currency)
            .or_default()
            .entry(label)
            .or_default();

        entry.add_group(
            hands, net_micros, bb_micros, p_walk, p_vpip, p_pfr_opp, p_pfr, p_rfi_opp, p_rfi,
        )?;
    }

    let mut rows = Vec::new();

    for (currency, pos_map) in agg_by_currency {
        let currency_code = CurrencyCode::from_str(&currency).map_err(|_| {
            ReportError::Storage(StorageError::CorruptRow {
                table: "hands",
                detail: format!("invalid currency '{currency}'"),
            })
        })?;
        for &label in &PositionLabel::ALL {
            if let Some(agg) = pos_map.get(&label) {
                if agg.hands <= 0 {
                    continue;
                }
                let bb_per_100_val = agg.bb_won.bb_per_100(agg.hands, "bb_per_100")?;
                let vpip_den = agg
                    .hands
                    .checked_sub(agg.p_walk)
                    .ok_or(ReportError::ArithmeticOverflow { figure: "vpip" })?;

                rows.push(PositionsRow {
                    currency: currency_code,
                    position: label,
                    hands: agg.hands,
                    net: micro_units_to_decimal(agg.net_micros),
                    bb_per_100: bb_per_100_val,
                    vpip: round_pct(agg.p_vpip, vpip_den),
                    pfr: round_pct(agg.p_pfr, agg.p_pfr_opp),
                    rfi: round_pct(agg.p_rfi, agg.p_rfi_opp),
                });
            }
        }
    }

    Ok(rows)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn positions_merge_hands_overflow_returns_error() {
        let mut entry = PositionAgg::default();
        entry
            .add_group(i64::MAX, 0, 100_000, 0, 0, 0, 0, 0, 0)
            .unwrap();
        let err = entry
            .add_group(1, 0, 100_000, 0, 0, 0, 0, 0, 0)
            .unwrap_err();
        assert!(matches!(
            err,
            ReportError::ArithmeticOverflow { figure: "hands" }
        ));
    }

    /// Every merged count names the CSV column it feeds (`p_walk` feeds the VPIP denominator).
    #[test]
    fn every_positions_merge_overflow_names_its_column() {
        // Argument order of `add_group`, without `bb_micros` (index 2), which sums nothing.
        let figures = [
            (0, "hands"),
            (1, "net"),
            (3, "vpip"),
            (4, "vpip"),
            (5, "pfr"),
            (6, "pfr"),
            (7, "rfi"),
            (8, "rfi"),
        ];
        let add = |entry: &mut PositionAgg, v: [i64; 9]| {
            entry.add_group(v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8])
        };
        for (index, figure) in figures {
            let mut big = [0; 9];
            let mut one = [0; 9];
            big[2] = 100_000;
            one[2] = 100_000;
            big[index] = i64::MAX;
            one[index] = 1;
            let mut entry = PositionAgg::default();
            add(&mut entry, big).unwrap();
            let err = add(&mut entry, one).unwrap_err();
            assert!(
                matches!(err, ReportError::ArithmeticOverflow { figure: f } if f == figure),
                "argument {index}: expected {figure}, got {err:?}"
            );
        }
        let mut entry = PositionAgg::default();
        add(&mut entry, [1, 2, 100_000, 3, 4, 5, 6, 7, 8]).unwrap();
        add(&mut entry, [10, 20, 100_000, 30, 40, 50, 60, 70, 80]).unwrap();
        assert_eq!(
            [
                entry.hands,
                entry.net_micros,
                entry.p_walk,
                entry.p_vpip,
                entry.p_pfr_opp,
                entry.p_pfr,
                entry.p_rfi_opp,
                entry.p_rfi
            ],
            [11, 22, 33, 44, 55, 66, 77, 88]
        );
    }
}
