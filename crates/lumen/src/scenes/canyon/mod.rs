//! The canyon scene's content, built from triangles only: the terrain ([`terrain`], heights
//! from [`heightfield`], `lumen-canyon-heightfield/1`), three craft ([`craft`]) and a banded
//! planet ([`planet`]), gathered in a [`CanyonContent`] ([`content`]) that records each
//! triangle's part, material and vertex normals.
//!
//! This module owns the canyon's content; the bunny and the assembled scene are not here yet.

pub mod content;
pub mod craft;
pub mod heightfield;
pub mod planet;
pub mod terrain;

mod convex;

pub use content::{CanyonContent, Craft, Part, PartKind, PLANET_BANDS};
pub use convex::{Pose, Rational};
