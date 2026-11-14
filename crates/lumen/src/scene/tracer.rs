//! The ray-query port: the two queries a path needs ([`Tracer`]), the hit it reports ([`Hit`])
//! and why a query failed ([`TraceError`]).

use core::fmt;

use bunny_broadphase::TraversalError;
use bunny_num::FixedQ32_32;
use bunny_query::{TraceRay, TriangleHit};

/// The farthest distance a camera or bounce ray searches: no limit, raw `i64::MAX`.
pub const FAR: FixedQ32_32 = FixedQ32_32::from_raw(i64::MAX);

/// The nearest triangle a ray hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    /// The primitive index of the triangle.
    pub primitive: u32,
    /// Bunny's `TriangleHit` for that triangle: the distance (a multiple of the ray's
    /// direction, floored) and the facing.
    pub hit: TriangleHit,
}

/// Why a ray could not be traced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceError {
    /// The traversal refused the tree.
    Traversal(TraversalError),
    /// The traversal named a primitive that the triangle list does not hold, or whose hit
    /// could not be repeated. Neither can happen with a tree built by
    /// [`Scene::new`](super::Scene::new).
    InconsistentHit,
}

impl fmt::Display for TraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Traversal(error) => write!(f, "traversal failed: {error:?}"),
            Self::InconsistentHit => f.write_str("traversal reported no valid hit"),
        }
    }
}

impl std::error::Error for TraceError {}

/// The two ray queries a path needs. [`Scene`](super::Scene) implements it with Bunny's
/// traversals; a test can wrap a scene to check every query against a test of every triangle.
pub trait Tracer {
    /// The nearest triangle `ray` hits at a distance in `0..=FAR`, or `None` for a miss. Of
    /// triangles at the same distance the lowest primitive index wins.
    ///
    /// # Errors
    ///
    /// A [`TraceError`] if the traversal fails.
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError>;

    /// Whether `ray` hits any triangle at a distance in `0..=t_max`.
    ///
    /// # Errors
    ///
    /// A [`TraceError`] if the traversal fails.
    fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError>;
}
