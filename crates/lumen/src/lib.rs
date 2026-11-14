//! Lumen: a deterministic ray tracer built on Bunny's integer math.
//!
//! This first slice renders a built-in reference scene ([`scene::reference_scene`]) on one
//! thread with one camera ray per pixel and a normal-to-colour shading rule
//! ([`render::shade`]), and writes it as a PNG file ([`png::write_png`]). Every number that
//! decides a pixel comes from Bunny (camera rays, the BVH and its closest-hit traversal, the
//! triangle test, normals, the pixel path) or from exact integer arithmetic, so the same scene
//! and size give the same RGBA bytes, and the same image hash ([`image::Rgba8Image::hash`]),
//! on every run and every machine.

pub mod cli;
pub mod image;
pub mod png;
pub mod render;
pub mod scene;
pub mod sha256;

/// The Bunny commit lumen is built against: the `rev` of every Bunny dependency in
/// `Cargo.toml`. A test checks that `Cargo.lock` resolves to it, so moving the pin means
/// editing this constant, next to the golden image hash it may change.
pub const BUNNY_REV: &str = "7fdd2e4926041b0a226567e0a6e1ec51e522304f";

pub use image::{ImageSize, Rgba8Image, SizeError};
pub use render::{render, render_reference, RenderError, SHADING_ID};
pub use scene::{reference_scene, Scene, SceneError, REFERENCE_SCENE_ID};
