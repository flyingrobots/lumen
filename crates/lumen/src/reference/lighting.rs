//! The reference scene's sun and sky.

use bunny_linalg::FixedVec3;
use bunny_num::fixed_q32_32::ONE_RAW;
use bunny_num::FixedQ32_32;
use bunny_optics::{DirectionalLight, SkyLight};

use super::per_256;
use crate::scene::Lighting;

/// The direction toward the reference sun, in sixteenths: `(0.5, 0.3125, 1.0)`. It comes from
/// the right, from in front of the scene (the camera's side) and from about 15 degrees above
/// the horizon, so the sphere and the octahedron cast long shadows back across the ground and
/// onto the wall. Its largest component is exactly 1.0, as Bunny's `DirectionalLight` requires.
pub const REFERENCE_TOWARD_SUN_16: [i64; 3] = [8, 5, 16];

/// The reference sun's irradiance per channel, in 256ths: about `(4.0, 3.8, 3.5)`, slightly warm.
pub const REFERENCE_SUN_256: [i64; 3] = [1024, 970, 900];

/// The reference sky's radiance per channel, in 256ths: about `(0.25, 0.33, 0.47)`, a pale blue.
pub const REFERENCE_SKY_256: [i64; 3] = [64, 85, 120];

/// The reference lighting: [`REFERENCE_TOWARD_SUN_16`] at [`REFERENCE_SUN_256`] and a
/// [`REFERENCE_SKY_256`] sky.
///
/// # Panics
///
/// Never: the constants are inside Bunny's accepted ranges (a test checks it), so the Bunny
/// constructors accept them.
#[must_use]
pub fn reference_lighting() -> Lighting {
    let [x, y, z] = REFERENCE_TOWARD_SUN_16.map(|c| FixedQ32_32::from_raw(c * (ONE_RAW / 16)));
    let sun = DirectionalLight::new(FixedVec3::new(x, y, z), REFERENCE_SUN_256.map(per_256))
        .expect("the reference sun is a valid Bunny light");
    let sky = SkyLight::new(REFERENCE_SKY_256.map(per_256)).expect("the reference sky is valid");
    Lighting { sun: Some(sun), sky }
}
