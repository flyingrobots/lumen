//! "Fixed Point", [`FIXED_POINT_SCENE_ID`]: the full-resolution Stanford bunny, smooth-shaded,
//! in dark sunglasses at a craps table, with two identical dice and the ON puck on the 8.
//!
//! In craps the point is the number the shooter must roll again. Both dice show four pips on
//! every face, so every roll is a hard eight and the point is fixed: a picture of determinism
//! for a ray tracer built on fixed-point arithmetic.
//!
//! This module owns the scene's content. It is built from triangles only, in primitive order:
//! the table ([`table`]: felt, rail, layout lines, numerals), the puck ([`puck`]), the two
//! dice ([`dice`]), the bunny ([`player`]) and its sunglasses ([`sunglasses`]). Every vertex is
//! a whole number of `2^-16` grid steps; [`content`] records which part each triangle belongs
//! to, its material and its vertex normals (the bunny's only, from [`full_bunny`]), and [`look`]
//! holds the albedos, the sun, the sky and the camera.

pub mod content;
pub mod dice;
pub mod error;
pub mod full_bunny;
pub mod look;
pub mod player;
pub mod puck;
pub mod sunglasses;
pub mod table;
pub mod turn;

mod numerals;
pub(crate) mod solids;

pub use content::{Die, FixedPointContent, Material, Part, PartKind, Side};
pub use error::FixedPointError;
pub use look::{fixed_point_camera, fixed_point_lighting, fixed_point_materials};
pub use solids::UNIT;

use crate::scene::{Scene, SceneSpec};

/// The stable name and version of [`fixed_point_scene`]. Any change to its triangles, their
/// order, its materials, its vertex normals, its lighting or its camera is a new version and
/// changes every image hash of it.
///
/// Version 1 (retired, flyingrobots/lumen#39) had the decimated bunny (`bun_zipper_res3.ply`,
/// 3 851 faces) shaded flat, which looked faceted close up, and a puck whose black side and
/// wide black rim made it read as a black puck. Version 2 has the full-resolution bunny shaded
/// smooth, its right glasses arm refitted, and a puck that reads as a white disc with black
/// lettering.
pub const FIXED_POINT_SCENE_ID: &str = "lumen-fixed-point-scene/2";

/// The scene's triangles in primitive order, with each one's material and part: the table,
/// the puck, the left die, the right die, the bunny and the sunglasses.
///
/// # Errors
///
/// A [`FixedPointError`] if the embedded bunny does not place or a die's turn is not exact;
/// a test shows neither happens.
pub fn fixed_point_content() -> Result<FixedPointContent, FixedPointError> {
    let mut content = FixedPointContent::default();
    table::push_table(&mut content);
    puck::push_puck(&mut content);
    dice::push_dice(&mut content)?;
    let bunny = player::placed_bunny()?;
    content.push_smooth(PartKind::Bunny, &bunny.triangles, &bunny.normals);
    sunglasses::push_sunglasses(&mut content, &bunny.face, &bunny.pose)?;
    Ok(content)
}

/// The Fixed Point scene, [`FIXED_POINT_SCENE_ID`]: [`fixed_point_content`] with
/// [`fixed_point_materials`] and [`fixed_point_lighting`], seen by [`fixed_point_camera`].
///
/// # Errors
///
/// A [`FixedPointError`] from [`fixed_point_content`], or [`FixedPointError::Scene`] if
/// [`Scene::new`] refuses it. Tests show the scene builds.
pub fn fixed_point_scene() -> Result<Scene, FixedPointError> {
    let content = fixed_point_content()?;
    let materials = fixed_point_materials()?;
    Scene::new(&SceneSpec {
        triangles: &content.triangles,
        material_of: &content.material_of,
        materials: &materials,
        normals: &content.normals,
        lighting: fixed_point_lighting()?,
        camera: fixed_point_camera(),
    })
    .map_err(FixedPointError::Scene)
}
