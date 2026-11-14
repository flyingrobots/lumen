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

/// What a worker counts besides its pixels' sum: nothing ([`NoTally`], a zero-sized type whose
/// counting is an empty function the optimiser can remove), or the bands it took
/// ([`BandTally`]). The scheduler is generic over it, so a render that did not ask for run
/// statistics does no counting work in the source; no cost above the noise of the runs ADR 0008
/// records was measured (+0.8 %, rank-sum z 1.33, 14 runs at 18 threads), and no IR or assembly
/// comparison is recorded.
pub(super) trait Tally: Copy + Send {
    /// Nothing counted yet.
    const ZERO: Self;
    /// Whether the scheduler keeps a record per worker.
    const RECORDS: bool;
    /// One more band taken.
    fn took_band(&mut self);
    /// Both tallies.
    #[must_use]
    fn and(self, other: Self) -> Self;
    /// The bands counted: zero for [`NoTally`].
    fn bands(self) -> u32;
}

/// Counts nothing: the tally of [`render_pixels`](super::render_pixels).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NoTally;

impl Tally for NoTally {
    const ZERO: Self = Self;
    const RECORDS: bool = false;
    fn took_band(&mut self) {}
    fn and(self, _: Self) -> Self {
        Self
    }
    fn bands(self) -> u32 {
        0
    }
}

/// Counts the bands taken: the tally of [`render_pixels_by_worker`](super::render_pixels_by_worker).
/// At most the image's rows, so it never saturates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BandTally(pub(super) u32);

impl Tally for BandTally {
    const ZERO: Self = Self(0);
    const RECORDS: bool = true;
    fn took_band(&mut self) {
        self.0 = self.0.saturating_add(1);
    }
    fn and(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }
    fn bands(self) -> u32 {
        self.0
    }
}

/// What one worker found, or what several found together: the sum of the values of the pixels
/// rendered, the earliest failing pixel among them, and the tally.
pub(super) struct Outcome<T, E, C = NoTally> {
    pub(super) total: T,
    pub(super) first: Option<Failure<E>>,
    pub(super) tally: C,
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

impl<T: Copy + Add<Output = T>, E, C: Tally> Outcome<T, E, C> {
    pub(super) const fn new(zero: T) -> Self {
        Self { total: zero, first: None, tally: C::ZERO }
    }

    /// Adds another worker's outcome: the sums add, and the earlier failure is kept. Integer
    /// addition and a minimum do not depend on the order outcomes arrive in.
    pub(super) fn merge(&mut self, other: Self) {
        self.total = self.total + other.total;
        self.tally = self.tally.and(other.tally);
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
pub(super) fn combine<T, E, C>(
    zero: T,
    outcomes: impl Iterator<Item = Outcome<T, E, C>>,
) -> Outcome<T, E, C>
where
    T: Copy + Add<Output = T>,
    C: Tally,
{
    outcomes.fold(Outcome::new(zero), |mut all, one| {
        all.merge(one);
        all
    })
}
