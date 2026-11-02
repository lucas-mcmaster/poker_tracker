//! Canonical, site-independent representation of a poker hand history.
//!
//! Every site parser produces this model, and storage and statistics consume only this model.
//! Site-specific vocabulary stays inside the site's parser; when a site has a concept the model
//! cannot express, extend the model with a site-neutral name (see `PotDeduction` and
//! `PlayerAdjustment`) rather than adding a site-specific field.

use std::fmt;
use std::str::FromStr;

use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::card::Card;
use crate::domain::money::CurrencyCode;

/// Maximum number of board runouts supported (run it once, twice or three times).
pub const MAX_RUNOUTS: usize = 3;

/// A stored identifier (an `as_str()` value or street code) that this build does not know.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("unknown {kind} identifier '{value}'")]
pub struct UnknownIdentifier {
    pub kind: &'static str,
    pub value: String,
}

/// Implements `ALL` and `FromStr` (the inverse of `as_str()`) for a stored identifier enum.
/// The variant list must be complete: a missing variant fails to compile.
macro_rules! stored_identifier {
    ($ty:ident, $kind:literal, [$($variant:ident),+ $(,)?]) => {
        impl $ty {
            /// Every variant, in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
        }

        const _: () = {
            // Compile-time completeness check for `ALL`.
            #[allow(dead_code)]
            const fn listed(v: $ty) {
                match v {
                    $($ty::$variant)|+ => {}
                }
            }
        };

        impl FromStr for $ty {
            type Err = UnknownIdentifier;

            /// Parses a stored `as_str()` value.
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::ALL
                    .iter()
                    .copied()
                    .find(|v| v.as_str() == s)
                    .ok_or_else(|| UnknownIdentifier {
                        kind: $kind,
                        value: s.to_string(),
                    })
            }
        }
    };
}

/// Poker network that produced a hand history. Add a variant together with its parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    GGPoker,
}

impl Platform {
    /// Stable identifier stored in SQLite. Never change an existing value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GGPoker => "GGPoker",
        }
    }
}

stored_identifier!(Platform, "platform", [GGPoker]);

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// High-level game format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameType {
    Cash,
}

impl GameType {
    /// Stable identifier stored in SQLite.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cash => "Cash",
        }
    }
}

stored_identifier!(GameType, "game type", [Cash]);

/// Betting structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BettingLimit {
    NoLimit,
}

impl BettingLimit {
    /// Stable identifier stored in SQLite.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoLimit => "NL",
        }
    }
}

stored_identifier!(BettingLimit, "betting limit", [NoLimit]);

/// Betting street inside a hand. `Showdown` is never used for player actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Street {
    Preflop = 0,
    Flop = 1,
    Turn = 2,
    River = 3,
    Showdown = 4,
}

impl Street {
    /// Integer code stored in SQLite (`0..=4`).
    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// The street with the given [`Street::code`], if any.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Preflop),
            1 => Some(Self::Flop),
            2 => Some(Self::Turn),
            3 => Some(Self::River),
            4 => Some(Self::Showdown),
            _ => None,
        }
    }
}

/// Player action or pot adjustment event on a street.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionType {
    /// Live small blind (counts toward the poster's street commitment).
    PostSmallBlind,
    /// Live big blind, including a big blind posted by a player entering out of position.
    PostBigBlind,
    /// Dead money (e.g. a missed small blind): enters the pot but does not count toward the
    /// poster's street commitment.
    PostDeadBlind,
    Fold,
    Check,
    Call,
    Bet,
    Raise,
    UncalledBetReturned,
}

impl ActionType {
    /// Stable identifier stored in SQLite.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PostSmallBlind => "PostSmallBlind",
            Self::PostBigBlind => "PostBigBlind",
            Self::PostDeadBlind => "PostDeadBlind",
            Self::Fold => "Fold",
            Self::Check => "Check",
            Self::Call => "Call",
            Self::Bet => "Bet",
            Self::Raise => "Raise",
            Self::UncalledBetReturned => "UncalledBetReturned",
        }
    }

    /// `true` if this action moves chips from the player's stack into the pot.
    #[must_use]
    pub const fn is_contribution(self) -> bool {
        matches!(
            self,
            Self::PostSmallBlind
                | Self::PostBigBlind
                | Self::PostDeadBlind
                | Self::Call
                | Self::Bet
                | Self::Raise
        )
    }

    /// `true` if this action's amount is added to the player's street commitment.
    #[must_use]
    pub const fn adds_to_street_commitment(self) -> bool {
        matches!(
            self,
            Self::PostSmallBlind | Self::PostBigBlind | Self::Call | Self::Bet | Self::Raise
        )
    }
}

stored_identifier!(
    ActionType,
    "action type",
    [
        PostSmallBlind,
        PostBigBlind,
        PostDeadBlind,
        Fold,
        Check,
        Call,
        Bet,
        Raise,
        UncalledBetReturned,
    ]
);

/// Metadata header for a single hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandHeader {
    pub platform: Platform,
    /// Hand identifier as printed by the site; unique per platform.
    pub site_hand_id: String,
    pub game_type: GameType,
    pub limit: BettingLimit,
    /// Currency of every amount in this hand.
    pub currency: CurrencyCode,
    pub small_blind: Decimal,
    pub big_blind: Decimal,
    /// Local timestamp exactly as printed by the site.
    pub timestamp: NaiveDateTime,
    /// UTC offset of `timestamp` in seconds, when the site states it (or the import is
    /// configured with it). `None` means unknown, so cross-site ordering is approximate.
    pub utc_offset_seconds: Option<i32>,
    pub table_name: String,
    pub max_seats: u8,
    pub button_seat: u8,
}

/// Seated player state within a hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub seat: u8,
    /// Screen name as printed by the site (may be anonymized, e.g. GGPoker hashes).
    pub name: String,
    pub starting_stack: Decimal,
    /// Known hole cards. Slots fill in order, so `[None, Some(_)]` never occurs. A single known
    /// card comes from a one-card show.
    pub hole_cards: [Option<Card>; 2],
    /// The account owner whose hand history this is (the player dealt known cards).
    pub is_hero: bool,
}

impl Player {
    /// Iterates over the hole cards known for this player.
    pub fn known_cards(&self) -> impl Iterator<Item = Card> + use<'_> {
        self.hole_cards.iter().flatten().copied()
    }

    /// Records a known hole card. Returns `false` if it conflicts with two cards already known.
    pub fn record_known_card(&mut self, card: Card) -> bool {
        match self.hole_cards {
            [Some(a), _] if a == card => true,
            [_, Some(b)] if b == card => true,
            [None, _] => {
                self.hole_cards = [Some(card), None];
                true
            }
            [Some(a), None] => {
                self.hole_cards = [Some(a), Some(card)];
                true
            }
            [Some(_), Some(_)] => false,
        }
    }
}

/// Sequential action recorded during a hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreetAction {
    pub sequence: u16,
    pub street: Street,
    pub player_name: String,
    pub action_type: ActionType,
    /// Incremental chips committed (or returned on `UncalledBetReturned`) in this specific action.
    pub amount: Decimal,
    /// Player's cumulative live chips committed on the current street after this action completes.
    pub street_total_after: Decimal,
    pub is_all_in: bool,
}

/// Community cards for a single board runout (`run_index = 0` for a single board,
/// `1..=MAX_RUNOUTS` for multi-run boards).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityCards {
    pub run_index: u8,
    pub flop: Option<[Card; 3]>,
    pub turn: Option<Card>,
    pub river: Option<Card>,
}

impl CommunityCards {
    /// All cards on this runout in deal order (flop, turn, river).
    pub fn cards(&self) -> impl Iterator<Item = Card> + use<'_> {
        self.flop
            .iter()
            .flatten()
            .copied()
            .chain(self.turn)
            .chain(self.river)
    }
}

/// Payout collected by a player. `run_index` is `0` for single-board hands and identifies the
/// runout for multi-run hands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PotWinner {
    pub player_name: String,
    pub amount: Decimal,
    pub run_index: u8,
}

/// Category of money the house takes out of the pot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeductionKind {
    /// Rake (the basis for rakeback and rake-paid statistics).
    Rake,
    /// Contribution to a jackpot.
    Jackpot,
    /// Any other site fee or promotion drop (keeps the site's label).
    Other,
}

impl DeductionKind {
    /// Stable identifier stored in SQLite.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rake => "Rake",
            Self::Jackpot => "Jackpot",
            Self::Other => "Other",
        }
    }
}

stored_identifier!(DeductionKind, "deduction kind", [Rake, Jackpot, Other]);

/// One amount the house took out of the pot, with the label the site printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PotDeduction {
    pub kind: DeductionKind,
    /// Site label, e.g. `"Rake"`, `"Jackpot"`, `"Bingo"`.
    pub label: String,
    pub amount: Decimal,
}

/// Category of money moving between a player and the site outside the pot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AdjustmentKind {
    /// EV Cashout taken, pot won afterwards: the player pays back the cashout risk
    /// (GGPoker `Pays Cashout Risk ($x)`). Always a debit.
    EvCashoutRiskPaid,
}

impl AdjustmentKind {
    /// Stable identifier stored in SQLite.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EvCashoutRiskPaid => "EvCashoutRiskPaid",
        }
    }

    /// `Some(true)` if amounts of this kind are always credits, `Some(false)` if always
    /// debits, `None` if either sign is possible.
    #[must_use]
    pub const fn is_credit(self) -> Option<bool> {
        match self {
            Self::EvCashoutRiskPaid => Some(false),
        }
    }
}

stored_identifier!(AdjustmentKind, "adjustment kind", [EvCashoutRiskPaid]);

/// Money credited to (positive) or debited from (negative) a player outside the pot.
/// Adjustments do not affect pot accounting but do affect the player's net result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerAdjustment {
    pub player_name: String,
    pub kind: AdjustmentKind,
    /// Street on which the adjustment was triggered (e.g. when an EV Cashout was chosen).
    pub street: Street,
    /// Signed amount: positive credits the player, negative debits them.
    pub amount: Decimal,
}

/// Financial summary and pot distribution at the conclusion of a hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandOutcome {
    pub total_pot: Decimal,
    /// Everything the house took out of the pot, in the order the site printed it.
    pub deductions: Vec<PotDeduction>,
    pub uncalled_returned: Decimal,
    pub winners: Vec<PotWinner>,
    pub adjustments: Vec<PlayerAdjustment>,
}

impl HandOutcome {
    /// Checked sum of all deductions, returning `None` on overflow.
    #[must_use]
    pub fn checked_total_deductions(&self) -> Option<Decimal> {
        self.deductions
            .iter()
            .try_fold(Decimal::ZERO, |acc, d| acc.checked_add(d.amount))
    }

    /// Checked sum of the deductions of one kind, returning `None` on overflow.
    #[must_use]
    pub fn checked_deductions_of(&self, kind: DeductionKind) -> Option<Decimal> {
        self.deductions
            .iter()
            .filter(|d| d.kind == kind)
            .try_fold(Decimal::ZERO, |acc, d| acc.checked_add(d.amount))
    }

    /// Checked sum of all payouts collected by winners, returning `None` on overflow.
    #[must_use]
    pub fn checked_total_collected(&self) -> Option<Decimal> {
        self.winners
            .iter()
            .try_fold(Decimal::ZERO, |acc, w| acc.checked_add(w.amount))
    }

    /// Checked signed sum of all player adjustments, returning `None` on overflow.
    #[must_use]
    pub fn checked_total_adjustments(&self) -> Option<Decimal> {
        self.adjustments
            .iter()
            .try_fold(Decimal::ZERO, |acc, a| acc.checked_add(a.amount))
    }
}

/// Per-player financial result of a hand, in the hand's currency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerResult {
    pub player_name: String,
    /// Chips moved into the pot before uncalled bets were returned.
    pub gross_committed: Decimal,
    /// Chips that stayed in the pot (`gross_committed - returned uncalled bets`).
    pub invested: Decimal,
    /// Sum of all pot payouts to this player.
    pub won: Decimal,
    /// Signed sum of this player's [`PlayerAdjustment`]s.
    pub adjustments: Decimal,
    /// `won - invested + adjustments`.
    pub net: Decimal,
}

/// Complete, validated canonical representation of a single poker hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalHand {
    pub header: HandHeader,
    pub players: Vec<Player>,
    pub actions: Vec<StreetAction>,
    pub boards: Vec<CommunityCards>,
    pub outcome: HandOutcome,
}

impl CanonicalHand {
    /// Number of runouts: `0` if no board was dealt, `1` for a single board, `2..` for multi-run.
    #[must_use]
    pub fn run_count(&self) -> usize {
        if self.boards.iter().any(|b| b.run_index > 0) {
            self.boards.len()
        } else {
            usize::from(!self.boards.is_empty())
        }
    }

    /// Checked sum of all chips that stayed in the pot (`contributions - uncalled returns`).
    #[must_use]
    pub fn checked_total_contributed(&self) -> Option<Decimal> {
        self.actions
            .iter()
            .try_fold(Decimal::ZERO, |acc, a| match a.action_type {
                ActionType::UncalledBetReturned => acc.checked_sub(a.amount),
                t if t.is_contribution() => acc.checked_add(a.amount),
                _ => Some(acc),
            })
    }

    /// Checked per-player results in seat order, returning `None` on arithmetic overflow.
    #[must_use]
    pub fn checked_player_results(&self) -> Option<Vec<PlayerResult>> {
        self.players
            .iter()
            .map(|p| {
                let mut gross = Decimal::ZERO;
                let mut returned = Decimal::ZERO;
                for a in self.actions.iter().filter(|a| a.player_name == p.name) {
                    if a.action_type == ActionType::UncalledBetReturned {
                        returned = returned.checked_add(a.amount)?;
                    } else if a.action_type.is_contribution() {
                        gross = gross.checked_add(a.amount)?;
                    }
                }
                let won = self
                    .outcome
                    .winners
                    .iter()
                    .filter(|w| w.player_name == p.name)
                    .try_fold(Decimal::ZERO, |acc, w| acc.checked_add(w.amount))?;
                let adjustments = self
                    .outcome
                    .adjustments
                    .iter()
                    .filter(|a| a.player_name == p.name)
                    .try_fold(Decimal::ZERO, |acc, a| acc.checked_add(a.amount))?;
                let invested = gross.checked_sub(returned)?;
                let net = won.checked_sub(invested)?.checked_add(adjustments)?;
                Some(PlayerResult {
                    player_name: p.name.clone(),
                    gross_committed: gross,
                    invested,
                    won,
                    adjustments,
                    net,
                })
            })
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use std::str::FromStr;

    #[test]
    fn record_known_card_fills_slots_in_order_and_detects_conflicts() {
        let c = |s: &str| Card::from_str(s).unwrap();
        let mut p = Player {
            seat: 1,
            name: "p".into(),
            starting_stack: dec!(10),
            hole_cards: [None, None],
            is_hero: false,
        };
        assert!(p.record_known_card(c("Jc")));
        assert_eq!(p.hole_cards, [Some(c("Jc")), None]);
        assert!(
            p.record_known_card(c("Jc")),
            "re-showing a known card is fine"
        );
        assert!(p.record_known_card(c("Kd")));
        assert_eq!(p.hole_cards, [Some(c("Jc")), Some(c("Kd"))]);
        assert!(p.record_known_card(c("Kd")));
        assert!(
            !p.record_known_card(c("2s")),
            "a third distinct card conflicts"
        );
    }

    #[test]
    fn action_type_contribution_semantics() {
        assert!(ActionType::PostDeadBlind.is_contribution());
        assert!(!ActionType::PostDeadBlind.adds_to_street_commitment());
        assert!(ActionType::Raise.adds_to_street_commitment());
        assert!(!ActionType::UncalledBetReturned.is_contribution());
        assert!(!ActionType::Check.is_contribution());
    }

    #[test]
    fn stored_identifiers_round_trip_and_unknown_ones_are_rejected() {
        fn round_trip<T>(all: &[T], as_str: fn(T) -> &'static str) -> usize
        where
            T: Copy + PartialEq + fmt::Debug + FromStr<Err = UnknownIdentifier>,
        {
            for v in all {
                assert_eq!(as_str(*v).parse::<T>(), Ok(*v));
            }
            all.len()
        }
        assert_eq!(round_trip(Platform::ALL, Platform::as_str), 1);
        assert_eq!(round_trip(GameType::ALL, GameType::as_str), 1);
        assert_eq!(round_trip(BettingLimit::ALL, BettingLimit::as_str), 1);
        assert_eq!(round_trip(ActionType::ALL, ActionType::as_str), 9);
        assert_eq!(round_trip(DeductionKind::ALL, DeductionKind::as_str), 3);
        assert_eq!(round_trip(AdjustmentKind::ALL, AdjustmentKind::as_str), 1);
        for code in 0..=4 {
            assert_eq!(Street::from_code(code).map(Street::code), Some(code));
        }
        assert_eq!(Street::from_code(5), None);
        assert_eq!(
            "Shove".parse::<ActionType>(),
            Err(UnknownIdentifier {
                kind: "action type",
                value: "Shove".into()
            })
        );
        assert!(
            "ggpoker".parse::<Platform>().is_err(),
            "identifiers are case-sensitive"
        );
    }

    #[test]
    fn deduction_and_adjustment_totals() {
        let outcome = HandOutcome {
            total_pot: dec!(3.05),
            deductions: vec![
                PotDeduction {
                    kind: DeductionKind::Rake,
                    label: "Rake".into(),
                    amount: dec!(0.14),
                },
                PotDeduction {
                    kind: DeductionKind::Jackpot,
                    label: "Jackpot".into(),
                    amount: dec!(0.1),
                },
                PotDeduction {
                    kind: DeductionKind::Other,
                    label: "Bingo".into(),
                    amount: dec!(0),
                },
            ],
            uncalled_returned: dec!(0),
            winners: vec![],
            adjustments: vec![PlayerAdjustment {
                player_name: "p".into(),
                kind: AdjustmentKind::EvCashoutRiskPaid,
                street: Street::Turn,
                amount: dec!(-0.48),
            }],
        };
        assert_eq!(outcome.checked_total_deductions(), Some(dec!(0.24)));
        assert_eq!(
            outcome.checked_deductions_of(DeductionKind::Rake),
            Some(dec!(0.14))
        );
        assert_eq!(outcome.checked_total_adjustments(), Some(dec!(-0.48)));
        assert_eq!(AdjustmentKind::EvCashoutRiskPaid.is_credit(), Some(false));
    }
}
