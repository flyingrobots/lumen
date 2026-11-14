//! Exact conversion of a decimal number to Q32.32, `lumen-ascii-ply-q32.32/1`.
//!
//! The rule is meant to give the same raw value, and refuse the same texts, as Bunny's
//! `bunny-decimal-q32-32/1` (in review as flyingrobots/bunny#281), so that replacing this interim
//! code with Bunny's reader changes no bit. For decimal text `t` with exact rational value
//! `v(t)`, the raw value is `round_half_even(v(t) * 2^32)`: to the nearest integer, an exact tie
//! to the even neighbour, applied to the magnitude and then the sign (so `-x` converts to
//! `-raw(x)` and `-0` is 0). No floating point is involved.
//!
//! **Grammar.** `[+-] digits [. digits] [(e|E) [+-] digits]`, with at least one digit on one side
//! of the point (`5.`, `.5`, `-.5e1` are valid; `.`, `e5`, `1e` are not). Leading zeros are
//! allowed; anything else (spaces, `nan`, `inf`, hexadecimal) is refused.
//!
//! **Decision.** Write the value as `D * 10^e` with `D` the digits without leading and trailing
//! zeros, `s` their count and `m = s + e`, so that `10^(m-1) <= |v| < 10^m`:
//!
//! 1. no nonzero digit: zero, whatever the exponent;
//! 2. `m >= 11`: `|v| >= 1e10 > 2^31`, [`DecimalError::OutOfRange`];
//! 3. `m <= -10`: `|v| < 1e-10`, below half a raw unit (`2^-33`), so zero;
//! 4. otherwise, `s > 28` is [`DecimalError::TooManyDigits`] (the working integers are 128-bit),
//!    and the value is computed exactly: `(D * 10^e) << 32` for `e >= 0` (`e <= 9`), or
//!    `(D << 32) / 10^-e` rounded half to even (`-e <= 37`).
//!
//! The result must fit: a magnitude of at most `2^63 - 1` for a positive value and `2^63` for a
//! negative one, decided after rounding. The exponent is read with saturation, so an exponent of
//! any length is decided by rules 1 to 3. The text is scanned twice and nothing is allocated.

#[cfg(test)]
mod tests;

use core::fmt;

use bunny_num::FixedQ32_32;

/// The most significant digits a decimal in range may have: `10^28 * 2^32 < 2^127`, so the
/// working integer fits a `u128`.
pub const MAX_SIGNIFICANT_DIGITS: usize = 28;

/// The exponent saturates at this magnitude; any larger exponent is decided by rules 1 to 3.
const EXPONENT_SATURATION: i64 = 1 << 40;

/// Why a decimal number was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecimalError {
    /// The text is empty.
    Empty,
    /// A character outside the grammar, or no digit in the mantissa.
    Malformed,
    /// A second decimal point.
    SecondPoint,
    /// An exponent marker with no digits after it.
    Exponent,
    /// A value in range to be computed with more than 28 significant digits.
    TooManyDigits,
    /// The rounded value does not fit a Q32.32 raw `i64`.
    OutOfRange,
}

impl fmt::Display for DecimalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "empty number",
            Self::Malformed => "malformed number",
            Self::SecondPoint => "number with a second decimal point",
            Self::Exponent => "number with an exponent marker but no exponent digits",
            Self::TooManyDigits => "number with more than 28 significant digits",
            Self::OutOfRange => "number outside the Q32.32 range",
        })
    }
}

impl std::error::Error for DecimalError {}

/// Converts a decimal to Q32.32 by `lumen-ascii-ply-q32.32/1`. See the
/// [module documentation](self).
///
/// # Errors
///
/// A [`DecimalError`] for text outside the grammar, a value out of range, or a value in range
/// with more than 28 significant digits.
pub fn decimal_to_q32_32(text: &str) -> Result<FixedQ32_32, DecimalError> {
    let decimal = Decimal::parse(text)?;
    let magnitude = decimal.raw_magnitude(text.as_bytes())?;
    let raw = if decimal.negative {
        // 2^63 is the magnitude of i64::MIN.
        0_i64.checked_sub_unsigned(u64::try_from(magnitude).map_err(|_| DecimalError::OutOfRange)?)
    } else {
        i64::try_from(magnitude).ok()
    };
    raw.map(FixedQ32_32::from_raw).ok_or(DecimalError::OutOfRange)
}

/// A decimal's shape, found without converting it: `(-1)^negative * D * 10^exponent`, with `D`
/// the digits from index `first` to index `last` (counting digits only, not the point) of the
/// mantissa.
struct Decimal {
    negative: bool,
    /// The byte range of the mantissa in the text (sign and exponent excluded).
    mantissa: (usize, usize),
    /// The digit indices of the first and last nonzero digit, if any.
    nonzero: Option<(usize, usize)>,
    /// The power of ten of the last nonzero digit.
    exponent: i64,
}

impl Decimal {
    /// Checks the grammar and finds the significant digits and their power of ten.
    fn parse(text: &str) -> Result<Self, DecimalError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(DecimalError::Empty);
        }
        let (negative, start) = match bytes.first() {
            Some(b'-') => (true, 1),
            Some(b'+') => (false, 1),
            _ => (false, 0),
        };
        let marker = bytes.iter().position(|&b| b == b'e' || b == b'E');
        let end = marker.unwrap_or(bytes.len());
        let body = bytes.get(start..end).ok_or(DecimalError::Malformed)?;
        let shape = scan_mantissa(body)?;
        let exponent = match marker {
            None => 0,
            Some(at) => parse_exponent(bytes.get(at.saturating_add(1)..).unwrap_or_default())?,
        };
        let nonzero = shape.nonzero;
        let last_power = match nonzero {
            // The last nonzero digit, at digit index `last`, has place value
            // 10^(digits before the point - 1 - last).
            Some((_, last)) => signed(shape.before_point) - 1 - signed(last),
            None => 0,
        };
        let exponent = exponent.saturating_add(last_power);
        Ok(Self { negative, mantissa: (start, end), nonzero, exponent })
    }

    /// The magnitude of the raw value, rounded half to even, by the decision in the
    /// [module documentation](self).
    fn raw_magnitude(&self, text: &[u8]) -> Result<u128, DecimalError> {
        let Some((first, last)) = self.nonzero else { return Ok(0) };
        let significant = last - first + 1;
        let order = self.exponent.saturating_add(signed(significant));
        if order >= 11 {
            return Err(DecimalError::OutOfRange);
        }
        if order <= -10 {
            return Ok(0);
        }
        if significant > MAX_SIGNIFICANT_DIGITS {
            return Err(DecimalError::TooManyDigits);
        }
        let body = text.get(self.mantissa.0..self.mantissa.1).unwrap_or_default();
        let digits = significant_value(body, first, last);
        // digits < 10^28 < 2^94, so the shift fits; -9 <= order <= 10 bounds the exponent.
        let scaled = digits << 32;
        let power =
            u32::try_from(self.exponent.unsigned_abs()).map_err(|_| DecimalError::OutOfRange)?;
        let ten_power = 10_u128.checked_pow(power).ok_or(DecimalError::OutOfRange)?;
        if self.exponent >= 0 {
            scaled.checked_mul(ten_power).ok_or(DecimalError::OutOfRange)
        } else {
            Ok(round_half_even(scaled, ten_power))
        }
    }
}

/// A count as an `i64`, saturating (counts here are bounded by the text's length).
fn signed(count: usize) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}

/// What [`scan_mantissa`] finds.
struct MantissaShape {
    before_point: usize,
    nonzero: Option<(usize, usize)>,
}

/// Checks `digits [. digits]` with at least one digit, and finds the digit indices of the first
/// and last nonzero digit and the number of digits before the point.
fn scan_mantissa(body: &[u8]) -> Result<MantissaShape, DecimalError> {
    let (mut digits, mut before_point) = (0_usize, None);
    let mut nonzero: Option<(usize, usize)> = None;
    for &b in body {
        match b {
            b'0'..=b'9' => {
                if b != b'0' {
                    nonzero = Some(nonzero.map_or((digits, digits), |(first, _)| (first, digits)));
                }
                digits = digits.saturating_add(1);
            }
            b'.' if before_point.is_some() => return Err(DecimalError::SecondPoint),
            b'.' => before_point = Some(digits),
            _ => return Err(DecimalError::Malformed),
        }
    }
    if digits == 0 {
        return Err(DecimalError::Malformed);
    }
    Ok(MantissaShape { before_point: before_point.unwrap_or(digits), nonzero })
}

/// The integer spelled by the digits with indices `first..=last` of `body` (the point skipped).
fn significant_value(body: &[u8], first: usize, last: usize) -> u128 {
    body.iter()
        .filter(|b| b.is_ascii_digit())
        .enumerate()
        .filter(|&(index, _)| (first..=last).contains(&index))
        .fold(0_u128, |value, (_, &b)| value * 10 + u128::from(b - b'0'))
}

/// `numerator / denominator` rounded to the nearest integer, ties to even.
const fn round_half_even(numerator: u128, denominator: u128) -> u128 {
    let (quotient, remainder) = (numerator / denominator, numerator % denominator);
    // remainder < denominator <= 10^37 < 2^127, so doubling it does not overflow.
    let twice = remainder * 2;
    if twice > denominator || (twice == denominator && quotient % 2 == 1) {
        quotient + 1
    } else {
        quotient
    }
}

/// An exponent: an optional sign and at least one digit, saturating at
/// `EXPONENT_SATURATION`.
fn parse_exponent(text: &[u8]) -> Result<i64, DecimalError> {
    let (negative, digits) = match text.split_first() {
        Some((b'-', rest)) => (true, rest),
        Some((b'+', rest)) => (false, rest),
        _ => (false, text),
    };
    if digits.is_empty() {
        return Err(DecimalError::Exponent);
    }
    let mut value = 0_i64;
    for &b in digits {
        if !b.is_ascii_digit() {
            return Err(DecimalError::Malformed);
        }
        value =
            value.saturating_mul(10).saturating_add(i64::from(b - b'0')).min(EXPONENT_SATURATION);
    }
    Ok(if negative { -value } else { value })
}
