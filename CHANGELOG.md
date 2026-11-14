# Changelog

All notable changes to lumen are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and lumen uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Lumen has not been released, so this section is the net change from nothing: what exists at the
head of `main`, each part described as it now is. Things that were added and then changed or
removed before any release (the tile scheduler and `--tile`, the normal-colour view
`lumen-normal-shading/1`, reference scene versions 1 and 2, Fixed Point version 1, the interim
PLY reader `lumen-ascii-ply-q32.32/1`, the `lumen::cli` module, `RenderError::Shading`) are not
listed; their history is in `git log` and the pull requests named. Each issue named is where the
part was built.

### Added

- **The `lumen` command** ([reference](docs/reference/cli.md)). `lumen render` builds a built-in
  scene (`--scene reference|fixed-point`), renders it at `--width` by `--height` (each
  `1..=32768`, at most `2^27` pixels; default 320 by 180) with `--samples` (a power of two from 1
  to 1024, default 4) and `--seed` (`0..=4294967295`, default 0) on `--threads` (`1..=256`,
  default what the system reports) taking bands of `--band-rows` rows (default 1), prints a
  report (the render's provenance, the threads and band height, the rays, the image hash, the
  file and the wall-clock times) and, with `--out`, writes the PNG; `--stats` adds the scene, ray
  and run statistics. `lumen stats` prints a scene's statistics without rendering; `--help` and
  `--version` (with the Bunny commit). A refused command line or value exits 2 with a message
  (and the usage text, except for a size); anything that fails after that exits 1. Arguments
  are read as OS strings; every option also takes `--option=value`; a separate value that reads
  as an option is refused; leading zeros are decimal. `--out` is checked before the render and
  published atomically: a temporary file beside the destination, synced and renamed into place
  (the directory synced on Unix), so it holds the complete new PNG or what was there before;
  the temporary name keeps at most 32 bytes of the destination's, so any name the file system
  accepts publishes. (flyingrobots/lumen#2, #3, #4, #12, #15, #19, #28, #29, #54, #57, #159.)
- **The renderer.** Each sample is one diffuse path composed of Bunny's contracts with no
  arithmetic of Lumen's own: `lumen-diffuse-path/1` (a camera ray from `bunny-sobol-owen/1`
  pair 0, the nearest triangle, direct light from one directional light with a shadow ray from
  Bunny's start point, one cosine-weighted bounce from pair 1, a uniform sky, in the order of
  `bunny-diffuse-shading/1`), or `lumen-smooth-diffuse-path/1` for a scene with vertex normals
  (Bunny's `bunny-smooth-shading/1` at smooth triangles; ADR 0007). Pixels are the mean of a
  `PixelAccumulator`, toned by `bunny-tone-srgb/1` and written by `write_rgba8`. Queries use a
  binned-SAH tree in Bunny's compact layout, chosen by measurement.
  (flyingrobots/lumen#2, #3, #39.)
- **Three built-in scenes** ([reference](docs/reference/scenes.md)), entirely in Q32.32
  integers: `lumen-reference-scene/3` (ground, wall, octahedron, a bumpy sphere and the Stanford
  bunny, 31 511 triangles), `lumen-fixed-point-scene/2` (the full-resolution Stanford bunny,
  smooth shaded, in sunglasses at a craps table with two dice and the ON puck, 71 575 triangles)
  and `lumen-canyon-scene/1` (566 005 triangles: a terrain of 480 256 from the integer generator
  `lumen-canyon-heightfield/1`, specified exactly in `scenes::canyon::heightfield`, three craft
  of Lumen's own design, the full-resolution bunny on a ledge and a banded planet).
  `BuiltinScene` is their registry. The canyon's public helpers refuse what they cannot compute
  rather than wrap or panic (`HeightfieldSpec::bounds` and `centre_256` with `HeightfieldError`,
  `Rational::new` and `Pose::place` with `PoseError`). Both Stanford meshes are in
  `assets/stanford-bunny/` with their provenance, read by Bunny's ASCII PLY reader and placed
  exactly (`mesh`). (flyingrobots/lumen#2, #14, #15, #27, #39, #40, #160.)
- **Rendering on all cores** (ADR 0003, which supersedes ADR 0001): row bands from `chunks_mut`
  handed out under one small lock, results returned through `join`, one shared atomic. The image
  and the ray count are the same at every thread count and band height; a failed render reports
  the raster-first failing pixel; a panic or a refused thread halts the other workers. Release
  builds use fat LTO and one codegen unit. (flyingrobots/lumen#4, #28, #56, #57.)
- **The library** ([reference](docs/reference/library.md)): the crate root is the API, with only
  `reference`, `scenes` and `mesh` as public modules (ADR 0002); the library reads no argument,
  file, clock or environment and starts threads only at the count it is given. Every refusal is
  a typed, `#[non_exhaustive]` error that carries what it saw and keeps its cause as `source()`.
  `PrimitiveIndex` names a triangle by its position, which breaks ties.
  (flyingrobots/lumen#19, #20, #21.)
- **Provenance and content identity**: `RenderProvenance`, thirteen fields from `scene` to
  `scene-content`, built from the scene; `Scene::content_id`, the SHA-256 of the canonical
  encoding `lumen-scene-content/2` of the scene's input (ADR 0005). Equal provenance means equal
  bytes for a scene rendered as its own tracer. (flyingrobots/lumen#19, #38, #39, #57.)
- **Statistics** (ADR 0008): `Scene::statistics` (triangles by material, bounds, the tree's
  shape, buffer sizes), ray statistics in every `Render` (`RayOutcomes`, `RayStatistics`), and
  run statistics on request (`render_traced_with_run`, `RunStatistics`), which a render that
  does not ask for pays nothing for. Traversal statistics wait for flyingrobots/bunny#335.
  (flyingrobots/lumen#29.)
- **The image hash and the PNG**: `sha256:` and the lowercase hex SHA-256 of the raw RGBA8 bytes
  (ADR 0009), with Lumen's own SHA-256; a PNG encoder with stored deflate blocks, whose bytes are
  a fixed function of the pixels (`write_png`, `png_len`). (flyingrobots/lumen#2.)
- **Tests**: hand-derived radiance, shadows against exact geometry, every ray and every sample
  against brute force and an independent composition, order independence, counts against
  oracles other than the counting code, golden image hashes and content digests (one value for
  every platform), simulated hand-out orders, generated tests from a seeded generator, and the
  documentation's own checks (the command reference against the program, the library reference
  against the crate root, the tutorial by running it). Each integration test file states its size
  class. (flyingrobots/lumen#3, #4, #14, #24, #25, #55, #57, #162.)
- **CI**: the determinism matrix (`Determinism / ...` on x86-64 and aarch64 Linux, aarch64
  macOS, x86-64 Windows and `wasm32-wasip1` under Node, each in both profiles; ADR 0004), `fmt,
  clippy`, and `Gates / cargo-deny` (no registry crate, Bunny's repository only, pinned by
  `rev`), `Gates / cargo-audit`, `Gates / file length`, `Gates / nondeterminism`,
  `Gates / doc links` and `Signal / coverage`. (flyingrobots/lumen#23, #25.)
- **Nondeterminism bans** (ADR 0006): `clippy.toml` bans the clock, sleeping, the core count,
  thread spawning, hash maps, every stable item of `std::env`, `fs`, `process` and `net`, floats
  (also as `c_double` and `c_float` of `core::ffi` and `std::os::raw`; the scan also names `f16`
  and `f128`), `Duration`'s float methods and Bunny's float bridge, each under an Echo rule ID; `xtask
  clippy-bans` checks on a seeded fixture that every ban resolves and fires and that the std item
  list (`xtask/bans/std-items.txt`, from the toolchain's documentation) is complete; the token
  scan (`xtask nondeterminism`) covers what Clippy cannot see, over the deterministic core found
  by following the module tree, and fails every lint attribute that would silence a ban. The
  scan also checks the workspace boundary from `cargo metadata`: every deterministic crate's
  library is core and must deny float arithmetic; a member neither reached by `lumen` nor
  classified as tooling, a path dependency outside the workspace, a dependency from anywhere but
  Bunny's repository at a full commit, a build script or procedural macro in a deterministic
  crate, a member's own lint levels, a `[patch]` or `[replace]`, and any `.cargo/config` fail.
  (flyingrobots/lumen#17, #115, #119, #120, #156, #158.)
- **Repository tooling**: `xtask` (`file-length`, `nondeterminism`, `clippy-bans`, `std-items`,
  `doc-links`) and a `Makefile` (`make demo`, `reference`, `fixed-point`, `4k`,
  `4k-fixed-point`, `canyon`, `4k-canyon`, `test`, `nondeterminism`, `clippy-bans`,
  `doc-links`, `help`) over the container runner. (flyingrobots/lumen#17, #23, #25, #40, #104,
  #115.)
- **Documentation**: the README as a short router, a [tutorial](docs/tutorial.md), the command,
  library and scene references, an [explanation](docs/explanation.md) ending in a
  requirement-to-evidence map, a how-to for measuring render time, a
  [contributor guide](CONTRIBUTING.md), an [index](docs/README.md), ADRs 0001 to 0009 and dated
  measurement records in `docs/measurements/`. (flyingrobots/lumen#5, #25, #39, #40, #57.)
- **Dependencies**: Bunny's `bunny-num`, `bunny-linalg`, `bunny-geom`, `bunny-query`,
  `bunny-broadphase`, `bunny-optics` and `bunny-codec` (default features off), version 0.7.1,
  as git dependencies pinned to `d4fe51d38fa10f8d6496bd110955314edac30bbf` (`BUNNY_REV`, checked
  against `Cargo.lock` by a test); no crates.io crate. Dependencies build with `opt-level = 2`
  in the dev and test profiles.
