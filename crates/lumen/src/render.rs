//! The integrator and the render loop: samples per pixel, each one diffuse path with a
//! shadow-casting directional light and one bounce, composed only of Bunny calls, then Bunny's
//! pixel path, on one or more threads.
//!
//! The image is a pure function of the scene and the settings. The [`Schedule`] (how many
//! threads, how tall the row bands) decides only who computes a pixel and when, never its value:
//! every pixel's samples are accumulated for that pixel alone, by one worker, in its own
//! `PixelAccumulator`, so no accumulation order exists between threads.

use core::fmt;
use core::ops::Add;

use bunny_num::FixedQ32_32;
use bunny_optics::{
    write_rgba8, PinholeCamera, PixelAccumulator, PixelSampler, ToneCurve, MAX_SAMPLES_PER_PIXEL,
    SHADING_ID,
};

use crate::image::{ImageSize, Rgba8Image};
use crate::scene::{CountingTracer, Scene, Tracer};
use crate::schedule::Schedule;
use crate::workers::{render_pixels, render_pixels_by_worker, Reservation, WorkerError, WorkerRun};

mod error;
mod probe;
mod run;
mod shade;
mod statistics;
mod traversal;

pub use error::{PathError, RenderError};
use probe::{Closest, Counted, Plain, Probe, Unmeasured};
pub use run::{RunStatistics, WorkerStatistics};
use shade::{direct, Shade};
pub use statistics::{RayCheckError, RayOutcomes, RayStatistics};
pub use traversal::{TraversalStatistics, TraversalWork};

/// The stable name and version of [`trace_path`] on a scene with no vertex normal
/// ([`Integrator::DiffusePath`]).
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

/// The stable name and version of [`trace_path`] on a scene with at least one vertex normal
/// ([`Integrator::SmoothDiffusePath`]).
///
/// `lumen-smooth-diffuse-path/1` is `lumen-diffuse-path/1` with one change, made per triangle: at
/// a hit on a triangle with at least one vertex normal (`Scene::vertex_normals`), the hit is
/// shaded by Bunny's `bunny-smooth-shading/1` instead of the flat steps. That is
/// `SmoothHit::new(surface_hit, hit.barycentric_numerators(), normals)` (which interpolates the
/// normals with `smooth_normal`, `bunny-smooth-normal/1`), then `SmoothHit::light_sample` for the
/// direct term and `SmoothHit::bounce` for the bounce: a ray as before, or, when the direction is
/// at or below the geometric surface, no ray and `B = 0`. Everything else (the camera ray, the
/// sampler pairs, the queries, the surface hit and its start point, the shadow ray, the
/// reflection, the order of operations) is `lumen-diffuse-path/1`'s, and a triangle without a
/// vertex normal is shaded exactly as by it. It adds no arithmetic of its own.
pub const SMOOTH_INTEGRATOR_ID: &str = "lumen-smooth-diffuse-path/1";

/// The provenance's `shading` field for a scene shaded by [`SMOOTH_INTEGRATOR_ID`]: the Bunny
/// shading contracts its paths use, joined by `+` in the order they apply at a smooth hit.
/// `bunny-diffuse-shading/1` (the surface hit, start point, shadow ray and reflection, and every
/// flat hit), then `bunny-smooth-shading/1` (`SmoothHit`), then `bunny-smooth-normal/1` (the
/// interpolation `SmoothHit::new` calls). Each part is Bunny's constant (`SHADING_ID`,
/// `SMOOTH_SHADING_ID`, `SMOOTH_NORMAL_ID`), and a test holds the string to them.
///
/// `bunny-vertex-normals/1` is not part of it: vertex normals are an input the scene supplies,
/// however it computed them, and the content digest covers their values (ADR 0007).
pub const SMOOTH_SHADING_IDS: &str =
    "bunny-diffuse-shading/1+bunny-smooth-shading/1+bunny-smooth-normal/1";

/// Which composition [`trace_path`] is for a scene, and its stable name: the provenance's
/// `integrator` field, and the Bunny shading contracts it uses: the `shading` field (ADR 0007).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Integrator {
    /// [`INTEGRATOR_ID`]: no triangle of the scene has a vertex normal.
    DiffusePath,
    /// [`SMOOTH_INTEGRATOR_ID`]: at least one triangle has a vertex normal.
    SmoothDiffusePath,
}

impl Integrator {
    /// The integrator [`trace_path`] is for `scene`: smooth exactly when at least one triangle
    /// has a vertex normal ([`Scene::smooth_triangles`] is nonzero).
    #[must_use]
    pub const fn for_scene(scene: &Scene) -> Self {
        if scene.smooth_triangles() == 0 {
            Self::DiffusePath
        } else {
            Self::SmoothDiffusePath
        }
    }

    /// The stable name and version.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::DiffusePath => INTEGRATOR_ID,
            Self::SmoothDiffusePath => SMOOTH_INTEGRATOR_ID,
        }
    }

    /// The Bunny shading contracts the integrator uses, the provenance's `shading` field:
    /// Bunny's `SHADING_ID` (`bunny-diffuse-shading/1`) alone for [`Integrator::DiffusePath`],
    /// [`SMOOTH_SHADING_IDS`] for [`Integrator::SmoothDiffusePath`].
    #[must_use]
    pub const fn shading_id(self) -> &'static str {
        match self {
            Self::DiffusePath => SHADING_ID,
            Self::SmoothDiffusePath => SMOOTH_SHADING_IDS,
        }
    }
}

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
    pub fn sampler(self, x: u32, y: u32) -> PixelSampler {
        PixelSampler::new(u64::from(self.seed), x, y)
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
        // At most 2^39 rays in all (see `Add`), so the sum never saturates.
        self.closest.saturating_add(self.shadow)
    }
}

impl Add for RayCount {
    type Output = Self;

    /// The rays of both. Ray counts of a render stay far below `u64::MAX` (an image has at most
    /// 2^27 pixels of at most 1024 samples of at most four rays: the camera ray, the bounce ray
    /// and a shadow ray at each of their hits, so at most 2^39 rays), so the sums never
    /// saturate.
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
    /// The rays traced for it: `outcomes.count()`.
    pub rays: RayCount,
    /// What each of its rays found (ADR 0008).
    pub outcomes: RayOutcomes,
}

impl PathSample {
    const fn new(radiance: [FixedQ32_32; 3], outcomes: RayOutcomes) -> Self {
        Self { radiance, rays: outcomes.count(), outcomes }
    }
}

const ZERO: [FixedQ32_32; 3] = [FixedQ32_32::ZERO; 3];

/// `lumen-diffuse-path/1`, or `lumen-smooth-diffuse-path/1` on a scene with vertex normals
/// ([`Integrator::for_scene`]): the radiance of sample `sample` of the pixel `sampler` belongs to.
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
/// At a hit on a triangle with a vertex normal, steps 3 to 5 use Bunny's `SmoothHit` (see
/// [`SMOOTH_INTEGRATOR_ID`]): its light sample, and its bounce, which may be rejected as below the
/// geometric surface; then no bounce ray is traced and `B = 0`.
///
/// Secondary rays start at Bunny's start point and are queried with a minimum distance of zero
/// and no primitive excluded. The result depends only on the scene, the camera, the sampler's
/// seed and pixel, and `sample`: no state is shared between calls.
///
/// # Errors
///
/// [`PathError::NoRay`] for a pixel outside the camera's image and [`PathError::Trace`] if a
/// query fails. For a hit the tracer reports, [`PathError::UnknownPrimitive`] past the scene's
/// triangles, [`PathError::NoSurface`] on a zero-area triangle and [`PathError::SurfaceHit`]
/// with Bunny's reason when Bunny refuses the hit; then [`PathError::BounceRay`] and
/// [`PathError::RadianceOverflow`], which no input reaches. The scene as its own tracer reports
/// only hits Bunny accepts.
pub fn trace_path<T: Tracer + ?Sized>(
    scene: &Scene,
    tracer: &T,
    camera: &PinholeCamera,
    sampler: PixelSampler,
    sample: u32,
) -> Result<PathSample, PathError> {
    path(scene, &Plain(tracer), camera, sampler, sample).map(|(found, Unmeasured)| found)
}

/// [`trace_path`] with `probe` answering the ray queries, and the traversal work it recorded.
fn path<P: Probe + ?Sized>(
    scene: &Scene,
    probe: &P,
    camera: &PinholeCamera,
    sampler: PixelSampler,
    sample: u32,
) -> Result<(PathSample, P::Work), PathError> {
    let lighting = scene.lighting();
    // A path adds at most one to each camera and bounce count and two to the shadow counts, so
    // no count saturates.
    let mut found = RayOutcomes::default();
    let mut work = P::Work::default();
    let point = sampler.camera_sample(sample);
    let ray0 = camera.ray(point).ok_or(PathError::NoRay { sample, point })?;
    let Some(hit1) = probe.closest(&ray0, Closest::Camera, &mut work)? else {
        found.camera_misses = 1;
        return Ok((PathSample::new(lighting.sky.radiance(), found), work));
    };
    found.camera_hits = 1;
    let h1 = Shade::at(scene, &ray0, &hit1)?;
    let d1 = direct(scene, probe, &h1, &mut found, &mut work)?;
    let Some(ray1) = h1.bounce(sampler.point(sample, BOUNCE_PAIR))? else {
        // Bunny's smooth shading rejected the bounce: no ray, and it contributes zero.
        found.rejected_bounces = 1;
        let radiance = h1.reflect(d1, ZERO)?;
        return Ok((PathSample::new(radiance, found), work));
    };
    let b = match probe.closest(&ray1, Closest::Bounce, &mut work)? {
        None => {
            found.bounce_misses = 1;
            lighting.sky.radiance()
        }
        Some(hit2) => {
            found.bounce_hits = 1;
            let h2 = Shade::at(scene, &ray1, &hit2)?;
            let d2 = direct(scene, probe, &h2, &mut found, &mut work)?;
            h2.reflect(d2, ZERO)?
        }
    };
    let radiance = h1.reflect(d1, b)?;
    Ok((PathSample::new(radiance, found), work))
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
/// Any error of [`trace_path`], and [`PathError::Pixel`] if the accumulator refuses a sample.
pub fn pixel_radiance(
    scene: &Scene,
    camera: &PinholeCamera,
    settings: RenderSettings,
    x: u32,
    y: u32,
) -> Result<([FixedQ32_32; 3], RayCount), PathError> {
    let frame = Frame { scene, probe: Plain(scene), camera, settings };
    let (radiance, found, Unmeasured) = frame.radiance(x, y)?;
    Ok((radiance, found.count()))
}

/// A rendered image and the rays it took.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Render {
    /// The image.
    pub image: Rgba8Image,
    /// The rays traced: `statistics.count()`.
    pub rays: RayCount,
    /// What the rays found, in all and per pixel (ADR 0008).
    pub statistics: RayStatistics,
}

/// The pixels of one render, shared read-only by every worker: the scene, the probe whose tracer
/// answers its ray queries, the camera at the image size and the settings.
struct Frame<'a, P> {
    scene: &'a Scene,
    probe: P,
    camera: &'a PinholeCamera,
    settings: RenderSettings,
}

/// What one pixel of a render that counts traversals adds up: its rays and their traversal work.
#[derive(Clone, Copy, Debug)]
struct CountedPixel {
    rays: RayStatistics,
    traversal: TraversalStatistics,
}

impl Add for CountedPixel {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self { rays: self.rays + other.rays, traversal: self.traversal + other.traversal }
    }
}

/// A pixel's radiance, what its rays found and the traversal work they recorded.
type Radiance<W> = ([FixedQ32_32; 3], RayOutcomes, W);

impl<P: Probe> Frame<'_, P> {
    /// [`pixel_radiance`] with the frame's probe.
    fn radiance(&self, x: u32, y: u32) -> Result<Radiance<P::Work>, PathError> {
        let sampler = self.settings.sampler(x, y);
        let mut accumulator = PixelAccumulator::new();
        let mut found = RayOutcomes::default();
        let mut work = P::Work::default();
        for sample in 0..self.settings.samples_per_pixel() {
            let (path, traversed) = path(self.scene, &self.probe, self.camera, sampler, sample)?;
            accumulator.add_sample(path.radiance)?;
            found = found + path.outcomes;
            work = work + traversed;
        }
        Ok((accumulator.resolve()?, found, work))
    }

    /// The four RGBA8 bytes of pixel `(x, y)`: its radiance through [`TONE_CURVE`] and
    /// `write_rgba8`; and its ray statistics and traversal work.
    fn rgba(&self, x: u32, y: u32) -> Result<([u8; 4], RayStatistics, P::Work), PathError> {
        let (radiance, found, work) = self.radiance(x, y)?;
        let mut rgba = [0_u8; 4];
        write_rgba8(&mut rgba, TONE_CURVE, radiance)?;
        Ok((rgba, RayStatistics::pixel(found, self.settings.samples_per_pixel()), work))
    }
}

/// Renders `scene` at `size` with `settings` on the workers of `schedule`, with `tracer`
/// answering the ray queries.
///
/// Each pixel is the radiance of [`pixel_radiance`] (with `tracer` in place of the scene),
/// written by `write_rgba8` through [`TONE_CURVE`]: opaque RGBA8, row-major, row 0 at the top.
/// The scene and the tracer are shared read-only by every worker.
/// [`render_pixels`](crate::render_pixels) decides who computes which pixel and when, never its
/// value: every pixel's samples go into its own `PixelAccumulator`, so the image and the ray
/// count are the same for every schedule.
///
/// # Errors
///
/// [`RenderError::Camera`] if Bunny refuses the camera at this size;
/// [`RenderError::Pixel`] with the coordinates and the [`PathError`] of the first failing pixel
/// in raster order (an error of [`pixel_radiance`] or of `write_rgba8`);
/// [`RenderError::OutOfMemory`] with what was being reserved if a buffer cannot be reserved;
/// [`RenderError::Spawn`] with the worker's number and the operating system's error if a worker
/// thread cannot be started.
pub fn render_traced<T: Tracer + Sync + ?Sized>(
    scene: &Scene,
    tracer: &T,
    size: ImageSize,
    settings: RenderSettings,
    schedule: Schedule,
) -> Result<Render, RenderError> {
    let camera = scene.camera().camera(size.width(), size.height())?;
    let frame = Frame { scene, probe: Plain(tracer), camera: &camera, settings };
    // `render_pixels` counts nothing per worker and `Plain` no traversal: the render pays nothing
    // for run or traversal statistics.
    let (image, statistics) = render_pixels(size, schedule, RayStatistics::NONE, |x, y| {
        frame.rgba(x, y).map(|(rgba, rays, Unmeasured)| (rgba, rays))
    })
    .map_err(render_error)?;
    Ok(Render { image, rays: statistics.count(), statistics })
}

/// [`render_traced`], with what each worker did ([`RunStatistics`]): information about the run,
/// which differs from run to run; the [`Render`] does not.
///
/// # Errors
///
/// Any error of [`render_traced`].
pub fn render_traced_with_run<T: Tracer + Sync + ?Sized>(
    scene: &Scene,
    tracer: &T,
    size: ImageSize,
    settings: RenderSettings,
    schedule: Schedule,
) -> Result<(Render, RunStatistics), RenderError> {
    let camera = scene.camera().camera(size.width(), size.height())?;
    let frame = Frame { scene, probe: Plain(tracer), camera: &camera, settings };
    let done = render_pixels_by_worker(size, schedule, RayStatistics::NONE, |x, y| {
        frame.rgba(x, y).map(|(rgba, rays, Unmeasured)| (rgba, rays))
    })
    .map_err(render_error)?;
    let run = run_statistics(&done.workers, |rays| rays, size, schedule)?;
    let statistics = done.total;
    Ok((Render { image: done.image, rays: statistics.count(), statistics }, run))
}

/// [`render_traced_with_run`], with the traversal work of every ray by kind
/// ([`TraversalStatistics`]), through Bunny's counted searches ([`CountingTracer`]). The image,
/// the ray statistics and the errors are those of [`render_traced`]: the counted searches return
/// the uncounted ones' answers, and nothing a path decides reads a count.
///
/// # Errors
///
/// Any error of [`render_traced`].
pub fn render_traced_with_traversal<T: CountingTracer + Sync + ?Sized>(
    scene: &Scene,
    tracer: &T,
    size: ImageSize,
    settings: RenderSettings,
    schedule: Schedule,
) -> Result<(Render, RunStatistics, TraversalStatistics), RenderError> {
    let camera = scene.camera().camera(size.width(), size.height())?;
    let frame = Frame { scene, probe: Counted(tracer), camera: &camera, settings };
    let zero =
        CountedPixel { rays: RayStatistics::NONE, traversal: TraversalStatistics::default() };
    let done = render_pixels_by_worker(size, schedule, zero, |x, y| {
        frame.rgba(x, y).map(|(rgba, rays, traversal)| (rgba, CountedPixel { rays, traversal }))
    })
    .map_err(render_error)?;
    let run = run_statistics(&done.workers, |pixel| pixel.rays, size, schedule)?;
    let statistics = done.total.rays;
    let render = Render { image: done.image, rays: statistics.count(), statistics };
    Ok((render, run, done.total.traversal))
}

/// The run statistics of `workers`, each worker's ray statistics read by `rays`.
fn run_statistics<V: Copy>(
    workers: &[WorkerRun<V>],
    rays: impl Fn(V) -> RayStatistics,
    size: ImageSize,
    schedule: Schedule,
) -> Result<RunStatistics, RenderError> {
    // One record per worker, reserved without aborting on failure, as the scheduler's own list.
    let mut records = Vec::new();
    let handles = workers.len();
    records.try_reserve_exact(handles).map_err(|error| RenderError::OutOfMemory {
        reserving: Reservation::Workers { handles },
        error,
    })?;
    records.extend(
        workers.iter().map(|w| WorkerStatistics::of(w.bands, &rays(w.total), size.width())),
    );
    Ok(RunStatistics {
        band_rows: schedule.band().get(),
        bands: schedule.band().count(size),
        workers: records,
    })
}

/// A worker's failure as the render's: the pixel and its path error, or the reservation or
/// spawn that failed.
fn render_error(error: WorkerError<PathError>) -> RenderError {
    match error {
        WorkerError::Pixel { x, y, error } => RenderError::Pixel { x, y, error },
        WorkerError::OutOfMemory { reserving, error } => {
            RenderError::OutOfMemory { reserving, error }
        }
        WorkerError::Spawn(error) => RenderError::Spawn(error),
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
/// The same image on three threads with bands of 4 rows as on one thread:
///
/// ```
/// use lumen::{reference_scene, render, render_scheduled, ImageSize, RenderSettings};
/// use lumen::{BandRows, Schedule};
///
/// let scene = reference_scene()?;
/// let size = ImageSize::new(16, 9)?;
/// let settings = RenderSettings::new(1, 0)?;
/// let schedule = Schedule::new(3, BandRows::new(4)?)?;
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
/// order, any split into bands and any number of threads give the same bytes.
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
