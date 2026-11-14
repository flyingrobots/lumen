//! The two dice: one chamfered cube with four raised pips on every face, placed twice by
//! exact rational turns about `+Y` and whole-step translations.
//!
//! Every coordinate of the die's mesh is a multiple of [`DIE_STEP`] (25) grid steps, and each
//! turn's denominator divides 25, so turning it is exact: the two dice are the same mesh under
//! two rigid motions, vertex for vertex, with nothing rounded.

use super::content::{Die, FixedPointContent, PartKind};
use super::error::FixedPointError;
use super::solids::{facing, fan, quad, ring, walls, StepTriangle, Steps};
use super::turn::RationalTurn;

/// Every die coordinate is a multiple of this many grid steps.
pub const DIE_STEP: i64 = 25;

/// Half the die's edge, in steps: `16 400` (about 0.2502 units; the edge is about 0.5).
pub const DIE_HALF: i64 = 16_400;
/// The chamfer on every edge and corner: `2 625` steps (about 0.04 units).
pub const DIE_CHAMFER: i64 = 2_625;
/// The pips' centres are `8 200` steps from the face's centre on both face axes.
pub const PIP_OFFSET: i64 = 8_200;
/// The pips' radius (`3 600` steps) and how far they stand out of the face (`650` steps).
pub const PIP_RADIUS: i64 = 3_600;
/// How far a pip stands out of its face.
pub const PIP_RISE: i64 = 650;

/// Where a die is: turned about its centre, then moved so its centre is at `centre` (steps).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiePose {
    /// The turn about the die's vertical axis.
    pub turn: RationalTurn,
    /// The die's centre, in grid steps.
    pub centre: Steps,
}

/// The height of a die's centre above the felt: it rests on the four pips of its bottom face.
pub const DIE_CENTRE_Y: i64 = DIE_HALF + PIP_RISE;

/// The two dice: the left one turned by `atan(3/4)` (about 36.9 degrees), the right one by
/// `-atan(7/24)` (about -16.3 degrees), both on the felt near the camera.
pub const DICE: [(Die, DiePose); 2] = [
    (
        Die::Left,
        DiePose {
            turn: RationalTurn { cos: 4, sin: 3, hyp: 5 },
            centre: [-36_864, DIE_CENTRE_Y, 16_384],
        },
    ),
    (
        Die::Right,
        DiePose {
            turn: RationalTurn { cos: 24, sin: -7, hyp: 25 },
            centre: [32_768, DIE_CENTRE_Y, 24_576],
        },
    ),
];

impl DiePose {
    /// The point `p` of the unplaced die, placed; `None` if the turn is not exact for it.
    #[must_use]
    pub fn place(&self, p: Steps) -> Option<Steps> {
        let [x, y, z] = self.turn.exact(p, [0; 3])?;
        let [cx, cy, cz] = self.centre;
        Some([x.checked_add(cx)?, y.checked_add(cy)?, z.checked_add(cz)?])
    }
}

/// The unplaced die, centred on the origin: its body (6 faces, 12 chamfer strips and 8 corner
/// triangles: 44 triangles) and its 24 pips (each a 12-sided raised disc: 12 top triangles and
/// 24 wall triangles, 864 in all).
#[must_use]
pub fn die_mesh() -> (Vec<StepTriangle>, Vec<StepTriangle>) {
    (die_body(), die_pips())
}

/// The point at `along` on axis `axis` and `(u, v)` on the other two axes, in order.
fn on_axis(axis: usize, along: i64, (u, v): (i64, i64)) -> Steps {
    match axis {
        0 => [along, u, v],
        1 => [v, along, u],
        _ => [u, v, along],
    }
}

fn die_body() -> Vec<StepTriangle> {
    let (a, b) = (DIE_HALF, DIE_HALF - DIE_CHAMFER);
    let mut out = Vec::new();
    for axis in 0..3 {
        for sign in [1, -1] {
            let corners =
                [(-b, -b), (b, -b), (b, b), (-b, b)].map(|uv| on_axis(axis, sign * a, uv));
            quad(&mut out, corners, on_axis(axis, sign, (0, 0)));
        }
    }
    // The strip along the edge between the faces on axes `axis` and `axis + 1`.
    for axis in 0..3 {
        for (s1, s2) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let point = |first: i64, second: i64, w: i64| on_axis(axis, first, (second, w));
            let strip = [
                point(s1 * a, s2 * b, -b),
                point(s1 * a, s2 * b, b),
                point(s1 * b, s2 * a, b),
                point(s1 * b, s2 * a, -b),
            ];
            quad(&mut out, strip, point(s1, s2, 0));
        }
    }
    for sx in [1, -1] {
        for sy in [1, -1] {
            for sz in [1, -1] {
                let (px, py, pz) =
                    ([sx * a, sy * b, sz * b], [sx * b, sy * a, sz * b], [sx * b, sy * b, sz * a]);
                out.push(facing(px, py, pz, [sx, sy, sz]));
            }
        }
    }
    out
}

fn die_pips() -> Vec<StepTriangle> {
    let outline = ring(PIP_RADIUS, 12, DIE_STEP);
    let mut out = Vec::new();
    for axis in 0..3 {
        for sign in [1, -1] {
            for (pu, pv) in [(1, 1), (1, -1), (-1, -1), (-1, 1)] {
                let (cu, cv) = (pu * PIP_OFFSET, pv * PIP_OFFSET);
                let base: Vec<Steps> = outline
                    .iter()
                    .map(|&(u, v)| on_axis(axis, sign * DIE_HALF, (cu + u, cv + v)))
                    .collect();
                let rise = on_axis(axis, sign * PIP_RISE, (0, 0));
                walls(&mut out, &base, rise, on_axis(axis, sign * DIE_HALF, (cu, cv)));
                let top: Vec<Steps> = outline
                    .iter()
                    .map(|&(u, v)| on_axis(axis, sign * (DIE_HALF + PIP_RISE), (cu + u, cv + v)))
                    .collect();
                let centre = on_axis(axis, sign * (DIE_HALF + PIP_RISE), (cu, cv));
                fan(&mut out, centre, &top, on_axis(axis, sign, (0, 0)));
            }
        }
    }
    out
}

/// Appends both dice, each as its body then its pips.
///
/// # Errors
///
/// [`FixedPointError::InexactTurn`] if a turn is not exact for a vertex of the die, which a test shows does
/// not happen.
pub(crate) fn push_dice(content: &mut FixedPointContent) -> Result<(), FixedPointError> {
    let (body, pips) = die_mesh();
    for (die, pose) in DICE {
        let place = |triangles: &[StepTriangle]| -> Result<Vec<StepTriangle>, FixedPointError> {
            triangles
                .iter()
                .map(|t| {
                    let [a, b, c] = t.map(|p| pose.place(p));
                    Ok([
                        a.ok_or(FixedPointError::InexactTurn)?,
                        b.ok_or(FixedPointError::InexactTurn)?,
                        c.ok_or(FixedPointError::InexactTurn)?,
                    ])
                })
                .collect()
        };
        content.push_steps(PartKind::DieBody(die), &place(&body)?);
        content.push_steps(PartKind::Pips(die), &place(&pips)?);
    }
    Ok(())
}
