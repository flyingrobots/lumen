//! Size class: small (in process, no file, no network).
//!
//! Tests of the canyon scene through lumen's public API: the terrain's heightfield generator,
//! `lumen-canyon-heightfield/1`; the terrain mesh; the craft and the planet; and the assembled
//! scene's integrity.

mod heightfield;
mod integrity;
mod parts;
mod support;
mod terrain;
