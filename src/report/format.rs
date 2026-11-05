//! Formatting helpers for report output (stats.md §1.2).

use std::borrow::Cow;

use rust_decimal::{Decimal, RoundingStrategy};

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

/// Rounds a Decimal half-even to 2 decimal places with scale 2, normalizing negative zero.
#[must_use]
pub fn round_half_even_2dp(val: Decimal) -> Decimal {
    let mut rounded = val.round_dp_with_strategy(2, RoundingStrategy::MidpointNearestEven);
    rounded.rescale(2);
    if rounded.is_zero() && rounded.is_sign_negative() {
        rounded = Decimal::new(0, 2);
    }
    rounded
}

/// Computes `(num / den) * 100` rounded half-even to 2 decimal places, or `None` if `den <= 0`.
#[must_use]
pub fn round_pct(num: i64, den: i64) -> Option<Decimal> {
    if den <= 0 {
        None
    } else {
        let val = (Decimal::from(num) / Decimal::from(den)) * Decimal::from(100);
        Some(round_half_even_2dp(val))
    }
}

/// Computes `num / den` rounded half-even to 2 decimal places, or `None` if `den <= 0`.
#[must_use]
pub fn round_factor(num: i64, den: i64) -> Option<Decimal> {
    if den <= 0 {
        None
    } else {
        let val = Decimal::from(num) / Decimal::from(den);
        Some(round_half_even_2dp(val))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn round_half_even_scale_two_and_no_negative_zero() {
        let d = round_half_even_2dp(Decimal::new(-1, 3)); // -0.001
        assert_eq!(d, Decimal::new(0, 2));
        assert_eq!(d.scale(), 2);
        assert!(!d.is_sign_negative());

        // Ties round to nearest even hundredth
        let d2 = round_half_even_2dp(Decimal::new(53125, 3)); // 53.125 -> 53.12 (2 is even)
        assert_eq!(d2, Decimal::new(5312, 2));
        assert_eq!(d2.scale(), 2);

        let d2_odd = round_half_even_2dp(Decimal::new(53135, 3)); // 53.135 -> 53.14 (4 is even)
        assert_eq!(d2_odd, Decimal::new(5314, 2));
        assert_eq!(d2_odd.scale(), 2);

        let d3 = round_half_even_2dp(Decimal::new(0, 0));
        assert_eq!(d3.scale(), 2);
        assert_eq!(d3.to_string(), "0.00");

        // Negative zero input normalizes to positive zero
        let neg_zero = -Decimal::new(0, 2);
        assert!(neg_zero.is_sign_negative());
        let norm_zero = round_half_even_2dp(neg_zero);
        assert_eq!(norm_zero.scale(), 2);
        assert!(!norm_zero.is_sign_negative());
        assert_eq!(norm_zero.to_string(), "0.00");

        // -0.005 rounds to -0.00, normalized to 0.00
        let neg_tie = round_half_even_2dp(Decimal::new(-5, 3));
        assert_eq!(neg_tie.scale(), 2);
        assert!(!neg_tie.is_sign_negative());
        assert_eq!(neg_tie.to_string(), "0.00");
    }
}
