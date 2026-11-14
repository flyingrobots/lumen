//! The three craft: shapes of lumen's own, built from convex solids, posed in the canyon.
//!
//! Each craft is drawn nose toward `-Z` (along the canyon, away from the camera) about its own
//! origin, in 64ths of 3 units, then posed ([`Pose`]). Every craft is a painted hull and dark
//! engine blocks.

use super::content::{CanyonContent, Craft, PartKind};
use super::convex::{box_corners, push_convex, Convex, Pose, PoseError, Rational, BOX_FACES};
use super::error::CanyonError;
use crate::scenes::fixed_point::solids::{StepTriangle, Steps};
use crate::vertex_grid::{at, U};

/// A point of a craft's drawing, in grid steps: the drawing's units are 64ths of 3 units.
const fn p(x: i64, y: i64, z: i64) -> Steps {
    let s = U * 3 / 64;
    [x * s, y * s, z * s]
}

/// A position in 64ths of a unit, in grid steps.
const fn o(x: i64, y: i64, z: i64) -> Steps {
    let s = U / 64;
    [x * s, y * s, z * s]
}

/// The turn by `atan(7/24)`, about 16.3 degrees: the 7-24-25 triangle. [`Rational::new`] checks
/// it when the crate compiles, so a constant that is not a rotation stops the build and no
/// render can meet one.
const TURN_7_24: Rational = match Rational::new(24, 7, 25) {
    Ok(turn) => turn,
    Err(_) => panic!("24, 7, 25 is a Pythagorean triple"),
};

/// The turn by `-atan(7/24)`, checked as [`TURN_7_24`] is.
const TURN_MINUS_7_24: Rational = match Rational::new(24, -7, 25) {
    Ok(turn) => turn,
    Err(_) => panic!("24, -7, 25 is a Pythagorean triple"),
};

/// The poses of the skiff, the kite and the tug, in [`Craft::ALL`] order.
pub const CRAFT_POSES: [Pose; 3] = [
    Pose { bank: TURN_MINUS_7_24, heading: TURN_7_24, offset: o(-160, 7 * 64, -10 * 64) },
    Pose { bank: TURN_7_24, heading: Rational::NONE, offset: o(-352, 4 * 64, -24 * 64) },
    Pose { bank: TURN_MINUS_7_24, heading: TURN_MINUS_7_24, offset: o(-480, 11 * 32, -15 * 64) },
];

/// The skiff's hull: a hexagonal body with a pointed nose, two straight wings swept back and a
/// fin on its back.
fn skiff_hull(out: &mut Vec<StepTriangle>, pose: &Pose) -> Result<(), PoseError> {
    let ring =
        |z: i64| [p(14, 0, z), p(7, 8, z), p(-7, 8, z), p(-14, 0, z), p(-7, -6, z), p(7, -6, z)];
    let (front, back) = (ring(-38), ring(45));
    let mut corners = Vec::from(front);
    corners.extend_from_slice(&back);
    corners.push(p(0, 1, -84));
    let sides: Vec<[usize; 4]> = (0..6).map(|k| [k, (k + 1) % 6, 6 + (k + 1) % 6, 6 + k]).collect();
    let noses: Vec<[usize; 3]> = (0..6).map(|k| [k, (k + 1) % 6, 12]).collect();
    let mut faces: Vec<&[usize]> = sides.iter().map(<[usize; 4]>::as_slice).collect();
    faces.extend(noses.iter().map(<[usize; 3]>::as_slice));
    let tail = [6, 7, 8, 9, 10, 11];
    faces.push(&tail);
    push_convex(out, Convex { corners: &corners, faces: &faces }, pose)?;
    for side in [1, -1] {
        let wing = [
            p(side * 12, 2, -8),
            p(side * 66, 0, 22),
            p(side * 66, 0, 38),
            p(side * 12, 2, 40),
            p(side * 12, -1, -8),
            p(side * 66, -2, 22),
            p(side * 66, -2, 38),
            p(side * 12, -1, 40),
        ];
        let faces: [&[usize]; 6] = [
            &[0, 1, 2, 3],
            &[4, 5, 6, 7],
            &[0, 1, 5, 4],
            &[1, 2, 6, 5],
            &[2, 3, 7, 6],
            &[3, 0, 4, 7],
        ];
        push_convex(out, Convex { corners: &wing, faces: &faces }, pose)?;
    }
    let fin = [p(-1, 8, 10), p(-1, 8, 44), p(-1, 30, 48), p(1, 8, 10), p(1, 8, 44), p(1, 30, 48)];
    let faces: [&[usize]; 5] =
        [&[0, 1, 2], &[3, 4, 5], &[0, 1, 4, 3], &[1, 2, 5, 4], &[2, 0, 3, 5]];
    push_convex(out, Convex { corners: &fin, faces: &faces }, pose)
}

/// The skiff's engine: one block across its tail.
fn skiff_engine(out: &mut Vec<StepTriangle>, pose: &Pose) -> Result<(), PoseError> {
    let block = box_corners(p(-11, -5, 45), p(11, 7, 60));
    push_convex(out, Convex { corners: &block, faces: &BOX_FACES }, pose)
}

/// The kite's hull: a flat diamond wing, thickest along its spine.
fn kite_hull(out: &mut Vec<StepTriangle>, pose: &Pose) -> Result<(), PoseError> {
    let corners =
        [p(0, 0, -64), p(58, 0, 16), p(0, 0, 32), p(-58, 0, 16), p(0, 12, -6), p(0, -5, -6)];
    let faces: [&[usize]; 8] = [
        &[4, 0, 1],
        &[4, 1, 2],
        &[4, 2, 3],
        &[4, 3, 0],
        &[5, 0, 1],
        &[5, 1, 2],
        &[5, 2, 3],
        &[5, 3, 0],
    ];
    push_convex(out, Convex { corners: &corners, faces: &faces }, pose)
}

/// The kite's engines: two pods under its trailing edges.
fn kite_engines(out: &mut Vec<StepTriangle>, pose: &Pose) -> Result<(), PoseError> {
    for side in [1, -1] {
        let (a, b) = if side > 0 { (26, 38) } else { (-38, -26) };
        let block = box_corners(p(a, -6, 8), p(b, 2, 30));
        push_convex(out, Convex { corners: &block, faces: &BOX_FACES }, pose)?;
    }
    Ok(())
}

/// The tug's hull: a wedge tapering to a narrow nose, two pontoons beside it and a short wing
/// joining them.
fn tug_hull(out: &mut Vec<StepTriangle>, pose: &Pose) -> Result<(), PoseError> {
    let wedge = [
        p(-3, -2, -50),
        p(3, -2, -50),
        p(-3, 3, -50),
        p(3, 3, -50),
        p(-16, -8, 24),
        p(16, -8, 24),
        p(-16, 10, 24),
        p(16, 10, 24),
    ];
    push_convex(out, Convex { corners: &wedge, faces: &BOX_FACES }, pose)?;
    let wing = box_corners(p(-22, -2, -2), p(22, 1, 18));
    push_convex(out, Convex { corners: &wing, faces: &BOX_FACES }, pose)?;
    for (low, high) in [(-31, -21), (21, 31)] {
        let pontoon = box_corners(p(low, -6, -24), p(high, 3, 30));
        push_convex(out, Convex { corners: &pontoon, faces: &BOX_FACES }, pose)?;
    }
    Ok(())
}

/// The tug's engines: one block behind each pontoon.
fn tug_engine(out: &mut Vec<StepTriangle>, pose: &Pose) -> Result<(), PoseError> {
    for (low, high) in [(-30, -22), (22, 30)] {
        let block = box_corners(p(low, -5, 30), p(high, 2, 40));
        push_convex(out, Convex { corners: &block, faces: &BOX_FACES }, pose)?;
    }
    Ok(())
}

/// The triangles of a craft's part (a [`PartKind::Hull`] or [`PartKind::Engine`]), posed, in
/// grid steps; none for any other part.
///
/// # Errors
///
/// A [`PoseError`] if a corner does not pose; a test shows every craft does.
pub fn craft_part(kind: PartKind) -> Result<Vec<StepTriangle>, PoseError> {
    let pose = |craft: Craft| match craft {
        Craft::Skiff => &CRAFT_POSES[0],
        Craft::Kite => &CRAFT_POSES[1],
        Craft::Tug => &CRAFT_POSES[2],
    };
    let mut out = Vec::new();
    match kind {
        PartKind::Hull(Craft::Skiff) => skiff_hull(&mut out, pose(Craft::Skiff))?,
        PartKind::Engine(Craft::Skiff) => skiff_engine(&mut out, pose(Craft::Skiff))?,
        PartKind::Hull(Craft::Kite) => kite_hull(&mut out, pose(Craft::Kite))?,
        PartKind::Engine(Craft::Kite) => kite_engines(&mut out, pose(Craft::Kite))?,
        PartKind::Hull(Craft::Tug) => tug_hull(&mut out, pose(Craft::Tug))?,
        PartKind::Engine(Craft::Tug) => tug_engine(&mut out, pose(Craft::Tug))?,
        PartKind::Terrain | PartKind::Bunny | PartKind::Band(_) => {}
    }
    Ok(out)
}

/// Appends every craft to `content`, in [`Craft::ALL`] order, each its hull then its engines.
///
/// # Errors
///
/// [`CanyonError::Craft`] if a craft's part does not pose ([`craft_part`]); a test shows none
/// fails.
pub fn push_craft(content: &mut CanyonContent) -> Result<(), CanyonError> {
    for craft in Craft::ALL {
        for kind in [PartKind::Hull(craft), PartKind::Engine(craft)] {
            let part = craft_part(kind).map_err(CanyonError::Craft)?;
            let triangles: Vec<_> = part.iter().map(|t| t.map(|[x, y, z]| at(x, y, z))).collect();
            content.push(kind, &triangles);
        }
    }
    Ok(())
}
