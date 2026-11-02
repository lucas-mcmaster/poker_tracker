//! Site-independent parser trait, hand-block splitter, and the canonical-hand validator.

use std::collections::{HashMap, HashSet};

use rust_decimal::Decimal;
use thiserror::Error;

use crate::domain::canonical::{
    ActionType, AdjustmentKind, CanonicalHand, MAX_RUNOUTS, Platform, Player, PlayerResult, Street,
};
use crate::domain::card::Card;
use crate::domain::money::decimal_to_micro_units;

/// Structured errors emitted during hand history parsing and validation.
///
/// `line` fields are 1-based line numbers relative to the start of the hand block; add
/// [`HandBlock::start_line`] `- 1` to get the line number in the source file.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ParseError {
    #[error("hand block is empty")]
    EmptyHandBlock,

    #[error("invalid hand header line: '{line}'")]
    InvalidHeader { line: String },

    #[error("game, limit or stakes format not supported by this parser: '{line}'")]
    UnsupportedGame { line: String },

    #[error("invalid table header line: '{line}'")]
    InvalidTableHeader { line: String },

    #[error("invalid card syntax: '{token}'")]
    InvalidCardSyntax { token: String },

    #[error("invalid monetary amount: '{raw}'")]
    InvalidAmount { raw: String },

    #[error("invalid timestamp: '{raw}'")]
    InvalidTimestamp { raw: String },

    #[error("action references unknown or unseated player: '{player_name}'")]
    UnknownPlayer { player_name: String },

    #[error("conflicting hole cards for player '{player_name}'")]
    ConflictingHoleCards { player_name: String },

    #[error("invalid street transition at line {line}: cannot move from {current} to {attempted}")]
    IncompleteStreet {
        line: usize,
        current: String,
        attempted: String,
    },

    #[error("illegal action at line {line} ({reason}): '{content}'")]
    IllegalAction {
        line: usize,
        content: String,
        reason: String,
    },

    #[error("physical card collision detected for card '{card}'")]
    DuplicateCard { card: String },

    #[error("unrecognized line at line {line}: '{content}'")]
    UnrecognizedLine { line: usize, content: String },

    #[error("unsupported off-pot adjustment (e.g. cashout/insurance) at line {line}: '{content}'")]
    UnsupportedAdjustment { line: usize, content: String },

    #[error("summary section disagrees with hand body in hand {hand_id}: {detail}")]
    SummaryMismatch { hand_id: String, detail: String },

    #[error(
        "mismatched pot accounting in hand {hand_id}: total_pot ({total_pot}) - deductions ({total_deductions}) != collected ({total_collected})"
    )]
    MismatchedPotAccounting {
        hand_id: String,
        total_pot: Decimal,
        total_deductions: Decimal,
        total_collected: Decimal,
    },

    #[error(
        "chip conservation failed in hand {hand_id}: actions put {contributed} into the pot but total_pot is {total_pot}"
    )]
    PotContributionMismatch {
        hand_id: String,
        contributed: Decimal,
        total_pot: Decimal,
    },

    #[error("invariant violated in hand {hand_id}: {detail}")]
    InvariantViolation { hand_id: String, detail: String },
}

/// One raw hand block from a session file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandBlock<'a> {
    /// Block text with trailing whitespace removed.
    pub text: &'a str,
    /// 1-based line number of the block's first line in the source text (after BOM removal).
    pub start_line: usize,
}

/// Splits a multi-hand session into hand blocks separated by blank lines (LF or CRLF),
/// stripping a leading UTF-8 BOM and recording each block's starting line number.
///
/// This is the default splitter for [`HandParser::split_blocks`]; sites with a different
/// container format override that method.
pub fn split_hand_blocks(raw_session: &str) -> impl Iterator<Item = HandBlock<'_>> {
    let raw = raw_session.strip_prefix('\u{feff}').unwrap_or(raw_session);
    let mut offset = 0usize;
    let mut line_no = 1usize;

    let next_line = |offset: usize| -> Option<&str> {
        let rest = raw.get(offset..)?;
        if rest.is_empty() {
            return None;
        }
        let len = rest.find('\n').map_or(rest.len(), |i| i + 1);
        rest.get(..len)
    };

    std::iter::from_fn(move || {
        loop {
            let line = next_line(offset)?;
            if !line.trim().is_empty() {
                break;
            }
            offset += line.len();
            line_no += 1;
        }
        let start = offset;
        let start_line = line_no;
        let mut end = offset;
        while let Some(line) = next_line(offset) {
            offset += line.len();
            line_no += 1;
            if line.trim().is_empty() {
                break;
            }
            end = offset;
        }
        raw.get(start..end).map(|text| HandBlock {
            text: text.trim_end(),
            start_line,
        })
    })
}

/// Returns the first non-blank line of `raw_session` (after a BOM), for format sniffing.
#[must_use]
pub fn first_content_line(raw_session: &str) -> Option<&str> {
    raw_session
        .strip_prefix('\u{feff}')
        .unwrap_or(raw_session)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
}

fn violation(hand: &CanonicalHand, detail: impl Into<String>) -> ParseError {
    ParseError::InvariantViolation {
        hand_id: hand.header.site_hand_id.clone(),
        detail: detail.into(),
    }
}

/// Checks if a [`Card`] is already present in `mask`; if not, sets its bit.
fn check_and_insert_card(mask: &mut u64, card: Card) -> Result<(), ParseError> {
    let bit = 1u64 << card.raw();
    if (*mask & bit) != 0 {
        return Err(ParseError::DuplicateCard {
            card: card.to_string(),
        });
    }
    *mask |= bit;
    Ok(())
}

/// Verifies a [`Decimal`] is losslessly convertible to integer micro-units.
fn validate_lossless(value: Decimal) -> Result<(), ParseError> {
    if decimal_to_micro_units(value).is_none() {
        return Err(ParseError::InvalidAmount {
            raw: value.to_string(),
        });
    }
    Ok(())
}

/// Verifies a [`Decimal`] is nonnegative and losslessly convertible to integer micro-units.
fn validate_nonnegative_monetary(value: Decimal) -> Result<(), ParseError> {
    if value < Decimal::ZERO {
        return Err(ParseError::InvalidAmount {
            raw: value.to_string(),
        });
    }
    validate_lossless(value)
}

fn validate_header(hand: &CanonicalHand) -> Result<(), ParseError> {
    let h = &hand.header;
    if h.site_hand_id.trim().is_empty() || h.table_name.trim().is_empty() {
        return Err(violation(hand, "empty hand id or table name"));
    }
    if !(2..=10).contains(&h.max_seats) || h.button_seat < 1 || h.button_seat > h.max_seats {
        return Err(violation(
            hand,
            format!(
                "invalid table size {} / button seat {}",
                h.max_seats, h.button_seat
            ),
        ));
    }
    if let Some(offset) = h.utc_offset_seconds
        && offset.unsigned_abs() > 18 * 3600
    {
        return Err(violation(
            hand,
            format!("UTC offset {offset}s out of range"),
        ));
    }
    validate_nonnegative_monetary(h.small_blind)?;
    validate_nonnegative_monetary(h.big_blind)?;
    if h.small_blind <= Decimal::ZERO || h.big_blind < h.small_blind {
        return Err(violation(
            hand,
            "blinds must satisfy 0 < small blind <= big blind",
        ));
    }
    Ok(())
}

/// Validates seating and hole cards; returns the bitmask of all known hole cards.
fn validate_players(hand: &CanonicalHand) -> Result<u64, ParseError> {
    if hand.players.len() < 2 || hand.players.len() > usize::from(hand.header.max_seats) {
        return Err(violation(
            hand,
            format!(
                "{} seated players at a {}-max table",
                hand.players.len(),
                hand.header.max_seats
            ),
        ));
    }
    let mut mask: u64 = 0;
    let mut seats: HashSet<u8> = HashSet::with_capacity(hand.players.len());
    let mut names: HashSet<&str> = HashSet::with_capacity(hand.players.len());
    let mut heroes = 0usize;

    for p in &hand.players {
        if p.seat < 1 || p.seat > hand.header.max_seats || !seats.insert(p.seat) {
            return Err(violation(
                hand,
                format!("invalid or duplicate seat {}", p.seat),
            ));
        }
        if p.name.trim().is_empty() || !names.insert(p.name.as_str()) {
            return Err(ParseError::UnknownPlayer {
                player_name: p.name.clone(),
            });
        }
        validate_nonnegative_monetary(p.starting_stack)?;
        if p.starting_stack <= Decimal::ZERO {
            return Err(violation(hand, format!("player '{}' has no chips", p.name)));
        }
        if p.hole_cards[0].is_none() && p.hole_cards[1].is_some() {
            return Err(violation(
                hand,
                format!("non-canonical hole cards for '{}'", p.name),
            ));
        }
        for card in p.known_cards() {
            check_and_insert_card(&mut mask, card)?;
        }
        if p.is_hero {
            heroes += 1;
            if p.known_cards().count() != 2 {
                return Err(violation(hand, "hero must have two known hole cards"));
            }
        }
    }
    if heroes != 1 {
        return Err(violation(
            hand,
            format!("expected exactly one hero, found {heroes}"),
        ));
    }
    Ok(mask)
}

fn validate_actions(
    hand: &CanonicalHand,
    by_name: &HashMap<&str, &Player>,
) -> Result<(), ParseError> {
    let mut commitments: HashMap<(Street, &str), Decimal> = HashMap::new();
    let mut gross: HashMap<&str, Decimal> = HashMap::new();
    let mut folded: HashSet<&str> = HashSet::new();
    let mut last_street = Street::Preflop;

    for (idx, act) in hand.actions.iter().enumerate() {
        if usize::from(act.sequence) != idx {
            return Err(violation(
                hand,
                format!("action sequence {} at position {idx}", act.sequence),
            ));
        }
        if act.street < last_street || act.street == Street::Showdown {
            return Err(violation(
                hand,
                format!(
                    "action {} recorded out of street order ({:?})",
                    act.sequence, act.street
                ),
            ));
        }
        last_street = act.street;

        let name = act.player_name.as_str();
        let Some(player) = by_name.get(name) else {
            return Err(ParseError::UnknownPlayer {
                player_name: act.player_name.clone(),
            });
        };
        if folded.contains(name) {
            return Err(violation(hand, format!("'{name}' acts after folding")));
        }
        validate_nonnegative_monetary(act.amount)?;
        validate_nonnegative_monetary(act.street_total_after)?;

        let t = act.action_type;
        let zero_amount = matches!(t, ActionType::Fold | ActionType::Check);
        if zero_amount != act.amount.is_zero() {
            return Err(violation(
                hand,
                format!(
                    "action {} ({}) has amount {}",
                    act.sequence,
                    t.as_str(),
                    act.amount
                ),
            ));
        }

        let key = (act.street, name);
        let prev = commitments.get(&key).copied().unwrap_or(Decimal::ZERO);
        let expected = if t.adds_to_street_commitment() {
            prev.checked_add(act.amount)
        } else if t == ActionType::UncalledBetReturned {
            prev.checked_sub(act.amount)
        } else {
            Some(prev)
        };
        if expected != Some(act.street_total_after) || act.street_total_after < Decimal::ZERO {
            return Err(violation(
                hand,
                format!(
                    "street_total_after chain broken at action {} ('{name}')",
                    act.sequence
                ),
            ));
        }
        commitments.insert(key, act.street_total_after);

        if t.is_contribution() {
            let g = gross
                .get(name)
                .copied()
                .unwrap_or(Decimal::ZERO)
                .checked_add(act.amount)
                .ok_or_else(|| violation(hand, "contribution overflow"))?;
            if g > player.starting_stack {
                return Err(violation(
                    hand,
                    format!(
                        "'{name}' commits {g} with a starting stack of {}",
                        player.starting_stack
                    ),
                ));
            }
            if act.is_all_in && g != player.starting_stack {
                return Err(violation(
                    hand,
                    format!(
                        "'{name}' is marked all-in after committing {g} of {}",
                        player.starting_stack
                    ),
                ));
            }
            if !act.is_all_in && g == player.starting_stack {
                return Err(violation(
                    hand,
                    format!(
                        "'{name}' commits the whole stack of {g} in action {} without being marked all-in",
                        act.sequence
                    ),
                ));
            }
            gross.insert(name, g);
        } else if act.is_all_in {
            return Err(violation(
                hand,
                format!("non-contribution action {} marked all-in", act.sequence),
            ));
        }
        if t == ActionType::Fold {
            folded.insert(name);
        }
    }
    Ok(())
}

fn validate_outcome(
    hand: &CanonicalHand,
    by_name: &HashMap<&str, &Player>,
    run_count: usize,
) -> Result<(), ParseError> {
    let o = &hand.outcome;
    validate_nonnegative_monetary(o.total_pot)?;
    validate_nonnegative_monetary(o.uncalled_returned)?;
    for d in &o.deductions {
        validate_nonnegative_monetary(d.amount)?;
        if d.label.trim().is_empty() {
            return Err(violation(hand, "pot deduction without a label"));
        }
    }

    // Summary arithmetic: total_pot - deductions == collected.
    let total_deductions = o
        .checked_total_deductions()
        .ok_or_else(|| violation(hand, "deductions overflow"))?;
    let total_collected = o
        .checked_total_collected()
        .ok_or_else(|| violation(hand, "collected overflow"))?;
    if o.total_pot.checked_sub(total_deductions) != Some(total_collected) {
        return Err(ParseError::MismatchedPotAccounting {
            hand_id: hand.header.site_hand_id.clone(),
            total_pot: o.total_pot,
            total_deductions,
            total_collected,
        });
    }

    // Chip conservation: what the actions put in is exactly the pot.
    let contributed = hand
        .checked_total_contributed()
        .ok_or_else(|| violation(hand, "contribution overflow"))?;
    if contributed != o.total_pot {
        return Err(ParseError::PotContributionMismatch {
            hand_id: hand.header.site_hand_id.clone(),
            contributed,
            total_pot: o.total_pot,
        });
    }
    let returned = hand
        .actions
        .iter()
        .filter(|a| a.action_type == ActionType::UncalledBetReturned)
        .try_fold(Decimal::ZERO, |acc, a| acc.checked_add(a.amount));
    if returned != Some(o.uncalled_returned) {
        return Err(violation(
            hand,
            "uncalled_returned disagrees with returned-bet actions",
        ));
    }

    // Winners.
    let folded: HashSet<&str> = hand
        .actions
        .iter()
        .filter(|a| a.action_type == ActionType::Fold)
        .map(|a| a.player_name.as_str())
        .collect();
    if o.total_pot > Decimal::ZERO && o.winners.is_empty() {
        return Err(violation(hand, "pot has no winner"));
    }
    let mut runs_with_winner: HashSet<u8> = HashSet::new();
    for w in &o.winners {
        if !by_name.contains_key(w.player_name.as_str()) {
            return Err(ParseError::UnknownPlayer {
                player_name: w.player_name.clone(),
            });
        }
        validate_nonnegative_monetary(w.amount)?;
        if w.amount.is_zero() {
            return Err(violation(
                hand,
                format!("zero payout to '{}'", w.player_name),
            ));
        }
        if folded.contains(w.player_name.as_str()) {
            return Err(violation(
                hand,
                format!("folded player '{}' wins a pot", w.player_name),
            ));
        }
        let valid_run = if run_count >= 2 {
            (1..=run_count).contains(&usize::from(w.run_index))
        } else {
            w.run_index == 0
        };
        if !valid_run {
            return Err(violation(
                hand,
                format!("payout on invalid run {}", w.run_index),
            ));
        }
        runs_with_winner.insert(w.run_index);
    }
    if run_count >= 2 && runs_with_winner.len() != run_count {
        return Err(violation(
            hand,
            "every runout must award its share of the pot",
        ));
    }

    // Off-pot adjustments.
    let mut adjusted_players: HashSet<&str> = HashSet::new();
    for a in &o.adjustments {
        if !by_name.contains_key(a.player_name.as_str()) {
            return Err(ParseError::UnknownPlayer {
                player_name: a.player_name.clone(),
            });
        }
        if !adjusted_players.insert(a.player_name.as_str()) {
            return Err(violation(
                hand,
                format!("multiple adjustments for player '{}'", a.player_name),
            ));
        }
        validate_lossless(a.amount)?;
        let sign_ok = match a.kind.is_credit() {
            Some(true) => a.amount > Decimal::ZERO,
            Some(false) => a.amount < Decimal::ZERO,
            None => !a.amount.is_zero(),
        };
        if !sign_ok {
            return Err(violation(
                hand,
                format!(
                    "{} adjustment for '{}' has amount {}",
                    a.kind.as_str(),
                    a.player_name,
                    a.amount
                ),
            ));
        }
        if a.street == Street::Showdown {
            return Err(violation(hand, "adjustment triggered at showdown"));
        }
        let won = o
            .winners
            .iter()
            .filter(|w| w.player_name == a.player_name)
            .try_fold(Decimal::ZERO, |acc, w| acc.checked_add(w.amount))
            .ok_or_else(|| violation(hand, "winner amount overflow"))?;
        match a.kind {
            AdjustmentKind::EvCashoutReceived => {
                if folded.contains(a.player_name.as_str()) {
                    return Err(violation(
                        hand,
                        format!("folded player '{}' received EV cashout", a.player_name),
                    ));
                }
                if won != Decimal::ZERO {
                    return Err(violation(
                        hand,
                        format!("winning player '{}' received EV cashout", a.player_name),
                    ));
                }
            }
            AdjustmentKind::EvCashoutRiskPaid => {
                if won <= Decimal::ZERO {
                    return Err(violation(
                        hand,
                        format!(
                            "non-winning player '{}' paid EV cashout risk",
                            a.player_name
                        ),
                    ));
                }
            }
        }
    }

    // Per-player results must be computable and lossless.
    let results = hand
        .checked_player_results()
        .ok_or_else(|| violation(hand, "player result overflow"))?;
    for r in &results {
        validate_nonnegative_monetary(r.invested)?;
        validate_nonnegative_monetary(r.won)?;
        validate_lossless(r.net)?;
    }
    validate_side_pots(hand, &results)
}

/// Side-pot eligibility: the players who put at most `L` live chips into the pot can together
/// win at most what every player put in up to `L` each, plus all dead money. Checked for every
/// winner's level `L`, so a short all-in stack can never be paid from a side pot.
fn validate_side_pots(hand: &CanonicalHand, results: &[PlayerResult]) -> Result<(), ParseError> {
    let overflow = || violation(hand, "side-pot arithmetic overflow");
    let mut dead_total = Decimal::ZERO;
    let mut live: Vec<(Decimal, Decimal)> = Vec::with_capacity(results.len());
    for r in results {
        let dead = hand
            .actions
            .iter()
            .filter(|a| {
                a.player_name == r.player_name && a.action_type == ActionType::PostDeadBlind
            })
            .try_fold(Decimal::ZERO, |acc, a| acc.checked_add(a.amount))
            .ok_or_else(overflow)?;
        dead_total = dead_total.checked_add(dead).ok_or_else(overflow)?;
        let live_in = r.invested.checked_sub(dead).ok_or_else(overflow)?;
        live.push((live_in, r.won));
    }
    for &(level, won) in &live {
        if won.is_zero() {
            continue;
        }
        let mut cap = dead_total;
        let mut paid = Decimal::ZERO;
        for &(other_live, other_won) in &live {
            cap = cap
                .checked_add(other_live.min(level))
                .ok_or_else(overflow)?;
            if other_live <= level {
                paid = paid.checked_add(other_won).ok_or_else(overflow)?;
            }
        }
        if paid > cap {
            return Err(violation(
                hand,
                format!(
                    "players who put {level} or less into the pot are paid {paid}, more than the {cap} they could win"
                ),
            ));
        }
    }
    Ok(())
}

/// Validates board progression, run structure and 52-card uniqueness across all runouts.
fn validate_boards(hand: &CanonicalHand, hole_cards_mask: u64) -> Result<usize, ParseError> {
    let multi = hand.boards.iter().any(|b| b.run_index > 0);
    let run_count = hand.run_count();
    if multi {
        let contiguous = hand
            .boards
            .iter()
            .enumerate()
            .all(|(i, b)| usize::from(b.run_index) == i + 1);
        if !contiguous || !(2..=MAX_RUNOUTS).contains(&run_count) {
            return Err(violation(
                hand,
                "multi-run boards must be runs 1..=n with 2 <= n <= MAX_RUNOUTS",
            ));
        }
        if hand.boards.iter().any(|b| b.river.is_none()) {
            return Err(violation(
                hand,
                "every multi-run board must be dealt to the river",
            ));
        }
    } else if hand.boards.len() > 1 {
        return Err(violation(hand, "multiple boards without run indices"));
    }

    let mut global_dealt_mask = hole_cards_mask;
    let first_board = hand.boards.first();
    for (board_idx, board) in hand.boards.iter().enumerate() {
        if (board.turn.is_some() && board.flop.is_none())
            || (board.river.is_some() && board.turn.is_none())
        {
            return Err(ParseError::IncompleteStreet {
                line: 0,
                current: "BOARD".to_string(),
                attempted: format!("run_index {}", board.run_index),
            });
        }

        // Each runout on its own must not collide with any hole card.
        let mut single_board_mask = hole_cards_mask;
        for card in board.cards() {
            check_and_insert_card(&mut single_board_mask, card)?;
        }

        // Across runouts, only cards that are not shared with the first runout are new.
        let (shares_flop, shares_turn) = match first_board {
            Some(first) if board_idx > 0 => {
                let f = board.flop == first.flop;
                (f, f && board.turn == first.turn)
            }
            _ => (false, false),
        };
        if !shares_flop && let Some(flop) = board.flop {
            for card in flop {
                check_and_insert_card(&mut global_dealt_mask, card)?;
            }
        }
        if !shares_turn && let Some(turn) = board.turn {
            check_and_insert_card(&mut global_dealt_mask, turn)?;
        }
        if let Some(river) = board.river {
            check_and_insert_card(&mut global_dealt_mask, river)?;
        }
    }
    Ok(run_count)
}

/// First seat clockwise after `from` (wrapping; `from` itself is checked last) satisfying `pred`.
/// `seats` must be sorted ascending.
fn next_clockwise(seats: &[u8], from: u8, pred: impl Fn(u8) -> bool) -> Option<u8> {
    seats
        .iter()
        .copied()
        .filter(|s| *s > from)
        .chain(seats.iter().copied().filter(|s| *s <= from))
        .find(|s| pred(*s))
}

/// Mutable state of one hand while its betting is replayed.
struct BettingState {
    folded: HashSet<u8>,
    all_in: HashSet<u8>,
    gross: HashMap<u8, Decimal>,
    committed: HashMap<u8, Decimal>,
}

impl BettingState {
    fn live_count(&self, seats: &[u8]) -> usize {
        seats.iter().filter(|s| !self.folded.contains(s)).count()
    }

    fn can_act(&self, seat: u8) -> bool {
        !self.folded.contains(&seat) && !self.all_in.contains(&seat)
    }

    fn committed(&self, seat: u8) -> Decimal {
        self.committed.get(&seat).copied().unwrap_or(Decimal::ZERO)
    }

    fn max_committed(&self) -> Decimal {
        self.committed
            .values()
            .copied()
            .max()
            .unwrap_or(Decimal::ZERO)
    }

    /// Chips the player has put in over the whole hand (before any uncalled-bet return).
    fn gross(&self, seat: u8) -> Decimal {
        self.gross.get(&seat).copied().unwrap_or(Decimal::ZERO)
    }

    fn commit(&mut self, seat: u8, amount: Decimal, stack: Decimal) -> Option<()> {
        let g = self.gross(seat).checked_add(amount)?;
        if g == stack {
            self.all_in.insert(seat);
        }
        self.gross.insert(seat, g);
        Some(())
    }

    /// The round is settled for a lone player who can still act once they match the highest
    /// commitment: nobody is left to bet against, so they owe no action.
    fn settle_lone_player(&self, seats: &[u8], needs: &mut HashSet<u8>) {
        let mut able = seats.iter().copied().filter(|s| self.can_act(*s));
        if let (Some(only), None) = (able.next(), able.next())
            && self.committed(only) >= self.max_committed()
        {
            needs.remove(&only);
        }
    }
}

/// Replays the betting street by street (No-Limit rules, seat order clockwise by seat number):
///
/// - blinds come first and match the stakes (less only when the poster is all-in); preflop
///   action starts left of the first big blind poster, later streets start left of the button;
/// - every player still able to act must act, in turn, until the round closes; a bet or raise
///   makes everyone else still able to act owe an action again; a lone player who can still
///   act and already matches the highest commitment owes nothing;
/// - checks, calls, bets and raises must be legal for the amount faced: a raise needs a bet to
///   raise and an opponent who can still respond; a bet is at least the big blind and a raise
///   adds at least the last full bet or raise (or completes a smaller all-in to the big blind),
///   unless the player is all-in; an all-in for less than a full raise does not reopen the
///   betting for players who already acted;
/// - an uncalled bet returns exactly the unmatched part of the largest commitment, after the
///   betting closed;
/// - actions happen only on streets whose cards were dealt; no street is dealt after the hand
///   was decided by folds; a hand contested by two or more players reaches the river.
fn validate_betting(hand: &CanonicalHand) -> Result<(), ParseError> {
    let mut seats: Vec<u8> = hand.players.iter().map(|p| p.seat).collect();
    seats.sort_unstable();
    let seat_of: HashMap<&str, u8> = hand
        .players
        .iter()
        .map(|p| (p.name.as_str(), p.seat))
        .collect();
    let stack_of: HashMap<u8, Decimal> = hand
        .players
        .iter()
        .map(|p| (p.seat, p.starting_stack))
        .collect();
    let seat = |name: &str| -> Result<u8, ParseError> {
        seat_of
            .get(name)
            .copied()
            .ok_or_else(|| ParseError::UnknownPlayer {
                player_name: name.to_string(),
            })
    };
    let stack = |s: u8| stack_of.get(&s).copied().unwrap_or(Decimal::ZERO);
    let is_post = |t: ActionType| {
        matches!(
            t,
            ActionType::PostSmallBlind
                | ActionType::PostBigBlind
                | ActionType::PostDeadBlind
                | ActionType::PostStraddle
        )
    };
    let is_blind = |t: ActionType| {
        matches!(
            t,
            ActionType::PostSmallBlind | ActionType::PostBigBlind | ActionType::PostDeadBlind
        )
    };
    let (small_blind, big_blind) = (hand.header.small_blind, hand.header.big_blind);

    // Forced posts lead the hand and match the stakes.
    let posts = hand
        .actions
        .iter()
        .take_while(|a| is_post(a.action_type))
        .count();
    if hand
        .actions
        .iter()
        .skip(posts)
        .any(|a| is_post(a.action_type))
    {
        return Err(violation(hand, "forced post after the betting started"));
    }
    let posted = hand.actions.get(..posts).unwrap_or(&[]);
    if posted
        .iter()
        .filter(|a| a.action_type == ActionType::PostSmallBlind)
        .count()
        > 1
    {
        return Err(violation(hand, "more than one small blind"));
    }
    let Some(first_big_blind_idx) = posted
        .iter()
        .position(|a| a.action_type == ActionType::PostBigBlind)
    else {
        return Err(violation(hand, "no big blind posted"));
    };
    if posted[..first_big_blind_idx]
        .iter()
        .any(|a| a.action_type == ActionType::PostStraddle)
    {
        return Err(violation(hand, "straddle before big blind"));
    }
    if let Some(first_straddle_idx) = posted
        .iter()
        .position(|a| a.action_type == ActionType::PostStraddle)
        && posted[first_straddle_idx..]
            .iter()
            .any(|a| is_blind(a.action_type))
    {
        return Err(violation(hand, "blind posted after a straddle"));
    }
    let straddle_count = posted
        .iter()
        .filter(|a| a.action_type == ActionType::PostStraddle)
        .count();
    if straddle_count > 2 {
        return Err(violation(hand, "more than two straddles"));
    }

    let first_big_blind = &posted[first_big_blind_idx];
    let big_blind_seat = seat(&first_big_blind.player_name)?;

    let last_straddle = posted
        .iter()
        .rev()
        .find(|a| a.action_type == ActionType::PostStraddle);
    let straddle_anchor = match last_straddle {
        Some(s) => Some(seat(&s.player_name)?),
        None => None,
    };

    let mut posted_commitments: HashMap<u8, Decimal> = HashMap::new();
    for a in posted {
        let s = seat(&a.player_name)?;
        let current = posted_commitments.get(&s).copied().unwrap_or(Decimal::ZERO);
        let size_ok = match a.action_type {
            ActionType::PostSmallBlind => {
                (a.amount == small_blind || (a.is_all_in && a.amount < small_blind))
                    && a.street_total_after
                        == current.checked_add(a.amount).unwrap_or(Decimal::ZERO)
            }
            ActionType::PostBigBlind => {
                (a.amount == big_blind || (a.is_all_in && a.amount < big_blind))
                    && a.street_total_after
                        == current.checked_add(a.amount).unwrap_or(Decimal::ZERO)
            }
            ActionType::PostDeadBlind => a.amount <= big_blind && a.street_total_after == current,
            ActionType::PostStraddle => {
                let highest_live = posted_commitments
                    .values()
                    .copied()
                    .max()
                    .unwrap_or(Decimal::ZERO);
                let expected = highest_live
                    .checked_mul(Decimal::from(2))
                    .ok_or_else(|| violation(hand, "straddle sizing overflow"))?;
                (a.street_total_after == expected
                    || (a.is_all_in && a.street_total_after < expected))
                    && a.amount > Decimal::ZERO
                    && a.street_total_after
                        == current.checked_add(a.amount).unwrap_or(Decimal::ZERO)
            }
            _ => false,
        };
        if !size_ok {
            let msg = if a.action_type == ActionType::PostStraddle {
                format!(
                    "{} of {} by '{}' does not match required size",
                    a.action_type.as_str(),
                    a.amount,
                    a.player_name
                )
            } else {
                format!(
                    "{} of {} by '{}' does not match the {small_blind}/{big_blind} stakes",
                    a.action_type.as_str(),
                    a.amount,
                    a.player_name
                )
            };
            return Err(violation(hand, msg));
        }
        if a.action_type.adds_to_street_commitment() {
            posted_commitments.insert(s, a.street_total_after);
        }
    }

    let mut state = BettingState {
        folded: HashSet::new(),
        all_in: HashSet::new(),
        gross: HashMap::new(),
        committed: HashMap::new(),
    };
    for a in posted {
        let s = seat(&a.player_name)?;
        state.committed.insert(s, a.street_total_after);
        state
            .commit(s, a.amount, stack(s))
            .ok_or_else(|| violation(hand, "contribution overflow"))?;
    }

    let reached = |street: Street| match street {
        Street::Preflop => true,
        Street::Flop => hand.boards.iter().any(|b| b.flop.is_some()),
        Street::Turn => hand.boards.iter().any(|b| b.turn.is_some()),
        Street::River => hand.boards.iter().any(|b| b.river.is_some()),
        Street::Showdown => false,
    };

    let mut idx = posts;
    for street in [Street::Preflop, Street::Flop, Street::Turn, Street::River] {
        if !reached(street) {
            break;
        }
        if street != Street::Preflop {
            if state.live_count(&seats) <= 1 {
                return Err(violation(
                    hand,
                    format!("{street:?} dealt after the hand was decided"),
                ));
            }
            state.committed.clear();
        }
        let able: Vec<u8> = seats
            .iter()
            .copied()
            .filter(|s| state.can_act(*s))
            .collect();
        let max = state.max_committed();
        let mut needs: HashSet<u8> = able
            .iter()
            .copied()
            .filter(|s| able.len() >= 2 || state.committed(*s) < max)
            .collect();
        // Players who already acted voluntarily on this street (posting a blind is not acting).
        let mut acted: HashSet<u8> = HashSet::new();
        // Size of the last full bet or raise; the minimum bet is the big blind (or straddle).
        let mut min_raise = if street == Street::Preflop {
            last_straddle
                .map(|s| s.street_total_after)
                .unwrap_or(big_blind)
        } else {
            big_blind
        };
        let mut anchor = if street == Street::Preflop {
            straddle_anchor.unwrap_or(big_blind_seat)
        } else {
            hand.header.button_seat
        };

        while let Some(a) = hand.actions.get(idx).filter(|a| a.street == street) {
            idx += 1;
            let s = seat(&a.player_name)?;
            let current = state.committed(s);
            let prev_max = state.max_committed();

            if a.action_type == ActionType::UncalledBetReturned {
                if !needs.is_empty() && state.live_count(&seats) > 1 {
                    return Err(violation(
                        hand,
                        "uncalled bet returned while betting was open",
                    ));
                }
                let second = state
                    .committed
                    .iter()
                    .filter(|(other, _)| **other != s)
                    .map(|(_, v)| *v)
                    .max()
                    .unwrap_or(Decimal::ZERO);
                if current <= second || current.checked_sub(second) != Some(a.amount) {
                    return Err(violation(
                        hand,
                        format!(
                            "uncalled bet of {} to '{}' is not the unmatched part of the largest bet",
                            a.amount, a.player_name
                        ),
                    ));
                }
                state.committed.insert(s, a.street_total_after);
                continue;
            }

            if state.live_count(&seats) <= 1 {
                return Err(violation(
                    hand,
                    format!("'{}' acts after the hand was decided", a.player_name),
                ));
            }
            if next_clockwise(&seats, anchor, |x| needs.contains(&x)) != Some(s) {
                return Err(violation(
                    hand,
                    format!(
                        "'{}' acts out of turn on the {street:?} (action {})",
                        a.player_name, a.sequence
                    ),
                ));
            }
            let total = a.street_total_after;
            let goes_all_in = state.gross(s).checked_add(a.amount) == Some(stack(s));
            let opponent_can_respond = seats.iter().any(|x| *x != s && state.can_act(*x));
            let increment = total.checked_sub(prev_max).unwrap_or(Decimal::ZERO);
            // A player who already acted may raise again only when facing at least a full
            // raise since then (a check never gives up that right).
            let may_raise = !acted.contains(&s)
                || current.is_zero()
                || prev_max
                    .checked_sub(current)
                    .is_some_and(|d| d >= min_raise);
            let legal = match a.action_type {
                ActionType::Fold => true,
                ActionType::Check => current == prev_max,
                ActionType::Call => total <= prev_max && (total == prev_max || goes_all_in),
                ActionType::Bet => {
                    prev_max.is_zero()
                        && opponent_can_respond
                        && (a.amount >= big_blind || goes_all_in)
                }
                // After an all-in for less than the big blind, raising to the big blind
                // ("completing") is also a legal raise.
                ActionType::Raise => {
                    prev_max > Decimal::ZERO
                        && total > prev_max
                        && opponent_can_respond
                        && may_raise
                        && (increment >= min_raise
                            || goes_all_in
                            || (prev_max < big_blind && total >= big_blind))
                }
                _ => false,
            };
            if !legal {
                return Err(violation(
                    hand,
                    format!(
                        "illegal {} to {total} by '{}' facing {prev_max} on the {street:?}",
                        a.action_type.as_str(),
                        a.player_name
                    ),
                ));
            }

            needs.remove(&s);
            acted.insert(s);
            state.committed.insert(s, total);
            if a.action_type.is_contribution() {
                state
                    .commit(s, a.amount, stack(s))
                    .ok_or_else(|| violation(hand, "contribution overflow"))?;
            }
            if a.action_type == ActionType::Fold {
                state.folded.insert(s);
            }
            if total > prev_max {
                if increment >= min_raise {
                    min_raise = increment;
                }
                needs = seats
                    .iter()
                    .copied()
                    .filter(|x| *x != s && state.can_act(*x))
                    .collect();
            }
            state.settle_lone_player(&seats, &mut needs);
            anchor = s;
        }

        if state.live_count(&seats) >= 2 && !needs.is_empty() {
            return Err(violation(
                hand,
                format!(
                    "the {street:?} betting ended before {} player(s) acted",
                    needs.len()
                ),
            ));
        }
    }

    if idx < hand.actions.len() {
        return Err(violation(
            hand,
            "actions recorded on a street whose cards were never dealt",
        ));
    }
    if state.live_count(&seats) >= 2 && !reached(Street::River) {
        return Err(violation(
            hand,
            "hand contested by several players ended before the river",
        ));
    }
    Ok(())
}

/// Validates every structural, monetary, seating, betting, board, pot-conservation and
/// card-uniqueness invariant of a [`CanonicalHand`]. Site-independent.
///
/// Invoked by every parser after parsing and again by the storage layer before persisting.
///
/// # Errors
/// Returns the first [`ParseError`] describing a violated invariant.
pub fn validate_hand(hand: &CanonicalHand) -> Result<(), ParseError> {
    validate_header(hand)?;
    let hole_mask = validate_players(hand)?;
    let by_name: HashMap<&str, &Player> =
        hand.players.iter().map(|p| (p.name.as_str(), p)).collect();
    validate_actions(hand, &by_name)?;
    let run_count = validate_boards(hand, hole_mask)?;
    validate_betting(hand)?;
    validate_outcome(hand, &by_name, run_count)?;
    Ok(())
}

/// Iterator of parsed hand blocks produced by [`HandParser::parse_session`].
pub type ParsedBlocks<'a> =
    Box<dyn Iterator<Item = (HandBlock<'a>, Result<CanonicalHand, ParseError>)> + 'a>;

/// Contract implemented by every site parser. Object-safe, so parsers can be registered in a
/// [`crate::parser::ParserRegistry`] and selected per file at runtime.
pub trait HandParser {
    /// The [`Platform`] whose hand histories this parser reads.
    fn platform(&self) -> Platform;

    /// Short human-readable description used in diagnostics (e.g. `"GGPoker (CAD)"`).
    fn describe(&self) -> String;

    /// Returns `true` if `raw_session` looks like this site's export format. Must be cheap and
    /// must not claim files from other sites; it should inspect only the opening line(s).
    fn recognizes(&self, raw_session: &str) -> bool;

    /// Splits a session file into hand blocks. Defaults to blank-line separation.
    fn split_blocks<'a>(&self, raw_session: &'a str) -> Vec<HandBlock<'a>> {
        split_hand_blocks(raw_session).collect()
    }

    /// Parses a single raw hand block into a validated [`CanonicalHand`].
    ///
    /// # Errors
    /// Returns [`ParseError`] if syntax, betting rules, summary cross-checks or invariants fail.
    fn parse_hand(&self, raw_block: &str) -> Result<CanonicalHand, ParseError>;

    /// Best-effort extraction of the site hand id from a raw block, used for diagnostics when
    /// parsing fails.
    fn peek_hand_id(&self, raw_block: &str) -> Option<String>;

    /// Lazily parses every hand block in a session, yielding each block with its result.
    fn parse_session<'a>(&'a self, raw_session: &'a str) -> ParsedBlocks<'a> {
        Box::new(
            self.split_blocks(raw_session)
                .into_iter()
                .map(move |block| (block, self.parse_hand(block.text))),
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::domain::canonical::{
        AdjustmentKind, BettingLimit, CommunityCards, DeductionKind, GameType, HandHeader,
        HandOutcome, PlayerAdjustment, PotDeduction, PotWinner, StreetAction,
    };
    use crate::domain::money::CurrencyCode;
    use chrono::NaiveDate;
    use rust_decimal_macros::dec;
    use std::str::FromStr;

    fn c(s: &str) -> Card {
        Card::from_str(s).unwrap()
    }

    fn act(
        seq: u16,
        street: Street,
        name: &str,
        t: ActionType,
        amount: Decimal,
        total: Decimal,
    ) -> StreetAction {
        StreetAction {
            sequence: seq,
            street,
            player_name: name.to_string(),
            action_type: t,
            amount,
            street_total_after: total,
            is_all_in: false,
        }
    }

    fn player(seat: u8, name: &str, cards: [Option<Card>; 2], hero: bool) -> Player {
        Player {
            seat,
            name: name.to_string(),
            starting_stack: dec!(10),
            hole_cards: cards,
            is_hero: hero,
        }
    }

    /// SB limps, BB checks, SB bets the flop, BB folds, bet returned. Pot 0.20, rake 0.01.
    fn valid_hand() -> CanonicalHand {
        use ActionType::*;
        use Street::*;
        CanonicalHand {
            header: HandHeader {
                platform: Platform::GGPoker,
                site_hand_id: "HDTEST1".into(),
                game_type: GameType::Cash,
                limit: BettingLimit::NoLimit,
                currency: CurrencyCode::CAD,
                small_blind: dec!(0.05),
                big_blind: dec!(0.1),
                timestamp: NaiveDate::from_ymd_opt(2026, 8, 21)
                    .unwrap()
                    .and_hms_opt(17, 0, 0)
                    .unwrap(),
                utc_offset_seconds: None,
                table_name: "T".into(),
                max_seats: 6,
                button_seat: 3,
            },
            players: vec![
                player(1, "A", [None, None], false),
                player(2, "Hero", [Some(c("As")), Some(c("Kd"))], true),
                player(3, "C", [None, None], false),
            ],
            actions: vec![
                act(0, Preflop, "A", PostSmallBlind, dec!(0.05), dec!(0.05)),
                act(1, Preflop, "Hero", PostBigBlind, dec!(0.1), dec!(0.1)),
                act(2, Preflop, "C", Fold, dec!(0), dec!(0)),
                act(3, Preflop, "A", Call, dec!(0.05), dec!(0.1)),
                act(4, Preflop, "Hero", Check, dec!(0), dec!(0.1)),
                act(5, Flop, "A", Bet, dec!(0.2), dec!(0.2)),
                act(6, Flop, "Hero", Fold, dec!(0), dec!(0)),
                act(7, Flop, "A", UncalledBetReturned, dec!(0.2), dec!(0)),
            ],
            boards: vec![CommunityCards {
                run_index: 0,
                flop: Some([c("2c"), c("7d"), c("9h")]),
                turn: None,
                river: None,
            }],
            outcome: HandOutcome {
                total_pot: dec!(0.2),
                deductions: vec![PotDeduction {
                    kind: DeductionKind::Rake,
                    label: "Rake".into(),
                    amount: dec!(0.01),
                }],
                uncalled_returned: dec!(0.2),
                winners: vec![PotWinner {
                    player_name: "A".into(),
                    amount: dec!(0.19),
                    run_index: 0,
                }],
                adjustments: vec![],
            },
        }
    }

    fn assert_violation(h: &CanonicalHand) {
        assert!(
            validate_hand(h).is_err(),
            "expected validator to reject mutated hand"
        );
    }

    #[test]
    fn baseline_hand_is_valid_and_results_reconcile() {
        let h = valid_hand();
        validate_hand(&h).unwrap();
        let r = h.checked_player_results().unwrap();
        assert_eq!(r[0].net, dec!(0.09));
        assert_eq!(r[1].net, dec!(-0.1));
        assert_eq!(r[2].net, dec!(0));
        let total: Decimal = r.iter().map(|x| x.net).sum();
        assert_eq!(total, dec!(-0.01), "sum of nets equals minus the rake");
    }

    #[test]
    fn rejects_actions_that_do_not_sum_to_the_pot() {
        // Summary arithmetic still balances (0.25 - 0.06 = 0.19), but actions put in only 0.20.
        let mut h = valid_hand();
        h.outcome.total_pot = dec!(0.25);
        h.outcome.deductions[0].amount = dec!(0.06);
        assert!(matches!(
            validate_hand(&h),
            Err(ParseError::PotContributionMismatch { .. })
        ));
    }

    #[test]
    fn rejects_broken_street_total_chain() {
        let mut h = valid_hand();
        h.actions[0].street_total_after = dec!(7);
        assert_violation(&h);
    }

    #[test]
    fn rejects_zero_or_multiple_heroes() {
        let mut h = valid_hand();
        h.players[0].is_hero = true;
        assert_violation(&h);
        let mut h = valid_hand();
        h.players[1].is_hero = false;
        assert_violation(&h);
    }

    #[test]
    fn rejects_commitment_beyond_stack_and_false_all_in() {
        let mut h = valid_hand();
        h.players[0].starting_stack = dec!(0.05);
        assert_violation(&h);
        let mut h = valid_hand();
        h.actions[3].is_all_in = true;
        assert_violation(&h);
    }

    #[test]
    fn rejects_out_of_order_streets_and_sequences() {
        let mut h = valid_hand();
        h.actions[0].street = Street::River;
        assert_violation(&h);
        let mut h = valid_hand();
        h.actions[2].sequence = 9;
        assert_violation(&h);
    }

    #[test]
    fn rejects_folded_winner_and_action_after_fold() {
        let mut h = valid_hand();
        h.outcome.winners[0].player_name = "C".into();
        assert_violation(&h);
        let mut h = valid_hand();
        h.actions[4].player_name = "C".into();
        assert_violation(&h);
    }

    #[test]
    fn rejects_mismatched_summary_accounting() {
        let mut h = valid_hand();
        h.outcome.deductions[0].amount = dec!(0.02);
        assert!(matches!(
            validate_hand(&h),
            Err(ParseError::MismatchedPotAccounting { .. })
        ));
    }

    #[test]
    fn rejects_duplicate_physical_cards() {
        let mut h = valid_hand();
        h.boards[0].flop = Some([c("As"), c("7d"), c("9h")]);
        assert_eq!(
            validate_hand(&h),
            Err(ParseError::DuplicateCard { card: "As".into() })
        );
    }

    #[test]
    fn rejects_non_canonical_partial_hole_cards_and_unlabeled_deductions() {
        let mut h = valid_hand();
        h.players[0].hole_cards = [None, Some(c("2s"))];
        assert_violation(&h);
        let mut h = valid_hand();
        h.outcome.deductions[0].label = " ".into();
        assert_violation(&h);
    }

    #[test]
    fn validates_adjustment_players_and_signs() {
        let adjustment = |name: &str, kind: AdjustmentKind, amount: Decimal| PlayerAdjustment {
            player_name: name.into(),
            kind,
            street: Street::Flop,
            amount,
        };
        let mut h = valid_hand();
        h.outcome.adjustments.push(adjustment(
            "A",
            AdjustmentKind::EvCashoutRiskPaid,
            dec!(-0.05),
        ));
        validate_hand(&h).unwrap();
        assert_eq!(h.checked_player_results().unwrap()[0].net, dec!(0.04));

        let mut h = valid_hand();
        h.outcome.adjustments.push(adjustment(
            "A",
            AdjustmentKind::EvCashoutRiskPaid,
            dec!(0.05),
        ));
        assert_violation(&h);
        let mut h = valid_hand();
        h.outcome.adjustments.push(adjustment(
            "Nobody",
            AdjustmentKind::EvCashoutRiskPaid,
            dec!(-0.05),
        ));
        assert!(matches!(
            validate_hand(&h),
            Err(ParseError::UnknownPlayer { .. })
        ));

        // Multiple adjustments for one player
        let mut h = valid_hand();
        h.outcome.adjustments.push(adjustment(
            "A",
            AdjustmentKind::EvCashoutRiskPaid,
            dec!(-0.05),
        ));
        h.outcome.adjustments.push(adjustment(
            "A",
            AdjustmentKind::EvCashoutRiskPaid,
            dec!(-0.02),
        ));
        assert_violation(&h);

        // EvCashoutReceived for winning player
        let mut h = valid_hand();
        h.outcome.adjustments.push(adjustment(
            "A",
            AdjustmentKind::EvCashoutReceived,
            dec!(0.05),
        ));
        assert_violation(&h);

        // EvCashoutRiskPaid for non-winning player
        let mut h = valid_hand();
        h.outcome.adjustments.push(adjustment(
            "Hero",
            AdjustmentKind::EvCashoutRiskPaid,
            dec!(-0.05),
        ));
        assert_violation(&h);

        // EvCashoutReceived for folded player
        let mut h = valid_hand();
        h.outcome.adjustments.push(adjustment(
            "Hero",
            AdjustmentKind::EvCashoutReceived,
            dec!(0.05),
        ));
        assert_violation(&h);
    }

    #[test]
    fn rejects_payout_on_nonexistent_run() {
        let mut h = valid_hand();
        h.outcome.winners[0].run_index = 2;
        assert_violation(&h);
    }

    fn renumber(h: &mut CanonicalHand) {
        for (i, a) in h.actions.iter_mut().enumerate() {
            a.sequence = u16::try_from(i).unwrap();
        }
    }

    fn betting_error(h: &CanonicalHand) -> String {
        match validate_hand(h) {
            Err(ParseError::InvariantViolation { detail, .. }) => detail,
            other => panic!("expected an invariant violation, got {other:?}"),
        }
    }

    #[test]
    fn rejects_actions_out_of_turn() {
        // Preflop action starts left of the big blind (seat 3), not with the small blind.
        let mut h = valid_hand();
        h.actions.swap(2, 3);
        renumber(&mut h);
        assert!(betting_error(&h).contains("out of turn"));
        // The same player checking twice in a row.
        let mut h = valid_hand();
        let repeat = h.actions[4].clone();
        h.actions.insert(5, repeat);
        renumber(&mut h);
        assert!(betting_error(&h).contains("out of turn"));
    }

    #[test]
    fn rejects_a_betting_round_that_closes_before_everyone_acted() {
        // The big blind never uses the option, yet the flop is dealt.
        let mut h = valid_hand();
        h.actions.remove(4);
        renumber(&mut h);
        assert!(betting_error(&h).contains("ended before"));
    }

    #[test]
    fn rejects_actions_and_cards_on_streets_that_cannot_exist() {
        // Flop actions without a flop.
        let mut h = valid_hand();
        h.boards.clear();
        assert!(betting_error(&h).contains("never dealt"));
        // A turn dealt after everyone but one player folded.
        let mut h = valid_hand();
        h.boards[0].turn = Some(c("Qc"));
        assert!(betting_error(&h).contains("after the hand was decided"));
    }

    #[test]
    fn rejects_an_uncalled_bet_of_the_wrong_size() {
        let mut h = valid_hand();
        h.actions[7].amount = dec!(0.1);
        h.actions[7].street_total_after = dec!(0.1);
        h.outcome.uncalled_returned = dec!(0.1);
        assert!(betting_error(&h).contains("unmatched part"));
    }

    #[test]
    fn rejects_a_contested_hand_that_ends_before_the_river() {
        // Hero calls the flop bet instead of folding, but no turn or river follows.
        let mut h = valid_hand();
        h.actions[6].action_type = ActionType::Call;
        h.actions[6].amount = dec!(0.2);
        h.actions[6].street_total_after = dec!(0.2);
        h.actions.truncate(7);
        h.outcome.uncalled_returned = dec!(0);
        assert!(betting_error(&h).contains("before the river"));
    }

    #[test]
    fn rejects_an_action_by_a_player_who_is_already_all_in() {
        // A's limp puts their whole 0.10 stack in; they cannot bet the flop afterwards.
        let mut h = valid_hand();
        h.players[0].starting_stack = dec!(0.1);
        h.actions[3].is_all_in = true;
        h.actions[5].amount = dec!(0.2);
        let err = validate_hand(&h).unwrap_err();
        assert!(
            matches!(err, ParseError::InvariantViolation { .. }),
            "{err:?}"
        );
    }

    /// Builds a consistent hand from betting steps. Each step's amount is the chips the action
    /// adds, except for raises, where it is the street total raised to (and for an uncalled bet,
    /// the chips returned). Seats follow `stacks` order from 1; the player named "Hero" holds
    /// As Kd. All-in flags, street totals, sequences, the pot and the returns are derived, and
    /// `streets_dealt` (0..=3) board streets are dealt from 2c 7d 9h / Qc / 3s.
    fn scenario(
        stacks: &[(&str, Decimal)],
        button: u8,
        steps: &[(Street, &str, ActionType, Decimal)],
        streets_dealt: usize,
        winners: &[(&str, Decimal)],
    ) -> CanonicalHand {
        let mut h = valid_hand();
        h.header.button_seat = button;
        h.players = stacks
            .iter()
            .enumerate()
            .map(|(i, (name, st))| Player {
                seat: u8::try_from(i + 1).unwrap(),
                name: (*name).to_string(),
                starting_stack: *st,
                hole_cards: if *name == "Hero" {
                    [Some(c("As")), Some(c("Kd"))]
                } else {
                    [None, None]
                },
                is_hero: *name == "Hero",
            })
            .collect();
        let mut committed: HashMap<(Street, &str), Decimal> = HashMap::new();
        let mut gross: HashMap<&str, Decimal> = HashMap::new();
        let (mut pot, mut returned) = (Decimal::ZERO, Decimal::ZERO);
        h.actions.clear();
        for (i, &(street, name, t, x)) in steps.iter().enumerate() {
            let cur = committed.get(&(street, name)).copied().unwrap_or_default();
            let (amount, total) = match t {
                ActionType::Raise => (x - cur, x),
                ActionType::UncalledBetReturned => (x, cur - x),
                ActionType::Fold | ActionType::Check => (Decimal::ZERO, cur),
                ActionType::PostDeadBlind => (x, cur),
                _ => (x, cur + x),
            };
            committed.insert((street, name), total);
            let stack = stacks.iter().find(|(n, _)| *n == name).unwrap().1;
            let mut all_in = false;
            if t.is_contribution() {
                let g = gross.get(name).copied().unwrap_or_default() + amount;
                gross.insert(name, g);
                all_in = g == stack;
                pot += amount;
            } else if t == ActionType::UncalledBetReturned {
                pot -= amount;
                returned += amount;
            }
            let mut a = act(u16::try_from(i).unwrap(), street, name, t, amount, total);
            a.is_all_in = all_in;
            h.actions.push(a);
        }
        h.boards = if streets_dealt == 0 {
            vec![]
        } else {
            vec![CommunityCards {
                run_index: 0,
                flop: Some([c("2c"), c("7d"), c("9h")]),
                turn: (streets_dealt >= 2).then(|| c("Qc")),
                river: (streets_dealt >= 3).then(|| c("3s")),
            }]
        };
        h.outcome = HandOutcome {
            total_pot: pot,
            deductions: vec![PotDeduction {
                kind: DeductionKind::Rake,
                label: "Rake".into(),
                amount: Decimal::ZERO,
            }],
            uncalled_returned: returned,
            winners: winners
                .iter()
                .map(|(n, amt)| PotWinner {
                    player_name: (*n).to_string(),
                    amount: *amt,
                    run_index: 0,
                })
                .collect(),
            adjustments: vec![],
        };
        h
    }

    const TEN: Decimal = dec!(10);

    #[test]
    fn a_short_all_in_preflop_closes_the_betting_for_the_big_blind() {
        use ActionType::*;
        use Street::*;
        // C (first to act) calls all-in for less than the big blind and A folds: the big blind
        // has nobody left to bet against, so the unmatched 0.02 goes straight back.
        let h = scenario(
            &[("A", TEN), ("Hero", TEN), ("C", dec!(0.08))],
            3,
            &[
                (Preflop, "A", PostSmallBlind, dec!(0.05)),
                (Preflop, "Hero", PostBigBlind, dec!(0.1)),
                (Preflop, "C", Call, dec!(0.08)),
                (Preflop, "A", Fold, dec!(0)),
                (Preflop, "Hero", UncalledBetReturned, dec!(0.02)),
            ],
            3,
            &[("C", dec!(0.21))],
        );
        assert!(h.actions[2].is_all_in);
        validate_hand(&h).unwrap();

        // All-in for exactly the big blind: the big blind owes no action and the board runs out.
        let h = scenario(
            &[("A", TEN), ("Hero", TEN), ("C", dec!(0.1))],
            3,
            &[
                (Preflop, "A", PostSmallBlind, dec!(0.05)),
                (Preflop, "Hero", PostBigBlind, dec!(0.1)),
                (Preflop, "C", Call, dec!(0.1)),
                (Preflop, "A", Fold, dec!(0)),
            ],
            3,
            &[("Hero", dec!(0.25))],
        );
        validate_hand(&h).unwrap();

        // A small blind posted all-in for less than the stake is still a legal blind.
        let h = scenario(
            &[("A", dec!(0.03)), ("Hero", TEN), ("C", TEN)],
            3,
            &[
                (Preflop, "A", PostSmallBlind, dec!(0.03)),
                (Preflop, "Hero", PostBigBlind, dec!(0.1)),
                (Preflop, "C", Fold, dec!(0)),
                (Preflop, "Hero", UncalledBetReturned, dec!(0.07)),
            ],
            3,
            &[("A", dec!(0.06))],
        );
        validate_hand(&h).unwrap();
    }

    #[test]
    fn rejects_raises_without_a_bet_or_without_an_opponent_who_can_respond() {
        use ActionType::*;
        use Street::*;
        // A "raise" on a flop where nobody has bet.
        let mut h = valid_hand();
        h.actions[5].action_type = Raise;
        assert!(betting_error(&h).contains("illegal Raise"));

        // C shoves, A folds: the big blind can call, but raising is pointless and illegal
        // because nobody is left who could respond.
        let steps = |hero: (ActionType, Decimal)| {
            let mut steps = vec![
                (Preflop, "A", PostSmallBlind, dec!(0.05)),
                (Preflop, "Hero", PostBigBlind, dec!(0.1)),
                (Preflop, "C", Raise, dec!(0.5)),
                (Preflop, "A", Fold, dec!(0)),
                (Preflop, "Hero", hero.0, hero.1),
            ];
            if hero.0 == Raise {
                steps.push((Preflop, "Hero", UncalledBetReturned, dec!(1)));
            }
            steps
        };
        let stacks = [("A", TEN), ("Hero", TEN), ("C", dec!(0.5))];
        let call = scenario(
            &stacks,
            3,
            &steps((Call, dec!(0.4))),
            3,
            &[("C", dec!(1.05))],
        );
        validate_hand(&call).unwrap();
        let raise = scenario(
            &stacks,
            3,
            &steps((Raise, dec!(1.5))),
            3,
            &[("C", dec!(1.05))],
        );
        assert!(betting_error(&raise).contains("illegal Raise"));
    }

    #[test]
    fn rejects_undersized_bets_and_raises_unless_all_in() {
        use ActionType::*;
        use Street::*;
        // A flop bet smaller than the big blind.
        let mut h = valid_hand();
        h.actions[5].amount = dec!(0.05);
        h.actions[5].street_total_after = dec!(0.05);
        h.actions[6].street_total_after = dec!(0);
        h.actions[7].amount = dec!(0.05);
        h.outcome.uncalled_returned = dec!(0.05);
        assert!(betting_error(&h).contains("illegal Bet"));

        // A preflop raise of only 0.05 over the big blind, legal only as an all-in.
        let steps = [
            (Preflop, "A", PostSmallBlind, dec!(0.05)),
            (Preflop, "Hero", PostBigBlind, dec!(0.1)),
            (Preflop, "C", Raise, dec!(0.15)),
            (Preflop, "A", Fold, dec!(0)),
            (Preflop, "Hero", Fold, dec!(0)),
            (Preflop, "C", UncalledBetReturned, dec!(0.05)),
        ];
        let winners = [("C", dec!(0.25))];
        let short = scenario(
            &[("A", TEN), ("Hero", TEN), ("C", TEN)],
            3,
            &steps,
            0,
            &winners,
        );
        assert!(betting_error(&short).contains("illegal Raise"));
        let all_in = scenario(
            &[("A", TEN), ("Hero", TEN), ("C", dec!(0.15))],
            3,
            &steps,
            0,
            &winners,
        );
        validate_hand(&all_in).unwrap();

        // After an all-in flop bet of 0.05, raising to the 0.10 big blind completes the bet.
        let steps = [
            (Preflop, "A", PostSmallBlind, dec!(0.05)),
            (Preflop, "Hero", PostBigBlind, dec!(0.1)),
            (Preflop, "C", Call, dec!(0.1)),
            (Preflop, "A", Call, dec!(0.05)),
            (Preflop, "Hero", Check, dec!(0)),
            (Flop, "A", Bet, dec!(0.05)),
            (Flop, "Hero", Raise, dec!(0.1)),
            (Flop, "C", Fold, dec!(0)),
            (Flop, "Hero", UncalledBetReturned, dec!(0.05)),
        ];
        let stacks = [("A", dec!(0.15)), ("Hero", TEN), ("C", TEN)];
        let h = scenario(&stacks, 3, &steps, 3, &[("A", dec!(0.4))]);
        validate_hand(&h).unwrap();
    }

    #[test]
    fn an_incomplete_all_in_raise_does_not_reopen_the_betting() {
        use ActionType::*;
        use Street::*;
        // C raises to 0.30 (a full raise of 0.20); D shoves 0.40, only 0.10 more. C already
        // acted and faces less than a full raise, so C may call or fold but not raise.
        let stacks = [("A", TEN), ("Hero", TEN), ("C", TEN), ("D", dec!(0.4))];
        let opening = [
            (Preflop, "A", PostSmallBlind, dec!(0.05)),
            (Preflop, "Hero", PostBigBlind, dec!(0.1)),
            (Preflop, "C", Raise, dec!(0.3)),
            (Preflop, "D", Raise, dec!(0.4)),
            (Preflop, "A", Fold, dec!(0)),
            (Preflop, "Hero", Call, dec!(0.3)),
        ];
        let mut call = opening.to_vec();
        call.push((Preflop, "C", Call, dec!(0.1)));
        for street in [Flop, Turn, River] {
            call.push((street, "Hero", Check, dec!(0)));
            call.push((street, "C", Check, dec!(0)));
        }
        let h = scenario(&stacks, 4, &call, 3, &[("D", dec!(1.25))]);
        validate_hand(&h).unwrap();

        let mut reraise = opening.to_vec();
        reraise.push((Preflop, "C", Raise, dec!(1.2)));
        reraise.push((Preflop, "Hero", Fold, dec!(0)));
        reraise.push((Preflop, "C", UncalledBetReturned, dec!(0.8)));
        let h = scenario(&stacks, 4, &reraise, 3, &[("D", dec!(1.25))]);
        assert!(betting_error(&h).contains("illegal Raise"));

        // Two short shoves that together add a full raise (0.40 -> 0.55 after 0.30) do reopen it.
        let stacks = [
            ("A", dec!(0.55)),
            ("Hero", TEN),
            ("C", TEN),
            ("D", dec!(0.4)),
        ];
        let steps = [
            (Preflop, "A", PostSmallBlind, dec!(0.05)),
            (Preflop, "Hero", PostBigBlind, dec!(0.1)),
            (Preflop, "C", Raise, dec!(0.3)),
            (Preflop, "D", Raise, dec!(0.4)),
            (Preflop, "A", Raise, dec!(0.55)),
            (Preflop, "Hero", Call, dec!(0.45)),
            (Preflop, "C", Raise, dec!(1.2)),
            (Preflop, "Hero", Fold, dec!(0)),
            (Preflop, "C", UncalledBetReturned, dec!(0.65)),
        ];
        let h = scenario(
            &stacks,
            4,
            &steps,
            3,
            &[("D", dec!(1.6)), ("A", dec!(0.45))],
        );
        validate_hand(&h).unwrap();
    }

    #[test]
    fn rejects_a_whole_stack_commitment_not_marked_all_in() {
        let mut h = valid_hand();
        h.players[0].starting_stack = dec!(0.1);
        h.actions.truncate(5);
        h.boards.clear();
        h.outcome.uncalled_returned = dec!(0);
        assert!(betting_error(&h).contains("without being marked all-in"));
    }

    #[test]
    fn rejects_blinds_that_do_not_match_the_stakes() {
        // A 0.06 small blind; A's completion shrinks to 0.04, so every total still balances.
        let mut h = valid_hand();
        h.actions[0].amount = dec!(0.06);
        h.actions[0].street_total_after = dec!(0.06);
        h.actions[3].amount = dec!(0.04);
        assert!(betting_error(&h).contains("does not match the 0.05/0.1 stakes"));

        // A 0.25 big blind.
        let mut h = valid_hand();
        h.actions[1].amount = dec!(0.25);
        h.actions[1].street_total_after = dec!(0.25);
        h.actions[3].amount = dec!(0.2);
        h.actions[3].street_total_after = dec!(0.25);
        h.actions[4].street_total_after = dec!(0.25);
        assert!(betting_error(&h).contains("PostBigBlind of 0.25"));

        // Dead money larger than a big blind.
        let mut h = valid_hand();
        h.actions[2] = act(
            2,
            Street::Preflop,
            "C",
            ActionType::PostDeadBlind,
            dec!(0.15),
            dec!(0),
        );
        h.actions.insert(
            3,
            act(3, Street::Preflop, "C", ActionType::Fold, dec!(0), dec!(0)),
        );
        renumber(&mut h);
        h.outcome.total_pot = dec!(0.35);
        h.outcome.winners[0].amount = dec!(0.34);
        assert!(betting_error(&h).contains("PostDeadBlind of 0.15"));
    }

    #[test]
    fn rejects_payouts_beyond_side_pot_eligibility() {
        use ActionType::*;
        use Street::*;
        // A is all-in for 0.20 against two 1.00 stacks: A can win the 0.60 main pot only.
        let mut steps = vec![
            (Preflop, "A", PostSmallBlind, dec!(0.05)),
            (Preflop, "Hero", PostBigBlind, dec!(0.1)),
            (Preflop, "C", Raise, dec!(1)),
            (Preflop, "A", Call, dec!(0.15)),
            (Preflop, "Hero", Call, dec!(0.9)),
        ];
        for street in [Flop, Turn, River] {
            steps.push((street, "Hero", Check, dec!(0)));
            steps.push((street, "C", Check, dec!(0)));
        }
        let stacks = [("A", dec!(0.2)), ("Hero", TEN), ("C", TEN)];
        let fair = scenario(&stacks, 3, &steps, 3, &[("A", dec!(0.6)), ("C", dec!(1.6))]);
        validate_hand(&fair).unwrap();
        for winners in [
            vec![("A", dec!(2.2))],
            vec![("A", dec!(1)), ("C", dec!(1.2))],
        ] {
            let h = scenario(&stacks, 3, &steps, 3, &winners);
            assert!(betting_error(&h).contains("more than the 0.60 they could win"));
        }
    }

    #[test]
    fn straddle_validator_rules() {
        use ActionType::*;
        use Street::*;
        let stacks = [("A", TEN), ("B", TEN), ("C", TEN), ("Hero", TEN)];

        // 1. Anchor shift: with C straddling, Hero (seat 4) must act first preflop.
        // If A acts before Hero, it is out of turn.
        let out_of_turn = [
            (Preflop, "A", PostSmallBlind, dec!(0.05)),
            (Preflop, "B", PostBigBlind, dec!(0.1)),
            (Preflop, "C", PostStraddle, dec!(0.2)),
            (Preflop, "A", Fold, dec!(0)), // A acts out of turn; Hero should act first
        ];
        let h = scenario(&stacks, 4, &out_of_turn, 0, &[("C", dec!(0.35))]);
        assert!(betting_error(&h).contains("acts out of turn on the Preflop"));

        // Valid order: Hero -> A -> B -> C (straddler checks their option)
        let valid_option_check = [
            (Preflop, "A", PostSmallBlind, dec!(0.05)),
            (Preflop, "B", PostBigBlind, dec!(0.1)),
            (Preflop, "C", PostStraddle, dec!(0.2)),
            (Preflop, "Hero", Fold, dec!(0)),
            (Preflop, "A", Fold, dec!(0)),
            (Preflop, "B", Call, dec!(0.1)),
            (Preflop, "C", Check, dec!(0)),
            (Flop, "B", Check, dec!(0)),
            (Flop, "C", Bet, dec!(0.2)),
            (Flop, "B", Fold, dec!(0)),
            (Flop, "C", UncalledBetReturned, dec!(0.2)),
        ];
        let h = scenario(&stacks, 4, &valid_option_check, 1, &[("C", dec!(0.45))]);
        validate_hand(&h).unwrap();

        // 2. Straddler option: if C raises instead of checking, it is legal.
        let valid_option_raise = [
            (Preflop, "A", PostSmallBlind, dec!(0.05)),
            (Preflop, "B", PostBigBlind, dec!(0.1)),
            (Preflop, "C", PostStraddle, dec!(0.2)),
            (Preflop, "Hero", Fold, dec!(0)),
            (Preflop, "A", Fold, dec!(0)),
            (Preflop, "B", Call, dec!(0.1)),
            (Preflop, "C", Raise, dec!(0.6)), // C exercises option by raising to 0.60
            (Preflop, "B", Fold, dec!(0)),
            (Preflop, "C", UncalledBetReturned, dec!(0.4)),
        ];
        let h = scenario(&stacks, 4, &valid_option_raise, 0, &[("C", dec!(0.45))]);
        validate_hand(&h).unwrap();

        // If C's option is omitted (round ends before C acts), betting ends early.
        let omitted_option = [
            (Preflop, "A", PostSmallBlind, dec!(0.05)),
            (Preflop, "B", PostBigBlind, dec!(0.1)),
            (Preflop, "C", PostStraddle, dec!(0.2)),
            (Preflop, "Hero", Fold, dec!(0)),
            (Preflop, "A", Fold, dec!(0)),
            (Preflop, "B", Call, dec!(0.1)),
            (Flop, "B", Check, dec!(0)),
        ];
        let h = scenario(&stacks, 4, &omitted_option, 1, &[("B", dec!(0.45))]);
        assert!(betting_error(&h).contains("the Preflop betting ended before 1 player(s) acted"));

        // 3. 2x size requirement: straddle must be exactly 2x highest previous live commitment.
        let wrong_size = [
            (Preflop, "A", PostSmallBlind, dec!(0.05)),
            (Preflop, "B", PostBigBlind, dec!(0.1)),
            (Preflop, "C", PostStraddle, dec!(0.3)), // 0.3 != 2 * 0.1
        ];
        let h = scenario(&stacks, 4, &wrong_size, 0, &[("C", dec!(0.45))]);
        assert!(
            betting_error(&h).contains("PostStraddle of 0.3 by 'C' does not match required size")
        );

        // 4. Maximum of 2 straddles: a 3rd straddle is rejected.
        let stacks5 = [
            ("A", TEN),
            ("B", TEN),
            ("C", TEN),
            ("D", TEN),
            ("Hero", TEN),
        ];
        let three_straddles = [
            (Preflop, "A", PostSmallBlind, dec!(0.05)),
            (Preflop, "B", PostBigBlind, dec!(0.1)),
            (Preflop, "C", PostStraddle, dec!(0.2)),
            (Preflop, "D", PostStraddle, dec!(0.4)),
            (Preflop, "Hero", PostStraddle, dec!(0.8)),
        ];
        let h = scenario(&stacks5, 5, &three_straddles, 0, &[("Hero", dec!(1.55))]);
        assert!(betting_error(&h).contains("more than two straddles"));

        // 5. Minimum raise over a straddle: over 0.20, min raise is to 0.40 (raising to 0.30 is illegal).
        let small_raise = [
            (Preflop, "A", PostSmallBlind, dec!(0.05)),
            (Preflop, "B", PostBigBlind, dec!(0.1)),
            (Preflop, "C", PostStraddle, dec!(0.2)),
            (Preflop, "Hero", Raise, dec!(0.3)), // Min raise is to 0.40!
        ];
        let h = scenario(&stacks, 4, &small_raise, 0, &[("Hero", dec!(0.65))]);
        assert!(betting_error(&h).contains("illegal Raise to 0.3"));

        // Over double straddle (0.20 then 0.40), min raise is to 0.80 (raising to 0.70 is illegal).
        let small_double_raise = [
            (Preflop, "A", PostSmallBlind, dec!(0.05)),
            (Preflop, "B", PostBigBlind, dec!(0.1)),
            (Preflop, "C", PostStraddle, dec!(0.2)),
            (Preflop, "D", PostStraddle, dec!(0.4)),
            (Preflop, "Hero", Raise, dec!(0.7)), // Min raise is to 0.80!
        ];
        let h = scenario(&stacks5, 5, &small_double_raise, 0, &[("Hero", dec!(1.45))]);
        assert!(betting_error(&h).contains("illegal Raise to 0.7"));
    }

    #[test]
    fn split_hand_blocks_handles_crlf_bom_and_reports_start_lines() {
        let raw = "\u{feff}Hand one line 1\r\nHand one line 2\r\n\r\n\r\nHand two line 1\nHand two line 2\n\n";
        let blocks: Vec<_> = split_hand_blocks(raw).collect();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].text, "Hand one line 1\r\nHand one line 2");
        assert_eq!(blocks[0].start_line, 1);
        assert_eq!(blocks[1].text, "Hand two line 1\nHand two line 2");
        assert_eq!(blocks[1].start_line, 5);
        assert_eq!(split_hand_blocks("\n\n  \n").count(), 0);
        assert_eq!(
            first_content_line("\u{feff}\n  \n  Header  \nx"),
            Some("Header")
        );
    }
}
