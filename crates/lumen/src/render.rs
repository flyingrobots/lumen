//! The integrator and the render loop: samples per pixel, each one diffuse path with a
//! shadow-casting directional light and one bounce, composed only of Bunny calls, then Bunny's
//! pixel path, on one or more threads.
//!
//! The image is a pure function of the scene and the settings. The [`Schedule`] (how many
//! threads, how large the tiles) decides only who computes a pixel and when, never its value:
//! every pixel's samples are accumulated for that pixel alone, by one worker, in its own
//! `PixelAccumulator`, so no accumulation order exists between threads.

use core::fmt;
use core::ops::Add;

use bunny_num::FixedQ32_32;
use bunny_optics::{
    write_rgba8, CameraError, PinholeCamera, PixelAccumulator, PixelError, PixelSampler,
    SurfaceHit, ToneCurve, MAX_SAMPLES_PER_PIXEL, SHADOW_T_MAX,
};
use bunny_query::TraceRay;

use crate::image::{ImageSize, Rgba8Image, SizeError};
use crate::scene::{Hit, Scene, SceneError, TraceError, Tracer};
use crate::schedule::Schedule;
use crate::workers::{render_pixels, WorkerError};

/// The stable name and version of lumen's integrator, [`trace_path`].
///
/// `lumen-diffuse-path/1` is a composition of named Bunny contracts and adds no arithmetic of
/// its own: camera rays from `PinholeCamera::ray`; sample positions and bounce points from
/// `bunny-sobol-owen/1` (pair 0 for the camera, pair 1 for the bounce); closest-hit and any-hit
/// queries over `TraceRay::hit_triangle` (minimum distance zero; `i64::MAX` raw for camera and
/// bounce rays, `SHADOW_T_MAX` for shadow rays); the surface hit, light sample, bounce
/// direction, sky and diffuse reflection of `bunny-diffuse-shading/1`, combined in that
/// contract's order of operations with at most one bounce; and the mean of
/// `PixelAccumulator`. The pixel value then goes through [`TONE_CURVE`] and `write_rgba8`
/// (`bunny-rgba8-opaque/1`). Geordi's integrator identity is meant to pin the same composition.
pub const INTEGRATOR_ID: &str = "lumen-diffuse-path/1";

/// The most bounces a path takes: one.
pub const MAX_BOUNCES: u32 = 1;

/// The tone curve every pixel goes through: Bunny's `bunny-tone-srgb/1`.
pub const TONE_CURVE: ToneCurve = ToneCurve::SrgbV1;

/// The sampler pair of the sub-pixel offset (Bunny's `PixelSampler::camera_sample`).
pub const CAMERA_PAIR: u32 = 0;

/// The sampler pair of the bounce direction.
pub const BOUNCE_PAIR: u32 = 1;

/// Checked render settings: the sample count per pixel and the seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderSettings {
    samples_per_pixel: u32,
    seed: u32,
}

/// Why render settings were refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsError {
    /// The sample count is not a power of two (zero included). Bunny's sampler states its
    /// stratification for aligned power-of-two blocks only.
    SamplesNotPowerOfTwo(u32),
    /// The sample count is above Bunny's `MAX_SAMPLES_PER_PIXEL` (1024).
    TooManySamples(u32),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SamplesNotPowerOfTwo(n) => {
                write!(f, "samples per pixel must be a power of two (1, 2, 4, ..., 1024), not {n}")
            }
            Self::TooManySamples(n) => {
                write!(f, "samples per pixel must be at most {MAX_SAMPLES_PER_PIXEL}, not {n}")
            }
        }
    }
}

impl std::error::Error for SettingsError {}

impl RenderSettings {
    /// Settings with `samples_per_pixel` samples, a power of two in `1..=1024`, and `seed`.
    ///
    /// The seed is a `u32`, the domain of Geordi's render settings; Bunny's sampler widens it
    /// to `u64`.
    ///
    /// # Errors
    ///
    /// [`SettingsError::TooManySamples`] above 1024, then
    /// [`SettingsError::SamplesNotPowerOfTwo`] for zero or any other count that is not a power
    /// of two.
    pub const fn new(samples_per_pixel: u32, seed: u32) -> Result<Self, SettingsError> {
        if samples_per_pixel > MAX_SAMPLES_PER_PIXEL {
            return Err(SettingsError::TooManySamples(samples_per_pixel));
        }
        if !samples_per_pixel.is_power_of_two() {
            return Err(SettingsError::SamplesNotPowerOfTwo(samples_per_pixel));
        }
        Ok(Self { samples_per_pixel, seed })
    }

    /// The samples per pixel.
    #[must_use]
    pub const fn samples_per_pixel(self) -> u32 {
        self.samples_per_pixel
    }

    /// The seed.
    #[must_use]
    pub const fn seed(self) -> u32 {
        self.seed
    }

    /// Bunny's sampler for pixel `(x, y)` under this seed.
    #[must_use]
    pub const fn sampler(self, x: u32, y: u32) -> PixelSampler {
        PixelSampler::new(self.seed as u64, x, y)
    }
}

/// Why a render failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderError {
    /// The image size was refused.
    Size(SizeError),
    /// The scene could not be built.
    Scene(SceneError),
    /// Bunny refused the camera.
    Camera(CameraError),
    /// The camera gave no ray for a pixel sample inside the image.
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
    /// A shading step refused its input: a hit on a triangle without a surface, a start point
    /// outside the trace domain, a sampler point outside `[0, 1)` or a radiance sum that is not
    /// representable. None of these happens for a scene built by `Scene::new` inside the trace
    /// domain.
    Shading,
    /// Memory for the image could not be reserved.
    OutOfMemory,
    /// The operating system refused to start a worker thread.
    Spawn,
    /// A tile did not fit the image (an internal invariant of the scheduler, never expected).
    TileGeometry,
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
            Self::Shading => f.write_str("a shading step refused its input"),
            Self::OutOfMemory => f.write_str("not enough memory for the image"),
            Self::Spawn => f.write_str("a worker thread could not be started"),
            Self::TileGeometry => f.write_str("a tile did not fit the image (a lumen bug)"),
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

/// How many rays were traced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RayCount {
    /// Closest-hit queries: camera rays and bounce rays.
    pub closest: u64,
    /// Any-hit queries: shadow rays.
    pub shadow: u64,
}

impl RayCount {
    /// Every ray traced.
    #[must_use]
    pub const fn total(self) -> u64 {
        self.closest + self.shadow
    }
}

impl Add for RayCount {
    type Output = Self;

    /// The rays of both. Ray counts of a render stay far below `u64::MAX` (an image has at most
    /// 2^27 pixels of at most 1024 samples of at most five rays), so the sums never saturate.
    fn add(self, other: Self) -> Self {
        Self {
            closest: self.closest.saturating_add(other.closest),
            shadow: self.shadow.saturating_add(other.shadow),
        }
    }
}

/// The radiance of one sample and the rays it traced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathSample {
    /// Linear radiance per channel.
    pub radiance: [FixedQ32_32; 3],
    /// The rays traced for it.
    pub rays: RayCount,
}

const ZERO: [FixedQ32_32; 3] = [FixedQ32_32::ZERO; 3];

/// `lumen-diffuse-path/1`: the radiance of sample `sample` of the pixel `sampler` belongs to.
///
/// This is Bunny's order of operations for one path (`docs/topics/optics/README.md` in Bunny,
/// "Order Of Operations"), step for step, with `tracer` answering the ray queries:
///
/// 1. `ray0 = camera.ray(sampler.camera_sample(sample))` (pair 0);
/// 2. `closest(ray0)`; a miss is the sky's radiance, and the path ends;
/// 3. `h1 = SurfaceHit::new(surface[p1], ray0, hit1)`: the facing frame and the start point of
///    every secondary ray;
/// 4. `D1`: `h1.light_sample(sun)`, visible when it has a shadow ray and `occluded(shadow ray,
///    SHADOW_T_MAX)` is false (no sun: zero);
/// 5. `ray1 = h1.bounce_ray(sampler.point(sample, 1))` (pair 1); `closest(ray1)`: a miss gives
///    `B = sky`; a hit gives `h2`, its direct term `D2` as in step 4, and
///    `B = albedo[p2].reflect(D2, 0)`;
/// 6. `L = albedo[p1].reflect(D1, B)`.
///
/// Secondary rays start at Bunny's start point and are queried with a minimum distance of zero
/// and no primitive excluded. The result depends only on the scene, the camera, the sampler's
/// seed and pixel, and `sample`: no state is shared between calls.
///
/// # Errors
///
/// [`RenderError::NoRay`] for a pixel outside the camera's image, [`RenderError::Trace`] if a
/// traversal fails, and [`RenderError::Shading`] if a Bunny shading step refuses its input.
pub fn trace_path<T: Tracer + ?Sized>(
    scene: &Scene,
    tracer: &T,
    camera: &PinholeCamera,
    sampler: PixelSampler,
    sample: u32,
) -> Result<PathSample, RenderError> {
    let lighting = scene.lighting();
    let mut count = RayCount { closest: 1, shadow: 0 };
    let ray0 = camera
        .ray(sampler.camera_sample(sample))
        .ok_or(RenderError::NoRay { x: sampler.x(), y: sampler.y() })?;
    let Some(hit1) = tracer.closest(&ray0)? else {
        return Ok(PathSample { radiance: lighting.sky.radiance(), rays: count });
    };
    let h1 = surface_hit(scene, &ray0, &hit1)?;
    let d1 = direct(scene, tracer, &h1, &mut count)?;
    let ray1 = h1.bounce_ray(sampler.point(sample, BOUNCE_PAIR)).ok_or(RenderError::Shading)?;
    count.closest += 1;
    let b = match tracer.closest(&ray1)? {
        None => lighting.sky.radiance(),
        Some(hit2) => {
            let h2 = surface_hit(scene, &ray1, &hit2)?;
            let d2 = direct(scene, tracer, &h2, &mut count)?;
            albedo(scene, hit2.primitive)?.reflect(d2, ZERO).ok_or(RenderError::Shading)?
        }
    };
    let radiance = albedo(scene, hit1.primitive)?.reflect(d1, b).ok_or(RenderError::Shading)?;
    Ok(PathSample { radiance, rays: count })
}

/// Bunny's `SurfaceHit::new` for a hit the tracer reported.
fn surface_hit(scene: &Scene, ray: &TraceRay, hit: &Hit) -> Result<SurfaceHit, RenderError> {
    let surface = scene.surface(hit.primitive).ok_or(RenderError::Shading)?;
    SurfaceHit::new(surface, ray, &hit.hit).ok_or(RenderError::Shading)
}

/// The direct term at a surface hit: Bunny's light sample, made visible by an any-hit query of
/// its shadow ray from the start point with `SHADOW_T_MAX`. Zero without a sun.
fn direct<T: Tracer + ?Sized>(
    scene: &Scene,
    tracer: &T,
    hit: &SurfaceHit,
    rays: &mut RayCount,
) -> Result<[FixedQ32_32; 3], RenderError> {
    let Some(sun) = scene.lighting().sun else { return Ok(ZERO) };
    let light = hit.light_sample(&sun);
    let visible = match light.shadow_ray() {
        Some(shadow) => {
            rays.shadow += 1;
            !tracer.occluded(shadow, SHADOW_T_MAX)?
        }
        None => false,
    };
    Ok(light.radiance(visible))
}

fn albedo(scene: &Scene, primitive: u32) -> Result<&bunny_optics::Albedo, RenderError> {
    scene.albedo(primitive).ok_or(RenderError::Shading)
}

/// The resolved linear radiance of pixel `(x, y)` and the rays it traced: samples `0` to
/// `samples_per_pixel - 1` of [`trace_path`] with the scene as its own tracer, each added to a
/// Bunny `PixelAccumulator`, then `resolve` (the mean, rounded to nearest, ties to even).
///
/// The accumulator's sums are exact integer sums, so the result does not depend on the order
/// the samples are evaluated in; this function holds no state between calls.
///
/// # Errors
///
/// Any error of [`trace_path`] or of the accumulator.
pub fn pixel_radiance(
    scene: &Scene,
    camera: &PinholeCamera,
    settings: RenderSettings,
    x: u32,
    y: u32,
) -> Result<([FixedQ32_32; 3], RayCount), RenderError> {
    Frame { scene, tracer: scene, camera, settings }.radiance(x, y)
}

/// A rendered image and the rays it took.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Render {
    /// The image.
    pub image: Rgba8Image,
    /// The rays traced.
    pub rays: RayCount,
}

/// The pixels of one render, shared read-only by every worker: the scene, the tracer that
/// answers its ray queries, the camera at the image size and the settings.
struct Frame<'a, T: ?Sized> {
    scene: &'a Scene,
    tracer: &'a T,
    camera: &'a PinholeCamera,
    settings: RenderSettings,
}

impl<T: Tracer + ?Sized> Frame<'_, T> {
    /// [`pixel_radiance`] with the frame's tracer.
    fn radiance(&self, x: u32, y: u32) -> Result<([FixedQ32_32; 3], RayCount), RenderError> {
        let sampler = self.settings.sampler(x, y);
        let mut accumulator = PixelAccumulator::new();
        let mut rays = RayCount::default();
        for sample in 0..self.settings.samples_per_pixel() {
            let path = trace_path(self.scene, self.tracer, self.camera, sampler, sample)?;
            accumulator.add_sample(path.radiance)?;
            rays = rays + path.rays;
        }
        Ok((accumulator.resolve()?, rays))
    }

    /// The four RGBA8 bytes of pixel `(x, y)`: its radiance through [`TONE_CURVE`] and
    /// `write_rgba8`.
    fn rgba(&self, x: u32, y: u32) -> Result<([u8; 4], RayCount), RenderError> {
        let (radiance, rays) = self.radiance(x, y)?;
        let mut rgba = [0_u8; 4];
        write_rgba8(&mut rgba, TONE_CURVE, radiance)?;
        Ok((rgba, rays))
    }
}

/// Renders `scene` at `size` with `settings` on the workers of `schedule`, with `tracer`
/// answering the ray queries.
///
/// Each pixel is the radiance of [`pixel_radiance`] (with `tracer` in place of the scene),
/// written by `write_rgba8` through [`TONE_CURVE`]: opaque RGBA8, row-major, row 0 at the top.
/// The scene and the tracer are shared read-only by every worker. [`render_pixels`] decides who
/// computes which pixel and when, never its value: every pixel's samples go into its own
/// `PixelAccumulator`, so the image and the ray count are the same for every schedule.
///
/// # Errors
///
/// [`RenderError::Camera`] if Bunny refuses the camera at this size; the error of
/// [`pixel_radiance`] or `write_rgba8` for the first failing pixel in raster order;
/// [`RenderError::OutOfMemory`] if a buffer cannot be reserved; [`RenderError::Spawn`] if a
/// worker thread cannot be started.
pub fn render_traced<T: Tracer + Sync + ?Sized>(
    scene: &Scene,
    tracer: &T,
    size: ImageSize,
    settings: RenderSettings,
    schedule: Schedule,
) -> Result<Render, RenderError> {
    let camera = scene.camera().camera(size.width(), size.height())?;
    let frame = Frame { scene, tracer, camera: &camera, settings };
    match render_pixels(size, schedule, RayCount::default(), |x, y| frame.rgba(x, y)) {
        Ok((image, rays)) => Ok(Render { image, rays }),
        Err(WorkerError::Pixel { error, .. }) => Err(error),
        Err(WorkerError::OutOfMemory) => Err(RenderError::OutOfMemory),
        Err(WorkerError::Spawn) => Err(RenderError::Spawn),
        Err(WorkerError::TileGeometry) => Err(RenderError::TileGeometry),
    }
}

/// Renders `scene` at `size` with `settings` on the workers of `schedule`: [`render_traced`]
/// with the scene as its own tracer.
///
/// # Errors
///
/// Any error of [`render_traced`].
///
/// # Examples
///
/// The same image on three threads with 4 by 4 tiles as on one thread:
///
/// ```
/// use lumen::{reference_scene, render, render_scheduled, ImageSize, RenderSettings};
/// use lumen::{Schedule, TileSize};
///
/// let scene = reference_scene()?;
/// let size = ImageSize::new(16, 9)?;
/// let settings = RenderSettings::new(1, 0)?;
/// let schedule = Schedule::new(3, TileSize::new(4, 4)?)?;
/// let threaded = render_scheduled(&scene, size, settings, schedule)?;
/// assert_eq!(threaded, render(&scene, size, settings)?);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn render_scheduled(
    scene: &Scene,
    size: ImageSize,
    settings: RenderSettings,
    schedule: Schedule,
) -> Result<Render, RenderError> {
    render_traced(scene, scene, size, settings, schedule)
}

/// Renders `scene` at `size` with `settings` on one thread: [`render_scheduled`] with
/// [`Schedule::single`].
///
/// Each pixel is [`pixel_radiance`], written by `write_rgba8` through [`TONE_CURVE`]: opaque
/// RGBA8, row-major, row 0 at the top. Each pixel is a pure function of its coordinates, so any
/// order, any split into tiles and any number of threads give the same bytes.
///
/// # Errors
///
/// Any error of [`render_traced`].
pub fn render(
    scene: &Scene,
    size: ImageSize,
    settings: RenderSettings,
) -> Result<Render, RenderError> {
    render_scheduled(scene, size, settings, Schedule::single())
}

/// The scene is shared by reference between worker threads, so it and everything reachable from
/// it must be `Sync` as it is, with no lock or wrapper. This fails to compile if it is not.
const _: () = {
    const fn shared_read_only<T: Sync + ?Sized>() {}
    shared_read_only::<Scene>();
};

/// Renders the built-in reference scene at `width` by `height` with `settings`.
///
/// # Errors
///
/// [`RenderError::Size`] for a size outside the limits of [`ImageSize::new`], and any
/// error of [`render`].
pub fn render_reference(
    width: u32,
    height: u32,
    settings: RenderSettings,
) -> Result<Render, RenderError> {
    let size = ImageSize::new(width, height)?;
    render(&crate::reference::reference_scene()?, size, settings)
}
