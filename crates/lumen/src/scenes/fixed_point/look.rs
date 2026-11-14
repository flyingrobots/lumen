//! How the Fixed Point scene looks: its diffuse albedos, its sun and sky, and its camera.

use bunny_linalg::FixedVec3;
use bunny_num::fixed_q32_32::ONE_RAW;
use bunny_num::FixedQ32_32;
use bunny_optics::{Albedo, DirectionalLight, SkyLight};

use super::error::FixedPointError;
use crate::scene::{CameraPlacement, Lighting};
use crate::vertex_grid::{at, grid, U};

/// The linear albedo of each material as `[red, green, blue]` in 256ths, in the order of
/// [`super::Material::index`]: casino-green felt, dark brown leather, white layout paint, a red
/// die, white pips and puck face, a black rim, the bunny's light warm grey and the glasses'
/// near black.
pub const FIXED_POINT_ALBEDO_256: [[i64; 3]; 8] = [
    [14, 62, 32],
    [45, 22, 14],
    [225, 225, 215],
    [170, 12, 16],
    [240, 240, 232],
    [10, 10, 10],
    [205, 200, 192],
    [6, 6, 8],
];

/// The direction toward the sun in sixteenths: `(-0.25, 1.0, 1.0)`, high (about 44 degrees
/// above the horizon) and nearly behind the camera, 14 degrees to its left, so it lights the
/// bunny's face and chest straight on and the dice and the bunny throw their shadows back across
/// the felt. Version 1's sun, `(-0.5, 1.0, 0.875)`, came more from the side: on the smooth
/// full-resolution bunny its shadow edges crossed the cheek, neck and chest, where the shadow
/// rays' geometric test follows the mesh's triangles; from the front those edges fall near the
/// silhouette. Its largest component is exactly 1.0, as Bunny's `DirectionalLight` requires.
pub const FIXED_POINT_TOWARD_SUN_16: [i64; 3] = [-4, 16, 16];

/// The sun's irradiance per channel in 256ths: about `(4.0, 3.85, 3.6)`, slightly warm.
pub const FIXED_POINT_SUN_256: [i64; 3] = [1024, 985, 920];

/// The sky's radiance per channel in 256ths: about `(0.0625, 0.0625, 0.07)`, a dim room behind
/// the table, light enough that the shadowed sides keep their shape.
pub const FIXED_POINT_SKY_256: [i64; 3] = [16, 16, 18];

/// A Q32.32 value of `n / 256`.
const fn per_256(n: i64) -> FixedQ32_32 {
    FixedQ32_32::from_raw(n * (ONE_RAW / 256))
}

/// The materials, [`FIXED_POINT_ALBEDO_256`] as Bunny albedos.
///
/// # Errors
///
/// [`FixedPointError::Constant`] if Bunny refuses an albedo; every channel is in `0..=256`,
/// so it does not.
pub fn fixed_point_materials() -> Result<Vec<Albedo>, FixedPointError> {
    FIXED_POINT_ALBEDO_256
        .iter()
        .map(|channels| Albedo::new(channels.map(per_256)).map_err(FixedPointError::Constant))
        .collect()
}

/// The sun and the sky.
///
/// # Errors
///
/// [`FixedPointError::Constant`] if Bunny refuses the sun or the sky, which a test shows it
/// does not.
pub fn fixed_point_lighting() -> Result<Lighting, FixedPointError> {
    let [x, y, z] = FIXED_POINT_TOWARD_SUN_16.map(|c| FixedQ32_32::from_raw(c * (ONE_RAW / 16)));
    let sun = DirectionalLight::new(FixedVec3::new(x, y, z), FIXED_POINT_SUN_256.map(per_256))
        .map_err(FixedPointError::Constant)?;
    let sky = SkyLight::new(FIXED_POINT_SKY_256.map(per_256)).map_err(FixedPointError::Constant)?;
    Ok(Lighting { sun: Some(sun), sky })
}

/// The camera: low over the near rail at `(0, 2.125, 4.25)`, looking along
/// `(0, -0.140625, -1)` (about 8 degrees down), across the dice and the number boxes to the
/// bunny, with the vertical half-angle's tangent `15/32` (about 50 degrees in all). Framed
/// for 16:9: the ear tip near the top, the dice near the bottom.
#[must_use]
pub const fn fixed_point_camera() -> CameraPlacement {
    CameraPlacement {
        position: at(0, U * 17 / 8, U * 17 / 4),
        look: at(0, -U * 9 / 64, -U),
        up: at(0, U, 0),
        tan_half_fov: grid(U * 15 / 32),
    }
}
