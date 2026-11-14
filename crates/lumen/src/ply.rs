//! An interim, strict reader for one ASCII PLY profile, with exact decimal-to-Q32.32 conversion.
//!
//! **Interim.** Lumen's rule is that numerical code belongs in Bunny, but Bunny's PLY reader
//! (`bunny-codec`, `parse_binary_ply`) reads only binary little-endian files whose vertices are
//! exactly `float x`, `float y`, `float z`, so it cannot read the Stanford bunny that Geordi
//! carries (an ASCII file with two more vertex columns). An ASCII reader in Bunny is tracked as
//! flyingrobots/bunny#247; this module is to be replaced by it. It is kept small and reads only
//! the profile of that one file.
//!
//! **The profile.** The header must be exactly, line by line (with any number of `comment` lines
//! after the `format` line):
//!
//! ```text
//! ply
//! format ascii 1.0
//! element vertex <count>
//! property float x
//! property float y
//! property float z
//! property float confidence
//! property float intensity
//! element face <count>
//! property list uchar int vertex_indices
//! end_header
//! ```
//!
//! followed by one line per vertex (five numbers separated by single spaces), one line per face
//! (`3 a b c`, every index below the vertex count) and nothing after the last face's line
//! ending. Lines end in `\n` only. Anything else is a typed [`PlyError`]. The `confidence` and
//! `intensity` columns must be well-formed numbers but are not used.
//!
//! **The conversion, `lumen-ascii-ply-q32.32/1`** ([`ASCII_PLY_ID`], [`decimal_to_q32_32`]): a
//! decimal `[-]digits[.digits][(e|E)[+|-]digits]` is read, with integers only, as a sign, an
//! integer mantissa `m` (every digit, the decimal point removed) and a power of ten `e` (the
//! exponent minus the number of digits after the point). Its Q32.32 raw value is the exact
//! rational `m * 2^32 * 10^e`, rounded to the nearest integer with ties to even, then given the
//! sign. No floating point is involved, so every decimal maps to one raw value on every machine.

use core::fmt;

use bunny_linalg::FixedVec3;
use bunny_num::FixedQ32_32;

/// The stable name and version of the conversion rule of [`decimal_to_q32_32`] and
/// [`parse_ascii_ply`]. Interim: to be replaced by Bunny's ASCII PLY reader
/// (flyingrobots/bunny#247).
pub const ASCII_PLY_ID: &str = "lumen-ascii-ply-q32.32/1";

/// The Stanford bunny, `assets/stanford-bunny/bun_zipper_res3.ply`, embedded byte for byte.
///
/// Stanford Computer Graphics Laboratory, Stanford 3D Scanning Repository; lumen's copy is
/// Geordi's checked-in asset (see `assets/stanford-bunny/README.md`).
pub const STANFORD_BUNNY_PLY: &str =
    include_str!("../../../assets/stanford-bunny/bun_zipper_res3.ply");

/// The SHA-256 of [`STANFORD_BUNNY_PLY`], the same as Geordi's asset hash.
pub const STANFORD_BUNNY_SHA256: &str =
    "975e7f9b160b4ea15b0e225e21b10828ebcf678df020d2f6a46aa408fdcf5cd6";

/// The most significant digits a decimal may have: `10^28 * 2^32 < 2^127`, so the product
/// fits in `u128`.
const MAX_DIGITS: usize = 28;

/// The most digits an exponent may have.
const MAX_EXPONENT_DIGITS: usize = 4;

/// The most vertices or faces a file may declare.
const MAX_COUNT: usize = 1 << 24;

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

/// Why a PLY text was refused. Line numbers count from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlyError {
    /// The first line is not `ply`.
    NotPly,
    /// The `format` line is not `format ascii 1.0`.
    UnsupportedFormat {
        /// The line number.
        line: usize,
    },
    /// A header line is not the one the profile requires there.
    UnexpectedHeader {
        /// The line number.
        line: usize,
        /// The line the profile expects (`<count>` stands for a count).
        expected: &'static str,
    },
    /// An element count is not a decimal integer of at most 2^24.
    BadCount {
        /// The line number.
        line: usize,
    },
    /// The text ends before the header, a declared vertex or face line, or the last line
    /// ending.
    Truncated {
        /// The number of the first missing line.
        line: usize,
    },
    /// A vertex line does not hold exactly five numbers separated by single spaces.
    VertexFields {
        /// The line number.
        line: usize,
    },
    /// A number on a vertex line was refused.
    Number {
        /// The line number.
        line: usize,
        /// Why.
        error: DecimalError,
    },
    /// A face line is not a list of three indices.
    FaceNotTriangle {
        /// The face's index, from 0.
        face: usize,
    },
    /// A face index is not a decimal integer.
    BadIndex {
        /// The line number.
        line: usize,
    },
    /// A face index is negative or not below the vertex count.
    IndexOutOfRange {
        /// The face's index, from 0.
        face: usize,
    },
    /// Something follows the last face's line ending.
    TrailingData {
        /// The line number.
        line: usize,
    },
}

impl fmt::Display for PlyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotPly => f.write_str("not a PLY file"),
            Self::UnsupportedFormat { line } => {
                write!(f, "line {line}: the format must be `format ascii 1.0`")
            }
            Self::UnexpectedHeader { line, expected } => {
                write!(f, "line {line}: expected `{expected}`")
            }
            Self::BadCount { line } => write!(f, "line {line}: bad element count"),
            Self::Truncated { line } => write!(f, "the file ends before line {line}"),
            Self::VertexFields { line } => write!(f, "line {line}: a vertex needs five numbers"),
            Self::Number { line, error } => write!(f, "line {line}: {error}"),
            Self::FaceNotTriangle { face } => write!(f, "face {face} is not a triangle"),
            Self::BadIndex { line } => write!(f, "line {line}: bad vertex index"),
            Self::IndexOutOfRange { face } => write!(f, "face {face} names a missing vertex"),
            Self::TrailingData { line } => write!(f, "line {line}: data after the last face"),
        }
    }
}

impl std::error::Error for PlyError {}

/// A triangle mesh read from a PLY text: vertex positions in Q32.32 and faces as indices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlyMesh {
    /// The vertex positions, in file order.
    pub vertices: Vec<FixedVec3>,
    /// The faces, in file order, each three indices into `vertices` in file order.
    pub faces: Vec<[u32; 3]>,
}

impl PlyMesh {
    /// The smallest and largest coordinate on each axis, or `None` for no vertices.
    #[must_use]
    pub fn bounds(&self) -> Option<(FixedVec3, FixedVec3)> {
        let first = *self.vertices.first()?;
        Some(self.vertices.iter().fold((first, first), |(lo, hi), v| {
            (
                FixedVec3::new(lo.x.min(v.x), lo.y.min(v.y), lo.z.min(v.z)),
                FixedVec3::new(hi.x.max(v.x), hi.y.max(v.y), hi.z.max(v.z)),
            )
        }))
    }
}

/// The header lines between the vertex count and the face count.
const VERTEX_PROPERTIES: [&str; 5] = [
    "property float x",
    "property float y",
    "property float z",
    "property float confidence",
    "property float intensity",
];

/// Lines of a text, numbered from 1, each without its `\n`.
struct Lines<'a> {
    rest: Option<&'a str>,
    number: usize,
}

impl<'a> Lines<'a> {
    /// The next line ended by `\n`, or [`PlyError::Truncated`].
    fn next(&mut self) -> Result<(usize, &'a str), PlyError> {
        self.number += 1;
        let line = self.number;
        let rest = self.rest.ok_or(PlyError::Truncated { line })?;
        let (text, after) = rest.split_once('\n').ok_or(PlyError::Truncated { line })?;
        self.rest = Some(after);
        Ok((line, text))
    }
}

/// Reads a PLY text of the profile in the [module documentation](self), converting every
/// coordinate with [`decimal_to_q32_32`].
///
/// # Errors
///
/// The first [`PlyError`] in file order.
pub fn parse_ascii_ply(text: &str) -> Result<PlyMesh, PlyError> {
    let mut lines = Lines { rest: Some(text), number: 0 };
    let (vertex_count, face_count) = parse_header(&mut lines)?;
    // Grown line by line rather than reserved from the declared counts, so a header cannot
    // make the reader allocate more than the text holds.
    let mut vertices = Vec::new();
    for _ in 0..vertex_count {
        let (line, text) = lines.next()?;
        vertices.push(parse_vertex(line, text)?);
    }
    let mut faces = Vec::new();
    for face in 0..face_count {
        let (line, text) = lines.next()?;
        faces.push(parse_face(face, line, text, vertex_count)?);
    }
    match lines.rest {
        Some("") => Ok(PlyMesh { vertices, faces }),
        _ => Err(PlyError::TrailingData { line: lines.number + 1 }),
    }
}

/// The header, line by line: the vertex count and the face count.
fn parse_header(lines: &mut Lines<'_>) -> Result<(usize, usize), PlyError> {
    let (_, first) = lines.next().map_err(|_| PlyError::NotPly)?;
    if first != "ply" {
        return Err(PlyError::NotPly);
    }
    let (line, format) = lines.next()?;
    if format != "format ascii 1.0" {
        return Err(if format.starts_with("format ") {
            PlyError::UnsupportedFormat { line }
        } else {
            PlyError::UnexpectedHeader { line, expected: "format ascii 1.0" }
        });
    }
    let (mut line, mut header) = lines.next()?;
    while header.starts_with("comment ") || header == "comment" {
        (line, header) = lines.next()?;
    }
    let vertex_count = count(line, header, "element vertex ")?;
    for expected in VERTEX_PROPERTIES {
        expect(lines, expected)?;
    }
    let (line, header) = lines.next()?;
    let face_count = count(line, header, "element face ")?;
    expect(lines, "property list uchar int vertex_indices")?;
    expect(lines, "end_header")?;
    Ok((vertex_count, face_count))
}

/// A vertex line: five numbers separated by single spaces, of which the first three are the
/// position.
fn parse_vertex(line: usize, text: &str) -> Result<FixedVec3, PlyError> {
    let mut fields = text.split(' ');
    let mut coordinates = [FixedQ32_32::ZERO; 3];
    for column in 0..VERTEX_PROPERTIES.len() {
        let field = fields.next().filter(|f| !f.is_empty());
        let field = field.ok_or(PlyError::VertexFields { line })?;
        let value = decimal_to_q32_32(field).map_err(|error| PlyError::Number { line, error })?;
        if let Some(slot) = coordinates.get_mut(column) {
            *slot = value;
        }
    }
    if fields.next().is_some() {
        return Err(PlyError::VertexFields { line });
    }
    let [x, y, z] = coordinates;
    Ok(FixedVec3::new(x, y, z))
}

/// A face line: `3` and three indices below `vertex_count`, separated by single spaces.
fn parse_face(
    face: usize,
    line: usize,
    text: &str,
    vertex_count: usize,
) -> Result<[u32; 3], PlyError> {
    let mut fields = text.split(' ');
    if fields.next() != Some("3") {
        return Err(PlyError::FaceNotTriangle { face });
    }
    let mut indices = [0_u32; 3];
    for slot in &mut indices {
        let field = fields.next().ok_or(PlyError::FaceNotTriangle { face })?;
        let index = parse_index(field).ok_or(PlyError::BadIndex { line })?;
        *slot = u32::try_from(index)
            .ok()
            .filter(|&i| usize::try_from(i).is_ok_and(|i| i < vertex_count))
            .ok_or(PlyError::IndexOutOfRange { face })?;
    }
    if fields.next().is_some() {
        return Err(PlyError::FaceNotTriangle { face });
    }
    Ok(indices)
}

/// The next header line, which must be `expected`.
fn expect(lines: &mut Lines<'_>, expected: &'static str) -> Result<(), PlyError> {
    let (line, text) = lines.next()?;
    if text == expected {
        Ok(())
    } else {
        Err(PlyError::UnexpectedHeader { line, expected })
    }
}

/// The count of an `element` line beginning with `prefix`.
fn count(line: usize, text: &str, prefix: &'static str) -> Result<usize, PlyError> {
    let expected =
        if prefix == "element vertex " { "element vertex <count>" } else { "element face <count>" };
    let digits = text.strip_prefix(prefix).ok_or(PlyError::UnexpectedHeader { line, expected })?;
    let bad = PlyError::BadCount { line };
    if digits.is_empty() || digits.len() > 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad);
    }
    let value: usize = digits.parse().map_err(|_| bad)?;
    if value > MAX_COUNT {
        return Err(bad);
    }
    Ok(value)
}

/// A face index: an optional `-` and one to ten digits.
fn parse_index(text: &str) -> Option<i64> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    if digits.is_empty() || digits.len() > 10 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(text: &str) -> i64 {
        decimal_to_q32_32(text).expect("a decimal").raw()
    }

    /// Exact values derived by hand. For `m / 10^k`, the raw value is `m * 2^32 / 10^k`
    /// rounded to nearest, ties to even; `2^32 = 4294967296`.
    #[test]
    fn decimals_convert_exactly_with_ties_to_even() {
        assert_eq!(raw("0"), 0);
        assert_eq!(raw("-0"), 0);
        assert_eq!(raw("1"), 1 << 32);
        assert_eq!(raw("1.5"), 3 << 31);
        assert_eq!(raw("-2.25"), -(9 << 30));
        assert_eq!(raw("0.25e1"), 5 << 31);
        assert_eq!(raw("25E-2"), 1 << 30);
        assert_eq!(raw(".5"), 1 << 31);
        assert_eq!(raw("5."), 5 << 32);
        // 0.1: 4294967296 / 10 = 429496729.6, rounds up.
        assert_eq!(raw("0.1"), 429_496_730);
        // 2^-33 = 5^33 / 10^33 = 1.16415321826934814453125e-10, exactly half a raw unit: a tie,
        // to even (0). Three halves round to 2, five halves to 2, and the sign is symmetric.
        assert_eq!(raw("1.16415321826934814453125e-10"), 0);
        assert_eq!(raw("3.49245965480804443359375e-10"), 2);
        assert_eq!(raw("5.82076609134674072265625e-10"), 2);
        assert_eq!(raw("-3.49245965480804443359375e-10"), -2);
        // Just above and just below the tie.
        assert_eq!(raw("1.16415321826934814453126e-10"), 1);
        assert_eq!(raw("1.16415321826934814453124e-10"), 0);
        // Far below half a raw unit, with a denominator too large for u128.
        assert_eq!(raw("1e-40"), 0);
        // The largest value below 2^31 and the first one beyond.
        assert_eq!(raw("2147483647"), 2_147_483_647 << 32);
        assert_eq!(decimal_to_q32_32("2147483648"), Err(DecimalError::OutOfRange));
        assert_eq!(decimal_to_q32_32("1e30"), Err(DecimalError::OutOfRange));
    }

    /// The bunny's numbers, derived by hand (`q` and `r` are the quotient and remainder of
    /// `m * 2^32` by `10^k`).
    #[test]
    fn bunny_coordinates_convert_to_their_hand_derived_raw_values() {
        // The first vertex, (-0.0369122, 0.127512, 0.00276757):
        // 369122 * 2^32 = 1585366918234112 = 158536691 * 10^7 + 8234112; 2r > 10^7, round up.
        assert_eq!(raw("-0.0369122"), -158_536_692);
        // 127512 * 2^32 = 547659869847552 = 547659869 * 10^6 + 847552; round up.
        assert_eq!(raw("0.127512"), 547_659_870);
        // 276757 * 2^32 = 1188662263939072 = 11886622 * 10^8 + 63939072; round up.
        assert_eq!(raw("0.00276757"), 11_886_623);
        // The bounds of `bunny.mesh.json`:
        // 943643 * 2^32 = 4052915824099328 = 405291582 * 10^7 + 4099328; 2r < 10^7, round down.
        assert_eq!(raw("-0.0943643"), -405_291_582);
        // 334143 * 2^32 = 1435133257187328 = 143513325 * 10^7 + 7187328; round up.
        assert_eq!(raw("0.0334143"), 143_513_326);
        // 616721 * 2^32 = 2648796525756416 = 264879652 * 10^7 + 5756416; round up.
        assert_eq!(raw("-0.0616721"), -264_879_653);
        // 609346 * 2^32 = 2617121141948416 = 261712114 * 10^7 + 1948416; round down.
        assert_eq!(raw("0.0609346"), 261_712_114);
        // 184813 * 2^32 = 793765790875648 = 793765790 * 10^6 + 875648; round up.
        assert_eq!(raw("0.184813"), 793_765_791);
        // 584651 * 2^32 = 2511056924573696 = 251105692 * 10^7 + 4573696; round down.
        assert_eq!(raw("0.0584651"), 251_105_692);
        // The file's one exponent, 8.20736e-05 = 820736 / 10^10:
        // 820736 * 2^32 = 3525034278649856 = 352503 * 10^10 + 4278649856; round down.
        assert_eq!(raw("8.20736e-05"), 352_503);
    }

    /// A xorshift generator, so the generated cases are the same on every run.
    fn xorshift(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    /// Over generated decimals `m / 10^k` (up to 12 digits, `k` from 3 to 15, so below 10^9 and
    /// inside the Q32.32 range, either sign), the raw
    /// value `q` satisfies the definition of rounding to nearest, checked by multiplication
    /// rather than division: `2 * |m * 2^32 - q * 10^k| <= 10^k`, and on equality `q` is even.
    /// Over generated exact ties `(2n + 1) / 2^33 = (2n + 1) * 5^33 / 10^33`, the raw value is
    /// the even neighbour of `n + 1/2`.
    #[test]
    fn generated_decimals_round_to_nearest_with_ties_to_even() {
        const SEED: u64 = 0x5eed_f1ce_b00c_0001;
        let mut state = SEED;
        for case in 0..20_000 {
            let mantissa = u128::from(xorshift(&mut state) % 1_000_000_000_000);
            let k = 3 + u32::try_from(xorshift(&mut state) % 13).expect("small");
            let negative = xorshift(&mut state) % 2 == 1;
            let digits =
                format!("{mantissa:0>width$}", width = usize::try_from(k).expect("small") + 1);
            let (int, frac) = digits.split_at(digits.len() - usize::try_from(k).expect("small"));
            let text = format!("{}{int}.{frac}", if negative { "-" } else { "" });
            let got = raw(&text);
            let q = u128::from(got.unsigned_abs());
            assert!(got == 0 || (got < 0) == negative, "seed {SEED:#x} case {case}: {text}");
            let ten_k = 10_u128.pow(k);
            let error = (mantissa << 32).abs_diff(q * ten_k);
            assert!(2 * error <= ten_k, "seed {SEED:#x} case {case}: {text} gave {got}");
            if 2 * error == ten_k {
                assert_eq!(q % 2, 0, "seed {SEED:#x} case {case}: {text} gave {got}");
            }
        }
        // Ties are rare among random decimals; the constructed ones below cover them.
        let five_33 = 5_u128.pow(33);
        for case in 0..2_000 {
            let n = u128::from(xorshift(&mut state) % 42_000);
            let text = format!("{}e-33", (2 * n + 1) * five_33);
            let want = i64::try_from(if n % 2 == 0 { n } else { n + 1 }).expect("small");
            assert_eq!(raw(&text), want, "seed {SEED:#x} tie case {case}: {text}");
            assert_eq!(raw(&format!("-{text}")), -want, "seed {SEED:#x} tie case {case}");
        }
    }

    #[test]
    fn malformed_decimals_are_refused() {
        for (text, error) in [
            ("", DecimalError::Empty),
            ("-", DecimalError::Malformed),
            (".", DecimalError::Malformed),
            ("+1", DecimalError::Malformed),
            ("--1", DecimalError::Malformed),
            ("1-", DecimalError::Malformed),
            ("0x10", DecimalError::Malformed),
            ("1.0\r", DecimalError::Malformed),
            (" 1", DecimalError::Malformed),
            ("nan", DecimalError::Malformed),
            ("e5", DecimalError::Malformed),
            ("1.2.3", DecimalError::SecondPoint),
            ("-0.03.5", DecimalError::SecondPoint),
            ("1e", DecimalError::Exponent),
            ("1e+", DecimalError::Exponent),
            ("1e12345", DecimalError::Exponent),
            ("1e1.5", DecimalError::Malformed),
            ("1e-1e", DecimalError::Malformed),
            ("12345678901234567890123456789", DecimalError::TooManyDigits),
        ] {
            assert_eq!(decimal_to_q32_32(text), Err(error), "{text:?}");
        }
        // Leading zeros are not significant digits.
        assert_eq!(raw("0000000000000000000000000000001"), 1 << 32);
    }

    /// A valid two-triangle file, from which each refused case below differs by one change.
    const SMALL: &str = "ply\nformat ascii 1.0\ncomment a test\nelement vertex 4\n\
        property float x\nproperty float y\nproperty float z\nproperty float confidence\n\
        property float intensity\nelement face 2\nproperty list uchar int vertex_indices\n\
        end_header\n0 0 0 1 0.5\n1 0 0 1 0.5\n0 1 0 1 0.5\n0 0 -1.5 1 0.5\n3 0 1 2\n3 0 2 3\n";

    fn refused(from: &str, to: &str) -> PlyError {
        assert!(SMALL.contains(from), "{from:?}");
        parse_ascii_ply(&SMALL.replacen(from, to, 1)).expect_err("refused")
    }

    #[test]
    fn a_small_file_reads_exactly() {
        let mesh = parse_ascii_ply(SMALL).expect("reads");
        let one = FixedQ32_32::ONE;
        let zero = FixedQ32_32::ZERO;
        assert_eq!(
            mesh.vertices,
            vec![
                FixedVec3::new(zero, zero, zero),
                FixedVec3::new(one, zero, zero),
                FixedVec3::new(zero, one, zero),
                FixedVec3::new(zero, zero, FixedQ32_32::from_raw(-(3 << 31))),
            ]
        );
        assert_eq!(mesh.faces, vec![[0, 1, 2], [0, 2, 3]]);
        let (lo, hi) = mesh.bounds().expect("vertices");
        assert_eq!(lo, FixedVec3::new(zero, zero, FixedQ32_32::from_raw(-(3 << 31))));
        assert_eq!(hi, FixedVec3::new(one, one, zero));
    }

    #[test]
    fn files_outside_the_profile_are_refused() {
        assert_eq!(refused("ply\n", "plx\n"), PlyError::NotPly);
        assert_eq!(parse_ascii_ply(""), Err(PlyError::NotPly));
        assert_eq!(
            refused("format ascii 1.0", "format binary_little_endian 1.0"),
            PlyError::UnsupportedFormat { line: 2 }
        );
        // A missing property: the next line is not the one the profile requires.
        assert_eq!(
            refused("property float confidence\n", ""),
            PlyError::UnexpectedHeader { line: 8, expected: "property float confidence" }
        );
        assert_eq!(
            refused("property float y", "property double y"),
            PlyError::UnexpectedHeader { line: 6, expected: "property float y" }
        );
        assert_eq!(
            refused("element face 2", "element triangle 2"),
            PlyError::UnexpectedHeader { line: 10, expected: "element face <count>" }
        );
        assert_eq!(
            refused("element vertex 4", "element vertex -4"),
            PlyError::BadCount { line: 4 }
        );
        assert_eq!(
            refused("end_header\n", "end_header \n"),
            PlyError::UnexpectedHeader { line: 12, expected: "end_header" }
        );
        // A face with four indices, and one with a count of 3 but four indices.
        assert_eq!(refused("3 0 2 3\n", "4 0 1 2 3\n"), PlyError::FaceNotTriangle { face: 1 });
        assert_eq!(refused("3 0 2 3\n", "3 0 1 2 3\n"), PlyError::FaceNotTriangle { face: 1 });
        assert_eq!(refused("3 0 2 3\n", "3 0 2\n"), PlyError::FaceNotTriangle { face: 1 });
        // An index out of range, negative or not a number.
        assert_eq!(refused("3 0 2 3\n", "3 0 2 4\n"), PlyError::IndexOutOfRange { face: 1 });
        assert_eq!(refused("3 0 1 2\n", "3 0 -1 2\n"), PlyError::IndexOutOfRange { face: 0 });
        assert_eq!(refused("3 0 1 2\n", "3 0 1.0 2\n"), PlyError::BadIndex { line: 17 });
        // A number with a second decimal point, on line 14.
        assert_eq!(
            refused("1 0 0 1 0.5", "1.0.0 0 0 1 0.5"),
            PlyError::Number { line: 14, error: DecimalError::SecondPoint }
        );
        // Wrong field counts and separators on a vertex line.
        assert_eq!(refused("1 0 0 1 0.5", "1 0 0 1"), PlyError::VertexFields { line: 14 });
        assert_eq!(refused("1 0 0 1 0.5", "1 0 0 1 0.5 7"), PlyError::VertexFields { line: 14 });
        assert_eq!(refused("1 0 0 1 0.5", "1  0 0 1 0.5"), PlyError::VertexFields { line: 14 });
        // A carriage return is not a line ending here.
        assert_eq!(
            refused("1 0 0 1 0.5\n", "1 0 0 1 0.5\r\n"),
            PlyError::Number { line: 14, error: DecimalError::Malformed }
        );
        // Data after the last face.
        assert_eq!(refused("3 0 2 3\n", "3 0 2 3\n\n"), PlyError::TrailingData { line: 19 });
    }

    #[test]
    fn a_truncated_file_is_refused() {
        // Cut inside the vertices, inside the faces, before the last line ending and inside
        // the header.
        let cut = |end: &str| {
            let at = SMALL.find(end).expect("present") + end.len();
            parse_ascii_ply(&SMALL[..at])
        };
        assert_eq!(cut("0 1 0 1 0.5\n"), Err(PlyError::Truncated { line: 16 }));
        assert_eq!(cut("3 0 1 2\n"), Err(PlyError::Truncated { line: 18 }));
        assert_eq!(cut("3 0 1 2\n3 0 2 3"), Err(PlyError::Truncated { line: 18 }));
        assert_eq!(cut("element face 2\n"), Err(PlyError::Truncated { line: 11 }));
    }

    #[test]
    fn the_stanford_bunny_reads_with_its_declared_counts() {
        let mesh = parse_ascii_ply(STANFORD_BUNNY_PLY).expect("the bunny reads");
        assert_eq!(mesh.vertices.len(), 1889);
        assert_eq!(mesh.faces.len(), 3851);
        let first = mesh.vertices[0];
        assert_eq!(
            [first.x.raw(), first.y.raw(), first.z.raw()],
            [-158_536_692, 547_659_870, 11_886_623]
        );
    }

    /// The manifest's bounds (`bunny.mesh.json` in Geordi), the same decimal strings as in the
    /// file, converted by the same routine, equal the converted mesh's bounds exactly.
    #[test]
    fn the_bunny_bounds_equal_the_manifest_bounds_exactly() {
        let manifest = |values: [&str; 3]| {
            let [x, y, z] = values.map(|v| decimal_to_q32_32(v).expect("a decimal"));
            FixedVec3::new(x, y, z)
        };
        let mesh = parse_ascii_ply(STANFORD_BUNNY_PLY).expect("the bunny reads");
        let (lo, hi) = mesh.bounds().expect("vertices");
        assert_eq!(lo, manifest(["-0.0943643", "0.0334143", "-0.0616721"]));
        assert_eq!(hi, manifest(["0.0609346", "0.184813", "0.0584651"]));
    }

    #[test]
    fn the_committed_asset_has_geordis_hash() {
        let digest = crate::sha256::digest(STANFORD_BUNNY_PLY.as_bytes());
        assert_eq!(crate::sha256::hex(&digest), STANFORD_BUNNY_SHA256);
        assert_eq!(
            STANFORD_BUNNY_SHA256,
            "975e7f9b160b4ea15b0e225e21b10828ebcf678df020d2f6a46aa408fdcf5cd6"
        );
    }
}
