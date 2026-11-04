//! Money attribution and blinds computation (stats.md §6.1).

use rust_decimal::Decimal;

use crate::domain::{ActionType, CanonicalHand, DeductionKind, decimal_to_micro_units};

/// Money figures computed per player for a single hand in integer micro-units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HandPlayerMoney {
    /// Contributed share of the hand's rake deductions.
    pub rake_attributed_micros: i64,
    /// Contributed share of the hand's `Jackpot` and `Other` deductions.
    pub fees_attributed_micros: i64,
    /// Sum of posted small, big, and dead blinds.
    pub blinds_posted_micros: i64,
    /// Sum of posted straddles.
    pub straddles_posted_micros: i64,
}

/// Splits an integer total `total` (in micro-units) among players according to their
/// weights using the largest-remainder rule (Hamilton / Hare-Niemeyer method).
///
/// Each player gets `floor(total * weight / total_weight)`. The leftover micro-units
/// go one each to the players with the largest remainders `(total * weight) % total_weight`.
/// Ties in remainder are broken in favor of the lower seat number.
///
/// If `total` is 0 or `total_weight` is 0, every part is 0.
/// All intermediate products use `i128` to prevent overflow.
#[must_use]
pub fn split_by_largest_remainder(total: i64, weights_and_seats: &[(u8, i64)]) -> Vec<i64> {
    if total <= 0 || weights_and_seats.is_empty() {
        return vec![0; weights_and_seats.len()];
    }

    let w_sum: i128 = weights_and_seats
        .iter()
        .map(|&(_, w)| i128::from(w.max(0)))
        .sum();
    if w_sum <= 0 {
        return vec![0; weights_and_seats.len()];
    }

    let total_128 = i128::from(total);

    let mut base_parts = Vec::with_capacity(weights_and_seats.len());
    let mut remainders = Vec::with_capacity(weights_and_seats.len());

    for (idx, &(seat, w)) in weights_and_seats.iter().enumerate() {
        let w_128 = i128::from(w.max(0));
        let prod = total_128.saturating_mul(w_128);
        let base_128 = prod / w_sum;
        let base = i64::try_from(base_128).unwrap_or(0);
        let rem = prod % w_sum;
        base_parts.push(base);
        remainders.push((idx, seat, rem));
    }

    let distributed_128: i128 = base_parts.iter().map(|&b| i128::from(b)).sum();
    let leftover = usize::try_from((total_128.saturating_sub(distributed_128)).max(0)).unwrap_or(0);

    if leftover > 0 {
        // Sort descending by remainder; ties broken by lower seat number (ascending)
        remainders.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));

        for item in remainders.iter().take(leftover) {
            if let Some(part) = base_parts.get_mut(item.0) {
                *part = part.saturating_add(1);
            }
        }
    }

    base_parts
}

/// Computes money figures for all players in `hand`, aligned with `hand.players`.
#[must_use]
pub fn compute_hand_money(hand: &CanonicalHand) -> Vec<HandPlayerMoney> {
    if hand.players.is_empty() {
        return Vec::new();
    }

    let player_investeds: Vec<i64> = match hand.checked_player_results() {
        Some(results) => results
            .iter()
            .map(|r| decimal_to_micro_units(r.invested).unwrap_or(0))
            .collect(),
        None => vec![0; hand.players.len()],
    };

    let weights_and_seats: Vec<(u8, i64)> = hand
        .players
        .iter()
        .zip(player_investeds.iter())
        .map(|(p, &inv)| (p.seat, inv))
        .collect();

    let total_pot_micros = decimal_to_micro_units(hand.outcome.total_pot).unwrap_or(0);

    let rake_dec = hand
        .outcome
        .checked_deductions_of(DeductionKind::Rake)
        .unwrap_or(Decimal::ZERO);
    let rake_micros = decimal_to_micro_units(rake_dec).unwrap_or(0);
    let rake_parts = if total_pot_micros == 0 {
        vec![0; hand.players.len()]
    } else {
        split_by_largest_remainder(rake_micros, &weights_and_seats)
    };

    let fees_dec = hand
        .outcome
        .deductions
        .iter()
        .filter(|d| d.kind != DeductionKind::Rake)
        .try_fold(Decimal::ZERO, |acc, d| acc.checked_add(d.amount))
        .unwrap_or(Decimal::ZERO);
    let fees_micros = decimal_to_micro_units(fees_dec).unwrap_or(0);
    let fees_parts = if total_pot_micros == 0 {
        vec![0; hand.players.len()]
    } else {
        split_by_largest_remainder(fees_micros, &weights_and_seats)
    };

    let mut money_stats = Vec::with_capacity(hand.players.len());
    for (i, p) in hand.players.iter().enumerate() {
        let mut blinds_dec: Option<Decimal> = Some(Decimal::ZERO);
        let mut straddles_dec: Option<Decimal> = Some(Decimal::ZERO);
        for a in hand.actions.iter().filter(|a| a.player_name == p.name) {
            match a.action_type {
                ActionType::PostSmallBlind
                | ActionType::PostBigBlind
                | ActionType::PostDeadBlind => {
                    if let Some(sum) = blinds_dec {
                        blinds_dec = sum.checked_add(a.amount);
                    }
                }
                ActionType::PostStraddle => {
                    if let Some(sum) = straddles_dec {
                        straddles_dec = sum.checked_add(a.amount);
                    }
                }
                _ => {}
            }
        }
        let blinds_posted_micros = blinds_dec.and_then(decimal_to_micro_units).unwrap_or(0);
        let straddles_posted_micros = straddles_dec.and_then(decimal_to_micro_units).unwrap_or(0);

        money_stats.push(HandPlayerMoney {
            rake_attributed_micros: *rake_parts.get(i).unwrap_or(&0),
            fees_attributed_micros: *fees_parts.get(i).unwrap_or(&0),
            blinds_posted_micros,
            straddles_posted_micros,
        });
    }

    money_stats
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn largest_remainder_basic_split() {
        let weights = [(1, 33), (2, 33), (3, 34)];
        let split = split_by_largest_remainder(10, &weights);
        assert_eq!(split, vec![3, 3, 4]);
        assert_eq!(split.iter().sum::<i64>(), 10);
    }

    #[test]
    fn largest_remainder_tie_breaking_lower_seat() {
        // Total = 1, two players with equal weights (50 each).
        // Leftover = 1. Seat 1 should win over seat 2.
        let weights = [(1, 50), (2, 50)];
        let split = split_by_largest_remainder(1, &weights);
        assert_eq!(split, vec![1, 0]);
        assert_eq!(split.iter().sum::<i64>(), 1);

        // Reverse input order: seat 2 first, seat 1 second.
        // Seat 1 still wins the tie.
        let weights_rev = [(2, 50), (1, 50)];
        let split_rev = split_by_largest_remainder(1, &weights_rev);
        assert_eq!(split_rev, vec![0, 1]);
        assert_eq!(split_rev.iter().sum::<i64>(), 1);
    }

    #[test]
    fn largest_remainder_zero_cases() {
        let weights = [(1, 100), (2, 200)];
        assert_eq!(split_by_largest_remainder(0, &weights), vec![0, 0]);
        let zero_weights = [(1, 0), (2, 0)];
        assert_eq!(split_by_largest_remainder(100, &zero_weights), vec![0, 0]);
    }
}
