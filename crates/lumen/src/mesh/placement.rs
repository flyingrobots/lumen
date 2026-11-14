//! Placing a triangle mesh on the reference scene's vertex grid.
//!
//! A [`MeshPlacement`] applies an exact [`MeshTurn`] (none, a quarter or a half turn about `+Y`), a scale by a power of two, a floor to the
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
    /// A quarter turn about `+Y` that takes `-X` to `+Z`: `(x, y, z)` to `(z, y, -x)`.
    QuarterAboutY,
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
    /// The placed position of `v`.
    ///
    /// # Errors
    ///
    /// [`CoordinateOverflow`] when a placed coordinate does not fit a Q32.32 raw `i64`.
    pub fn place(&self, v: FixedVec3) -> Result<FixedVec3, CoordinateOverflow> {
        let (x, z) = (v.x.raw(), v.z.raw());
        let (turned_x, turned_z) = match self.turn {
            MeshTurn::None => (Some(x), Some(z)),
            MeshTurn::HalfAboutY => (x.checked_neg(), z.checked_neg()),
            MeshTurn::QuarterAboutY => (Some(z), x.checked_neg()),
        };
        let one = |raw: Option<i64>, steps: i64| {
            let scaled = raw?.checked_mul(1_i64.checked_shl(self.shift)?)?;
            let snapped = snap(FixedQ32_32::from_raw(scaled)).raw();
            snapped.checked_add(steps.checked_mul(GRID_RAW)?).map(FixedQ32_32::from_raw)
        };
        let [ox, oy, oz] = self.offset_steps;
        let overflow = |axis| move || CoordinateOverflow { axis };
        Ok(FixedVec3::new(
            one(turned_x, ox).ok_or_else(overflow(Axis::X))?,
            one(Some(v.y.raw()), oy).ok_or_else(overflow(Axis::Y))?,
            one(turned_z, oz).ok_or_else(overflow(Axis::Z))?,
        ))
    }
}

/// A coordinate axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// X.
    X,
    /// Y.
    Y,
    /// Z.
    Z,
}

/// A placed coordinate does not fit a Q32.32 raw `i64`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoordinateOverflow {
    /// The axis that overflowed.
    pub axis: Axis,
}

impl fmt::Display for CoordinateOverflow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the placed {:?} coordinate overflows Q32.32", self.axis)
    }
}

impl std::error::Error for CoordinateOverflow {}

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
    /// A vertex of this face overflows Q32.32 once placed.
    Overflow {
        /// The face index.
        face: usize,
        /// The overflow.
        overflow: CoordinateOverflow,
    },
    /// A placed vertex of this face is outside the trace domain (256 units).
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
            Self::Overflow { face, overflow } => write!(f, "face {face}: {overflow}"),
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
/// does not have, [`PlaceError::Overflow`] if a placed coordinate overflows Q32.32, or
/// [`PlaceError::OutsideTraceDomain`] if a placed vertex leaves the trace domain.
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
            *slot = placement
                .place(*vertex)
                .map_err(|overflow| PlaceError::Overflow { face, overflow })?;
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
        let overflow = CoordinateOverflow { axis: Axis::X };
        assert_eq!(place_mesh(&mesh, &huge), Err(PlaceError::Overflow { face: 0, overflow }));
    }

    /// Oracle: the rotation matrix of a quarter turn about `+Y` taking `-X` to `+Z`:
    /// `(x, y, z)` to `(z, y, -x)`, applied by hand to two constructed points.
    #[test]
    fn a_quarter_turn_takes_minus_x_to_plus_z() {
        let quarter =
            MeshPlacement { turn: MeshTurn::QuarterAboutY, shift: 1, offset_steps: [1, 2, 3] };
        let p = |x: i64, y: i64, z: i64| FixedVec3::new(grid(x), grid(y), grid(z));
        assert_eq!(quarter.place(p(-U, 0, 0)), Ok(p(1, 2, 2 * U + 3)));
        assert_eq!(quarter.place(p(U, 2 * U, 3 * U)), Ok(p(6 * U + 1, 4 * U + 2, -2 * U + 3)));
        let edge =
            FixedVec3::new(FixedQ32_32::from_raw(i64::MIN), FixedQ32_32::ZERO, FixedQ32_32::ZERO);
        let none = MeshPlacement { shift: 0, offset_steps: [0; 3], ..quarter };
        assert_eq!(none.place(edge), Err(CoordinateOverflow { axis: Axis::Z }));
    }
}
