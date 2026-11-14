# Changelog

All notable changes to lumen are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and lumen uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- The scan follows aliases across files (flyingrobots/lumen#157): an `extern crate std as host`
  at the crate root, or a `pub use std as x` reached as `crate::m::x` from another file, is
  resolved in every core file of every deterministic crate. A glob import of a std module and a
  std path built from a macro metavariable (`std::$m::var_os`, `$root::fs`) are refused as the
  new rule `hidden-path`. Before, these passed both layers under a `cfg` the Linux Clippy run
  does not build.
- A float type under another name is refused (flyingrobots/lumen#156): `clippy.toml` bans
  `c_double` and `c_float` of `core::ffi` (which `std::ffi` re-exports) and of `std::os::raw`, and the
  scan names `c_double`, `c_float`, `f16` and `f128` (and names with the word `f16` or `f128`).
  Before, `<core::ffi::c_double as From<u32>>::from(x).sin()` passed both layers in the core.
- The nondeterminism gate checks the workspace boundary (flyingrobots/lumen#158, ADR 0006's
  amendment of 2026-11-14): `xtask nondeterminism` reads `cargo metadata` and fails on a workspace
  member that is neither reached by `lumen`'s normal and build dependencies nor classified as
  tooling, on a path dependency outside the workspace, on a dependency from anywhere but Bunny's
  repository at a full commit, and on a build script or procedural macro in a deterministic
  crate. Before, a second workspace crate with floats, or a `build.rs` setting a `cfg`, decided
  library behaviour with no finding.
- The scan reads every deterministic crate's library as core, not only `crates/lumen/src`, and
  fails a deterministic crate whose root does not deny `clippy::float_arithmetic`, one outside
  `crates/` and `xtask/`, a member whose manifest sets its own lint levels (a `[lints]` table
  other than `workspace = true`), a `[patch]` or `[replace]` in the root manifest, and any
  `.cargo/config.toml` or `.cargo/config` in the repository (flyingrobots/lumen#158).

### Fixed

- The canyon heightfield's public helpers no longer wrap or panic on a spec the generator
  would refuse (flyingrobots/lumen#160, the heightfield half). `HeightfieldSpec::bounds` and
  `HeightfieldSpec::centre_256` now return `Result`, refusing with `generate`'s own
  `HeightfieldError`: before, `bounds` of a layer of amplitude `i64::MAX` wrapped to a wrong
  range in release and panicked in debug, and `centre_256` read a meander spacing of 0 as 1 and
  overflowed past an amplitude of about `2^48`. `terrain::stratum_material` sums the corners in
  `i128`, so extreme heights no longer overflow. Tests reach every refusal and accept each limit
  (`tests/canyon/limits.rs`), and kill three generator mutations that survived (the plateau's
  weight dropped from the bounds, the spread truncated instead of floored, the grid's column
  check deleted). The scene's bytes are unchanged: the golden image hash, the 4K hash, the
  content identity and the generator's heights for the scene's spec are the same before and
  after.

- The canyon's craft poses no longer panic or wrap on a public input (flyingrobots/lumen#160,
  the poses half). `Rational`'s fields are private: `Rational::new` refuses a denominator that
  is not positive and numerators off `cos^2 + sin^2 = hyp^2` with `PoseError::NotARotation`, and
  `Rational::cos`, `sin` and `hyp` read it. `Pose::place` returns `Result`: before,
  `Rational { cos: 1, sin: 0, hyp: 0 }` made it panic dividing by zero, and its products and
  sums were unchecked; now a point whose posed coordinates leave `i64` is refused with
  `PoseError::Overflow`. `craft::craft_part` and `craft::push_craft` return the error, the
  latter as the new `CanyonError::Craft`, and the convex solids' means are checked. Tests in
  `tests/canyon/pose.rs`. The scene's bytes are unchanged: the golden image hash, the 4K hash
  and the content identity are the same before and after.

- Statistics and scan tests that could not fail now compare against something computed
  independently (flyingrobots/lumen#162). The memory test asserts all seven buffers, vertex
  normals included, and the total as their sum on a scene with no normals (reference), one with
  normals on every triangle (Fixed Point) and a three-triangle scene worked by hand; before, it
  summed six terms on the reference scene alone, so dropping `vertex_normals` from
  `SceneMemory::total` passed. The ray ceiling is tested at exactly four rays per sample (passes)
  and one past (refused), which a `>=` fails. The scan's seeded forms include `FxHashMap` and
  `FxHashSet`. The ray-count assertion `rays == statistics.count()`, which compared a value with
  itself, compares both with the queries the observing tracer saw. The statistics test that
  compared two calls checks every value of each call (before and after a render, and on a second
  build) against values worked by hand and a tree walked by the test. The tree oracle starts from
  a record of the test's own rather than from the value under test. A triangle with one or two
  vertex normals is tested to render the flat bytes (ADR 0007 clarification): shading smooth only
  when all three are present is equivalent.

- `render --out` no longer refuses a valid long file name (flyingrobots/lumen#159). The temporary
  file was named `.NAME.lumen-PID-N.tmp` after the whole destination name, so a destination of
  244 bytes or more made a temporary name past the 255-byte limit and was refused as unwritable
  (`File name too long`). The temporary name now keeps at most 32 bytes of the destination's
  name, cut at a character boundary, and is at most 57 bytes. A name of 255 bytes, the limit, now
  publishes. It is still in the destination's directory, still created exclusively, and still
  removed on every failure. Tests publish 244- and 255-byte names through a file system seam that
  enforces the 255-byte limit and through the host's own, fail a rename at the longest name, and
  check the temporary name's form.

- Documentation no longer claims more than the code does (flyingrobots/lumen#57, the
  documentation half). "Equal provenance means equal bytes" is scoped to a render of the scene as
  its own tracer: `RenderProvenance`, `render_traced`, the README and ADRs 0002, 0005 and 0007
  say that a caller-supplied `Tracer` is outside it. The stop rule is stated as what is
  guaranteed (a worker that has observed a failure in row `y` begins no row and takes no band from
  `y` down; the reported failure is the raster-first one, with the tests named), not as a
  real-time fact, in the README, the scheduler's documentation and ADR 0003. ADR 0003's untested
  claim that no worker takes a band below the failure is now tested
  (`after_a_failure_no_band_from_its_row_down_is_handed_out`); ignoring `end` when a band is
  taken, formerly an equivalent mutant, fails it. The golden hashes' comments name `BUNNY_REV` as
  the commit they hold at and keep the older revisions as history. The README's claim that the
  one `as` conversion left is `widen` now names the command's information-only cast and the
  CRC table's. ADRs are amended by dated clarification sections; no decision changes.

- A panic in the pixel function now halts the other workers (flyingrobots/lumen#56). Each worker
  owns a guard that calls the halt when dropped while unwinding, so every other worker begins no
  row after it has seen the halt; the panic still reaches the caller through `resume_unwind` once
  every worker has stopped, with no image. Before, on a 64 by 512 image with one-row bands and a
  panic at `(0, 0)`, 32 705 of 32 768 pixels were computed at 2 and 4 workers before the panic
  reached the caller; now, in the simulation, none after the panic. The worker loop is
  `Worker::step` and the spawn sequence `start_helpers`, which the unit tests' simulated workers
  now run themselves: they model a panic (under `catch_unwind`) and a refused spawn after some
  helpers have run, so removing the guard's halt or the halt on a refused spawn each fails a test
  deterministically. `tests/threads.rs` checks on real threads that the panic reaches the caller,
  at 1, 2 and 4 threads. The README no longer says the real-thread error tests bound the stop
  rule: their limit is the whole image, and only the simulation checks the stop rule.

- `lumen render --out` publishes the PNG atomically (flyingrobots/lumen#54): it writes a uniquely
  named temporary file next to the destination, syncs it, renames it into place and syncs the
  directory (Unix), and on any failure removes the temporary file and leaves the destination as it
  was. Before, a failed write left a truncated file and destroyed an existing one. An unwritable
  destination is now refused before the render starts. Changed messages: a directory now reads
  `PATH: is a directory` (was `Is a directory (os error 21)`); a read-only file is refused as
  `PATH: the file is read-only` (was `Permission denied (os error 13)` for a non-root user); a symbolic link is
  replaced instead of written through; `--out /dev/null` now works (it rendered and then failed
  with `Invalid argument (os error 22)` from `fsync`); a replaced file gets the default mode. Exit codes are
  unchanged (1).

### Added

- A measurement record of the canyon scene (`docs/measurements/2026-11-14-canyon.md`, closes
  flyingrobots/lumen#40): `lumen stats` (566 005 triangles, 575 687 tree nodes, leaf depth mean
  19.13 and at most 27, 258 640 800 bytes), three exclusive 4K renders at 16 samples and 18 CPUs
  (render 14.242 to 14.691 seconds, image hash `sha256:cbd02...9eab`, checked outside lumen with
  Python's `zlib` and `hashlib`), the ray counts, the same hash at 1 and 18 threads at 960 by
  540, and 40.7 ns per ray against 15.0 for the reference scene and 18.0 for Fixed Point in the
  same session. `make canyon` and `make 4k-canyon`, and the canyon in `make demo`.

- `lumen render --scene canyon` and `lumen stats --scene canyon` (part of flyingrobots/lumen#40):
  `BuiltinScene::Canyon` registers `lumen-canyon-scene/1`, with a golden image hash (96 by 54, 4
  samples, `sha256:019fcddd...66c5`) and content identity (`sha256:a809761e...f88d`) in
  `tests/golden/mod.rs`, checked by the library, command and thread tests (the same hash at every
  thread count and band height). The canyon's tests add what the camera sees (bunny, each craft
  and the planet by pixel count) and every ray of a 48 by 27 image against a test of all 566 005
  triangles, which runs in the release profile only. The other scenes' hashes and identities are
  unchanged. README, "The canyon scene", with the picture's known limits.

- The canyon scene, `lumen-canyon-scene/1` (part of flyingrobots/lumen#40), assembled:
  `lumen::canyon_scene()` and `scenes::canyon::canyon_content()` build 566 005 triangles (the
  terrain, the three craft, the full-resolution Stanford bunny smooth on its ledge, the planet)
  with 16 materials, a low sun behind the camera, a near-black sky and a camera low in the
  canyon (`scenes::canyon::look`); `CanyonError` for the refusals no input reaches. Tests: the
  parts in order, the trace domain, no degenerate triangle, only the bunny smooth, and the bunny
  resting on its ledge to the grid step. `--scene canyon` and its golden hash follow.

- The canyon scene's craft and planet (part of flyingrobots/lumen#40): three craft of lumen's own
  design (`scenes::canyon::craft`: the skiff, the tug and the kite, 170 triangles of convex
  solids posed by rational rotations, each a painted hull and dark engines) and a banded planet
  (`scenes::canyon::planet`: 16 128 triangles from Bunny's `FixedAngle::sin_cos`, eight latitude
  bands, each its own material), appended by `push_craft` and `push_planet` to a
  `CanyonContent`, the scene's triangle list with each triangle's part, material and normals.
  `fixed_point::solids` is now visible to the crate (no change to Fixed Point).

- The canyon scene's terrain (part of flyingrobots/lumen#40): `scenes::canyon::terrain` places
  `CANYON_HEIGHTFIELD` (seed 2009, a 513 by 470 vertex grid an eighth of a unit apart across the
  canyon and three eighths along it, a floor 14 units across, four-tier walls, a ragged plateau
  about 12 units up, a bend, and a ledge cut into the left wall) and `terrain_triangles` meshes
  it: 480 256 triangles, two per cell, each cell coloured by its rock stratum (three materials in
  bands 1.5 units thick). README, "The canyon scene".

- The canyon terrain generator, `lumen-canyon-heightfield/1` (part of flyingrobots/lumen#40):
  `lumen::scenes::canyon::heightfield::generate` computes a canyon's heights from a
  `HeightfieldSpec` in integers only: layered value noise from a SplitMix64 lattice hash, a
  meandering centre line carried in 256ths of a column, a floor, terraced walls and a plateau,
  and level shelves cut into the rock. The algorithm is specified in the module documentation and
  the README ("Canyon terrain"); `HeightfieldSpec::bounds` states the range of every height and
  `HeightfieldError` names a refused parameter. Tests check exact values against SplitMix64's
  published output and an independent Python model.

- The compiler layer's own check (part of flyingrobots/lumen#115 and flyingrobots/lumen#119):
  `cargo run -p xtask -- clippy-bans`, `make clippy-bans`, and a `Clippy bans` step in the
  `fmt, clippy` job. It runs Clippy on `xtask/clippy-fixture` (a workspace member that compiles
  only under `--cfg lumen_clippy_fixture`) and fails on a misspelt or unresolved ban (Clippy
  only warns, so `Path::exits` had passed with exit 0), on a ban that never fires, on a seeded
  form not reported under its rule, on a reported look-alike, on a second `clippy.toml` or a
  `.clippy.toml` anywhere (a crate's own replaces the root one) or `CLIPPY_CONF_DIR`, and on a
  toolchain that moves a ban lint to another lint group. All 53 bans fire on main. The
  compiler layer's seeded forms, until now a recorded run in ADR 0006, are the fixture's
  `forms.rs`: ADR 0006's 42 that Clippy catches and six from flyingrobots/lumen#115 (an
  `extern crate` alias, three raw identifiers, `r#f64`, float arithmetic on an inferred float);
  each must be reported under its rule.
- The std part of the ban list is derived from the pinned toolchain's documentation
  (flyingrobots/lumen#115): `xtask/bans/std-items.txt` holds every public item of `std::env`,
  `fs`, `process`, `net`, `thread` and `time` and the float methods of `Duration`, as
  `cargo run -p xtask -- std-items <dir> <version>` reads them from rust-docs (`all.html` and
  each item's page, for stability). `clippy-bans` fails when an item decided banned has no
  `clippy.toml` entry, when a new item has no decision, or when the list is for another
  toolchain; ADR 0006 gives the regeneration commands.
  toolchain that moves a ban lint to another lint group. All 53 bans fire on main.
- Run statistics (part of flyingrobots/lumen#29; ADR 0008): `render_traced_with_run` returns
  the render with a `RunStatistics` (the band height, the band count and, per worker, a
  `WorkerStatistics` of bands, rows, pixels and rays), from `render_pixels_by_worker` (`Rendered`,
  `WorkerRun`), whose band counts ride in each worker's outcome returned at `join` (rows are
  its pixels over the width).
  `render --stats` ends with the `run.` lines; each per-worker line is labelled
  `(information only: differs from run to run)`. Counts only: no per-worker clock. A render
  that does not ask pays nothing: the scheduler counts through a private zero-sized tally then,
  and the plain render measured within noise of the base; `--stats` costs about 2.5 % of render
  time (ADR 0008 has the numbers and the two rejected versions).
- A measurement record of Fixed Point version 2 at 4K and 64 samples
  (`docs/measurements/2026-11-14-4k-fixed-point-v2.md`, closes flyingrobots/lumen#39): three
  exclusive runs at 18 CPUs, all with image hash
  `sha256:d276602407edb72109ebfbc8cec9121230171cf156144f139fb2bc3ca1c13bef`, checked outside
  lumen with Python's `zlib` and `hashlib`; totals 20.2 to 22.4 seconds (mean 21.2) against 19.2
  for version 1 at the commit before it; the same hash at 1 and 18 threads at 960 by 540.

- Ray statistics and `lumen render --stats` (part of flyingrobots/lumen#29; ADR 0008):
  `trace_path` records what each ray found in `PathSample::outcomes` (a `RayOutcomes`: camera hits
  and misses, bounce hits, misses and rejections, shadow rays occluded and unoccluded), and
  `Render::statistics` (a `RayStatistics`) adds the pixels, the samples and the fewest and most
  rays of any pixel; `PathSample::rays` and `Render::rays` are unchanged and now derived from them.
  `RayStatistics::check` states what every render satisfies (`RayCheckError` otherwise).
  `render --stats` prints the unchanged report, the scene block of `lumen stats` and the `rays.`
  lines (README, "Statistics"); without it the output is byte-identical. The outcomes are always
  collected: measured, they cost nothing within the noise (README, "Performance"; ADR 0008).
  `PathSample` and `Render` gain a field, which breaks a caller that builds one by its fields.
- `lumen stats [--scene NAME]` (part of flyingrobots/lumen#29; ADR 0008) builds a scene and
  prints its statistics without rendering: its name, content identity and integrator, then
  `key value` lines for the triangles by material with each albedo, the bounds, the sun, the sky,
  the camera placement, the tree's builder, layout and shape, and the bytes of each buffer
  (README, "Statistics", documents every key). Fixed-point values print in decimal with six
  places, rounded to nearest with ties away from zero, from their exact raw integers; the mean
  leaf depth with two places, by integer division. A scene whose statistics cannot be read is a
  new `Failure::Statistics` (exit code 1), which no built-in scene reaches.
- Scene statistics (part of flyingrobots/lumen#29; [ADR 0008](docs/adr/0008-statistics.md)):
  `Scene::statistics()` returns a `SceneStatistics`, computed on request from the built scene and
  never on a ray's path: triangles in all, by material, smooth and of zero area; the bounds; the
  tree's shape (`TreeStatistics`: nodes, interior nodes, leaves, the deepest leaf and the sum of
  leaf depths, triangles per leaf, fewest, most and a histogram over `LEAF_SIZE_BUCKETS`), read by
  a walk of Bunny's public compact nodes; and the bytes of each buffer (`SceneMemory`). The tree's
  builder and layout are named by `TREE_BUILDER` and `TREE_LAYOUT`; `Scene::materials()` gives
  the albedo table. `crates/lumen/tests/statistics.rs` (small) checks the tree's shape against a
  recursive walk of a tree rebuilt with Bunny's builder from the same boxes and against Bunny's
  documented bounds, the per-material counts against the reference scene's material indices, and
  smooth, zero-area and unused-material counts on a scene built to hold them. ADR 0008 also
  decides the command's output and the ray, run and traversal statistics, which are not yet
  built; the traversal counters wait for flyingrobots/bunny#335.
- Independent oracles for what a render reports (flyingrobots/lumen#55), in
  `crates/lumen/tests/counts.rs` (medium: real threads at 1, 2 and 4): a pixel at 64 and 1024
  samples equals that many direct `trace_path` calls in one `PixelAccumulator`; `Render::rays` and
  every `PathSample::rays` equal the queries a counting tracer that wraps the scene observed, per
  kind, for both built-in scenes, a camera that sees only sky (camera rays only) and a smooth
  floor where Bunny rejects bounces (a rejected bounce traces and counts no ray); camera rays are
  exactly `width * height * samples` and the total at most four times that; seeds differing only
  in bit 31 or bit 0 give different images, each equal to a recomputation with Bunny's
  `PixelSampler::new`. `png/tests.rs` reads the stored-block headers at exactly 65 535 filtered
  bytes (one block) and the next sizes up (two); the strict decoder now reports each block's
  length. The sample cap at 32, the doubled shadow count, the uncounted bounce, `MAX_STORED =
  65_534`, `MAX_STORED_U64 = 65_534` and the dropped top and lowest seed bits each fail a test.
- Smooth shading (part of flyingrobots/lumen#39; ADR 0007): `trace_path` shades a hit on a
  triangle with at least one vertex normal with Bunny's `bunny-smooth-shading/1`
  (`SmoothHit::new` at the hit's exact barycentric numerators, its light sample and its bounce,
  which Bunny may reject as below the surface: then no bounce ray is traced and it adds zero).
  Other triangles are shaded flat as before. A scene with any vertex normal is rendered by
  `lumen-smooth-diffuse-path/1` (`SMOOTH_INTEGRATOR_ID`), which its provenance's `integrator`
  field names (`Integrator::for_scene`, `Scene::smooth_triangles`); its `shading` field is then
  `bunny-diffuse-shading/1+bunny-smooth-shading/1+bunny-smooth-normal/1` (`SMOOTH_SHADING_IDS`,
  `Integrator::shading_id`). Every other scene keeps `lumen-diffuse-path/1` and
  `bunny-diffuse-shading/1`, and both built-in scenes keep their image hashes and their printed
  provenance. The shading of a hit
  moved to `render/shade.rs` first, unchanged. `tests/smooth.rs` (also on the WebAssembly leg)
  checks the integrator's choice, the reference scene with its face normals at every vertex
  rendering its flat bytes, a hand-derived smooth value, the rejected bounce, and every sample of a
  mixed scene against Bunny's steps composed in the test over brute-force queries; four seeded
  mutations of the integrator each fail it.

- Vertex normals in the scene model (part of flyingrobots/lumen#39): `SceneSpec::normals`, empty
  or three `Option<FixedUnitVec3>` per triangle (the input of Bunny's `SmoothHit::new`), checked
  by `Scene::new` (the new `SceneError::NormalCount`, with both counts) and kept
  (`Scene::vertex_normals`). Every `SceneSpec` literal gains `normals`; the built-in scenes pass
  none. No integrator reads them yet, and no pixel changes.

- The full-resolution Stanford bunny (part of flyingrobots/lumen#39):
  `assets/stanford-bunny/bun_zipper.ply` (35 947 vertices, 69 451 faces), the archive's file byte
  for byte (SHA-256 `b1acc63b...66fe`, 3 033 195 bytes, trailing spaces kept), with its
  provenance in `assets/stanford-bunny/README.md`. It is embedded as
  `scenes::fixed_point::full_bunny::FULL_BUNNY_PLY`, which no scene uses yet. Tests pin its
  hash and length, Stanford's counts, its extreme decimals as bounds, and its topology as
  Bunny's reader reads it: no duplicate vertex or face, 1 113 unused vertices, manifold and
  consistently wound, and 223 boundary edges in five holes. Unused, it adds no byte to the
  command; an experimental build that renders it is 3.08 MB larger.

- Nondeterminism bans, the compiler layer (part of flyingrobots/lumen#17; ADR 0006): `clippy.toml`
  bans the clock, sleeping, the core count, thread spawning, hash maps and sets, arguments, the
  environment, files, processes, sockets and the float types, each reason starting with an Echo
  `ban-nondeterminism` rule ID, and `lib.rs` denies float arithmetic. The command, examples,
  `tests/cli.rs`, `xtask`, the scheduler's thread pool and two unit tests carry reasoned
  `#[expect]` waivers. `deny.toml` now allows no registry, so a crates.io crate fails
  `Gates / cargo-deny`. Main had no finding inside the deterministic core other than the
  scheduler's threads. No code, pixel or output byte changed.
- Nondeterminism scan (flyingrobots/lumen#17; ADR 0006): `cargo run -p xtask -- nondeterminism`,
  `make nondeterminism` and the `Gates / nondeterminism` job read the library's tokens for what
  Clippy cannot see: float types, literals and casts, banned names and `std` paths under a `cfg`
  that is not built or in a macro body, open addressing in any line, resolved through each file's
  `use` trees; and every file's ban waivers must be `expect` with rule IDs in the reason. Waivers
  are `// nondeterminism: allow <rule-id> -- <reason>`; a malformed or unused one fails. 46 of the
  50 seeded forms and none of the 10 look-alikes are reported (`xtask/src/nondeterminism/tests.rs`);
  with Clippy, 50 of 50. Main has no finding.

### Changed

- Fixed Point is `lumen-fixed-point-scene/2` (part of flyingrobots/lumen#39); version 1 is
  retired. The bunny is the full-resolution Stanford bunny (69 451 triangles, none dropped),
  shaded smooth with Bunny's `VertexNormals` computed over the placed and posed positions (each
  vertex placed once, the faces kept as indices), so the scene is rendered by
  `lumen-smooth-diffuse-path/1`; scale, turn and position are unchanged. The glasses' right arm
  ends further out (240/256 instead of 210/256), measured to clear the full mesh's head. The
  puck reads as a white disc with black lettering: its rim is a 1/32 ring, its side is white
  (the new `PartKind::PuckSide`), it is 1/8 high instead of 3/16, and "ON" is bolder. The sun
  moves to nearly behind the camera, toward `(-0.25, 1.0, 1.0)` (was `(-0.5, 1.0, 0.875)`), so
  the smooth bunny's shadow edges, which follow its triangles, fall off its face and chest. New
  `FixedPointError::Normals` (with Bunny's `VertexNormalError` as its source) and
  `FixedPointContent::normals`. New tests: every bunny triangle has three vertex normals and no
  other part any; the puck as the camera sees it (white and black pixel counts and rendered
  levels). Golden 96 by 54 hash
  `sha256:da6cb05db0d366a2088f19e6eefed2ab017dd0d27c6c5e364a85121054d6093e`, content
  `sha256:f80160bbd1ebb6e79f186bf5a86f2ce3419d4ef31168cc4cab0b6fd1f0f64e11`.
- The command line, part of flyingrobots/lumen#57 (the documentation claims in that issue remain):
  a separate option value that reads as an option (`--out --width`) is a usage error, exit 2,
  instead of a file named `--width`; every option also takes `--option=value`, which allows such a
  value. A refused image size exits 2, like a refused sample count, thread count or band height
  (was 1); its message is unchanged. Leading zeros in numbers stay accepted for every numeric
  option and are now documented and tested. The report's byte count is the number of bytes
  written to the file (checked against the PNG's length), not the expectation itself.

- The scene content encoding is `lumen-scene-content/2` (part of flyingrobots/lumen#39; ADR
  0005): after the camera, a `u64` count of vertex-normal entries (zero, or one per triangle),
  then per vertex a tag and, if present, the normal's three raw components. Every content digest
  changes, the built-in scenes' included (reference
  `sha256:c1e109477c09328421d76efefd923bd625d842fe3afb93dfadb21ae905bd0f66`, Fixed Point
  `sha256:7094ced63e59606707dc9c172c1801426183ff0b1ceda06a1c65a6a11062a086`); no image hash
  does. The Python-computed golden is recomputed for version 2 and joined by one with normals.
- Bunny is pinned to `d4fe51d38fa10f8d6496bd110955314edac30bbf` (was
  `3c0dab3e77c0d45de8904906a134e846dfe5ec6c`), Bunny's main after its smooth-shading work
  (flyingrobots/bunny#289: `VertexNormals`, `smooth_normal`, `SmoothHit`, and
  `TriangleHit::barycentric_numerators`) and the traversal visit guard (a traversal that
  would visit more interior nodes than the tree holds, which only a tree with a cycle can, now
  fails with `TraversalError::InvalidNodeIndex`; a tree built by `Scene::new` has none). Part
  of flyingrobots/lumen#39. Lumen calls none of the new APIs yet; both golden image hashes and
  both golden content digests are unchanged.
- The `lumen` command's `main` maps one typed `Failure` to its message and exit code (part of
  flyingrobots/lumen#21), in place of `Result<(), String>` and errors turned into strings as
  they arose. Each variant keeps the library's or the system's error as its `source()`. Every
  message and exit code is byte-identical to before.

- `write_png` returns `Result<(), PngError>` instead of an `io::Result` built partly from
  strings (part of flyingrobots/lumen#21). `PngError::Write` keeps the writer's `io::Error` as
  its `source()` and its text; the encoder's own length checks are `ChunkTooLong { length }`
  and `BlockTooLong { length }`, which no image reaches. `PngError` is `#[non_exhaustive]`. The
  command prints the same messages.

- A refused scene says what was wrong (part of flyingrobots/lumen#21). `SceneError` variants
  carry their values: `TooManyTriangles { triangles, limit }`, `OutsideTraceDomain { primitive,
  vertex, point }` (lumen repeats Bunny's documented check of `TraceTriangle::new` after a
  refusal to name the vertex; `TriangleRefused` if every coordinate is inside),
  `MaterialCount { triangles, material_indices }`, `UnknownMaterial { primitive, material,
  materials }` and `OutOfMemory { buffer, items, error }` with a new `SceneBuffer` and the
  allocator's `TryReserveError`. `BvhBuild` is split into `NodeCountOverflow { triangles }`
  (formerly reported as `TooManyTriangles`), `BvhBuild { triangles, nodes }` and
  `BvhCompact(CompactError)`, which keeps Bunny's conversion error. `SceneError`,
  `FixedPointError`, `BuiltinSceneError` and `PlaceError` implement `source()`;
  `FixedPointError`'s text no longer prints a `Debug` form. `SceneError`, `FixedPointError` and
  `BuiltinSceneError` are no longer `Copy`; they, `SceneBuffer` and `PlaceError` are
  `#[non_exhaustive]`. No pixel, hash, digest, line of the command's output or message it
  prints changes.

- A ray query says what failed (part of flyingrobots/lumen#21). `TraceError::InconsistentHit`
  is split into `UnknownPrimitive { primitive, triangles }` and `HitNotRepeatable { primitive,
  distance }`, and `Display` names Bunny's `TraversalError` kinds in words instead of printing
  their `Debug` form (`traversal failed: its stack of 64 nodes overflowed`, not `traversal
  failed: StackOverflow`). `Scene::new` checks once that the tree names only its own
  triangles (`SceneError::TreeNamesUnknownPrimitive`, which no input reaches), so the traversal
  callbacks' missing-triangle case, formerly a silent miss, cannot occur for a built scene.
  `PrimitiveIndex::to_usize` returns `usize`: the conversion is lossless on every target lumen
  builds for. No pixel, hash, digest or line of the command's output changes.

- Memory, spawn, size and buffer-length failures say what failed (part of
  flyingrobots/lumen#21). `RenderError::OutOfMemory` and `WorkerError::OutOfMemory` carry a
  `Reservation` (the image's bytes or the worker handles) and the allocator's
  `TryReserveError`; `Spawn` carries a `SpawnError` with the refused worker's number and the
  operating system's `io::Error` as its `source()`. `OutOfMemory` is no longer returned for a
  failure that is not memory: the workers render into a buffer the image reserved at its length,
  so the impossible `None` of `Rgba8Image::from_bytes` is gone. `from_bytes` returns
  `Result<_, LengthError>` with both lengths; `ImageSize::byte_len` returns `usize`, computed and
  checked once by `ImageSize::new`; `SizeError`'s variants carry the size asked for and the
  limit. `RenderError` and `WorkerError` are no longer `Copy`. `WorkerError`, `Reservation` and
  `SizeError` are `#[non_exhaustive]`. A spawn failure after a helper has started is now tested
  through an injected spawner, and the halt it calls in the worker simulator. No pixel, hash,
  digest, line of the command's output or message the command prints changes.

- A failed path says what failed (part of flyingrobots/lumen#21). `trace_path` and
  `pixel_radiance` return the new `PathError`, and `RenderError::Shading`, which merged four
  causes and carried nothing, is gone: `PathError::UnknownPrimitive` (the index and the scene's
  triangle count), `NoSurface` (a zero-area triangle), `SurfaceHit` and `BounceRay` (the
  triangle and Bunny's `SecondaryRayError`), `RadianceOverflow` (the triangle and both terms),
  `NoRay` (the sample index and Bunny's `PixelSample`), `Trace` and `Pixel`. Lumen calls Bunny's
  fast `Option` forms and only after a `None` the `try_` form for the reason, so a render that
  succeeds runs the same code (render time unchanged within noise; see the pull request). A
  failed render keeps the failing pixel's coordinates: `RenderError::Pixel { x, y, error }`
  replaces `RenderError::NoRay`, `Trace` and `Pixel`, which `render_traced` returned without
  them. `RenderError` and `PathError` are `#[non_exhaustive]` and implement `source()`. No pixel,
  hash, digest or line of the command's output changes, and no message the command prints.

- The command-line tests are hermetic and one-behaviour-each (part of flyingrobots/lumen#24): every
  test that runs `lumen` owns a scratch directory named for the test and the process id, used as
  the working directory with absolute paths, and removed at the end, so refusal tests cannot
  write into the source tree and the test and release profiles can run at once. Tests that
  covered two behaviours or branched on the platform are split, and each integration test file
  states a size class; the README's new Contributing section documents both conventions.
- Generated tests for the claims that quantify over inputs (flyingrobots/lumen#24): a pixel's
  result is the same for random evaluation orders, permutations and merge trees of its samples
  (`tests/generated.rs`, also run on `wasm32-wasip1`); SHA-256 gives the digest of the whole for
  random splits of random inputs of boundary and random lengths, and the published digests for
  random splits of the published vectors (`src/sha256.rs`); PNG files of 73 random and
  block-boundary sizes decode to the same pixels, have the length `png_len` gives and the block
  count the format defines (`src/png/generated.rs`). The seeded generator is
  `tests/generator/mod.rs`. Each test was shown to fail on a seeded mutation of the code it checks.
- `png.rs`'s unit tests moved to `png/tests.rs` (flyingrobots/lumen#34), so the file is under the
  300-line review limit and its entry in the file-length gate's exceptions is gone. No code,
  public path or output byte changed.
- `RenderProvenance` is built from the scene, not a label (flyingrobots/lumen#38, ADR 0005):
  `RenderProvenance::new(&scene, size, settings)` for an unnamed scene and
  `RenderProvenance::named(name, &scene, size, settings)` with a name. The `&'static str`
  constructor and `scene()` are gone; `name()` and `scene_content()` replace them. `fields()`
  gains a thirteenth field, `scene-content` (`ProvenanceValue::Content`), at the end, and the
  `scene` field of an unnamed scene is `ProvenanceValue::Unnamed`; `PROVENANCE_FIELDS` is 13.
  lumen is unreleased, so there is no deprecation step. `lumen render` prints one more line,
  `scene-content sha256:...`, after `bounces`; every other line is unchanged. The built-in
  scenes' digests are pinned in `tests/golden/mod.rs` and checked by the library and command
  tests, and a one-triangle scene's digest computed with Python's `hashlib` is pinned too. No
  pixel changes.

- `Scene::new` refuses an invalid camera placement (part of flyingrobots/lumen#20): it builds
  Bunny's `PinholeCamera` at a one-pixel image before anything is allocated and returns the new
  `SceneError::Camera(CameraError)` for a position outside the trace domain, a tangent not in
  `(0, 256]`, a zero look direction or an up hint that is zero or parallel to it. Before, such a
  scene built and every render of it failed with `RenderError::Camera`. A test covers each
  invalid field; a mutation that discards the check fails it.
- `lumen::reference::reference_lighting()` and `reference_materials()` return
  `Result<_, ShadingError>` instead of panicking if Bunny refused a constant, as the Fixed Point
  scene's do; `reference_scene()` reports such a refusal as the new `SceneError::Constant`.
- `SceneError` and `TraceError` are `#[non_exhaustive]` (ADR 0002): a `match` on them outside
  the crate needs a wildcard arm.
- The `RayCount` addition's documentation says a sample traces at most four rays (camera,
  bounce and a shadow ray at each hit), not five; the bound of 2^39 rays per render stands.

- Bunny is pinned to `3c0dab3e77c0d45de8904906a134e846dfe5ec6c` (was
  `dcd6bd37613738de93f7084e7c8aa0283de964ea`), a commit of Bunny's main after the 0.7.1 release,
  for every Bunny crate lumen uses, and `bunny-codec` (default features off, so no crates.io
  crate) is added (flyingrobots/lumen#27). Both golden image hashes are unchanged. Of Bunny's
  0.7.1 behaviour changes, `ray_intersects_aabb` and `intersect_ray` are not called by lumen,
  whose traversal uses the compact BVH, and the `try_look_at` correction is not used.
- The Stanford bunny is read with Bunny's ASCII PLY reader (`bunny_codec::parse_ascii_ply`) and
  the interim reader is deleted: `mesh/ply.rs`, `mesh/decimal.rs`, their tests and the identity
  `lumen-ascii-ply-q32.32/1`, which reported what Bunny's `bunny-decimal-q32-32/1` and
  `bunny-ascii-ply-triangle-mesh/1` now report. `PlaceError::Ply` carries Bunny's
  `AsciiPlyError` (kind and line). Bunny's reader accepts more than the interim one (CRLF, other
  whitespace, comments, other vertex properties) and has no count or line-length limits; the
  embedded file is fixed, and the converted mesh is pinned by a SHA-256 recorded from the interim
  reader, which agreed with Bunny's on every vertex and face. `lumen::mesh` no longer exports
  `parse_ascii_ply`, `PlyError`, `Element`, `DecimalError`, `decimal_to_q32_32` or `ASCII_PLY_ID`;
  it exports `read_ascii_ply`.

### Added

- A `Makefile` (flyingrobots/lumen#104): `make demo`, `make reference`, `make fixed-point`,
  `make 4k`, `make 4k-fixed-point`, `make test` and `make help`. Each runs the documented
  container command through `devrun` and puts the image in `out/`, which git ignores.

- A scene's content identity (part of flyingrobots/lumen#38, ADR 0005): `Scene::content_id()`
  returns a `SceneContentId`, the SHA-256 of the canonical encoding `lumen-scene-content/1`
  (`SCENE_CONTENT_ENCODING`) of the `SceneSpec`: the triangles' raw coordinates in primitive
  order, each with its material index, the material table, a tagged optional light, the sky and
  the camera placement, little-endian with `u64` counts. `Scene::new` computes it once, from the
  spec alone, before the tree is built: 6.8 ms of 33.7 ms for the reference scene in the release
  profile (120 ms of 154 ms unoptimized), 1.3 ms of 5.6 ms for Fixed Point. Tests rebuild the
  encoding by hand from the ADR, change each field once (every coordinate of a triangle, the
  order, a material index, each albedo channel, the light's direction, irradiance and presence,
  the sky, each camera field), build two scenes that a count-free encoding would confuse, and
  move a triangle inside a closed cube, which changes the digest and, with every ray checked
  against a test of every other triangle, no pixel. Mutations that drop the material indices,
  hash in sorted order, omit a count, the light tag or the camera each fail a test. No pixel
  changes; both golden image hashes are unchanged.
- `lumen::PrimitiveIndex` (flyingrobots/lumen#20): the name of a triangle in its scene, a `u32`
  that is its zero-based position in `SceneSpec::triangles`, in input order. `Hit::primitive`,
  `Scene::surface`, `Scene::albedo` and `SceneError::OutsideTraceDomain` /
  `SceneError::UnknownMaterial` take or carry it in place of `u32` and `usize`, so the scene code
  no longer mixes the two for one quantity. Its documentation and ADR 0002 state that the order
  breaks ties (the lowest index wins) and is part of the scene's identity; a test shows two
  coincident triangles listed in the other order give the other one's material.

- Supply-chain, size and coverage checks in CI (flyingrobots/lumen#23), in
  `.github/workflows/gates.yml`: `Gates / cargo-deny` (`deny.toml`: advisories, Apache-2.0 only,
  no duplicate or wildcard versions, crates.io and Bunny's git repository by `rev` as the only
  sources), `Gates / cargo-audit`, `Gates / file length` and `Signal / coverage` (line coverage
  in the job summary; it never fails).
- `xtask`, a workspace member with the file-length gate: `cargo run --locked -p xtask --
  file-length` counts lines of code (non-blank, outside comments) in every `.rs` file under
  `crates/` and `xtask/` and applies the Rust standard's limits (300 for a source file without an
  exception, 500 at most, 750 for a test file). Two files are listed as dated exceptions:
  `crates/lumen/src/scene.rs` (327, flyingrobots/lumen#20) and `crates/lumen/src/png.rs` (328,
  flyingrobots/lumen#34).
- A determinism matrix in CI (flyingrobots/lumen#23): `Determinism / Linux x86-64`,
  `Determinism / Linux aarch64`, `Determinism / macOS aarch64` and `Determinism / Windows x86-64`
  run the whole test suite in the test and the release profile, each first checking that the
  runner's `rustc` host is the platform the job is named for; `Determinism / WebAssembly
  (wasm32-wasip1, Node)` runs the library's unit tests, `tests/render.rs`, `tests/library.rs` and
  `tests/fixed_point/` as `wasm32-wasip1` programs under Node's built-in WASI
  (`scripts/wasi-run.mjs`) in both profiles, and compiles the library for
  `wasm32-unknown-unknown`. Every job checks the same two golden hash constants (the reference
  scene and the Fixed Point scene, `tests/golden/mod.rs`).
- `tests/library.rs`: the golden image rendered on one worker through the library alone (runs on
  every platform, WebAssembly included), and on WebAssembly a two-worker render must be
  `RenderError::Spawn`.
- `docs/adr/0004-webassembly-determinism-leg.md`: WebAssembly is tested on `wasm32-wasip1` under
  Node's WASI with the standard test harness and no added dependency, on one worker.
- `docs/adr/0003-row-bands.md` (flyingrobots/lumen#28): the row-band scheduler, the proof that
  the reported error is the raster-first failure and that work past it stops, the measurements
  that located the 6a816ef slowdown in code generation, and what was rejected. It supersedes
  ADR 0001. A dated record, `docs/measurements/2026-11-13-4k-row-bands.md`: the 4K reference
  scene at `c731bb4`, three runs at 18 threads (total 5.983 to 6.582 s) and one at 1 thread
  (66.156 s), with the same image hash as before, and an interleaved Fixed Point 4K comparison
  at 64 samples (median 20.734 s with row bands, 23.075 s with tiles, the same hash).
- `lumen render --scene reference|fixed-point` (flyingrobots/lumen#15): the command renders the
  Fixed Point scene. The default is `reference`, whose output and golden hash are unchanged. An
  unknown name is a usage error (exit code 2) listing the valid names, and `--help` lists them.
- `lumen::BuiltinScene` (`ALL`, `name()`, `id()`, `build()`, `FromStr`) with `ParseSceneError`
  and `BuiltinSceneError`: the registry of named scenes, in the library so another front end
  gets the same list. The scene identity in `RenderProvenance` now comes from the chosen scene.
  The `fixed_point` example uses it, and the Fixed Point golden hash moved to the shared
  `tests/golden/mod.rs` so the command's test and the library's use one constant.
- The "Fixed Point" scene, `lumen-fixed-point-scene/1` (part of flyingrobots/lumen#15), as
  library content in `crates/lumen/src/scenes/fixed_point/`: the Stanford bunny in dark
  sunglasses at a craps table, with two identical dice showing four pips on every face and the
  ON puck on the 8. 5 975 triangles: felt, padded rail, layout lines and seven-segment numerals
  for the boxes 4, 5, 6, 8, 9 and 10; the puck; two chamfered dice, the same mesh under two
  exact rational turns; the bunny scaled by 32 and turned three-quarters toward the camera; and
  sunglasses fitted to the bunny's measured nose. `lumen::fixed_point_scene()` builds it and
  `fixed_point_content()` lists every triangle's part and material.
- `crates/lumen/examples/fixed_point.rs`, which renders the Fixed Point scene to a PNG without
  the CLI.
- `MeshTurn::QuarterAboutY`, a quarter turn of a placed mesh, `(x, y, z)` to `(z, y, -x)`.
- Tests of the Fixed Point scene: four pips on every face of both dice, counted from the
  geometry; the dice congruent and related by their stated motion; the numerals against a
  seven-segment truth table; the puck inside the 8 box; the sunglasses within 0.25 units in
  front of the face and touching nothing; counts, the trace domain, no degenerate triangle and
  everything resting on the felt; every traced ray against brute force; a golden hash (96 by 54,
  4 samples, seed 0). The reference scene's golden hash is unchanged.

- `RenderProvenance` (flyingrobots/lumen#19): the record of what was rendered, built from the
  scene's identity, the image size and the settings. `fields()` lists `scene`, `integrator`,
  `shading`, `sampler`, `tone`, `encoding`, `bunny`, `size`, `pixels`, `samples`, `seed` and
  `bounces` in that stable order as `ProvenanceValue`s. `lumen render` prints its first twelve
  lines from it. Threads, band height, file and times are facts of the run and stay in the command's
  report.
- Compiling doc examples for building a scene, rendering, hashing and provenance, and writing a
  PNG into memory; `crates/lumen/tests/library.rs`, a render through the library API alone
  checked against the golden hash and PNG bytes recorded before the change.
- `docs/adr/0002-library-api-surface.md`: the crate root is the API, and what is and is not
  provenance.
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
- A dated measurement record, `docs/measurements/2026-11-13-4k-reference.md`
  (flyingrobots/lumen#5): the 4K reference scene at 16 samples per pixel at `d8deec4`, five runs at
  18 CPUs with a total of 5.828 to 6.232 seconds, one run at 1 thread (72.457 s) and one at 6 CPUs
  (13.767 s), the same image hash in all, with its conditions and limits. Linked from the README.
- A "Performance" section in the README: how to reproduce the thread-scaling, tile and 4K
  measurements through `devrun` with `--cpus` and `--exclusive`.

- The Stanford bunny in the reference scene (flyingrobots/lumen#14): `lumen-reference-scene/3`
  appends its 3 851 triangles (none dropped; 31 511 triangles in all) after the triangles of
  version 2, with a green albedo, standing on the ground left of the sphere. The mesh,
  `assets/stanford-bunny/bun_zipper_res3.ply`, is Geordi's checked-in copy of Stanford's
  `bun_zipper_res3.ply` (Stanford 3D Scanning Repository, Stanford Computer Graphics
  Laboratory), byte for byte (SHA-256
  `975e7f9b160b4ea15b0e225e21b10828ebcf678df020d2f6a46aa408fdcf5cd6`), with its provenance,
  Stanford's reconstruction README and attribution; `.gitattributes` keeps its bytes intact.
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

- `scene.rs` is split by responsibility (part of flyingrobots/lumen#20): `scene/mod.rs` (the
  scene model: `Scene`, `SceneSpec`, `Lighting`, `CameraPlacement`, `SceneError`),
  `scene/tracer.rs` (the `Tracer` port with `Hit`, `TraceError` and `FAR`), `scene/tree.rs` (the
  adapter that builds and holds Bunny's tree and implements `Tracer` for `Scene`) and
  `scene/tests.rs`. A move only: the crate root's paths, every image hash and the command's
  output are unchanged. Each file is under 300 lines of code, so the file-length gate's exception
  for `scene.rs` is removed.

- CI: the `fmt, clippy, test` job is now `fmt, clippy`; the tests run in the `Determinism / ...`
  jobs (flyingrobots/lumen#23). The README's and the crate documentation's platform claim is
  narrowed to the five platforms those jobs check, with a table of which job proves which.

- Threads take bands of whole rows instead of tiles (flyingrobots/lumen#28; ADR 0003 supersedes
  ADR 0001). `chunks_mut` cuts the image into disjoint row bands, and a worker writes its band's
  pixels straight into the image: the per-worker tile buffer, the copy and the image mutex are
  gone. One small mutex around the band iterator keeps hand-out dynamic. Each worker returns its
  ray count and earliest failure through `join`, and the caller sums the counts and keeps the
  smallest `(y, x)`: the record mutex is gone. The only shared value a worker writes is one
  atomic, the first row no longer wanted. After a failure in row `y`, no row from `y` down is
  begun (tiles stopped at the end of the failing row of tiles). The image, the ray count and the
  reported error are unchanged at every schedule; the golden and 4K hashes hold.
- `lumen render --tile N | WxH` is replaced by `--band-rows R` (default 1, chosen by
  measurement), and the report's `tile WxH` line by `band-rows R`. lumen is unreleased, so
  `--tile` is removed rather than kept with another meaning; it is now refused as an unknown
  option. In the library, `BandRows` and `DEFAULT_BAND_ROWS` replace `TileSize` and
  `DEFAULT_TILE`, `Schedule::band` and `Schedule::with_band` replace `Schedule::tile` and
  `Schedule::with_tile`, and `ScheduleError::EmptyBand` replaces `ScheduleError::EmptyTile`.
- Release builds use `lto = "fat"` and `codegen-units = 1`: renders are 6 to 18 % faster at 1, 4
  and 18 threads (1280 by 720, 16 samples, interleaved exclusive runs; the 4K render at one
  thread went from 72.5 s to 66.2 s, at 18 threads it is unchanged within noise). This also accounts for
  the 8 % multi-thread slowdown that appeared when the scheduler was split out (0fc51d7 to
  6a816ef). With whole-program optimization both commits are equally fast, so the slowdown came
  from inlining across the Bunny crate boundary, not from the shared state. No pixel changes.
- The command line moves out of the library (flyingrobots/lumen#19): `lumen::cli` is gone, and
  argument parsing, the usage text, the default thread count, the clock, the file and the
  terminal live in the `lumen` binary (`crates/lumen/src/bin/lumen/`). The command's output and
  exit codes are unchanged byte for byte apart from the timing values.
- The library's API is the crate root: the `image`, `png`, `render`, `scene`, `schedule`,
  `sha256` and `workers` modules are private, and their public items are re-exported at the
  root (`lumen::trace_path`, `lumen::SceneSpec`, `lumen::write_png`, ...). `reference` and
  `mesh` stay public modules. The SHA-256 implementation, the PNG signature and the CRC-32 and
  Adler-32 types are internal. No pixel and no golden hash changes.
- `lumen render --out FILE` is optional: without it the image is rendered and hashed but not
  written, and the report says `png none`. `UsageError::MissingOut` is gone; `UsageError` gains
  `Schedule` and `InvalidTile`, and `Command::Render` gains `threads` and `tile`.
- `render` (one thread) now runs through the tile scheduler with one worker; its bytes, ray count
  and errors are unchanged (the golden hash holds).
- The report's `render-seconds` no longer includes the scene build, and `ns-per-ray` is the
  render's wall time divided by the rays traced (on all threads).
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

- `TileSize`, `DEFAULT_TILE`, `RenderError::TileGeometry`, `WorkerError::TileGeometry` and the
  command's `UsageError::InvalidTile` (flyingrobots/lumen#28): there are no tiles and no tile
  geometry left to fail.
- `lumen-reference-scene/2` is retired.
- `lumen-reference-scene/1` and the normal-colour debugging view `lumen-normal-shading/1` are
  retired; the view is not kept behind a flag.

### Fixed

- `clippy.toml` lists every stable function and type of `std::env`, `std::fs`, `std::process`
  and `std::net` (but the address types) in the pinned toolchain's documentation, so the members
  flyingrobots/lumen#115 found unlisted are banned in the compiler layer: `std::fs::exists`,
  `copy` and the rest of `fs`, `env::set_current_dir`, `set_var`, `split_paths`, the process
  and socket types, `ToSocketAddrs`; also `thread::sleep_ms`, `park_timeout`,
  `park_timeout_ms`, `SystemTimeError`, the twelve stable float methods of `Duration`
  (`as_secs_f64`, `mul_f64`, `from_secs_f64`, ...) and Bunny's float bridge (`Scalar`,
  `Vec2`/`Vec3`, `Ray3`/`Aabb3`/`Sphere3`, the codec's float meshes and vertices, `from_f32`,
  `try_from_f32`, `into_float`, `try_from_float`): 143 bans, up from 53, each firing on
  `xtask/clippy-fixture`. The token scan also reports Bunny's float-bridge types by path and
  names ending `_float`. Main has no new finding in either layer.
- The nondeterminism scan resolves `extern crate std as host` and chains of renames
  (`use host::env as e; e::var`), so std members `clippy.toml` does not list are caught by the
  scan's module rule when reached through an alias: `host::fs::exists`,
  `host::env::set_current_dir`, `host::env::consts::OS` (part of flyingrobots/lumen#115 and
  flyingrobots/lumen#119). `std::thread::sleep_ms`, `sleep_until`, `park_timeout` and
  `park_timeout_ms` are `thread-sleep`. A name with the word `f32` or `f64` (`as_secs_f64`,
  `mul_f64`, `from_secs_f64`, `to_f32`) is `float-type`, which closes float arithmetic written
  with no float token. Main has none of these.
- Nothing but a reasoned `expect` turns a nondeterminism ban off (part of
  flyingrobots/lumen#119). Scan waivers are taken from comments only, so
  `// nondeterminism: allow ...` inside a string literal no longer waives a finding on its line.
  The scan now fails, in every file: `#[allow(clippy::all)]`, `#[allow(clippy::style)]` and
  `#[allow(clippy::restriction)]` (each silenced a ban in Clippy 0.1.96), and any other lowering
  of those groups or of `warnings`; `warn` of a ban lint (silenced under `allow(warnings)`); the
  renamed `disallowed_method` and `disallowed_type`; a lint list built by a macro
  (`#[allow(clippy::$lint)]`); and, in the core, any attribute built from a macro metavariable.
  Main has none of these.
- The gates decide which files are test files, and which are the deterministic core, by the
  module tree, not by file names (closes flyingrobots/lumen#120, part of flyingrobots/lumen#119):
  `xtask/src/layout.rs` follows each crate's `mod` declarations as `rustc` resolves them. A
  module named `tests.rs` or under `src/tests/` without `#[cfg(test)]` is now held to the
  300-line source limit and scanned; a library module loaded by `#[path]` from `tests/` is core.
  Both gates fail on `include!` and on a `#[path]` to anything but a `.rs` file they read (a
  `.inc` file was neither counted nor scanned) and on a `path` inside `cfg_attr`. On main, only
  `src/png/generated.rs` changes role: it is declared only under `#[cfg(test)]`, so it is a
  test file and no longer scanned.
- The file-length gate and the nondeterminism scan read a file as `rustc` lexes it
  (flyingrobots/lumen#120, part of flyingrobots/lumen#119): a leading shebang line is skipped,
  not lexed, so `#!/bin/x /*` no longer hides the rest of a file from both gates (a 603-line file
  had counted as one line and been scanned not at all); a byte-order mark before it is skipped
  too. Float literals follow the Rust reference's grammar, so `..1.5` and `1e+_3` are floats and
  `1usize+2`, which the scan reported as a float, is not. Raw identifiers are read without `r#`,
  so `std::r#env::var` is a `std-env` finding. New tests kill the three mutations of the lexer
  and the `env!` ban that the review found surviving.
- The PLY reader's generated-corruption test assumed a 64-bit `usize`: it converted each 64-bit
  generator output to `usize` before reducing it, which fails on `wasm32` and aborted the
  WebAssembly test run (found by the new WebAssembly job, flyingrobots/lumen#23). It now reduces
  in `u64` first; on 64-bit targets the generated cases are the same as before.

- `lumen render` reads its arguments with `std::env::args_os()`, so an argument that is not
  valid Unicode no longer panics: a non-Unicode `--out` path is used as given, and a
  non-Unicode number or option is a usage error (flyingrobots/lumen#12).
