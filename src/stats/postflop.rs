//! Postflop decision replay and opportunity counters.

use std::collections::{HashMap, HashSet};

use rust_decimal::Decimal;

use crate::domain::canonical::{ActionType, CanonicalHand, Street};

use super::{PlayerHandStats, ReplayState};

pub(crate) fn replay_postflop<'a>(
    hand: &'a CanonicalHand,
    stats: &mut [PlayerHandStats],
    state: &mut ReplayState<'a>,
    preflop_aggressor: Option<&'a str>,
) {
    let mut flop_cbet_by: Option<&'a str> = None;

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
            if !state.folded.contains(n)
                && let Some(&idx) = state.name_to_idx.get(n)
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

        state.committed.clear();
        let mut bets: u32 = 0;
        let mut player_decisions: HashMap<&'a str, usize> = HashMap::new();

        // Flop-specific state
        let mut flop_cbet_occurred = false;
        let mut raises_since_flop_cbet: u32 = 0;
        let mut faced_flop_cbet_seen: HashSet<&'a str> = HashSet::new();
        let mut flop_first_decision_is_check: HashSet<&'a str> = HashSet::new();
        let mut flop_xr_opp_seen: HashSet<&'a str> = HashSet::new();

        // Turn-specific state
        let mut turn_cbet_by: Option<&'a str> = None;
        let mut turn_cbet_occurred = false;
        let mut raises_since_turn_cbet: u32 = 0;
        let mut faced_turn_cbet_seen: HashSet<&'a str> = HashSet::new();

        for a in &hand.actions {
            if a.street != street {
                continue;
            }
            let n = a.player_name.as_str();

            if a.action_type == ActionType::UncalledBetReturned {
                state.committed.insert(n, a.street_total_after);
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
                let facing = state
                    .committed
                    .values()
                    .copied()
                    .fold(Decimal::ZERO, Decimal::max);
                let player_comm = state.committed.get(n).copied().unwrap_or(Decimal::ZERO);
                let to_call = facing.saturating_sub(player_comm);
                let player_gross = state.gross.get(n).copied().unwrap_or(Decimal::ZERO);
                let player_stack = state.stack.get(n).copied().unwrap_or(Decimal::ZERO);
                let remaining = player_stack.saturating_sub(player_gross);
                let others_can_act = hand.players.iter().any(|p| {
                    p.name != n
                        && !state.folded.contains(p.name.as_str())
                        && !state.all_in.contains(p.name.as_str())
                });
                let can_bet =
                    to_call == Decimal::ZERO && remaining > Decimal::ZERO && others_can_act;

                let dec_count = player_decisions.entry(n).or_insert(0);
                let is_first_street_decision = *dec_count == 0;
                *dec_count += 1;

                if let Some(&idx) = state.name_to_idx.get(n)
                    && let Some(s) = stats.get_mut(idx)
                {
                    match street {
                        Street::Flop => {
                            // Flop c-bet opp & c-bet
                            if is_first_street_decision
                                && preflop_aggressor == Some(n)
                                && bets == 0
                                && can_bet
                            {
                                s.f_cbet_opp = 1;
                                if a.action_type == ActionType::Bet {
                                    s.f_cbet = 1;
                                    flop_cbet_occurred = true;
                                    flop_cbet_by = Some(n);
                                }
                            }

                            // Facing flop c-bet
                            if flop_cbet_occurred
                                && flop_cbet_by != Some(n)
                                && raises_since_flop_cbet == 0
                            {
                                s.f_cbet_faced = 1;
                                if faced_flop_cbet_seen.insert(n) {
                                    match a.action_type {
                                        ActionType::Fold => s.f_cbet_fold = 1,
                                        ActionType::Call => s.f_cbet_call = 1,
                                        ActionType::Raise => s.f_cbet_raise = 1,
                                        _ => {}
                                    }
                                }
                            }

                            // Flop check-raise
                            if is_first_street_decision {
                                if a.action_type == ActionType::Check {
                                    flop_first_decision_is_check.insert(n);
                                }
                            } else if flop_first_decision_is_check.contains(n)
                                && to_call > Decimal::ZERO
                                && flop_xr_opp_seen.insert(n)
                            {
                                s.f_xr_opp = 1;
                                if a.action_type == ActionType::Raise {
                                    s.f_xr = 1;
                                }
                            }

                            // Action counts & replay trackers
                            match a.action_type {
                                ActionType::Fold => s.f_folds += 1,
                                ActionType::Check => s.f_checks += 1,
                                ActionType::Call => s.f_calls += 1,
                                ActionType::Bet => {
                                    s.f_bets += 1;
                                    bets += 1;
                                }
                                ActionType::Raise => {
                                    s.f_raises += 1;
                                    if flop_cbet_occurred {
                                        raises_since_flop_cbet += 1;
                                    }
                                }
                                _ => {}
                            }
                        }
                        Street::Turn => {
                            // Turn c-bet opp & c-bet
                            if is_first_street_decision
                                && flop_cbet_by == Some(n)
                                && bets == 0
                                && can_bet
                            {
                                s.t_cbet_opp = 1;
                                if a.action_type == ActionType::Bet {
                                    s.t_cbet = 1;
                                    turn_cbet_occurred = true;
                                    turn_cbet_by = Some(n);
                                }
                            }

                            // Facing turn c-bet
                            if turn_cbet_occurred
                                && turn_cbet_by != Some(n)
                                && raises_since_turn_cbet == 0
                            {
                                s.t_cbet_faced = 1;
                                if faced_turn_cbet_seen.insert(n) {
                                    match a.action_type {
                                        ActionType::Fold => s.t_cbet_fold = 1,
                                        ActionType::Call => s.t_cbet_call = 1,
                                        ActionType::Raise => s.t_cbet_raise = 1,
                                        _ => {}
                                    }
                                }
                            }

                            // Action counts & replay trackers
                            match a.action_type {
                                ActionType::Fold => s.t_folds += 1,
                                ActionType::Check => s.t_checks += 1,
                                ActionType::Call => s.t_calls += 1,
                                ActionType::Bet => {
                                    s.t_bets += 1;
                                    bets += 1;
                                }
                                ActionType::Raise => {
                                    s.t_raises += 1;
                                    if turn_cbet_occurred {
                                        raises_since_turn_cbet += 1;
                                    }
                                }
                                _ => {}
                            }
                        }
                        Street::River => match a.action_type {
                            ActionType::Fold => s.r_folds += 1,
                            ActionType::Check => s.r_checks += 1,
                            ActionType::Call => s.r_calls += 1,
                            ActionType::Bet => s.r_bets += 1,
                            ActionType::Raise => s.r_raises += 1,
                            _ => {}
                        },
                        _ => {}
                    }
                }

                if a.action_type.is_contribution() {
                    let g = state.gross.entry(n).or_default();
                    *g = g.saturating_add(a.amount);
                    let st = state.stack.get(n).copied().unwrap_or(Decimal::ZERO);
                    if *g >= st || a.is_all_in {
                        state.all_in.insert(n);
                    }
                }
                state.committed.insert(n, a.street_total_after);
                if a.action_type == ActionType::Fold {
                    state.folded.insert(n);
                }
            }
        }
    }
}
