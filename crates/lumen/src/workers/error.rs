//! Why [`render_pixels`](super::render_pixels) stopped: [`WorkerError`].

use core::fmt;

/// Why [`render_pixels`](super::render_pixels) stopped.
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
    /// Memory for the image or the worker handles could not be reserved.
    OutOfMemory,
    /// The operating system refused to start a worker thread.
    Spawn,
}

impl<E: fmt::Display> fmt::Display for WorkerError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pixel { x, y, error } => write!(f, "pixel ({x}, {y}): {error}"),
            Self::OutOfMemory => f.write_str("not enough memory for the image"),
            Self::Spawn => f.write_str("a worker thread could not be started"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for WorkerError<E> {}
