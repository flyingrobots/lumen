//! The ON puck: a white disc with a black rim lying in the 8 box, with "ON" in block letters.

use super::content::{FixedPointContent, PartKind};
use super::solids::{fan, flat_rect, quad, ring, walls, StepTriangle, Steps, UNIT};
use super::table::{FELT_Y, PAINT_Y};

/// The puck's centre on the felt, in grid steps: in the far half of the 8 box, behind its
/// numeral.
pub const PUCK_CENTRE: Steps = [UNIT * 7 / 8, FELT_Y, -UNIT * 15 / 4];
/// The puck's radius (0.5 units), the radius of its white face (0.4) and its height
/// (`3/16`).
pub const PUCK_RADIUS: i64 = UNIT / 2;
/// The radius of the white face inside the black rim.
pub const PUCK_FACE_RADIUS: i64 = UNIT * 2 / 5;
/// The puck's height.
pub const PUCK_HEIGHT: i64 = UNIT * 3 / 16;

/// The letters' height (along Z), width, stroke and the gap between them.
const LETTER_DEPTH: i64 = UNIT * 3 / 8;
const LETTER_WIDTH: i64 = UNIT * 7 / 32;
const LETTER_STROKE: i64 = UNIT / 16;
const LETTER_GAP: i64 = UNIT / 16;

/// Appends the puck's white face, its black rim (the ring around the face and the side) and
/// its letters.
pub(crate) fn push_puck(content: &mut FixedPointContent) {
    let [cx, cy, cz] = PUCK_CENTRE;
    let top = cy + PUCK_HEIGHT;
    let around = |radius: i64, y: i64| -> Vec<Steps> {
        ring(radius, 24, 1).into_iter().map(|(u, v)| [cx + u, y, cz - v]).collect()
    };
    let (outer, inner) = (around(PUCK_RADIUS, top), around(PUCK_FACE_RADIUS, top));
    let mut face = Vec::new();
    fan(&mut face, [cx, top, cz], &inner, [0, 1, 0]);
    content.push_steps(PartKind::PuckFace, &face);
    let mut rim = Vec::new();
    for (i, (&o0, &i0)) in outer.iter().zip(&inner).enumerate() {
        let next = (i + 1) % outer.len();
        if let (Some(&o1), Some(&i1)) = (outer.get(next), inner.get(next)) {
            quad(&mut rim, [i0, o0, o1, i1], [0, 1, 0]);
        }
    }
    walls(&mut rim, &around(PUCK_RADIUS, cy), [0, PUCK_HEIGHT, 0], [cx, cy, cz]);
    content.push_steps(PartKind::PuckRim, &rim);
    content.push_steps(PartKind::PuckLetters, &letters(top + PAINT_Y));
}

/// "ON", centred on the puck, reading left to right from the camera with its top away from it.
fn letters(y: i64) -> Vec<StepTriangle> {
    let [cx, _, cz] = PUCK_CENTRE;
    let left = cx - LETTER_WIDTH - LETTER_GAP / 2;
    let (far, near) = (cz - LETTER_DEPTH / 2, cz + LETTER_DEPTH / 2);
    let (w, s) = (LETTER_WIDTH, LETTER_STROKE);
    let mut out = Vec::new();
    // O: two full-height sides, a top and a bottom between them.
    for rect in [
        [left, far, left + s, near],
        [left + w - s, far, left + w, near],
        [left + s, far, left + w - s, far + s],
        [left + s, near - s, left + w - s, near],
    ] {
        flat_rect(&mut out, rect, y);
    }
    // N: two full-height sides and a diagonal from the top left to the bottom right.
    let n = left + w + LETTER_GAP;
    flat_rect(&mut out, [n, far, n + s, near], y);
    flat_rect(&mut out, [n + w - s, far, n + w, near], y);
    let slant = s * 3 / 2;
    let diagonal = [
        [n + s, y, far],
        [n + s, y, far + slant],
        [n + w - s, y, near],
        [n + w - s, y, near - slant],
    ];
    quad(&mut out, diagonal, [0, 1, 0]);
    out
}
