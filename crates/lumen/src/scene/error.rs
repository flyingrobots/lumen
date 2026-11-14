//! Why a scene could not be built: [`SceneError`], with the buffer a failed reservation was for
//! ([`SceneBuffer`]).

use core::fmt;
use std::collections::TryReserveError;

use bunny_broadphase::CompactError;
use bunny_linalg::FixedVec3;
use bunny_optics::{CameraError, ShadingError};

use super::PrimitiveIndex;
use crate::mesh::PlaceError;

/// Why a scene could not be built. Each variant carries what was observed and, where there is
/// one, the limit.
///
/// Non-exhaustive: a later version may refuse a scene for a new reason, so a `match` outside
/// this crate needs a wildcard arm. Not `Copy`: it can hold the allocator's error.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SceneError {
    /// The scene has no triangles.
    Empty,
    /// The scene has more triangles than a `u32` primitive index can name.
    TooManyTriangles {
        /// The triangle count given.
        triangles: usize,
        /// The most triangles, `u32::MAX`.
        limit: u32,
    },
    /// A vertex of the triangle at this primitive index has a coordinate outside the trace
    /// domain (`abs(raw) >= 2^40`, 256 units), the check Bunny's `TraceTriangle::new` makes.
    OutsideTraceDomain {
        /// The primitive index of the triangle.
        primitive: PrimitiveIndex,
        /// Which vertex, 0, 1 or 2: the first with a coordinate outside.
        vertex: u8,
        /// That vertex.
        point: FixedVec3,
    },
    /// Bunny's `TraceTriangle::new` refused the triangle at this primitive index although every
    /// coordinate is inside the trace domain. Bunny documents no other reason, so no input is
    /// known to reach this; it is here in case Bunny adds one.
    TriangleRefused {
        /// The primitive index of the triangle.
        primitive: PrimitiveIndex,
    },
    /// `material_of` does not have one entry per triangle.
    MaterialCount {
        /// The triangle count.
        triangles: usize,
        /// The length of `material_of`.
        material_indices: usize,
    },
    /// The triangle at this primitive index names a material that does not exist.
    UnknownMaterial {
        /// The primitive index of the triangle.
        primitive: PrimitiveIndex,
        /// The material index it names.
        material: u32,
        /// The material count: every valid index is below it.
        materials: usize,
    },
    /// The node count of the tree, twice the triangle count, does not fit a `usize`. Only on a
    /// target with a 32-bit `usize` and more than 2^31 triangles.
    NodeCountOverflow {
        /// The triangle count.
        triangles: usize,
    },
    /// Bunny's `build_bvh_sah` refused to build the tree. Its only documented reason is buffers
    /// too small, and lumen gives it room for `2 * triangles` nodes where it needs
    /// `2 * triangles - 1`, so no input is known to reach this. Bunny gives no reason of its own.
    BvhBuild {
        /// The triangle count.
        triangles: usize,
        /// The room given for nodes.
        nodes: usize,
    },
    /// Bunny's `compact_bvh` refused to convert the tree to its compact layout. Its error has no
    /// `Display` or `Error` of its own, so lumen names each kind, and there is no `source()`.
    BvhCompact(CompactError),
    /// Bunny's tree builder put a primitive index in the tree that names no triangle. Bunny's
    /// builder fills the buffer with indices of the boxes it is given, so no input reaches this;
    /// `Scene::new` checks it once so that no traversal of the scene can meet such an index.
    TreeNamesUnknownPrimitive {
        /// The index found in the tree.
        index: u32,
        /// The scene's triangle count: every valid index is below it.
        triangles: usize,
    },
    /// Memory for one of the scene's buffers could not be reserved.
    OutOfMemory {
        /// The buffer.
        buffer: SceneBuffer,
        /// The number of items asked for.
        items: usize,
        /// The allocator's refusal.
        error: TryReserveError,
    },
    /// A mesh of the scene could not be read or placed.
    Mesh(PlaceError),
    /// Bunny's `PinholeCamera::new` refused the camera placement: its position is outside the
    /// trace domain, its tangent is not in `(0, 256]`, its look direction is zero, or its up
    /// hint is zero or parallel to the look direction.
    Camera(CameraError),
    /// A lighting or material constant of a built-in scene is outside Bunny's accepted range.
    Constant(ShadingError),
}

/// One of the buffers [`Scene::new`](super::Scene::new) reserves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SceneBuffer {
    /// Bunny's prepared `TraceTriangle`s, one per triangle.
    Triangles,
    /// The triangles' `TriangleSurface`s.
    Surfaces,
    /// The triangles' bounding boxes, the tree builder's input.
    Boxes,
    /// The tree's nodes as Bunny's builder writes them.
    Nodes,
    /// The tree's primitive-index buffer.
    PrimitiveIndices,
    /// The tree's nodes in Bunny's compact layout.
    CompactNodes,
    /// The material index of each triangle.
    MaterialIndices,
    /// The materials.
    Materials,
}

impl fmt::Display for SceneBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Triangles => "triangles",
            Self::Surfaces => "surfaces",
            Self::Boxes => "bounding boxes",
            Self::Nodes => "tree nodes",
            Self::PrimitiveIndices => "tree primitive indices",
            Self::CompactNodes => "compact tree nodes",
            Self::MaterialIndices => "material indices",
            Self::Materials => "materials",
        })
    }
}

/// Bunny's `CompactError`, in words.
const fn compact_reason(error: CompactError) -> &'static str {
    match error {
        CompactError::InvalidNodeIndex => "a node names a child outside the tree",
        CompactError::OutputTooSmall => "the output holds fewer nodes than the tree has",
        CompactError::NotATree => "a node is reached twice",
        CompactError::TooDeep => "the tree is deeper than 64 levels",
    }
}

impl fmt::Display for SceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("scene has no triangles"),
            Self::TooManyTriangles { triangles, limit } => {
                write!(f, "scene has {triangles} triangles, more than {limit}")
            }
            Self::OutsideTraceDomain { primitive, vertex, point } => write!(
                f,
                "triangle {primitive} has vertex {vertex} at raw ({}, {}, {}), outside the trace \
                 domain",
                point.x.raw(),
                point.y.raw(),
                point.z.raw(),
            ),
            Self::TriangleRefused { primitive } => {
                write!(f, "triangle {primitive} was refused, with every coordinate in the domain")
            }
            Self::MaterialCount { triangles, material_indices } => write!(
                f,
                "scene needs one material index per triangle: {triangles} triangles, \
                 {material_indices} material indices"
            ),
            Self::UnknownMaterial { primitive, material, materials } => write!(
                f,
                "triangle {primitive} names material {material}, but the scene has {materials} \
                 materials"
            ),
            Self::NodeCountOverflow { triangles } => {
                write!(f, "a tree over {triangles} triangles has more nodes than a usize counts")
            }
            Self::BvhBuild { triangles, nodes } => write!(
                f,
                "the bounding volume hierarchy over {triangles} triangles could not be built in \
                 {nodes} nodes"
            ),
            Self::BvhCompact(error) => write!(
                f,
                "the bounding volume hierarchy could not be converted: {}",
                compact_reason(*error)
            ),
            Self::TreeNamesUnknownPrimitive { index, triangles } => write!(
                f,
                "the bounding volume hierarchy names triangle {index}, but the scene has \
                 {triangles} triangles"
            ),
            Self::OutOfMemory { buffer, items, error } => {
                write!(f, "not enough memory for the scene's {items} {buffer}: {error}")
            }
            Self::Mesh(error) => write!(f, "a mesh could not be placed: {error}"),
            Self::Camera(error) => write!(f, "invalid camera placement: {error}"),
            Self::Constant(error) => write!(f, "a lighting or material constant: {error}"),
        }
    }
}

impl std::error::Error for SceneError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::OutOfMemory { error, .. } => Some(error),
            Self::Mesh(error) => Some(error),
            Self::Camera(error) => Some(error),
            Self::Constant(error) => Some(error),
            Self::Empty
            | Self::TooManyTriangles { .. }
            | Self::OutsideTraceDomain { .. }
            | Self::TriangleRefused { .. }
            | Self::MaterialCount { .. }
            | Self::UnknownMaterial { .. }
            | Self::NodeCountOverflow { .. }
            | Self::BvhBuild { .. }
            | Self::BvhCompact(_)
            | Self::TreeNamesUnknownPrimitive { .. } => None,
        }
    }
}
