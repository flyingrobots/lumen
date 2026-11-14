//! The name of one triangle of a scene: [`PrimitiveIndex`].

use core::fmt;

use crate::schedule::widen;

/// The index of a triangle in its scene: its zero-based position in [`SceneSpec::triangles`],
/// in the order the caller gave them.
///
/// **Representation.** A `u32`, the width of the primitive-index buffer that Bunny's tree
/// builders fill and its traversals report, so a scene holds at most `u32::MAX` triangles
/// ([`SceneError::TooManyTriangles`]). [`get`](Self::get) returns the integer unchanged; there
/// is no other encoding.
///
/// **Order.** Indices compare as integers. The order is not incidental: of triangles hit at the
/// same distance, [`Tracer::closest`] reports the lowest index, so two scenes with the same
/// triangles in a different order can render different pixels. The order of
/// [`SceneSpec::triangles`] is therefore part of a scene's identity, and a content digest of a
/// scene must cover its triangles, and the material index of each, in this order.
///
/// [`SceneSpec::triangles`]: super::SceneSpec::triangles
/// [`SceneError::TooManyTriangles`]: super::SceneError::TooManyTriangles
/// [`Tracer::closest`]: super::Tracer::closest
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrimitiveIndex(u32);

impl PrimitiveIndex {
    /// The triangle at zero-based position `index`. Any `u32` names a position; whether a scene
    /// has a triangle there is the scene's to answer (for example [`Scene::surface`] is `None`
    /// past its end).
    ///
    /// [`Scene::surface`]: super::Scene::surface
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// The position as the `u32` Bunny's traversals use.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The position as a `usize`, for indexing a slice in primitive order. Lossless: lumen
    /// builds only for targets whose `usize` has at least 32 bits, which a compile-time assertion
    /// checks.
    #[must_use]
    pub const fn to_usize(self) -> usize {
        widen(self.0)
    }
}

impl From<u32> for PrimitiveIndex {
    fn from(index: u32) -> Self {
        Self(index)
    }
}

impl From<PrimitiveIndex> for u32 {
    fn from(index: PrimitiveIndex) -> Self {
        index.0
    }
}

/// The decimal position, as in "triangle 7".
impl fmt::Display for PrimitiveIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
