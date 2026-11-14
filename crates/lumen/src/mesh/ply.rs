//! An interim, strict reader for one ASCII PLY profile.
//!
//! **Interim.** Lumen's rule is that numerical code belongs in Bunny, but Bunny's PLY reader
//! (`bunny-codec`, `parse_binary_ply`) reads only binary little-endian files whose vertices are
//! exactly `float x`, `float y`, `float z`, so it cannot read the Stanford bunny that Geordi
//! carries (an ASCII file with two more vertex columns). An ASCII reader in Bunny is tracked as
//! flyingrobots/bunny#247; this module is to be replaced by it. It is kept small and reads only
//! the profile of that one file, converting coordinates with [`decimal_to_q32_32`].
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

#[cfg(test)]
mod tests;

use core::fmt;

use bunny_linalg::FixedVec3;
use bunny_num::FixedQ32_32;

use super::decimal::{decimal_to_q32_32, DecimalError};

/// The most vertices a file may declare. The bunny has 1 889.
pub const MAX_VERTICES: usize = 1 << 20;

/// The most faces a file may declare. The bunny has 3 851.
pub const MAX_FACES: usize = 1 << 20;

/// The longest line, in bytes without its `\n`. The bunny's longest is 47 bytes.
pub const MAX_LINE_BYTES: usize = 256;

/// The most digits an element count or a face index may have.
const MAX_INTEGER_DIGITS: usize = 10;

/// Which element of the file a count or a limit is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Element {
    /// The vertices.
    Vertex,
    /// The faces.
    Face,
}

impl fmt::Display for Element {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Vertex => "vertex",
            Self::Face => "face",
        })
    }
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
    /// An element count is not one to ten decimal digits.
    BadCount {
        /// The line number.
        line: usize,
        /// The element counted.
        element: Element,
    },
    /// An element count is above its limit ([`MAX_VERTICES`], [`MAX_FACES`]).
    CountTooLarge {
        /// The line number.
        line: usize,
        /// The element counted.
        element: Element,
        /// The count declared.
        declared: u64,
        /// The limit.
        limit: usize,
    },
    /// A line is longer than [`MAX_LINE_BYTES`].
    LineTooLong {
        /// The line number.
        line: usize,
        /// The line's length in bytes, without its `\n`.
        length: usize,
        /// The limit.
        limit: usize,
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
        /// The fields found before the first empty one (two spaces in a row, or a space at
        /// either end), or in all; more than five means the line has extra fields.
        found: usize,
    },
    /// A number on a vertex line was refused.
    Number {
        /// The line number.
        line: usize,
        /// The column, from 0 (`x`, `y`, `z`, `confidence`, `intensity`).
        column: usize,
        /// Why.
        error: DecimalError,
    },
    /// A face line is not `3` followed by exactly three indices.
    FaceNotTriangle {
        /// The face's index, from 0.
        face: usize,
        /// The line number.
        line: usize,
    },
    /// A face index is not an optional `-` and one to ten decimal digits.
    BadIndex {
        /// The face's index, from 0.
        face: usize,
        /// The line number.
        line: usize,
    },
    /// A face index is negative or not below the vertex count.
    IndexOutOfRange {
        /// The face's index, from 0.
        face: usize,
        /// The line number.
        line: usize,
        /// The index found.
        index: i64,
        /// The number of vertices.
        vertex_count: usize,
    },
    /// Something follows the last face's line ending.
    TrailingData {
        /// The line number.
        line: usize,
    },
}

impl fmt::Display for PlyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::NotPly => f.write_str("not a PLY file"),
            Self::UnsupportedFormat { line } => {
                write!(f, "line {line}: the format must be `format ascii 1.0`")
            }
            Self::UnexpectedHeader { line, expected } => {
                write!(f, "line {line}: expected `{expected}`")
            }
            Self::BadCount { line, element } => write!(f, "line {line}: bad {element} count"),
            Self::CountTooLarge { line, element, declared, limit } => {
                write!(f, "line {line}: {declared} {element} elements, more than {limit}")
            }
            Self::LineTooLong { line, length, limit } => {
                write!(f, "line {line}: {length} bytes, more than {limit}")
            }
            Self::Truncated { line } => write!(f, "the file ends before line {line}"),
            Self::VertexFields { line, found } => {
                write!(f, "line {line}: a vertex needs five numbers, found {found}")
            }
            Self::Number { line, column, error } => {
                write!(f, "line {line} column {column}: {error}")
            }
            Self::FaceNotTriangle { face, line } => {
                write!(f, "line {line}: face {face} is not a triangle")
            }
            Self::BadIndex { face, line } => write!(f, "line {line}: face {face} has a bad index"),
            Self::IndexOutOfRange { face, line, index, vertex_count } => write!(
                f,
                "line {line}: face {face} names vertex {index}, but there are {vertex_count}"
            ),
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
    rest: &'a str,
    number: usize,
}

impl<'a> Lines<'a> {
    /// The number the next line will have.
    const fn next_number(&self) -> usize {
        self.number.saturating_add(1)
    }

    /// The next line ended by `\n`, [`PlyError::Truncated`] if there is none, or
    /// [`PlyError::LineTooLong`].
    fn next(&mut self) -> Result<(usize, &'a str), PlyError> {
        let line = self.next_number();
        let (text, after) = self.rest.split_once('\n').ok_or(PlyError::Truncated { line })?;
        if text.len() > MAX_LINE_BYTES {
            return Err(PlyError::LineTooLong { line, length: text.len(), limit: MAX_LINE_BYTES });
        }
        self.number = line;
        self.rest = after;
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
    let mut lines = Lines { rest: text, number: 0 };
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
    if lines.rest.is_empty() {
        Ok(PlyMesh { vertices, faces })
    } else {
        Err(PlyError::TrailingData { line: lines.next_number() })
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
    let vertex_count = count(line, header, Element::Vertex)?;
    for expected in VERTEX_PROPERTIES {
        expect(lines, expected)?;
    }
    let (line, header) = lines.next()?;
    let face_count = count(line, header, Element::Face)?;
    expect(lines, "property list uchar int vertex_indices")?;
    expect(lines, "end_header")?;
    Ok((vertex_count, face_count))
}

/// A vertex line: five numbers separated by single spaces, of which the first three are the
/// position.
fn parse_vertex(line: usize, text: &str) -> Result<FixedVec3, PlyError> {
    let fields: Vec<&str> = text.split(' ').take(VERTEX_PROPERTIES.len() + 1).collect();
    let found = fields.iter().position(|f| f.is_empty()).unwrap_or(fields.len());
    if found != VERTEX_PROPERTIES.len() || fields.len() != VERTEX_PROPERTIES.len() {
        return Err(PlyError::VertexFields { line, found });
    }
    let mut coordinates = [FixedQ32_32::ZERO; 3];
    for (column, field) in fields.iter().enumerate() {
        let value =
            decimal_to_q32_32(field).map_err(|error| PlyError::Number { line, column, error })?;
        if let Some(slot) = coordinates.get_mut(column) {
            *slot = value;
        }
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
    let not_triangle = PlyError::FaceNotTriangle { face, line };
    let mut fields = text.split(' ');
    if fields.next() != Some("3") {
        return Err(not_triangle);
    }
    let mut indices = [0_u32; 3];
    for slot in &mut indices {
        let field = fields.next().ok_or(not_triangle)?;
        let index = parse_index(field).ok_or(PlyError::BadIndex { face, line })?;
        *slot = u32::try_from(index)
            .ok()
            .filter(|&i| usize::try_from(i).is_ok_and(|i| i < vertex_count))
            .ok_or(PlyError::IndexOutOfRange { face, line, index, vertex_count })?;
    }
    if fields.next().is_some() {
        return Err(not_triangle);
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

/// The count of an `element vertex` or `element face` line, within its limit.
fn count(line: usize, text: &str, element: Element) -> Result<usize, PlyError> {
    let (prefix, expected, limit) = match element {
        Element::Vertex => ("element vertex ", "element vertex <count>", MAX_VERTICES),
        Element::Face => ("element face ", "element face <count>", MAX_FACES),
    };
    let digits = text.strip_prefix(prefix).ok_or(PlyError::UnexpectedHeader { line, expected })?;
    let declared = decimal_digits(digits).ok_or(PlyError::BadCount { line, element })?;
    usize::try_from(declared).ok().filter(|&n| n <= limit).ok_or(PlyError::CountTooLarge {
        line,
        element,
        declared,
        limit,
    })
}

/// One to ten decimal digits as a number.
fn decimal_digits(digits: &str) -> Option<u64> {
    if digits.is_empty() || digits.len() > MAX_INTEGER_DIGITS {
        return None;
    }
    digits.bytes().try_fold(0_u64, |value, b| {
        let digit = b.is_ascii_digit().then(|| u64::from(b - b'0'))?;
        value.checked_mul(10)?.checked_add(digit)
    })
}

/// A face index: an optional `-` and one to ten digits.
fn parse_index(text: &str) -> Option<i64> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, text),
    };
    let magnitude = i64::try_from(decimal_digits(digits)?).ok()?;
    Some(if negative { -magnitude } else { magnitude })
}
