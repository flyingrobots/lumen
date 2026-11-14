//! The integrator and the render loop: samples per pixel, each one diffuse path with a
//! shadow-casting directional light and one bounce, composed only of Bunny calls, then Bunny's
//! pixel path, on one or more threads.
//!
//! The image is a pure function of the scene and the settings. The [`Schedule`] (how many
//! threads, how large the tiles) decides only who computes a pixel and when, never its value:
//! every pixel's samples are accumulated for that pixel alone, by one worker, in its own
//! `PixelAccumulator`, so no accumulation order exists between threads.

use core::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

use bunny_num::FixedQ32_32;
use bunny_optics::{
    write_rgba8, CameraError, PinholeCamera, PixelAccumulator, PixelError, PixelSampler,
    SurfaceHit, ToneCurve, MAX_SAMPLES_PER_PIXEL, SHADOW_T_MAX,
};
use bunny_query::TraceRay;

use crate::image::{ImageSize, Rgba8Image, SizeError};
use crate::scene::{Hit, Scene, SceneError, TraceError, Tracer};

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

    const fn add(self, other: Self) -> Self {
        Self { closest: self.closest + other.closest, shadow: self.shadow + other.shadow }
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
    traced_pixel_radiance(scene, scene, camera, settings, x, y)
}

/// [`pixel_radiance`] with `tracer` answering the ray queries.
fn traced_pixel_radiance<T: Tracer + ?Sized>(
    scene: &Scene,
    tracer: &T,
    camera: &PinholeCamera,
    settings: RenderSettings,
    x: u32,
    y: u32,
) -> Result<([FixedQ32_32; 3], RayCount), RenderError> {
    let sampler = settings.sampler(x, y);
    let mut accumulator = PixelAccumulator::new();
    let mut rays = RayCount::default();
    for sample in 0..settings.samples_per_pixel() {
        let path = trace_path(scene, tracer, camera, sampler, sample)?;
        accumulator.add_sample(path.radiance)?;
        rays = rays.add(path.rays);
    }
    Ok((accumulator.resolve()?, rays))
}

/// A rendered image and the rays it took.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Render {
    /// The image.
    pub image: Rgba8Image,
    /// The rays traced.
    pub rays: RayCount,
}

/// The most worker threads a render takes: 256, well above any core count lumen is run on, and
/// low enough that a mistyped `--threads` cannot ask the system for thousands of threads.
pub const MAX_THREADS: u32 = 256;

/// The default tile: 32 by 32 pixels, chosen by measurement (see the README, "Performance").
pub const DEFAULT_TILE: TileSize = TileSize { width: 32, height: 32 };

/// Why a schedule was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleError {
    /// Zero threads were asked for.
    NoThreads,
    /// More than [`MAX_THREADS`] threads were asked for.
    TooManyThreads(u32),
    /// A tile width or height is zero.
    EmptyTile,
}

impl fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoThreads => f.write_str("threads must be at least 1"),
            Self::TooManyThreads(n) => {
                write!(f, "threads must be at most {MAX_THREADS}, not {n}")
            }
            Self::EmptyTile => f.write_str("tile width and height must be at least 1"),
        }
    }
}

impl std::error::Error for ScheduleError {}

/// A checked tile size: the rectangle of pixels one worker renders before taking the next.
///
/// Tiles cover the image in row-major order from the top left; the tiles in the last column and
/// the last row are cut to the image. A tile larger than the image is the whole image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileSize {
    width: u32,
    height: u32,
}

impl TileSize {
    /// A tile of `width` by `height` pixels.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::EmptyTile`] if either is zero.
    pub const fn new(width: u32, height: u32) -> Result<Self, ScheduleError> {
        if width == 0 || height == 0 {
            return Err(ScheduleError::EmptyTile);
        }
        Ok(Self { width, height })
    }

    /// The tile width.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    /// The tile height.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }
}

/// How a render is spread over threads: the number of worker threads and the tile size.
///
/// A schedule never changes a pixel; it decides only which worker computes which tile and when.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Schedule {
    threads: u32,
    tile: TileSize,
}

impl Schedule {
    /// `threads` workers (`1..=MAX_THREADS`) taking tiles of `tile`.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::NoThreads`] for zero, [`ScheduleError::TooManyThreads`] above
    /// [`MAX_THREADS`].
    pub const fn new(threads: u32, tile: TileSize) -> Result<Self, ScheduleError> {
        if threads == 0 {
            return Err(ScheduleError::NoThreads);
        }
        if threads > MAX_THREADS {
            return Err(ScheduleError::TooManyThreads(threads));
        }
        Ok(Self { threads, tile })
    }

    /// One worker (the calling thread) and the default tile.
    #[must_use]
    pub const fn single() -> Self {
        Self { threads: 1, tile: DEFAULT_TILE }
    }

    /// As many workers as `std::thread::available_parallelism` reports (inside a container, the
    /// CPU limit), at most [`MAX_THREADS`], and one if it reports nothing; the default tile.
    #[must_use]
    pub fn available() -> Self {
        let threads = std::thread::available_parallelism()
            .map_or(1, |n| u32::try_from(n.get()).unwrap_or(MAX_THREADS))
            .clamp(1, MAX_THREADS);
        Self { threads, tile: DEFAULT_TILE }
    }

    /// The threads asked for.
    #[must_use]
    pub const fn threads(self) -> u32 {
        self.threads
    }

    /// The tile size.
    #[must_use]
    pub const fn tile(self) -> TileSize {
        self.tile
    }

    /// The same schedule with another tile size.
    #[must_use]
    pub const fn with_tile(self, tile: TileSize) -> Self {
        Self { threads: self.threads, tile }
    }

    /// The workers a render of `size` uses: the threads asked for, but never more than there
    /// are tiles. The calling thread is one of them.
    #[must_use]
    pub fn workers(self, size: ImageSize) -> u32 {
        let tiles = Tiles::new(size, self.tile).count;
        u32::try_from(tiles).map_or(self.threads, |tiles| self.threads.min(tiles).max(1))
    }
}

/// The tiles of an image: how many per row and in all.
#[derive(Clone, Copy)]
struct Tiles {
    size: ImageSize,
    tile: TileSize,
    across: usize,
    count: usize,
}

/// One tile's pixels: columns `x0..x1` of rows `y0..y1`.
#[derive(Clone, Copy)]
struct TileRect {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

impl Tiles {
    fn new(size: ImageSize, tile: TileSize) -> Self {
        let across = size.width().div_ceil(tile.width) as usize;
        let down = size.height().div_ceil(tile.height) as usize;
        Self { size, tile, across, count: across * down }
    }

    /// Tile `index`, counted row-major from the top left.
    #[allow(clippy::cast_possible_truncation)] // index < count, so each quotient fits u32.
    fn rect(self, index: usize) -> TileRect {
        let x0 = (index % self.across) as u32 * self.tile.width;
        let y0 = (index / self.across) as u32 * self.tile.height;
        TileRect {
            x0,
            y0,
            x1: x0.saturating_add(self.tile.width).min(self.size.width()),
            y1: y0.saturating_add(self.tile.height).min(self.size.height()),
        }
    }

    /// The index of the last tile in the same row of tiles as tile `index`.
    fn row_end(self, index: usize) -> usize {
        (index / self.across + 1) * self.across - 1
    }
}

/// The first failing pixel found so far, in raster order (row, then column).
#[derive(Clone, Copy)]
struct Failure {
    y: u32,
    x: u32,
    error: RenderError,
}

/// What the workers share: the next tile to hand out, the last tile still worth rendering, the
/// first failure, an abort flag, the image and the ray counts.
struct Shared<'a> {
    tiles: Tiles,
    next: AtomicUsize,
    last: AtomicUsize,
    failure: Mutex<Option<Failure>>,
    abort: Mutex<Option<RenderError>>,
    aborted: AtomicBool,
    image: Mutex<&'a mut [u8]>,
    closest: AtomicU64,
    shadow: AtomicU64,
}

impl Shared<'_> {
    /// Whether tile `index` still matters: no failure in an earlier row of tiles and no abort.
    fn wanted(&self, index: usize) -> bool {
        index <= self.last.load(Ordering::SeqCst) && !self.aborted.load(Ordering::SeqCst)
    }

    /// Records a failure at `(x, y)` in tile `index`. Every pixel before `(x, y)` in raster
    /// order lies in this row of tiles or an earlier one, so tiles after this row's end are no
    /// longer needed, and the earliest failure among the rest is the one a raster-order render
    /// would meet first.
    fn fail(&self, index: usize, x: u32, y: u32, error: RenderError) {
        self.last.fetch_min(self.tiles.row_end(index), Ordering::SeqCst);
        let mut failure = self.failure.lock().unwrap_or_else(PoisonError::into_inner);
        if failure.is_none_or(|f| (y, x) < (f.y, f.x)) {
            *failure = Some(Failure { y, x, error });
        }
    }

    /// Stops every worker for a reason that is not a pixel's (no memory, no thread).
    fn abort(&self, error: RenderError) {
        self.abort.lock().unwrap_or_else(PoisonError::into_inner).get_or_insert(error);
        self.aborted.store(true, Ordering::SeqCst);
    }
}

/// One worker: takes the next tile until none is left or none is wanted, renders it into its own
/// buffer, and copies the finished tile into the image at the tile's place.
fn work<F>(shared: &Shared<'_>, pixel: &F)
where
    F: Fn(u32, u32) -> Result<([u8; 4], RayCount), RenderError> + Sync,
{
    let tiles = shared.tiles;
    let tile_bytes = tiles.tile.width.min(tiles.size.width()) as usize
        * tiles.tile.height.min(tiles.size.height()) as usize
        * 4;
    let mut buffer = Vec::new();
    if buffer.try_reserve_exact(tile_bytes).is_err() {
        shared.abort(RenderError::OutOfMemory);
        return;
    }
    let mut rays = RayCount::default();
    'tiles: loop {
        let index = shared.next.fetch_add(1, Ordering::SeqCst);
        if index >= tiles.count || !shared.wanted(index) {
            break;
        }
        let rect = tiles.rect(index);
        buffer.clear();
        for y in rect.y0..rect.y1 {
            if !shared.wanted(index) {
                continue 'tiles;
            }
            for x in rect.x0..rect.x1 {
                match pixel(x, y) {
                    Ok((rgba, pixel_rays)) => {
                        buffer.extend_from_slice(&rgba);
                        rays = rays.add(pixel_rays);
                    }
                    Err(error) => {
                        shared.fail(index, x, y, error);
                        continue 'tiles;
                    }
                }
            }
        }
        let row_bytes = (rect.x1 - rect.x0) as usize * 4;
        let stride = tiles.size.width() as usize * 4;
        let mut image = shared.image.lock().unwrap_or_else(PoisonError::into_inner);
        for (row, src) in (rect.y0..rect.y1).zip(buffer.chunks_exact(row_bytes)) {
            let at = row as usize * stride + rect.x0 as usize * 4;
            image[at..at + row_bytes].copy_from_slice(src);
        }
    }
    shared.closest.fetch_add(rays.closest, Ordering::SeqCst);
    shared.shadow.fetch_add(rays.shadow, Ordering::SeqCst);
}

/// The scheduler: renders every pixel of `size` with `pixel` on `schedule.workers(size)`
/// workers, the calling thread being one of them, and returns the image and the summed rays.
///
/// `pixel(x, y)` gives the four RGBA8 bytes of pixel `(x, y)` and the rays it traced. Tiles of
/// `schedule.tile()` are handed out one at a time from a shared counter, in row-major order, to
/// whichever worker asks next, so a slow region or a slow core never holds up the others. A
/// worker renders a whole tile into its own buffer, then copies it into the image at the tile's
/// place. Each pixel is written exactly once, by the worker that computed it, so the image is
/// the same bytes for every thread count, tile size and timing, as long as `pixel` is a pure
/// function of `(x, y)`. With one thread no thread is started and the calling thread renders
/// every tile in order: the same code path with one worker.
///
/// # Errors
///
/// If `pixel` fails, the error of the first failing pixel in raster order (top row first, left
/// to right), the same error at every thread count and tile size: the error a one-pixel-at-a-time
/// render in raster order would return. Once a pixel fails, workers stop taking tiles below that
/// row of tiles and abandon such tiles at their next row. [`RenderError::OutOfMemory`] if a
/// buffer cannot be reserved and [`RenderError::Spawn`] if a worker thread cannot be started.
pub fn render_pixels<F>(
    size: ImageSize,
    schedule: Schedule,
    pixel: F,
) -> Result<Render, RenderError>
where
    F: Fn(u32, u32) -> Result<([u8; 4], RayCount), RenderError> + Sync,
{
    let len = size.byte_len().ok_or(RenderError::OutOfMemory)?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(len).map_err(|_| RenderError::OutOfMemory)?;
    bytes.resize(len, 0);
    let tiles = Tiles::new(size, schedule.tile);
    let shared = Shared {
        tiles,
        next: AtomicUsize::new(0),
        last: AtomicUsize::new(tiles.count.saturating_sub(1)),
        failure: Mutex::new(None),
        abort: Mutex::new(None),
        aborted: AtomicBool::new(false),
        image: Mutex::new(&mut bytes),
        closest: AtomicU64::new(0),
        shadow: AtomicU64::new(0),
    };
    let helpers = schedule.workers(size).saturating_sub(1);
    std::thread::scope(|scope| {
        for n in 0..helpers {
            let spawned = std::thread::Builder::new()
                .name(format!("lumen-worker-{}", n + 1))
                .spawn_scoped(scope, || work(&shared, &pixel));
            if spawned.is_err() {
                shared.abort(RenderError::Spawn);
                return;
            }
        }
        work(&shared, &pixel);
    });
    if let Some(error) = *shared.abort.lock().unwrap_or_else(PoisonError::into_inner) {
        return Err(error);
    }
    if let Some(failure) = *shared.failure.lock().unwrap_or_else(PoisonError::into_inner) {
        return Err(failure.error);
    }
    let rays =
        RayCount { closest: shared.closest.into_inner(), shadow: shared.shadow.into_inner() };
    let image = Rgba8Image::from_bytes(size, bytes).ok_or(RenderError::OutOfMemory)?;
    Ok(Render { image, rays })
}

/// Renders `scene` at `size` with `settings` on the workers of `schedule`, with `tracer`
/// answering the ray queries.
///
/// Each pixel is the radiance of [`pixel_radiance`] (with `tracer` in place of the scene),
/// written by `write_rgba8` through [`TONE_CURVE`]: opaque RGBA8, row-major, row 0 at the top.
/// The scene and the tracer are shared read-only by every worker; [`render_pixels`] decides who
/// computes which pixel, never its value.
///
/// # Errors
///
/// [`RenderError::Camera`] if Bunny refuses the camera at this size, and any error of
/// [`render_pixels`], [`pixel_radiance`] or `write_rgba8`, for the first failing pixel in
/// raster order.
pub fn render_traced<T: Tracer + Sync + ?Sized>(
    scene: &Scene,
    tracer: &T,
    size: ImageSize,
    settings: RenderSettings,
    schedule: Schedule,
) -> Result<Render, RenderError> {
    let camera = scene.camera().camera(size.width(), size.height())?;
    render_pixels(size, schedule, |x, y| {
        let (radiance, rays) = traced_pixel_radiance(scene, tracer, &camera, settings, x, y)?;
        let mut rgba = [0_u8; 4];
        write_rgba8(&mut rgba, TONE_CURVE, radiance)?;
        Ok((rgba, rays))
    })
}

/// Renders `scene` at `size` with `settings` on the workers of `schedule`: [`render_traced`]
/// with the scene as its own tracer.
///
/// # Errors
///
/// Any error of [`render_traced`].
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
/// [`RenderError::Camera`] if Bunny refuses the camera at this size,
/// [`RenderError::OutOfMemory`] if the buffer cannot be reserved, and any error of
/// [`pixel_radiance`] or `write_rgba8`, for the first failing pixel in raster order.
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
    render(&crate::scene::reference_scene()?, size, settings)
}
