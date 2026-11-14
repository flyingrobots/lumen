# Changelog

All notable changes to lumen are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and lumen uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The Stanford bunny in the reference scene (flyingrobots/lumen#14): `lumen-reference-scene/3`
  appends its 3 851 triangles (none dropped; 31 511 triangles in all) after the triangles of
  version 2, with a green albedo, standing on the ground left of the sphere. The mesh,
  `assets/stanford-bunny/bun_zipper_res3.ply`, is Geordi's checked-in copy of Stanford's
  `bun_zipper_res3.ply` (Stanford 3D Scanning Repository, Stanford Computer Graphics
  Laboratory), byte for byte (SHA-256
  `975e7f9b160b4ea15b0e225e21b10828ebcf678df020d2f6a46aa408fdcf5cd6`), with its provenance,
  Stanford's reconstruction README and attribution; `.gitattributes` keeps its bytes intact.
- An interim, strict ASCII PLY reader for exactly that file's profile
  (`crates/lumen/src/mesh/ply.rs`, `mesh/decimal.rs`), with typed errors carrying line numbers
  and found values, and explicit limits (2^20 vertices and faces, 256-byte lines, ten-digit
  counts and indices). It converts decimals to Q32.32 with integers only by
  `lumen-ascii-ply-q32.32/1`, which follows Bunny's `bunny-decimal-q32-32/1` (the exact value
  times `2^32`, rounded to nearest, ties to even, sign applied after rounding). It is to be
  replaced by Bunny's ASCII PLY reader (flyingrobots/bunny#247).
- Mesh placement (`crates/lumen/src/mesh/placement.rs`): a half turn about `+Y`, a power-of-two scale, a
  floor to the `2^-16` grid and a whole-step translation; faces with zero area after placement
  are dropped and listed.
- Tests: the asset hash; hand-derived conversions, Bunny's tie vectors and range edges, and
  generated decimals; the manifest's bounds; the reader's refusals and limits; generated corrupt
  files that must be refused without a panic; the placed bounds; the dropped and kept counts; the
  winding and holes; brute force over a render that hits the bunny; the bunny's pixels and its
  shadow; a new golden hash.

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

- The reference scene's content (its shapes, the bunny, camera, materials and lighting) moves
  out of `scene.rs` into the `reference` module, and the vertex grid into `vertex_grid.rs`;
  `scene.rs` keeps the scene machinery. Library paths change from `lumen::scene::reference_*`
  and `REFERENCE_*` to `lumen::reference::...` (`lumen::reference_scene` and
  `lumen::REFERENCE_SCENE_ID` are unchanged). No pixel changes.
- The reference camera moves from `(0, 1, 5)` looking along `(0, -0.25, -1)` to `(-1, 2, 5)`
  looking along `(0.1875, -0.375, -1)`, so the bunny's shadow can be seen; every object of
  version 2 stays in the picture. The golden hash (96 by 54, 4 samples, seed 0) is updated for
  the new scene identity.
- Scenes are traced with a `build_bvh_sah` tree in Bunny's compact layout
  (`compact_closest_hit`, `compact_any_hit`), the fastest of the four combinations measured, and
  each triangle's Bunny `TriangleSurface` is built once with the scene.
- Bunny is pinned to `dcd6bd37613738de93f7084e7c8aa0283de964ea`, which adds the diffuse shading
  steps.
- Dependencies (Bunny) build with `opt-level = 2` in the dev and test profiles.

### Removed

- `lumen-reference-scene/2` is retired.
- `lumen-reference-scene/1` and the normal-colour debugging view `lumen-normal-shading/1` are
  retired; the view is not kept behind a flag.

### Fixed

- `lumen render` reads its arguments with `std::env::args_os()`, so an argument that is not
  valid Unicode no longer panics: a non-Unicode `--out` path is used as given, and a
  non-Unicode number or option is a usage error (flyingrobots/lumen#12).
