//! Hand-out orders explored on one thread, with no clock and no real threads: simulated workers
//! advance one step at a time (take a band, render one row of it, leave the band) through the
//! same `Worker::step` and `combine` the threads run, in an order chosen by a
//! policy: a seeded generator, or a scripted worst case. Every failure message names the policy,
//! so any order can be replayed. A step budget, not a timeout, ends a simulation that does not
//! finish. Every simulation runs under both tallies: the one `render_pixels` uses, which counts
//! nothing, and the one `render_pixels_by_worker` uses, which counts bands.

use core::cell::Cell;
use std::any::Any;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

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
    /// Worker 0 moves first (it takes the top band), then waits this many steps while the first
    /// other worker still running moves, then moves whenever it is running.
    Hold(usize),
}

/// A panic's payload: whatever the pixel function or a worker panicked with.
type Payload = Box<dyn Any + Send>;

/// A simulated worker: running (the production `Worker`, idle or partway through a band), done
/// with its outcome, or ended by a panic in the pixel function.
enum Sim<'s, 'a, C = NoTally> {
    Live(Worker<'s, 'a, u64, Coord, C>),
    Done(Outcome<u64, Coord, C>),
    Panicked,
}

impl<C: Tally> Sim<'_, '_, C> {
    const fn is_live(&self) -> bool {
        matches!(self, Self::Live(_))
    }

    /// The first row of the band the worker is in, if it is in one.
    fn band(&self) -> Option<u32> {
        match self {
            Self::Live(worker) => worker.band.as_ref().map(|(first, _)| *first),
            _ => None,
        }
    }
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
/// `step` is the number of steps taken so far.
fn pick<C: Tally>(
    sims: &[Sim<'_, '_, C>],
    policy: Policy,
    rng: &mut u64,
    step: usize,
) -> Option<usize> {
    let live: Vec<usize> = (0..sims.len()).filter(|&n| sims[n].is_live()).collect();
    let first = *live.first()?;
    Some(match policy {
        Policy::Hold(steps) if first == 0 && (1..=steps).contains(&step) => {
            live.get(1).copied().unwrap_or(first)
        }
        Policy::First | Policy::Hold(_) => first,
        Policy::Seeded(_) => {
            *rng ^= *rng << 13;
            *rng ^= *rng >> 7;
            *rng ^= *rng << 17;
            let count = u64::try_from(live.len()).expect("few workers");
            live[usize::try_from(*rng % count).expect("below the count")]
        }
        Policy::LowestBandFirst => {
            let idle = live.iter().copied().find(|&n| sims[n].band().is_none());
            idle.unwrap_or_else(|| {
                live.iter().copied().max_by_key(|&n| sims[n].band().unwrap_or(0)).expect("live")
            })
        }
    })
}

/// Moves worker `sim` one step, `Worker::step` as `work` runs it, and returns the payload if
/// the step panicked. The worker moves into the step, so a panic drops it as the panic unwinds
/// out of the step, as it unwinds out of `work` on a thread; the worker is then `Panicked`.
fn advance<C: Tally>(
    sim: &mut Sim<'_, '_, C>,
    pixel: &impl Fn(u32, u32) -> PixelResult,
) -> Option<Payload> {
    let Sim::Live(worker) = core::mem::replace(sim, Sim::Panicked) else { return None };
    let stepped = catch_unwind(AssertUnwindSafe(move || {
        let mut worker = worker;
        let more = worker.step(pixel);
        (more, worker)
    }));
    match stepped {
        Ok((true, worker)) => *sim = Sim::Live(worker),
        Ok((false, worker)) => *sim = Sim::Done(worker.finish()),
        Err(payload) => return Some(payload),
    }
    None
}

/// The step budget of a simulation: each band taken once, each of its rows rendered once and
/// one more step to see it end; each worker also makes one last take. More steps mean a
/// scheduler that does not end.
fn budget(size: ImageSize, rows: u32, workers: usize) -> usize {
    let bands = widen(size.height().div_ceil(rows));
    bands * (widen(rows.min(size.height())) + 2) + workers
}

/// The outcomes of the simulated workers that ended, combined, and the first panic's payload
/// passed on with `resume_unwind`, as the threads' scope passes it on once every worker ended.
fn ended<C: Tally>(sims: Vec<Sim<'_, '_, C>>, panic: Option<Payload>) -> Outcome<u64, Coord, C> {
    if let Some(payload) = panic {
        resume_unwind(payload);
    }
    combine(
        0,
        sims.into_iter().map(|sim| match sim {
            Sim::Done(outcome) => outcome,
            Sim::Live(_) | Sim::Panicked => unreachable!("every worker ran to its end"),
        }),
    )
}

/// Which tally the simulated workers count with: none, as `render_pixels` runs them, or bands, as
/// `render_pixels_by_worker` does. Every simulated test runs under both.
#[derive(Clone, Copy, Debug)]
enum Counting {
    Nothing,
    Bands,
}

/// Every policy, under each tally.
fn with_tallies(
    policies: impl Iterator<Item = Policy>,
) -> impl Iterator<Item = (Policy, Counting)> {
    policies.flat_map(|policy| [(policy, Counting::Nothing), (policy, Counting::Bands)])
}

/// [`simulate_on`] the 13 by 9 image with `tally`. With the band tally, a render that succeeds
/// without a halt took every band.
fn simulate(
    (rows, tally): (u32, Counting),
    workers: usize,
    policy: Policy,
    pixel: &impl Fn(u32, u32) -> PixelResult,
    halt: Option<(usize, &Cell<bool>)>,
) -> Simulated {
    let size = ImageSize::new(WIDTH, HEIGHT).expect("size");
    match tally {
        Counting::Nothing => simulate_on::<NoTally>(size, rows, workers, policy, pixel, halt).0,
        Counting::Bands => {
            let (got, bands) = simulate_on::<BandTally>(size, rows, workers, policy, pixel, halt);
            if got.is_ok() && halt.is_none() {
                assert_eq!(bands, HEIGHT.div_ceil(rows), "{policy:?}: bands taken");
            }
            got
        }
    }
}

/// Runs `workers` simulated workers over an image of `size` in bands of `rows` in the order
/// `policy` chooses, with `pixel` as the pixel function, and returns what `render_pixels` would;
/// a panic in `pixel` reaches the caller once every simulated worker has ended. With `halt` set
/// to `(n, flag)`, the workers are halted before step `n`, as a failed spawn halts them, and
/// `flag` is raised at that moment.
fn simulate_on<C: Tally>(
    size: ImageSize,
    rows: u32,
    workers: usize,
    policy: Policy,
    pixel: &impl Fn(u32, u32) -> PixelResult,
    halt: Option<(usize, &Cell<bool>)>,
) -> (Simulated, u32) {
    let schedule = Schedule::new(1, BandRows::new(rows).expect("rows")).expect("schedule");
    let mut image = match blank(size) {
        Ok(image) => image,
        Err(error) => return (Err(error), 0),
    };
    let mut rng = match policy {
        Policy::Seeded(seed) => seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
        _ => 1,
    };
    let outcome = {
        let shared = Shared::new(size, schedule, image.bytes_mut().as_chunks_mut::<4>().0);
        let mut sims: Vec<Sim<'_, '_, C>> =
            (0..workers).map(|_| Sim::Live(Worker::new(&shared, 0))).collect();
        let budget = budget(size, rows, workers);
        let (mut steps, mut panic) = (0, None);
        while let Some(n) = pick(&sims, policy, &mut rng, steps) {
            assert!(steps < budget, "{policy:?}: no end within {budget} steps");
            if let Some((_, flag)) = halt.filter(|(at, _)| *at == steps) {
                shared.halt();
                flag.set(true);
            }
            let payload = advance(&mut sims[n], pixel);
            panic = panic.or(payload);
            steps += 1;
        }
        ended(sims, panic)
    };
    let bands = outcome.tally.bands();
    (outcome.into_result().map(|total| (image.bytes().to_vec(), total)), bands)
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
            for (policy, tally) in with_tallies(policies()) {
                let got = simulate((rows, tally), workers, policy, &pixel(&[]), None);
                assert_eq!(
                    got,
                    Ok((want.clone(), u64::from(WIDTH * HEIGHT))),
                    "{policy:?} {tally:?}, {workers} workers, bands of {rows}"
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
                for (policy, tally) in with_tallies(policies()) {
                    let got = simulate((rows, tally), workers, policy, &pixel(failing), None);
                    let want = Err(WorkerError::Pixel { x, y, error: (x, y) });
                    assert_eq!(
                        got, want,
                        "{policy:?} {tally:?}, {workers} workers, bands of {rows}, {failing:?}"
                    );
                }
            }
        }
    }
}

/// Oracle: the stop rule. A worker that has observed a failure in row `y` begins no row from `y`
/// down, and the failing worker leaves its band; rows above `y` still run, since an earlier
/// failure may hide there. In the simulation the failure is recorded in the same step it happens,
/// every worker observes it at its next step and a row is one step, so under every order every
/// pixel computed after the failure is above row `y`: "after" is exact here. On threads, when
/// another worker observes it depends on the scheduler (ADR 0003, clarification of 2026-11-14).
#[test]
fn after_a_failure_no_pixel_from_its_row_down_is_rendered() {
    let witnesses = Cell::new(0_usize);
    for (fx, fy) in [(0, 0), (6, 4), (12, 3), (5, 8)] {
        for rows in BANDS {
            for workers in WORKERS {
                for (policy, tally) in with_tallies(policies()) {
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
                    let got = simulate((rows, tally), workers, policy, &counting, None);
                    let label = format!(
                        "{policy:?} {tally:?}, {workers} workers, bands of {rows}, ({fx}, {fy})"
                    );
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

/// Oracle: the band starts, by hand. Bands of 2 rows of the 13 by 9 image start at rows 0, 2, 4,
/// 6 and 8. After the top band is taken and a failure in row `y` is recorded, the bands handed
/// out are exactly those starting above `y`; after a halt, none. This is the hand-out half of
/// the stop rule, which the pixel counts above cannot see: a band taken and left before its
/// first row computes no pixel (ADR 0003, clarification of 2026-11-14).
#[test]
fn after_a_failure_no_band_from_its_row_down_is_handed_out() {
    let size = ImageSize::new(WIDTH, HEIGHT).expect("size");
    let schedule = Schedule::new(1, BandRows::new(2).expect("rows")).expect("schedule");
    for failing_row in 0..=HEIGHT {
        for halted in [false, true] {
            let mut image = blank::<Coord>(size).expect("image");
            let shared = Shared::new(size, schedule, image.bytes_mut().as_chunks_mut::<4>().0);
            let mut taken = vec![shared.take().map(|(first, _)| first)];
            shared.fail(failing_row);
            if halted {
                shared.halt();
            }
            while let Some((first, _)) = shared.take() {
                taken.push(Some(first));
            }
            let mut want: Vec<Option<u32>> = vec![Some(0)];
            if !halted {
                want.extend((2..HEIGHT).step_by(2).filter(|&first| first < failing_row).map(Some));
            }
            assert_eq!(taken, want, "failure in row {failing_row}, halted: {halted}");
        }
    }
}

/// Oracle: hand arithmetic. Outcomes merge by summing (values and bands) and by keeping the
/// raster-earlier failure, in either order.
#[test]
fn outcomes_merge_to_the_sum_and_the_raster_earlier_failure() {
    let at = |x, y| Some(Failure { y, x, error: (x, y) });
    for (a, b, want) in [(at(9, 0), at(2, 5), (9, 0)), (at(3, 4), at(4, 4), (3, 4))] {
        for (first, second) in [(a, b), (b, a)] {
            let mut merged = Outcome { total: 2_u64, first, tally: BandTally(1) };
            merged.merge(Outcome { total: 5, first: second, tally: BandTally(3) });
            assert_eq!((merged.total, merged.tally), (7, BandTally(4)));
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
            for (policy, tally) in with_tallies(policies().take(20)) {
                for halt_at in [0, 1, 2, 5, 11, 40] {
                    let (halted, after) = (Cell::new(false), Cell::new(0_usize));
                    let counting = |x, y| {
                        if halted.get() {
                            after.set(after.get() + 1);
                        }
                        pixel(&[])(x, y)
                    };
                    let label = format!("{policy:?} {tally:?}, {workers} workers, bands of {rows}");
                    let got = simulate(
                        (rows, tally),
                        workers,
                        policy,
                        &counting,
                        Some((halt_at, &halted)),
                    );
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
/// Not on WebAssembly, which starts no thread at all (`tests/library.rs` checks that there).
#[cfg(not(target_family = "wasm"))]
struct RefuseAt {
    refuse: u32,
    started: Cell<u32>,
}

#[cfg(not(target_family = "wasm"))]
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
#[cfg(not(target_family = "wasm"))]
#[test]
fn a_refused_worker_is_reported_with_its_number_and_its_error() {
    use std::sync::atomic::AtomicUsize;
    let size = ImageSize::new(64, 48).expect("size");
    for (threads, refuse) in [(4, 1), (4, 2), (4, 3), (7, 6)] {
        let spawner = RefuseAt { refuse, started: Cell::new(0) };
        let asked = AtomicUsize::new(0);
        let schedule = Schedule::new(threads, BandRows::new(1).expect("rows")).expect("schedule");
        let counted = render_pixels_on::<BandTally, _, _, _, _>(&spawner, size, schedule, 0_u64, {
            |x, y| pixel(&[])(x, y)
        })
        .map(|rendered| (rendered.image, rendered.total));
        spawner.started.set(0);
        let got =
            render_pixels_on::<NoTally, _, _, _, _>(&spawner, size, schedule, 0_u64, |x, y| {
                let n = asked.fetch_add(1, Ordering::SeqCst);
                assert!(n < 64 * 48, "asked for more pixels than the image has");
                pixel(&[])(x, y)
            })
            .map(|rendered| (rendered.image, rendered.total));
        assert_eq!(got, counted, "both tallies");
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

/// The payload of the simulated pixel function's panic, raised with `resume_unwind`, which
/// unwinds like a panic but prints no message.
#[cfg(panic = "unwind")]
#[derive(Debug, PartialEq, Eq)]
struct PixelPanic(Coord);

/// Oracle: the halt rule after a panic, counted exactly. On a 64 by 512 image with one-row
/// bands the pixel function panics at `(0, 0)`. Under every order tried the panic reaches the
/// caller with its own payload and no image, and no pixel is computed after it: the panicking
/// worker halts the others, and each begins no row after that (a row is one step, so "after" is
/// exact). So the pixels computed are the panicking one and the whole rows rendered before it,
/// `1 + 64 * rows`, and for the scripted orders `rows` is known: worker 0 takes row 0 and
/// renders it first (`Hold(0)`: no row), after the first other worker has rendered 100 rows in
/// 300 steps of three (take, render, leave: `Hold(300)`), or last (`Hold(MAX)`: all 511 other
/// rows). Without the halt, the other workers render every row left: 32 705 pixels at 2 and 4
/// workers whatever the order.
#[cfg(panic = "unwind")]
#[test]
fn a_panic_in_the_pixel_function_stops_every_worker_at_its_next_row() {
    let size = ImageSize::new(64, 512).expect("size");
    let whole = |rows: u32| u64::from(1 + 64 * rows);
    let mut cases = vec![
        (Policy::First, 1, 1),
        (Policy::First, 2, 1),
        (Policy::First, 4, 1),
        (Policy::Hold(0), 4, 1),
        (Policy::Hold(300), 1, 1),
        (Policy::Hold(300), 2, whole(100)),
        (Policy::Hold(300), 4, whole(100)),
        (Policy::Hold(usize::MAX), 2, whole(511)),
        (Policy::Hold(usize::MAX), 4, whole(511)),
    ];
    // Seeded orders: the count is not known in advance, only its form.
    cases.extend((0..20).flat_map(|seed| [2, 4].map(|n| (Policy::Seeded(seed), n, 0))));
    for (policy, workers, want) in cases {
        let (asked, after, panicked) = (Cell::new(0_u64), Cell::new(0_u64), Cell::new(false));
        let counting = |x, y| {
            asked.set(asked.get() + 1);
            if panicked.get() {
                after.set(after.get() + 1);
            }
            if (x, y) == (0, 0) {
                panicked.set(true);
                resume_unwind(Box::new(PixelPanic((x, y))));
            }
            Ok(([9; 4], 1))
        };
        // The band-counting scheduler first: it must panic the same way, with the same count.
        let counted = catch_unwind(AssertUnwindSafe(|| {
            simulate_on::<BandTally>(size, 1, workers, policy, &counting, None)
        }));
        assert!(counted.is_err(), "{policy:?}: the band tally's panic reaches the caller");
        let counted_asked = asked.get();
        assert_eq!(after.get(), 0, "{policy:?}: the band tally computed a pixel after the panic");
        asked.set(0);
        panicked.set(false);
        let got = catch_unwind(AssertUnwindSafe(|| {
            simulate_on::<NoTally>(size, 1, workers, policy, &counting, None)
        }));
        assert_eq!(asked.get(), counted_asked, "{policy:?}: both tallies, the same pixels");
        let label = format!("{policy:?}, {workers} workers: {} pixels", asked.get());
        let payload = got.expect_err("the panic reaches the caller, with no image");
        assert_eq!(payload.downcast_ref::<PixelPanic>(), Some(&PixelPanic((0, 0))), "{label}");
        assert_eq!(after.get(), 0, "{label}, {} of them after the panic", after.get());
        if want == 0 {
            assert_eq!((asked.get() - 1) % 64, 0, "{label}: not whole rows and the panic");
        } else {
            assert_eq!(asked.get(), want, "{label}");
        }
    }
}

/// Simulated start-up: [`start_helpers`] with a start that refuses helper `refuse` (an `Other`
/// error) and otherwise adds a simulated worker, after which the workers started so far move
/// `gap` steps in the order `policy` picks, as threads already started run while the next is
/// started. After the refusal the started workers run to their end, as the scope joins them; the
/// calling worker never runs. Returns the start-up's result, the pixels computed after the
/// refusal, and whether any started worker was still running at the refusal with rows left.
fn simulate_refusal<C: Tally>(
    rows: u32,
    workers: u32,
    refuse: u32,
    gap: usize,
    policy: Policy,
) -> (Result<usize, WorkerError<Coord>>, u64, bool) {
    let size = ImageSize::new(WIDTH, HEIGHT).expect("size");
    let schedule = Schedule::new(1, BandRows::new(rows).expect("rows")).expect("schedule");
    let mut image = blank::<Coord>(size).expect("image");
    let shared = Shared::new(size, schedule, image.bytes_mut().as_chunks_mut::<4>().0);
    let (refused, after, pending) = (Cell::new(false), Cell::new(0_u64), Cell::new(false));
    let counting = |x, y| {
        if refused.get() {
            after.set(after.get() + 1);
        }
        pixel(&[])(x, y)
    };
    let mut rng = match policy {
        Policy::Seeded(seed) => seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
        _ => 1,
    };
    let (mut sims, mut steps) = (Vec::new(), 0);
    let started = start_helpers(&shared, workers, |worker| {
        if worker == refuse {
            let left = shared.wanted_end() > 0 && sims.iter().any(Sim::is_live);
            pending.set(left && lock(&shared.bands).len() > 0);
            refused.set(true);
            return Err(io::Error::other("refused by the test"));
        }
        sims.push(Sim::Live(Worker::<u64, Coord, C>::new(&shared, 0)));
        for _ in 0..gap {
            let Some(n) = pick(&sims, policy, &mut rng, steps) else { break };
            assert!(advance(&mut sims[n], &counting).is_none(), "no panic here");
            steps += 1;
        }
        Ok(())
    });
    let budget = budget(size, rows, widen(workers));
    while let Some(n) = pick(&sims, policy, &mut rng, steps) {
        assert!(steps < budget, "{policy:?}: no end within {budget} steps");
        assert!(advance(&mut sims[n], &counting).is_none(), "no panic here");
        steps += 1;
    }
    (started.map(|handles| handles.len()), after.get(), pending.get())
}

/// Oracle: the halt rule on a refused start, through the start-up the threads run. When helper
/// `refuse` cannot be started after helpers 1 to `refuse - 1` have started and run for `gap`
/// steps each, start-up returns `Spawn` for it, and no started worker computes a pixel after the
/// refusal, under every order, gap, band height and refused worker tried. `Hold(4)` stops the
/// first helper partway through its band, so it meets the halt between two rows of a band. Witness count: refusals
/// that came while a started worker was running and bands were left, so a start-up that did not
/// halt would have rendered more.
#[test]
fn after_a_refused_start_no_started_worker_begins_a_row() {
    let mut witnesses = 0_usize;
    for rows in [1, 2, 9] {
        for (workers, refuse) in [(2, 1), (4, 2), (4, 3), (7, 6)] {
            for gap in [0, 1, 3, 10] {
                let scripted = [Policy::First, Policy::Hold(4)];
                for policy in scripted.into_iter().chain((0..20).map(Policy::Seeded)) {
                    let (got, after, pending) =
                        simulate_refusal::<NoTally>(rows, workers, refuse, gap, policy);
                    let counted = simulate_refusal::<BandTally>(rows, workers, refuse, gap, policy);
                    assert_eq!(
                        (&got, after, pending),
                        (&counted.0, counted.1, counted.2),
                        "tallies"
                    );
                    let label = format!("{policy:?}, bands of {rows}, worker {refuse} of {workers} refused after {gap} steps");
                    let want = SpawnError::new(refuse, io::Error::other("refused by the test"));
                    assert_eq!(got, Err(WorkerError::Spawn(want)), "{label}");
                    assert_eq!(after, 0, "{label}: {after} pixels after the refusal");
                    witnesses += usize::from(pending);
                }
            }
        }
    }
    assert!(witnesses > 0, "no refusal came while a started worker had rows left");
}
