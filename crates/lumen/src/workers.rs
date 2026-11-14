//! This module owns running a pixel function over the row bands of an image on worker threads.
//!
//! The image is cut into bands of whole rows by `chunks_mut`, so the bands are contiguous,
//! disjoint slices of the one image buffer, and the borrow checker proves it. Bands are handed
//! out one at a time, top to bottom, to whichever worker asks next, so a slow region or a slow
//! core never holds up the others. A worker that takes a band owns that slice and writes each
//! pixel straight into it, at the place fixed by the pixel's coordinates: if the pixel function
//! is a pure function of `(x, y)`, the image is the same bytes for every thread count, band
//! height and hand-out order. Each worker returns its own sum and its own earliest failure; the
//! caller combines them after `join`. A worker that panics halts the others at their next row.
//! See `docs/adr/0003-row-bands.md`.

use core::iter::{StepBy, Zip};
use core::ops::{Add, Range};
use core::slice::ChunksMut;
use std::io;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread::{Scope, ScopedJoinHandle};

use crate::image::{ImageSize, Rgba8Image};
use crate::schedule::{widen, Schedule};

mod error;
mod outcome;

pub use error::{Reservation, SpawnError, WorkerError};
use outcome::{combine, Failure, Outcome};
pub use outcome::{Rendered, WorkerRun};

/// How [`render_pixels`] starts its helper threads: the seam a test drives to refuse a spawn.
pub(crate) trait Spawner {
    /// Starts worker `worker` (from 1) running `run` in `scope`.
    fn spawn<'scope, 'env, T, F>(
        &self,
        scope: &'scope Scope<'scope, 'env>,
        worker: u32,
        run: F,
    ) -> io::Result<ScopedJoinHandle<'scope, T>>
    where
        T: Send + 'scope,
        F: FnOnce() -> T + Send + 'scope;
}

/// The operating system's threads, named `lumen-worker-N`.
pub(crate) struct OsThreads;

impl Spawner for OsThreads {
    #[expect(
        clippy::disallowed_types,
        reason = "thread-spawn: the scheduler's helper threads (ADR 0003), started for \
                  render_pixels_on only"
    )]
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
        std::thread::Builder::new().name(format!("lumen-worker-{worker}")).spawn_scoped(scope, run)
    }
}

/// One pixel of the image: its four RGBA8 bytes.
type Rgba = [u8; 4];

/// The bands not yet handed out, top to bottom, each with the index of its first row.
type Bands<'a> = Zip<StepBy<Range<u32>>, ChunksMut<'a, Rgba>>;

/// The rows of one band, top to bottom, each with its row index.
type Rows<'a> = Zip<Range<u32>, ChunksMut<'a, Rgba>>;

/// Whether a row was rendered, or the worker is to leave its band: the row is no longer wanted
/// or one of its pixels failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Done,
    Leave,
}

/// What the workers share.
///
/// Lock: `bands`, the only one. It is held only to take the next band (`Iterator::next` on the
/// band iterator, a few integer steps): no other lock, no I/O and no call into the pixel function
/// happens under it. `end` is the only shared value a worker writes after the start.
struct Shared<'a> {
    /// The image width in pixels, the length of a row.
    width: usize,
    /// The image width, as the bound of the column index.
    columns: u32,
    /// The image height.
    height: u32,
    bands: Mutex<Bands<'a>>,
    /// Rows from `end` down are no longer wanted: the image height at the start, then the row of
    /// the earliest failure recorded so far, or 0 when a worker thread could not be started or a
    /// worker panicked (a halt). It only decreases.
    end: AtomicU32,
}

/// A guard on the band lock. A poisoned lock is still usable: the band iterator is changed only
/// by `next`, which does not panic, so a panic elsewhere leaves it consistent.
fn lock<V>(mutex: &Mutex<V>) -> MutexGuard<'_, V> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl<'a> Shared<'a> {
    /// The image `pixels` of `size`, cut into bands of `schedule.band()` rows.
    fn new(size: ImageSize, schedule: Schedule, pixels: &'a mut [Rgba]) -> Self {
        // `ImageSize` refuses a zero width; `max(1)` states it to `chunks_mut`, which needs it.
        let width = widen(size.width()).max(1);
        let band = schedule.band();
        let starts = (0..size.height()).step_by(widen(band.get()));
        Self {
            width,
            columns: size.width(),
            height: size.height(),
            bands: Mutex::new(starts.zip(pixels.chunks_mut(band.pixels(width)))),
            end: AtomicU32::new(size.height()),
        }
    }

    /// The next band and its first row, or `None` when none is left or the next one is not
    /// wanted. Bands are handed out top to bottom and `end` only decreases, so once a band is
    /// not wanted no later band is.
    fn take(&self) -> Option<(u32, Rows<'a>)> {
        let (first, band) = lock(&self.bands).next()?;
        let rows = (first..self.height).zip(band.chunks_mut(self.width));
        (first < self.wanted_end()).then_some((first, rows))
    }

    /// `end` with relaxed ordering: it is only a hint to stop early. Every value it takes is a
    /// failing row or 0 (a halt), never below the raster-first failing row, so a late view of it
    /// costs extra work at most, never a different result. The workers' results reach the
    /// caller through `join`, which orders them.
    fn wanted_end(&self) -> u32 {
        self.end.load(Ordering::Relaxed)
    }

    /// Renders row `y` into `row`, adding each pixel's value to `outcome`; or leaves, without a
    /// pixel, if the row is no longer wanted; or stops at the row's first failing pixel and
    /// records it.
    fn render_row<T, E, F>(
        &self,
        y: u32,
        row: &mut [Rgba],
        pixel: &F,
        outcome: &mut Outcome<T, E>,
    ) -> Row
    where
        T: Copy + Add<Output = T>,
        F: Fn(u32, u32) -> Result<(Rgba, T), E>,
    {
        if y >= self.wanted_end() {
            return Row::Leave;
        }
        for (x, out) in (0..self.columns).zip(row.iter_mut()) {
            match pixel(x, y) {
                Ok((rgba, value)) => {
                    *out = rgba;
                    outcome.total = outcome.total + value;
                }
                Err(error) => {
                    self.fail(y);
                    outcome.record(Some(Failure { y, x, error }));
                    return Row::Leave;
                }
            }
        }
        Row::Done
    }

    /// Marks rows from `y` down as no longer wanted, after a failure in row `y`. Every pixel
    /// before the failing one in raster order is in an earlier row, or earlier in row `y`,
    /// which only this worker renders and already has; so no row from `y` down is needed.
    fn fail(&self, y: u32) {
        self.end.fetch_min(y, Ordering::Relaxed);
    }

    /// Stops every worker at the next row it begins: a worker thread could not be started, or a
    /// worker panicked.
    fn halt(&self) {
        self.end.store(0, Ordering::Relaxed);
    }
}

/// Halts every worker when it is dropped while its thread unwinds from a panic: a worker that
/// panics in the pixel function stops the others at the next row each begins, instead of
/// leaving them to render the rest of the image before the panic reaches the caller.
struct HaltOnUnwind<'s, 'a>(&'s Shared<'a>);

impl Drop for HaltOnUnwind<'_, '_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.halt();
        }
    }
}

/// One worker between steps: its outcome so far and the band it is in, if any. `work` runs it to
/// the end on a thread; the unit tests' simulated workers run the same steps one at a time.
struct Worker<'s, 'a, T, E> {
    shared: &'s Shared<'a>,
    outcome: Outcome<T, E>,
    /// The band being rendered, with its first row, and the rows of it not yet begun.
    band: Option<(u32, Rows<'a>)>,
    /// Dropped with the worker: if a panic unwinds through the worker, the others are halted.
    _unwind: HaltOnUnwind<'s, 'a>,
}

impl<'s, 'a, T, E> Worker<'s, 'a, T, E>
where
    T: Copy + Add<Output = T>,
{
    const fn new(shared: &'s Shared<'a>, zero: T) -> Self {
        Self { shared, outcome: Outcome::new(zero), band: None, _unwind: HaltOnUnwind(shared) }
    }

    /// One step: take the next band, render the next row of the band, or leave the band when its
    /// rows are done or it is to be left. `false` when no band is left or wanted: the worker is
    /// done.
    fn step<F>(&mut self, pixel: &F) -> bool
    where
        F: Fn(u32, u32) -> Result<(Rgba, T), E>,
    {
        match self.band.as_mut() {
            None => {
                self.band = self.shared.take();
                let took = self.band.is_some();
                self.outcome.bands = self.outcome.bands.saturating_add(u32::from(took));
                took
            }
            Some((_, rows)) => {
                match rows.next() {
                    None => self.band = None,
                    Some((y, row)) => {
                        if self.shared.render_row(y, row, pixel, &mut self.outcome) == Row::Leave {
                            self.band = None;
                        }
                    }
                }
                true
            }
        }
    }

    /// The worker's sum and earliest failure.
    fn finish(self) -> Outcome<T, E> {
        self.outcome
    }
}

/// One worker: takes bands until none is left or wanted, renders each row by row, and returns
/// its sum and its earliest failure.
fn work<T, E, F>(shared: &Shared<'_>, pixel: &F, zero: T) -> Outcome<T, E>
where
    T: Copy + Add<Output = T>,
    F: Fn(u32, u32) -> Result<(Rgba, T), E>,
{
    let mut worker = Worker::new(shared, zero);
    while worker.step(pixel) {}
    worker.finish()
}

/// Starts helpers 1 to `workers - 1` with `start`, in order, and returns what it gave for each.
/// If `start` refuses one, halts the workers already started (they stop at their next row) and
/// returns [`WorkerError::Spawn`] for it with the refusal.
fn start_helpers<H, E>(
    shared: &Shared<'_>,
    workers: u32,
    mut start: impl FnMut(u32) -> io::Result<H>,
) -> Result<Vec<H>, WorkerError<E>> {
    // `Schedule::workers` is at least 1: the calling thread.
    let helpers = widen(workers).saturating_sub(1);
    let mut handles = Vec::new();
    handles.try_reserve_exact(helpers).map_err(|error| WorkerError::OutOfMemory {
        reserving: Reservation::Workers { handles: helpers },
        error,
    })?;
    for worker in 1..workers {
        match start(worker) {
            Ok(handle) => handles.push(handle),
            Err(error) => {
                shared.halt();
                return Err(WorkerError::Spawn(SpawnError::new(worker, error)));
            }
        }
    }
    Ok(handles)
}

/// Renders every pixel of `size` with `pixel` on `schedule.workers(size)` workers, the calling
/// thread being one of them, and returns the image and the sum of the pixels' values (`zero`
/// plus each pixel's `T`; lumen counts rays this way).
///
/// `pixel(x, y)` gives the four RGBA8 bytes of pixel `(x, y)` and its value. Bands of
/// `schedule.band()` whole rows are handed out top to bottom to whichever worker asks next, and
/// the worker writes the band's pixels straight into the image. With one thread no thread is
/// started and the calling thread renders every band in order: the same code path with one
/// worker. The image is the same bytes for every schedule whenever `pixel` is a pure function of
/// its coordinates, and the sum is the same whenever `+` is associative and commutative on the
/// values (as integer addition is).
///
/// A panic in `pixel` halts the other workers: each begins no row after it has seen the halt
/// (a row already begun is finished). The panic is passed on to the caller once every worker
/// has stopped, and no image is returned.
///
/// # Errors
///
/// [`WorkerError::Pixel`] for the first failing pixel in raster order (top row first, left to
/// right), the same at every thread count, band height and hand-out order. After a failure in
/// row `y`, workers begin no row from `y` down. [`WorkerError::OutOfMemory`] if the image or the
/// worker handles cannot be reserved, [`WorkerError::Spawn`] with the worker's number and the
/// operating system's error if a worker thread cannot be started; the workers already started
/// stop at their next row.
pub fn render_pixels<T, E, F>(
    size: ImageSize,
    schedule: Schedule,
    zero: T,
    pixel: F,
) -> Result<(Rgba8Image, T), WorkerError<E>>
where
    T: Copy + Add<Output = T> + Send,
    E: Send,
    F: Fn(u32, u32) -> Result<(Rgba, T), E> + Sync,
{
    let rendered = render_pixels_on(&OsThreads, size, schedule, zero, pixel)?;
    Ok((rendered.image, rendered.total))
}

/// [`render_pixels`], with what each worker did: its bands and sum ([`Rendered`]). Which
/// worker took which band differs from run to run; the image and the total do not.
///
/// # Errors
///
/// Those of [`render_pixels`].
pub fn render_pixels_by_worker<T, E, F>(
    size: ImageSize,
    schedule: Schedule,
    zero: T,
    pixel: F,
) -> Result<Rendered<T>, WorkerError<E>>
where
    T: Copy + Add<Output = T> + Send,
    E: Send,
    F: Fn(u32, u32) -> Result<(Rgba, T), E> + Sync,
{
    render_pixels_on(&OsThreads, size, schedule, zero, pixel)
}

/// [`render_pixels`] with `spawner` starting the helper threads.
#[expect(
    clippy::disallowed_methods,
    reason = "thread-spawn: the one thread pool (ADR 0003); each pixel is written at the place its \
              coordinates fix and the sums are integer, so no schedule or hand-out order reaches the \
              image or the ray count"
)]
pub(crate) fn render_pixels_on<S, T, E, F>(
    spawner: &S,
    size: ImageSize,
    schedule: Schedule,
    zero: T,
    pixel: F,
) -> Result<Rendered<T>, WorkerError<E>>
where
    S: Spawner,
    T: Copy + Add<Output = T> + Send,
    E: Send,
    F: Fn(u32, u32) -> Result<(Rgba, T), E> + Sync,
{
    let mut image = blank(size)?;
    let workers = schedule.workers(size);
    // One record per worker, reserved before any thread starts.
    let mut runs = Vec::new();
    runs.try_reserve_exact(widen(workers)).map_err(|error| WorkerError::OutOfMemory {
        reserving: Reservation::Workers { handles: widen(workers) },
        error,
    })?;
    let outcome = {
        let (pixels, _) = image.bytes_mut().as_chunks_mut::<4>();
        let shared = &Shared::new(size, schedule, pixels);
        let pixel = &pixel;
        std::thread::scope(|scope| {
            // On a refusal the scope joins the workers already started.
            let handles = start_helpers(shared, workers, |worker| {
                spawner.spawn(scope, worker, move || work(shared, pixel, zero))
            })?;
            let mine = work(shared, pixel, zero);
            let theirs = handles.into_iter().map(|handle| {
                handle.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            });
            let all = core::iter::once(mine).chain(theirs).inspect(|one| {
                runs.push(WorkerRun { bands: one.bands, total: one.total });
            });
            Ok(combine(zero, all))
        })?
    };
    let total = outcome.into_result()?;
    Ok(Rendered { image, total, workers: runs })
}

/// A zeroed image of `size`, reserved without aborting on failure.
fn blank<E>(size: ImageSize) -> Result<Rgba8Image, WorkerError<E>> {
    Rgba8Image::zeroed(size).map_err(|error| WorkerError::OutOfMemory {
        reserving: Reservation::Image { bytes: size.byte_len() },
        error,
    })
}

#[cfg(test)]
mod tests;
