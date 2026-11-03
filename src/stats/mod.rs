//! Derived statistics and player position determination.
//!
//! Pure, site-neutral functions that compute player positions and counters from a
//! [`CanonicalHand`].

use std::collections::{HashMap, HashSet};

use rust_decimal::Decimal;

use crate::domain::canonical::{ActionType, CanonicalHand, Street};

/// Current version of the derived statistics cache schema and definitions.
pub const STATS_VERSION: i64 = 1;

/// Derived statistics cache columns in schema order.
pub const COLUMNS: &[&str] = &[
    "hands",
    "p_walk",
    "p_vpip",
    "p_pfr_opp",
    "p_pfr",
    "f_saw",
    "t_saw",
    "r_saw",
    "sd_went",
    "sd_won",
    "won_hand",
    "f_saw_won",
    "p_bets",
    "p_raises",
    "p_calls",
    "p_checks",
    "p_folds",
    "f_bets",
    "f_raises",
    "f_calls",
    "f_checks",
    "f_folds",
    "t_bets",
    "t_raises",
    "t_calls",
    "t_checks",
    "t_folds",
    "r_bets",
    "r_raises",
    "r_calls",
    "r_checks",
    "r_folds",
];

/// Statistics and counters for one player in one hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerHandStats {
    pub player_name: String,
    pub position: Option<u8>,
    pub hands: i64,
    pub p_walk: i64,
    pub p_vpip: i64,
    pub p_pfr_opp: i64,
    pub p_pfr: i64,
    pub f_saw: i64,
    pub t_saw: i64,
    pub r_saw: i64,
    pub sd_went: i64,
    pub sd_won: i64,
    pub won_hand: i64,
    pub f_saw_won: i64,
    pub p_bets: i64,
    pub p_raises: i64,
    pub p_calls: i64,
    pub p_checks: i64,
    pub p_folds: i64,
    pub f_bets: i64,
    pub f_raises: i64,
    pub f_calls: i64,
    pub f_checks: i64,
    pub f_folds: i64,
    pub t_bets: i64,
    pub t_raises: i64,
    pub t_calls: i64,
    pub t_checks: i64,
    pub t_folds: i64,
    pub r_bets: i64,
    pub r_raises: i64,
    pub r_calls: i64,
    pub r_checks: i64,
    pub r_folds: i64,
}

impl PlayerHandStats {
    /// Initializes player stats with default zero counters.
    #[must_use]
    pub fn new(player_name: String, position: Option<u8>) -> Self {
        Self {
            player_name,
            position,
            hands: 1,
            p_walk: 0,
            p_vpip: 0,
            p_pfr_opp: 0,
            p_pfr: 0,
            f_saw: 0,
            t_saw: 0,
            r_saw: 0,
            sd_went: 0,
            sd_won: 0,
            won_hand: 0,
            f_saw_won: 0,
            p_bets: 0,
            p_raises: 0,
            p_calls: 0,
            p_checks: 0,
            p_folds: 0,
            f_bets: 0,
            f_raises: 0,
            f_calls: 0,
            f_checks: 0,
            f_folds: 0,
            t_bets: 0,
            t_raises: 0,
            t_calls: 0,
            t_checks: 0,
            t_folds: 0,
            r_bets: 0,
            r_raises: 0,
            r_calls: 0,
            r_checks: 0,
            r_folds: 0,
        }
    }

    /// Name of the player.
    #[must_use]
    pub fn player_name(&self) -> &str {
        &self.player_name
    }

    /// Assigned position code (0..=9), if standard layout held.
    #[must_use]
    pub fn position(&self) -> Option<u8> {
        self.position
    }

    /// Looks up a counter value by column name.
    #[must_use]
    pub fn get(&self, column: &str) -> Option<i64> {
        match column {
            "hands" => Some(self.hands),
            "p_walk" => Some(self.p_walk),
            "p_vpip" => Some(self.p_vpip),
            "p_pfr_opp" => Some(self.p_pfr_opp),
            "p_pfr" => Some(self.p_pfr),
            "f_saw" => Some(self.f_saw),
            "t_saw" => Some(self.t_saw),
            "r_saw" => Some(self.r_saw),
            "sd_went" => Some(self.sd_went),
            "sd_won" => Some(self.sd_won),
            "won_hand" => Some(self.won_hand),
            "f_saw_won" => Some(self.f_saw_won),
            "p_bets" => Some(self.p_bets),
            "p_raises" => Some(self.p_raises),
            "p_calls" => Some(self.p_calls),
            "p_checks" => Some(self.p_checks),
            "p_folds" => Some(self.p_folds),
            "f_bets" => Some(self.f_bets),
            "f_raises" => Some(self.f_raises),
            "f_calls" => Some(self.f_calls),
            "f_checks" => Some(self.f_checks),
            "f_folds" => Some(self.f_folds),
            "t_bets" => Some(self.t_bets),
            "t_raises" => Some(self.t_raises),
            "t_calls" => Some(self.t_calls),
            "t_checks" => Some(self.t_checks),
            "t_folds" => Some(self.t_folds),
            "r_bets" => Some(self.r_bets),
            "r_raises" => Some(self.r_raises),
            "r_calls" => Some(self.r_calls),
            "r_checks" => Some(self.r_checks),
            "r_folds" => Some(self.r_folds),
            _ => None,
        }
    }
}

/// Maps a position code and table size to a human-readable position label.
#[must_use]
pub fn position_label(code: u8, max_seats: u8) -> &'static str {
    match code {
        9 => "SB",
        8 => "BB",
        0 => "BTN",
        1 => "CO",
        2 => "MP",
        3 | 4 => {
            if max_seats <= 6 {
                "EP"
            } else {
                "MP"
            }
        }
        5..=7 => "EP",
        _ => "?",
    }
}

/// Assigns position codes (0..=9) to seats in a hand, or empty map if standard layout fails.
#[must_use]
pub fn assign_positions(hand: &CanonicalHand) -> HashMap<u8, u8> {
    if hand.players.len() < 2 {
        return HashMap::new();
    }
    let mut seats: Vec<u8> = hand.players.iter().map(|p| p.seat).collect();
    seats.sort_unstable();

    let btn = hand.header.button_seat;
    if !seats.contains(&btn) {
        return HashMap::new();
    }

    let mut sbs = Vec::new();
    let mut bbs = Vec::new();
    for a in &hand.actions {
        if a.action_type == ActionType::PostSmallBlind {
            sbs.push(a.player_name.as_str());
        } else if a.action_type == ActionType::PostBigBlind {
            bbs.push(a.player_name.as_str());
        }
    }

    if sbs.len() != 1 || bbs.is_empty() {
        return HashMap::new();
    }

    let Some(&sb_poster) = sbs.first() else {
        return HashMap::new();
    };
    let Some(&bb_poster) = bbs.first() else {
        return HashMap::new();
    };

    let Some(sb_seat) = hand
        .players
        .iter()
        .find(|p| p.name == sb_poster)
        .map(|p| p.seat)
    else {
        return HashMap::new();
    };
    let Some(bb_seat) = hand
        .players
        .iter()
        .find(|p| p.name == bb_poster)
        .map(|p| p.seat)
    else {
        return HashMap::new();
    };

    let cw_after = |start: u8| -> Vec<u8> {
        let mut after = Vec::with_capacity(seats.len());
        for &s in &seats {
            if s > start {
                after.push(s);
            }
        }
        for &s in &seats {
            if s <= start {
                after.push(s);
            }
        }
        after
    };

    if seats.len() == 2 {
        if sb_seat != btn || bb_seat == btn {
            return HashMap::new();
        }
    } else {
        let cw_btn = cw_after(btn);
        if cw_btn.first().copied() != Some(sb_seat) {
            return HashMap::new();
        }
        let cw_sb = cw_after(sb_seat);
        if cw_sb.first().copied() != Some(bb_seat) {
            return HashMap::new();
        }
    }

    let mut pos = HashMap::with_capacity(seats.len());
    pos.insert(sb_seat, 9);
    pos.insert(bb_seat, 8);

    let cw_btn = cw_after(btn);
    let mut ccw = Vec::with_capacity(seats.len());
    ccw.push(btn);
    if cw_btn.len() > 1 {
        for &s in cw_btn[..cw_btn.len() - 1].iter().rev() {
            ccw.push(s);
        }
    }

    let mut code = 0;
    for s in ccw {
        if pos.contains_key(&s) {
            continue;
        }
        pos.insert(s, code);
        code += 1;
    }

    pos
}

/// Derives player stats and counters for all seated players in `hand`.
///
/// Infallible and panic-free for any hand storage can load.
#[must_use]
pub fn derive_hand_stats(hand: &CanonicalHand) -> Vec<PlayerHandStats> {
    if hand.players.is_empty() {
        return Vec::new();
    }

    let pos_map = assign_positions(hand);
    let mut stats: Vec<PlayerHandStats> = hand
        .players
        .iter()
        .map(|p| PlayerHandStats::new(p.name.clone(), pos_map.get(&p.seat).copied()))
        .collect();

    let mut name_to_idx = HashMap::with_capacity(hand.players.len());
    let mut stack = HashMap::with_capacity(hand.players.len());
    let mut gross = HashMap::with_capacity(hand.players.len());
    for (idx, p) in hand.players.iter().enumerate() {
        name_to_idx.insert(p.name.as_str(), idx);
        stack.insert(p.name.as_str(), p.starting_stack);
        gross.insert(p.name.as_str(), Decimal::ZERO);
    }

    let mut folded = HashSet::new();
    let mut all_in = HashSet::new();
    let mut committed = HashMap::new();
    let mut preflop_decisions = HashMap::new();

    // Preflop replay
    for a in &hand.actions {
        if a.street != Street::Preflop {
            continue;
        }
        let n = a.player_name.as_str();
        if matches!(
            a.action_type,
            ActionType::PostSmallBlind
                | ActionType::PostBigBlind
                | ActionType::PostDeadBlind
                | ActionType::PostStraddle
        ) {
            let g = gross.entry(n).or_default();
            *g = g.saturating_add(a.amount);
            let st = stack.get(n).copied().unwrap_or(Decimal::ZERO);
            if *g >= st || a.is_all_in {
                all_in.insert(n);
            }
            committed.insert(n, a.street_total_after);
            continue;
        }

        if a.action_type == ActionType::UncalledBetReturned {
            committed.insert(n, a.street_total_after);
            continue;
        }

        if matches!(
            a.action_type,
            ActionType::Fold
                | ActionType::Check
                | ActionType::Call
                | ActionType::Bet
                | ActionType::Raise
        ) {
            let facing = committed
                .values()
                .copied()
                .fold(Decimal::ZERO, Decimal::max);
            let player_comm = committed.get(n).copied().unwrap_or(Decimal::ZERO);
            let to_call = facing.saturating_sub(player_comm);
            let player_gross = gross.get(n).copied().unwrap_or(Decimal::ZERO);
            let player_stack = stack.get(n).copied().unwrap_or(Decimal::ZERO);
            let remaining = player_stack.saturating_sub(player_gross);
            let others_can_act = hand.players.iter().any(|p| {
                p.name != n
                    && !folded.contains(p.name.as_str())
                    && !all_in.contains(p.name.as_str())
            });
            let can_raise = remaining > to_call && others_can_act;

            if let Some(&idx) = name_to_idx.get(n)
                && let Some(s) = stats.get_mut(idx)
            {
                *preflop_decisions.entry(n).or_insert(0usize) += 1;
                match a.action_type {
                    ActionType::Fold => s.p_folds += 1,
                    ActionType::Check => s.p_checks += 1,
                    ActionType::Call => {
                        s.p_calls += 1;
                        s.p_vpip = 1;
                    }
                    ActionType::Bet => {
                        s.p_bets += 1;
                    }
                    ActionType::Raise => {
                        s.p_raises += 1;
                        s.p_vpip = 1;
                        s.p_pfr = 1;
                    }
                    _ => {}
                }
                if can_raise {
                    s.p_pfr_opp = 1;
                }
            }

            if a.action_type.is_contribution() {
                let g = gross.entry(n).or_default();
                *g = g.saturating_add(a.amount);
                let st = stack.get(n).copied().unwrap_or(Decimal::ZERO);
                if *g >= st || a.is_all_in {
                    all_in.insert(n);
                }
            }
            committed.insert(n, a.street_total_after);
            if a.action_type == ActionType::Fold {
                folded.insert(n);
            }
        }
    }

    // Walk check: every other player folded preflop and this player never had to decide
    if hand.players.len() >= 2 && folded.len() == hand.players.len() - 1 {
        for p in &hand.players {
            let n = p.name.as_str();
            if !folded.contains(n)
                && preflop_decisions.get(n).copied().unwrap_or(0) == 0
                && let Some(&idx) = name_to_idx.get(n)
                && let Some(s) = stats.get_mut(idx)
            {
                s.p_walk = 1;
            }
        }
    }

    // Postflop replay
    for street in [Street::Flop, Street::Turn, Street::River] {
        let dealt = hand.boards.iter().any(|b| match street {
            Street::Flop => b.flop.is_some(),
            Street::Turn => b.turn.is_some(),
            Street::River => b.river.is_some(),
            _ => false,
        });
        if !dealt {
            break;
        }

        for p in &hand.players {
            let n = p.name.as_str();
            if !folded.contains(n)
                && let Some(&idx) = name_to_idx.get(n)
                && let Some(s) = stats.get_mut(idx)
            {
                match street {
                    Street::Flop => s.f_saw = 1,
                    Street::Turn => s.t_saw = 1,
                    Street::River => s.r_saw = 1,
                    _ => {}
                }
            }
        }

        committed.clear();
        for a in &hand.actions {
            if a.street != street {
                continue;
            }
            let n = a.player_name.as_str();
            if a.action_type == ActionType::UncalledBetReturned {
                committed.insert(n, a.street_total_after);
                continue;
            }
            if matches!(
                a.action_type,
                ActionType::Fold
                    | ActionType::Check
                    | ActionType::Call
                    | ActionType::Bet
                    | ActionType::Raise
            ) {
                if let Some(&idx) = name_to_idx.get(n)
                    && let Some(s) = stats.get_mut(idx)
                {
                    match (street, a.action_type) {
                        (Street::Flop, ActionType::Fold) => s.f_folds += 1,
                        (Street::Flop, ActionType::Check) => s.f_checks += 1,
                        (Street::Flop, ActionType::Call) => s.f_calls += 1,
                        (Street::Flop, ActionType::Bet) => s.f_bets += 1,
                        (Street::Flop, ActionType::Raise) => s.f_raises += 1,
                        (Street::Turn, ActionType::Fold) => s.t_folds += 1,
                        (Street::Turn, ActionType::Check) => s.t_checks += 1,
                        (Street::Turn, ActionType::Call) => s.t_calls += 1,
                        (Street::Turn, ActionType::Bet) => s.t_bets += 1,
                        (Street::Turn, ActionType::Raise) => s.t_raises += 1,
                        (Street::River, ActionType::Fold) => s.r_folds += 1,
                        (Street::River, ActionType::Check) => s.r_checks += 1,
                        (Street::River, ActionType::Call) => s.r_calls += 1,
                        (Street::River, ActionType::Bet) => s.r_bets += 1,
                        (Street::River, ActionType::Raise) => s.r_raises += 1,
                        _ => {}
                    }
                }

                if a.action_type.is_contribution() {
                    let g = gross.entry(n).or_default();
                    *g = g.saturating_add(a.amount);
                    let st = stack.get(n).copied().unwrap_or(Decimal::ZERO);
                    if *g >= st || a.is_all_in {
                        all_in.insert(n);
                    }
                }
                committed.insert(n, a.street_total_after);
                if a.action_type == ActionType::Fold {
                    folded.insert(n);
                }
            }
        }
    }

    // Outcome
    let live_count = hand
        .players
        .iter()
        .filter(|p| !folded.contains(p.name.as_str()))
        .count();
    for p in &hand.players {
        let n = p.name.as_str();
        let won = hand
            .outcome
            .winners
            .iter()
            .filter(|w| w.player_name == n)
            .fold(Decimal::ZERO, |acc, w| acc.saturating_add(w.amount));

        if let Some(&idx) = name_to_idx.get(n)
            && let Some(s) = stats.get_mut(idx)
        {
            if won > Decimal::ZERO {
                s.won_hand = 1;
                if s.f_saw == 1 {
                    s.f_saw_won = 1;
                }
            }
            if live_count >= 2 && !folded.contains(n) {
                s.sd_went = 1;
                if won > Decimal::ZERO {
                    s.sd_won = 1;
                }
            }
        }
    }

    stats
}
