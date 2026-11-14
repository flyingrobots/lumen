//! The views computed from the camera ray's first hit: one ray through each pixel's centre,
//! Bunny's closest hit, and a colour from what Bunny reports about it.

use bunny_geom::FixedAabb3;
use bunny_linalg::{FixedUnitVec3, FixedVec3};
use bunny_optics::{smooth_normal, PinholeCamera, PixelSample, PixelStream};

use super::{colour, View};
use crate::image::ImageSize;
use crate::render::{render_error, render_scheduled, PathError, RayOutcomes, RayStatistics};
use crate::render::{Render, RenderError, RenderSettings};
use crate::scene::{Hit, Scene, Tracer};
use crate::schedule::Schedule;
use crate::workers::render_pixels;

/// Renders `view` of `scene` at `size` on the workers of `schedule`.
///
/// [`View::Lit`] is [`render_scheduled`] with `settings`, exactly. Every other view traces one
/// camera ray per pixel, through the pixel's centre (Bunny's `PixelSample::centre`), whatever
/// `settings` asks: its [`RayStatistics`] count one sample per pixel. The view's pixel is a
/// function of the scene, the size and the pixel alone, so the image is the same for every
/// schedule.
///
/// # Errors
///
/// [`RenderError::Camera`] if Bunny refuses the camera at this size, and the failures of
/// [`render_scheduled`]: a pixel's [`PathError`] (`NoRay`, `Trace`, `UnknownPrimitive`,
/// `NoSurface`), memory and threads.
pub fn render_view(
    scene: &Scene,
    view: View,
    size: ImageSize,
    settings: RenderSettings,
    schedule: Schedule,
) -> Result<Render, RenderError> {
    if view == View::Lit {
        return render_scheduled(scene, size, settings, schedule);
    }
    let camera = scene.camera().camera(size.width(), size.height())?;
    let depth_range = depth_range(camera.position(), scene.bounds());
    let frame = Frame { scene, camera: &camera, view, depth_range };
    let (image, statistics) = render_pixels(size, schedule, RayStatistics::NONE, |x, y| {
        let (rgba, outcomes) = frame.pixel(x, y)?;
        Ok((rgba, RayStatistics::pixel(outcomes, 1)))
    })
    .map_err(render_error)?;
    Ok(Render { image, rays: statistics.count(), statistics })
}

/// What every pixel of one view render shares.
struct Frame<'a> {
    scene: &'a Scene,
    camera: &'a PinholeCamera,
    view: View,
    /// The depth view's range, raw Q32.32 ([`depth_range`]).
    depth_range: i128,
}

/// The seeds of Bunny's hash for the ID views: one per view, so a triangle and a material with
/// the same index get unrelated colours.
const PRIMITIVE_ID_SEED: u64 = 0x7072_696D_6974_6976; // "primitiv"
const MATERIAL_ID_SEED: u64 = 0x6D61_7465_7269_616C; // "material"

impl Frame<'_> {
    /// The colour of pixel `(x, y)`, and its one camera ray's outcome.
    fn pixel(&self, x: u32, y: u32) -> Result<([u8; 4], RayOutcomes), PathError> {
        let point = PixelSample::centre(x, y);
        let ray = self.camera.ray(point).ok_or(PathError::NoRay { sample: 0, point })?;
        let mut found = RayOutcomes::default();
        let Some(hit) = self.scene.closest(&ray)? else {
            found.camera_misses = 1;
            return Ok((colour::MISS, found));
        };
        found.camera_hits = 1;
        Ok((self.hit_colour(&hit)?, found))
    }

    /// The colour of a first hit.
    fn hit_colour(&self, hit: &Hit) -> Result<[u8; 4], PathError> {
        let (scene, primitive) = (self.scene, hit.primitive);
        let surface = scene.surface(primitive).ok_or(PathError::NoSurface { primitive })?;
        let geometric = surface.frame().normal();
        let front = hit.hit.front_face;
        Ok(match self.view {
            View::Lit | View::Normals => colour::normal(geometric, front),
            View::ShadingNormals => match interpolated(scene, hit) {
                Shading::Interpolated(normal) => colour::normal(normal.into_inner(), front),
                Shading::Cancelled => colour::NO_NORMAL,
                Shading::Geometric => colour::normal(geometric, front),
            },
            View::Depth => colour::depth(hit.hit.distance.raw(), self.depth_range),
            View::PrimitiveId => colour::id(hashed(PRIMITIVE_ID_SEED, primitive.get())),
            View::MaterialId => {
                let triangles = scene.triangles().len();
                let material = scene
                    .material_index(primitive)
                    .ok_or(PathError::UnknownPrimitive { primitive, triangles })?;
                colour::id(hashed(MATERIAL_ID_SEED, material))
            }
        })
    }
}

/// 64 bits of Bunny's `bunny-sample-hash/1` for `index` under `seed`: `PixelStream::new(seed,
/// index, 0).bits(0, 0)`, a pure function of the two.
fn hashed(seed: u64, index: u32) -> u64 {
    PixelStream::new(seed, index, 0).bits(0, 0)
}

/// The depth view's range: the largest difference, on any axis, between the eye and a corner of
/// the scene's box, in raw Q32.32. A camera ray's direction has no component above 1, so a hit at
/// distance `d` (in multiples of the direction) is at most `d` from the eye on every axis, and a
/// hit on the box's far corner along the camera's main axis is near this range.
fn depth_range(eye: FixedVec3, bounds: Option<FixedAabb3>) -> i128 {
    let Some(bounds) = bounds else { return 1 };
    let axis = |e: i64, lo: i64, hi: i64| {
        let (e, lo, hi) = (i128::from(e), i128::from(lo), i128::from(hi));
        (e - lo).abs().max((hi - e).abs())
    };
    let (min, max) = (bounds.min, bounds.max);
    axis(eye.x.raw(), min.x.raw(), max.x.raw())
        .max(axis(eye.y.raw(), min.y.raw(), max.y.raw()))
        .max(axis(eye.z.raw(), min.z.raw(), max.z.raw()))
        .max(1)
}

/// What shades a hit in the shading-normals view.
enum Shading {
    /// Bunny's `smooth_normal` of the triangle's three vertex normals at the hit.
    Interpolated(FixedUnitVec3),
    /// The three vertex normals cancel at the hit: `smooth_normal` gives none.
    Cancelled,
    /// The triangle lacks a vertex normal, where Bunny's smooth shading uses the geometric one.
    Geometric,
}

/// The hit's shading normal by Bunny's `smooth_normal` (`bunny-smooth-normal/1`) at the hit's
/// exact barycentric numerators, when its triangle has all three vertex normals.
fn interpolated(scene: &Scene, hit: &Hit) -> Shading {
    let Some(&[Some(a), Some(b), Some(c)]) = scene.vertex_normals(hit.primitive) else {
        return Shading::Geometric;
    };
    smooth_normal([a, b, c], hit.hit.barycentric_numerators())
        .map_or(Shading::Cancelled, Shading::Interpolated)
}
