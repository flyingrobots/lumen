//! Tests of reading the embedded bunny with Bunny's reader.

use super::*;
use crate::mesh::PlaceError;
use crate::reference::bunny::STANFORD_BUNNY_PLY;

/// A SHA-256 over the converted asset: every vertex's raw `x`, `y`, `z` as little-endian `i64`
/// in file order, then every face's three indices as little-endian `u32` in file order.
fn mesh_digest(vertices: &[FixedVec3], faces: &[[u32; 3]]) -> String {
    let mut bytes = Vec::new();
    for v in vertices {
        for c in [v.x, v.y, v.z] {
            bytes.extend_from_slice(&c.raw().to_le_bytes());
        }
    }
    for face in faces {
        for index in face {
            bytes.extend_from_slice(&index.to_le_bytes());
        }
    }
    crate::sha256::hex(&crate::sha256::digest(&bytes))
}

/// The digest of the asset as the interim reader of `lumen-ascii-ply-q32.32/1` converted it,
/// recorded before that reader was deleted
/// (flyingrobots/lumen#27).
const BUNNY_MESH_SHA256: &str = "e71004eeb1638058a898c433c3bb156671bd083f5bda7c9423b9c0e614428c86";

/// The asset, converted by Bunny's reader, equals what the interim reader of
/// `lumen-ascii-ply-q32.32/1` produced before it was deleted: the counts, the first vertex, the
/// bounds (the manifest's decimal strings, converted by Bunny) and the digest of every raw
/// coordinate and index in order. The interim reader and this value were compared vertex by
/// vertex and face by face when the reader was replaced.
#[test]
fn the_converted_asset_equals_what_the_interim_reader_produced() {
    let mesh = read_ascii_ply(STANFORD_BUNNY_PLY.as_bytes()).expect("Bunny's reader reads it");
    assert_eq!((mesh.vertices.len(), mesh.faces.len()), (1889, 3851));
    let first = mesh.vertices[0];
    assert_eq!(
        [first.x.raw(), first.y.raw(), first.z.raw()],
        [-158_536_692, 547_659_870, 11_886_623]
    );
    assert_eq!(mesh_digest(&mesh.vertices, &mesh.faces), BUNNY_MESH_SHA256);
}

#[test]
fn a_refusal_keeps_bunnys_kind_and_line() {
    let refused = read_ascii_ply(b"ply\nformat ascii 1.0\nend_header\n").expect_err("no vertices");
    assert_eq!(refused.kind(), bunny_codec::AsciiPlyErrorKind::MissingVertexElement);
    assert_eq!(refused.line(), 3);
    let placed = PlaceError::Ply(refused);
    assert_eq!(
        std::error::Error::source(&placed).map(ToString::to_string),
        Some(refused.to_string())
    );
}

#[test]
fn the_identities_reported_are_bunnys() {
    assert_eq!(bunny_codec::ASCII_PLY_PROFILE_ID, "bunny-ascii-ply-triangle-mesh/1");
    assert_eq!(bunny_codec::DECIMAL_Q32_32_ID, "bunny-decimal-q32-32/1");
}
