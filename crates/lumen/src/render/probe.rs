//! This module owns how a path asks its ray queries: through a [`Tracer`], counting nothing
//! ([`Plain`]), or through a [`CountingTracer`], recording each query's traversal work under its
//! kind of ray ([`Counted`]).
//!
//! The path is generic over the [`Probe`], so a render that does not ask for traversal statistics
//! calls the uncounted searches and carries a zero-sized record the compiler removes, the pattern
//! of the scheduler's tallies (ADR 0008). The record is written after each query answers; nothing
//! a path decides reads it.

use core::ops::Add;

use bunny_num::FixedQ32_32;
use bunny_query::TraceRay;

use super::{TraversalStatistics, TraversalWork};
use crate::scene::{CountingTracer, Hit, TraceError, Tracer};

/// Which ray a closest-hit query traces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Closest {
    /// The camera ray of a sample.
    Camera,
    /// The bounce ray from the first hit.
    Bounce,
}

/// The ray queries of a path, and what they record of the traversals' work.
pub(super) trait Probe {
    /// The record: [`Unmeasured`], or [`TraversalStatistics`].
    type Work: Copy + Default + Add<Output = Self::Work>;

    /// [`Tracer::closest`] for a ray of kind `kind`, its work added to `work`.
    fn closest(
        &self,
        ray: &TraceRay,
        kind: Closest,
        work: &mut Self::Work,
    ) -> Result<Option<Hit>, TraceError>;

    /// [`Tracer::occluded`] for a shadow ray, its work added to `work`.
    fn occluded(
        &self,
        ray: &TraceRay,
        t_max: FixedQ32_32,
        work: &mut Self::Work,
    ) -> Result<bool, TraceError>;
}

/// No traversal work recorded: zero-sized.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Unmeasured;

impl Add for Unmeasured {
    type Output = Self;

    fn add(self, _: Self) -> Self {
        Self
    }
}

/// A tracer's uncounted queries.
pub(super) struct Plain<'a, T: ?Sized>(pub(super) &'a T);

impl<T: Tracer + ?Sized> Probe for Plain<'_, T> {
    type Work = Unmeasured;

    fn closest(
        &self,
        ray: &TraceRay,
        _: Closest,
        _: &mut Unmeasured,
    ) -> Result<Option<Hit>, TraceError> {
        self.0.closest(ray)
    }

    fn occluded(
        &self,
        ray: &TraceRay,
        t_max: FixedQ32_32,
        _: &mut Unmeasured,
    ) -> Result<bool, TraceError> {
        self.0.occluded(ray, t_max)
    }
}

/// A counting tracer's counted queries, each one's counts added under its kind.
pub(super) struct Counted<'a, T: ?Sized>(pub(super) &'a T);

impl<T: CountingTracer + ?Sized> Probe for Counted<'_, T> {
    type Work = TraversalStatistics;

    fn closest(
        &self,
        ray: &TraceRay,
        kind: Closest,
        work: &mut TraversalStatistics,
    ) -> Result<Option<Hit>, TraceError> {
        let (hit, counts) = self.0.closest_counted(ray)?;
        let slot = match kind {
            Closest::Camera => &mut work.camera,
            Closest::Bounce => &mut work.bounce,
        };
        *slot = *slot + TraversalWork::ray(counts);
        Ok(hit)
    }

    fn occluded(
        &self,
        ray: &TraceRay,
        t_max: FixedQ32_32,
        work: &mut TraversalStatistics,
    ) -> Result<bool, TraceError> {
        let (blocked, counts) = self.0.occluded_counted(ray, t_max)?;
        work.shadow = work.shadow + TraversalWork::ray(counts);
        Ok(blocked)
    }
}
