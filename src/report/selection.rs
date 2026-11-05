//! Single shared selection fragment for all report queries and stakes probes (SPEC_v3.8 §4 M17.1.4).

use crate::domain::Stakes;

/// The SQL selection fragment and bound parameters.
#[derive(Debug, Clone)]
pub(crate) struct Selection {
    pub sql: String,
    pub params: Vec<rusqlite::types::Value>,
}

/// Builds the single shared selection fragment for report queries.
///
/// ```sql
/// FROM hands h
/// JOIN hand_players hp ON hp.hand_id = h.id
/// JOIN hand_player_stats hps ON hps.hand_id = hp.hand_id AND hps.player_id = hp.player_id
/// WHERE (<player>) AND (<stakes>) AND (<filter>)
/// ```
///
/// `<stakes>` is `1` for `None`, `0` for an empty list, and otherwise
/// `(h.small_blind_micros, h.big_blind_micros) IN (VALUES (?, ?), …)`, bound small blind then big
/// blind per item. Row values keep the expression depth constant: an `OR` chain of the same pairs
/// exceeds SQLite's depth limit of 1,000 at about 994 items (SPEC_v3.8 amendment A1).
pub(crate) fn selection(stakes: Option<&[Stakes]>) -> Selection {
    let (stakes_clause, params) = match stakes {
        None => ("1".to_string(), Vec::new()),
        Some([]) => ("0".to_string(), Vec::new()),
        Some(list) => {
            let mut params = Vec::with_capacity(list.len() * 2);
            for s in list {
                params.push(rusqlite::types::Value::Integer(s.small_blind_micros));
                params.push(rusqlite::types::Value::Integer(s.big_blind_micros));
            }
            let rows = vec!["(?, ?)"; list.len()].join(", ");
            (
                format!("(h.small_blind_micros, h.big_blind_micros) IN (VALUES {rows})"),
                params,
            )
        }
    };
    let sql = format!(
        "FROM hands h \
         JOIN hand_players hp ON hp.hand_id = h.id \
         JOIN hand_player_stats hps ON hps.hand_id = hp.hand_id AND hps.player_id = hp.player_id \
         WHERE (hp.is_hero = 1) AND ({stakes_clause}) AND (1)"
    );
    Selection { sql, params }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::types::Value;

    const FROM: &str = "FROM hands h \
        JOIN hand_players hp ON hp.hand_id = h.id \
        JOIN hand_player_stats hps ON hps.hand_id = hp.hand_id AND hps.player_id = hp.player_id \
        WHERE (hp.is_hero = 1) AND ";

    #[test]
    fn selection_fragment_for_no_empty_and_listed_stakes() {
        let all = selection(None);
        assert_eq!(all.sql, format!("{FROM}(1) AND (1)"));
        assert!(all.params.is_empty());

        let none = selection(Some(&[]));
        assert_eq!(none.sql, format!("{FROM}(0) AND (1)"));
        assert!(none.params.is_empty());

        let list = [
            Stakes {
                small_blind_micros: 50_000,
                big_blind_micros: 100_000,
            },
            Stakes {
                small_blind_micros: 1_000_000,
                big_blind_micros: 2_000_000,
            },
        ];
        let two = selection(Some(&list));
        assert_eq!(
            two.sql,
            format!(
                "{FROM}((h.small_blind_micros, h.big_blind_micros) IN (VALUES (?, ?), (?, ?))) \
                 AND (1)"
            )
        );
        assert_eq!(
            two.params,
            [50_000, 100_000, 1_000_000, 2_000_000].map(Value::Integer)
        );
    }
}
