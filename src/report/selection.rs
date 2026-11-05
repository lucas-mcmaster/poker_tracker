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
pub(crate) fn selection(stakes: Option<&[Stakes]>) -> Selection {
    let (stakes_clause, params) = match stakes {
        None => ("1".to_string(), Vec::new()),
        Some([]) => ("0".to_string(), Vec::new()),
        Some(list) => {
            let mut clause = String::new();
            let mut params = Vec::with_capacity(list.len() * 2);
            for (i, s) in list.iter().enumerate() {
                if i > 0 {
                    clause.push_str(" OR ");
                }
                clause.push_str("(h.small_blind_micros = ? AND h.big_blind_micros = ?)");
                params.push(rusqlite::types::Value::Integer(s.small_blind_micros));
                params.push(rusqlite::types::Value::Integer(s.big_blind_micros));
            }
            (format!("({clause})"), params)
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
