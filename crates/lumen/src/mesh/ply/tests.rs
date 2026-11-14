//! Tests of the PLY reader.

use super::*;

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
    assert_eq!(refused("element vertex 4", "element vertex -4"), PlyError::BadCount { line: 4 });
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
