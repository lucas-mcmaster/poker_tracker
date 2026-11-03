//! Exact all-in equity computation via exhaustive board completion.
//!
//! Enumerates every completion of the remaining board cards exactly. A runout
//! won by `k` players awards `1/k` to each. Equities are returned as reduced
//! fractions (`u64 / u64`) in lowest terms.

use crate::domain::Card;
use crate::stats::eval;

/// Helper representing suit bitmasks for fast combination during board enumeration.
#[derive(Clone, Copy)]
pub(crate) struct SuitMasks {
    pub suits: [u16; 4],
}

impl SuitMasks {
    #[inline]
    pub const fn empty() -> Self {
        Self { suits: [0; 4] }
    }

    #[inline]
    pub fn add_card(&mut self, card: Card) {
        self.suits[card.suit() as usize] |= 1 << (card.rank() as u8);
    }

    #[inline]
    pub fn combine(&self, other: &Self) -> Self {
        Self {
            suits: [
                self.suits[0] | other.suits[0],
                self.suits[1] | other.suits[1],
                self.suits[2] | other.suits[2],
                self.suits[3] | other.suits[3],
            ],
        }
    }
}

/// Greatest common divisor using the Euclidean algorithm.
#[inline]
pub(crate) fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// Least common multiple.
#[inline]
fn lcm(a: u64, b: u64) -> u64 {
    if a == 0 || b == 0 {
        0
    } else {
        (a / gcd(a, b)) * b
    }
}

#[inline(always)]
fn for_each_board(
    base: SuitMasks,
    deck: &[SuitMasks],
    cards_needed: usize,
    mut f: impl FnMut(SuitMasks),
) {
    match cards_needed {
        0 => f(base),
        1 => {
            for &c0 in deck {
                f(base.combine(&c0));
            }
        }
        2 => {
            for (i, c0) in deck.iter().enumerate() {
                let m0 = base.combine(c0);
                for c1 in deck.iter().skip(i + 1) {
                    f(m0.combine(c1));
                }
            }
        }
        3 => {
            for (i, c0) in deck.iter().enumerate() {
                let m0 = base.combine(c0);
                for (j, c1) in deck.iter().enumerate().skip(i + 1) {
                    let m1 = m0.combine(c1);
                    for c2 in deck.iter().skip(j + 1) {
                        f(m1.combine(c2));
                    }
                }
            }
        }
        4 => {
            for (i, c0) in deck.iter().enumerate() {
                let m0 = base.combine(c0);
                for (j, c1) in deck.iter().enumerate().skip(i + 1) {
                    let m1 = m0.combine(c1);
                    for (k, c2) in deck.iter().enumerate().skip(j + 1) {
                        let m2 = m1.combine(c2);
                        for c3 in deck.iter().skip(k + 1) {
                            f(m2.combine(c3));
                        }
                    }
                }
            }
        }
        5 => {
            for (i, c0) in deck.iter().enumerate() {
                let m0 = base.combine(c0);
                for (j, c1) in deck.iter().enumerate().skip(i + 1) {
                    let m1 = m0.combine(c1);
                    for (k, c2) in deck.iter().enumerate().skip(j + 1) {
                        let m2 = m1.combine(c2);
                        for (l, c3) in deck.iter().enumerate().skip(k + 1) {
                            let m3 = m2.combine(c3);
                            for c4 in deck.iter().skip(l + 1) {
                                f(m3.combine(c4));
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// Computes exact all-in equity for each player by enumerating all completions of `board`.
///
/// Returns `None` if:
/// - `hole_cards.len() < 2`
/// - `board.len() > 5`
/// - any card is repeated across hole cards and board.
///
/// Returns each player's equity as a reduced fraction `(numerator, denominator)` in lowest terms.
#[must_use]
pub fn exact_equity(hole_cards: &[[Card; 2]], board: &[Card]) -> Option<Vec<(u64, u64)>> {
    let n = hole_cards.len();
    if n < 2 || board.len() > 5 {
        return None;
    }

    // Check for duplicate cards
    let mut used: u64 = 0;
    for pair in hole_cards {
        for &card in pair {
            let bit = 1u64 << card.raw();
            if (used & bit) != 0 {
                return None;
            }
            used |= bit;
        }
    }
    for &card in board {
        let bit = 1u64 << card.raw();
        if (used & bit) != 0 {
            return None;
        }
        used |= bit;
    }

    let mut deck_masks = Vec::with_capacity(52);
    for raw in 0..52 {
        if (used & (1u64 << raw)) == 0
            && let Some(card) = Card::from_raw(raw)
        {
            let mut sm = SuitMasks::empty();
            sm.add_card(card);
            deck_masks.push(sm);
        }
    }

    let mut base_board = SuitMasks::empty();
    for &card in board {
        base_board.add_card(card);
    }

    let mut player_masks = Vec::with_capacity(n);
    for pair in hole_cards {
        let mut sm = SuitMasks::empty();
        sm.add_card(pair[0]);
        sm.add_card(pair[1]);
        player_masks.push(sm);
    }

    let cards_needed = 5 - board.len();

    if n == 2 {
        let p0 = player_masks[0];
        let p1 = player_masks[1];
        let mut w0 = 0u64;
        let mut w1 = 0u64;
        let mut total_runouts = 0u64;

        for_each_board(base_board, &deck_masks, cards_needed, |bm| {
            total_runouts += 1;
            let s0 = eval::evaluate_suits(bm.combine(&p0).suits);
            let s1 = eval::evaluate_suits(bm.combine(&p1).suits);
            if s0 > s1 {
                w0 += 2;
            } else if s1 > s0 {
                w1 += 2;
            } else {
                w0 += 1;
                w1 += 1;
            }
        });

        if total_runouts == 0 {
            return None;
        }

        let total_points = total_runouts.saturating_mul(2);
        let g0 = gcd(w0, total_points);
        let g1 = gcd(w1, total_points);

        Some(vec![
            (w0 / g0, total_points / g0),
            (w1 / g1, total_points / g1),
        ])
    } else {
        // Multi-way (n > 2)
        let mut l = 1u64;
        for i in 2..=(n as u64) {
            l = lcm(l, i);
        }

        let mut points = vec![0u64; n];
        let mut total_runouts = 0u64;
        let mut scores = vec![0u32; n];

        for_each_board(base_board, &deck_masks, cards_needed, |bm| {
            total_runouts += 1;
            let mut max_s = 0;
            for (idx, pm) in player_masks.iter().enumerate() {
                let s = eval::evaluate_suits(bm.combine(pm).suits);
                scores[idx] = s;
                if s > max_s {
                    max_s = s;
                }
            }

            let mut k = 0u64;
            for &s in &scores {
                if s == max_s {
                    k += 1;
                }
            }

            let share = l / k;
            for (idx, &s) in scores.iter().enumerate() {
                if s == max_s {
                    points[idx] += share;
                }
            }
        });

        if total_runouts == 0 {
            return None;
        }

        let total_points = total_runouts.saturating_mul(l);
        let result = points
            .into_iter()
            .map(|pts| {
                let g = gcd(pts, total_points);
                (pts / g, total_points / g)
            })
            .collect();

        Some(result)
    }
}
