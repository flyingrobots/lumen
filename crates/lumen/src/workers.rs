//! This module owns running a pixel function over the tiles of an image on worker threads and
//! assembling the image.
//!
//! Tiles are handed out one at a time from a shared counter, in row-major order, to whichever
//! worker asks next, so a slow region or a slow core never holds up the others. A worker renders
//! a whole tile into its own buffer, then copies it into the image at the tile's place. Each
//! pixel is written once, by the worker that computed it, at a position fixed by its
//! coordinates: if the pixel function is a pure function of `(x, y)`, the image is the same
//! bytes for every thread count, tile size and hand-out order. See
//! `docs/adr/0001-tile-scheduling.md`.

use core::fmt;
use core::ops::Add;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::image::{ImageSize, Rgba8Image};
use crate::schedule::{widen, Schedule, TileRect, Tiles};

/// Why [`render_pixels`] stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerError<E> {
    /// The pixel function failed at `(x, y)`: the first failing pixel in raster order.
    Pixel {
        /// The pixel column.
        x: u32,
        /// The pixel row.
        y: u32,
        /// The pixel function's error.
        error: E,
    },
    /// Memory for the image or a tile buffer could not be reserved.
    OutOfMemory,
    /// The operating system refused to start a worker thread.
    Spawn,
    /// A tile did not fit the image or its buffer. The tile geometry guarantees this never
    /// happens; it is reported rather than allowed to panic inside a worker.
    TileGeometry,
}

impl<E: fmt::Display> fmt::Display for WorkerError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pixel { x, y, error } => write!(f, "pixel ({x}, {y}): {error}"),
            Self::OutOfMemory => f.write_str("not enough memory for the image"),
            Self::Spawn => f.write_str("a worker thread could not be started"),
            Self::TileGeometry => f.write_str("a tile did not fit the image (a lumen bug)"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for WorkerError<E> {}

/// The first failing pixel found so far.
#[derive(Clone, Copy)]
struct Failure<E> {
    y: u32,
    x: u32,
    error: E,
}

/// What the workers record for the end of the render: the first failing pixel, a reason to
/// stop that is not a pixel's, and the sum of the finished workers' values.
struct Record<T, E> {
    first: Option<Failure<E>>,
    halt: Option<WorkerError<E>>,
    total: T,
}

/// What the workers share.
///
/// Locks: `record` and `image`. Each is taken alone, never while the other is held, and held
/// only to read or replace a few values or to copy one finished tile; no I/O and no call into
/// the pixel function happens under either lock.
struct Shared<'a, T, E> {
    tiles: Tiles,
    /// The next tile to hand out. It is claimed at most once per tile plus once per worker,
    /// at most `2^27 + 256` times, so it never wraps.
    next: AtomicU32,
    /// The last tile still worth rendering: the last tile, or the end of the row of tiles of
    /// the earliest failure so far.
    last: AtomicU32,
    /// Set when the render stops for a reason that is not a pixel's.
    halted: AtomicBool,
    record: Mutex<Record<T, E>>,
    image: Mutex<&'a mut [u8]>,
}

/// One tile being rendered: its index and pixels.
#[derive(Clone, Copy, Debug)]
struct Job {
    index: u32,
    rect: TileRect,
}

/// A worker's own state: its tile buffer and the sum of its pixels' values.
struct Lane<T> {
    buffer: Vec<u8>,
    total: T,
}

/// Whether a tile row was rendered or the tile is no longer wanted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Done,
    Abandon,
}

/// A guard on a lock that a panicking worker may have poisoned; the data stays usable because
/// every update under these locks is a single assignment or a whole-tile copy.
fn lock<V>(mutex: &Mutex<V>) -> MutexGuard<'_, V> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl<'a, T: Copy + Add<Output = T>, E: Copy> Shared<'a, T, E> {
    fn new(tiles: Tiles, image: &'a mut [u8], zero: T) -> Self {
        Self {
            tiles,
            next: AtomicU32::new(0),
            last: AtomicU32::new(tiles.last_index()),
            halted: AtomicBool::new(false),
            record: Mutex::new(Record { first: None, halt: None, total: zero }),
            image: Mutex::new(image),
        }
    }

    /// Whether tile `index` still matters: no failure in an earlier row of tiles, no halt.
    fn wanted(&self, index: u32) -> bool {
        index <= self.last.load(Ordering::SeqCst) && !self.halted.load(Ordering::SeqCst)
    }

    /// The next tile to render, or `None` when there is none left or none is wanted. Indices
    /// only grow, so once one is unwanted every later one is too.
    fn claim(&self) -> Option<Job> {
        let index = self.next.fetch_add(1, Ordering::SeqCst);
        let rect = self.tiles.rect(index)?;
        self.wanted(index).then_some(Job { index, rect })
    }

    /// Renders row `y` of `job` into the lane, or records the row's first failing pixel.
    fn render_row<F>(&self, job: Job, y: u32, pixel: &F, lane: &mut Lane<T>) -> Row
    where
        F: Fn(u32, u32) -> Result<([u8; 4], T), E>,
    {
        if !self.wanted(job.index) {
            return Row::Abandon;
        }
        for x in job.rect.x0..job.rect.x1 {
            match pixel(x, y) {
                Ok((rgba, value)) => {
                    lane.buffer.extend_from_slice(&rgba);
                    lane.total = lane.total + value;
                }
                Err(error) => {
                    self.fail(job.index, Failure { y, x, error });
                    return Row::Abandon;
                }
            }
        }
        Row::Done
    }

    /// Records a failure in tile `index`. Every pixel before it in raster order lies in this
    /// row of tiles or an earlier one, so no tile after this row's end is needed any more, and
    /// the earliest failure among the rest is the one a raster-order render meets first.
    fn fail(&self, index: u32, failure: Failure<E>) {
        self.last.fetch_min(self.tiles.row_end(index), Ordering::SeqCst);
        let mut record = lock(&self.record);
        if record.first.is_none_or(|f| (failure.y, failure.x) < (f.y, f.x)) {
            record.first = Some(failure);
        }
    }

    /// Stops every worker for a reason that is not a pixel's; the first reason is kept.
    fn halt(&self, error: WorkerError<E>) {
        lock(&self.record).halt.get_or_insert(error);
        self.halted.store(true, Ordering::SeqCst);
    }

    /// Copies a finished tile from the lane into the image, row by row, with every offset
    /// checked; `None` if a row does not fit (never, for a tile of [`Tiles`]).
    fn store(&self, job: Job, lane: &Lane<T>) -> Option<()> {
        let stride = widen(self.tiles.size().width()).checked_mul(4)?;
        let row_bytes = widen(job.rect.width()).checked_mul(4)?;
        let start = widen(job.rect.x0).checked_mul(4)?;
        let mut image = lock(&self.image);
        for (row, y) in (job.rect.y0..job.rect.y1).enumerate() {
            let from = row.checked_mul(row_bytes)?;
            let source = lane.buffer.get(from..from.checked_add(row_bytes)?)?;
            let at = widen(y).checked_mul(stride)?.checked_add(start)?;
            image.get_mut(at..at.checked_add(row_bytes)?)?.copy_from_slice(source);
        }
        Some(())
    }

    /// Adds a finished worker's sum to the total.
    fn add(&self, value: T) {
        let mut record = lock(&self.record);
        record.total = record.total + value;
    }

    /// Renders one claimed tile into the lane and stores it, unless it is abandoned.
    fn run_job<F>(&self, job: Job, pixel: &F, lane: &mut Lane<T>)
    where
        F: Fn(u32, u32) -> Result<([u8; 4], T), E>,
    {
        lane.buffer.clear();
        let done =
            (job.rect.y0..job.rect.y1).all(|y| self.render_row(job, y, pixel, lane) == Row::Done);
        if done && self.store(job, lane).is_none() {
            self.halt(WorkerError::TileGeometry);
        }
    }
}

/// One worker: claims tiles until none is left or wanted and renders each.
fn work<T, E, F>(shared: &Shared<'_, T, E>, pixel: &F, zero: T)
where
    T: Copy + Add<Output = T>,
    E: Copy,
    F: Fn(u32, u32) -> Result<([u8; 4], T), E>,
{
    let mut lane = Lane { buffer: Vec::new(), total: zero };
    let Some(capacity) = shared.tiles.max_tile_bytes() else {
        shared.halt(WorkerError::TileGeometry);
        return;
    };
    if lane.buffer.try_reserve_exact(capacity).is_err() {
        shared.halt(WorkerError::OutOfMemory);
        return;
    }
    while let Some(job) = shared.claim() {
        shared.run_job(job, pixel, &mut lane);
    }
    shared.add(lane.total);
}

/// Renders every pixel of `size` with `pixel` on `schedule.workers(size)` workers, the calling
/// thread being one of them, and returns the image and the sum of the pixels' values (`zero`
/// plus each pixel's `T`; lumen counts rays this way).
///
/// `pixel(x, y)` gives the four RGBA8 bytes of pixel `(x, y)` and its value. Tiles of
/// `schedule.tile()` are handed out from a shared counter to whichever worker asks next. With
/// one thread no thread is started and the calling thread renders every tile in order: the same
/// code path with one worker. The image is the same bytes for every schedule whenever `pixel` is
/// a pure function of its coordinates, and the sum is the same whenever `+` is associative and
/// commutative on the values (as integer addition is).
///
/// # Errors
///
/// [`WorkerError::Pixel`] for the first failing pixel in raster order (top row first, left to
/// right), the same at every thread count, tile size and hand-out order. After a failure,
/// workers take no tile past the end of that row of tiles and abandon such a tile at its next
/// row. [`WorkerError::OutOfMemory`] if a buffer cannot be reserved, [`WorkerError::Spawn`] if a
/// worker thread cannot be started, [`WorkerError::TileGeometry`] if a tile does not fit.
pub fn render_pixels<T, E, F>(
    size: ImageSize,
    schedule: Schedule,
    zero: T,
    pixel: F,
) -> Result<(Rgba8Image, T), WorkerError<E>>
where
    T: Copy + Add<Output = T> + Send,
    E: Copy + Send,
    F: Fn(u32, u32) -> Result<([u8; 4], T), E> + Sync,
{
    let tiles = Tiles::new(size, schedule.tile()).ok_or(WorkerError::TileGeometry)?;
    let mut bytes = blank(size)?;
    let shared = Shared::new(tiles, &mut bytes, zero);
    let (shared_ref, pixel_ref) = (&shared, &pixel);
    std::thread::scope(|scope| {
        for n in 1..schedule.workers(size) {
            let spawned = std::thread::Builder::new()
                .name(format!("lumen-worker-{n}"))
                .spawn_scoped(scope, move || work(shared_ref, pixel_ref, zero));
            if spawned.is_err() {
                shared.halt(WorkerError::Spawn);
                return;
            }
        }
        work(&shared, &pixel, zero);
    });
    let total = finish(shared)?;
    let image = Rgba8Image::from_bytes(size, bytes).ok_or(WorkerError::TileGeometry)?;
    Ok((image, total))
}

/// A zeroed RGBA8 buffer for `size`, reserved without aborting on failure.
fn blank<E>(size: ImageSize) -> Result<Vec<u8>, WorkerError<E>> {
    let len = size.byte_len().ok_or(WorkerError::OutOfMemory)?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(len).map_err(|_| WorkerError::OutOfMemory)?;
    bytes.resize(len, 0);
    Ok(bytes)
}

/// What the workers recorded: a halt, else the first failing pixel, else the total.
fn finish<T, E>(shared: Shared<'_, T, E>) -> Result<T, WorkerError<E>> {
    let record = shared.record.into_inner().unwrap_or_else(PoisonError::into_inner);
    if let Some(error) = record.halt {
        return Err(error);
    }
    if let Some(f) = record.first {
        return Err(WorkerError::Pixel { x: f.x, y: f.y, error: f.error });
    }
    Ok(record.total)
}

#[cfg(test)]
mod tests;
