//! Lumen: a deterministic ray tracer built on Bunny's integer math.
//!
//! Lumen renders a built-in scene ([`BuiltinScene`]: the reference scene, [`reference_scene`], or
//! the Fixed Point showpiece, [`fixed_point_scene`]), in bands of rows on all cores
//! ([`render_scheduled`], [`Schedule`], [`render_pixels`]) with a power-of-two number of samples
//! per pixel, each one diffuse path with a shadow-casting directional light, a uniform sky and one
//! bounce ([`trace_path`], `lumen-diffuse-path/1`), and encodes it as a PNG ([`write_png`]). Every
//! number that decides a pixel comes from Bunny: the sampler, camera rays, the BVH and its
//! traversals, the triangle test, the shading steps and their order of operations, and the pixel
//! path. So the same scene, size, sample count and seed give the same RGBA bytes, and the same
//! image hash ([`Rgba8Image::hash`]), on every run, at every thread count and band height, on
//! every platform CI checks (x86-64 and aarch64 Linux, aarch64 macOS, x86-64 Windows and
//! `wasm32-wasip1`). [`RenderProvenance`] records those inputs.
//!
//! The library is the crate root: every type and function a front end needs is re-exported
//! here, and only [`mod@reference`] and [`scenes`] (the scenes' content) and [`mesh`] (the mesh
//! reader they use) are public modules. It reads no arguments, files, clock or environment, prints nothing
//! and asks the host nothing: the caller passes the thread count and any writer. The `lumen`
//! command (`src/bin/lumen/`) is one such front end.
//!
//! # Example
//!
//! Render the reference scene, record what was rendered, hash it and encode it as a PNG in
//! memory:
//!
//! ```
//! use lumen::{reference_scene, render_scheduled, write_png, ImageSize, RenderProvenance};
//! use lumen::{RenderSettings, Schedule, DEFAULT_BAND_ROWS, REFERENCE_SCENE_ID};
//!
//! let scene = reference_scene()?;
//! let size = ImageSize::new(32, 18)?;
//! let settings = RenderSettings::new(2, 7)?;
//! let render = render_scheduled(&scene, size, settings, Schedule::new(2, DEFAULT_BAND_ROWS)?)?;
//! let provenance = RenderProvenance::new(REFERENCE_SCENE_ID, size, settings);
//! assert_eq!(provenance.size(), render.image.size());
//! let hash = render.image.hash();
//! assert!(hash.starts_with("sha256:") && hash.len() == 71);
//! let mut png = Vec::new();
//! write_png(&mut png, &render.image)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod builtin_scene;
mod image;
pub mod mesh;
mod png;
mod provenance;
pub mod reference;
mod render;
mod scene;
pub mod scenes;
mod schedule;
mod sha256;
mod vertex_grid;
mod workers;

/// The Bunny commit lumen is built against: the `rev` of every Bunny dependency in
/// `Cargo.toml`. A test checks that `Cargo.lock` resolves to it, so moving the pin means
/// editing this constant, next to the golden image hash it may change.
pub const BUNNY_REV: &str = "3c0dab3e77c0d45de8904906a134e846dfe5ec6c";

pub use builtin_scene::{BuiltinScene, BuiltinSceneError, ParseSceneError};
pub use image::{ImageSize, Rgba8Image, SizeError, MAX_DIMENSION, MAX_PIXELS};
pub use png::{png_len, write_png};
pub use provenance::{ProvenanceValue, RenderProvenance, PROVENANCE_FIELDS};
pub use reference::{reference_scene, REFERENCE_SCENE_ID};
pub use render::{
    pixel_radiance, render, render_reference, render_scheduled, render_traced, trace_path,
    PathSample, RayCount, Render, RenderError, RenderSettings, SettingsError, BOUNCE_PAIR,
    CAMERA_PAIR, INTEGRATOR_ID, MAX_BOUNCES, TONE_CURVE,
};
pub use scene::{
    CameraPlacement, Hit, Lighting, Scene, SceneError, SceneSpec, TraceError, Tracer, FAR,
};
pub use scenes::fixed_point::{fixed_point_scene, FIXED_POINT_SCENE_ID};
pub use schedule::{BandRows, Schedule, ScheduleError, DEFAULT_BAND_ROWS, MAX_THREADS};
pub use workers::{render_pixels, WorkerError};
