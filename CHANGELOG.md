# Changelog

All notable changes to lumen are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and lumen uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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

- Dependencies (Bunny) build with `opt-level = 2` in the dev and test profiles.
