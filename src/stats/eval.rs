//! Bitmask 7-card poker hand evaluator.
//!
//! Ranks any 5- to 7-card hand into the nine categories with kickers as a single
//! comparable integer (`u32`). Pure bitwise arithmetic, zero heap allocation,
//! no floating point, and no panics.

use crate::domain::Card;

/// Extracts the highest rank present in a 13-bit rank mask (`0..=12`).
#[inline]
const fn top_rank(mask: u16) -> u8 {
    if mask == 0 {
        0
    } else {
        15 - (mask.leading_zeros() as u8)
    }
}

/// Detects the highest 5-card straight in a 13-bit rank mask (`0 = Two .. 12 = Ace`).
///
/// Returns the highest rank of the straight (`3 = Five` for the wheel `A-2-3-4-5` up
/// to `12 = Ace` for Broadway `T-J-Q-K-A`).
#[inline]
fn detect_straight(mask: u16) -> Option<u8> {
    let ace_bit = u32::from((mask & (1 << 12)) != 0);
    let s_mask = (u32::from(mask) << 1) | ace_bit;
    let test = s_mask & (s_mask >> 1) & (s_mask >> 2) & (s_mask >> 3) & (s_mask >> 4);
    if test != 0 {
        let k = 31 - test.leading_zeros();
        Some((k + 3) as u8)
    } else {
        None
    }
}

/// Evaluates a 5- to 7-card hand represented by 4 suit bitmasks (`suits[0..4]` for clubs,
/// diamonds, hearts, spades, where bit `r` is set for rank `r in 0..=12`).
///
/// Returns a comparable integer (`u32`) where higher values beat lower values.
#[inline]
#[must_use]
pub fn evaluate_suits(suits: [u16; 4]) -> u32 {
    let s0 = suits[0];
    let s1 = suits[1];
    let s2 = suits[2];
    let s3 = suits[3];

    // In 7 cards, at most one suit can have >= 5 cards.
    let mut flush_suit = None;
    if s0.count_ones() >= 5 {
        flush_suit = Some(s0);
    } else if s1.count_ones() >= 5 {
        flush_suit = Some(s1);
    } else if s2.count_ones() >= 5 {
        flush_suit = Some(s2);
    } else if s3.count_ones() >= 5 {
        flush_suit = Some(s3);
    }

    if let Some(fmask) = flush_suit {
        if let Some(high) = detect_straight(fmask) {
            // Category 8: Straight Flush
            return (8 << 20) | (u32::from(high) << 16);
        }
        // Category 5: Flush (top 5 cards in the flush suit)
        let k1 = top_rank(fmask);
        let m2 = fmask & !(1 << k1);
        let k2 = top_rank(m2);
        let m3 = m2 & !(1 << k2);
        let k3 = top_rank(m3);
        let m4 = m3 & !(1 << k3);
        let k4 = top_rank(m4);
        let m5 = m4 & !(1 << k4);
        let k5 = top_rank(m5);
        return (5 << 20)
            | (u32::from(k1) << 16)
            | (u32::from(k2) << 12)
            | (u32::from(k3) << 8)
            | (u32::from(k4) << 4)
            | u32::from(k5);
    }

    // Rank counting via bit-parallel addition
    let p01 = s0 & s1;
    let p23 = s2 & s3;
    let o01 = s0 | s1;
    let o23 = s2 | s3;

    let at_least_1 = o01 | o23;
    let at_least_2 = p01 | p23 | (o01 & o23);
    let at_least_3 = (p01 & o23) | (p23 & o01);
    let at_least_4 = p01 & p23;

    // Category 7: Four of a Kind
    if at_least_4 != 0 {
        let q = top_rank(at_least_4);
        let k = top_rank(at_least_1 & !(1 << q));
        return (7 << 20) | (u32::from(q) << 16) | (u32::from(k) << 12);
    }

    // Category 6: Full House
    if at_least_3 != 0 {
        let trip = top_rank(at_least_3);
        let rem_pairs = at_least_2 & !(1 << trip);
        if rem_pairs != 0 {
            let pair = top_rank(rem_pairs);
            return (6 << 20) | (u32::from(trip) << 16) | (u32::from(pair) << 12);
        }
    }

    // Category 4: Straight
    if let Some(high) = detect_straight(at_least_1) {
        return (4 << 20) | (u32::from(high) << 16);
    }

    // Category 3: Three of a Kind
    if at_least_3 != 0 {
        let trip = top_rank(at_least_3);
        let rem = at_least_1 & !(1 << trip);
        let k1 = top_rank(rem);
        let k2 = top_rank(rem & !(1 << k1));
        return (3 << 20) | (u32::from(trip) << 16) | (u32::from(k1) << 12) | (u32::from(k2) << 8);
    }

    // Category 2: Two Pair
    if at_least_2.count_ones() >= 2 {
        let p1 = top_rank(at_least_2);
        let rem_pairs = at_least_2 & !(1 << p1);
        let p2 = top_rank(rem_pairs);
        let k = top_rank(at_least_1 & !((1 << p1) | (1 << p2)));
        return (2 << 20) | (u32::from(p1) << 16) | (u32::from(p2) << 12) | (u32::from(k) << 8);
    }

    // Category 1: One Pair
    if at_least_2 != 0 {
        let p = top_rank(at_least_2);
        let rem = at_least_1 & !(1 << p);
        let k1 = top_rank(rem);
        let rem2 = rem & !(1 << k1);
        let k2 = top_rank(rem2);
        let k3 = top_rank(rem2 & !(1 << k2));
        return (1 << 20)
            | (u32::from(p) << 16)
            | (u32::from(k1) << 12)
            | (u32::from(k2) << 8)
            | (u32::from(k3) << 4);
    }

    // Category 0: High Card
    let k1 = top_rank(at_least_1);
    let m2 = at_least_1 & !(1 << k1);
    let k2 = top_rank(m2);
    let m3 = m2 & !(1 << k2);
    let k3 = top_rank(m3);
    let m4 = m3 & !(1 << k3);
    let k4 = top_rank(m4);
    let m5 = m4 & !(1 << k4);
    let k5 = top_rank(m5);
    (u32::from(k1) << 16)
        | (u32::from(k2) << 12)
        | (u32::from(k3) << 8)
        | (u32::from(k4) << 4)
        | u32::from(k5)
}

/// Evaluates any 5- to 7-card hand into a comparable integer.
///
/// Returns `0` if fewer than 5 or more than 7 cards are passed.
#[must_use]
pub fn evaluate(cards: &[Card]) -> u32 {
    if cards.len() < 5 || cards.len() > 7 {
        return 0;
    }
    let mut suits = [0u16; 4];
    for card in cards {
        suits[card.suit() as usize] |= 1 << (card.rank() as u8);
    }
    evaluate_suits(suits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn parse_cards(s: &str) -> Vec<Card> {
        s.split_whitespace()
            .filter_map(|tok| Card::from_str(tok).ok())
            .collect()
    }

    #[test]
    fn every_category_beats_the_one_below_it() {
        // High card < One pair < Two pair < Trips < Straight < Flush < Full house < Quads < Straight flush
        let high_card = evaluate(&parse_cards("Ac Kd Jh 8s 4c"));
        let one_pair = evaluate(&parse_cards("Ac Ad Jh 8s 4c"));
        let two_pair = evaluate(&parse_cards("Ac Ad Kh Kd 4c"));
        let trips = evaluate(&parse_cards("Ac Ad Ah Kd 4c"));
        let straight = evaluate(&parse_cards("Ac Kd Qs Jh Tc"));
        let flush = evaluate(&parse_cards("Ac Kc Jc 8c 4c"));
        let full_house = evaluate(&parse_cards("Ac Ad Ah Kd Kc"));
        let quads = evaluate(&parse_cards("Ac Ad Ah As 4c"));
        let straight_flush = evaluate(&parse_cards("Ac Kc Qc Jc Tc"));

        assert!(high_card < one_pair);
        assert!(one_pair < two_pair);
        assert!(two_pair < trips);
        assert!(trips < straight);
        assert!(straight < flush);
        assert!(flush < full_house);
        assert!(full_house < quads);
        assert!(quads < straight_flush);
    }

    #[test]
    fn kickers_decide_within_category() {
        // High Card
        assert!(
            evaluate(&parse_cards("Ac Kd Jh 8s 5c")) > evaluate(&parse_cards("Ac Kd Jh 8s 4c"))
        );
        // One Pair
        assert!(
            evaluate(&parse_cards("Ac Ad Jh 8s 5c")) > evaluate(&parse_cards("Ac Ad Jh 8s 4c"))
        );
        // Two Pair
        assert!(
            evaluate(&parse_cards("Ac Ad Kh Kd 5c")) > evaluate(&parse_cards("Ac Ad Kh Kd 4c"))
        );
        // Trips
        assert!(
            evaluate(&parse_cards("Ac Ad Ah Kd 5c")) > evaluate(&parse_cards("Ac Ad Ah Kd 4c"))
        );
        // Quads
        assert!(
            evaluate(&parse_cards("Ac Ad Ah As 5c")) > evaluate(&parse_cards("Ac Ad Ah As 4c"))
        );
        // Flush
        assert!(
            evaluate(&parse_cards("Ac Kc Jc 8c 5c")) > evaluate(&parse_cards("Ac Kc Jc 8c 4c"))
        );
    }

    #[test]
    fn wheel_and_steel_wheel() {
        // Wheel (A-2-3-4-5) is 5-high, lower than 6-high (2-3-4-5-6)
        let wheel = evaluate(&parse_cards("Ac 2d 3h 4s 5c"));
        let six_straight = evaluate(&parse_cards("2d 3h 4s 5c 6s"));
        assert!(wheel < six_straight);
        assert_eq!(wheel >> 20, 4); // Category 4: Straight
        assert_eq!((wheel >> 16) & 0xF, 3); // High card Five (rank 3)

        // Steel wheel (A-2-3-4-5 suited) is the lowest straight flush
        let steel_wheel = evaluate(&parse_cards("Ac 2c 3c 4c 5c"));
        let six_sf = evaluate(&parse_cards("2c 3c 4c 5c 6c"));
        assert!(steel_wheel < six_sf);
        assert_eq!(steel_wheel >> 20, 8); // Category 8: Straight Flush
        assert_eq!((steel_wheel >> 16) & 0xF, 3); // High card Five (rank 3)
    }

    #[test]
    fn board_can_play_for_both_split() {
        // Board plays: J T 9 8 7 straight on board, players have lower cards
        let p1 = evaluate(&parse_cards("2c 3d Jh Th 9s 8c 7d"));
        let p2 = evaluate(&parse_cards("4c 5d Jh Th 9s 8c 7d"));
        assert_eq!(p1, p2);
    }

    #[test]
    fn five_six_seven_cards_agree() {
        // Adding cards that don't improve the best 5-card hand yields the same evaluation
        let h5 = evaluate(&parse_cards("Ac Kd Jh 8s 4c"));
        let h6 = evaluate(&parse_cards("Ac Kd Jh 8s 4c 2d"));
        let h7 = evaluate(&parse_cards("Ac Kd Jh 8s 4c 2d 3h"));
        assert_eq!(h5, h6);
        assert_eq!(h6, h7);

        // Flushes
        let f5 = evaluate(&parse_cards("Ac Kc Jc 8c 4c"));
        let f6 = evaluate(&parse_cards("Ac Kc Jc 8c 4c 2c"));
        let f7 = evaluate(&parse_cards("Ac Kc Jc 8c 4c 2c 3h"));
        assert_eq!(f5, f6);
        assert_eq!(f6, f7);

        // Full houses (3 of Ace, 2 of King)
        let fh5 = evaluate(&parse_cards("Ac Ad Ah Kc Kd"));
        let fh6 = evaluate(&parse_cards("Ac Ad Ah Kc Kd 2s"));
        let fh7 = evaluate(&parse_cards("Ac Ad Ah Kc Kd 2s 3d"));
        assert_eq!(fh5, fh6);
        assert_eq!(fh6, fh7);

        // Quads
        let q5 = evaluate(&parse_cards("Ac Ad Ah As Kd"));
        let q6 = evaluate(&parse_cards("Ac Ad Ah As Kd 2c"));
        let q7 = evaluate(&parse_cards("Ac Ad Ah As Kd 2c 3c"));
        assert_eq!(q5, q6);
        assert_eq!(q6, q7);
    }
}
