//! Size class: small (in process, no file, no network), except one large test that runs only in
//! the release profile (`rendering.rs`).
//!
//! Tests of the canyon scene through lumen's public API: the terrain's heightfield generator,
//! `lumen-canyon-heightfield/1`; the terrain mesh; the craft and the planet; the assembled
//! scene's integrity; and rendering (what the camera sees, every ray of a small image against
//! brute force, the golden hash).

#[path = "../golden/mod.rs"]
mod golden;
mod heightfield;
mod integrity;
mod parts;
mod rendering;
mod support;
mod terrain;
