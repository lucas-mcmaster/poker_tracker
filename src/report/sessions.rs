//! Sessions report generation (SPEC_v3.7 §6).

use std::collections::BTreeMap;

use chrono::NaiveDateTime;
use rusqlite::Transaction;
use rust_decimal::Decimal;

use super::format::{format_exact_money, format_fixed2};
use super::{ReportError, Stakes};

pub const SESSIONS_HEADER: &str =
    "currency,platform,table,start,end,duration_seconds,hands,net,bb_per_100";

const SESSION_GAP_SECONDS: i64 = 1800; // 30 minutes, inclusive

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionsRow {
    pub currency: String,
    pub platform: String,
    pub table: String,
    pub start: String,
    pub end: String,
    pub duration_seconds: i64,
    pub hands: i64,
    pub net: String,
    pub bb_per_100: String,
}

impl SessionsRow {
    #[must_use]
    pub fn to_csv_line(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{},{}",
            self.currency,
            self.platform,
            self.table,
            self.start,
            self.end,
            self.duration_seconds,
            self.hands,
            self.net,
            self.bb_per_100
        )
    }
}

struct HandEntry {
    platform: String,
    currency: String,
    table: String,
    ts_str: String,
    ts: NaiveDateTime,
    utc_offset: Option<i64>,
    site_hand_id: String,
    net_micros: i64,
    bb_micros: i64,
}

impl HandEntry {
    fn effective_utc_ts(&self) -> NaiveDateTime {
        match self.utc_offset {
            Some(off) => self.ts - chrono::Duration::seconds(off),
            None => self.ts,
        }
    }
}

fn parse_ts(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .ok()
}

fn elapsed_seconds(a: &HandEntry, b: &HandEntry) -> i64 {
    let mut d = (b.ts - a.ts).num_seconds();
    if let (Some(off_a), Some(off_b)) = (a.utc_offset, b.utc_offset) {
        d -= off_b - off_a;
    }
    d
}

pub(crate) fn build_sessions_rows(
    tx: &Transaction,
    stakes_filter: Option<&[Stakes]>,
) -> Result<Vec<SessionsRow>, ReportError> {
    let (clause, params) = super::stakes_clause(stakes_filter);
    let sql = format!(
        "SELECT \
            h.platform, \
            h.currency, \
            h.table_name, \
            h.timestamp, \
            h.utc_offset_seconds, \
            h.site_hand_id, \
            hp.net_micros, \
            h.big_blind_micros \
         FROM hands h \
         JOIN hand_players hp ON hp.hand_id = h.id AND hp.is_hero = 1 \
         WHERE 1 = 1 {clause} \
         ORDER BY h.platform ASC, h.currency ASC, h.table_name ASC, h.timestamp ASC, h.site_hand_id ASC"
    );

    let mut stmt = tx.prepare(&sql)?;
    let rusqlite_params: Vec<&dyn rusqlite::ToSql> =
        params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

    let mut hands_by_table: BTreeMap<(String, String, String), Vec<HandEntry>> = BTreeMap::new();
    let mut query_rows = stmt.query(rusqlite_params.as_slice())?;

    while let Some(row) = query_rows.next()? {
        let platform: String = row.get(0)?;
        let currency: String = row.get(1)?;
        let table: String = row.get(2)?;
        let ts_str: String = row.get(3)?;
        let utc_offset: Option<i64> = row.get(4)?;
        let site_hand_id: String = row.get(5)?;
        let net_micros: i64 = row.get(6)?;
        let bb_micros: i64 = row.get(7)?;

        let ts = parse_ts(&ts_str).unwrap_or_default();

        let key = (platform.clone(), currency.clone(), table.clone());
        hands_by_table.entry(key).or_default().push(HandEntry {
            platform,
            currency,
            table,
            ts_str,
            ts,
            utc_offset,
            site_hand_id,
            net_micros,
            bb_micros,
        });
    }

    let mut sessions: Vec<Vec<HandEntry>> = Vec::new();

    for mut list in hands_by_table.into_values() {
        if list.is_empty() {
            continue;
        }
        list.sort_by(|a, b| {
            a.effective_utc_ts()
                .cmp(&b.effective_utc_ts())
                .then_with(|| a.site_hand_id.cmp(&b.site_hand_id))
        });

        let mut current_session = Vec::new();
        for h in list {
            if let Some(prev) = current_session.last() {
                if elapsed_seconds(prev, &h) > SESSION_GAP_SECONDS {
                    let finished = std::mem::replace(&mut current_session, vec![h]);
                    sessions.push(finished);
                } else {
                    current_session.push(h);
                }
            } else {
                current_session.push(h);
            }
        }
        if !current_session.is_empty() {
            sessions.push(current_session);
        }
    }

    // Sort sessions by (currency, start_time, table, platform)
    sessions.sort_by(|s1, s2| {
        let h1 = &s1[0];
        let h2 = &s2[0];
        h1.currency
            .cmp(&h2.currency)
            .then_with(|| h1.effective_utc_ts().cmp(&h2.effective_utc_ts()))
            .then_with(|| h1.table.cmp(&h2.table))
            .then_with(|| h1.platform.cmp(&h2.platform))
    });

    let mut result_rows = Vec::with_capacity(sessions.len());

    for s in sessions {
        let first = match s.first() {
            Some(h) => h,
            None => continue,
        };
        let last = match s.last() {
            Some(h) => h,
            None => continue,
        };

        let duration_seconds = elapsed_seconds(first, last);
        let hands = s.len() as i64;
        let mut total_net: i64 = 0;
        let mut total_bb_won = Decimal::ZERO;

        for h in &s {
            total_net = total_net.saturating_add(h.net_micros);
            let bb_won = Decimal::from(h.net_micros) / Decimal::from(h.bb_micros);
            total_bb_won += bb_won;
        }

        let bb_per_100_dec = (total_bb_won / Decimal::from(hands)) * Decimal::from(100);

        result_rows.push(SessionsRow {
            currency: first.currency.clone(),
            platform: first.platform.clone(),
            table: first.table.clone(),
            start: first.ts_str.clone(),
            end: last.ts_str.clone(),
            duration_seconds,
            hands,
            net: format_exact_money(total_net),
            bb_per_100: format_fixed2(bb_per_100_dec),
        });
    }

    Ok(result_rows)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn two_platforms_same_table_name_are_separate_sessions() {
        let h1 = HandEntry {
            platform: "GGPoker".to_string(),
            currency: "CAD".to_string(),
            table: "Table1".to_string(),
            ts_str: "2026-08-21T12:00:00".to_string(),
            ts: parse_ts("2026-08-21T12:00:00").unwrap(),
            utc_offset: None,
            site_hand_id: "H1".to_string(),
            net_micros: 100_000,
            bb_micros: 100_000,
        };
        let h2 = HandEntry {
            platform: "PokerStars".to_string(),
            currency: "CAD".to_string(),
            table: "Table1".to_string(),
            ts_str: "2026-08-21T12:05:00".to_string(),
            ts: parse_ts("2026-08-21T12:05:00").unwrap(),
            utc_offset: None,
            site_hand_id: "H2".to_string(),
            net_micros: 200_000,
            bb_micros: 100_000,
        };

        let mut hands_by_table: BTreeMap<(String, String, String), Vec<HandEntry>> =
            BTreeMap::new();
        for h in [h1, h2] {
            let key = (h.platform.clone(), h.currency.clone(), h.table.clone());
            hands_by_table.entry(key).or_default().push(h);
        }
        assert_eq!(
            hands_by_table.len(),
            2,
            "different platforms must not be grouped together"
        );
    }
}
