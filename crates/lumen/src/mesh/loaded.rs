//! A triangle mesh read from ASCII PLY bytes by Bunny's reader.
//!
//! The reading, including the decimal to Q32.32 conversion (`bunny-decimal-q32-32/1`) and the
//! file profile (`bunny-ascii-ply-triangle-mesh/1`), is Bunny's `bunny_codec::parse_ascii_ply`.
//! This module only gathers its validated iterators into the vectors that placement reads.

#[cfg(test)]
mod tests;

use bunny_codec::{parse_ascii_ply, AsciiPlyError};
use bunny_linalg::FixedVec3;

/// A mesh as read: the vertices in file order and the faces in file order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlyMesh {
    /// The vertex positions, in file order, as Q32.32 values.
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

/// Reads `bytes` as an ASCII PLY triangle mesh with Bunny's reader.
///
/// # Errors
///
/// Bunny's [`AsciiPlyError`], with its kind and 1-based line number, if the bytes are not a mesh
/// of the profile `bunny-ascii-ply-triangle-mesh/1`.
pub fn read_ascii_ply(bytes: &[u8]) -> Result<PlyMesh, AsciiPlyError> {
    let mesh = parse_ascii_ply(bytes)?;
    // The reader has validated every record, so the counts are those of the body and the
    // reservation is bounded by the input's size.
    let mut vertices = Vec::with_capacity(mesh.vertex_count());
    for vertex in mesh.vertices() {
        let vertex = vertex?;
        vertices.push(FixedVec3::new(vertex.x, vertex.y, vertex.z));
    }
    let mut faces = Vec::with_capacity(mesh.face_count());
    for face in mesh.triangles() {
        let face = face?;
        faces.push([face.v0, face.v1, face.v2]);
    }
    Ok(PlyMesh { vertices, faces })
}
