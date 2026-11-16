//! The statistics messages (ADR 0008, ADR 0015): `scene_statistics`, which `lumen stats` and
//! `render --stats` print, and `ray_statistics`, `traversal_statistics` and `run_statistics`,
//! which `render --stats` adds.
//!
//! Every number is an integer. A fixed-point value is a string, in decimal with six places,
//! rounded to nearest with ties away from zero, from its exact raw integer, because a JSON
//! number would be read as a float. A mean is not printed: its total and its count are, and a
//! program divides. docs/reference/cli.md, "Statistics", documents every member.

use core::fmt;

use bunny_linalg::FixedVec3;
use bunny_num::FixedQ32_32;
use lumen::{
    Integrator, RayStatistics, RenderSettings, RunStatistics, Scene, SceneStatistics, TREE_BUILDER,
    TREE_LAYOUT,
};

use crate::json::{array, Object, Value};
use crate::messages::message;

/// The `scene_statistics` message: the scene's name, content identity and integrator, then its
/// contents, lights and camera, its tree and its memory.
pub(crate) fn scene_statistics(name: &str, scene: &Scene, stats: &SceneStatistics) -> Object {
    let object = message("scene_statistics")
        .with("scene", name)
        .with("scene_content", scene.content_id().to_string())
        .with("integrator", Integrator::for_scene(scene).id());
    let object = contents(object, scene, stats);
    let object = lights_and_camera(object, scene);
    object.with("tree", tree(stats)).with("memory", memory(stats))
}

fn contents(object: Object, scene: &Scene, stats: &SceneStatistics) -> Object {
    let triangles = stats.triangles;
    let albedos = scene.materials().iter().map(|albedo| channels(albedo.channels()));
    object
        .with("triangles", triangles)
        // Lumen's triangles hold their own three vertices; none is shared.
        .with("vertices", u64::try_from(triangles).unwrap_or(u64::MAX).saturating_mul(3))
        .with("triangles_smooth", stats.smooth_triangles)
        .with("triangles_zero_area", stats.zero_area_triangles)
        .with("material_triangles", array(stats.triangles_by_material.iter().copied()))
        .with("material_albedo", array(albedos))
        .with("bounds_min", vector(stats.bounds.min))
        .with("bounds_max", vector(stats.bounds.max))
}

fn lights_and_camera(object: Object, scene: &Scene) -> Object {
    let lighting = scene.lighting();
    let sun = lighting.sun.map(|sun| {
        Object::new()
            .with("toward", vector(sun.toward_light()))
            .with("irradiance", channels(sun.irradiance()))
    });
    let camera = scene.camera();
    object
        .with("light", Object::new().with("sun", sun).with("sky", channels(lighting.sky.radiance())))
        .with(
            "camera",
            Object::new()
                .with("position", vector(camera.position))
                .with("look", vector(camera.look))
                .with("up", vector(camera.up))
                .with("tan_half_fov", fixed(camera.tan_half_fov)),
        )
}

fn tree(stats: &SceneStatistics) -> Object {
    let tree = &stats.tree;
    Object::new()
        .with("builder", TREE_BUILDER)
        .with("layout", TREE_LAYOUT)
        .with("nodes", tree.nodes)
        .with("interior_nodes", tree.interior_nodes)
        .with("leaves", tree.leaves)
        .with("leaf_depth_max", tree.max_leaf_depth)
        .with("leaf_depth_total", tree.leaf_depth_total)
        .with("leaf_triangles", tree.leaf_triangles)
        .with("leaf_triangles_min", tree.min_leaf_triangles)
        .with("leaf_triangles_max", tree.max_leaf_triangles)
        .with("leaf_sizes", array(tree.leaf_sizes))
}

/// The byte counts: this build's type sizes times the counts, so they can differ between
/// targets (a 32-bit `usize`, for one), unlike every other value; the schema marks them
/// information only.
fn memory(stats: &SceneStatistics) -> Object {
    let memory = &stats.memory;
    Object::new()
        .with("triangles_bytes", memory.triangles)
        .with("surfaces_bytes", memory.surfaces)
        .with("vertex_normals_bytes", memory.vertex_normals)
        .with("material_indices_bytes", memory.material_indices)
        .with("materials_bytes", memory.materials)
        .with("tree_nodes_bytes", memory.tree_nodes)
        .with("tree_primitive_indices_bytes", memory.tree_primitive_indices)
        .with("total_bytes", memory.total())
}

/// The `ray_statistics` message (`render --stats`): what the rays found, per kind, then per
/// pixel. Every value is a function of the scene, the size and the settings.
pub(crate) fn ray_statistics(rays: &RayStatistics, settings: RenderSettings) -> Object {
    let found = &rays.outcomes;
    message("ray_statistics")
        .with("camera", found.camera())
        .with("camera_hits", found.camera_hits)
        .with("camera_misses", found.camera_misses)
        .with("bounce", found.bounce())
        .with("bounce_hits", found.bounce_hits)
        .with("bounce_misses", found.bounce_misses)
        .with("bounce_rejected", found.rejected_bounces)
        .with("shadow", found.shadow())
        .with("shadow_occluded", found.shadow_occluded)
        .with("shadow_unoccluded", found.shadow_unoccluded)
        .with("total", rays.count().total())
        .with("pixels", rays.pixels)
        .with("samples_per_pixel", settings.samples_per_pixel())
        .with("samples", rays.samples)
        .with("per_pixel_min", if rays.pixels == 0 { 0 } else { rays.min_pixel_rays })
        .with("per_pixel_max", rays.max_pixel_rays)
}

/// The `run_statistics` message (`render --stats`): the band height and the band count, which
/// follow from the size and the schedule, then each worker's bands, rows, pixels and rays, which
/// depend on timing (information only).
pub(crate) fn run_statistics(run: &RunStatistics) -> Object {
    let workers = run.workers.iter().map(|worker| {
        Object::new()
            .with("bands", worker.bands)
            .with("rows", worker.rows)
            .with("pixels", worker.pixels)
            .with("rays", worker.rays.total())
    });
    message("run_statistics")
        .with("band_rows", run.band_rows)
        .with("bands", run.bands)
        .with("workers", array(workers))
}

/// A Q32.32 value as a decimal string with six places.
pub(crate) fn fixed(value: FixedQ32_32) -> Value {
    Value::String(Fixed(value).to_string())
}

/// A point or direction: its three components.
fn vector(v: FixedVec3) -> Value {
    array([fixed(v.x), fixed(v.y), fixed(v.z)])
}

/// Three channels, red, green and blue.
fn channels(channels: [FixedQ32_32; 3]) -> Value {
    array(channels.map(fixed))
}

/// A Q32.32 value in decimal with six places: `raw / 2^32`, rounded to nearest, ties away from
/// zero, computed exactly in `i128`. The sign is printed only for a value that rounds to nonzero.
pub(crate) struct Fixed(pub(crate) FixedQ32_32);

impl fmt::Display for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const PLACES: u128 = 1_000_000;
        // |raw| < 2^63, so |raw| * 10^6 < 2^83: no overflow.
        let raw = self.0.raw();
        let millionths = (u128::from(raw.unsigned_abs()) * PLACES + (1 << 31)) >> 32;
        let sign = if raw < 0 && millionths > 0 { "-" } else { "" };
        write!(f, "{sign}{}.{:06}", millionths / PLACES, millionths % PLACES)
    }
}

#[cfg(test)]
mod tests;
mod traversal;

pub(crate) use traversal::traversal_statistics;
