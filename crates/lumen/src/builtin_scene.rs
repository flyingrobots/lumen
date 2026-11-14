//! The scenes lumen ships, by name: the registry every front end shares.
//!
//! This module owns the list of [`BuiltinScene`]s, their command-line names and their
//! identities, so that a second front end gets the same list as the `lumen` command. A scene's
//! content stays in [`crate::reference`] and [`crate::scenes`].

use core::fmt;
use core::str::FromStr;

use crate::reference::{reference_scene, REFERENCE_SCENE_ID};
use crate::scene::{Scene, SceneError};
use crate::scenes::fixed_point::{fixed_point_scene, FixedPointError, FIXED_POINT_SCENE_ID};

/// A scene that ships with lumen.
///
/// Each one has a name (`reference`, `fixed-point`) that [`FromStr`] reads and [`name`] gives
/// back, a stable identity (the string [`RenderProvenance`] records) and its own camera and
/// lighting, which [`build`] returns inside the [`Scene`].
///
/// [`name`]: BuiltinScene::name
/// [`build`]: BuiltinScene::build
/// [`RenderProvenance`]: crate::RenderProvenance
///
/// # Examples
///
/// ```
/// use lumen::BuiltinScene;
///
/// let scene: BuiltinScene = "fixed-point".parse()?;
/// assert_eq!(scene, BuiltinScene::FixedPoint);
/// assert_eq!(scene.id(), "lumen-fixed-point-scene/1");
/// assert!("fixed point".parse::<BuiltinScene>().is_err());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuiltinScene {
    /// The reference scene, [`REFERENCE_SCENE_ID`].
    Reference,
    /// The Fixed Point scene, [`FIXED_POINT_SCENE_ID`].
    FixedPoint,
}

impl BuiltinScene {
    /// Every built-in scene, in the order the command lists them.
    pub const ALL: [Self; 2] = [Self::Reference, Self::FixedPoint];

    /// The name a front end accepts for this scene.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::FixedPoint => "fixed-point",
        }
    }

    /// The scene's identity: the stable name and version that [`RenderProvenance`] records.
    ///
    /// [`RenderProvenance`]: crate::RenderProvenance
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Reference => REFERENCE_SCENE_ID,
            Self::FixedPoint => FIXED_POINT_SCENE_ID,
        }
    }

    /// Builds the scene, with its own camera and lighting.
    ///
    /// # Errors
    ///
    /// A [`BuiltinSceneError`] if the scene cannot be built. Tests show every built-in scene
    /// builds.
    pub fn build(self) -> Result<Scene, BuiltinSceneError> {
        match self {
            Self::Reference => reference_scene().map_err(BuiltinSceneError::Reference),
            Self::FixedPoint => fixed_point_scene().map_err(BuiltinSceneError::FixedPoint),
        }
    }
}

impl FromStr for BuiltinScene {
    type Err = ParseSceneError;

    /// Reads a name exactly as [`BuiltinScene::name`] gives it: no case folding, no spaces.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|scene| scene.name() == text)
            .ok_or_else(|| ParseSceneError { given: text.to_owned() })
    }
}

/// A scene name that is not one of [`BuiltinScene::ALL`]. Its message lists the valid names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseSceneError {
    given: String,
}

impl ParseSceneError {
    /// The text that was not a scene name.
    #[must_use]
    pub fn given(&self) -> &str {
        &self.given
    }
}

impl fmt::Display for ParseSceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown scene {:?}; the scenes are ", self.given)?;
        for (i, scene) in BuiltinScene::ALL.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            f.write_str(scene.name())?;
        }
        Ok(())
    }
}

impl std::error::Error for ParseSceneError {}

/// Why a [`BuiltinScene`] could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinSceneError {
    /// The reference scene was refused.
    Reference(SceneError),
    /// The Fixed Point scene could not be built.
    FixedPoint(FixedPointError),
}

impl fmt::Display for BuiltinSceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reference(error) => write!(f, "the reference scene could not be built: {error}"),
            Self::FixedPoint(error) => write!(f, "the Fixed Point scene: {error}"),
        }
    }
}

impl std::error::Error for BuiltinSceneError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Reference(error) => Some(error),
            Self::FixedPoint(error) => Some(error),
        }
    }
}
