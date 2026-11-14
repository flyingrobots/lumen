//! The bunny at the table: the Stanford bunny scaled by 32, standing on the felt behind the
//! number boxes and turned three-quarters toward the camera, and the measurement of its face
//! that the sunglasses are fitted to.

use bunny_linalg::FixedVec3;
use bunny_optics::TriangleSurface;
use bunny_query::TraceTriangle;

use crate::mesh::{parse_ascii_ply, place_mesh, MeshPlacement, MeshTurn, PlaceError};
use crate::reference::bunny::STANFORD_BUNNY_PLY;
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
    /// The faces dropped because they have zero area once placed and turned, in face order.
    pub dropped: Vec<usize>,
    /// The face measurement, before the turn.
    pub face: FaceMeasure,
    /// The turn and move applied after the measurement.
    pub pose: BunnyPose,
}

/// The Stanford bunny turned a quarter about `+Y` (the file's bunny faces `-X`; turned, it
/// faces `+Z`, the camera), scaled by `2^`[`BUNNY_SHIFT`], floored to the grid with its lowest
/// vertex exactly on the felt, measured ([`FaceMeasure`]), then posed ([`BunnyPose`]).
///
/// # Errors
///
/// [`FixedPointError::Bunny`] if the embedded bunny does not read or place, and
/// [`FixedPointError::EmptyBunny`] if it has no triangles and [`FixedPointError::Overflow`] if
/// a posed vertex overflows; a test shows none of these happens.
pub fn placed_bunny() -> Result<PlacedBunny, FixedPointError> {
    let mesh = parse_ascii_ply(STANFORD_BUNNY_PLY)
        .map_err(|error| FixedPointError::Bunny(PlaceError::Ply(error)))?;
    let mut placement =
        MeshPlacement { turn: MeshTurn::QuarterAboutY, shift: BUNNY_SHIFT, offset_steps: [0; 3] };
    let first = place_mesh(&mesh, &placement).map_err(FixedPointError::Bunny)?;
    let lowest = lowest_y(&first.triangles).ok_or(FixedPointError::EmptyBunny)?;
    placement.offset_steps = [0, FELT_Y - lowest, 0];
    let facing = place_mesh(&mesh, &placement).map_err(FixedPointError::Bunny)?;
    let face = measure(&facing.triangles).ok_or(FixedPointError::EmptyBunny)?;
    let pose = BunnyPose { pivot: face.nose };
    let mut placed = PlacedBunny { triangles: Vec::new(), dropped: facing.dropped, face, pose };
    for (index, triangle) in facing.triangles.iter().enumerate() {
        let [a, b, c] = pose_triangle(&pose, &triangle.map(steps))?;
        let kept =
            TraceTriangle::new(at(a[0], a[1], a[2]), at(b[0], b[1], b[2]), at(c[0], c[1], c[2]))
                .and_then(|t| TriangleSurface::new(&t));
        match kept {
            Some(_) => placed.triangles.push([a, b, c].map(|[x, y, z]| at(x, y, z))),
            None => placed.dropped.push(index),
        }
    }
    Ok(placed)
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

fn lowest_y(triangles: &[[FixedVec3; 3]]) -> Option<i64> {
    triangles.iter().flatten().map(|&v| steps(v)[1]).min()
}

/// The nose and the ear tip of a bunny facing `+Z`, or `None` for no triangles.
fn measure(triangles: &[[FixedVec3; 3]]) -> Option<FaceMeasure> {
    let lowest = lowest_y(triangles)?;
    let ear_tip_y = triangles.iter().flatten().map(|&v| steps(v)[1]).max()?;
    let middle = lowest + (ear_tip_y - lowest) / 2;
    let mut nose: Option<Steps> = None;
    for vertex in triangles.iter().flatten().map(|&v| steps(v)) {
        let further = nose.is_none_or(|best| vertex[2] > best[2]);
        if vertex[1] >= middle && further {
            nose = Some(vertex);
        }
    }
    Some(FaceMeasure { nose: nose?, ear_tip_y })
}
