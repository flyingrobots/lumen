//! The reference scene's diffuse materials and the material of each triangle.

use bunny_optics::Albedo;

use super::{per_256, BUNNY_FIRST_PRIMITIVE, REFERENCE_TRIANGLE_COUNT};

/// The reference materials, as `[red, green, blue]` linear albedo in 256ths, indexed by
/// [`REFERENCE_MATERIAL_OF`]: a light warm grey ground, a pale stone wall, a blue octahedron,
/// a red sphere and a green bunny.
pub const REFERENCE_ALBEDO_256: [[i64; 3]; 5] =
    [[150, 145, 135], [175, 165, 145], [50, 95, 200], [205, 60, 45], [70, 165, 85]];

/// The material index of each object of [`reference_triangles`]: ground 0, wall 1, octahedron
/// 2, sphere 3, bunny 4.
pub const REFERENCE_MATERIAL_OF: [u32; 5] = [0, 1, 2, 3, 4];

/// The reference materials, [`REFERENCE_ALBEDO_256`] as Bunny albedos.
///
/// # Panics
///
/// Never: every channel is in `0..=256`, inside Bunny's `0..=1.0`.
#[must_use]
pub fn reference_materials() -> [Albedo; 5] {
    REFERENCE_ALBEDO_256
        .map(|channels| Albedo::new(channels.map(per_256)).expect("albedo channels are in 0..=1"))
}

/// The material index of each reference triangle, in primitive order.
#[must_use]
pub fn reference_material_of() -> Vec<u32> {
    let [ground, wall, octahedron, sphere, bunny] = REFERENCE_MATERIAL_OF;
    let mut material_of = Vec::with_capacity(REFERENCE_TRIANGLE_COUNT);
    material_of.extend([ground; 2]);
    material_of.extend([wall; 2]);
    material_of.extend([octahedron; 8]);
    material_of.resize(BUNNY_FIRST_PRIMITIVE, sphere);
    material_of.resize(REFERENCE_TRIANGLE_COUNT, bunny);
    material_of
}
