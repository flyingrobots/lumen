//! Why the canyon scene could not be built.

use core::fmt;

use bunny_optics::{ShadingError, VertexNormalError};

use super::heightfield::HeightfieldError;
use crate::mesh::PlaceError;
use crate::scene::SceneError;

/// Why the canyon scene could not be built. None of these happens for the scene as defined;
/// tests check that it builds.
///
/// Non-exhaustive: a later version may refuse the scene for a new reason, so a `match` outside
/// this crate needs a wildcard arm. Not `Copy`, since [`SceneError`] is not.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CanyonError {
    /// The heightfield generator refused the terrain's spec.
    Heightfield(HeightfieldError),
    /// The embedded Stanford bunny could not be read.
    Bunny(PlaceError),
    /// The bunny has no faces.
    EmptyBunny,
    /// A placed coordinate of the bunny overflows.
    Overflow,
    /// Bunny's `VertexNormals::new` refused the placed bunny.
    Normals(VertexNormalError),
    /// A constant of the scene's lighting or materials is outside Bunny's accepted range.
    Constant(ShadingError),
    /// The scene's triangles were refused by [`crate::scene::Scene::new`].
    Scene(SceneError),
}

impl fmt::Display for CanyonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Heightfield(error) => write!(f, "the terrain's heightfield: {error}"),
            Self::Bunny(error) => write!(f, "the bunny could not be placed: {error}"),
            Self::EmptyBunny => f.write_str("the bunny has no faces"),
            Self::Overflow => f.write_str("a placed coordinate of the bunny overflows"),
            Self::Normals(error) => write!(f, "the bunny's vertex normals were refused: {error}"),
            Self::Constant(error) => {
                write!(f, "a lighting or material constant was refused: {error}")
            }
            Self::Scene(error) => write!(f, "the scene could not be built: {error}"),
        }
    }
}

impl std::error::Error for CanyonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Heightfield(error) => Some(error),
            Self::Bunny(error) => Some(error),
            Self::Normals(error) => Some(error),
            Self::Constant(error) => Some(error),
            Self::Scene(error) => Some(error),
            Self::EmptyBunny | Self::Overflow => None,
        }
    }
}
