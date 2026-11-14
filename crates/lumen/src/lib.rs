//! Lumen: a deterministic ray tracer built on Bunny's integer math.
//!
//! Lumen renders a built-in scene ([`BuiltinScene`]: the reference scene, [`reference_scene`], the
//! Fixed Point showpiece, [`fixed_point_scene`], or the canyon, [`canyon_scene`]), in bands of rows on all cores
//! ([`render_scheduled`], [`Schedule`], [`render_pixels`]) with a power-of-two number of samples
//! per pixel, each one diffuse path with a shadow-casting directional light, a uniform sky and one
//! bounce ([`trace_path`], `lumen-diffuse-path/1`, or `lumen-smooth-diffuse-path/1` with Bunny's
//! smooth shading where a scene gives vertex normals), and encodes it as a PNG ([`write_png`]). Every
//! number that decides a pixel comes from Bunny: the sampler, camera rays, the BVH and its
//! traversals, the triangle test, the shading steps and their order of operations, and the pixel
//! path. So the same scene, size, sample count and seed give the same RGBA bytes, and the same
//! image hash ([`Rgba8Image::hash`]), on every run, at every thread count and band height, on
//! every platform CI checks (x86-64 and aarch64 Linux, aarch64 macOS, x86-64 Windows and
//! `wasm32-wasip1`). [`RenderProvenance`] records those inputs.
//!
//! The library is the crate root: every type and function a front end needs is re-exported
//! here, and only [`mod@reference`] and [`scenes`] (the scenes' content) and [`mesh`] (the mesh
//! types they use) are public modules. It reads no arguments, files, clock or environment, prints nothing
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
//! let provenance = RenderProvenance::named(REFERENCE_SCENE_ID, &scene, size, settings);
//! assert_eq!(provenance.size(), render.image.size());
//! let hash = render.image.hash();
//! assert!(hash.starts_with("sha256:") && hash.len() == 71);
//! let mut png = Vec::new();
//! write_png(&mut png, &render.image)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

// No float in the library: `clippy.toml` bans the float types and Bunny's float conversion;
// this catches arithmetic on a float whose type is only inferred (docs/adr/0006-nondeterminism-gate.md).
#![deny(clippy::float_arithmetic)]

mod builtin_scene;
#[cfg(test)]
#[path = "../tests/generator/mod.rs"]
mod generator;
mod image;
pub mod mesh;
mod png;
#[cfg(test)]
#[path = "png/generated.rs"]
mod png_generated;
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
pub const BUNNY_REV: &str = "2cd551e248d55407a8f27e84dcb26d1b1d8768bd";

pub use builtin_scene::{BuiltinScene, BuiltinSceneError, ParseSceneError};
pub use image::{ImageSize, LengthError, Rgba8Image, SizeError, MAX_DIMENSION, MAX_PIXELS};
pub use png::{png_len, write_png, PngError};
pub use provenance::{ProvenanceValue, RenderProvenance, PROVENANCE_FIELDS};
pub use reference::{reference_scene, REFERENCE_SCENE_ID};
pub use render::{
    pixel_radiance, render, render_reference, render_scheduled, render_traced,
    render_traced_with_run, render_traced_with_traversal, trace_path, Integrator, PathError,
    PathSample, RayCheckError, RayCount, RayOutcomes, RayStatistics, Render, RenderError,
    RenderSettings, RunStatistics, SettingsError, TraversalStatistics, TraversalWork,
    WorkerStatistics, BOUNCE_PAIR, CAMERA_PAIR, INTEGRATOR_ID, MAX_BOUNCES, SMOOTH_INTEGRATOR_ID,
    SMOOTH_SHADING_IDS, TONE_CURVE,
};
pub use scene::{
    CameraPlacement, CountingTracer, Hit, Lighting, PrimitiveIndex, Scene, SceneBuffer,
    SceneContentId, SceneError, SceneMemory, SceneSpec, SceneStatistics, StatisticsError,
    TraceError, Tracer, TreeStatistics, FAR, LEAF_SIZE_BUCKETS, SCENE_CONTENT_ENCODING,
    TREE_BUILDER, TREE_LAYOUT,
};
pub use scenes::canyon::{canyon_scene, CANYON_SCENE_ID};
pub use scenes::fixed_point::{fixed_point_scene, FIXED_POINT_SCENE_ID};
pub use schedule::{BandRows, Schedule, ScheduleError, DEFAULT_BAND_ROWS, MAX_THREADS};
pub use workers::{
    render_pixels, render_pixels_by_worker, Rendered, Reservation, SpawnError, WorkerError,
    WorkerRun,
};
