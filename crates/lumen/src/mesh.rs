//! Placing a triangle mesh in a scene, and the Stanford bunny placed for the reference scene.
//!
//! A mesh read by [`crate::ply`] is placed by a [`MeshPlacement`]: an optional half turn about
//! `+Y`, a scale by a power of two, a floor to the reference scene's `2^-16` vertex grid and a
//! translation by whole grid steps. Every step is exact or a stated floor, so a placed vertex is a
//! pure function of the file's bytes. Faces that have zero area once placed are dropped and
//! listed ([`PlacedMesh::dropped`]); none reaches a scene.

use core::fmt;

use bunny_linalg::FixedVec3;
use bunny_num::FixedQ32_32;
use bunny_optics::TriangleSurface;
use bunny_query::TraceTriangle;

use crate::ply::{parse_ascii_ply, PlyError, PlyMesh, STANFORD_BUNNY_PLY};
use crate::scene::{snap, GRID_RAW};

/// The bunny file's face count.
pub const BUNNY_FACE_COUNT: usize = 3851;

/// The bunny's faces that are zero-area after placement and are dropped: none. Every face of
/// `bun_zipper_res3.ply` keeps a nonzero area after the conversion, the scale and the floor to
/// the `2^-16` grid (the smallest has about `2.2e-5` square units); a test checks the count.
pub const BUNNY_DROPPED_FACES: usize = 0;

/// The bunny's triangles in the reference scene.
pub const BUNNY_TRIANGLE_COUNT: usize = BUNNY_FACE_COUNT - BUNNY_DROPPED_FACES;

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

/// The bunny's placement in the reference scene ([`crate::scene::reference_scene`]): a half turn
/// about `+Y` (the file's bunny faces `-X`; turned, it faces `+X`, toward the sphere, three-quarters
/// toward the camera), a scale of
/// 8 (about 1.24 units long, 1.21 tall and 0.96 deep), and a translation that puts it left of the
/// sphere, its lowest vertex exactly on the ground plane.
///
/// The offsets in grid steps of `2^-16` units:
///
/// - X: `-155 648`, `-2.375` units, so it spans `x = -2.8625..=-1.6201`;
/// - Y: `-91 246`. The lowest vertex (index 1748, `y = 0.0334143`, raw `143 513 326`) scales to
///   raw `1 148 106 608`, which floors to `17 518` grid steps; `GROUND_Y` is `-73 728` steps, and
///   `-73 728 - 17 518 = -91 246`. So that vertex lies exactly on the ground (`y = -1.125`): the
///   bunny stands on the plane rather than floating a hair above it, and Bunny's start-point
///   rule handles the touching surfaces;
/// - Z: `32 768`, `0.5` units (the octahedron's depth), so it spans `z = 0.0323..=0.9934`.
pub const BUNNY_PLACEMENT: MeshPlacement = MeshPlacement {
    turn: MeshTurn::HalfAboutY,
    shift: 3,
    offset_steps: [-155_648, -91_246, 32_768],
};

/// The Stanford bunny (`assets/stanford-bunny/bun_zipper_res3.ply`, read by
/// [`parse_ascii_ply`]) placed by [`BUNNY_PLACEMENT`]: 3 851 triangles, none dropped.
///
/// The file's winding is consistent: every edge shared by exactly two faces is traversed in
/// opposite directions by them (a test checks it), and the faces wind counter-clockwise seen from
/// outside. It is not a closed surface: Stanford's decimation does not preserve topology, so 60
/// edges belong to one face only (four holes, three in the base and a small one low on the
/// chest), 141 edges are shared by three or more faces, and 83 faces repeat another face's
/// vertices (14 in the same order, 69 reversed). None of this matters to two-sided shading: a
/// duplicate face is a coplanar tie that the lower primitive index wins.
///
/// # Errors
///
/// A [`PlaceError`] if the embedded file is refused or does not place; neither happens (a test
/// checks it).
pub fn bunny_triangles() -> Result<PlacedMesh, PlaceError> {
    let mesh = parse_ascii_ply(STANFORD_BUNNY_PLY).map_err(PlaceError::Ply)?;
    place_mesh(&mesh, &BUNNY_PLACEMENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{grid, reference_scene, GROUND_Y, REFERENCE_TRIANGLE_COUNT, U};

    fn bunny_raw_bounds(triangles: &[[FixedVec3; 3]]) -> ([i64; 3], [i64; 3]) {
        let mut lo = [i64::MAX; 3];
        let mut hi = [i64::MIN; 3];
        for v in triangles.iter().flatten() {
            for (axis, c) in [v.x, v.y, v.z].into_iter().enumerate() {
                lo[axis] = lo[axis].min(c.raw() / GRID_RAW);
                hi[axis] = hi[axis].max(c.raw() / GRID_RAW);
            }
        }
        (lo, hi)
    }

    /// The placed bunny's bounds in grid steps, derived by hand from the converted file bounds
    /// (`ply::tests`): coordinate `c` is placed at `floor(s * c * 8 / 2^16) + offset`, with
    /// `s = -1` on X and Z (the half turn).
    ///
    /// - min X from the file's max X, raw `261 712 114`: `-2 093 696 912 / 65 536 = -31 947.28`,
    ///   floor `-31 948`, plus `-155 648`: `-187 596`;
    /// - max X from the file's min X, raw `-405 291 582`: `3 242 332 656 / 65 536 = 49 474.07`,
    ///   floor `49 474`, plus `-155 648`: `-106 174`;
    /// - min Y, raw `143 513 326`: `1 148 106 608 / 65 536 = 17 518.72`, floor `17 518`, plus
    ///   `-91 246`: `-73 728`, which is `GROUND_Y`: the lowest vertex is on the ground;
    /// - max Y, raw `793 765 791`: `6 350 126 328 / 65 536 = 96 895.24`, floor `96 895`, plus
    ///   `-91 246`: `5 649`;
    /// - min Z from the file's max Z, raw `251 105 692`: `-2 008 845 536 / 65 536 = -30 652.55`,
    ///   floor `-30 653`, plus `32 768`: `2 115`;
    /// - max Z from the file's min Z, raw `-264 879 653`: `2 119 037 224 / 65 536 = 32 333.94`,
    ///   floor `32 333`, plus `32 768`: `65 101`.
    #[test]
    fn the_bunny_is_placed_where_stated() {
        let bunny = bunny_triangles().expect("the bunny reads and places");
        let (lo, hi) = bunny_raw_bounds(&bunny.triangles);
        assert_eq!(lo, [-187_596, -73_728, 2_115]);
        assert_eq!(hi, [-106_174, 5_649, 65_101]);
        assert_eq!(lo[1], GROUND_Y);
        // Exactly one vertex rests on the ground: the file's vertex 1748.
        let mesh = parse_ascii_ply(STANFORD_BUNNY_PLY).expect("reads");
        let on_ground: Vec<usize> = (0..mesh.vertices.len())
            .filter(|&i| {
                BUNNY_PLACEMENT.place(mesh.vertices[i]).expect("places").y == grid(GROUND_Y)
            })
            .collect();
        assert_eq!(on_ground, vec![1748]);
        // Clear of the sphere (|x| <= 1.04) and of the octahedron (x >= 1.25), inside the
        // ground and in front of the wall.
        assert!(hi[0] < -U * 3 / 2 && lo[0] > -3 * U && lo[2] > 0 && hi[2] < U);
    }

    #[test]
    fn the_bunny_keeps_every_face_and_none_is_degenerate() {
        let bunny = bunny_triangles().expect("the bunny reads and places");
        assert_eq!(bunny.dropped, Vec::<usize>::new());
        assert_eq!(bunny.dropped.len(), BUNNY_DROPPED_FACES);
        assert_eq!(bunny.triangles.len(), BUNNY_TRIANGLE_COUNT);
        assert_eq!(bunny.triangles.len(), 3851);
        // An independent exact test: the cross product of two edges is nonzero.
        let steps = |v: FixedVec3| [v.x, v.y, v.z].map(|c| i128::from(c.raw() / GRID_RAW));
        for (index, &[v0, v1, v2]) in bunny.triangles.iter().enumerate() {
            let (p0, p1, p2) = (steps(v0), steps(v1), steps(v2));
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let normal = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            assert_ne!(normal, [0; 3], "bunny triangle {index} has zero area");
        }
        // Every reference triangle has a surface: no zero-area triangle reaches the scene.
        let scene = reference_scene().expect("builds");
        for primitive in 0..u32::try_from(REFERENCE_TRIANGLE_COUNT).expect("fits") {
            assert!(scene.surface(primitive).is_some(), "primitive {primitive}");
        }
    }

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

    /// The bunny's winding is consistent: an edge shared by exactly two faces is traversed in
    /// opposite directions by them. Its holes and its non-manifold edges are counted as stated
    /// in [`bunny_triangles`], and its faces wind counter-clockwise seen from outside: the signed
    /// volume they enclose is positive (about 0.347 cubic units after placement).
    #[test]
    fn the_bunny_winding_is_consistent_and_its_holes_are_counted() {
        use std::collections::HashMap;
        let mesh = parse_ascii_ply(STANFORD_BUNNY_PLY).expect("reads");
        let mut directed: HashMap<(u32, u32), u32> = HashMap::new();
        for &[a, b, c] in &mesh.faces {
            for edge in [(a, b), (b, c), (c, a)] {
                *directed.entry(edge).or_default() += 1;
            }
        }
        let mut undirected: HashMap<(u32, u32), u32> = HashMap::new();
        for (&(a, b), &n) in &directed {
            *undirected.entry((a.min(b), a.max(b))).or_default() += n;
        }
        let (mut boundary, mut shared_by_more, mut inconsistent) = (0, 0, 0);
        for (&(a, b), &n) in &undirected {
            match n {
                1 => boundary += 1,
                2 => {
                    if directed.get(&(a, b)) != Some(&1) {
                        inconsistent += 1;
                    }
                }
                _ => shared_by_more += 1,
            }
        }
        assert_eq!((boundary, shared_by_more, inconsistent), (60, 141, 0));
        let mut faces: HashMap<[u32; 3], u32> = HashMap::new();
        for &face in &mesh.faces {
            let mut key = face;
            key.sort_unstable();
            *faces.entry(key).or_default() += 1;
        }
        let repeats: u32 = faces.values().map(|n| n - 1).sum();
        assert_eq!(repeats, 83);
        let bunny = bunny_triangles().expect("places");
        let steps = |v: FixedVec3| [v.x, v.y, v.z].map(|c| i128::from(c.raw() / GRID_RAW));
        let six_volume: i128 = bunny
            .triangles
            .iter()
            .map(|&[a, b, c]| {
                let (a, b, c) = (steps(a), steps(b), steps(c));
                a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                    + a[2] * (b[0] * c[1] - b[1] * c[0])
            })
            .sum();
        // In grid steps cubed: 6 * 0.347 * 2^48 is about 5.86e14.
        assert_eq!(six_volume, 585_898_999_030_544);
    }
}
