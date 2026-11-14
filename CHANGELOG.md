# Changelog

All notable changes to lumen are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and lumen uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Rendering on all cores (flyingrobots/lumen#4): `lumen render --threads N` (`1..=256`, default
  what `std::thread::available_parallelism` reports) and `--tile N | WxH` (default 32 by 32).
  Tiles are handed out from an atomic counter to whichever worker asks next; each worker renders
  a tile into its own buffer and copies it into the image. Safe Rust and the standard library
  only. The image is the same bytes at every thread count and tile size, and one thread is the
  same code path with one worker. A failing pixel gives the typed error of the first failing
  pixel in raster order at every schedule, and stops the other workers.
- Library: the `schedule` module (`Schedule`, `TileSize`, `ScheduleError`, `MAX_THREADS`,
  `DEFAULT_TILE`); the `workers` module (`render_pixels`, the scheduler over any pixel function
  and any summable per-pixel value, and `WorkerError`, which names the failing pixel);
  `render_scheduled` and `render_traced` (any `Tracer + Sync`); `RenderError::Spawn` and
  `RenderError::TileGeometry`; `Add` for `RayCount`. The library takes an explicit thread count;
  only the binary asks the host.
- A timing report: `lumen render` prints the pixels, threads used and asked for, the tile, and
  wall-clock `scene-seconds`, `render-seconds`, `hash-seconds`, `png-seconds`, `total-seconds`,
  `rays-per-second` and `ns-per-ray`, all from a monotonic clock and never part of the image.
- Tests: hand-out orders of simulated workers, through the scheduler's own steps on one thread,
  chosen by 200 seeds and two scripted worst cases (one worker takes all; tiles finish in reverse
  order), checked against a raster-order loop for the image and for the first failure; tile
  geometry covering every pixel once. In `crates/lumen/tests/threads.rs`, at fixed thread counts:
  the golden hash at 1, 2, 3, 4, 7 and 18 threads and eight tile sizes; a 320 by 180 image equal
  on one thread, on many and in a raster-order loop without the scheduler; deterministic errors,
  bounded by a count of work taken, for injected pixel failures and failing traversals; worker
  stop after a failure.
- `docs/adr/0001-tile-scheduling.md`: the concurrency model, why the image cannot depend on it,
  how the first error is chosen, and what was rejected.
- A "Performance" section in the README: how to reproduce the thread-scaling, tile and 4K
  measurements through `devrun` with `--cpus` and `--exclusive`.

### Changed

- `lumen render --out FILE` is optional: without it the image is rendered and hashed but not
  written, and the report says `png none`. `UsageError::MissingOut` is gone; `UsageError` gains
  `Schedule` and `InvalidTile`, and `Command::Render` gains `threads` and `tile`.
- `render` (one thread) now runs through the tile scheduler with one worker; its bytes, ray count
  and errors are unchanged (the golden hash holds).
- The report's `render-seconds` no longer includes the scene build, and `ns-per-ray` is the
  render's wall time divided by the rays traced (on all threads).

- A lit render (flyingrobots/lumen#3): `lumen render` takes `--samples N` (a power of two from 1
  to 1024, default 4) and `--seed S` (`0..=4294967295`, default 0), and renders each sample as
  one diffuse path, `lumen-diffuse-path/1`: Bunny's `bunny-sobol-owen/1` sampler (pair 0 for the
  camera, pair 1 for the bounce), the camera ray's nearest hit, direct light from one directional
  light with a shadow ray (`any_hit`, `SHADOW_T_MAX`) from Bunny's start point, one
  cosine-weighted bounce with direct light at its hit, and a uniform sky, combined in the order
  of operations of `bunny-diffuse-shading/1`. Lumen adds no arithmetic of its own. Pixels are the
  mean of a `PixelAccumulator`. It prints the integrator, shading, sampler, tone and encoding
  identities, the settings, the rays traced and the image hash.
- The reference scene `lumen-reference-scene/2`: the triangles and camera of version 1 with a
  diffuse albedo per object, a sun low from the right front and a pale blue sky. Scenes are built
  from a `SceneSpec` (triangles, a material per triangle, albedos, an optional directional light
  and a sky) shaped after Geordi's ray-traced scene profile.
- `RenderSettings`, which refuses a sample count that is not a power of two or is above 1024.
- Tests: hand-derived radiance bits for constructed scenes (lit, lit at an angle, occluded, sky
  only, white under a sky, a ceiling `2^-12` above the ground); shadow pixels against an exact
  integer projection of an occluder; every traced ray against a test of every triangle; every
  sample against an independent composition of the documented order; order independence of
  samples and pixels; a convergence guard; a new golden hash (96 by 54, 4 samples, seed 0).
- `crates/lumen/examples/traversal.rs`, which times Bunny's two tree builders and two traversal
  layouts on a small render and checks they give the same bits.
- `lumen render --out FILE [--width W] [--height H]`: renders the built-in reference scene to a
  PNG file on one thread, with one camera ray per pixel, and prints the scene, shading, tone
  curve and Bunny identities and the image hash (`sha256:` over the raw RGBA8 bytes)
  (flyingrobots/lumen#2).
- The reference scene `lumen-reference-scene/1`, defined in code and made of triangles only: a
  bumpy sphere of 27 648 triangles, an octahedron, a back wall and a ground, with every vertex
  on a `2^-16` grid so that it is exactly representable as `f32`.
- The shading rule `lumen-normal-shading/1`: the geometric normal, turned to face the ray,
  mapped to colour as `(n + 1) / 2`; a constant sky for misses. Pixels go through Bunny's
  `PixelAccumulator`, `bunny-tone-srgb/1` and `write_rgba8`.
- A dependency-free PNG encoder with stored deflate blocks, and a dependency-free SHA-256.
- Tests: closest hits against a test of every triangle, hand-derived pixels, determinism within
  one process and across two processes, a golden image hash, SHA-256 and checksum test vectors,
  a strict PNG decoder, and command-line errors.
- Bunny 0.7.0 crates (`bunny-num`, `bunny-linalg`, `bunny-geom`, `bunny-query`,
  `bunny-broadphase`, `bunny-optics`) as git dependencies pinned to commit
  `7fdd2e4926041b0a226567e0a6e1ec51e522304f`.
- CI also runs the tests in the release profile, so the golden hash is checked with and without
  optimization.

### Changed

- Scenes are traced with a `build_bvh_sah` tree in Bunny's compact layout
  (`compact_closest_hit`, `compact_any_hit`), the fastest of the four combinations measured, and
  each triangle's Bunny `TriangleSurface` is built once with the scene.
- Bunny is pinned to `dcd6bd37613738de93f7084e7c8aa0283de964ea`, which adds the diffuse shading
  steps.
- Dependencies (Bunny) build with `opt-level = 2` in the dev and test profiles.

### Removed

- `lumen-reference-scene/1` and the normal-colour debugging view `lumen-normal-shading/1` are
  retired; the view is not kept behind a flag.

### Fixed

- `lumen render` reads its arguments with `std::env::args_os()`, so an argument that is not
  valid Unicode no longer panics: a non-Unicode `--out` path is used as given, and a
  non-Unicode number or option is a usage error (flyingrobots/lumen#12).
