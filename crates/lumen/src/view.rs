//! This module owns the debug views: which pictures of a scene `--view` can render besides the
//! lit image ([`View`]), their stable identities, and how each one's pixels are computed
//! ([`render_view`]). See `docs/adr/0009-debug-views.md`.
//!
//! A view other than [`View::Lit`] is a deterministic function of the scene and the image size,
//! computed from Bunny results only: lumen's own arithmetic is the mapping of an integer to a
//! colour. It goes through the same scheduler as the lit image, so it is the same bytes at every
//! thread count and band height.

use core::fmt;
use core::str::FromStr;

mod colour;
mod first_hit;

pub use first_hit::render_view;

/// What a render draws: the lit image, or a debug view of the scene.
///
/// A debug view is a different picture of the same scene, so it is a different integrator
/// identity in the provenance ([`View::id`]): `lumen-view-<name>/1`, versioned like an
/// integrator. A change to what a view draws is a new version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum View {
    /// The lit image of the scene's integrator: `lumen-diffuse-path/1` or
    /// `lumen-smooth-diffuse-path/1`, unchanged.
    Lit,
    /// The geometric normal of the camera ray's first hit, as a colour ([`NORMALS_VIEW_ID`]).
    Normals,
    /// The shading normal of the first hit: the interpolated vertex normal where the triangle
    /// has three, the geometric normal otherwise ([`SHADING_NORMALS_VIEW_ID`]).
    ShadingNormals,
}

/// The identity of [`View::Normals`].
pub const NORMALS_VIEW_ID: &str = "lumen-view-normals/1";

/// The identity of [`View::ShadingNormals`].
pub const SHADING_NORMALS_VIEW_ID: &str = "lumen-view-shading-normals/1";

impl View {
    /// Every view, in the order the command lists them.
    pub const ALL: [Self; 3] = [Self::Lit, Self::Normals, Self::ShadingNormals];

    /// The name `--view` accepts.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Lit => "lit",
            Self::Normals => "normals",
            Self::ShadingNormals => "shading-normals",
        }
    }

    /// The view's identity, which the provenance records as its `integrator`; `None` for
    /// [`View::Lit`], whose identity is the scene's integrator's.
    #[must_use]
    pub const fn id(self) -> Option<&'static str> {
        match self {
            Self::Lit => None,
            Self::Normals => Some(NORMALS_VIEW_ID),
            Self::ShadingNormals => Some(SHADING_NORMALS_VIEW_ID),
        }
    }

    /// The Bunny shading contracts the view's pixels use, the provenance's `shading` field;
    /// `None` for [`View::Lit`] (the integrator's). The normals view uses none: the geometric
    /// normal is the triangle's surface normal, part of the scene.
    #[must_use]
    pub const fn shading_id(self) -> Option<&'static str> {
        match self {
            Self::Lit => None,
            Self::Normals => Some("none"),
            Self::ShadingNormals => Some(bunny_optics::SMOOTH_NORMAL_ID),
        }
    }

    /// Whether a pixel of the view is one camera ray through the pixel's centre, whatever the
    /// samples per pixel: every view but [`View::Lit`] so far. Its provenance records one sample
    /// and no bounce.
    #[must_use]
    pub const fn is_single_sample(self) -> bool {
        !matches!(self, Self::Lit)
    }
}

impl fmt::Display for View {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for View {
    type Err = ParseViewError;

    /// Reads a name exactly as [`View::name`] gives it.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|view| view.name() == text)
            .ok_or_else(|| ParseViewError { given: text.to_owned() })
    }
}

/// A view name that is not one of [`View::ALL`]. Its message lists the valid names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseViewError {
    /// The text given.
    pub given: String,
}

impl fmt::Display for ParseViewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown view {:?}; the views are", self.given)?;
        for (i, view) in View::ALL.iter().enumerate() {
            write!(f, "{} {}", if i == 0 { "" } else { "," }, view.name())?;
        }
        Ok(())
    }
}

impl std::error::Error for ParseViewError {}
