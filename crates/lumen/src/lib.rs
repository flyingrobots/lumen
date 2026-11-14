//! Lumen: a deterministic ray tracer built on Bunny's integer math.
//!
//! Lumen renders a built-in reference scene ([`scene::reference_scene`]) in tiles on all cores
//! ([`render::render_scheduled`]) with a power-of-two number of samples per pixel, each one
//! diffuse path with a shadow-casting directional light, a uniform sky and one bounce ([`render::trace_path`],
//! `lumen-diffuse-path/1`), and writes it as a PNG file ([`png::write_png`]). Every number
//! that decides a pixel comes from Bunny: the sampler, camera rays, the BVH and its traversals,
//! the triangle test, the shading steps and their order of operations, and the pixel path. So
//! the same scene, size, sample count and seed give the same RGBA bytes, and the same image hash
//! ([`image::Rgba8Image::hash`]), on every run and every machine, at every thread count and tile
//! size.

pub mod cli;
pub mod image;
pub mod png;
pub mod render;
pub mod scene;
pub mod sha256;

/// The Bunny commit lumen is built against: the `rev` of every Bunny dependency in
/// `Cargo.toml`. A test checks that `Cargo.lock` resolves to it, so moving the pin means
/// editing this constant, next to the golden image hash it may change.
pub const BUNNY_REV: &str = "dcd6bd37613738de93f7084e7c8aa0283de964ea";

pub use image::{ImageSize, Rgba8Image, SizeError};
pub use render::{
    render, render_pixels, render_reference, render_scheduled, render_traced, Render, RenderError,
    RenderSettings, Schedule, ScheduleError, SettingsError, TileSize, INTEGRATOR_ID, MAX_THREADS,
};
pub use scene::{reference_scene, Scene, SceneError, SceneSpec, Tracer, REFERENCE_SCENE_ID};
