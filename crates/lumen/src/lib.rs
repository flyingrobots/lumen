//! Lumen: a deterministic ray tracer built on Bunny's integer math.
//!
//! Lumen renders a built-in reference scene ([`reference::reference_scene`]) in tiles on all
//! cores ([`render::render_scheduled`], [`schedule`], [`workers`]) with a power-of-two number of
//! samples per pixel, each one diffuse path with a shadow-casting directional light, a uniform
//! sky and one bounce ([`render::trace_path`], `lumen-diffuse-path/1`), and writes it as a PNG
//! file ([`png::write_png`]). Every number
//! that decides a pixel comes from Bunny: the sampler, camera rays, the BVH and its traversals,
//! the triangle test, the shading steps and their order of operations, and the pixel path. So
//! the same scene, size, sample count and seed give the same RGBA bytes, and the same image hash
//! ([`image::Rgba8Image::hash`]), on every run and every machine, at every thread count and tile
//! size.

pub mod image;
pub mod mesh;
pub mod png;
mod provenance;
pub mod reference;
pub mod render;
pub mod scene;
pub mod schedule;
pub mod sha256;
mod vertex_grid;
pub mod workers;

/// The Bunny commit lumen is built against: the `rev` of every Bunny dependency in
/// `Cargo.toml`. A test checks that `Cargo.lock` resolves to it, so moving the pin means
/// editing this constant, next to the golden image hash it may change.
pub const BUNNY_REV: &str = "dcd6bd37613738de93f7084e7c8aa0283de964ea";

pub use image::{ImageSize, Rgba8Image, SizeError};
pub use png::{png_len, write_png};
pub use provenance::{ProvenanceValue, RenderProvenance, PROVENANCE_FIELDS};
pub use reference::{reference_scene, REFERENCE_SCENE_ID};
pub use render::{
    render, render_reference, render_scheduled, render_traced, RayCount, Render, RenderError,
    RenderSettings, SettingsError, INTEGRATOR_ID, MAX_BOUNCES, TONE_CURVE,
};
pub use scene::{Scene, SceneError, SceneSpec, Tracer};
pub use schedule::{Schedule, ScheduleError, TileSize, DEFAULT_TILE, MAX_THREADS};
pub use workers::{render_pixels, WorkerError};
