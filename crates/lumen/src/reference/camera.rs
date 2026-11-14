//! The reference scene's camera placement.

use crate::scene::CameraPlacement;
use crate::vertex_grid::{at, grid, U};

/// The reference camera: at `(-1, 2, 5)`, looking along `(0.1875, -0.375, -1)` (down and a
/// little to the right, toward the sphere), up `+Y`, with a vertical field of view whose
/// half-angle has tangent 0.5 (about 53 degrees in all).
///
/// Versions 1 and 2 stood at `(0, 1, 5)` looking along `(0, -0.25, -1)`. From there the line of
/// sight to the bunny runs almost exactly along its shadow (the sun's ground direction is
/// `(-0.5, -1)` in X and Z, and the bunny is at about `(-2.2, 0.5)`), so the bunny hid its own
/// shadow. One unit left and one up, the shadow falls clear of the bunny and every object and
/// shadow of version 2 stays in the picture.
#[must_use]
pub const fn reference_camera() -> CameraPlacement {
    CameraPlacement {
        position: at(-U, 2 * U, 5 * U),
        look: at(U * 3 / 16, -U * 3 / 8, -U),
        up: at(0, U, 0),
        tan_half_fov: grid(U / 2),
    }
}
