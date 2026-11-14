//! Size class: medium (real threads at fixed counts; in process, no file, no network).
//!
//! What each worker of a render did (`render_traced_with_run`, `render_pixels_by_worker`, ADR
//! 0008): the split differs from run to run, but it must add up to the render's totals, which do
//! not, at every thread count and band height.

use bunny_num::FixedQ32_32;
use bunny_query::TraceRay;
use lumen::{
    reference_scene, render, render_pixels, render_pixels_by_worker, render_scheduled,
    render_traced, render_traced_with_run, BandRows, BuiltinScene, Hit, ImageSize, PrimitiveIndex,
    RenderSettings, Scene, Schedule, TraceError, Tracer,
};

mod golden;
use golden::{GOLDEN_96X54_4SPP, GOLDEN_FIXED_POINT_96X54_4SPP};

fn schedule(threads: u32, rows: u32) -> Schedule {
    Schedule::new(threads, BandRows::new(rows).expect("rows")).expect("schedule")
}

/// Oracle: the render's own totals and the band arithmetic (the height over the band height,
/// rounded up). At 1, 2, 4 and 18 threads and band heights 1, 2 and 5 on a 23 by 90 image, tall
/// enough that every band height gives 18 bands, so 18 threads are 18 workers: the workers'
/// bands, rows, pixels and rays sum to the bands, the height, the pixels and the rays; there is
/// one record per worker the schedule uses; and the render equals the one-thread render.
#[test]
fn the_workers_add_up_to_the_render_at_every_schedule() {
    let scene = reference_scene().expect("builds");
    let (size, settings) =
        (ImageSize::new(23, 90).expect("size"), RenderSettings::new(2, 3).expect("ok"));
    let alone = render(&scene, size, settings).expect("renders");
    for threads in [1, 2, 4, 18] {
        for rows in [1, 2, 5] {
            let at = format!("{threads} threads, bands of {rows}");
            let schedule = schedule(threads, rows);
            let (got, run) =
                render_traced_with_run(&scene, &scene, size, settings, schedule).expect(&at);
            assert_eq!(got, alone, "{at}: the render");
            assert_eq!((run.band_rows, run.bands), (rows, 90_u32.div_ceil(rows)), "{at}");
            assert_eq!(
                run.workers.len(),
                usize::try_from(schedule.workers(size)).expect("small"),
                "{at}"
            );
            assert_eq!(run.workers.len(), usize::try_from(threads).expect("small"), "{at}");
            let sum =
                |f: fn(&lumen::WorkerStatistics) -> u64| run.workers.iter().map(f).sum::<u64>();
            assert_eq!(sum(|w| u64::from(w.bands)), u64::from(run.bands), "{at}: bands");
            assert_eq!(sum(|w| u64::from(w.rows)), 90, "{at}: rows");
            assert_eq!(sum(|w| w.pixels), 23 * 90, "{at}: pixels");
            assert_eq!(sum(|w| w.rays.total()), got.rays.total(), "{at}: rays");
            for worker in &run.workers {
                assert_eq!(worker.pixels, u64::from(worker.rows) * 23, "{at}: whole rows");
            }
        }
    }
}

/// Oracle: one worker renders every band, in order, so it took them all.
#[test]
fn one_thread_takes_every_band() {
    let scene = reference_scene().expect("builds");
    let (size, settings) =
        (ImageSize::new(8, 9).expect("size"), RenderSettings::new(1, 0).expect("ok"));
    let (_, run) =
        render_traced_with_run(&scene, &scene, size, settings, schedule(1, 2)).expect("renders");
    assert_eq!(run.workers.len(), 1);
    let only = run.workers[0];
    assert_eq!((only.bands, only.rows, only.pixels), (5, 9, 72));
}

/// Oracle: the pixel count, by a pixel function that counts one per pixel. The scheduler's own
/// per-worker records sum to its total, and the total is the image's pixel count. The image is
/// 31 by 54 in bands of 3, 18 bands, so 18 threads are 18 workers.
#[test]
fn the_schedulers_per_worker_sums_add_up_to_its_total() {
    let size = ImageSize::new(31, 54).expect("size");
    for threads in [1, 2, 4, 18] {
        let done = render_pixels_by_worker(size, schedule(threads, 3), 0_u64, |_, _| {
            Ok::<_, ()>(([0, 0, 0, 255], 1_u64))
        })
        .expect("renders");
        assert_eq!(done.total, 31 * 54, "{threads} threads");
        assert_eq!(done.workers.len(), usize::try_from(threads).expect("small"), "{threads} threads");
        let total: u64 = done.workers.iter().map(|w| w.total).sum();
        assert_eq!(total, done.total, "{threads} threads");
        let bands: u32 = done.workers.iter().map(|w| w.bands).sum();
        assert_eq!(bands, 18, "{threads} threads");
    }
}

/// Oracle: the golden image hashes, and the other path. The render that counts nothing per worker
/// (`render_scheduled`) and the one that counts bands (`render_traced_with_run`) give the golden
/// image, the same image, the same ray statistics, at 1, 2, 4 and 18 threads and band heights 1
/// and 3, for both built-in scenes.
#[test]
fn both_tallies_render_the_golden_images_and_the_same_statistics() {
    let (size, settings) =
        (ImageSize::new(96, 54).expect("size"), RenderSettings::new(4, 0).expect("ok"));
    for (builtin, golden) in [
        (BuiltinScene::Reference, GOLDEN_96X54_4SPP),
        (BuiltinScene::FixedPoint, GOLDEN_FIXED_POINT_96X54_4SPP),
    ] {
        let scene = builtin.build().expect("builds");
        for threads in [1, 2, 4, 18] {
            for rows in [1, 3] {
                let at = format!("{}, {threads} threads, bands of {rows}", builtin.name());
                let plain =
                    render_scheduled(&scene, size, settings, schedule(threads, rows)).expect(&at);
                let (counted, _) =
                    render_traced_with_run(&scene, &scene, size, settings, schedule(threads, rows))
                        .expect(&at);
                assert_eq!(plain.image.hash(), golden, "{at}: the golden image");
                assert_eq!(plain, counted, "{at}: both tallies");
            }
        }
    }
}

/// Fails every query whose ray hits triangle 28 000 or above (in the reference scene's bunny), as a
/// tracer that contradicts its tree would, so a render fails at a pixel fixed by the scene.
struct FailsOnTheBunny<'a>(&'a Scene);

impl Tracer for FailsOnTheBunny<'_> {
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
        let found = self.0.closest(ray)?;
        match found {
            Some(hit) if hit.primitive >= PrimitiveIndex::new(28_000) => {
                Err(TraceError::UnknownPrimitive { primitive: hit.primitive, triangles: 0 })
            }
            other => Ok(other),
        }
    }

    fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError> {
        self.0.occluded(ray, t_max)
    }
}

/// Oracle: the other path. Both tallies report the same raster-first failure at every schedule.
#[test]
fn both_tallies_report_the_same_failure() {
    let scene = reference_scene().expect("builds");
    let tracer = FailsOnTheBunny(&scene);
    let (size, settings) =
        (ImageSize::new(32, 18).expect("size"), RenderSettings::new(1, 0).expect("ok"));
    let want = render_traced(&scene, &tracer, size, settings, schedule(1, 1)).expect_err("fails");
    for threads in [1, 2, 4, 18] {
        for rows in [1, 3] {
            let at = format!("{threads} threads, bands of {rows}");
            let plain = render_traced(&scene, &tracer, size, settings, schedule(threads, rows));
            let counted =
                render_traced_with_run(&scene, &tracer, size, settings, schedule(threads, rows));
            assert_eq!(plain.expect_err(&at), want, "{at}");
            assert_eq!(counted.expect_err(&at), want, "{at}");
        }
    }
    // The scheduler's two entry points, directly: the same image and total.
    let pixel = |x: u32, y: u32| {
        Ok::<_, ()>(([u8::try_from(x % 256).expect("byte"), 0, 0, 255], u64::from(y)))
    };
    let plain = render_pixels(size, schedule(4, 2), 0_u64, pixel).expect("renders");
    let counted = render_pixels_by_worker(size, schedule(4, 2), 0_u64, pixel).expect("renders");
    assert_eq!((plain.0, plain.1), (counted.image, counted.total));
}
