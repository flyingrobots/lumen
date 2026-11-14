//! Why the Fixed Point scene could not be built.

use core::fmt;

use bunny_optics::ShadingError;

use crate::mesh::PlaceError;
use crate::scene::SceneError;

/// Why the Fixed Point scene could not be built. None of these happens for the scene as
/// defined; tests check that it builds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixedPointError {
    /// The embedded Stanford bunny could not be read or placed.
    Bunny(PlaceError),
    /// The placed bunny has no triangles to measure.
    EmptyBunny,
    /// A posed coordinate of the bunny or its sunglasses overflows.
    Overflow,
    /// A die's rational turn does not map one of its vertices to whole grid steps.
    InexactTurn,
    /// A constant of the scene's lighting or materials is outside Bunny's accepted range.
    Constant(ShadingError),
    /// The scene's triangles were refused by [`crate::scene::Scene::new`].
    Scene(SceneError),
}

impl fmt::Display for FixedPointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bunny(error) => write!(f, "the bunny could not be placed: {error}"),
            Self::EmptyBunny => f.write_str("the placed bunny has no triangles"),
            Self::Overflow => f.write_str("a posed coordinate overflows"),
            Self::InexactTurn => f.write_str("a die's turn is not exact on the grid"),
            Self::Constant(error) => {
                write!(f, "a lighting or material constant was refused: {error:?}")
            }
            Self::Scene(error) => write!(f, "the scene could not be built: {error}"),
        }
    }
}

impl std::error::Error for FixedPointError {}
