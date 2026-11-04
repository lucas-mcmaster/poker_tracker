//! Formatting helpers for report output (stats.md §1.2).

use std::borrow::Cow;

use rust_decimal::{Decimal, RoundingStrategy};

use crate::domain::money::micro_units_to_decimal;

/// Quotes a CSV text field per RFC 4180 if it contains comma, double quote, CR, or LF.
#[must_use]
pub fn quote_csv_text(s: &str) -> Cow<'_, str> {
    if s.contains([',', '"', '\r', '\n']) {
        let mut out = String::with_capacity(s.len() + 4);
        out.push('"');
        for ch in s.chars() {
            if ch == '"' {
                out.push('"');
            }
            out.push(ch);
        }
        out.push('"');
        Cow::Owned(out)
    } else {
        Cow::Borrowed(s)
    }
}

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

#[cfg(test)]
mod tests {
    use super::quote_csv_text;

    #[test]
    fn csv_text_is_quoted_for_each_special_character_alone() {
        // (field, printed): RFC 4180 quotes a field with `,`, `"`, CR or LF and doubles quotes.
        for (field, printed) in [
            ("plain", "plain"),
            ("", ""),
            ("a,b", "\"a,b\""),
            ("a\"b", "\"a\"\"b\""),
            ("a\rb", "\"a\rb\""),
            ("a\nb", "\"a\nb\""),
        ] {
            assert_eq!(quote_csv_text(field), printed, "{field:?}");
        }
    }
}
