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
/// rendered and the earliest failing pixel among them.
pub(super) struct Outcome<T, E> {
    pub(super) total: T,
    pub(super) first: Option<Failure<E>>,
}

impl<T: Copy + Add<Output = T>, E> Outcome<T, E> {
    pub(super) const fn new(zero: T) -> Self {
        Self { total: zero, first: None }
    }

    /// Adds another worker's outcome: the sums add, and the earlier failure is kept. Integer
    /// addition and a minimum do not depend on the order outcomes arrive in.
    pub(super) fn merge(&mut self, other: Self) {
        self.total = self.total + other.total;
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
