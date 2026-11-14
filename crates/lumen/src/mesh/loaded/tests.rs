//! Tests of reading the embedded bunny with Bunny's reader.

use super::*;
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
/// recorded before that reader was deleted.
const BUNNY_MESH_SHA256: &str = "e71004eeb1638058a898c433c3bb156671bd083f5bda7c9423b9c0e614428c86";

#[test]
fn bunnys_reader_equals_the_interim_reader_on_every_vertex_and_face() {
    let old = crate::mesh::ply::parse_ascii_ply(STANFORD_BUNNY_PLY).expect("the interim reads");
    let new = read_ascii_ply(STANFORD_BUNNY_PLY.as_bytes()).expect("Bunny's reads");
    assert_eq!(new.vertices.len(), old.vertices.len());
    assert_eq!(new.faces.len(), old.faces.len());
    for (i, (a, b)) in old.vertices.iter().zip(&new.vertices).enumerate() {
        assert_eq!([a.x.raw(), a.y.raw(), a.z.raw()], [b.x.raw(), b.y.raw(), b.z.raw()], "{i}");
    }
    assert_eq!(old.faces, new.faces);
    let digest = |m: &[FixedVec3], f: &[[u32; 3]]| mesh_digest(m, f);
    assert_eq!(digest(&old.vertices, &old.faces), digest(&new.vertices, &new.faces));
    assert_eq!(digest(&new.vertices, &new.faces), BUNNY_MESH_SHA256);
}
