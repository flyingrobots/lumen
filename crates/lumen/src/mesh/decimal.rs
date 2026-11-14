//! Exact conversion of a decimal number to Q32.32, `lumen-ascii-ply-q32.32/1`.
//!
//! A decimal `[-]digits[.digits][(e|E)[+|-]digits]` is read, with integers only, as a sign, an
//! integer mantissa `m` (every digit, the decimal point removed) and a power of ten `e` (the
//! exponent minus the number of digits after the point). Its Q32.32 raw value is the exact
//! rational `m * 2^32 * 10^e`, rounded to the nearest integer with ties to even, then given the
//! sign. No floating point is involved, so every decimal maps to one raw value on every machine.

#[cfg(test)]
mod tests;

use core::fmt;

use bunny_num::FixedQ32_32;

/// The most significant digits a decimal may have: `10^28 * 2^32 < 2^127`, so the product
/// fits in `u128`.
const MAX_DIGITS: usize = 28;

/// The most digits an exponent may have.
const MAX_EXPONENT_DIGITS: usize = 4;

/// Why a decimal number was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecimalError {
    /// The text is empty.
    Empty,
    /// A character other than a digit, one leading `-`, one `.` and one exponent, or no digit
    /// before the exponent.
    Malformed,
    /// A second decimal point.
    SecondPoint,
    /// More than 28 significant digits.
    TooManyDigits,
    /// An exponent with no digits or more than four.
    Exponent,
    /// The value's magnitude is not below `2^31` (it does not fit a Q32.32 raw `i64`).
    OutOfRange,
}

impl fmt::Display for DecimalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "empty number",
            Self::Malformed => "malformed number",
            Self::SecondPoint => "number with a second decimal point",
            Self::TooManyDigits => "number with more than 28 significant digits",
            Self::Exponent => "number with a missing or overlong exponent",
            Self::OutOfRange => "number outside the Q32.32 range",
        })
    }
}

impl std::error::Error for DecimalError {}

/// Converts a decimal to Q32.32 by `lumen-ascii-ply-q32.32/1`: the exact value times `2^32`,
/// rounded to the nearest integer, ties to even. See the [module documentation](self).
///
/// # Errors
///
/// A [`DecimalError`] for text outside the grammar, more than 28 significant digits, an
/// exponent with more than four digits, or a value whose raw form does not fit an `i64`.
pub fn decimal_to_q32_32(text: &str) -> Result<FixedQ32_32, DecimalError> {
    let decimal = Decimal::parse(text)?;
    let magnitude = decimal.raw_magnitude()?;
    let raw = i64::try_from(magnitude).map_err(|_| DecimalError::OutOfRange)?;
    Ok(FixedQ32_32::from_raw(if decimal.negative { -raw } else { raw }))
}

/// A decimal read exactly: `(-1)^negative * mantissa * 10^power`.
struct Decimal {
    negative: bool,
    mantissa: u128,
    power: i64,
}

impl Decimal {
    /// Reads `[-]digits[.digits][(e|E)[+|-]digits]` with integers only.
    fn parse(text: &str) -> Result<Self, DecimalError> {
        if text.is_empty() {
            return Err(DecimalError::Empty);
        }
        let bytes = text.as_bytes();
        let (negative, rest) = match bytes.split_first() {
            Some((b'-', rest)) => (true, rest),
            _ => (false, bytes),
        };
        let (body, exponent) = match rest.iter().position(|&b| b == b'e' || b == b'E') {
            Some(at) => {
                let (body, marker_and_exponent) = rest.split_at(at);
                (body, marker_and_exponent.get(1..))
            }
            None => (rest, None),
        };
        let (mantissa, after_point) = parse_digits(body)?;
        let exponent = exponent.map_or(Ok(0), parse_exponent)?;
        Ok(Self { negative, mantissa, power: exponent - after_point })
    }

    /// `mantissa * 2^32 * 10^power`, rounded to the nearest integer, ties to even.
    fn raw_magnitude(&self) -> Result<u128, DecimalError> {
        if self.mantissa == 0 {
            return Ok(0);
        }
        // The mantissa is below 10^28 < 2^94, so shifting it by 32 does not overflow.
        let scaled = self.mantissa << 32;
        if self.power >= 0 {
            return u32::try_from(self.power)
                .ok()
                .and_then(|p| 10_u128.checked_pow(p))
                .and_then(|scale| scaled.checked_mul(scale))
                .ok_or(DecimalError::OutOfRange);
        }
        match u32::try_from(-self.power).ok().and_then(|k| 10_u128.checked_pow(k)) {
            // 10^-power does not fit u128 (power below -38): the value is below
            // 10^28 * 10^-39 = 1 / 10^11, so its raw form is below 2^32 / 10^11 < 1 / 2, which
            // rounds to zero.
            None => Ok(0),
            Some(denominator) => Ok(round_half_even(scaled, denominator)),
        }
    }
}

/// The digits of a decimal without its sign and exponent, with at most one `.`: the integer
/// they spell with the point removed, and the number of digits after the point.
fn parse_digits(body: &[u8]) -> Result<(u128, i64), DecimalError> {
    let (mut mantissa, mut digits, mut significant) = (0_u128, 0_usize, 0_usize);
    let mut after_point: Option<i64> = None;
    for &b in body {
        match (b, after_point) {
            (b'0'..=b'9', _) => {
                digits += 1;
                if mantissa != 0 || b != b'0' {
                    significant += 1;
                }
                if significant > MAX_DIGITS {
                    return Err(DecimalError::TooManyDigits);
                }
                mantissa = mantissa
                    .checked_mul(10)
                    .and_then(|m| m.checked_add(u128::from(b - b'0')))
                    .ok_or(DecimalError::TooManyDigits)?;
                after_point = after_point.map(|n| n + 1);
            }
            (b'.', Some(_)) => return Err(DecimalError::SecondPoint),
            (b'.', None) => after_point = Some(0),
            _ => return Err(DecimalError::Malformed),
        }
    }
    if digits == 0 {
        return Err(DecimalError::Malformed);
    }
    Ok((mantissa, after_point.unwrap_or(0)))
}

/// `numerator / denominator` rounded to the nearest integer, ties to even.
const fn round_half_even(numerator: u128, denominator: u128) -> u128 {
    let (quotient, remainder) = (numerator / denominator, numerator % denominator);
    // remainder < denominator <= 10^38 < 2^127, so doubling it does not overflow.
    let twice = remainder * 2;
    if twice > denominator || (twice == denominator && quotient % 2 == 1) {
        quotient + 1
    } else {
        quotient
    }
}

/// An exponent: an optional sign and one to four digits.
fn parse_exponent(text: &[u8]) -> Result<i64, DecimalError> {
    let (negative, digits) = match text.split_first() {
        Some((b'-', rest)) => (true, rest),
        Some((b'+', rest)) => (false, rest),
        _ => (false, text),
    };
    if digits.is_empty() || digits.len() > MAX_EXPONENT_DIGITS {
        return Err(DecimalError::Exponent);
    }
    let mut value = 0_i64;
    for &b in digits {
        if !b.is_ascii_digit() {
            return Err(DecimalError::Malformed);
        }
        value = value * 10 + i64::from(b - b'0');
    }
    Ok(if negative { -value } else { value })
}
