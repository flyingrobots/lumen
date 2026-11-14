//! Tests of the PLY reader.

use super::*;
use crate::mesh::decimal::DecimalError;

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
        PlyError::BadCount { line: 4, element: Element::Vertex }
    );
    assert_eq!(
        refused("end_header\n", "end_header \n"),
        PlyError::UnexpectedHeader { line: 12, expected: "end_header" }
    );
    // A face with four indices, and one with a count of 3 but four indices.
    assert_eq!(
        refused("3 0 2 3\n", "4 0 1 2 3\n"),
        PlyError::FaceNotTriangle { face: 1, line: 18 }
    );
    assert_eq!(
        refused("3 0 2 3\n", "3 0 1 2 3\n"),
        PlyError::FaceNotTriangle { face: 1, line: 18 }
    );
    assert_eq!(refused("3 0 2 3\n", "3 0 2\n"), PlyError::FaceNotTriangle { face: 1, line: 18 });
    // An index out of range, negative or not a number.
    assert_eq!(
        refused("3 0 2 3\n", "3 0 2 4\n"),
        PlyError::IndexOutOfRange { face: 1, line: 18, index: 4, vertex_count: 4 }
    );
    assert_eq!(
        refused("3 0 1 2\n", "3 0 -1 2\n"),
        PlyError::IndexOutOfRange { face: 0, line: 17, index: -1, vertex_count: 4 }
    );
    assert_eq!(refused("3 0 1 2\n", "3 0 1.0 2\n"), PlyError::BadIndex { face: 0, line: 17 });
    // A number with a second decimal point, on line 14.
    assert_eq!(
        refused("1 0 0 1 0.5", "1.0.0 0 0 1 0.5"),
        PlyError::Number { line: 14, column: 0, error: DecimalError::SecondPoint }
    );
    // Wrong field counts and separators on a vertex line.
    assert_eq!(refused("1 0 0 1 0.5", "1 0 0 1"), PlyError::VertexFields { line: 14, found: 4 });
    assert_eq!(
        refused("1 0 0 1 0.5", "1 0 0 1 0.5 7"),
        PlyError::VertexFields { line: 14, found: 6 }
    );
    assert_eq!(
        refused("1 0 0 1 0.5", "1  0 0 1 0.5"),
        PlyError::VertexFields { line: 14, found: 1 }
    );
    // A carriage return is not a line ending here.
    assert_eq!(
        refused("1 0 0 1 0.5\n", "1 0 0 1 0.5\r\n"),
        PlyError::Number { line: 14, column: 4, error: DecimalError::Malformed }
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

/// Every limit is a typed refusal: the declared counts, the digits of a count and of an index,
/// and the length of a line, each just past its limit, while the limit itself is accepted.
#[test]
fn inputs_past_each_limit_are_refused() {
    let vertices = |n: &str| refused("element vertex 4", &format!("element vertex {n}"));
    let over = MAX_VERTICES + 1;
    assert_eq!(
        vertices(&over.to_string()),
        PlyError::CountTooLarge {
            line: 4,
            element: Element::Vertex,
            declared: u64::try_from(over).expect("fits"),
            limit: MAX_VERTICES
        }
    );
    // At the limit the count is accepted: the reader goes on to the vertices, and refuses the
    // first face line as a vertex.
    assert_eq!(vertices(&MAX_VERTICES.to_string()), PlyError::VertexFields { line: 17, found: 4 });
    assert_eq!(vertices("12345678901"), PlyError::BadCount { line: 4, element: Element::Vertex });
    assert_eq!(
        refused("element face 2", "element face 99999999"),
        PlyError::CountTooLarge {
            line: 10,
            element: Element::Face,
            declared: 99_999_999,
            limit: MAX_FACES
        }
    );
    assert_eq!(
        refused("3 0 1 2\n", "3 0 12345678901 2\n"),
        PlyError::BadIndex { face: 0, line: 17 }
    );
    let long_comment = format!("comment {}", "x".repeat(MAX_LINE_BYTES - 8));
    assert_eq!(long_comment.len(), MAX_LINE_BYTES);
    assert!(parse_ascii_ply(&SMALL.replacen("comment a test", &long_comment, 1)).is_ok());
    let too_long = format!("{long_comment}x");
    assert_eq!(
        refused("comment a test", &too_long),
        PlyError::LineTooLong { line: 3, length: MAX_LINE_BYTES + 1, limit: MAX_LINE_BYTES }
    );
}

/// A xorshift generator, so the generated cases are the same on every run.
fn xorshift(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

/// Over generated corruptions of a valid file (bytes replaced by characters that matter to the
/// grammar, bytes deleted or duplicated, the text cut short), the reader never panics, and every
/// text it accepts has exactly the declared shape. The seed and case are printed on failure.
#[test]
fn generated_corrupt_files_are_refused_without_panicking() {
    const SEED: u64 = 0x0b0d_cafe_f00d_0001;
    const ALPHABET: &[u8] = b"0123456789 .-+eE\n\rply3xa";
    let mut state = SEED;
    let (mut refused_count, mut accepted) = (0_u32, 0_u32);
    for case in 0..20_000 {
        let mut bytes = SMALL.as_bytes().to_vec();
        for _ in 0..=xorshift(&mut state) % 3 {
            let at = usize::try_from(xorshift(&mut state)).expect("fits") % bytes.len();
            match xorshift(&mut state) % 4 {
                0 => bytes.truncate(at),
                1 => {
                    bytes.remove(at);
                }
                2 => bytes.insert(at, bytes[at]),
                _ => {
                    let pick =
                        usize::try_from(xorshift(&mut state)).expect("fits") % ALPHABET.len();
                    bytes[at] = ALPHABET[pick];
                }
            }
            if bytes.is_empty() {
                break;
            }
        }
        let text = String::from_utf8(bytes).expect("ASCII stays UTF-8");
        let outcome = std::panic::catch_unwind(|| parse_ascii_ply(&text));
        let Ok(result) = outcome else {
            panic!("seed {SEED:#x} case {case}: the reader panicked on {text:?}");
        };
        match result {
            Ok(mesh) => {
                accepted += 1;
                assert!(
                    mesh.faces
                        .iter()
                        .flatten()
                        .all(|&i| usize::try_from(i).is_ok_and(|i| i < mesh.vertices.len())),
                    "seed {SEED:#x} case {case}: {text:?}"
                );
            }
            Err(_) => refused_count += 1,
        }
    }
    // Both outcomes occur in volume (19 429 refused and 571 accepted when this was written).
    assert!(refused_count > 15_000 && accepted > 200, "{refused_count} refused, {accepted} accepted");
}
