//! Rendering on real threads at fixed counts: the image and the ray count do not depend on the
//! thread count or the tile size; a failing pixel gives the same typed error at every thread
//! count and tile size; and the workers stop after a failure. Hand-out orders themselves are
//! explored deterministically in `src/workers/tests.rs`.

use std::sync::atomic::{AtomicUsize, Ordering};

use bunny_broadphase::TraversalError;
use bunny_num::FixedQ32_32;
use bunny_optics::write_rgba8;
use bunny_query::TraceRay;
use lumen::{
    pixel_radiance, reference_scene, render, render_pixels, render_scheduled, render_traced,
    trace_path, Hit, ImageSize, RayCount, RenderError, RenderSettings, Rgba8Image, Scene, Schedule,
    ScheduleError, TileSize, TraceError, Tracer, WorkerError, DEFAULT_TILE, MAX_THREADS,
    TONE_CURVE,
};

mod golden;
use golden::GOLDEN_96X54_4SPP;

fn settings(samples: u32, seed: u32) -> RenderSettings {
    RenderSettings::new(samples, seed).expect("valid settings")
}

fn size(width: u32, height: u32) -> ImageSize {
    ImageSize::new(width, height).expect("valid size")
}

fn tile(width: u32, height: u32) -> TileSize {
    TileSize::new(width, height).expect("valid tile")
}

fn schedule(threads: u32, tile: TileSize) -> Schedule {
    Schedule::new(threads, tile).expect("valid schedule")
}

/// The thread counts every test tries: fixed, so the tests do not depend on the host's cores
/// (18 is the CPU count of the machine the measurements were taken on).
const THREAD_COUNTS: [u32; 6] = [1, 2, 3, 4, 7, 18];

/// The reference image computed without the scheduler: every pixel in raster order with
/// `pixel_radiance` and `write_rgba8`.
fn raster_order(
    scene: &Scene,
    size: ImageSize,
    settings: RenderSettings,
) -> (Rgba8Image, RayCount) {
    let camera = scene.camera().camera(size.width(), size.height()).expect("camera");
    let mut bytes = Vec::new();
    let mut rays = RayCount::default();
    for y in 0..size.height() {
        for x in 0..size.width() {
            let (radiance, pixel_rays) =
                pixel_radiance(scene, &camera, settings, x, y).expect("renders");
            let mut rgba = [0_u8; 4];
            write_rgba8(&mut rgba, TONE_CURVE, radiance).expect("four bytes");
            bytes.extend_from_slice(&rgba);
            rays.closest += pixel_rays.closest;
            rays.shadow += pixel_rays.shadow;
        }
    }
    (Rgba8Image::from_bytes(size, bytes).expect("right length"), rays)
}

// ---------------------------------------------------------------------------------------------
// The image does not depend on the schedule.

/// The golden 96 by 54 image at every thread count and at tile sizes that divide the image,
/// do not divide it, are one pixel, one row, one column or larger than the image.
#[test]
fn the_golden_hash_holds_at_every_thread_count_and_tile_size() {
    let scene = reference_scene().expect("builds");
    let (want_image, want_rays) = raster_order(&scene, size(96, 54), settings(4, 0));
    assert_eq!(want_image.hash(), GOLDEN_96X54_4SPP);
    let tiles = [(1, 1), (7, 5), (16, 16), (32, 32), (96, 1), (1, 54), (13, 54), (200, 200)];
    for threads in THREAD_COUNTS {
        for (w, h) in tiles {
            let render = render_scheduled(
                &scene,
                size(96, 54),
                settings(4, 0),
                schedule(threads, tile(w, h)),
            )
            .expect("renders");
            assert_eq!(render.image.hash(), GOLDEN_96X54_4SPP, "{threads} threads, {w}x{h} tiles");
            assert_eq!(render.rays, want_rays, "{threads} threads, {w}x{h} tiles");
        }
    }
}

/// A larger image with more samples gives the same bytes on one thread and on many, and the
/// same bytes as a raster-order loop over `pixel_radiance` that does not use the scheduler.
#[test]
fn a_larger_image_is_the_same_on_one_thread_and_on_many() {
    let scene = reference_scene().expect("builds");
    let (size, settings) = (size(320, 180), settings(8, 3));
    let one = render_scheduled(&scene, size, settings, Schedule::single()).expect("renders");
    let (oracle, rays) = raster_order(&scene, size, settings);
    assert_eq!(one.image, oracle);
    assert_eq!(one.rays, rays);
    for schedule in
        [schedule(18, DEFAULT_TILE), schedule(5, tile(24, 10)), schedule(16, tile(320, 1))]
    {
        let many = render_scheduled(&scene, size, settings, schedule).expect("renders");
        assert_eq!(many.image.hash(), one.image.hash(), "{schedule:?}");
        assert_eq!(many, one, "{schedule:?}");
    }
    // `render` is the one-thread schedule.
    assert_eq!(render(&scene, size, settings).expect("renders"), one);
}

// ---------------------------------------------------------------------------------------------
// Schedules.

#[test]
fn schedules_are_checked() {
    assert_eq!(Schedule::new(0, DEFAULT_TILE), Err(ScheduleError::NoThreads));
    assert_eq!(
        Schedule::new(MAX_THREADS + 1, DEFAULT_TILE),
        Err(ScheduleError::TooManyThreads(MAX_THREADS + 1))
    );
    assert_eq!(Schedule::new(u32::MAX, DEFAULT_TILE), Err(ScheduleError::TooManyThreads(u32::MAX)));
    assert_eq!(Schedule::new(MAX_THREADS, DEFAULT_TILE).map(Schedule::threads), Ok(MAX_THREADS));
    assert_eq!(TileSize::new(0, 1), Err(ScheduleError::EmptyTile));
    assert_eq!(TileSize::new(1, 0), Err(ScheduleError::EmptyTile));
    assert_eq!(Schedule::single().threads(), 1);
    // Never more workers than tiles.
    assert_eq!(schedule(18, tile(32, 32)).workers(size(1, 1)), 1);
    assert_eq!(schedule(18, tile(32, 32)).workers(size(64, 33)), 4);
    assert_eq!(schedule(3, tile(32, 32)).workers(size(64, 33)), 3);
    assert_eq!(schedule(MAX_THREADS, tile(1, 1)).workers(size(4096, 4096)), MAX_THREADS);
}

// ---------------------------------------------------------------------------------------------
// Errors.

/// A pixel function that fails at `failing` pixels with `NoRay` naming the pixel, and otherwise
/// returns a colour made from the coordinates. It counts the pixels it was asked for and panics
/// (failing the test, through the thread scope) past `limit`: a scheduler that does not stop
/// fails on work taken, not on a clock.
fn failing_pixels<'a>(
    failing: &[(u32, u32)],
    asked: &'a AtomicUsize,
    limit: usize,
) -> impl Fn(u32, u32) -> Result<([u8; 4], RayCount), RenderError> + Sync + 'a {
    let failing = failing.to_vec();
    move |x, y| {
        let n = asked.fetch_add(1, Ordering::SeqCst);
        assert!(n < limit, "asked for more than {limit} pixels");
        if failing.contains(&(x, y)) {
            return Err(RenderError::NoRay { x, y });
        }
        Ok(([x.to_le_bytes()[0], y.to_le_bytes()[0], 7, 255], RayCount { closest: 1, shadow: 0 }))
    }
}

/// Failing pixels and the one whose error must be reported.
type Case<'a> = (&'a [(u32, u32)], (u32, u32));

/// Whatever the threads and tiles, the error is that of the first failing pixel in raster
/// order (top row first), which is not always in the lowest-numbered failing tile: with 32 by 32
/// tiles, `(40, 0)` is in tile 1 and `(3, 5)` in tile 0, and `(40, 0)` comes first.
#[test]
fn the_first_failing_pixel_in_raster_order_is_reported_at_every_schedule() {
    {
        let cases: [Case<'_>; 5] = [
            (&[(37, 20)], (37, 20)),
            (&[(3, 5), (40, 0)], (40, 0)),
            (&[(63, 47), (0, 47)], (0, 47)),
            (&[(10, 40), (50, 33), (5, 33)], (5, 33)),
            // Every pixel on a diagonal: many tiles fail at once.
            (&(0..48).map(|y| (50 - y, y)).collect::<Vec<_>>(), (50, 0)),
        ];
        let tiles = [(1, 1), (5, 3), (32, 32), (64, 1), (1, 48), (200, 200)];
        for (failing, (x, y)) in cases {
            for threads in THREAD_COUNTS.into_iter().chain([16]) {
                for (w, h) in tiles {
                    let asked = AtomicUsize::new(0);
                    let result = render_pixels(
                        size(64, 48),
                        schedule(threads, tile(w, h)),
                        RayCount::default(),
                        failing_pixels(failing, &asked, 64 * 48),
                    );
                    assert_eq!(
                        result,
                        Err(WorkerError::Pixel { x, y, error: RenderError::NoRay { x, y } }),
                        "{failing:?}, {threads} threads, {w}x{h} tiles"
                    );
                }
            }
        }
    }
}

/// After a failure workers take no tile beyond the failing row of tiles. On one thread the count
/// is exact: a failure at the first pixel of tile 0 abandons that tile and renders the other 15
/// tiles of the first row of 32 by 32 tiles of a 512 by 512 image, and nothing after them.
#[test]
fn workers_stop_after_a_failure() {
    {
        let image = size(512, 512);
        let asked = AtomicUsize::new(0);
        let first =
            Err(WorkerError::Pixel { x: 0, y: 0, error: RenderError::NoRay { x: 0, y: 0 } });
        let one = schedule(1, tile(32, 32));
        let result = render_pixels(
            image,
            one,
            RayCount::default(),
            failing_pixels(&[(0, 0)], &asked, 512 * 512),
        );
        assert_eq!(result, first);
        assert_eq!(asked.load(Ordering::SeqCst), 1 + 15 * 32 * 32);
        // On many threads the same error. How much work runs before the failure is recorded
        // depends on the operating system's scheduling, so it is not bounded here; the stop rule
        // itself is checked exactly, under every hand-out order, in `src/workers/tests.rs`.
        for threads in [2, 4, 7, 16] {
            let asked = AtomicUsize::new(0);
            let result = render_pixels(
                image,
                schedule(threads, tile(32, 32)),
                RayCount::default(),
                failing_pixels(&[(0, 0)], &asked, 512 * 512),
            );
            assert_eq!(result, first);
        }
    }
}

/// A tracer whose closest-hit query fails for rays heading right and up (`InvalidNodeIndex`) and
/// for rays heading left and down (`StackOverflow`), and answers every other ray with the scene.
/// The first failing pixel in raster order is then in the top row but not at its left end.
///
/// It counts closest-hit queries and panics past `limit`, so a render that does not stop fails
/// on work taken, not on a clock.
struct Failing<'a> {
    scene: &'a Scene,
    queries: AtomicUsize,
    limit: usize,
}

impl Tracer for Failing<'_> {
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
        let n = self.queries.fetch_add(1, Ordering::SeqCst);
        assert!(n < self.limit, "more than {} closest-hit queries", self.limit);
        let d = ray.direction();
        let zero = FixedQ32_32::ZERO;
        if d.x > zero && d.y > zero && d.x.raw() > 1 << 30 {
            return Err(TraceError::Traversal(TraversalError::InvalidNodeIndex));
        }
        if d.x < zero && d.y < zero && d.x.raw() < -(1 << 30) {
            return Err(TraceError::Traversal(TraversalError::StackOverflow));
        }
        self.scene.closest(ray)
    }

    fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError> {
        self.scene.occluded(ray, t_max)
    }
}

/// A traversal failure in the real integrator surfaces as the typed error of the first failing
/// pixel in raster order, the one a raster-order loop over `trace_path` meets first, at every
/// thread count and tile size, within a budget of queries.
#[test]
fn a_traversal_failure_is_the_same_typed_error_at_every_schedule() {
    {
        let scene = reference_scene().expect("builds");
        // Two closest-hit queries per sample at most: a camera ray and a bounce ray.
        let budget = 64 * 36 * 2 * 2;
        let failing = Failing { scene: &scene, queries: AtomicUsize::new(0), limit: usize::MAX };
        let (size, settings) = (size(64, 36), settings(2, 0));
        let camera = scene.camera().camera(64, 36).expect("camera");
        let mut first = None;
        'pixels: for y in 0..36 {
            for x in 0..64 {
                for sample in 0..2 {
                    if let Err(error) =
                        trace_path(&scene, &failing, &camera, settings.sampler(x, y), sample)
                    {
                        first = Some(error);
                        break 'pixels;
                    }
                }
            }
        }
        let want = first.expect("some pixel fails");
        let mut seen = Vec::new();
        for threads in THREAD_COUNTS {
            for (w, h) in [(1, 1), (8, 8), (32, 32), (64, 1)] {
                let bounded =
                    Failing { scene: &scene, queries: AtomicUsize::new(0), limit: budget };
                let result =
                    render_traced(&scene, &bounded, size, settings, schedule(threads, tile(w, h)));
                assert_eq!(result, Err(want), "{threads} threads, {w}x{h} tiles");
                seen.push(result);
            }
        }
        // Both kinds of failure occur in the image, so the choice between them is a real one.
        let mut kinds = Vec::new();
        for y in 0..36 {
            for x in 0..64 {
                let sampler = settings.sampler(x, y);
                if let Err(error) = trace_path(&scene, &failing, &camera, sampler, 0) {
                    if !kinds.contains(&error) {
                        kinds.push(error);
                    }
                }
            }
        }
        assert_eq!(kinds.len(), 2, "{kinds:?}");
        assert!(!seen.is_empty());
    }
}
