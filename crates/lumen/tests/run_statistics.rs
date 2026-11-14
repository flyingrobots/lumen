//! Size class: medium (real threads at fixed counts; in process, no file, no network).
//!
//! What each worker of a render did (`render_traced_with_run`, `render_pixels_by_worker`, ADR
//! 0008): the split differs from run to run, but it must add up to the render's totals, which do
//! not, at every thread count and band height.

use lumen::{
    reference_scene, render, render_pixels_by_worker, render_traced_with_run, BandRows, ImageSize,
    RenderSettings, Schedule,
};

fn schedule(threads: u32, rows: u32) -> Schedule {
    Schedule::new(threads, BandRows::new(rows).expect("rows")).expect("schedule")
}

/// Oracle: the render's own totals and the band arithmetic (the height over the band height,
/// rounded up). At 1, 2, 4 and 18 threads and band heights 1, 2 and 5 on a 23 by 17 image: the
/// workers' bands, rows, pixels and rays sum to the bands, the height, the pixels and the rays;
/// there is one record per worker the schedule uses; and the render equals the one-thread render.
#[test]
fn the_workers_add_up_to_the_render_at_every_schedule() {
    let scene = reference_scene().expect("builds");
    let (size, settings) =
        (ImageSize::new(23, 17).expect("size"), RenderSettings::new(2, 3).expect("ok"));
    let alone = render(&scene, size, settings).expect("renders");
    for threads in [1, 2, 4, 18] {
        for rows in [1, 2, 5] {
            let at = format!("{threads} threads, bands of {rows}");
            let schedule = schedule(threads, rows);
            let (got, run) =
                render_traced_with_run(&scene, &scene, size, settings, schedule).expect(&at);
            assert_eq!(got, alone, "{at}: the render");
            assert_eq!((run.band_rows, run.bands), (rows, 17_u32.div_ceil(rows)), "{at}");
            assert_eq!(
                run.workers.len(),
                usize::try_from(schedule.workers(size)).expect("small"),
                "{at}"
            );
            let sum =
                |f: fn(&lumen::WorkerStatistics) -> u64| run.workers.iter().map(f).sum::<u64>();
            assert_eq!(sum(|w| u64::from(w.bands)), u64::from(run.bands), "{at}: bands");
            assert_eq!(sum(|w| u64::from(w.rows)), 17, "{at}: rows");
            assert_eq!(sum(|w| w.pixels), 23 * 17, "{at}: pixels");
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
/// per-worker records sum to its total, and the total is the image's pixel count.
#[test]
fn the_schedulers_per_worker_sums_add_up_to_its_total() {
    let size = ImageSize::new(31, 29).expect("size");
    for threads in [1, 2, 4, 18] {
        let done = render_pixels_by_worker(size, schedule(threads, 3), 0_u64, |_, _| {
            Ok::<_, ()>(([0, 0, 0, 255], 1_u64))
        })
        .expect("renders");
        assert_eq!(done.total, 31 * 29, "{threads} threads");
        let total: u64 = done.workers.iter().map(|w| w.total).sum();
        assert_eq!(total, done.total, "{threads} threads");
        let bands: u32 = done.workers.iter().map(|w| w.bands).sum();
        assert_eq!(bands, 29_u32.div_ceil(3), "{threads} threads");
    }
}
