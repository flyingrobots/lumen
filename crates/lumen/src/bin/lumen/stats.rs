//! What `lumen stats` prints: a built scene's statistics as `key value` lines (ADR 0008).
//!
//! Each line is a key with no spaces, one space, and its value: one token, or several separated
//! by single spaces (a vector's three components). Fixed-point values are printed in decimal
//! with six places, rounded to nearest with ties away from zero, from their exact raw integers.
//! README, "Statistics", documents every key.

use core::fmt;
use std::io::{self, Write};

use bunny_linalg::FixedVec3;
use bunny_num::FixedQ32_32;
use lumen::{Integrator, Scene, SceneStatistics, TREE_BUILDER, TREE_LAYOUT};

/// Writes `lumen stats`: the scene's name, content identity and integrator, then its block.
pub(crate) fn write_stats(
    out: &mut impl Write,
    name: &str,
    scene: &Scene,
    stats: &SceneStatistics,
) -> io::Result<()> {
    writeln!(out, "scene {name}")?;
    writeln!(out, "scene-content {}", scene.content_id())?;
    writeln!(out, "integrator {}", Integrator::for_scene(scene).id())?;
    write_scene_block(out, scene, stats)
}

/// The label of every `memory.` line: a byte count is this build's type sizes times the counts,
/// so it can differ between targets (a 32-bit `usize`, for one), unlike every other value.
pub(crate) const MEMORY_NOTE: &str = "(information only: this build's type sizes)";

/// The scene's statistics: its contents, lights and camera, its tree, its memory.
pub(crate) fn write_scene_block(
    out: &mut impl Write,
    scene: &Scene,
    stats: &SceneStatistics,
) -> io::Result<()> {
    write_contents(out, scene, stats)?;
    write_lights_and_camera(out, scene)?;
    write_tree(out, stats)?;
    write_memory(out, stats)
}

fn write_contents(out: &mut impl Write, scene: &Scene, stats: &SceneStatistics) -> io::Result<()> {
    let triangles = stats.triangles;
    writeln!(out, "scene.triangles {triangles}")?;
    // Lumen's triangles hold their own three vertices; none is shared.
    writeln!(out, "scene.vertices {}", wide(triangles) * 3)?;
    writeln!(out, "scene.triangles-smooth {}", stats.smooth_triangles)?;
    writeln!(out, "scene.triangles-zero-area {}", stats.zero_area_triangles)?;
    writeln!(out, "scene.materials {}", stats.triangles_by_material.len())?;
    for (material, (count, albedo)) in
        stats.triangles_by_material.iter().zip(scene.materials()).enumerate()
    {
        writeln!(out, "scene.material.{material}.triangles {count}")?;
        writeln!(out, "scene.material.{material}.albedo {}", Channels(albedo.channels()))?;
    }
    writeln!(out, "scene.bounds-min {}", Vector(stats.bounds.min))?;
    writeln!(out, "scene.bounds-max {}", Vector(stats.bounds.max))
}

fn write_lights_and_camera(out: &mut impl Write, scene: &Scene) -> io::Result<()> {
    let lighting = scene.lighting();
    match lighting.sun {
        Some(sun) => {
            writeln!(out, "light.sun present")?;
            writeln!(out, "light.sun-toward {}", Vector(sun.toward_light()))?;
            writeln!(out, "light.sun-irradiance {}", Channels(sun.irradiance()))?;
        }
        None => writeln!(out, "light.sun none")?,
    }
    writeln!(out, "light.sky {}", Channels(lighting.sky.radiance()))?;
    let camera = scene.camera();
    writeln!(out, "camera.position {}", Vector(camera.position))?;
    writeln!(out, "camera.look {}", Vector(camera.look))?;
    writeln!(out, "camera.up {}", Vector(camera.up))?;
    writeln!(out, "camera.tan-half-fov {}", Fixed(camera.tan_half_fov))
}

fn write_tree(out: &mut impl Write, stats: &SceneStatistics) -> io::Result<()> {
    let tree = &stats.tree;
    writeln!(out, "tree.builder {TREE_BUILDER}")?;
    writeln!(out, "tree.layout {TREE_LAYOUT}")?;
    writeln!(out, "tree.nodes {}", tree.nodes)?;
    writeln!(out, "tree.interior-nodes {}", tree.interior_nodes)?;
    writeln!(out, "tree.leaves {}", tree.leaves)?;
    writeln!(out, "tree.leaf-depth-max {}", tree.max_leaf_depth)?;
    writeln!(out, "tree.leaf-depth-total {}", tree.leaf_depth_total)?;
    let leaves = wide(tree.leaves);
    writeln!(out, "tree.leaf-depth-mean {}", Ratio(u128::from(tree.leaf_depth_total), leaves))?;
    writeln!(out, "tree.leaf-triangles {}", tree.leaf_triangles)?;
    writeln!(out, "tree.leaf-triangles-min {}", tree.min_leaf_triangles)?;
    writeln!(out, "tree.leaf-triangles-max {}", tree.max_leaf_triangles)?;
    write!(out, "tree.leaf-sizes")?;
    for count in tree.leaf_sizes {
        write!(out, " {count}")?;
    }
    writeln!(out)
}

fn write_memory(out: &mut impl Write, stats: &SceneStatistics) -> io::Result<()> {
    let memory = &stats.memory;
    for (name, bytes) in [
        ("triangles", memory.triangles),
        ("surfaces", memory.surfaces),
        ("vertex-normals", memory.vertex_normals),
        ("material-indices", memory.material_indices),
        ("materials", memory.materials),
        ("tree-nodes", memory.tree_nodes),
        ("tree-primitive-indices", memory.tree_primitive_indices),
        ("total", memory.total()),
    ] {
        writeln!(out, "memory.{name}-bytes {bytes} {MEMORY_NOTE}")?;
    }
    Ok(())
}

/// A count, widened for arithmetic: lossless, as no target's `usize` is wider than 128 bits.
fn wide(count: usize) -> u128 {
    u128::try_from(count).unwrap_or(u128::MAX)
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

/// A point or direction: its three components, separated by spaces.
struct Vector(FixedVec3);

impl fmt::Display for Vector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", Fixed(self.0.x), Fixed(self.0.y), Fixed(self.0.z))
    }
}

/// Three channels, red, green and blue, separated by spaces.
struct Channels([FixedQ32_32; 3]);

impl fmt::Display for Channels {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b] = self.0;
        write!(f, "{} {} {}", Fixed(r), Fixed(g), Fixed(b))
    }
}

/// `numerator / denominator` in decimal with two places, rounded to nearest, ties up: exact
/// integer arithmetic. Zero over zero is `0.00`.
pub(crate) struct Ratio(pub(crate) u128, pub(crate) u128);

impl fmt::Display for Ratio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self(numerator, denominator) = *self;
        // A count of leaves or rays and a sum of them, each below 2^64: no product overflows.
        let hundredths = match denominator {
            0 => 0,
            d => (numerator * 200 + d) / (2 * d),
        };
        write!(f, "{}.{:02}", hundredths / 100, hundredths % 100)
    }
}

#[cfg(test)]
mod tests;
