//! Statistics report generation (SPEC_v3.7 §6).

use std::collections::BTreeMap;

use rusqlite::Transaction;

use super::format::{format_factor, format_pct};
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
            self.currency, self.stat, self.numerator, self.denominator, self.value
        )
    }
}

#[derive(Default)]
struct CurrencyStats {
    hands: i64,
    p_walk: i64,
    p_vpip: i64,
    p_pfr_opp: i64,
    p_pfr: i64,
    f_saw: i64,
    sd_went: i64,
    sd_won: i64,
    won_hand: i64,
    f_saw_won: i64,
    p_rfi_opp: i64,
    p_rfi: i64,
    p_3bet_opp: i64,
    p_3bet: i64,
    p_3bet_faced: i64,
    p_3bet_fold: i64,
    p_3bet_call: i64,
    p_4bet_opp: i64,
    p_4bet: i64,
    p_4bet_faced: i64,
    p_4bet_fold: i64,
    p_4bet_call: i64,
    p_cc_opp: i64,
    p_cc: i64,
    p_steal_opp: i64,
    p_steal: i64,
    p_sb_steal_faced: i64,
    p_sb_steal_fold: i64,
    p_sb_steal_raise: i64,
    p_bb_steal_faced: i64,
    p_bb_steal_fold: i64,
    p_bb_steal_raise: i64,
    f_cbet_opp: i64,
    f_cbet: i64,
    f_cbet_faced: i64,
    f_cbet_fold: i64,
    f_cbet_call: i64,
    f_cbet_raise: i64,
    t_cbet_opp: i64,
    t_cbet: i64,
    t_cbet_faced: i64,
    t_cbet_fold: i64,
    t_cbet_call: i64,
    t_cbet_raise: i64,
    f_xr_opp: i64,
    f_xr: i64,
    p_bets: i64,
    p_raises: i64,
    p_calls: i64,
    p_folds: i64,
    f_bets: i64,
    f_raises: i64,
    f_calls: i64,
    f_folds: i64,
    t_bets: i64,
    t_raises: i64,
    t_calls: i64,
    t_folds: i64,
    r_bets: i64,
    r_raises: i64,
    r_calls: i64,
    r_folds: i64,
}

pub(crate) fn build_stats_rows(
    tx: &Transaction,
    stakes_filter: Option<&[Stakes]>,
) -> Result<Vec<StatsRow>, ReportError> {
    let (clause, params) = super::stakes_clause(stakes_filter);
    let sql = format!(
        "SELECT \
            h.currency, \
            COUNT(*), \
            SUM(hps.p_walk), \
            SUM(hps.p_vpip), \
            SUM(hps.p_pfr_opp), \
            SUM(hps.p_pfr), \
            SUM(hps.f_saw), \
            SUM(hps.sd_went), \
            SUM(hps.sd_won), \
            SUM(hps.won_hand), \
            SUM(hps.f_saw_won), \
            SUM(hps.p_rfi_opp), \
            SUM(hps.p_rfi), \
            SUM(hps.p_3bet_opp), \
            SUM(hps.p_3bet), \
            SUM(hps.p_3bet_faced), \
            SUM(hps.p_3bet_fold), \
            SUM(hps.p_3bet_call), \
            SUM(hps.p_4bet_opp), \
            SUM(hps.p_4bet), \
            SUM(hps.p_4bet_faced), \
            SUM(hps.p_4bet_fold), \
            SUM(hps.p_4bet_call), \
            SUM(hps.p_cc_opp), \
            SUM(hps.p_cc), \
            SUM(hps.p_steal_opp), \
            SUM(hps.p_steal), \
            SUM(hps.p_sb_steal_faced), \
            SUM(hps.p_sb_steal_fold), \
            SUM(hps.p_sb_steal_raise), \
            SUM(hps.p_bb_steal_faced), \
            SUM(hps.p_bb_steal_fold), \
            SUM(hps.p_bb_steal_raise), \
            SUM(hps.f_cbet_opp), \
            SUM(hps.f_cbet), \
            SUM(hps.f_cbet_faced), \
            SUM(hps.f_cbet_fold), \
            SUM(hps.f_cbet_call), \
            SUM(hps.f_cbet_raise), \
            SUM(hps.t_cbet_opp), \
            SUM(hps.t_cbet), \
            SUM(hps.t_cbet_faced), \
            SUM(hps.t_cbet_fold), \
            SUM(hps.t_cbet_call), \
            SUM(hps.t_cbet_raise), \
            SUM(hps.f_xr_opp), \
            SUM(hps.f_xr), \
            SUM(hps.p_bets), \
            SUM(hps.p_raises), \
            SUM(hps.p_calls), \
            SUM(hps.p_folds), \
            SUM(hps.f_bets), \
            SUM(hps.f_raises), \
            SUM(hps.f_calls), \
            SUM(hps.f_folds), \
            SUM(hps.t_bets), \
            SUM(hps.t_raises), \
            SUM(hps.t_calls), \
            SUM(hps.t_folds), \
            SUM(hps.r_bets), \
            SUM(hps.r_raises), \
            SUM(hps.r_calls), \
            SUM(hps.r_folds) \
         FROM hands h \
         JOIN hand_players hp ON hp.hand_id = h.id AND hp.is_hero = 1 \
         JOIN hand_player_stats hps ON hps.hand_id = hp.hand_id AND hps.player_id = hp.player_id \
         WHERE 1 = 1 {clause} \
         GROUP BY h.currency \
         ORDER BY h.currency ASC"
    );

    let mut stmt = tx.prepare(&sql)?;
    let rusqlite_params: Vec<&dyn rusqlite::ToSql> =
        params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

    let mut by_currency: BTreeMap<String, CurrencyStats> = BTreeMap::new();
    let mut query_rows = stmt.query(rusqlite_params.as_slice())?;

    while let Some(row) = query_rows.next()? {
        let currency: String = row.get(0)?;
        let stats = CurrencyStats {
            hands: row.get(1)?,
            p_walk: row.get(2)?,
            p_vpip: row.get(3)?,
            p_pfr_opp: row.get(4)?,
            p_pfr: row.get(5)?,
            f_saw: row.get(6)?,
            sd_went: row.get(7)?,
            sd_won: row.get(8)?,
            won_hand: row.get(9)?,
            f_saw_won: row.get(10)?,
            p_rfi_opp: row.get(11)?,
            p_rfi: row.get(12)?,
            p_3bet_opp: row.get(13)?,
            p_3bet: row.get(14)?,
            p_3bet_faced: row.get(15)?,
            p_3bet_fold: row.get(16)?,
            p_3bet_call: row.get(17)?,
            p_4bet_opp: row.get(18)?,
            p_4bet: row.get(19)?,
            p_4bet_faced: row.get(20)?,
            p_4bet_fold: row.get(21)?,
            p_4bet_call: row.get(22)?,
            p_cc_opp: row.get(23)?,
            p_cc: row.get(24)?,
            p_steal_opp: row.get(25)?,
            p_steal: row.get(26)?,
            p_sb_steal_faced: row.get(27)?,
            p_sb_steal_fold: row.get(28)?,
            p_sb_steal_raise: row.get(29)?,
            p_bb_steal_faced: row.get(30)?,
            p_bb_steal_fold: row.get(31)?,
            p_bb_steal_raise: row.get(32)?,
            f_cbet_opp: row.get(33)?,
            f_cbet: row.get(34)?,
            f_cbet_faced: row.get(35)?,
            f_cbet_fold: row.get(36)?,
            f_cbet_call: row.get(37)?,
            f_cbet_raise: row.get(38)?,
            t_cbet_opp: row.get(39)?,
            t_cbet: row.get(40)?,
            t_cbet_faced: row.get(41)?,
            t_cbet_fold: row.get(42)?,
            t_cbet_call: row.get(43)?,
            t_cbet_raise: row.get(44)?,
            f_xr_opp: row.get(45)?,
            f_xr: row.get(46)?,
            p_bets: row.get(47)?,
            p_raises: row.get(48)?,
            p_calls: row.get(49)?,
            p_folds: row.get(50)?,
            f_bets: row.get(51)?,
            f_raises: row.get(52)?,
            f_calls: row.get(53)?,
            f_folds: row.get(54)?,
            t_bets: row.get(55)?,
            t_raises: row.get(56)?,
            t_calls: row.get(57)?,
            t_folds: row.get(58)?,
            r_bets: row.get(59)?,
            r_raises: row.get(60)?,
            r_calls: row.get(61)?,
            r_folds: row.get(62)?,
        };
        by_currency.insert(currency, stats);
    }

    let mut rows = Vec::new();

    for (currency, s) in by_currency {
        // Base 29 stats (all percents)
        let vpip_den = s.hands.saturating_sub(s.p_walk);
        let stat_definitions: [(&str, i64, i64); 29] = [
            ("vpip", s.p_vpip, vpip_den),
            ("pfr", s.p_pfr, s.p_pfr_opp),
            ("saw_flop", s.f_saw, s.hands),
            ("wtsd", s.sd_went, s.f_saw),
            ("wsd", s.sd_won, s.sd_went),
            ("wwsf", s.f_saw_won, s.f_saw),
            ("win", s.won_hand, s.hands),
            ("rfi", s.p_rfi, s.p_rfi_opp),
            ("three_bet_pf", s.p_3bet, s.p_3bet_opp),
            ("fold_to_three_bet_pf", s.p_3bet_fold, s.p_3bet_faced),
            ("call_three_bet_pf", s.p_3bet_call, s.p_3bet_faced),
            ("four_bet_pf", s.p_4bet, s.p_4bet_opp),
            ("fold_to_four_bet_pf", s.p_4bet_fold, s.p_4bet_faced),
            ("call_four_bet_pf", s.p_4bet_call, s.p_4bet_faced),
            ("cold_call_pf", s.p_cc, s.p_cc_opp),
            ("att_to_steal", s.p_steal, s.p_steal_opp),
            ("fold_sb_to_steal", s.p_sb_steal_fold, s.p_sb_steal_faced),
            ("raise_sb_steal", s.p_sb_steal_raise, s.p_sb_steal_faced),
            ("fold_bb_to_steal", s.p_bb_steal_fold, s.p_bb_steal_faced),
            ("raise_bb_steal", s.p_bb_steal_raise, s.p_bb_steal_faced),
            ("cbet_flop", s.f_cbet, s.f_cbet_opp),
            ("fold_to_cbet_flop", s.f_cbet_fold, s.f_cbet_faced),
            ("call_cbet_flop", s.f_cbet_call, s.f_cbet_faced),
            ("raise_cbet_flop", s.f_cbet_raise, s.f_cbet_faced),
            ("cbet_turn", s.t_cbet, s.t_cbet_opp),
            ("fold_to_cbet_turn", s.t_cbet_fold, s.t_cbet_faced),
            ("call_cbet_turn", s.t_cbet_call, s.t_cbet_faced),
            ("raise_cbet_turn", s.t_cbet_raise, s.t_cbet_faced),
            ("check_raise_flop", s.f_xr, s.f_xr_opp),
        ];

        for (name, num, den) in stat_definitions {
            rows.push(StatsRow {
                currency: currency.clone(),
                stat: name.to_string(),
                numerator: num,
                denominator: den,
                value: format_pct(num, den),
            });
        }

        // Aggression Factors (af_*) - factors, not percents
        let p_agg = s.p_bets.saturating_add(s.p_raises);
        let f_agg = s.f_bets.saturating_add(s.f_raises);
        let t_agg = s.t_bets.saturating_add(s.t_raises);
        let r_agg = s.r_bets.saturating_add(s.r_raises);
        let postflop_agg = f_agg.saturating_add(t_agg).saturating_add(r_agg);
        let postflop_calls = s
            .f_calls
            .saturating_add(s.t_calls)
            .saturating_add(s.r_calls);

        let af_definitions: [(&str, i64, i64); 5] = [
            ("af_pf", p_agg, s.p_calls),
            ("af_flop", f_agg, s.f_calls),
            ("af_turn", t_agg, s.t_calls),
            ("af_river", r_agg, s.r_calls),
            ("af_total", postflop_agg, postflop_calls),
        ];

        for (name, num, den) in af_definitions {
            rows.push(StatsRow {
                currency: currency.clone(),
                stat: name.to_string(),
                numerator: num,
                denominator: den,
                value: format_factor(num, den),
            });
        }

        // Aggression Frequency (afq_*) - percents
        let p_afq_den = p_agg.saturating_add(s.p_calls).saturating_add(s.p_folds);
        let f_afq_den = f_agg.saturating_add(s.f_calls).saturating_add(s.f_folds);
        let t_afq_den = t_agg.saturating_add(s.t_calls).saturating_add(s.t_folds);
        let r_afq_den = r_agg.saturating_add(s.r_calls).saturating_add(s.r_folds);

        let postflop_folds = s
            .f_folds
            .saturating_add(s.t_folds)
            .saturating_add(s.r_folds);
        let total_afq_den = postflop_agg
            .saturating_add(postflop_calls)
            .saturating_add(postflop_folds);

        let afq_definitions: [(&str, i64, i64); 5] = [
            ("afq_pf", p_agg, p_afq_den),
            ("afq_flop", f_agg, f_afq_den),
            ("afq_turn", t_agg, t_afq_den),
            ("afq_river", r_agg, r_afq_den),
            ("afq_total", postflop_agg, total_afq_den),
        ];

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
