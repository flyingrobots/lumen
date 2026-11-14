//! Hand-out orders explored on one thread, with no clock and no real threads: simulated workers
//! advance one step at a time (take a band, render one row of it, return the outcome) through
//! the same `take`, `render_row` and `combine` the threads run, in an order chosen by a
//! policy: a seeded generator, or a scripted worst case. Every failure message names the policy,
//! so any order can be replayed. A step budget, not a timeout, ends a simulation that does not
//! finish.

use core::cell::Cell;

use super::*;
use crate::schedule::BandRows;

/// A pixel's coordinates, also the simulated pixel function's error.
type Coord = (u32, u32);
/// What the simulated pixel function returns.
type PixelResult = Result<(Rgba, u64), Coord>;
/// What a simulated render returns: the image bytes and the pixel count, or why it stopped.
type Simulated = Result<(Vec<u8>, u64), WorkerError<Coord>>;
/// Failing pixels and the one whose error must be reported.
type Case<'a> = (&'a [Coord], Coord);

/// The simulated image: 13 by 9, so no band height but 1 and 9 divides it evenly into rows.
const WIDTH: u32 = 13;
const HEIGHT: u32 = 9;

/// Which simulated worker moves next.
#[derive(Clone, Copy, Debug)]
enum Policy {
    /// Drawn from a xorshift generator with this seed.
    Seeded(u64),
    /// Always the first worker still running: one worker takes everything it can.
    First,
    /// Idle workers take first; then the worker holding the lowest band on the image moves, so
    /// bands finish in reverse order and the top band (and its failure) comes last.
    LowestBandFirst,
}

/// A simulated worker: idle, or partway through a band (its first row and its rows left).
struct Sim<'a> {
    outcome: Outcome<u64, Coord>,
    band: Option<(u32, Rows<'a>)>,
    done: bool,
}

/// The pixel function under test: a colour from the coordinates, the value 1 per pixel, and a
/// failure (the pixel's own coordinates) at each pixel of `failing`.
fn pixel(failing: &[Coord]) -> impl Fn(u32, u32) -> PixelResult + '_ {
    move |x, y| {
        if failing.contains(&(x, y)) {
            return Err((x, y));
        }
        Ok(([x.to_le_bytes()[0], y.to_le_bytes()[0], 9, 255], 1))
    }
}

/// The next worker to move under `policy`, or `None` when all are done.
fn pick(sims: &[Sim<'_>], policy: Policy, rng: &mut u64) -> Option<usize> {
    let live: Vec<usize> = (0..sims.len()).filter(|&n| !sims[n].done).collect();
    let first = *live.first()?;
    Some(match policy {
        Policy::First => first,
        Policy::Seeded(_) => {
            *rng ^= *rng << 13;
            *rng ^= *rng >> 7;
            *rng ^= *rng << 17;
            let count = u64::try_from(live.len()).expect("few workers");
            live[usize::try_from(*rng % count).expect("below the count")]
        }
        Policy::LowestBandFirst => {
            let idle = live.iter().copied().find(|&n| sims[n].band.is_none());
            idle.unwrap_or_else(|| {
                let band = |n: usize| sims[n].band.as_ref().map_or(0, |(first, _)| *first);
                live.iter().copied().max_by_key(|&n| band(n)).expect("live")
            })
        }
    })
}

/// Moves worker `sim` one step: the steps of `work`, one at a time.
fn step<'a>(shared: &Shared<'a>, sim: &mut Sim<'a>, pixel: &impl Fn(u32, u32) -> PixelResult) {
    match sim.band.as_mut() {
        None => {
            sim.band = shared.take();
            sim.done = sim.band.is_none();
        }
        Some((_, rows)) => match rows.next() {
            None => sim.band = None,
            Some((y, row)) => {
                if shared.render_row(y, row, pixel, &mut sim.outcome) == Row::Leave {
                    sim.band = None;
                }
            }
        },
    }
}

/// Runs `workers` simulated workers over the 13 by 9 image in bands of `rows` in the order
/// `policy` chooses, with `pixel` as the pixel function, and returns what `render_pixels` would.
/// With `halt` set to `(n, flag)`, the workers are halted before step `n`, as a failed spawn
/// halts them, and `flag` is raised at that moment.
fn simulate(
    rows: u32,
    workers: usize,
    policy: Policy,
    pixel: &impl Fn(u32, u32) -> PixelResult,
    halt: Option<(usize, &Cell<bool>)>,
) -> Simulated {
    let size = ImageSize::new(WIDTH, HEIGHT).expect("size");
    let schedule = Schedule::new(1, BandRows::new(rows).expect("rows")).expect("schedule");
    let mut image = blank(size)?;
    let mut rng = match policy {
        Policy::Seeded(seed) => seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
        _ => 1,
    };
    let outcome = {
        let shared = Shared::new(size, schedule, image.bytes_mut().as_chunks_mut::<4>().0);
        let mut sims: Vec<Sim<'_>> = (0..workers)
            .map(|_| Sim { outcome: Outcome::new(0), band: None, done: false })
            .collect();
        // At most 9 bands, each taken once, each of its at most 9 rows rendered once and one
        // more step to see it end; each worker also makes one last take. More steps mean a
        // scheduler that does not end.
        let budget = 9 * (9 + 2) + workers;
        let mut steps = 0;
        while let Some(n) = pick(&sims, policy, &mut rng) {
            assert!(steps < budget, "{policy:?}: no end within {budget} steps");
            if let Some((_, flag)) = halt.filter(|(at, _)| *at == steps) {
                shared.halt();
                flag.set(true);
            }
            step(&shared, &mut sims[n], pixel);
            steps += 1;
        }
        combine(0, sims.into_iter().map(|sim| sim.outcome))
    };
    let total = outcome.into_result()?;
    Ok((image.bytes().to_vec(), total))
}

/// The image without any scheduling: every pixel in raster order.
fn raster() -> Vec<u8> {
    let pixel = pixel(&[]);
    let mut bytes = Vec::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            bytes.extend_from_slice(&pixel(x, y).expect("no failure").0);
        }
    }
    bytes
}

/// Every policy tried: the scripted worst cases and 200 seeds.
fn policies() -> impl Iterator<Item = Policy> {
    [Policy::First, Policy::LowestBandFirst].into_iter().chain((0..200).map(Policy::Seeded))
}

/// Band heights: one row, heights that leave a partial last band (2, 4, 5), one that divides
/// the image (3), the whole image (9) and more than the image (20).
const BANDS: [u32; 7] = [1, 2, 3, 4, 5, 9, 20];
const WORKERS: [usize; 5] = [1, 2, 3, 7, 200];

/// Oracle: the raster-order loop above. Under every policy, 1 to 200 workers (200 is more than
/// there are bands, so every band is in flight at once and `LowestBandFirst` finishes them in
/// exact reverse order) and seven band heights, the image equals it byte for byte and every
/// pixel is counted once.
#[test]
fn every_hand_out_order_gives_the_raster_image() {
    let want = raster();
    for rows in BANDS {
        for workers in WORKERS {
            for policy in policies() {
                let got = simulate(rows, workers, policy, &pixel(&[]), None);
                assert_eq!(
                    got,
                    Ok((want.clone(), u64::from(WIDTH * HEIGHT))),
                    "{policy:?}, {workers} workers, bands of {rows}"
                );
            }
        }
    }
}

/// Oracle: the failing pixel that comes first in raster order, by inspection of each case. It
/// is found under every policy, also when a lower band fails first: `LowestBandFirst` makes the
/// top failing band the last one rendered.
#[test]
fn every_hand_out_order_reports_the_raster_first_failure() {
    let cases: [Case<'_>; 5] = [
        (&[(12, 8)], (12, 8)),
        (&[(2, 5), (9, 0)], (9, 0)),
        (&[(0, 8), (12, 7), (6, 7)], (6, 7)),
        (&[(4, 4), (3, 4), (8, 4)], (3, 4)),
        // Column-major order would choose (1, 6).
        (&[(1, 6), (7, 2)], (7, 2)),
    ];
    for (failing, (x, y)) in cases {
        for rows in BANDS {
            for workers in WORKERS {
                for policy in policies() {
                    let got = simulate(rows, workers, policy, &pixel(failing), None);
                    let want = Err(WorkerError::Pixel { x, y, error: (x, y) });
                    assert_eq!(
                        got, want,
                        "{policy:?}, {workers} workers, bands of {rows}, {failing:?}"
                    );
                }
            }
        }
    }
}

/// Oracle: the stop rule. Once a pixel in row `y` fails, no worker begins a row from `y` down,
/// and the failing worker leaves its band; rows above `y` still run, since an earlier failure
/// may hide there. So under every order, every pixel computed after the failure is recorded is
/// above row `y`. In the simulation the failure is recorded in the same step it happens and a
/// row is one step, so "after" is exact, not a matter of timing.
#[test]
fn after_a_failure_no_pixel_from_its_row_down_is_rendered() {
    let witnesses = Cell::new(0_usize);
    for (fx, fy) in [(0, 0), (6, 4), (12, 3), (5, 8)] {
        for rows in BANDS {
            for workers in WORKERS {
                for policy in policies() {
                    let (failed, lowest, after) = (Cell::new(false), Cell::new(None), Cell::new(0));
                    let counting = |x, y| {
                        if failed.get() {
                            after.set(after.get() + 1);
                            lowest.set(lowest.get().max(Some(y)));
                        }
                        let result = pixel(&[(fx, fy)])(x, y);
                        failed.set(failed.get() || result.is_err());
                        result
                    };
                    let got = simulate(rows, workers, policy, &counting, None);
                    let label =
                        format!("{policy:?}, {workers} workers, bands of {rows}, ({fx}, {fy})");
                    assert_eq!(
                        got,
                        Err(WorkerError::Pixel { x: fx, y: fy, error: (fx, fy) }),
                        "{label}"
                    );
                    assert!(
                        lowest.get().is_none_or(|y| y < fy),
                        "{label}: row {lowest:?} after the failure"
                    );
                    witnesses.set(witnesses.get() + after.get());
                }
            }
        }
    }
    // Witness count: the rule was checked on pixels computed after a failure, not vacuously.
    assert!(witnesses.get() > 0);
}

/// Oracle: hand arithmetic. Outcomes merge by summing and by keeping the raster-earlier failure,
/// in either order.
#[test]
fn outcomes_merge_to_the_sum_and_the_raster_earlier_failure() {
    let at = |x, y| Some(Failure { y, x, error: (x, y) });
    for (a, b, want) in [(at(9, 0), at(2, 5), (9, 0)), (at(3, 4), at(4, 4), (3, 4))] {
        for (first, second) in [(a, b), (b, a)] {
            let mut merged = Outcome { total: 2_u64, first };
            merged.merge(Outcome { total: 5, first: second });
            assert_eq!(merged.total, 7);
            assert_eq!(
                merged.into_result(),
                Err(WorkerError::Pixel { x: want.0, y: want.1, error: want })
            );
        }
    }
}

/// Oracle: the halt rule. After `halt` (what a failed spawn calls) no worker begins a row, under
/// every order, band height and worker count, whenever it comes: before the first step, part way,
/// or after the last band is taken. A row is one step, so "after" is exact. Witness count: halts
/// that came while rows were still to render.
#[test]
fn after_a_halt_no_row_is_begun() {
    let mut witnesses = 0_usize;
    for rows in BANDS {
        for workers in WORKERS {
            for policy in policies().take(20) {
                for halt_at in [0, 1, 2, 5, 11, 40] {
                    let (halted, after) = (Cell::new(false), Cell::new(0_usize));
                    let counting = |x, y| {
                        if halted.get() {
                            after.set(after.get() + 1);
                        }
                        pixel(&[])(x, y)
                    };
                    let label = format!("{policy:?}, {workers} workers, bands of {rows}");
                    let got = simulate(rows, workers, policy, &counting, Some((halt_at, &halted)));
                    assert_eq!(after.get(), 0, "{label}, halted at step {halt_at}");
                    let full = u64::from(WIDTH * HEIGHT);
                    if got.as_ref().is_ok_and(|(_, count)| *count < full) {
                        witnesses += 1;
                    }
                }
            }
        }
    }
    assert!(witnesses > 0, "no halt came while rows were left");
}

/// Starts workers with the operating system, except worker `refuse`, which it refuses with an
/// `Other` error, as an operating system out of threads would. It counts the workers it started.
struct RefuseAt {
    refuse: u32,
    started: Cell<u32>,
}

impl Spawner for RefuseAt {
    fn spawn<'scope, 'env, T, F>(
        &self,
        scope: &'scope Scope<'scope, 'env>,
        worker: u32,
        run: F,
    ) -> io::Result<ScopedJoinHandle<'scope, T>>
    where
        T: Send + 'scope,
        F: FnOnce() -> T + Send + 'scope,
    {
        if worker == self.refuse {
            return Err(io::Error::other("refused by the test"));
        }
        self.started.set(self.started.get() + 1);
        OsThreads.spawn(scope, worker, run)
    }
}

/// Oracle: the injected refusal. On real threads, when the operating system refuses worker `n`,
/// the render returns `Spawn` for worker `n` with the operating system's error as its source,
/// after the `n - 1` workers already started have been joined, and never more pixels than the
/// image has (each worker asked is counted, and the count past the image fails the test). That
/// they stop at their next row is the halt rule, checked on every order above.
#[test]
fn a_refused_worker_is_reported_with_its_number_and_its_error() {
    use std::sync::atomic::AtomicUsize;
    let size = ImageSize::new(64, 48).expect("size");
    for (threads, refuse) in [(4, 1), (4, 2), (4, 3), (7, 6)] {
        let spawner = RefuseAt { refuse, started: Cell::new(0) };
        let asked = AtomicUsize::new(0);
        let schedule = Schedule::new(threads, BandRows::new(1).expect("rows")).expect("schedule");
        let got = render_pixels_on(&spawner, size, schedule, 0_u64, |x, y| {
            let n = asked.fetch_add(1, Ordering::SeqCst);
            assert!(n < 64 * 48, "asked for more pixels than the image has");
            pixel(&[])(x, y)
        });
        let label = format!("{threads} threads, worker {refuse} refused");
        let want = SpawnError::new(refuse, io::Error::other("refused by the test"));
        assert_eq!(got, Err(WorkerError::Spawn(want)), "{label}");
        assert_eq!(spawner.started.get(), refuse - 1, "{label}");
        let WorkerError::Spawn(spawn) = got.expect_err("refused") else {
            panic!("{label}: not a spawn error")
        };
        let text = format!("worker thread {refuse} could not be started: refused by the test");
        assert_eq!(spawn.to_string(), text, "{label}");
        let os = std::error::Error::source(&spawn).map(ToString::to_string);
        assert_eq!(os.as_deref(), Some("refused by the test"), "{label}");
        assert_eq!(spawn.error().kind(), io::ErrorKind::Other, "{label}");
    }
}
