//! Compact 1-byte playing card representation (`0..52`).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Playing card rank from `Two` (`0`) through `Ace` (`12`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Rank {
    Two = 0,
    Three = 1,
    Four = 2,
    Five = 3,
    Six = 4,
    Seven = 5,
    Eight = 6,
    Nine = 7,
    Ten = 8,
    Jack = 9,
    Queen = 10,
    King = 11,
    Ace = 12,
}

impl Rank {
    /// Converts a raw `0..=12` integer into a [`Rank`].
    #[must_use]
    pub const fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Two),
            1 => Some(Self::Three),
            2 => Some(Self::Four),
            3 => Some(Self::Five),
            4 => Some(Self::Six),
            5 => Some(Self::Seven),
            6 => Some(Self::Eight),
            7 => Some(Self::Nine),
            8 => Some(Self::Ten),
            9 => Some(Self::Jack),
            10 => Some(Self::Queen),
            11 => Some(Self::King),
            12 => Some(Self::Ace),
            _ => None,
        }
    }

    /// Parses an ASCII byte (`2..=9`, `T`, `J`, `Q`, `K`, `A`, case-insensitive) into a [`Rank`].
    #[must_use]
    pub const fn from_ascii(byte: u8) -> Option<Self> {
        match byte {
            b'2' => Some(Self::Two),
            b'3' => Some(Self::Three),
            b'4' => Some(Self::Four),
            b'5' => Some(Self::Five),
            b'6' => Some(Self::Six),
            b'7' => Some(Self::Seven),
            b'8' => Some(Self::Eight),
            b'9' => Some(Self::Nine),
            b'T' | b't' => Some(Self::Ten),
            b'J' | b'j' => Some(Self::Jack),
            b'Q' | b'q' => Some(Self::Queen),
            b'K' | b'k' => Some(Self::King),
            b'A' | b'a' => Some(Self::Ace),
            _ => None,
        }
    }

    /// Returns the canonical uppercase character for this [`Rank`].
    #[must_use]
    pub const fn to_char(self) -> char {
        match self {
            Self::Two => '2',
            Self::Three => '3',
            Self::Four => '4',
            Self::Five => '5',
            Self::Six => '6',
            Self::Seven => '7',
            Self::Eight => '8',
            Self::Nine => '9',
            Self::Ten => 'T',
            Self::Jack => 'J',
            Self::Queen => 'Q',
            Self::King => 'K',
            Self::Ace => 'A',
        }
    }
}

/// Playing card suit (`Clubs = 0`, `Diamonds = 1`, `Hearts = 2`, `Spades = 3`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Suit {
    Clubs = 0,
    Diamonds = 1,
    Hearts = 2,
    Spades = 3,
}

impl Suit {
    /// Converts a raw `0..=3` integer into a [`Suit`].
    #[must_use]
    pub const fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Clubs),
            1 => Some(Self::Diamonds),
            2 => Some(Self::Hearts),
            3 => Some(Self::Spades),
            _ => None,
        }
    }

    /// Parses an ASCII suit character (`c`, `d`, `h`, `s`, case-insensitive) into a [`Suit`].
    #[must_use]
    pub const fn from_ascii(byte: u8) -> Option<Self> {
        match byte {
            b'c' | b'C' => Some(Self::Clubs),
            b'd' | b'D' => Some(Self::Diamonds),
            b'h' | b'H' => Some(Self::Hearts),
            b's' | b'S' => Some(Self::Spades),
            _ => None,
        }
    }

    /// Returns the canonical lowercase character for this [`Suit`].
    #[must_use]
    pub const fn to_char(self) -> char {
        match self {
            Self::Clubs => 'c',
            Self::Diamonds => 'd',
            Self::Hearts => 'h',
            Self::Spades => 's',
        }
    }
}

/// Error returned when parsing an invalid card token.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("invalid card token: '{0}'")]
pub struct CardParseError(pub String);

/// Zero-cost 1-byte playing card (`0..52`), where bits `2..=5` encode [`Rank`]
/// and bits `0..=1` encode [`Suit`].
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Card(u8);

impl Card {
    /// Constructs a [`Card`] from a [`Rank`] and [`Suit`].
    #[must_use]
    pub const fn new(rank: Rank, suit: Suit) -> Self {
        Self(((rank as u8) << 2) | (suit as u8))
    }

    /// Constructs a [`Card`] from a raw `0..52` integer.
    #[must_use]
    pub const fn from_raw(raw: u8) -> Option<Self> {
        if raw < 52 { Some(Self(raw)) } else { None }
    }

    /// Returns the packed `0..52` integer representation.
    #[must_use]
    pub const fn raw(self) -> u8 {
        self.0
    }

    /// Extracts the [`Rank`] of this card.
    #[must_use]
    pub const fn rank(self) -> Rank {
        match Rank::from_u8(self.0 >> 2) {
            Some(rank) => rank,
            None => Rank::Two,
        }
    }

    /// Extracts the [`Suit`] of this card.
    #[must_use]
    pub const fn suit(self) -> Suit {
        match Suit::from_u8(self.0 & 0b11) {
            Some(suit) => suit,
            None => Suit::Clubs,
        }
    }
}

impl<'de> Deserialize<'de> for Card {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = u8::deserialize(deserializer)?;
        Self::from_raw(raw).ok_or_else(|| {
            serde::de::Error::custom(format!("invalid raw card value {raw}: expected 0..52"))
        })
    }
}

impl FromStr for Card {
    type Err = CardParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s.as_bytes();
        if let [r_byte, s_byte] = bytes
            && let Some(rank) = Rank::from_ascii(*r_byte)
            && let Some(suit) = Suit::from_ascii(*s_byte)
        {
            Ok(Self::new(rank, suit))
        } else {
            Err(CardParseError(s.to_string()))
        }
    }
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.rank().to_char(), self.suit().to_char())
    }
}

impl fmt::Debug for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Card({self})")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn all_52_cards_round_trip_and_pack_into_0_to_51() {
        let ranks = [
            '2', '3', '4', '5', '6', '7', '8', '9', 'T', 'J', 'Q', 'K', 'A',
        ];
        let suits = ['c', 'd', 'h', 's'];
        let mut seen_mask: u64 = 0;

        for r in ranks {
            for s in suits {
                let token = format!("{r}{s}");
                let card = Card::from_str(&token).expect("valid card token");
                assert_eq!(card.to_string(), token);
                assert!(card.raw() < 52);
                assert_eq!(Card::from_raw(card.raw()), Some(card));
                let bit = 1u64 << card.raw();
                assert_eq!(seen_mask & bit, 0, "duplicate raw card index");
                seen_mask |= bit;
            }
        }

        assert_eq!(seen_mask.count_ones(), 52);
        assert_eq!(Card::from_raw(52), None);
    }

    #[test]
    fn case_insensitive_parse_normalizes_display() {
        let card = Card::from_str("aH").expect("should parse mixed case");
        assert_eq!(card.rank(), Rank::Ace);
        assert_eq!(card.suit(), Suit::Hearts);
        assert_eq!(card.to_string(), "Ah");
    }

    #[test]
    fn rejects_invalid_card_strings() {
        for invalid in ["", "A", "10s", "1h", "AhAs", "Xz", "9x", "0d"] {
            assert!(
                Card::from_str(invalid).is_err(),
                "expected '{invalid}' to fail parsing"
            );
        }
    }

    #[test]
    fn serde_deserialization_enforces_0_to_51_invariant() {
        let c0: Card = serde_json::from_str("0").expect("0 is a valid card index");
        assert_eq!(c0.raw(), 0);

        let c51: Card = serde_json::from_str("51").expect("51 is a valid card index");
        assert_eq!(c51.raw(), 51);

        for invalid_json in ["52", "64", "255"] {
            assert!(
                serde_json::from_str::<Card>(invalid_json).is_err(),
                "expected raw value {invalid_json} to fail Card deserialization"
            );
        }
    }
}
