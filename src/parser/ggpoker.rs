//! State-machine parser for GGPoker (PokerCraft) No-Limit Hold'em cash hand histories.
//!
//! This module is the only place that knows the GGPoker text format. It maps GGPoker
//! vocabulary onto the site-neutral canonical model (e.g. the summary's Rake/Jackpot/Bingo/
//! Fortune/Tax fields become [`PotDeduction`]s; EV Cashout becomes a [`PlayerAdjustment`]).
//!
//! Format coverage policy: every line pattern below was observed in real GGPoker exports.
//! Anything else is rejected with [`ParseError::UnrecognizedLine`] (or a more specific error)
//! so that new formats fail loudly instead of being silently mis-parsed. To support a new
//! line, add the pattern together with a real sample hand and a test.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::LazyLock;

use chrono::NaiveDateTime;
use regex::Regex;
use rust_decimal::Decimal;

use crate::domain::canonical::{
    ActionType, AdjustmentKind, BettingLimit, CanonicalHand, CommunityCards, DeductionKind,
    GameType, HandHeader, HandOutcome, Platform, Player, PlayerAdjustment, PotDeduction, PotWinner,
    Street, StreetAction,
};
use crate::domain::card::Card;
use crate::domain::money::CurrencyCode;
use crate::parser::base::{HandParser, ParseError, first_content_line, validate_hand};

/// Compiles a static regex pattern at startup.
#[allow(clippy::expect_used)]
fn compile_static_regex(pattern: &'static str) -> Regex {
    Regex::new(pattern).expect("static regex pattern must be valid")
}

macro_rules! amt {
    () => {
        r"[0-9]+(?:\.[0-9]+)?"
    };
}

static RE_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(
        r"^Poker Hand #(?P<id>[A-Za-z0-9]+): (?P<game>.+?) \((?P<stakes>[^)]*)\) - (?P<ts>\d{4}/\d{2}/\d{2} \d{2}:\d{2}:\d{2})$",
    )
});

static RE_STAKES: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^\$(?P<sb>",
        amt!(),
        r")/\$(?P<bb>",
        amt!(),
        r")$"
    ))
});

static RE_TABLE: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(
        r"^Table '(?P<table>[^']+)' (?P<max>\d+)-max Seat #(?P<btn>\d+) is the button$",
    )
});

static RE_SEAT: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^Seat (?P<seat>\d+): (?P<name>.+?) \(\$(?P<stack>",
        amt!(),
        r") in chips\)$"
    ))
});

static RE_DEALT: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(r"^Dealt to (?P<name>.+?)(?: \[(?P<c1>\S+) (?P<c2>\S+)\])?$")
});

static RE_STREET: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(
        r"^\*\*\* (?:(?P<run>FIRST|SECOND|THIRD) )?(?P<street>HOLE CARDS|FLOP|TURN|RIVER|SHOWDOWN|SUMMARY) \*\*\*(?: \[(?P<b1>[^\]]+)\])?(?: \[(?P<b2>[^\]]+)\])?$",
    )
});

// One pattern per verb: an amount (or `to`, or `and is all-in`) is accepted only where GGPoker
// prints one, so stray text can never be silently dropped.
static RE_POST: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^(?P<name>.+?): posts (?P<kind>small blind|big blind|missed blind) \$(?P<amt>",
        amt!(),
        r")$"
    ))
});

static RE_STRADDLE: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^(?P<name>.+?): straddle \$(?P<amt>",
        amt!(),
        r")$"
    ))
});

static RE_FOLD_CHECK: LazyLock<Regex> =
    LazyLock::new(|| compile_static_regex(r"^(?P<name>.+?): (?P<verb>folds|checks)$"));

static RE_CALL_BET: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^(?P<name>.+?): (?P<verb>calls|bets) \$(?P<amt>",
        amt!(),
        r")(?P<allin> and is all-in)?$"
    ))
});

static RE_RAISE: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^(?P<name>.+?): raises \$(?P<by>",
        amt!(),
        r") to \$(?P<to>",
        amt!(),
        r")(?P<allin> and is all-in)?$"
    ))
});

static RE_SHOWS: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(r"^(?P<name>.+?): shows \[(?P<cards>[^\]]+)\](?: \(.*\))?$")
});

static RE_UNCALLED: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^Uncalled bet \(\$(?P<amt>",
        amt!(),
        r")\) returned to (?P<name>.+)$"
    ))
});

static RE_COLLECTED: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^(?P<name>.+?) collected \$(?P<amt>",
        amt!(),
        r") from pot$"
    ))
});

static RE_EV_CHOOSE: LazyLock<Regex> =
    LazyLock::new(|| compile_static_regex(r"^(?P<name>.+?): Chooses to EV Cashout$"));

static RE_EV_PAYS: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^(?P<name>.+?): Pays Cashout Risk \(\$(?P<amt>",
        amt!(),
        r")\)$"
    ))
});

static RE_EV_RECEIVES: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^(?P<name>.+?): Receives Cashout \(\$(?P<amt>",
        amt!(),
        r")\)$"
    ))
});

static RE_TOTAL_POT: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^Total pot \$(?P<pot>",
        amt!(),
        r") \| Rake \$(?P<rake>",
        amt!(),
        r") \| Jackpot \$(?P<jackpot>",
        amt!(),
        r") \| Bingo \$(?P<bingo>",
        amt!(),
        r") \| Fortune \$(?P<fortune>",
        amt!(),
        r") \| Tax \$(?P<tax>",
        amt!(),
        r")$"
    ))
});

static RE_SUMMARY_BOARD: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(r"^(?:(?P<run>FIRST|SECOND|THIRD) )?Board \[(?P<cards>[^\]]+)\]$")
});

static RE_RUN_COUNT: LazyLock<Regex> =
    LazyLock::new(|| compile_static_regex(r"^Hand was run (?P<n>two|three) times$"));

static RE_SUMMARY_SEAT: LazyLock<Regex> =
    LazyLock::new(|| compile_static_regex(r"^Seat (?P<seat>\d+): (?P<rest>.+)$"));

// Summary seat-line grammar (after `Seat <n>: <name>` and any position labels):
// ` folded before Flop [(didn't bet)]`, ` folded on the Flop|Turn|River`,
// ` won ($x)`, ` collected ($x)`, or
// ` showed [c1 c2] and <outcome>[, and <outcome>]…[, Cashout Risk ($x)]` with one outcome per
// runout, each `won ($x)` or `lost`, optionally followed by `with <hand description>` (words and
// at most one comma, never digits, so no amount can hide in it).
static RE_SEAT_LABEL: LazyLock<Regex> =
    LazyLock::new(|| compile_static_regex(r"^ \((?P<label>button|small blind|big blind)\)"));

static RE_SEAT_FOLDED: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(
        r"^ folded (?P<when>before Flop|on the Flop|on the Turn|on the River)(?P<nobet> \(didn't bet\))?$",
    )
});

static RE_SEAT_WON: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^ (?P<verb>won|collected) \(\$(?P<amt>",
        amt!(),
        r")\)$"
    ))
});

static RE_SEAT_SHOWED_UNCONTESTED: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^ showed \[(?P<cards>[^\]]+)\] and (?P<verb>won|collected) \(\$(?P<amt>",
        amt!(),
        r")\)$"
    ))
});

static RE_SEAT_SHOWED: LazyLock<Regex> =
    LazyLock::new(|| compile_static_regex(r"^ showed \[(?P<cards>[^\]]+)\] and (?P<outcomes>.+)$"));

static RE_SEAT_CASHOUT_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^(?P<body>.+), Cashout Risk \(\$(?P<amt>",
        amt!(),
        r")\)$"
    ))
});

static RE_SEAT_EV_CASHOUT_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^(?P<body>.+), EV Cashout \(\$(?P<amt>",
        amt!(),
        r")\)$"
    ))
});

static RE_SEAT_RUN_OUTCOME: LazyLock<Regex> = LazyLock::new(|| {
    compile_static_regex(concat!(
        r"^(?:won \(\$(?P<amt>",
        amt!(),
        r")\)|lost)(?: with [A-Za-z]+(?: [A-Za-z]+)*(?:, [A-Za-z]+(?: [A-Za-z]+)*)?)?$"
    ))
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParserPhase {
    SeatingAndBlinds,
    Play(Street),
    Showdown,
    Summary,
}

impl ParserPhase {
    const fn as_str(self) -> &'static str {
        match self {
            Self::SeatingAndBlinds => "SEATING_AND_BLINDS",
            Self::Play(Street::Preflop) => "PREFLOP",
            Self::Play(Street::Flop) => "FLOP",
            Self::Play(Street::Turn) => "TURN",
            Self::Play(Street::River) => "RIVER",
            Self::Play(Street::Showdown) | Self::Showdown => "SHOWDOWN",
            Self::Summary => "SUMMARY",
        }
    }
}

/// Parser for GGPoker No-Limit Hold'em cash hand histories.
///
/// GGPoker prints `$` whatever the account currency (observed: a CAD account), so the
/// currency cannot be read from the file and must be supplied for each source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgPokerParser {
    currency: CurrencyCode,
}

impl GgPokerParser {
    /// Creates a parser that records every amount in `currency` (the account currency).
    #[must_use]
    pub const fn new(currency: CurrencyCode) -> Self {
        Self { currency }
    }

    /// The currency recorded on every parsed hand.
    #[must_use]
    pub const fn currency(&self) -> CurrencyCode {
        self.currency
    }
}

fn cap<'h>(caps: &regex::Captures<'h>, name: &str) -> &'h str {
    caps.name(name).map_or("", |m| m.as_str())
}

fn parse_decimal(raw: &str) -> Result<Decimal, ParseError> {
    Decimal::from_str(raw).map_err(|_| ParseError::InvalidAmount {
        raw: raw.to_string(),
    })
}

fn parse_card_token(token: &str) -> Result<Card, ParseError> {
    Card::from_str(token).map_err(|_| ParseError::InvalidCardSyntax {
        token: token.to_string(),
    })
}

fn parse_card_list(bracket_content: &str) -> Result<Vec<Card>, ParseError> {
    bracket_content
        .split_whitespace()
        .map(parse_card_token)
        .collect()
}

fn parse_three_cards(bracket_content: &str) -> Result<[Card; 3], ParseError> {
    match parse_card_list(bracket_content)?.as_slice() {
        [a, b, c] => Ok([*a, *b, *c]),
        _ => Err(ParseError::InvalidCardSyntax {
            token: bracket_content.to_string(),
        }),
    }
}

fn parse_single_card(bracket_content: &str) -> Result<Card, ParseError> {
    match parse_card_list(bracket_content)?.as_slice() {
        [a] => Ok(*a),
        _ => Err(ParseError::InvalidCardSyntax {
            token: bracket_content.to_string(),
        }),
    }
}

fn run_index_from_word(word: Option<&str>) -> u8 {
    match word {
        Some("FIRST") => 1,
        Some("SECOND") => 2,
        Some("THIRD") => 3,
        _ => 0,
    }
}

fn get_or_create_board(boards: &mut Vec<CommunityCards>, run_index: u8) -> &mut CommunityCards {
    if let Some(pos) = boards.iter().position(|b| b.run_index == run_index) {
        &mut boards[pos]
    } else {
        boards.push(CommunityCards {
            run_index,
            flop: None,
            turn: None,
            river: None,
        });
        let last_idx = boards.len() - 1;
        &mut boards[last_idx]
    }
}

/// For runouts after the first (`run_index > 1`) that start on the turn or river, copies the
/// shared flop (and turn) from the first runout.
fn prepare_board_for_street(boards: &mut Vec<CommunityCards>, run_index: u8, street: Street) {
    if run_index <= 1 || boards.iter().any(|b| b.run_index == run_index) {
        return;
    }
    let shared = boards
        .iter()
        .find(|b| b.run_index == 1)
        .map(|b1| (b1.flop, b1.turn));
    if let Some((flop, turn)) = shared {
        match street {
            Street::Turn if flop.is_some() => boards.push(CommunityCards {
                run_index,
                flop,
                turn: None,
                river: None,
            }),
            Street::River if flop.is_some() && turn.is_some() => boards.push(CommunityCards {
                run_index,
                flop,
                turn,
                river: None,
            }),
            _ => {}
        }
    }
}

/// Facts read from the `*** SUMMARY ***` section, cross-checked against the hand body.
#[derive(Debug, Default)]
struct SummaryFacts {
    saw_total_pot: bool,
    /// `(run_index, cards)` for each `Board [..]` / `FIRST Board [..]` line.
    board_lines: Vec<(u8, Vec<Card>)>,
    stated_run_count: Option<usize>,
    /// Seat number -> sum of `won ($x)` / `collected ($x)` amounts.
    seat_won: HashMap<u8, Decimal>,
    /// Seat number -> `Cashout Risk ($x)`.
    seat_cashout_risk: HashMap<u8, Decimal>,
    /// Seat number -> `EV Cashout ($x)`.
    seat_cashout_received: HashMap<u8, Decimal>,
    seat_lines: Vec<u8>,
    /// Seat number -> position labels and result printed on its summary line.
    seat_summaries: HashMap<u8, SeatSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PayoutVerb {
    Won,
    Collected,
}

/// How a player's hand ended, as printed on their summary seat line.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SeatResult {
    /// `folded before Flop` / `folded on the <street>`, with or without `(didn't bet)`.
    Folded { street: Street, didnt_bet: bool },
    /// `won ($x)` / `collected ($x)` without a showdown.
    Won { verb: PayoutVerb },
    /// `showed [c1 c2] and won|lost …`, with one outcome per runout.
    Showed {
        cards: Vec<Card>,
        outcomes: usize,
        all_lost: bool,
    },
    /// `showed [c1]` / `showed [c1 c2] and won|collected ($x)` without a showdown.
    ShowedUncontested { cards: Vec<Card>, verb: PayoutVerb },
}

/// Labels and result read from one summary seat line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SeatSummary {
    button: bool,
    small_blind: bool,
    big_blind: bool,
    result: SeatResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CashoutSettlement {
    RiskPaid(Decimal),
    Received(Decimal),
}

/// EV cashout chosen but not yet settled.
#[derive(Debug)]
struct PendingCashout {
    player_name: String,
    chosen_on: Street,
    settlement: Option<CashoutSettlement>,
}

/// Mutable state accumulated while walking one hand block.
#[derive(Debug, Default)]
struct HandState {
    players: Vec<Player>,
    actions: Vec<StreetAction>,
    boards: Vec<CommunityCards>,
    winners: Vec<PotWinner>,
    cashouts: Vec<PendingCashout>,
    total_pot: Decimal,
    deductions: Vec<PotDeduction>,
    uncalled_returned: Decimal,
    sequence: u16,
    street_commitments: HashMap<String, Decimal>,
    /// Run whose showdown is currently being reported (`0` = single board).
    showdown_run: u8,
    /// Seats named on `Dealt to` lines, in the order printed.
    dealt_seats: Vec<u8>,
    /// Set by the first preflop line after the `Dealt to` lines.
    dealing_done: bool,
    /// `(small blind, big blind)` from the header.
    stakes: (Decimal, Decimal),
    summary: SummaryFacts,
}

impl HandState {
    fn ensure_seated(&self, name: &str) -> Result<(), ParseError> {
        if self.players.iter().any(|p| p.name == name) {
            Ok(())
        } else {
            Err(ParseError::UnknownPlayer {
                player_name: name.to_string(),
            })
        }
    }

    fn player_mut(&mut self, name: &str) -> Result<&mut Player, ParseError> {
        self.players
            .iter_mut()
            .find(|p| p.name == name)
            .ok_or_else(|| ParseError::UnknownPlayer {
                player_name: name.to_string(),
            })
    }

    fn push_action(
        &mut self,
        street: Street,
        name: &str,
        action_type: ActionType,
        amount: Decimal,
        street_total_after: Decimal,
        is_all_in: bool,
    ) -> Result<(), ParseError> {
        self.actions.push(StreetAction {
            sequence: self.sequence,
            street,
            player_name: name.to_string(),
            action_type,
            amount,
            street_total_after,
            is_all_in,
        });
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| ParseError::InvalidAmount {
                raw: "action sequence overflow".to_string(),
            })?;
        Ok(())
    }

    fn record_shown_cards(&mut self, name: &str, cards_raw: &str) -> Result<(), ParseError> {
        let cards = parse_card_list(cards_raw)?;
        if cards.is_empty() || cards.len() > 2 {
            return Err(ParseError::InvalidCardSyntax {
                token: cards_raw.to_string(),
            });
        }
        let player = self.player_mut(name)?;
        for card in cards {
            if !player.record_known_card(card) {
                return Err(ParseError::ConflictingHoleCards {
                    player_name: name.to_string(),
                });
            }
        }
        Ok(())
    }

    /// Handles a line between the seat list and `*** HOLE CARDS ***`: blind posts and straddles.
    fn apply_blind_line(&mut self, line_idx: usize, line: &str) -> Result<(), ParseError> {
        let unrecognized = || ParseError::UnrecognizedLine {
            line: line_idx,
            content: line.to_string(),
        };

        if let Some(caps) = RE_POST.captures(line) {
            // Reject any blind post that appears after a straddle
            if self
                .actions
                .iter()
                .any(|a| a.action_type == ActionType::PostStraddle)
            {
                return Err(unrecognized());
            }
            let name = cap(&caps, "name");
            self.ensure_seated(name)?;
            let amount = parse_decimal(cap(&caps, "amt"))?;
            if amount <= Decimal::ZERO {
                return Err(ParseError::IllegalAction {
                    line: line_idx,
                    content: line.to_string(),
                    reason: "amount must be positive".to_string(),
                });
            }
            // Never observed (spec s9): a missed blind in a heads-up hand, and a live blind
            // for less than the stake (an all-in blind; GGPoker's format for it is unknown).
            let stake = match cap(&caps, "kind") {
                "small blind" => Some(self.stakes.0),
                "big blind" => Some(self.stakes.1),
                _ => None,
            };
            if (stake.is_none() && self.players.len() == 2) || stake.is_some_and(|s| amount < s) {
                return Err(unrecognized());
            }
            let current = self
                .street_commitments
                .get(name)
                .copied()
                .unwrap_or(Decimal::ZERO);
            // GGPoker prints the small blind first, then the big blind(s) and any missed blinds.
            if cap(&caps, "kind") == "small blind" && !self.actions.is_empty() {
                return Err(unrecognized());
            }
            let (action_type, total_after) = match cap(&caps, "kind") {
                "small blind" => (ActionType::PostSmallBlind, current.checked_add(amount)),
                "big blind" => (ActionType::PostBigBlind, current.checked_add(amount)),
                _ => (ActionType::PostDeadBlind, Some(current)),
            };
            let total_after = total_after.ok_or_else(|| ParseError::InvalidAmount {
                raw: line.to_string(),
            })?;
            // GGPoker prints no "and is all-in" on a blind; a post of the whole stack is one.
            let posted_before = self
                .actions
                .iter()
                .filter(|a| a.player_name == name)
                .try_fold(Decimal::ZERO, |acc, a| acc.checked_add(a.amount))
                .ok_or_else(|| ParseError::InvalidAmount {
                    raw: line.to_string(),
                })?;
            let is_all_in = self.players.iter().any(|p| {
                p.name == name && posted_before.checked_add(amount) == Some(p.starting_stack)
            });
            self.street_commitments
                .insert(name.to_string(), total_after);
            return self.push_action(
                Street::Preflop,
                name,
                action_type,
                amount,
                total_after,
                is_all_in,
            );
        }

        if let Some(caps) = RE_STRADDLE.captures(line) {
            // Must appear after at least one PostBigBlind action
            if !self
                .actions
                .iter()
                .any(|a| a.action_type == ActionType::PostBigBlind)
            {
                return Err(unrecognized());
            }
            // Reject if 2 PostStraddle actions have already been recorded in the hand
            let straddles_count = self
                .actions
                .iter()
                .filter(|a| a.action_type == ActionType::PostStraddle)
                .count();
            if straddles_count >= 2 {
                return Err(ParseError::IllegalAction {
                    line: line_idx,
                    content: line.to_string(),
                    reason: "at most two straddles allowed".to_string(),
                });
            }
            let name = cap(&caps, "name");
            self.ensure_seated(name)?;
            // Never observed: a heads-up straddle, or a player straddling twice.
            if self.players.len() == 2
                || self
                    .actions
                    .iter()
                    .any(|a| a.player_name == name && a.action_type == ActionType::PostStraddle)
            {
                return Err(ParseError::IllegalAction {
                    line: line_idx,
                    content: line.to_string(),
                    reason: "straddle shape never observed (heads-up, or a second straddle by \
                             the same player)"
                        .to_string(),
                });
            }
            let total_after = parse_decimal(cap(&caps, "amt"))?;
            let current = self
                .street_commitments
                .get(name)
                .copied()
                .unwrap_or(Decimal::ZERO);
            let increment =
                total_after
                    .checked_sub(current)
                    .ok_or_else(|| ParseError::InvalidAmount {
                        raw: line.to_string(),
                    })?;
            if increment <= Decimal::ZERO {
                return Err(ParseError::IllegalAction {
                    line: line_idx,
                    content: line.to_string(),
                    reason: "straddle increment must be positive".to_string(),
                });
            }
            let max_before = self
                .street_commitments
                .values()
                .copied()
                .max()
                .unwrap_or(Decimal::ZERO);
            let expected = max_before.checked_mul(Decimal::from(2)).ok_or_else(|| {
                ParseError::InvalidAmount {
                    raw: line.to_string(),
                }
            })?;
            if total_after != expected {
                return Err(ParseError::IllegalAction {
                    line: line_idx,
                    content: line.to_string(),
                    reason: format!(
                        "straddle must be 2x highest previous commitment {max_before}, expected {expected} but got {total_after}"
                    ),
                });
            }
            let posted_before = self
                .actions
                .iter()
                .filter(|a| a.player_name == name)
                .try_fold(Decimal::ZERO, |acc, a| acc.checked_add(a.amount))
                .ok_or_else(|| ParseError::InvalidAmount {
                    raw: line.to_string(),
                })?;
            let is_all_in = self.players.iter().any(|p| {
                p.name == name && posted_before.checked_add(increment) == Some(p.starting_stack)
            });
            // All-in straddles are out of scope until a real sample shows how GGPoker prints one.
            if is_all_in {
                return Err(ParseError::IllegalAction {
                    line: line_idx,
                    content: line.to_string(),
                    reason: "all-in straddle never observed".to_string(),
                });
            }
            self.street_commitments
                .insert(name.to_string(), total_after);
            return self.push_action(
                Street::Preflop,
                name,
                ActionType::PostStraddle,
                increment,
                total_after,
                is_all_in,
            );
        }

        Err(unrecognized())
    }

    /// Handles a line inside a betting street (after `*** HOLE CARDS ***`).
    fn apply_play_line(
        &mut self,
        line_idx: usize,
        line: &str,
        street: Street,
    ) -> Result<(), ParseError> {
        let illegal = |reason: &str| ParseError::IllegalAction {
            line: line_idx,
            content: line.to_string(),
            reason: reason.to_string(),
        };

        if let Some(caps) = RE_UNCALLED.captures(line) {
            let name = cap(&caps, "name");
            self.ensure_seated(name)?;
            let amount = parse_decimal(cap(&caps, "amt"))?;
            let current = self
                .street_commitments
                .get(name)
                .copied()
                .unwrap_or(Decimal::ZERO);
            let new_total = current
                .checked_sub(amount)
                .filter(|t| *t >= Decimal::ZERO && amount > Decimal::ZERO)
                .ok_or_else(|| illegal("returned more than was committed on this street"))?;
            self.street_commitments.insert(name.to_string(), new_total);
            self.uncalled_returned = self
                .uncalled_returned
                .checked_add(amount)
                .ok_or_else(|| illegal("overflow"))?;
            return self.push_action(
                street,
                name,
                ActionType::UncalledBetReturned,
                amount,
                new_total,
                false,
            );
        }

        if let Some(caps) = RE_SHOWS.captures(line) {
            let name = cap(&caps, "name");
            return self.record_shown_cards(name, cap(&caps, "cards"));
        }

        if let Some(caps) = RE_EV_CHOOSE.captures(line) {
            let name = cap(&caps, "name");
            self.ensure_seated(name)?;
            // A cashout is offered only while cards are still to come, and only once.
            if street == Street::River || self.cashouts.iter().any(|c| c.player_name == name) {
                return Err(ParseError::UnsupportedAdjustment {
                    line: line_idx,
                    content: line.to_string(),
                });
            }
            self.cashouts.push(PendingCashout {
                player_name: name.to_string(),
                chosen_on: street,
                settlement: None,
            });
            return Ok(());
        }

        if let Some(caps) = RE_EV_PAYS.captures(line) {
            let name = cap(&caps, "name");
            let amount = parse_decimal(cap(&caps, "amt"))?;
            if street != Street::River {
                return Err(ParseError::UnsupportedAdjustment {
                    line: line_idx,
                    content: line.to_string(),
                });
            }
            let pending = self
                .cashouts
                .iter_mut()
                .find(|c| c.player_name == name && c.settlement.is_none())
                .ok_or_else(|| ParseError::UnsupportedAdjustment {
                    line: line_idx,
                    content: line.to_string(),
                })?;
            pending.settlement = Some(CashoutSettlement::RiskPaid(amount));
            return Ok(());
        }

        if let Some(caps) = RE_EV_RECEIVES.captures(line) {
            let name = cap(&caps, "name");
            let amount = parse_decimal(cap(&caps, "amt"))?;
            if street != Street::River {
                return Err(ParseError::UnsupportedAdjustment {
                    line: line_idx,
                    content: line.to_string(),
                });
            }
            let pending = self
                .cashouts
                .iter_mut()
                .find(|c| c.player_name == name && c.settlement.is_none())
                .ok_or_else(|| ParseError::UnsupportedAdjustment {
                    line: line_idx,
                    content: line.to_string(),
                })?;
            pending.settlement = Some(CashoutSettlement::Received(amount));
            return Ok(());
        }

        if RE_POST.is_match(line) || RE_STRADDLE.is_match(line) {
            return Err(illegal("forced post after hole cards were dealt"));
        }

        let (name, action_type, amounts, is_all_in) = if let Some(caps) = RE_RAISE.captures(line) {
            let by = parse_decimal(cap(&caps, "by"))?;
            let to = parse_decimal(cap(&caps, "to"))?;
            (
                cap(&caps, "name"),
                ActionType::Raise,
                Some((by, to)),
                caps.name("allin").is_some(),
            )
        } else if let Some(caps) = RE_CALL_BET.captures(line) {
            let amount = parse_decimal(cap(&caps, "amt"))?;
            let t = if cap(&caps, "verb") == "calls" {
                ActionType::Call
            } else {
                ActionType::Bet
            };
            (
                cap(&caps, "name"),
                t,
                Some((amount, Decimal::ZERO)),
                caps.name("allin").is_some(),
            )
        } else if let Some(caps) = RE_FOLD_CHECK.captures(line) {
            let t = if cap(&caps, "verb") == "folds" {
                ActionType::Fold
            } else {
                ActionType::Check
            };
            (cap(&caps, "name"), t, None, false)
        } else if line.contains("Cashout") {
            return Err(ParseError::UnsupportedAdjustment {
                line: line_idx,
                content: line.to_string(),
            });
        } else {
            return Err(ParseError::UnrecognizedLine {
                line: line_idx,
                content: line.to_string(),
            });
        };
        self.ensure_seated(name)?;

        let current = self
            .street_commitments
            .get(name)
            .copied()
            .unwrap_or(Decimal::ZERO);
        let facing = self
            .street_commitments
            .values()
            .copied()
            .max()
            .unwrap_or(Decimal::ZERO);
        let (first, second) = amounts.unwrap_or((Decimal::ZERO, Decimal::ZERO));
        if amounts.is_some() && (first <= Decimal::ZERO || second < Decimal::ZERO) {
            return Err(illegal("amount must be positive"));
        }
        let add = |amt: Decimal| current.checked_add(amt).ok_or_else(|| illegal("overflow"));

        let (amount, total_after) = match action_type {
            ActionType::Fold => (Decimal::ZERO, current),
            ActionType::Check => {
                if current != facing {
                    return Err(illegal("check while facing a bet"));
                }
                (Decimal::ZERO, current)
            }
            ActionType::Call => {
                let next = add(first)?;
                if next > facing || (!is_all_in && next != facing) {
                    return Err(illegal("call amount does not match the bet faced"));
                }
                (first, next)
            }
            ActionType::Bet => {
                if facing > Decimal::ZERO {
                    return Err(illegal("bet while facing a bet (expected a raise)"));
                }
                (first, add(first)?)
            }
            _ => {
                let (raise_by, to_total) = (first, second);
                if to_total <= current
                    || to_total <= facing
                    || facing.checked_add(raise_by) != Some(to_total)
                {
                    return Err(illegal("raise arithmetic is inconsistent"));
                }
                let incremental = to_total
                    .checked_sub(current)
                    .ok_or_else(|| illegal("overflow"))?;
                (incremental, to_total)
            }
        };

        self.street_commitments
            .insert(name.to_string(), total_after);
        self.push_action(street, name, action_type, amount, total_after, is_all_in)
    }

    /// Handles a line after a `*** SHOWDOWN ***` header.
    fn apply_showdown_line(&mut self, line_idx: usize, line: &str) -> Result<(), ParseError> {
        if let Some(caps) = RE_COLLECTED.captures(line) {
            let name = cap(&caps, "name");
            self.ensure_seated(name)?;
            let amount = parse_decimal(cap(&caps, "amt"))?;
            self.winners.push(PotWinner {
                player_name: name.to_string(),
                amount,
                run_index: self.showdown_run,
            });
            return Ok(());
        }
        if let Some(caps) = RE_SHOWS.captures(line) {
            let name = cap(&caps, "name");
            return self.record_shown_cards(name, cap(&caps, "cards"));
        }
        Err(ParseError::UnrecognizedLine {
            line: line_idx,
            content: line.to_string(),
        })
    }

    /// Handles a line in the `*** SUMMARY ***` section.
    fn apply_summary_line(&mut self, line_idx: usize, line: &str) -> Result<(), ParseError> {
        let unrecognized = || ParseError::UnrecognizedLine {
            line: line_idx,
            content: line.to_string(),
        };

        if let Some(caps) = RE_TOTAL_POT.captures(line) {
            if self.summary.saw_total_pot {
                return Err(unrecognized());
            }
            self.summary.saw_total_pot = true;
            self.total_pot = parse_decimal(cap(&caps, "pot"))?;
            // Map GGPoker's summary labels onto site-neutral deduction kinds.
            for (group, label, kind) in [
                ("rake", "Rake", DeductionKind::Rake),
                ("jackpot", "Jackpot", DeductionKind::Jackpot),
                ("bingo", "Bingo", DeductionKind::Other),
                ("fortune", "Fortune", DeductionKind::Other),
                ("tax", "Tax", DeductionKind::Other),
            ] {
                self.deductions.push(PotDeduction {
                    kind,
                    label: label.to_string(),
                    amount: parse_decimal(cap(&caps, group))?,
                });
            }
            return Ok(());
        }
        if line.starts_with("Total pot ") {
            // A fee label we have never seen: refuse rather than mis-account.
            return Err(unrecognized());
        }

        if let Some(caps) = RE_SUMMARY_BOARD.captures(line) {
            let run = run_index_from_word(caps.name("run").map(|m| m.as_str()));
            let cards = parse_card_list(cap(&caps, "cards"))?;
            self.summary.board_lines.push((run, cards));
            return Ok(());
        }

        if let Some(caps) = RE_RUN_COUNT.captures(line) {
            self.summary.stated_run_count = Some(if cap(&caps, "n") == "two" { 2 } else { 3 });
            return Ok(());
        }

        if let Some(caps) = RE_SUMMARY_SEAT.captures(line) {
            let seat: u8 = cap(&caps, "seat").parse().map_err(|_| unrecognized())?;
            let rest = cap(&caps, "rest");
            let player = self
                .players
                .iter()
                .find(|p| p.seat == seat)
                .ok_or_else(unrecognized)?;
            let after_name = rest
                .strip_prefix(player.name.as_str())
                .ok_or_else(unrecognized)?;
            if !(after_name.is_empty() || after_name.starts_with(' ')) {
                return Err(unrecognized());
            }
            if self.summary.seat_lines.contains(&seat) {
                return Err(unrecognized());
            }
            self.summary.seat_lines.push(seat);

            let mut summary = SeatSummary {
                button: false,
                small_blind: false,
                big_blind: false,
                result: SeatResult::Won {
                    verb: PayoutVerb::Won,
                },
            };
            let mut tail = after_name;
            while let Some(m) = RE_SEAT_LABEL.captures(tail) {
                match cap(&m, "label") {
                    "button" => summary.button = true,
                    "small blind" => summary.small_blind = true,
                    _ => summary.big_blind = true,
                }
                let consumed = m.get(0).map_or(0, |whole| whole.end());
                tail = tail.get(consumed..).unwrap_or("");
            }
            // A line that does not follow the grammar is refused; one that mentions a cashout in
            // an unknown form is reported as an unsupported adjustment.
            let refuse = || {
                if line.contains("Cashout") {
                    ParseError::UnsupportedAdjustment {
                        line: line_idx,
                        content: line.to_string(),
                    }
                } else {
                    unrecognized()
                }
            };
            let mut won = Decimal::ZERO;
            let mut cashout_risk = None;
            let mut cashout_received = None;
            summary.result = if let Some(m) = RE_SEAT_FOLDED.captures(tail) {
                let street = match cap(&m, "when") {
                    "before Flop" => Street::Preflop,
                    "on the Flop" => Street::Flop,
                    "on the Turn" => Street::Turn,
                    _ => Street::River,
                };
                SeatResult::Folded {
                    street,
                    didnt_bet: m.name("nobet").is_some(),
                }
            } else if let Some(m) = RE_SEAT_WON.captures(tail) {
                won = parse_decimal(cap(&m, "amt"))?;
                let verb = match cap(&m, "verb") {
                    "won" => PayoutVerb::Won,
                    _ => PayoutVerb::Collected,
                };
                SeatResult::Won { verb }
            } else if let Some(m) = RE_SEAT_SHOWED_UNCONTESTED.captures(tail) {
                won = parse_decimal(cap(&m, "amt"))?;
                let verb = match cap(&m, "verb") {
                    "won" => PayoutVerb::Won,
                    _ => PayoutVerb::Collected,
                };
                let cards_raw = cap(&m, "cards");
                let cards = parse_card_list(cards_raw)?;
                if cards.is_empty() || cards.len() > 2 {
                    return Err(ParseError::InvalidCardSyntax {
                        token: cards_raw.to_string(),
                    });
                }
                SeatResult::ShowedUncontested { cards, verb }
            } else if let Some(m) = RE_SEAT_SHOWED.captures(tail) {
                let mut body = cap(&m, "outcomes");
                if let Some(c) = RE_SEAT_CASHOUT_SUFFIX.captures(body) {
                    cashout_risk = Some(parse_decimal(cap(&c, "amt"))?);
                    body = c.name("body").map_or("", |b| b.as_str());
                    if body.contains("Cashout") {
                        return Err(ParseError::UnsupportedAdjustment {
                            line: line_idx,
                            content: line.to_string(),
                        });
                    }
                } else if let Some(c) = RE_SEAT_EV_CASHOUT_SUFFIX.captures(body) {
                    cashout_received = Some(parse_decimal(cap(&c, "amt"))?);
                    body = c.name("body").map_or("", |b| b.as_str());
                    if body.contains("Cashout") {
                        return Err(ParseError::UnsupportedAdjustment {
                            line: line_idx,
                            content: line.to_string(),
                        });
                    }
                }
                let mut outcomes = 0usize;
                let mut all_lost = true;
                for outcome in body.split(", and ") {
                    let o = RE_SEAT_RUN_OUTCOME.captures(outcome).ok_or_else(refuse)?;
                    if let Some(amt) = o.name("amt") {
                        won = won
                            .checked_add(parse_decimal(amt.as_str())?)
                            .ok_or_else(refuse)?;
                        all_lost = false;
                    }
                    outcomes += 1;
                }
                let cards_raw = cap(&m, "cards");
                let cards = parse_card_list(cards_raw)?;
                if cards.len() != 2 {
                    return Err(ParseError::InvalidCardSyntax {
                        token: cards_raw.to_string(),
                    });
                }
                SeatResult::Showed {
                    cards,
                    outcomes,
                    all_lost,
                }
            } else {
                return Err(refuse());
            };
            self.summary.seat_summaries.insert(seat, summary);
            self.summary.seat_won.insert(seat, won);
            if let Some(risk) = cashout_risk {
                self.summary.seat_cashout_risk.insert(seat, risk);
            }
            if let Some(received) = cashout_received {
                self.summary.seat_cashout_received.insert(seat, received);
            }
            return Ok(());
        }

        Err(unrecognized())
    }

    /// Cross-checks the summary section against the hand body.
    fn check_summary(
        &mut self,
        header: &HandHeader,
        last_line_idx: usize,
    ) -> Result<(), ParseError> {
        let hand_id = header.site_hand_id.as_str();
        let mismatch = |detail: String| ParseError::SummaryMismatch {
            hand_id: hand_id.to_string(),
            detail,
        };

        // Record known cards from ShowedUncontested before cross-checks. An opponent's summary
        // show must repeat every card they already showed in the body (Hero's summary may show
        // one of the two dealt cards).
        for player in &mut self.players {
            if let Some(seat_sum) = self.summary.seat_summaries.get(&player.seat)
                && let SeatResult::ShowedUncontested { cards, .. } = &seat_sum.result
            {
                if !player.is_hero && !player.known_cards().all(|k| cards.contains(&k)) {
                    return Err(mismatch(format!(
                        "summary show for '{}' omits a card shown in the body",
                        player.name
                    )));
                }
                for card in cards {
                    if !player.record_known_card(*card) {
                        return Err(ParseError::ConflictingHoleCards {
                            player_name: player.name.clone(),
                        });
                    }
                }
            }
        }

        let s = &self.summary;

        // Runouts.
        let multi = self.boards.iter().any(|b| b.run_index > 0);

        if !self.cashouts.is_empty() && (self.winners.len() > 1 || multi) {
            return Err(ParseError::UnsupportedAdjustment {
                line: last_line_idx,
                content: "EV Cashout in split pot or multi-run hand is unsupported".to_string(),
            });
        }

        for c in &self.cashouts {
            if c.settlement.is_none() {
                return Err(ParseError::UnsupportedAdjustment {
                    line: last_line_idx,
                    content: format!(
                        "'{}' chose EV Cashout but no 'Pays Cashout Risk' or 'Receives Cashout' settlement was found",
                        c.player_name
                    ),
                });
            }
        }

        let expected_runs = if multi { Some(self.boards.len()) } else { None };
        if s.stated_run_count != expected_runs {
            return Err(mismatch(format!(
                "summary states {:?} runs, body dealt {:?}",
                s.stated_run_count, expected_runs
            )));
        }

        // Board lines: a single board must match exactly; a multi-run summary may print only
        // the cards that differ from the first runout, so it must be a suffix.
        if s.board_lines.len() != self.boards.len() {
            return Err(mismatch(format!(
                "{} board lines for {} dealt boards",
                s.board_lines.len(),
                self.boards.len()
            )));
        }
        for (run, cards) in &s.board_lines {
            let board = self
                .boards
                .iter()
                .find(|b| b.run_index == *run)
                .ok_or_else(|| mismatch(format!("summary board for unknown run {run}")))?;
            let dealt: Vec<Card> = board.cards().collect();
            let ok = if multi {
                !cards.is_empty() && dealt.ends_with(cards)
            } else {
                dealt == *cards
            };
            if !ok {
                return Err(mismatch(format!(
                    "board for run {run} differs from summary"
                )));
            }
        }

        // One seat line per player, and its winnings / cashout risk match the body.
        if s.seat_lines.len() != self.players.len() {
            return Err(mismatch(
                "summary seat lines do not match seated players".to_string(),
            ));
        }
        for p in &self.players {
            let body_won = self
                .winners
                .iter()
                .filter(|w| w.player_name == p.name)
                .try_fold(Decimal::ZERO, |acc, w| acc.checked_add(w.amount))
                .ok_or_else(|| mismatch("overflow".to_string()))?;
            let summary_won = s.seat_won.get(&p.seat).copied().unwrap_or(Decimal::ZERO);
            if body_won != summary_won {
                return Err(mismatch(format!(
                    "'{}' collected {body_won} in the body but {summary_won} in the summary",
                    p.name
                )));
            }
            let body_cashout = self.cashouts.iter().find(|c| c.player_name == p.name);
            let summary_risk = s.seat_cashout_risk.get(&p.seat).copied();
            let summary_received = s.seat_cashout_received.get(&p.seat).copied();

            match body_cashout.and_then(|c| c.settlement) {
                Some(CashoutSettlement::RiskPaid(amt)) => {
                    if summary_risk != Some(amt) || summary_received.is_some() {
                        return Err(mismatch(format!(
                            "cashout risk for '{}' differs from summary",
                            p.name
                        )));
                    }
                    if body_won <= Decimal::ZERO {
                        return Err(mismatch(format!(
                            "'{}' paid cashout risk but did not win",
                            p.name
                        )));
                    }
                }
                Some(CashoutSettlement::Received(amt)) => {
                    if summary_received != Some(amt) || summary_risk.is_some() {
                        return Err(mismatch(format!(
                            "cashout received for '{}' differs from summary",
                            p.name
                        )));
                    }
                    let is_showed_all_lost = matches!(
                        s.seat_summaries.get(&p.seat).map(|s| &s.result),
                        Some(SeatResult::Showed { all_lost: true, .. })
                    );
                    if !is_showed_all_lost || body_won != Decimal::ZERO {
                        return Err(mismatch(format!(
                            "'{}' received cashout but did not lose showdown",
                            p.name
                        )));
                    }
                }
                None => {
                    if summary_risk.is_some() || summary_received.is_some() {
                        return Err(mismatch(format!(
                            "unexpected cashout in summary for '{}'",
                            p.name
                        )));
                    }
                }
            }
        }

        // Position labels, fold streets and shown cards on each seat line match the body.
        let first_big_blind = self
            .actions
            .iter()
            .find(|a| a.action_type == ActionType::PostBigBlind)
            .map(|a| a.player_name.as_str());
        let contested = self
            .players
            .iter()
            .filter(|p| {
                !self
                    .actions
                    .iter()
                    .any(|a| a.player_name == p.name && a.action_type == ActionType::Fold)
            })
            .count()
            >= 2;
        let runs = self.boards.len().max(1);
        let flop_dealt = self.boards.iter().any(|b| b.flop.is_some());
        for p in &self.players {
            let seat_line = s
                .seat_summaries
                .get(&p.seat)
                .ok_or_else(|| mismatch(format!("no summary line for '{}'", p.name)))?;
            let own: Vec<&StreetAction> = self
                .actions
                .iter()
                .filter(|a| a.player_name == p.name)
                .collect();
            let posted_small = own
                .iter()
                .any(|a| a.action_type == ActionType::PostSmallBlind);
            let heads_up = self.players.len() == 2;
            if heads_up && (p.seat == header.button_seat) != posted_small {
                return Err(mismatch(format!(
                    "heads-up button and small blind poster differ ('{}')",
                    p.name
                )));
            }
            let expect_button = p.seat == header.button_seat && !(heads_up && posted_small);
            if seat_line.button != expect_button
                || seat_line.small_blind != posted_small
                || seat_line.big_blind != (first_big_blind == Some(p.name.as_str()))
            {
                return Err(mismatch(format!(
                    "position labels for '{}' disagree with the button and blinds",
                    p.name
                )));
            }
            let folded_on = own
                .iter()
                .find(|a| a.action_type == ActionType::Fold)
                .map(|a| a.street);
            let contributed = own.iter().any(|a| a.action_type.is_contribution());
            let ok = match &seat_line.result {
                SeatResult::Folded { street, didnt_bet } => {
                    folded_on == Some(*street) && *didnt_bet == !contributed
                }
                SeatResult::Won { verb } => {
                    folded_on.is_none() && !contested && (*verb == PayoutVerb::Won) == flop_dealt
                }
                SeatResult::Showed {
                    cards,
                    outcomes,
                    all_lost,
                } => {
                    let mut shown = cards.clone();
                    let mut known: Vec<Card> = p.known_cards().collect();
                    shown.sort_unstable();
                    known.sort_unstable();
                    // `won` (the only payout verb this shape allows) needs a flop.
                    folded_on.is_none()
                        && shown.len() == 2
                        && shown == known
                        && *outcomes == runs
                        && (*all_lost || flop_dealt)
                }
                SeatResult::ShowedUncontested { cards, verb } => {
                    let no_dups = !matches!(cards.as_slice(), [a, b] if a == b);
                    folded_on.is_none()
                        && !contested
                        && runs == 1
                        && (1..=2).contains(&cards.len())
                        && no_dups
                        && cards.iter().all(|c| p.known_cards().any(|k| k == *c))
                        && (*verb == PayoutVerb::Won) == flop_dealt
                }
            };
            if !ok {
                return Err(mismatch(format!(
                    "summary result for '{}' disagrees with the hand body",
                    p.name
                )));
            }
        }
        Ok(())
    }

    fn apply_street_header(
        &mut self,
        phase: ParserPhase,
        line_idx: usize,
        caps: &regex::Captures<'_>,
    ) -> Result<ParserPhase, ParseError> {
        let run_index = run_index_from_word(caps.name("run").map(|m| m.as_str()));
        let street_token = cap(caps, "street");
        let fail = || ParseError::IncompleteStreet {
            line: line_idx,
            current: phase.as_str().to_string(),
            attempted: caps.get(0).map_or("", |m| m.as_str()).to_string(),
        };
        let multi = self.boards.iter().any(|b| b.run_index > 0);

        match street_token {
            "HOLE CARDS" => {
                if phase != ParserPhase::SeatingAndBlinds || run_index != 0 {
                    return Err(fail());
                }
                Ok(ParserPhase::Play(Street::Preflop))
            }
            "FLOP" => {
                let valid = (phase == ParserPhase::Play(Street::Preflop) && run_index <= 1)
                    || (phase == ParserPhase::Play(Street::River) && run_index > 1);
                if !valid || self.boards.iter().any(|b| b.run_index == run_index) {
                    return Err(fail());
                }
                if caps.name("b2").is_some() {
                    return Err(fail());
                }
                let flop = parse_three_cards(cap(caps, "b1"))?;
                get_or_create_board(&mut self.boards, run_index).flop = Some(flop);
                self.street_commitments.clear();
                Ok(ParserPhase::Play(Street::Flop))
            }
            "TURN" | "RIVER" => {
                let street = if street_token == "TURN" {
                    Street::Turn
                } else {
                    Street::River
                };
                prepare_board_for_street(&mut self.boards, run_index, street);
                let prev_street = if street == Street::Turn {
                    Street::Flop
                } else {
                    Street::Turn
                };
                let valid_phase = phase == ParserPhase::Play(prev_street)
                    || (phase == ParserPhase::Play(Street::River) && run_index > 1);
                let board = self
                    .boards
                    .iter_mut()
                    .find(|b| b.run_index == run_index)
                    .ok_or_else(fail)?;
                let ready = match street {
                    Street::Turn => board.flop.is_some() && board.turn.is_none(),
                    _ => board.turn.is_some() && board.river.is_none(),
                };
                if !valid_phase || !ready {
                    return Err(fail());
                }
                let prefix: Vec<Card> = board.cards().collect();
                let shown_prefix = parse_card_list(cap(caps, "b1"))?;
                let Some(new_card) = caps.name("b2") else {
                    return Err(fail());
                };
                if shown_prefix != prefix {
                    return Err(ParseError::InvalidCardSyntax {
                        token: cap(caps, "b1").to_string(),
                    });
                }
                let card = parse_single_card(new_card.as_str())?;
                if street == Street::Turn {
                    board.turn = Some(card);
                } else {
                    board.river = Some(card);
                }
                self.street_commitments.clear();
                Ok(ParserPhase::Play(street))
            }
            "SHOWDOWN" => {
                let valid = match run_index {
                    0 => matches!(phase, ParserPhase::Play(_)) && !multi,
                    1 => matches!(phase, ParserPhase::Play(_)) && multi,
                    n => {
                        phase == ParserPhase::Showdown
                            && n == self.showdown_run + 1
                            && usize::from(n) <= self.boards.len()
                    }
                };
                if !valid || caps.name("b1").is_some() {
                    return Err(fail());
                }
                self.showdown_run = run_index;
                self.street_commitments.clear();
                Ok(ParserPhase::Showdown)
            }
            "SUMMARY" => {
                let all_runs_shown = !multi || usize::from(self.showdown_run) == self.boards.len();
                if phase != ParserPhase::Showdown
                    || run_index != 0
                    || caps.name("b1").is_some()
                    || !all_runs_shown
                {
                    return Err(fail());
                }
                Ok(ParserPhase::Summary)
            }
            _ => Err(fail()),
        }
    }
}

impl HandParser for GgPokerParser {
    fn platform(&self) -> Platform {
        Platform::GGPoker
    }

    fn describe(&self) -> String {
        format!("GGPoker ({})", self.currency)
    }

    /// GGPoker exports start with `Poker Hand #<id>: <game> (<stakes>) - <timestamp>`.
    /// Any game is claimed here so that unsupported GGPoker games are reported per hand
    /// as `UnsupportedGame` rather than as an unrecognized file.
    fn recognizes(&self, raw_session: &str) -> bool {
        first_content_line(raw_session).is_some_and(|line| RE_HEADER.is_match(line))
    }

    fn peek_hand_id(&self, raw_block: &str) -> Option<String> {
        let first = raw_block.lines().next()?;
        let rest = first.strip_prefix("Poker Hand #")?;
        let id = rest.split(':').next()?;
        (!id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric())).then(|| id.to_string())
    }

    fn parse_hand(&self, raw_block: &str) -> Result<CanonicalHand, ParseError> {
        let mut lines = raw_block
            .lines()
            .enumerate()
            .map(|(idx, line)| (idx + 1, line.trim_end()))
            .filter(|(_, line)| !line.is_empty());

        // --- Header line -------------------------------------------------------------------
        let (_, header_line) = lines.next().ok_or(ParseError::EmptyHandBlock)?;
        let header_caps =
            RE_HEADER
                .captures(header_line)
                .ok_or_else(|| ParseError::InvalidHeader {
                    line: header_line.to_string(),
                })?;
        let unsupported = || ParseError::UnsupportedGame {
            line: header_line.to_string(),
        };
        if cap(&header_caps, "game") != "Hold'em No Limit" {
            return Err(unsupported());
        }
        let stakes = RE_STAKES
            .captures(cap(&header_caps, "stakes"))
            .ok_or_else(unsupported)?;
        let site_hand_id = cap(&header_caps, "id").to_string();
        let small_blind = parse_decimal(cap(&stakes, "sb"))?;
        let big_blind = parse_decimal(cap(&stakes, "bb"))?;
        let ts_raw = cap(&header_caps, "ts");
        let timestamp =
            NaiveDateTime::parse_from_str(ts_raw, "%Y/%m/%d %H:%M:%S").map_err(|_| {
                ParseError::InvalidTimestamp {
                    raw: ts_raw.to_string(),
                }
            })?;

        // --- Table line --------------------------------------------------------------------
        let (_, table_line) = lines.next().ok_or_else(|| ParseError::InvalidTableHeader {
            line: String::new(),
        })?;
        let bad_table = || ParseError::InvalidTableHeader {
            line: table_line.to_string(),
        };
        let table_caps = RE_TABLE.captures(table_line).ok_or_else(bad_table)?;
        let max_seats: u8 = cap(&table_caps, "max").parse().map_err(|_| bad_table())?;
        let button_seat: u8 = cap(&table_caps, "btn").parse().map_err(|_| bad_table())?;

        let header = HandHeader {
            platform: Platform::GGPoker,
            site_hand_id,
            game_type: GameType::Cash,
            limit: BettingLimit::NoLimit,
            currency: self.currency,
            small_blind,
            big_blind,
            timestamp,
            // GGPoker exports do not state a time zone.
            utc_offset_seconds: None,
            table_name: cap(&table_caps, "table").to_string(),
            max_seats,
            button_seat,
        };

        // --- Body --------------------------------------------------------------------------
        let mut state = HandState {
            stakes: (header.small_blind, header.big_blind),
            ..HandState::default()
        };
        let mut phase = ParserPhase::SeatingAndBlinds;
        let mut last_line_idx = 2;

        for (line_idx, line) in lines {
            last_line_idx = line_idx;

            if let Some(caps) = RE_STREET.captures(line) {
                phase = state.apply_street_header(phase, line_idx, &caps)?;
                continue;
            }

            match phase {
                ParserPhase::SeatingAndBlinds => {
                    if let Some(seat_caps) = RE_SEAT.captures(line) {
                        // Seat lines come first, in ascending seat order.
                        let misplaced = || ParseError::UnrecognizedLine {
                            line: line_idx,
                            content: line.to_string(),
                        };
                        if !state.actions.is_empty() {
                            return Err(misplaced());
                        }
                        let seat: u8 = cap(&seat_caps, "seat").parse().map_err(|_| misplaced())?;
                        if state.players.last().is_some_and(|p| p.seat >= seat) {
                            return Err(misplaced());
                        }
                        state.players.push(Player {
                            seat,
                            name: cap(&seat_caps, "name").to_string(),
                            starting_stack: parse_decimal(cap(&seat_caps, "stack"))?,
                            hole_cards: [None, None],
                            is_hero: false,
                        });
                    } else {
                        state.apply_blind_line(line_idx, line)?;
                    }
                }
                ParserPhase::Play(street) => {
                    if line.starts_with("Dealt to ") {
                        // One `Dealt to` line per seated player, in seat order, straight after
                        // `*** HOLE CARDS ***`.
                        let misplaced = || ParseError::UnrecognizedLine {
                            line: line_idx,
                            content: line.to_string(),
                        };
                        if street != Street::Preflop || state.dealing_done {
                            return Err(misplaced());
                        }
                        let dealt = RE_DEALT.captures(line).ok_or_else(|| {
                            ParseError::InvalidCardSyntax {
                                token: line.to_string(),
                            }
                        })?;
                        let name = cap(&dealt, "name");
                        state.ensure_seated(name)?;
                        let seat = state
                            .players
                            .iter()
                            .find(|p| p.name == name)
                            .map_or(0, |p| p.seat);
                        if state.dealt_seats.last().is_some_and(|last| *last >= seat) {
                            return Err(misplaced());
                        }
                        state.dealt_seats.push(seat);
                        if let (Some(c1), Some(c2)) = (dealt.name("c1"), dealt.name("c2")) {
                            let c1 = parse_card_token(c1.as_str())?;
                            let c2 = parse_card_token(c2.as_str())?;
                            let player = state.player_mut(name)?;
                            if player.is_hero
                                || !player.record_known_card(c1)
                                || !player.record_known_card(c2)
                            {
                                return Err(ParseError::ConflictingHoleCards {
                                    player_name: name.to_string(),
                                });
                            }
                            player.is_hero = true;
                        }
                    } else {
                        state.dealing_done = true;
                        state.apply_play_line(line_idx, line, street)?;
                    }
                }
                ParserPhase::Showdown => state.apply_showdown_line(line_idx, line)?,
                ParserPhase::Summary => state.apply_summary_line(line_idx, line)?,
            }
        }

        if phase != ParserPhase::Summary || !state.summary.saw_total_pot {
            return Err(ParseError::IncompleteStreet {
                line: last_line_idx,
                current: phase.as_str().to_string(),
                attempted: "SUMMARY".to_string(),
            });
        }
        if state.dealt_seats.len() != state.players.len() {
            return Err(ParseError::InvariantViolation {
                hand_id: header.site_hand_id,
                detail: format!(
                    "{} 'Dealt to' line(s) for {} seated players",
                    state.dealt_seats.len(),
                    state.players.len()
                ),
            });
        }
        state.check_summary(&header, last_line_idx)?;

        // GGPoker EV Cashout -> site-neutral player adjustment (a debit of the risk paid).
        let mut adjustments = Vec::with_capacity(state.cashouts.len());
        for c in state.cashouts {
            let Some(settlement) = c.settlement else {
                return Err(ParseError::UnsupportedAdjustment {
                    line: last_line_idx,
                    content: format!(
                        "'{}' chose EV Cashout but no 'Pays Cashout Risk' or 'Receives Cashout' settlement was found",
                        c.player_name
                    ),
                });
            };
            let (kind, amount) = match settlement {
                CashoutSettlement::RiskPaid(risk) => (AdjustmentKind::EvCashoutRiskPaid, -risk),
                CashoutSettlement::Received(received) => {
                    (AdjustmentKind::EvCashoutReceived, received)
                }
            };
            adjustments.push(PlayerAdjustment {
                player_name: c.player_name,
                kind,
                street: c.chosen_on,
                amount,
            });
        }

        let hand = CanonicalHand {
            header,
            players: state.players,
            actions: state.actions,
            boards: state.boards,
            outcome: HandOutcome {
                total_pot: state.total_pot,
                deductions: state.deductions,
                uncalled_returned: state.uncalled_returned,
                winners: state.winners,
                adjustments,
            },
        };

        validate_hand(&hand)?;
        Ok(hand)
    }
}
