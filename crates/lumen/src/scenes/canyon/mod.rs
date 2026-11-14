//! The canyon scene's content, built from triangles only. Its terrain comes from an integer
//! heightfield generator, [`heightfield`] (`lumen-canyon-heightfield/1`), placed on the scene's
//! grid and meshed by [`terrain`].
//!
//! This module owns the canyon's content; so far, its terrain.

pub mod heightfield;
pub mod terrain;
