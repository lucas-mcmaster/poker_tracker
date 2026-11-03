//! Formatting helpers for report output (stats.md §1.2).

use rust_decimal::{Decimal, RoundingStrategy};

use crate::domain::money::micro_units_to_decimal;

/// Formats exact money amounts with trailing zeros removed (`-25.79`, `14.23115`, `-1`, `0`).
#[must_use]
pub fn format_exact_money(micros: i64) -> String {
    micro_units_to_decimal(micros).to_string()
}

/// Rounds a Decimal half-even to 2 decimal places and formats with exactly 2 decimals.
#[must_use]
pub fn format_fixed2(val: Decimal) -> String {
    let rounded = val.round_dp_with_strategy(2, RoundingStrategy::MidpointNearestEven);
    format!("{rounded:.2}")
}

/// Formats a percentage `(num / den) * 100` half-even to 2 decimals.
/// Returns an empty string if `den == 0`.
#[must_use]
pub fn format_pct(num: i64, den: i64) -> String {
    if den == 0 {
        String::new()
    } else {
        let val = (Decimal::from(num) / Decimal::from(den)) * Decimal::from(100);
        format_fixed2(val)
    }
}

/// Formats an aggression factor `num / den` half-even to 2 decimals.
/// Returns an empty string if `den == 0`.
#[must_use]
pub fn format_factor(num: i64, den: i64) -> String {
    if den == 0 {
        String::new()
    } else {
        let val = Decimal::from(num) / Decimal::from(den);
        format_fixed2(val)
    }
}
