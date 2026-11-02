//! Stack continuity between consecutive hands at a table, a site-neutral cross-check that no
//! single hand can provide: a player's stack can grow between hands (a top-up or rebuy) but
//! never shrink. A shrinking stack means a misparsed or corrupted hand on one side of the gap.

use std::fmt;
use std::path::PathBuf;

use chrono::NaiveDateTime;
use rust_decimal::Decimal;

use crate::domain::canonical::{CanonicalHand, Platform};

/// Starting and ending stacks of one parsed hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackSnapshot {
    pub platform: Platform,
    pub table_name: String,
    pub timestamp: NaiveDateTime,
    pub hand_id: String,
    /// 1-based line in the source file where the hand starts.
    pub block_start_line: usize,
    pub players: Vec<PlayerStacks>,
}

/// One player's stack at the start and the end of a hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerStacks {
    pub name: String,
    pub is_hero: bool,
    pub starting: Decimal,
    /// `starting + net` (net includes off-pot adjustments).
    pub ending: Decimal,
}

impl StackSnapshot {
    /// The stacks of `hand`, or `None` if a result overflows (validated hands never do).
    #[must_use]
    pub fn of(hand: &CanonicalHand, block_start_line: usize) -> Option<Self> {
        let results = hand.checked_player_results()?;
        let players = hand
            .players
            .iter()
            .zip(results)
            .map(|(p, r)| {
                Some(PlayerStacks {
                    name: p.name.clone(),
                    is_hero: p.is_hero,
                    starting: p.starting_stack,
                    ending: p.starting_stack.checked_add(r.net)?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            platform: hand.header.platform,
            table_name: hand.header.table_name.clone(),
            timestamp: hand.header.timestamp,
            hand_id: hand.header.site_hand_id.clone(),
            block_start_line,
            players,
        })
    }
}

/// A player who starts a hand with less than they ended the previous hand at the same table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackWarning {
    pub file_path: PathBuf,
    pub table_name: String,
    pub player_name: String,
    pub is_hero: bool,
    pub previous_hand_id: String,
    pub hand_id: String,
    /// 1-based line in `file_path` where `hand_id` starts.
    pub block_start_line: usize,
    /// The player's stack at the end of `previous_hand_id`.
    pub previous_ending: Decimal,
    /// The player's stack at the start of `hand_id`.
    pub starting: Decimal,
}

impl fmt::Display for StackWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{} hand {}: '{}'{} starts with {} at table '{}' but ended hand {} with {}; one of \
             the two hands is probably misrecorded",
            self.file_path.display(),
            self.block_start_line,
            self.hand_id,
            self.player_name,
            if self.is_hero { " (hero)" } else { "" },
            self.starting,
            self.table_name,
            self.previous_hand_id,
            self.previous_ending,
        )
    }
}

/// Checks every pair of consecutive hands (by timestamp) at the same table in one file: each
/// player seated in both must start the later hand with at least the stack they ended the
/// earlier one with.
///
/// A pair is skipped when its order is unknown (identical timestamps) or when a block that did
/// not parse lies between the two hands in the file (`unparsed_lines` are those blocks' start
/// lines), because the missing hand may explain the change.
#[must_use]
pub fn check_stack_continuity(
    file_path: &std::path::Path,
    snapshots: &mut [StackSnapshot],
    unparsed_lines: &[usize],
) -> Vec<StackWarning> {
    snapshots.sort_by(|a, b| {
        (a.platform.as_str(), a.table_name.as_str(), a.timestamp).cmp(&(
            b.platform.as_str(),
            b.table_name.as_str(),
            b.timestamp,
        ))
    });
    let mut warnings = Vec::new();
    for pair in snapshots.windows(2) {
        let [prev, next] = pair else { continue };
        let (low, high) = if prev.block_start_line < next.block_start_line {
            (prev.block_start_line, next.block_start_line)
        } else {
            (next.block_start_line, prev.block_start_line)
        };
        if prev.platform != next.platform
            || prev.table_name != next.table_name
            || prev.timestamp == next.timestamp
            || unparsed_lines.iter().any(|l| (low..high).contains(l))
        {
            continue;
        }
        for p in &next.players {
            let Some(before) = prev.players.iter().find(|q| q.name == p.name) else {
                continue;
            };
            if p.starting < before.ending {
                warnings.push(StackWarning {
                    file_path: file_path.to_path_buf(),
                    table_name: next.table_name.clone(),
                    player_name: p.name.clone(),
                    is_hero: p.is_hero,
                    previous_hand_id: prev.hand_id.clone(),
                    hand_id: next.hand_id.clone(),
                    block_start_line: next.block_start_line,
                    previous_ending: before.ending,
                    starting: p.starting,
                });
            }
        }
    }
    warnings
}
