//! This module owns running a pixel function over the row bands of an image on worker threads.
//!
//! The image is cut into bands of whole rows by `chunks_mut`, so the bands are contiguous,
//! disjoint slices of the one image buffer, and the borrow checker proves it. Bands are handed
//! out one at a time, top to bottom, to whichever worker asks next, so a slow region or a slow
//! core never holds up the others. A worker that takes a band owns that slice and writes each
//! pixel straight into it, at the place fixed by the pixel's coordinates: if the pixel function
//! is a pure function of `(x, y)`, the image is the same bytes for every thread count, band
//! height and hand-out order. Each worker returns its own sum and its own earliest failure; the
//! caller combines them after `join`. See `docs/adr/0002-row-bands.md`.

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

pub use error::{Reservation, SpawnError, WorkerError};

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

/// A failing pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Failure<E> {
    y: u32,
    x: u32,
    error: E,
}

/// The one of two failures that comes first in raster order: the smaller `(y, x)`.
fn earlier<E>(a: Failure<E>, b: Failure<E>) -> Failure<E> {
    if (b.y, b.x) < (a.y, a.x) {
        b
    } else {
        a
    }
}

/// What one worker found, or what several found together: the sum of the values of the pixels
/// rendered and the earliest failing pixel among them.
struct Outcome<T, E> {
    total: T,
    first: Option<Failure<E>>,
}

impl<T: Copy + Add<Output = T>, E> Outcome<T, E> {
    const fn new(zero: T) -> Self {
        Self { total: zero, first: None }
    }

    /// Adds another worker's outcome: the sums add, and the earlier failure is kept. Integer
    /// addition and a minimum do not depend on the order outcomes arrive in.
    fn merge(&mut self, other: Self) {
        self.total = self.total + other.total;
        self.record(other.first);
    }

    fn record(&mut self, failure: Option<Failure<E>>) {
        self.first = match (self.first.take(), failure) {
            (Some(a), Some(b)) => Some(earlier(a, b)),
            (a, b) => a.or(b),
        };
    }

    /// The sum, or the earliest failure as the render's error.
    fn into_result(self) -> Result<T, WorkerError<E>> {
        match self.first {
            Some(Failure { y, x, error }) => Err(WorkerError::Pixel { x, y, error }),
            None => Ok(self.total),
        }
    }
}

/// The outcomes of all the workers, combined: `zero` plus every sum, and the earliest failure.
fn combine<T, E>(zero: T, outcomes: impl Iterator<Item = Outcome<T, E>>) -> Outcome<T, E>
where
    T: Copy + Add<Output = T>,
{
    outcomes.fold(Outcome::new(zero), |mut all, one| {
        all.merge(one);
        all
    })
}

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
    /// the earliest failure recorded so far, or 0 when a worker thread could not be started. It
    /// only decreases.
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

    /// Stops every worker at its next row, because a worker thread could not be started.
    fn halt(&self) {
        self.end.store(0, Ordering::Relaxed);
    }
}

/// One worker: takes bands until none is left or wanted, renders each row by row, and returns
/// its sum and its earliest failure.
fn work<T, E, F>(shared: &Shared<'_>, pixel: &F, zero: T) -> Outcome<T, E>
where
    T: Copy + Add<Output = T>,
    F: Fn(u32, u32) -> Result<(Rgba, T), E>,
{
    let mut outcome = Outcome::new(zero);
    while let Some((_, rows)) = shared.take() {
        for (y, row) in rows {
            if shared.render_row(y, row, pixel, &mut outcome) == Row::Leave {
                break;
            }
        }
    }
    outcome
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
/// A panic in `pixel` is passed on to the caller once every worker has stopped.
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
    render_pixels_on(&OsThreads, size, schedule, zero, pixel)
}

/// [`render_pixels`] with `spawner` starting the helper threads.
pub(crate) fn render_pixels_on<S, T, E, F>(
    spawner: &S,
    size: ImageSize,
    schedule: Schedule,
    zero: T,
    pixel: F,
) -> Result<(Rgba8Image, T), WorkerError<E>>
where
    S: Spawner,
    T: Copy + Add<Output = T> + Send,
    E: Send,
    F: Fn(u32, u32) -> Result<(Rgba, T), E> + Sync,
{
    let mut image = blank(size)?;
    let workers = schedule.workers(size);
    // `Schedule::workers` is at least 1: the calling thread.
    let helpers = widen(workers).saturating_sub(1);
    let outcome = {
        let (pixels, _) = image.bytes_mut().as_chunks_mut::<4>();
        let shared = &Shared::new(size, schedule, pixels);
        let pixel = &pixel;
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            handles.try_reserve_exact(helpers).map_err(|error| WorkerError::OutOfMemory {
                reserving: Reservation::Workers { handles: helpers },
                error,
            })?;
            for worker in 1..workers {
                match spawner.spawn(scope, worker, move || work(shared, pixel, zero)) {
                    Ok(handle) => handles.push(handle),
                    Err(error) => {
                        // The scope joins the workers already started; they stop at their next
                        // row.
                        shared.halt();
                        return Err(WorkerError::Spawn(SpawnError::new(worker, error)));
                    }
                }
            }
            let mine = work(shared, pixel, zero);
            let theirs = handles.into_iter().map(|handle| {
                handle.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            });
            Ok(combine(zero, core::iter::once(mine).chain(theirs)))
        })?
    };
    let total = outcome.into_result()?;
    Ok((image, total))
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
