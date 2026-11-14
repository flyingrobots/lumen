//! Why [`render_pixels`](super::render_pixels) stopped: [`WorkerError`], with what was being
//! reserved ([`Reservation`]) or which worker could not start ([`SpawnError`]).

use core::fmt;
use std::collections::TryReserveError;
use std::io;
use std::sync::Arc;

/// Why [`render_pixels`](super::render_pixels) stopped.
///
/// Non-exhaustive: a later version may report a new failure, so a `match` outside this crate
/// needs a wildcard arm.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
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
    /// Memory could not be reserved.
    OutOfMemory {
        /// What was being reserved, and how much.
        reserving: Reservation,
        /// The allocator's refusal.
        error: TryReserveError,
    },
    /// The operating system refused to start a worker thread.
    Spawn(SpawnError),
}

impl<E: fmt::Display> fmt::Display for WorkerError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pixel { x, y, error } => write!(f, "pixel ({x}, {y}): {error}"),
            Self::OutOfMemory { reserving, error } => {
                write!(f, "not enough memory for {reserving}: {error}")
            }
            Self::Spawn(error) => write!(f, "{error}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for WorkerError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Pixel { error, .. } => Some(error),
            Self::OutOfMemory { error, .. } => Some(error),
            Self::Spawn(error) => Some(error),
        }
    }
}

/// What a render was reserving memory for when the allocator refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Reservation {
    /// The image buffer.
    Image {
        /// Its length, [`ImageSize::byte_len`](crate::ImageSize::byte_len).
        bytes: usize,
    },
    /// The join handles of the helper threads.
    Workers {
        /// How many handles.
        handles: usize,
    },
}

impl fmt::Display for Reservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Image { bytes } => write!(f, "the image's {bytes} bytes"),
            Self::Workers { handles } => write!(f, "{handles} worker thread handles"),
        }
    }
}

/// A worker thread the operating system refused to start: its number and the operating
/// system's error, which [`source`](std::error::Error::source) returns.
///
/// Workers are numbered from 1; the calling thread, which is always a worker, is 0. The error is
/// shared behind an `Arc` so that the render's errors stay `Clone`. Two `SpawnError`s are equal
/// when their workers are, and their errors have the same kind, operating-system code and text.
#[derive(Clone, Debug)]
pub struct SpawnError {
    worker: u32,
    error: Arc<io::Error>,
}

impl SpawnError {
    /// The error for worker `worker` (from 1) that could not be started.
    #[must_use]
    pub fn new(worker: u32, error: io::Error) -> Self {
        Self { worker, error: Arc::new(error) }
    }

    /// The number of the worker that could not be started, from 1.
    #[must_use]
    pub const fn worker(&self) -> u32 {
        self.worker
    }

    /// The operating system's error.
    #[must_use]
    pub fn error(&self) -> &io::Error {
        &self.error
    }
}

impl PartialEq for SpawnError {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (&*self.error, &*other.error);
        self.worker == other.worker
            && a.kind() == b.kind()
            && a.raw_os_error() == b.raw_os_error()
            && a.to_string() == b.to_string()
    }
}

impl Eq for SpawnError {}

impl fmt::Display for SpawnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "worker thread {} could not be started: {}", self.worker, self.error)
    }
}

impl std::error::Error for SpawnError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.error)
    }
}
