//! Derived statistics and player position determination.
//!
//! Pure, site-neutral functions that compute player positions and counters from a
//! [`CanonicalHand`].

mod postflop;
mod preflop;

use std::collections::{HashMap, HashSet};

use rust_decimal::Decimal;

use crate::domain::canonical::{ActionType, CanonicalHand};

/// Current version of the derived statistics cache schema and definitions.
pub const STATS_VERSION: i64 = 2;

macro_rules! define_stats_columns {
    ($( $col:ident ),* $(,)?) => {
        /// Derived statistics cache columns in schema order.
        pub const COLUMNS: &[&str] = &[
            $( stringify!($col) ),*
        ];

        /// Statistics and counters for one player in one hand.
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct PlayerHandStats {
            pub player_name: String,
            pub position: Option<u8>,
            $( pub $col: i64, )*
        }

        impl PlayerHandStats {
            /// Initializes player stats with default zero counters (`hands = 1`).
            #[must_use]
            pub fn new(player_name: String, position: Option<u8>) -> Self {
                let mut stats = Self {
                    player_name,
                    position,
                    $( $col: 0, )*
                };
                stats.hands = 1;
                stats
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
                    $( stringify!($col) => Some(self.$col), )*
                    _ => None,
                }
            }
        }
    };
}

define_stats_columns! {
    // Preflop (stats.md §4.1)
    hands,
    p_walk,
    p_vpip,
    p_pfr_opp,
    p_pfr,
    p_rfi_opp,
    p_rfi,
    p_3bet_opp,
    p_3bet,
    p_3bet_faced,
    p_3bet_fold,
    p_3bet_call,
    p_4bet_opp,
    p_4bet,
    p_4bet_faced,
    p_4bet_fold,
    p_4bet_call,
    p_cc_opp,
    p_cc,
    p_steal_opp,
    p_steal,
    p_sb_steal_faced,
    p_sb_steal_fold,
    p_sb_steal_raise,
    p_bb_steal_faced,
    p_bb_steal_fold,
    p_bb_steal_raise,

    // Postflop (stats.md §4.2)
    f_saw,
    t_saw,
    r_saw,
    f_cbet_opp,
    f_cbet,
    f_cbet_faced,
    f_cbet_fold,
    f_cbet_call,
    f_cbet_raise,
    t_cbet_opp,
    t_cbet,
    t_cbet_faced,
    t_cbet_fold,
    t_cbet_call,
    t_cbet_raise,
    f_xr_opp,
    f_xr,

    // Action counts and outcome (stats.md §4.3)
    sd_went,
    sd_won,
    won_hand,
    f_saw_won,
    p_bets,
    p_raises,
    p_calls,
    p_checks,
    p_folds,
    f_bets,
    f_raises,
    f_calls,
    f_checks,
    f_folds,
    t_bets,
    t_raises,
    t_calls,
    t_checks,
    t_folds,
    r_bets,
    r_raises,
    r_calls,
    r_checks,
    r_folds,
}

pub(crate) struct ReplayState<'a> {
    pub name_to_idx: HashMap<&'a str, usize>,
    pub stack: HashMap<&'a str, Decimal>,
    pub gross: HashMap<&'a str, Decimal>,
    pub committed: HashMap<&'a str, Decimal>,
    pub folded: HashSet<&'a str>,
    pub all_in: HashSet<&'a str>,
}

impl<'a> ReplayState<'a> {
    pub fn new(hand: &'a CanonicalHand) -> Self {
        let mut name_to_idx = HashMap::with_capacity(hand.players.len());
        let mut stack = HashMap::with_capacity(hand.players.len());
        let mut gross = HashMap::with_capacity(hand.players.len());
        for (idx, p) in hand.players.iter().enumerate() {
            name_to_idx.insert(p.name.as_str(), idx);
            stack.insert(p.name.as_str(), p.starting_stack);
            gross.insert(p.name.as_str(), Decimal::ZERO);
        }
        Self {
            name_to_idx,
            stack,
            gross,
            committed: HashMap::new(),
            folded: HashSet::new(),
            all_in: HashSet::new(),
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

    let mut state = ReplayState::new(hand);

    let preflop_res = preflop::replay_preflop(hand, &mut stats, &mut state);

    // Walk check: every other player folded preflop and this player never had to decide
    if hand.players.len() >= 2 && state.folded.len() == hand.players.len() - 1 {
        for p in &hand.players {
            let n = p.name.as_str();
            if !state.folded.contains(n)
                && preflop_res.preflop_decisions.get(n).copied().unwrap_or(0) == 0
                && let Some(&idx) = state.name_to_idx.get(n)
                && let Some(s) = stats.get_mut(idx)
            {
                s.p_walk = 1;
            }
        }
    }

    postflop::replay_postflop(hand, &mut stats, &mut state, preflop_res.preflop_aggressor);

    // Outcome
    let live_count = hand
        .players
        .iter()
        .filter(|p| !state.folded.contains(p.name.as_str()))
        .count();
    for p in &hand.players {
        let n = p.name.as_str();
        let won = hand
            .outcome
            .winners
            .iter()
            .filter(|w| w.player_name == n)
            .fold(Decimal::ZERO, |acc, w| acc.saturating_add(w.amount));

        if let Some(&idx) = state.name_to_idx.get(n)
            && let Some(s) = stats.get_mut(idx)
        {
            if won > Decimal::ZERO {
                s.won_hand = 1;
                if s.f_saw == 1 {
                    s.f_saw_won = 1;
                }
            }
            if live_count >= 2 && !state.folded.contains(n) {
                s.sd_went = 1;
                if won > Decimal::ZERO {
                    s.sd_won = 1;
                }
            }
        }
    }

    stats
}
