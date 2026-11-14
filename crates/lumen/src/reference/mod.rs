//! Lumen's built-in reference scene, [`REFERENCE_SCENE_ID`].
//!
//! This module owns the content of the reference scene: its triangles in primitive order, its
//! materials, its lighting and its camera. The shapes defined in code are in [`shapes`], the
//! Stanford bunny is in [`bunny`]; [`reference_scene`] builds them into a [`Scene`].

pub mod bunny;
pub mod camera;
pub mod lighting;
pub mod materials;
pub mod shapes;

#[cfg(test)]
mod tests;

use bunny_linalg::FixedVec3;
use bunny_num::fixed_q32_32::ONE_RAW;
use bunny_num::FixedQ32_32;

pub use bunny::{bunny_triangles, BUNNY_TRIANGLE_COUNT};
pub use camera::reference_camera;
pub use lighting::{
    reference_lighting, REFERENCE_SKY_256, REFERENCE_SUN_256, REFERENCE_TOWARD_SUN_16,
};
pub use materials::{
    reference_material_of, reference_materials, REFERENCE_ALBEDO_256, REFERENCE_MATERIAL_OF,
};

use crate::scene::{Scene, SceneError, SceneSpec};

/// The stable name and version of [`reference_scene`].
///
/// Any change to the reference scene's triangles, their order, their materials, its lighting or
/// its camera placement is a new version, and changes every image hash of it. Version 1
/// (`lumen-reference-scene/1`, geometry and camera only, shaded by normal) and version 2
/// (`lumen-reference-scene/2`, version 1 with materials, a light and a sky) are retired.
/// Version 3 keeps every triangle of version 2, their order, its materials and its lighting,
/// appends the Stanford bunny ([`bunny_triangles`]) with a material of its own,
/// and moves the camera one unit left and one up so the bunny's shadow can be seen
/// ([`reference_camera`]).
pub const REFERENCE_SCENE_ID: &str = "lumen-reference-scene/3";

/// The reference scene's triangles, in primitive order:
///
/// 1. the ground, two triangles at `y = -1.125` spanning `-16..=16` on X and Z, normal `+Y`;
/// 2. a back wall, two triangles at `z = -4` spanning `x = -3..=3` and `y = -1.125..=2`,
///    normal `+Z` (toward the camera);
/// 3. an octahedron of radius 1 centred at `(2.25, -0.125, 0.5)`, its lowest vertex on the
///    ground, eight faces with normals `(+-1, +-1, +-1) / sqrt(3)`;
/// 4. the bumpy sphere: a cube subdivided 48 by 48 per face and pushed out to a radius of
///    1.0 to 1.04 around the origin, 27 648 triangles;
/// 5. the Stanford bunny, [`bunny_triangles`]: 3 851 triangles in the file's face
///    order, after every shape above so that their primitive indices are those of version 2.
///
/// Every triangle of the first four objects winds counter-clockwise seen from outside its object
/// (from above for the ground, from the camera's side for the wall), so its normal points
/// outward. The bunny keeps the file's winding (see [`bunny_triangles`]); shading
/// is two-sided.
///
/// # Panics
///
/// Never: the embedded bunny reads and places (a test checks it).
#[must_use]
pub fn reference_triangles() -> Vec<[FixedVec3; 3]> {
    let mut triangles = Vec::with_capacity(REFERENCE_TRIANGLE_COUNT);
    shapes::push_shapes(&mut triangles);
    let bunny = bunny_triangles().expect("the embedded bunny reads");
    triangles.extend_from_slice(&bunny.triangles);
    triangles
}

/// The first primitive index of the bunny in the reference scene: every triangle of version 2
/// comes first.
pub const BUNNY_FIRST_PRIMITIVE: usize = 4 + 8 + 6 * 48 * 48 * 2;

/// A Q32.32 value of `n / 256`.
pub(crate) const fn per_256(n: i64) -> FixedQ32_32 {
    FixedQ32_32::from_raw(n * (ONE_RAW / 256))
}

/// The number of triangles of the reference scene: 2 + 2 + 8 + 27 648 + 3 851 = 31 511.
pub const REFERENCE_TRIANGLE_COUNT: usize = BUNNY_FIRST_PRIMITIVE + BUNNY_TRIANGLE_COUNT;

/// The built-in reference scene, [`REFERENCE_SCENE_ID`]: [`reference_triangles`] with
/// [`reference_materials`] and [`reference_lighting`], seen by [`reference_camera`].
///
/// # Errors
///
/// [`SceneError::OutOfMemory`] when its buffers cannot be reserved. The other errors of
/// [`Scene::new`] do not occur for this scene; a test checks it builds.
pub fn reference_scene() -> Result<Scene, SceneError> {
    let triangles = reference_triangles();
    let material_of = reference_material_of();
    let materials = reference_materials();
    Scene::new(&SceneSpec {
        triangles: &triangles,
        material_of: &material_of,
        materials: &materials,
        lighting: reference_lighting(),
        camera: reference_camera(),
    })
}
