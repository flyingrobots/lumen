//! What the workers found: each worker's sum and earliest failing pixel ([`Outcome`]), and how
//! the outcomes of all the workers combine after `join` ([`combine`]).

use core::ops::Add;

use super::WorkerError;

/// A failing pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Failure<E> {
    pub(super) y: u32,
    pub(super) x: u32,
    pub(super) error: E,
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
/// rendered, the earliest failing pixel among them, and the bands taken.
pub(super) struct Outcome<T, E> {
    pub(super) total: T,
    pub(super) first: Option<Failure<E>>,
    /// Bands taken: at most the image's rows, so it never saturates. Counted once per band, not
    /// per row (ADR 0008).
    pub(super) bands: u32,
}

/// What one worker did in a render that succeeded: a fact about the run, not the image, since
/// which worker takes which band depends on timing (ADR 0008).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct WorkerRun<T> {
    /// The bands it took.
    pub bands: u32,
    /// The sum of its pixels' values.
    pub total: T,
}

/// An image, the sum of its pixels' values, and what each worker did, worker 0 (the calling
/// thread) first: the result of [`render_pixels_by_worker`](super::render_pixels_by_worker).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Rendered<T> {
    /// The image.
    pub image: crate::image::Rgba8Image,
    /// The sum of every pixel's value: the same at every schedule.
    pub total: T,
    /// Each worker's bands and sum, in worker order; they add up to the image's bands and
    /// `total`.
    pub workers: Vec<WorkerRun<T>>,
}

impl<T: Copy + Add<Output = T>, E> Outcome<T, E> {
    pub(super) const fn new(zero: T) -> Self {
        Self { total: zero, first: None, bands: 0 }
    }

    /// Adds another worker's outcome: the sums add, and the earlier failure is kept. Integer
    /// addition and a minimum do not depend on the order outcomes arrive in.
    pub(super) fn merge(&mut self, other: Self) {
        self.total = self.total + other.total;
        self.bands = self.bands.saturating_add(other.bands);
        self.record(other.first);
    }

    pub(super) fn record(&mut self, failure: Option<Failure<E>>) {
        self.first = match (self.first.take(), failure) {
            (Some(a), Some(b)) => Some(earlier(a, b)),
            (a, b) => a.or(b),
        };
    }

    /// The sum, or the earliest failure as the render's error.
    pub(super) fn into_result(self) -> Result<T, WorkerError<E>> {
        match self.first {
            Some(Failure { y, x, error }) => Err(WorkerError::Pixel { x, y, error }),
            None => Ok(self.total),
        }
    }
}

/// The outcomes of all the workers, combined: `zero` plus every sum, and the earliest failure.
pub(super) fn combine<T, E>(zero: T, outcomes: impl Iterator<Item = Outcome<T, E>>) -> Outcome<T, E>
where
    T: Copy + Add<Output = T>,
{
    outcomes.fold(Outcome::new(zero), |mut all, one| {
        all.merge(one);
        all
    })
}
