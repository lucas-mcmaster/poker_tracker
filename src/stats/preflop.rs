//! Preflop decision replay and opportunity counters.

use std::collections::{HashMap, HashSet};

use rust_decimal::Decimal;

use crate::domain::canonical::{ActionType, CanonicalHand, Street};

use super::{PlayerHandStats, ReplayState};

pub(crate) struct PreflopOutcome<'a> {
    pub preflop_aggressor: Option<&'a str>,
    pub preflop_decisions: HashMap<&'a str, usize>,
}

pub(crate) fn replay_preflop<'a>(
    hand: &'a CanonicalHand,
    stats: &mut [PlayerHandStats],
    state: &mut ReplayState<'a>,
) -> PreflopOutcome<'a> {
    let mut had_forced_post = HashSet::new();
    let mut has_straddle = false;
    for a in &hand.actions {
        if a.street != Street::Preflop {
            continue;
        }
        if matches!(
            a.action_type,
            ActionType::PostSmallBlind
                | ActionType::PostBigBlind
                | ActionType::PostDeadBlind
                | ActionType::PostStraddle
        ) {
            had_forced_post.insert(a.player_name.as_str());
            if a.action_type == ActionType::PostStraddle {
                has_straddle = true;
            }
        }
    }

    let mut raises: u32 = 0;
    let mut calls: u32 = 0;
    let mut preflop_decisions: HashMap<&'a str, usize> = HashMap::new();
    let mut first_decision_raises_eq_1_seen: HashSet<&'a str> = HashSet::new();
    let mut first_decision_raises_eq_2_seen: HashSet<&'a str> = HashSet::new();
    let mut first_decision_raises_ge_2_seen: HashSet<&'a str> = HashSet::new();
    let mut first_decision_raises_ge_3_seen: HashSet<&'a str> = HashSet::new();
    let mut steal_attempt_by: Option<&'a str> = None;
    let mut preflop_aggressor: Option<&'a str> = None;

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
            let g = state.gross.entry(n).or_default();
            *g = g.saturating_add(a.amount);
            let st = state.stack.get(n).copied().unwrap_or(Decimal::ZERO);
            if *g >= st || a.is_all_in {
                state.all_in.insert(n);
            }
            state.committed.insert(n, a.street_total_after);
            continue;
        }

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
            let can_raise = remaining > to_call && others_can_act;

            let dec_count = preflop_decisions.entry(n).or_insert(0);
            let is_first_decision = *dec_count == 0;
            *dec_count += 1;

            if let Some(&idx) = state.name_to_idx.get(n)
                && let Some(s) = stats.get_mut(idx)
            {
                let pos = s.position;

                // RFI & Steal opp / attempt
                if is_first_decision && raises == 0 && calls == 0 && can_raise {
                    s.p_rfi_opp = 1;
                    if a.action_type == ActionType::Raise {
                        s.p_rfi = 1;
                    }
                    if !has_straddle && matches!(pos, Some(0) | Some(1) | Some(9)) {
                        s.p_steal_opp = 1;
                        if a.action_type == ActionType::Raise {
                            s.p_steal = 1;
                        }
                    }
                }

                // Steal attempt tracking: first preflop Raise if made at p_steal_opp
                if a.action_type == ActionType::Raise && raises == 0 && s.p_steal_opp == 1 {
                    steal_attempt_by = Some(n);
                }

                // 3-Bet opp & 3-Bet
                if raises == 1 && first_decision_raises_eq_1_seen.insert(n) && can_raise {
                    s.p_3bet_opp = 1;
                    if a.action_type == ActionType::Raise {
                        s.p_3bet = 1;
                    }
                }

                // 3-Bet faced, fold, call
                if raises == 2 {
                    s.p_3bet_faced = 1;
                    if first_decision_raises_eq_2_seen.insert(n) {
                        match a.action_type {
                            ActionType::Fold => s.p_3bet_fold = 1,
                            ActionType::Call => s.p_3bet_call = 1,
                            _ => {}
                        }
                    }
                }

                // 4-Bet opp & 4-Bet
                if raises >= 2 && first_decision_raises_ge_2_seen.insert(n) && can_raise {
                    s.p_4bet_opp = 1;
                    if a.action_type == ActionType::Raise {
                        s.p_4bet = 1;
                    }
                }

                // 4-Bet faced, fold, call
                if raises >= 3 {
                    s.p_4bet_faced = 1;
                    if first_decision_raises_ge_3_seen.insert(n)
                        && a.action_type == ActionType::Call
                    {
                        s.p_4bet_call = 1;
                    }
                    if a.action_type == ActionType::Fold {
                        s.p_4bet_fold = 1;
                    }
                }

                // Cold call opp & cc
                if is_first_decision && raises >= 1 && !had_forced_post.contains(n) {
                    s.p_cc_opp = 1;
                    if a.action_type == ActionType::Call {
                        s.p_cc = 1;
                    }
                }

                // Steal defence
                if is_first_decision
                    && let Some(thief) = steal_attempt_by
                    && thief != n
                    && raises == 1
                    && calls == 0
                {
                    if pos == Some(9) {
                        s.p_sb_steal_faced = 1;
                        match a.action_type {
                            ActionType::Fold => s.p_sb_steal_fold = 1,
                            ActionType::Raise => s.p_sb_steal_raise = 1,
                            _ => {}
                        }
                    } else if pos == Some(8) {
                        s.p_bb_steal_faced = 1;
                        match a.action_type {
                            ActionType::Fold => s.p_bb_steal_fold = 1,
                            ActionType::Raise => s.p_bb_steal_raise = 1,
                            _ => {}
                        }
                    }
                }

                // M13 counters
                if can_raise {
                    s.p_pfr_opp = 1;
                }
                match a.action_type {
                    ActionType::Fold => s.p_folds += 1,
                    ActionType::Check => s.p_checks += 1,
                    ActionType::Call => {
                        s.p_calls += 1;
                        s.p_vpip = 1;
                        calls += 1;
                    }
                    ActionType::Bet => {
                        s.p_bets += 1;
                    }
                    ActionType::Raise => {
                        s.p_raises += 1;
                        s.p_vpip = 1;
                        s.p_pfr = 1;
                        raises += 1;
                        preflop_aggressor = Some(n);
                    }
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

    PreflopOutcome {
        preflop_aggressor,
        preflop_decisions,
    }
}
