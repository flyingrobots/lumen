//! Placing a triangle mesh on the reference scene's vertex grid.
//!
//! A [`MeshPlacement`] applies an exact [`MeshTurn`], a scale by a power of two, a floor to the
//! `2^-16` vertex grid and a translation by whole grid steps. Every step is exact or a stated
//! floor, so a placed vertex is a pure function of the mesh. Faces that have zero area once placed
//! are dropped and listed ([`PlacedMesh::dropped`]); none reaches a scene.

use core::fmt;

use bunny_linalg::FixedVec3;
use bunny_num::FixedQ32_32;
use bunny_optics::TriangleSurface;
use bunny_query::TraceTriangle;

use super::ply::{PlyError, PlyMesh};
use crate::vertex_grid::{snap, GRID_RAW};

/// A turn applied to a mesh before it is scaled: only turns that are exact in fixed point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshTurn {
    /// No turn.
    None,
    /// Half a turn about `+Y`: `(x, y, z)` to `(-x, y, -z)`.
    HalfAboutY,
}

/// How a mesh is placed in the scene: a [`MeshTurn`], a scale by `2^shift`, a floor to the
/// `2^-16` grid ([`snap`]) and a translation by whole grid steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshPlacement {
    /// The turn, applied first.
    pub turn: MeshTurn,
    /// The scale is `2^shift`, an exact shift of every raw coordinate.
    pub shift: u32,
    /// The translation in `2^-16` grid steps, added after the floor to the grid, so every
    /// placed coordinate stays on the grid.
    pub offset_steps: [i64; 3],
}

impl MeshPlacement {
    /// The placed position of `v`, or `None` when a coordinate overflows.
    #[must_use]
    pub fn place(&self, v: FixedVec3) -> Option<FixedVec3> {
        let turn = match self.turn {
            MeshTurn::None => 1,
            MeshTurn::HalfAboutY => -1,
        };
        let one = |raw: i64, sign: i64, steps: i64| {
            let scaled = raw.checked_mul(sign)?.checked_mul(1_i64.checked_shl(self.shift)?)?;
            let snapped = snap(FixedQ32_32::from_raw(scaled)).raw();
            snapped.checked_add(steps.checked_mul(GRID_RAW)?).map(FixedQ32_32::from_raw)
        };
        let [ox, oy, oz] = self.offset_steps;
        Some(FixedVec3::new(
            one(v.x.raw(), turn, ox)?,
            one(v.y.raw(), 1, oy)?,
            one(v.z.raw(), turn, oz)?,
        ))
    }
}

/// A mesh placed in the scene: its kept triangles in face order, and the faces dropped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedMesh {
    /// The triangles of the faces kept, in face order, with the file's vertex order.
    pub triangles: Vec<[FixedVec3; 3]>,
    /// The face indices dropped because they have zero area after placement, in face order.
    pub dropped: Vec<usize>,
}

/// Why a mesh could not be placed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceError {
    /// The PLY text was refused.
    Ply(PlyError),
    /// A face names a vertex the mesh does not have.
    MissingVertex {
        /// The face index.
        face: usize,
    },
    /// A placed coordinate overflows, or a placed face has a vertex outside the trace domain.
    OutsideTraceDomain {
        /// The face index.
        face: usize,
    },
}

impl fmt::Display for PlaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ply(error) => write!(f, "{error}"),
            Self::MissingVertex { face } => write!(f, "face {face} names a missing vertex"),
            Self::OutsideTraceDomain { face } => {
                write!(f, "face {face} is outside the trace domain once placed")
            }
        }
    }
}

impl std::error::Error for PlaceError {}

/// Places every face of `mesh` with `placement`, in face order, dropping each face whose placed
/// triangle has zero area: one for which Bunny's `TriangleSurface::new` returns `None` (its exact
/// plane normal is zero). Dropped faces are listed, never skipped silently, and no zero-area
/// triangle reaches the scene.
///
/// # Errors
///
/// For the first face that fails, [`PlaceError::MissingVertex`] if it names a vertex the mesh
/// does not have, or [`PlaceError::OutsideTraceDomain`] if a vertex overflows or leaves the trace
/// domain once placed.
pub fn place_mesh(mesh: &PlyMesh, placement: &MeshPlacement) -> Result<PlacedMesh, PlaceError> {
    let mut placed =
        PlacedMesh { triangles: Vec::with_capacity(mesh.faces.len()), dropped: Vec::new() };
    for (face, indices) in mesh.faces.iter().enumerate() {
        let outside = PlaceError::OutsideTraceDomain { face };
        let mut vertices =
            [FixedVec3::new(FixedQ32_32::ZERO, FixedQ32_32::ZERO, FixedQ32_32::ZERO); 3];
        for (slot, &index) in vertices.iter_mut().zip(indices) {
            let vertex = usize::try_from(index)
                .ok()
                .and_then(|i| mesh.vertices.get(i))
                .ok_or(PlaceError::MissingVertex { face })?;
            *slot = placement.place(*vertex).ok_or(outside)?;
        }
        let [a, b, c] = vertices;
        let triangle = TraceTriangle::new(a, b, c).ok_or(outside)?;
        if TriangleSurface::new(&triangle).is_some() {
            placed.triangles.push(vertices);
        } else {
            placed.dropped.push(face);
        }
    }
    Ok(placed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vertex_grid::{grid, U};

    /// A face that snaps to a line is dropped and listed; its neighbours are kept in order.
    #[test]
    fn a_face_that_becomes_degenerate_is_dropped_and_counted() {
        let step = FixedQ32_32::from_raw(1 << 10);
        let p = |x: i64, y: i64| FixedVec3::new(grid(x), grid(y), FixedQ32_32::ZERO);
        // Vertex 3 is 2^-22 units above vertex 0: placed with no scale, it floors onto vertex
        // 0's grid point, so face 1 (0, 1, 3) collapses to a segment.
        let mesh = PlyMesh {
            vertices: vec![
                p(0, 0),
                p(U, 0),
                p(0, U),
                FixedVec3::new(FixedQ32_32::ZERO, step, FixedQ32_32::ZERO),
            ],
            faces: vec![[0, 1, 2], [0, 1, 3], [2, 1, 0]],
        };
        let flat = MeshPlacement { turn: MeshTurn::None, shift: 0, offset_steps: [0; 3] };
        let placed = place_mesh(&mesh, &flat).expect("places");
        assert_eq!(placed.dropped, vec![1]);
        assert_eq!(
            placed.triangles,
            vec![[p(0, 0), p(U, 0), p(0, U)], [p(0, U), p(U, 0), p(0, 0)]]
        );
        // Scaled by 2^6 the same vertex lands 2^-16 above, on its own grid step: kept.
        let scaled = MeshPlacement { turn: MeshTurn::None, shift: 6, offset_steps: [0; 3] };
        assert_eq!(place_mesh(&mesh, &scaled).expect("places").dropped, Vec::<usize>::new());
        // A placement outside the trace domain is refused.
        let far = MeshPlacement { turn: MeshTurn::None, shift: 0, offset_steps: [256 * U, 0, 0] };
        assert_eq!(place_mesh(&mesh, &far), Err(PlaceError::OutsideTraceDomain { face: 0 }));
        let huge = MeshPlacement { turn: MeshTurn::None, shift: 40, offset_steps: [0; 3] };
        let missing = PlyMesh { vertices: mesh.vertices.clone(), faces: vec![[0, 1, 4]] };
        assert_eq!(place_mesh(&missing, &flat), Err(PlaceError::MissingVertex { face: 0 }));
        assert_eq!(place_mesh(&mesh, &huge), Err(PlaceError::OutsideTraceDomain { face: 0 }));
    }
}
