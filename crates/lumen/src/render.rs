//! The render loop: one camera ray per pixel, the closest hit, a shading rule, and Bunny's
//! pixel path, on one thread.

use core::fmt;

use bunny_num::fixed_q32_32::ONE_RAW;
use bunny_num::FixedQ32_32;
use bunny_optics::{
    write_rgba8, CameraError, PinholeCamera, PixelAccumulator, PixelError, PixelSample, ToneCurve,
};
use bunny_query::TraceRay;

use crate::image::{ImageSize, Rgba8Image, SizeError};
use crate::scene::{Hit, Scene, SceneError, TraceError};

/// The stable name and version of the shading rule, [`shade`].
pub const SHADING_ID: &str = "lumen-normal-shading/1";

/// The tone curve every pixel goes through: Bunny's `bunny-tone-srgb/1`.
pub const TONE_CURVE: ToneCurve = ToneCurve::SrgbV1;

/// The linear radiance of a ray that hits nothing: red 0, green 0, blue 0.25.
///
/// No normal maps to it: a red channel of 0 needs the normal `(-1, 0, 0)`, whose green
/// channel is 0.5. Through `bunny-tone-srgb/1` it is the code `(0, 0, 137)`.
pub const SKY: [FixedQ32_32; 3] =
    [FixedQ32_32::ZERO, FixedQ32_32::ZERO, FixedQ32_32::from_raw(ONE_RAW / 4)];

const HALF: FixedQ32_32 = FixedQ32_32::from_raw(ONE_RAW / 2);

/// Why a render failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderError {
    /// The image size was refused.
    Size(SizeError),
    /// The scene could not be built.
    Scene(SceneError),
    /// Bunny refused the camera.
    Camera(CameraError),
    /// The camera gave no ray for a pixel inside the image.
    NoRay {
        /// The pixel column.
        x: u32,
        /// The pixel row.
        y: u32,
    },
    /// A ray could not be traced.
    Trace(TraceError),
    /// Bunny's pixel path refused a value.
    Pixel(PixelError),
    /// A shading value was not representable.
    Shading,
    /// Memory for the image could not be reserved.
    OutOfMemory,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Size(error) => write!(f, "invalid image size: {error}"),
            Self::Scene(error) => write!(f, "invalid scene: {error}"),
            Self::Camera(error) => write!(f, "invalid camera: {error}"),
            Self::NoRay { x, y } => write!(f, "the camera gave no ray for pixel ({x}, {y})"),
            Self::Trace(error) => write!(f, "{error}"),
            Self::Pixel(error) => write!(f, "pixel value refused: {error}"),
            Self::Shading => f.write_str("a shading value was not representable"),
            Self::OutOfMemory => f.write_str("not enough memory for the image"),
        }
    }
}

impl std::error::Error for RenderError {}

impl From<SizeError> for RenderError {
    fn from(error: SizeError) -> Self {
        Self::Size(error)
    }
}

impl From<SceneError> for RenderError {
    fn from(error: SceneError) -> Self {
        Self::Scene(error)
    }
}

impl From<CameraError> for RenderError {
    fn from(error: CameraError) -> Self {
        Self::Camera(error)
    }
}

impl From<TraceError> for RenderError {
    fn from(error: TraceError) -> Self {
        Self::Trace(error)
    }
}

impl From<PixelError> for RenderError {
    fn from(error: PixelError) -> Self {
        Self::Pixel(error)
    }
}

/// The camera ray through the centre of pixel `(x, y)`: Bunny's `PinholeCamera::ray` for
/// `PixelSample::centre(x, y)` (top-left origin, `y` down).
///
/// # Errors
///
/// [`RenderError::NoRay`] when the pixel is outside the camera's image.
pub fn pixel_ray(camera: &PinholeCamera, x: u32, y: u32) -> Result<TraceRay, RenderError> {
    camera.ray(PixelSample::centre(x, y)).ok_or(RenderError::NoRay { x, y })
}

/// The nearest hit of the camera ray through the centre of pixel `(x, y)`.
///
/// # Errors
///
/// [`RenderError::NoRay`] for a pixel outside the image, and [`RenderError::Trace`] if the
/// traversal fails.
pub fn trace_pixel(
    scene: &Scene,
    camera: &PinholeCamera,
    x: u32,
    y: u32,
) -> Result<Option<Hit>, RenderError> {
    Ok(scene.closest(&pixel_ray(camera, x, y)?)?)
}

/// `lumen-normal-shading/1`: the linear radiance of one camera ray.
///
/// A miss is [`SKY`]. A hit is the triangle's geometric normal `n` (see `Scene::new`),
/// negated when the ray arrives at the back face so that it faces the ray, mapped to colour
/// channel by channel as `(n + 1) / 2`: `x` to red, `y` to green, `z` to blue. The addition
/// is exact and the halving is Bunny's Q32.32 multiplication by 0.5 (rounded to nearest,
/// ties to even). Each normal component lies in `-1.0..=1.0`, so each channel lies in
/// `0.0..=1.0`. No lighting is computed: lighting arrives with Bunny's shading step
/// (flyingrobots/bunny#160) in flyingrobots/lumen#3.
///
/// # Errors
///
/// [`RenderError::Shading`] when the hit names a triangle the scene does not hold, or a
/// value is not representable; neither happens for a hit the scene reported.
pub fn shade(scene: &Scene, hit: Option<Hit>) -> Result<[FixedQ32_32; 3], RenderError> {
    let Some(hit) = hit else { return Ok(SKY) };
    let index = usize::try_from(hit.primitive).map_err(|_| RenderError::Shading)?;
    let normal = *scene.normals().get(index).ok_or(RenderError::Shading)?;
    let facing = if hit.front_face { normal } else { -normal };
    let channel = |c: FixedQ32_32| c.checked_add(FixedQ32_32::ONE)?.checked_mul(HALF);
    match (channel(facing.x), channel(facing.y), channel(facing.z)) {
        (Some(r), Some(g), Some(b)) => Ok([r, g, b]),
        _ => Err(RenderError::Shading),
    }
}

/// Renders `scene` at `size` on one thread.
///
/// Each pixel gets one sample: the camera ray through its centre ([`trace_pixel`]), shaded
/// by [`shade`], added to a Bunny `PixelAccumulator`, resolved (the mean of one sample is
/// the sample) and written by `write_rgba8` through [`TONE_CURVE`]: opaque RGBA8, the
/// encoding later slices with more samples use.
///
/// # Errors
///
/// [`RenderError::Camera`] if Bunny refuses the camera at this size,
/// [`RenderError::OutOfMemory`] if the buffer cannot be reserved, and any error of
/// [`trace_pixel`], [`shade`] or the pixel path.
pub fn render(scene: &Scene, size: ImageSize) -> Result<Rgba8Image, RenderError> {
    let camera = scene.camera().camera(size.width(), size.height())?;
    let len = size.byte_len().ok_or(RenderError::OutOfMemory)?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(len).map_err(|_| RenderError::OutOfMemory)?;
    bytes.resize(len, 0);
    let row_len = len / usize::try_from(size.height()).map_err(|_| RenderError::OutOfMemory)?;
    for (y, row) in (0..size.height()).zip(bytes.chunks_exact_mut(row_len)) {
        for (x, pixel) in (0..size.width()).zip(row.chunks_exact_mut(4)) {
            let radiance = shade(scene, trace_pixel(scene, &camera, x, y)?)?;
            let mut accumulator = PixelAccumulator::new();
            accumulator.add_sample(radiance)?;
            write_rgba8(pixel, TONE_CURVE, accumulator.resolve()?)?;
        }
    }
    Rgba8Image::from_bytes(size, bytes).ok_or(RenderError::OutOfMemory)
}

/// Renders the built-in reference scene at `width` by `height`.
///
/// # Errors
///
/// [`RenderError::Size`] for a size outside the limits of [`ImageSize::new`], and any
/// error of [`render`].
pub fn render_reference(width: u32, height: u32) -> Result<Rgba8Image, RenderError> {
    let size = ImageSize::new(width, height)?;
    render(&crate::scene::reference_scene()?, size)
}
