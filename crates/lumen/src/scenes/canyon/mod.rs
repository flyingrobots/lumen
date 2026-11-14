//! "Canyon", [`CANYON_SCENE_ID`]: a fractal canyon of integer terrain, three small craft
//! flying along it, the full-resolution Stanford bunny on a ledge, a low sun, a near-black sky
//! and a banded planet. Composition after a 1990s canyon-chase render.
//!
//! This module owns the scene's content, built from triangles only, in primitive order: the
//! terrain ([`terrain`], heights from [`heightfield`]), the craft ([`craft`]), the bunny
//! ([`bunny`]) and the planet ([`planet`]). [`content`] records which part each triangle belongs
//! to, its material and its vertex normals (the bunny's only), and [`look`] holds the albedos,
//! the sun, the sky and the camera.

pub mod bunny;
pub mod content;
pub mod craft;
pub mod error;
pub mod heightfield;
pub mod look;
pub mod planet;
pub mod terrain;

mod convex;

pub use content::{CanyonContent, Craft, Part, PartKind, PLANET_BANDS};
pub use convex::{Pose, Rational};
pub use error::CanyonError;
pub use look::{canyon_camera, canyon_lighting, canyon_materials};

use crate::scene::{Scene, SceneSpec};

/// The stable name and version of [`canyon_scene`]. Any change to its triangles, their order,
/// its materials, its vertex normals, its lighting or its camera is a new version and changes
/// every image hash of it.
pub const CANYON_SCENE_ID: &str = "lumen-canyon-scene/1";

/// The scene's triangles in primitive order, with each one's material, part and normals: the
/// terrain, the craft (each its hull, then its engines), the bunny and the planet's bands.
///
/// # Errors
///
/// A [`CanyonError`] if the heightfield is refused or the embedded bunny does not place; a test
/// shows neither happens.
pub fn canyon_content() -> Result<CanyonContent, CanyonError> {
    let mut content = CanyonContent::default();
    let heights = terrain::canyon_heights().map_err(CanyonError::Heightfield)?;
    let (ground, strata) = terrain::terrain_triangles(&heights);
    content.push_materials(PartKind::Terrain, &ground, &strata);
    craft::push_craft(&mut content);
    let bunny = bunny::ledge_bunny()?;
    content.push_smooth(PartKind::Bunny, &bunny.triangles, &bunny.normals);
    planet::push_planet(&mut content);
    Ok(content)
}

/// The canyon scene, [`CANYON_SCENE_ID`]: [`canyon_content`] with [`canyon_materials`] and
/// [`canyon_lighting`], seen by [`canyon_camera`].
///
/// # Errors
///
/// A [`CanyonError`] from [`canyon_content`], or [`CanyonError::Scene`] if [`Scene::new`]
/// refuses it. Tests show the scene builds.
pub fn canyon_scene() -> Result<Scene, CanyonError> {
    let content = canyon_content()?;
    Scene::new(&SceneSpec {
        triangles: &content.triangles,
        material_of: &content.material_of,
        materials: &canyon_materials()?,
        normals: &content.normals,
        lighting: canyon_lighting()?,
        camera: canyon_camera(),
    })
    .map_err(CanyonError::Scene)
}
