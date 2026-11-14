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

/// The most vertices or faces a file may declare.
const MAX_COUNT: usize = 1 << 24;

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
