//! The bunny at the table: the Stanford bunny scaled by 32, standing on the felt behind the
//! number boxes and turned three-quarters toward the camera, and the measurement of its face
//! that the sunglasses are fitted to.

use bunny_linalg::{FixedUnitVec3, FixedVec3};
use bunny_optics::{TriangleSurface, VertexNormals};
use bunny_query::TraceTriangle;

use crate::mesh::{read_ascii_ply, MeshPlacement, MeshTurn, PlaceError, PlyMesh};

use super::full_bunny::FULL_BUNNY_PLY;
use crate::vertex_grid::{at, GRID_RAW};

use super::error::FixedPointError;
use super::solids::{StepTriangle, Steps, UNIT};
use super::table::FELT_Y;
use super::turn::RationalTurn;

/// The bunny's scale: `2^5`, so it stands about 4.85 units tall, nearly ten dice.
pub const BUNNY_SHIFT: u32 = 5;

/// Where the tip of the bunny's nose goes on X and Z, in grid steps: centred on the table,
/// 1.25 units behind the far line of the number boxes.
pub const NOSE_XZ: (i64, i64) = (0, -UNIT * 23 / 4);

/// The bunny's turn away from facing the camera straight on: `-atan(5/12)`, about 22.6
/// degrees toward the camera's right, so it stands three-quarters on, its tall ear and the
/// profile of its back toward the camera's left.
pub const BUNNY_TURN: RationalTurn = RationalTurn { cos: 12, sin: -5, hyp: 13 };

/// What the scene measures on the bunny before it is turned, when it faces `+Z`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceMeasure {
    /// The tip of the nose: of the vertices in the upper half of the bunny's height (the head
    /// and ears), the one farthest along `+Z`, the direction it faces. The first in face
    /// order wins a tie.
    pub nose: Steps,
    /// The height of the highest vertex, the tip of an ear.
    pub ear_tip_y: i64,
}

/// How the bunny and its sunglasses are turned and moved: by [`BUNNY_TURN`] about the
/// vertical line through the nose (each coordinate floored to the grid), then moved so the
/// nose is at [`NOSE_XZ`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BunnyPose {
    /// The nose tip before the turn.
    pub pivot: Steps,
}

impl BunnyPose {
    /// The point `p` (given facing `+Z`, before the turn) turned and moved.
    #[must_use]
    pub fn place(&self, p: Steps) -> Option<Steps> {
        let [x, y, z] = BUNNY_TURN.floored(p, self.pivot)?;
        let [px, _, pz] = self.pivot;
        Some([x.checked_add(NOSE_XZ.0 - px)?, y, z.checked_add(NOSE_XZ.1 - pz)?])
    }
}

/// The bunny as placed in the scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedBunny {
    /// Its triangles in the file's face order, without the faces dropped.
    pub triangles: Vec<[FixedVec3; 3]>,
    /// The vertex normals of each triangle, in the order of its vertices: Bunny's
    /// `VertexNormals` (`bunny-vertex-normals/1`) over the posed positions and the kept faces.
    pub normals: Vec<[Option<FixedUnitVec3>; 3]>,
    /// The faces dropped because they have zero area once placed and turned, in face order.
    pub dropped: Vec<usize>,
    /// The face measurement, before the turn.
    pub face: FaceMeasure,
    /// The turn and move applied after the measurement.
    pub pose: BunnyPose,
}

/// The full-resolution Stanford bunny ([`FULL_BUNNY_PLY`]) turned a quarter about `+Y` (the
/// file's bunny faces `-X`; turned, it faces `+Z`, the camera), scaled by `2^`[`BUNNY_SHIFT`],
/// floored to the grid with its lowest vertex exactly on the felt, measured ([`FaceMeasure`]),
/// then posed ([`BunnyPose`]), with smooth-shading normals.
///
/// Every vertex is placed and posed once, and the faces keep their indices into the posed
/// vertices, so the vertex normals are Bunny's `VertexNormals::new` over the positions that are
/// traced: computed after placement, they need no transform of their own and agree with the
/// floored geometry. A face whose posed triangle has zero area is dropped and listed; its cross
/// product is zero, so it adds nothing to a normal either way. The vertices that no face names
/// are placed too (Bunny's `VertexNormals` gives them `None`), but no triangle refers to them.
///
/// # Errors
///
/// [`FixedPointError::Bunny`] if the embedded bunny does not read or a face names a missing
/// vertex, [`FixedPointError::EmptyBunny`] if it has no faces, [`FixedPointError::Overflow`] if a
/// placed or posed vertex overflows, and [`FixedPointError::Normals`] if Bunny refuses the posed mesh; a test
/// shows none of these happens.
pub fn placed_bunny() -> Result<PlacedBunny, FixedPointError> {
    let mesh = read_ascii_ply(FULL_BUNNY_PLY.as_bytes())
        .map_err(|error| FixedPointError::Bunny(PlaceError::Ply(error)))?;
    let used = used_vertices(&mesh)?;
    let mut placement =
        MeshPlacement { turn: MeshTurn::QuarterAboutY, shift: BUNNY_SHIFT, offset_steps: [0; 3] };
    let lowest = placed_steps(&mesh, &placement)?
        .iter()
        .zip(&used)
        .filter(|(_, &used)| used)
        .map(|(p, _)| p[1])
        .min()
        .ok_or(FixedPointError::EmptyBunny)?;
    placement.offset_steps = [0, FELT_Y - lowest, 0];
    let facing = placed_steps(&mesh, &placement)?;
    let named = facing.iter().zip(&used).filter(|(_, &used)| used).map(|(&p, _)| p);
    let face = measure(named).ok_or(FixedPointError::EmptyBunny)?;
    let pose = BunnyPose { pivot: face.nose };
    let posed: Vec<FixedVec3> = facing
        .iter()
        .map(|&p| pose.place(p).map(|[x, y, z]| at(x, y, z)).ok_or(FixedPointError::Overflow))
        .collect::<Result<_, _>>()?;
    let mut kept = Vec::with_capacity(mesh.faces.len());
    let mut dropped = Vec::new();
    for (index, &face_indices) in mesh.faces.iter().enumerate() {
        let [a, b, c] = face_indices.map(|i| posed.get(widen(i)).copied());
        let area = a
            .zip(b)
            .zip(c)
            .and_then(|((a, b), c)| TraceTriangle::new(a, b, c))
            .and_then(|t| TriangleSurface::new(&t));
        match area {
            Some(_) => kept.push(face_indices),
            None => dropped.push(index),
        }
    }
    let vertex_normals = VertexNormals::new(&posed, &kept).map_err(FixedPointError::Normals)?;
    let corner = |i: u32| posed.get(widen(i)).copied().ok_or(FixedPointError::EmptyBunny);
    let normal = |i: u32| vertex_normals.normals().get(widen(i)).copied().flatten();
    let mut triangles = Vec::with_capacity(kept.len());
    let mut normals = Vec::with_capacity(kept.len());
    for face_indices in &kept {
        let [a, b, c] = face_indices.map(corner);
        triangles.push([a?, b?, c?]);
        normals.push(face_indices.map(normal));
    }
    Ok(PlacedBunny { triangles, normals, dropped, face, pose })
}

/// Whether each vertex of `mesh` is named by a face.
fn used_vertices(mesh: &PlyMesh) -> Result<Vec<bool>, FixedPointError> {
    let mut used = vec![false; mesh.vertices.len()];
    for (face, indices) in mesh.faces.iter().enumerate() {
        for &index in indices {
            let slot = used.get_mut(widen(index));
            *slot.ok_or(FixedPointError::Bunny(PlaceError::MissingVertex { face }))? = true;
        }
    }
    Ok(used)
}

/// Every vertex of `mesh` placed by `placement`, in grid steps.
fn placed_steps(mesh: &PlyMesh, placement: &MeshPlacement) -> Result<Vec<Steps>, FixedPointError> {
    mesh.vertices
        .iter()
        .map(|&v| placement.place(v).map(steps).map_err(|_| FixedPointError::Overflow))
        .collect()
}

/// A `u32` index as a `usize`: lossless on every target lumen builds for (at least 32 bits).
fn widen(index: u32) -> usize {
    usize::try_from(index).unwrap_or(usize::MAX)
}

/// A triangle given facing `+Z`, posed.
pub(crate) fn pose_triangle(
    pose: &BunnyPose,
    triangle: &StepTriangle,
) -> Result<StepTriangle, FixedPointError> {
    let [a, b, c] = triangle.map(|p| pose.place(p));
    Ok([
        a.ok_or(FixedPointError::Overflow)?,
        b.ok_or(FixedPointError::Overflow)?,
        c.ok_or(FixedPointError::Overflow)?,
    ])
}

/// A placed vertex in grid steps (every placed coordinate is a whole number of steps).
fn steps(v: FixedVec3) -> Steps {
    [v.x, v.y, v.z].map(|c| c.raw() / GRID_RAW)
}

/// The nose and the ear tip of a bunny facing `+Z`, from the vertices its faces name, or `None`
/// for none.
fn measure(vertices: impl Iterator<Item = Steps> + Clone) -> Option<FaceMeasure> {
    let lowest = vertices.clone().map(|v| v[1]).min()?;
    let ear_tip_y = vertices.clone().map(|v| v[1]).max()?;
    let middle = lowest + (ear_tip_y - lowest) / 2;
    let mut nose: Option<Steps> = None;
    for vertex in vertices {
        let further = nose.is_none_or(|best| vertex[2] > best[2]);
        if vertex[1] >= middle && further {
            nose = Some(vertex);
        }
    }
    Some(FaceMeasure { nose: nose?, ear_tip_y })
}
