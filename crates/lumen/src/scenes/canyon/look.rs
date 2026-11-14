//! How the canyon looks: its diffuse albedos, its low sun and near-black sky, and its camera.

use bunny_linalg::FixedVec3;
use bunny_num::fixed_q32_32::ONE_RAW;
use bunny_num::FixedQ32_32;
use bunny_optics::{Albedo, DirectionalLight, SkyLight};

use super::error::CanyonError;
use crate::scene::{CameraPlacement, Lighting};
use crate::vertex_grid::{at, grid, U};

/// The linear albedo of each material as `[red, green, blue]` in 256ths, in the order of
/// [`super::PartKind::material`]: the rock's three strata (a warm brown, a paler sandstone
/// and a darker red-brown), the skiff's orange, the kite's teal, the tug's
/// cream, the engines' dark grey, the bunny's light warm grey, and the planet's eight bands
/// from its south pole.
pub const CANYON_ALBEDO_256: [[i64; 3]; 16] = [
    [150, 96, 64],
    [178, 128, 86],
    [118, 74, 52],
    [225, 105, 25],
    [30, 150, 165],
    [215, 205, 180],
    [36, 36, 40],
    [205, 200, 192],
    [200, 170, 125],
    [150, 95, 60],
    [225, 205, 165],
    [175, 110, 70],
    [235, 220, 185],
    [120, 80, 55],
    [205, 185, 150],
    [140, 160, 185],
];

/// The direction toward the sun in sixteenths: low over the right rim, ahead of the camera.
pub const CANYON_TOWARD_SUN_16: [i64; 3] = [3, 6, 16];

/// The sun's irradiance per channel in 256ths: about `(4.5, 4.2, 3.7)`, warm.
pub const CANYON_SUN_256: [i64; 3] = [1152, 1075, 950];

/// The sky's radiance per channel in 256ths: near black.
pub const CANYON_SKY_256: [i64; 3] = [2, 2, 3];

/// A Q32.32 value of `n / 256`.
const fn per_256(n: i64) -> FixedQ32_32 {
    FixedQ32_32::from_raw(n * (ONE_RAW / 256))
}

/// The materials, [`CANYON_ALBEDO_256`] as Bunny albedos.
///
/// # Errors
///
/// [`CanyonError::Constant`] if Bunny refuses an albedo; every channel is in `0..=256`, so it
/// does not.
pub fn canyon_materials() -> Result<Vec<Albedo>, CanyonError> {
    CANYON_ALBEDO_256
        .iter()
        .map(|channels| Albedo::new(channels.map(per_256)).map_err(CanyonError::Constant))
        .collect()
}

/// The sun and the sky.
///
/// # Errors
///
/// [`CanyonError::Constant`] if Bunny refuses the sun or the sky, which a test shows it does
/// not.
pub fn canyon_lighting() -> Result<Lighting, CanyonError> {
    let [x, y, z] = CANYON_TOWARD_SUN_16.map(|c| FixedQ32_32::from_raw(c * (ONE_RAW / 16)));
    let sun = DirectionalLight::new(FixedVec3::new(x, y, z), CANYON_SUN_256.map(per_256))
        .map_err(CanyonError::Constant)?;
    let sky = SkyLight::new(CANYON_SKY_256.map(per_256)).map_err(CanyonError::Constant)?;
    Ok(Lighting { sun: Some(sun), sky })
}

/// The camera: low over the canyon floor near its mouth, looking along it.
#[must_use]
pub const fn canyon_camera() -> CameraPlacement {
    CameraPlacement {
        position: at(-U * 9 / 2, U * 17 / 4, U * 12),
        look: at(-U / 32, U / 10, -U),
        up: at(0, U, 0),
        tan_half_fov: grid(U / 2),
    }
}
