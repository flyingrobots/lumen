//! The bunny's sunglasses: two flat wraparound lenses, a bridge and two arms, placed relative
//! to the measured tip of its nose ([`FaceMeasure`]).
//!
//! Every distance here was chosen by measuring the placed mesh and checking close-up renders;
//! a test checks the fit (each lens sits within [`LENS_MAX_GAP`] in front of the face, and no
//! part of the glasses touches the bunny).

use super::content::{FixedPointContent, PartKind, Side};
use super::error::FixedPointError;
use super::player::{pose_triangle, BunnyPose, FaceMeasure};
use super::solids::{fan, quad, StepTriangle, Steps, UNIT};

/// The largest distance along `-Z` from a lens's centre to the face behind it: 0.25 units,
/// about a twentieth of the bunny's height.
pub const LENS_MAX_GAP: i64 = UNIT / 4;

/// How far above the nose tip the lenses' centre line is: the eyes' height.
const EYE_RISE: i64 = UNIT / 2;
/// How far in front of the nose tip the lenses' inner edges are: the glasses rest on the
/// ridge of the snout.
const PROUD: i64 = UNIT / 32;
/// `n / 256` units in grid steps: a multiple of 256 steps, so a quarter of it is exact.
const fn per_256(n: i64) -> i64 {
    n * (UNIT / 256)
}

/// A lens's outline as `(distance from the centre line, height above the eye line)`, around
/// its edge: a wayfarer, wider at the top. Each lens wraps round the face: it slopes back by
/// exactly a quarter of its distance from the centre line, so it is flat.
const LENS_OUTLINE: [(i64, i64); 8] = [
    (per_256(31), per_256(26)),
    (per_256(51), per_256(44)),
    (per_256(200), per_256(51)),
    (per_256(210), per_256(5)),
    (per_256(184), -per_256(41)),
    (per_256(141), -per_256(56)),
    (per_256(64), -per_256(51)),
    (per_256(36), -per_256(20)),
];
/// The bridge spans the centre line to this distance on each side, between these heights.
const BRIDGE_HALF: i64 = per_256(33);
const BRIDGE_LOW: i64 = per_256(15);
const BRIDGE_HIGH: i64 = per_256(33);
/// Where each arm ends, as `(distance from the centre line, distance behind the nose tip)`:
/// the left one passes outside the base of the bunny's swept-back ear, the right one runs close
/// along the side of its head. Measured on the full-resolution mesh: the right arm needs its end
/// at least 230/256 out at 205/256 back to clear the head, so it ends at 240. Both run between these heights above the eye line.
const ARM_END: [(i64, i64); 2] = [(per_256(282), per_256(179)), (per_256(240), per_256(205))];
const ARM_LOW: i64 = per_256(15);
const ARM_HIGH: i64 = per_256(33);

/// The point at `across` from the centre line (signed), `up` above the eye line, on the lens
/// surface: `PROUD` in front of the nose, less a quarter of the distance from the centre line.
fn on_lens(face: &FaceMeasure, across: i64, up: i64) -> Steps {
    let [nx, ny, nz] = face.nose;
    [nx + across, ny + EYE_RISE + up, nz + PROUD - across.abs() / 4]
}

/// Appends the left lens, the right lens, the bridge and the two arms, each built facing `+Z`
/// around the measured nose and then posed with the bunny.
///
/// # Errors
///
/// [`FixedPointError::Overflow`] if a posed coordinate overflows, which it does not.
pub(crate) fn push_sunglasses(
    content: &mut FixedPointContent,
    face: &FaceMeasure,
    pose: &BunnyPose,
) -> Result<(), FixedPointError> {
    let mut push = |kind: PartKind, local: &[StepTriangle]| -> Result<(), FixedPointError> {
        let posed: Result<Vec<StepTriangle>, FixedPointError> =
            local.iter().map(|t| pose_triangle(pose, t)).collect();
        content.push_steps(kind, &posed?);
        Ok(())
    };
    for (side, sign) in [(Side::Left, -1), (Side::Right, 1)] {
        push(PartKind::Lens(side), &lens(face, sign))?;
    }
    let [nx, ny, nz] = face.nose;
    let (y0, y1, z) = (ny + EYE_RISE + BRIDGE_LOW, ny + EYE_RISE + BRIDGE_HIGH, nz + PROUD);
    let mut bridge = Vec::new();
    let span = [
        [nx - BRIDGE_HALF, y0, z],
        [nx + BRIDGE_HALF, y0, z],
        [nx + BRIDGE_HALF, y1, z],
        [nx - BRIDGE_HALF, y1, z],
    ];
    quad(&mut bridge, span, [0, 0, 1]);
    push(PartKind::Bridge, &bridge)?;
    for (side, sign) in [(Side::Left, -1), (Side::Right, 1)] {
        push(PartKind::Arm(side), &arm(face, sign))?;
    }
    Ok(())
}

/// A lens: a fan from the mean of its outline's points.
fn lens(face: &FaceMeasure, sign: i64) -> Vec<StepTriangle> {
    let outline: Vec<Steps> =
        LENS_OUTLINE.iter().map(|&(out, up)| on_lens(face, sign * out, up)).collect();
    let count = i64::try_from(outline.len()).unwrap_or(1);
    let mean = |axis: usize| outline.iter().map(|p| p[axis]).sum::<i64>() / count;
    let mut lens = Vec::new();
    fan(&mut lens, [mean(0), mean(1), mean(2)], &outline, [0, 0, 1]);
    lens
}

/// An arm: a vertical strip from the lens's outer edge back to its [`ARM_END`].
fn arm(face: &FaceMeasure, sign: i64) -> Vec<StepTriangle> {
    let (hinge_out, _) = LENS_OUTLINE[3];
    let (end_out, end_back) = if sign < 0 { ARM_END[0] } else { ARM_END[1] };
    let hinge = |up: i64| on_lens(face, sign * hinge_out, up);
    let [nx, ny, nz] = face.nose;
    let end = |up: i64| [nx + sign * end_out, ny + EYE_RISE + up, nz - end_back];
    let mut strip = Vec::new();
    quad(&mut strip, [hinge(ARM_LOW), end(ARM_LOW), end(ARM_HIGH), hinge(ARM_HIGH)], [sign, 0, 0]);
    strip
}
