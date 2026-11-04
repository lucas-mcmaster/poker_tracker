//! Currency codes and lossless fixed-point money conversion.
//!
//! Amounts are `rust_decimal::Decimal` in memory and integer micro-units (millionths of the
//! currency unit) in storage. Amounts in different currencies are never combined; every hand
//! carries its own [`CurrencyCode`].

use std::fmt;
use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// Fixed-point integer scaling factor for storage: one micro-unit is one millionth of the
/// currency unit (`1.00 = 1,000,000`), so every `*_micros` column divides by `1e6`.
///
/// Supports any currency with up to 6 decimal places. An amount with finer precision is
/// rejected (never rounded) by [`decimal_to_micro_units`].
pub const MICRO_UNIT_SCALE: i64 = 1_000_000;

/// Number of decimal places represented by one micro-unit.
pub const MICRO_UNIT_DECIMALS: u32 = 6;

/// Losslessly converts a [`Decimal`] amount into integer micro-units (`1.00 = 1,000,000`).
///
/// Returns `None` if `value * 1,000,000` overflows, retains a non-zero fractional part, or
/// overflows `i64`.
#[must_use]
pub fn decimal_to_micro_units(value: Decimal) -> Option<i64> {
    let scaled = value.checked_mul(Decimal::from(MICRO_UNIT_SCALE))?;
    if !scaled.fract().is_zero() {
        return None;
    }
    scaled.try_into().ok()
}

/// Converts 64-bit integer micro-units (`1.00 = 1,000,000`) back into a [`Decimal`], with
/// trailing zeros removed (`18_600_000` becomes `18.6`).
#[must_use]
pub fn micro_units_to_decimal(micros: i64) -> Decimal {
    Decimal::new(micros, MICRO_UNIT_DECIMALS).normalize()
}

/// Error returned for a malformed currency code.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("invalid currency code '{0}': expected 3 uppercase ASCII letters (ISO 4217, e.g. CAD)")]
pub struct CurrencyParseError(pub String);

/// ISO-4217-style currency code (three uppercase ASCII letters), valid by construction.
///
/// Where a hand's currency comes from is a parser concern: some sites print it in the hand
/// history, others print only a symbol, so the account currency is configured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CurrencyCode([u8; 3]);

impl CurrencyCode {
    /// Canadian dollar.
    pub const CAD: Self = Self(*b"CAD");
    /// United States dollar.
    pub const USD: Self = Self(*b"USD");
    /// Euro.
    pub const EUR: Self = Self(*b"EUR");
    /// Pound sterling.
    pub const GBP: Self = Self(*b"GBP");

    /// Builds a code from three bytes, returning `None` unless all are uppercase ASCII letters.
    #[must_use]
    pub const fn from_bytes(code: [u8; 3]) -> Option<Self> {
        if code[0].is_ascii_uppercase()
            && code[1].is_ascii_uppercase()
            && code[2].is_ascii_uppercase()
        {
            Some(Self(code))
        } else {
            None
        }
    }

    /// The code as a string slice (e.g. `"CAD"`).
    #[must_use]
    pub fn as_str(&self) -> &str {
        // Always ASCII by construction.
        std::str::from_utf8(&self.0).unwrap_or("???")
    }
}

impl FromStr for CurrencyCode {
    type Err = CurrencyParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.as_bytes() {
            [a, b, c] => Self::from_bytes([*a, *b, *c]),
            _ => None,
        }
        .ok_or_else(|| CurrencyParseError(s.to_string()))
    }
}

impl fmt::Display for CurrencyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for CurrencyCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CurrencyCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn lossless_micro_unit_conversion_succeeds_for_6_decimal_places_or_fewer() {
        assert_eq!(MICRO_UNIT_SCALE, 1_000_000, "a micro-unit is one millionth");
        assert_eq!(decimal_to_micro_units(dec!(18.6)), Some(18_600_000));
        assert_eq!(decimal_to_micro_units(dec!(0.05)), Some(50_000));
        assert_eq!(decimal_to_micro_units(dec!(0.000001)), Some(1));
        assert_eq!(decimal_to_micro_units(dec!(0)), Some(0));
        assert_eq!(decimal_to_micro_units(dec!(-0.48)), Some(-480_000));
        assert_eq!(micro_units_to_decimal(18_600_000), dec!(18.6));
        assert_eq!(micro_units_to_decimal(18_600_000).to_string(), "18.6");
    }

    #[test]
    fn micro_unit_conversion_rejects_sub_micro_fractional_precision_and_overflow() {
        assert_eq!(decimal_to_micro_units(dec!(1.2345678)), None);
        assert_eq!(decimal_to_micro_units(dec!(0.0000001)), None);
        assert_eq!(decimal_to_micro_units(Decimal::MAX), None);
    }

    #[test]
    fn currency_codes_validate_and_round_trip() {
        assert_eq!("CAD".parse::<CurrencyCode>(), Ok(CurrencyCode::CAD));
        assert_eq!(CurrencyCode::USD.to_string(), "USD");
        for bad in ["cad", "CA", "CADX", "C$", "", "ÉUR"] {
            assert!(
                bad.parse::<CurrencyCode>().is_err(),
                "{bad} should be rejected"
            );
        }
        let json = serde_json::to_string(&CurrencyCode::CAD).unwrap();
        assert_eq!(json, "\"CAD\"");
        assert_eq!(
            serde_json::from_str::<CurrencyCode>(&json).unwrap(),
            CurrencyCode::CAD
        );
        assert!(serde_json::from_str::<CurrencyCode>("\"cad\"").is_err());
    }
}
