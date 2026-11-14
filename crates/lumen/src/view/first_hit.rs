//! The views computed from the camera ray's first hit: one ray through each pixel's centre,
//! Bunny's closest hit, and a colour from what Bunny reports about it.

use bunny_linalg::FixedUnitVec3;
use bunny_optics::{smooth_normal, PinholeCamera, PixelSample};

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
    let (image, statistics) = render_pixels(size, schedule, RayStatistics::NONE, |x, y| {
        let (rgba, outcomes) = pixel(scene, &camera, view, x, y)?;
        Ok((rgba, RayStatistics::pixel(outcomes, 1)))
    })
    .map_err(render_error)?;
    Ok(Render { image, rays: statistics.count(), statistics })
}

/// The colour of pixel `(x, y)` in `view`, and its one camera ray's outcome.
fn pixel(
    scene: &Scene,
    camera: &PinholeCamera,
    view: View,
    x: u32,
    y: u32,
) -> Result<([u8; 4], RayOutcomes), PathError> {
    let point = PixelSample::centre(x, y);
    let ray = camera.ray(point).ok_or(PathError::NoRay { sample: 0, point })?;
    let mut found = RayOutcomes::default();
    let Some(hit) = scene.closest(&ray)? else {
        found.camera_misses = 1;
        return Ok((colour::MISS, found));
    };
    found.camera_hits = 1;
    Ok((hit_colour(scene, view, &hit)?, found))
}

/// The colour of a first hit in `view`.
fn hit_colour(scene: &Scene, view: View, hit: &Hit) -> Result<[u8; 4], PathError> {
    let primitive = hit.primitive;
    let surface = scene.surface(primitive).ok_or(PathError::NoSurface { primitive })?;
    let geometric = surface.frame().normal();
    let front = hit.hit.front_face;
    Ok(match view {
        View::Lit | View::Normals => colour::normal(geometric, front),
        View::ShadingNormals => match interpolated(scene, hit) {
            Shading::Interpolated(normal) => colour::normal(normal.into_inner(), front),
            Shading::Cancelled => colour::NO_NORMAL,
            Shading::Geometric => colour::normal(geometric, front),
        },
    })
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
