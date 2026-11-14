//! Why a scene could not be built: [`SceneError`].

use core::fmt;

use bunny_optics::{CameraError, ShadingError};

use super::PrimitiveIndex;
use crate::mesh::PlaceError;

/// Why a scene could not be built.
///
/// Non-exhaustive: a later version may refuse a scene for a new reason, so a `match` outside
/// this crate needs a wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SceneError {
    /// The scene has no triangles.
    Empty,
    /// The scene has more triangles than a `u32` primitive index can name.
    TooManyTriangles,
    /// A vertex coordinate of the triangle at this primitive index is outside the trace domain
    /// (`abs(raw) >= 2^40`, 256 units).
    OutsideTraceDomain {
        /// The primitive index of the triangle.
        primitive: PrimitiveIndex,
    },
    /// `material_of` does not have one entry per triangle.
    MaterialCount,
    /// The triangle at this primitive index names a material that does not exist.
    UnknownMaterial {
        /// The primitive index of the triangle.
        primitive: PrimitiveIndex,
    },
    /// The bounding volume hierarchy could not be built or converted.
    BvhBuild,
    /// Bunny's tree builder put a primitive index in the tree that names no triangle. Bunny's
    /// builder fills the buffer with indices of the boxes it is given, so no input reaches this;
    /// `Scene::new` checks it once so that no traversal of the scene can meet such an index.
    TreeNamesUnknownPrimitive {
        /// The index found in the tree.
        index: u32,
        /// The scene's triangle count: every valid index is below it.
        triangles: usize,
    },
    /// Memory for the scene's buffers could not be reserved.
    OutOfMemory,
    /// A mesh of the scene could not be read or placed.
    Mesh(PlaceError),
    /// Bunny's `PinholeCamera::new` refused the camera placement: its position is outside the
    /// trace domain, its tangent is not in `(0, 256]`, its look direction is zero, or its up
    /// hint is zero or parallel to the look direction.
    Camera(CameraError),
    /// A lighting or material constant of a built-in scene is outside Bunny's accepted range.
    Constant(ShadingError),
}

impl fmt::Display for SceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("scene has no triangles"),
            Self::TooManyTriangles => f.write_str("scene has more than 2^32 - 1 triangles"),
            Self::OutsideTraceDomain { primitive } => {
                write!(f, "triangle {primitive} has a vertex outside the trace domain")
            }
            Self::MaterialCount => f.write_str("scene needs one material index per triangle"),
            Self::UnknownMaterial { primitive } => {
                write!(f, "triangle {primitive} names a material that does not exist")
            }
            Self::BvhBuild => f.write_str("the bounding volume hierarchy could not be built"),
            Self::TreeNamesUnknownPrimitive { index, triangles } => write!(
                f,
                "the bounding volume hierarchy names triangle {index}, but the scene has \
                 {triangles} triangles"
            ),
            Self::OutOfMemory => f.write_str("not enough memory for the scene"),
            Self::Mesh(error) => write!(f, "a mesh could not be placed: {error}"),
            Self::Camera(error) => write!(f, "invalid camera placement: {error}"),
            Self::Constant(error) => write!(f, "a lighting or material constant: {error}"),
        }
    }
}

impl std::error::Error for SceneError {}
