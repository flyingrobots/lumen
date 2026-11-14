//! The ray-query port: the two queries a path needs ([`Tracer`]), the hit it reports ([`Hit`])
//! and why a query failed ([`TraceError`]).

use core::fmt;

use bunny_broadphase::TraversalError;
use bunny_num::FixedQ32_32;
use bunny_query::{TraceRay, TriangleHit};

use super::PrimitiveIndex;

/// The farthest distance a camera or bounce ray searches: no limit, raw `i64::MAX`.
pub const FAR: FixedQ32_32 = FixedQ32_32::from_raw(i64::MAX);

/// The nearest triangle a ray hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    /// The primitive index of the triangle.
    pub primitive: PrimitiveIndex,
    /// Bunny's `TriangleHit` for that triangle: the distance (a multiple of the ray's
    /// direction, floored) and the facing.
    pub hit: TriangleHit,
}

/// Why a ray could not be traced.
///
/// Non-exhaustive: a later version may report a new failure, so a `match` outside this crate
/// needs a wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TraceError {
    /// Bunny's traversal refused the tree. Its error has no `Display` or `Error` of its own, so
    /// lumen names each kind, and there is no `source()`.
    Traversal(TraversalError),
    /// The traversal named a primitive the triangle list does not hold. `Scene::new` checks
    /// every index of its tree once, so the scene's own tracer never reports this.
    UnknownPrimitive {
        /// The primitive index named.
        primitive: PrimitiveIndex,
        /// The scene's triangle count: every valid index is below it.
        triangles: usize,
    },
    /// The traversal's nearest hit could not be found again by testing its triangle at exactly
    /// its distance. Bunny guarantees it can, so the scene's own tracer never reports this.
    HitNotRepeatable {
        /// The primitive index of the triangle.
        primitive: PrimitiveIndex,
        /// The distance the traversal reported, in multiples of the ray's direction.
        distance: FixedQ32_32,
    },
}

impl fmt::Display for TraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Traversal(error) => {
                let why = match error {
                    TraversalError::StackOverflow => "its stack of 64 nodes overflowed",
                    TraversalError::InvalidNodeIndex => "a node names a child outside the tree",
                    TraversalError::InvalidPrimitiveRange => {
                        "a leaf names primitives outside the primitive-index list"
                    }
                };
                write!(f, "traversal failed: {why}")
            }
            Self::UnknownPrimitive { primitive, triangles } => write!(
                f,
                "the traversal named triangle {primitive}, but the scene has {triangles} triangles"
            ),
            Self::HitNotRepeatable { primitive, distance } => write!(
                f,
                "the traversal's hit on triangle {primitive} at distance raw {} could not be \
                 repeated",
                distance.raw()
            ),
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
