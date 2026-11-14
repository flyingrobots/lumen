//! What Bunny's traversals did for a render's rays, per kind of ray: [`TraversalWork`] and
//! [`TraversalStatistics`] (ADR 0008).
//!
//! Every value is a sum or a maximum of Bunny's `TraversalCounts`, one record per query, so it
//! does not depend on the order the queries' records are combined in: the same at every thread
//! count and band height.

use core::ops::Add;

use bunny_broadphase::TraversalCounts;

/// The traversal work of the rays of one kind: their counts added (Bunny's `Add`, which sums
/// every counter and keeps the deepest stack), and the most any one of them took, counter by
/// counter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct TraversalWork {
    /// Every ray's counts, added. `total.peak_stack` is the deepest stack of any ray.
    pub total: TraversalCounts,
    /// Of each counter, the most any one ray took. Each maximum may be a different ray's.
    pub most: TraversalCounts,
}

impl TraversalWork {
    /// The work of one ray: its counts, as both the total and the most.
    #[must_use]
    pub(crate) const fn ray(counts: TraversalCounts) -> Self {
        Self { total: counts, most: counts }
    }
}

impl Add for TraversalWork {
    type Output = Self;

    /// The totals added and the larger of each maximum: associative and commutative, with the
    /// default (every counter zero) as the zero.
    fn add(self, other: Self) -> Self {
        Self { total: self.total + other.total, most: larger(self.most, other.most) }
    }
}

/// Counter by counter, the larger of `a` and `b`.
fn larger(a: TraversalCounts, b: TraversalCounts) -> TraversalCounts {
    let mut most = a;
    most.box_tests = a.box_tests.max(b.box_tests);
    most.box_entries = a.box_entries.max(b.box_entries);
    most.popped_skipped = a.popped_skipped.max(b.popped_skipped);
    most.interior_visits = a.interior_visits.max(b.interior_visits);
    most.leaf_visits = a.leaf_visits.max(b.leaf_visits);
    most.primitive_tests = a.primitive_tests.max(b.primitive_tests);
    most.peak_stack = a.peak_stack.max(b.peak_stack);
    most
}

/// The traversal work of a render's rays, by kind: camera and bounce rays (Bunny's closest-hit
/// search) and shadow rays (its any-hit search, which stops at the first hit). A rejected bounce
/// traced no ray and has no work. The number of rays of each kind, the divisor of a mean, is in
/// the render's [`RayOutcomes`](super::RayOutcomes).
///
/// A function of the scene, the size and the settings alone, like the ray statistics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct TraversalStatistics {
    /// The camera rays' work.
    pub camera: TraversalWork,
    /// The bounce rays' work.
    pub bounce: TraversalWork,
    /// The shadow rays' work.
    pub shadow: TraversalWork,
}

impl Add for TraversalStatistics {
    type Output = Self;

    /// Kind by kind.
    fn add(self, other: Self) -> Self {
        Self {
            camera: self.camera + other.camera,
            bounce: self.bounce + other.bounce,
            shadow: self.shadow + other.shadow,
        }
    }
}
