//! Sessions report generation (SPEC_v3.7 §6, `docs/stats.md` §6.1).

use std::collections::BTreeMap;

use chrono::{NaiveDateTime, TimeDelta};
use rusqlite::Transaction;

use super::format::{format_exact_money, quote_csv_text};
use super::{BbWon, ReportError, Stakes};
use crate::storage::StorageError;
use crate::storage::repository::TIMESTAMP_FORMAT;

pub const SESSIONS_HEADER: &str =
    "currency,platform,table,start,end,duration_seconds,hands,net,bb_per_100,ev_net";

/// The longest start-to-start gap between consecutive hands of one session (inclusive).
const SESSION_GAP_SECONDS: i64 = 30 * 60;

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
    pub ev_net: String,
}

impl SessionsRow {
    #[must_use]
    pub fn to_csv_line(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{},{},{}",
            quote_csv_text(&self.currency),
            quote_csv_text(&self.platform),
            quote_csv_text(&self.table),
            self.start,
            self.end,
            self.duration_seconds,
            self.hands,
            self.net,
            self.bb_per_100,
            self.ev_net
        )
    }
}

/// One hero hand as the sessions report reads it.
#[derive(Debug, Clone)]
struct HandEntry {
    platform: String,
    currency: String,
    table: String,
    /// The stored timestamp text, printed as `start` and `end`.
    timestamp_text: String,
    /// The timestamp as printed.
    local: NaiveDateTime,
    /// The UTC time, when the hand states a UTC offset.
    utc: Option<NaiveDateTime>,
    site_hand_id: String,
    net_micros: i64,
    big_blind_micros: i64,
    ev_net_micros: i64,
}

/// A hand paired with its time on its table's basis (`docs/stats.md` §6.1).
type TimedHand = (NaiveDateTime, HandEntry);

fn corrupt_hand_row(detail: String) -> ReportError {
    ReportError::Storage(StorageError::CorruptRow {
        table: "hands",
        detail,
    })
}

/// Reads a stored timestamp as printed and, when the hand states a UTC offset, its UTC time.
/// A value that does not parse fails closed, as loading the hand does.
fn read_times(
    text: &str,
    utc_offset_seconds: Option<i64>,
) -> Result<(NaiveDateTime, Option<NaiveDateTime>), ReportError> {
    let local = NaiveDateTime::parse_from_str(text, TIMESTAMP_FORMAT)
        .map_err(|_| corrupt_hand_row(format!("invalid timestamp '{text}'")))?;
    let utc = match utc_offset_seconds {
        None => None,
        Some(offset) => Some(
            TimeDelta::try_seconds(offset)
                .and_then(|delta| local.checked_sub_signed(delta))
                .ok_or_else(|| {
                    corrupt_hand_row(format!(
                        "timestamp '{text}' with UTC offset {offset} s is out of range"
                    ))
                })?,
        ),
    };
    Ok((local, utc))
}

/// Splits hero hands into table sessions (`docs/stats.md` §6.1), listed by currency, printed
/// start, table, then platform.
///
/// A table is a `(platform, currency, table name)` key. All of a table's hands use one time
/// basis: UTC when every one of them states an offset, otherwise the timestamps as printed.
/// They are ordered by that time, then site hand id, and a gap of more than 30 minutes starts a
/// new session. With one basis per table, no gap or duration is negative.
fn group_sessions(hands: Vec<HandEntry>) -> Vec<Vec<TimedHand>> {
    let mut tables: BTreeMap<(String, String, String), Vec<HandEntry>> = BTreeMap::new();
    for hand in hands {
        let key = (
            hand.platform.clone(),
            hand.currency.clone(),
            hand.table.clone(),
        );
        tables.entry(key).or_default().push(hand);
    }

    let mut sessions: Vec<Vec<TimedHand>> = Vec::new();
    for table_hands in tables.into_values() {
        let in_utc = table_hands.iter().all(|hand| hand.utc.is_some());
        let mut timed: Vec<TimedHand> = table_hands
            .into_iter()
            .map(|hand| {
                let time = match hand.utc {
                    Some(utc) if in_utc => utc,
                    _ => hand.local,
                };
                (time, hand)
            })
            .collect();
        timed.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.site_hand_id.cmp(&b.1.site_hand_id))
        });

        let mut current: Vec<TimedHand> = Vec::new();
        for entry in timed {
            let gap_exceeded = current.last().is_some_and(|(previous, _)| {
                entry.0.signed_duration_since(*previous).num_seconds() > SESSION_GAP_SECONDS
            });
            if gap_exceeded {
                sessions.push(std::mem::take(&mut current));
            }
            current.push(entry);
        }
        if !current.is_empty() {
            sessions.push(current);
        }
    }

    sessions.sort_by(|a, b| listing_key(a).cmp(&listing_key(b)));
    sessions
}

/// A session's listing order: currency, `start` as printed, table, platform (SPEC_v3.7 M15.4).
fn listing_key(session: &[TimedHand]) -> Option<(&str, NaiveDateTime, &str, &str)> {
    session.first().map(|(_, hand)| {
        (
            hand.currency.as_str(),
            hand.local,
            hand.table.as_str(),
            hand.platform.as_str(),
        )
    })
}

/// Whole seconds from a session's first hand to its last, on its table's basis.
fn duration_seconds(session: &[TimedHand]) -> i64 {
    match (session.first(), session.last()) {
        (Some((first, _)), Some((last, _))) => last.signed_duration_since(*first).num_seconds(),
        _ => 0,
    }
}

pub(crate) fn build_sessions_rows(
    tx: &Transaction,
    stakes_filter: Option<&[Stakes]>,
) -> Result<Vec<SessionsRow>, ReportError> {
    let sel = super::selection(stakes_filter);
    let sql = format!(
        "SELECT \
            h.platform, \
            h.currency, \
            h.table_name, \
            h.timestamp, \
            h.utc_offset_seconds, \
            h.site_hand_id, \
            hp.net_micros, \
            h.big_blind_micros, \
            hps.ev_net_micros \
         {}",
        sel.sql
    );

    let mut stmt = tx.prepare(&sql)?;
    let rusqlite_params: Vec<&dyn rusqlite::ToSql> = sel
        .params
        .iter()
        .map(|v| v as &dyn rusqlite::ToSql)
        .collect();
    let mut query_rows = stmt.query(rusqlite_params.as_slice())?;

    let mut hands = Vec::new();
    while let Some(row) = query_rows.next()? {
        let timestamp_text: String = row.get(3)?;
        let (local, utc) = read_times(&timestamp_text, row.get(4)?)?;
        hands.push(HandEntry {
            platform: row.get(0)?,
            currency: row.get(1)?,
            table: row.get(2)?,
            timestamp_text,
            local,
            utc,
            site_hand_id: row.get(5)?,
            net_micros: row.get(6)?,
            big_blind_micros: row.get(7)?,
            ev_net_micros: row.get(8)?,
        });
    }

    let mut rows = Vec::new();
    for session in group_sessions(hands) {
        if let Some(r) = session_row(&session)? {
            rows.push(r);
        }
    }
    Ok(rows)
}

pub(crate) fn aggregate_session_net(
    hands: impl IntoIterator<Item = (i64, i64)>,
) -> Result<(i64, i64), ReportError> {
    let mut total_net: i64 = 0;
    let mut total_ev_net: i64 = 0;
    for (net, ev_net) in hands {
        total_net = total_net
            .checked_add(net)
            .ok_or(ReportError::ArithmeticOverflow { figure: "net" })?;
        total_ev_net = total_ev_net
            .checked_add(ev_net)
            .ok_or(ReportError::ArithmeticOverflow { figure: "ev_net" })?;
    }
    Ok((total_net, total_ev_net))
}

/// One session's CSV row (`None` only for an empty session, which `group_sessions` never makes).
fn session_row(session: &[TimedHand]) -> Result<Option<SessionsRow>, ReportError> {
    let (Some((_, first)), Some((_, last))) = (session.first(), session.last()) else {
        return Ok(None);
    };
    let (total_net, total_ev_net) =
        aggregate_session_net(session.iter().map(|(_, h)| (h.net_micros, h.ev_net_micros)))?;
    let mut bb_won = BbWon::default();
    for (_, hand) in session {
        bb_won.add(hand.big_blind_micros, hand.net_micros);
    }
    let hands = i64::try_from(session.len()).unwrap_or(i64::MAX);
    let bb_per_100 = bb_won.format_bb_per_100(hands, "bb_per_100")?;

    Ok(Some(SessionsRow {
        currency: first.currency.clone(),
        platform: first.platform.clone(),
        table: first.table.clone(),
        start: first.timestamp_text.clone(),
        end: last.timestamp_text.clone(),
        duration_seconds: duration_seconds(session),
        hands,
        net: format_exact_money(total_net),
        bb_per_100,
        ev_net: format_exact_money(total_ev_net),
    }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn sessions_total_net_overflow_returns_error() {
        let err = aggregate_session_net([(i64::MAX, 0), (1, 0)]).unwrap_err();
        assert!(matches!(
            err,
            ReportError::ArithmeticOverflow { figure: "net" }
        ));
    }

    #[test]
    fn sessions_total_ev_net_overflow_returns_error() {
        let err = aggregate_session_net([(0, i64::MAX), (0, 1)]).unwrap_err();
        assert!(matches!(
            err,
            ReportError::ArithmeticOverflow { figure: "ev_net" }
        ));
        assert_eq!(aggregate_session_net([(1, -2), (3, -4)]).unwrap(), (4, -6));
    }

    fn hand(platform: &str, table: &str, id: &str, ts: &str, offset: Option<i64>) -> HandEntry {
        hand_in(platform, "CAD", table, id, ts, offset)
    }

    fn hand_in(
        platform: &str,
        currency: &str,
        table: &str,
        id: &str,
        ts: &str,
        offset: Option<i64>,
    ) -> HandEntry {
        let (local, utc) = read_times(ts, offset).unwrap();
        HandEntry {
            platform: platform.to_string(),
            currency: currency.to_string(),
            table: table.to_string(),
            timestamp_text: ts.to_string(),
            local,
            utc,
            site_hand_id: id.to_string(),
            net_micros: 0,
            big_blind_micros: 100_000,
            ev_net_micros: 0,
        }
    }

    /// Each session as its site hand ids and duration.
    fn summary(sessions: &[Vec<TimedHand>]) -> Vec<(Vec<&str>, i64)> {
        sessions
            .iter()
            .map(|session| {
                let ids = session
                    .iter()
                    .map(|(_, hand)| hand.site_hand_id.as_str())
                    .collect();
                (ids, duration_seconds(session))
            })
            .collect()
    }

    #[test]
    fn group_sessions_keeps_platforms_apart() {
        // Same currency and table name, five minutes apart: one session per platform.
        let split = group_sessions(vec![
            hand("PlatformA", "Table1", "A1", "2026-08-21T12:00:00", None),
            hand("PlatformB", "Table1", "B1", "2026-08-21T12:05:00", None),
        ]);
        assert_eq!(summary(&split), vec![(vec!["A1"], 0), (vec!["B1"], 0)]);

        // Control: the same two hands on one platform are one session of 300 s.
        let joined = group_sessions(vec![
            hand("PlatformA", "Table1", "A1", "2026-08-21T12:00:00", None),
            hand("PlatformA", "Table1", "A2", "2026-08-21T12:05:00", None),
        ]);
        assert_eq!(summary(&joined), vec![(vec!["A1", "A2"], 300)]);
    }

    #[test]
    fn group_sessions_uses_one_time_basis_per_table() {
        // Every hand states an offset, so UTC: 01:50 at -4 h is 05:50, 01:10 at -5 h is 06:10.
        let dst = group_sessions(vec![
            hand("PlatformA", "T", "D2", "2026-11-01T01:10:00", Some(-18_000)),
            hand("PlatformA", "T", "D1", "2026-11-01T01:50:00", Some(-14_400)),
        ]);
        assert_eq!(summary(&dst), vec![(vec!["D1", "D2"], 1_200)]);

        // Not every hand states one, so as printed: 10:00 (at -4 h), then 10:05 (no offset).
        let mixed = group_sessions(vec![
            hand("PlatformA", "T", "M2", "2026-08-23T10:05:00", None),
            hand("PlatformA", "T", "M1", "2026-08-23T10:00:00", Some(-14_400)),
        ]);
        assert_eq!(summary(&mixed), vec![(vec!["M1", "M2"], 300)]);
    }

    #[test]
    fn group_sessions_lists_by_currency_printed_start_then_table() {
        let first_ids = |sessions: Vec<Vec<TimedHand>>| -> Vec<String> {
            sessions
                .iter()
                .filter_map(|s| s.first().map(|(_, h)| h.site_hand_id.clone()))
                .collect()
        };
        // Currency first: CAD at 12:00 before USD at 09:00 (the table map holds the USD table
        // first, under PlatformA).
        let by_currency = group_sessions(vec![
            hand_in("PlatformA", "USD", "T", "U1", "2026-08-21T09:00:00", None),
            hand_in("PlatformB", "CAD", "T", "C1", "2026-08-21T12:00:00", None),
        ]);
        assert_eq!(first_ids(by_currency), ["C1", "U1"]);

        // Same start: table before platform (the map holds PlatformA's Tz first).
        let same_start = group_sessions(vec![
            hand("PlatformA", "Tz", "Z1", "2026-08-21T12:00:00", None),
            hand("PlatformB", "Ta", "A1", "2026-08-21T12:00:00", None),
        ]);
        assert_eq!(first_ids(same_start), ["A1", "Z1"]);

        // The printed start, not the table's basis time: 10:00 at -4 h (14:00 UTC) on a UTC
        // table is listed before another table's 12:00.
        let printed = group_sessions(vec![
            hand("PlatformA", "Tp", "P1", "2026-08-21T12:00:00", None),
            hand(
                "PlatformA",
                "Tu",
                "U1",
                "2026-08-21T10:00:00",
                Some(-14_400),
            ),
        ]);
        assert_eq!(first_ids(printed), ["U1", "P1"]);
    }

    #[test]
    fn session_bb_per_100_divides_once_per_big_blind() {
        // Eight hands at a 3.00 big blind: Σ net = 1237.23, so bb/100 = 1237.23 / 3 / 8 × 100 =
        // 5155.125 exactly, which is 5155.12 half-even. Dividing hand by hand rounds each
        // non-terminating quotient, sums to 5155.125000…01 and prints 5155.13.
        let nets = [
            786_950_000,
            -948_270_000,
            206_520_000,
            -185_470_000,
            589_550_000,
            300_960_000,
            -423_460_000,
            910_450_000,
        ];
        let session: Vec<TimedHand> = nets
            .iter()
            .enumerate()
            .map(|(i, &net_micros)| {
                let mut h = hand(
                    "PlatformA",
                    "T",
                    &format!("N{i}"),
                    "2026-08-21T12:00:00",
                    None,
                );
                h.net_micros = net_micros;
                h.big_blind_micros = 3_000_000;
                (h.local, h)
            })
            .collect();
        let row = session_row(&session).unwrap().unwrap();
        assert_eq!(row.net, "1237.23");
        assert_eq!(row.bb_per_100, "5155.12");
    }

    #[test]
    fn unparseable_timestamp_is_a_corrupt_row() {
        let err = read_times("21/08/2026 17:30:00", None).unwrap_err();
        assert!(
            matches!(
                &err,
                ReportError::Storage(StorageError::CorruptRow { table: "hands", detail })
                    if detail.contains("21/08/2026 17:30:00")
            ),
            "{err:?}"
        );
    }
}
