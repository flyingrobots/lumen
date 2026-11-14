# The `lumen` library

Reference for the library crate's public surface: what it exports, what each part promises and
how it fails. The complete, generated API reference is the rustdoc, built from the same source:

```sh
cargo doc --no-deps -p lumen --open
```

This page is the map to it. The crate has two targets. The library (`crates/lumen/src/lib.rs`) renders; the `lumen` command
(`crates/lumen/src/bin/lumen/`) is a front end over it.

## The crate root, by family

Every item `crates/lumen/src/lib.rs` re-exports, by what it is for; a test
(`the_library_reference_names_every_item_of_the_crate_root`) fails if one is missing here.

| Family | Items |
| --- | --- |
| Scenes | `Scene` (with `Scene::bounds`, the tree's root box, and `Scene::material_index`), `SceneSpec`, `CameraPlacement`, `Lighting`, `SceneError`, `SceneBuffer`, `SceneContentId`, `SCENE_CONTENT_ENCODING`, `PrimitiveIndex` |
| Ray queries | `Tracer`, `CountingTracer` (the same queries, also returning Bunny's `TraversalCounts`), `Hit`, `TraceError`, `FAR` |
| Built-in scenes | `BuiltinScene`, `BuiltinSceneError`, `ParseSceneError`, `reference_scene`, `REFERENCE_SCENE_ID`, `fixed_point_scene`, `FIXED_POINT_SCENE_ID`, `canyon_scene`, `CANYON_SCENE_ID`, and the modules `reference`, `scenes` and `mesh` |
| Image size and pixels | `ImageSize`, `SizeError`, `MAX_DIMENSION`, `MAX_PIXELS`, `Rgba8Image`, `LengthError` |
| Settings and the integrator | `RenderSettings`, `SettingsError`, `Integrator`, `INTEGRATOR_ID`, `SMOOTH_INTEGRATOR_ID`, `SMOOTH_SHADING_IDS`, `TONE_CURVE`, `CAMERA_PAIR`, `BOUNCE_PAIR`, `MAX_BOUNCES`, `trace_path`, `pixel_radiance`, `PathSample`, `PathError`, `RayCount` |
| Rendering | `render`, `render_scheduled`, `render_traced`, `render_traced_with_run`, `render_traced_with_traversal`, `render_reference`, `Render`, `RenderError`, `Schedule`, `BandRows`, `ScheduleError`, `DEFAULT_BAND_ROWS`, `MAX_THREADS` |
| The scheduler, for any pixel function | `render_pixels`, `render_pixels_by_worker`, `Rendered`, `WorkerRun`, `WorkerError`, `Reservation`, `SpawnError` |
| Provenance | `RenderProvenance`, `ProvenanceValue`, `PROVENANCE_FIELDS`, `BUNNY_REV` |
| Statistics | `SceneStatistics`, `TreeStatistics`, `SceneMemory`, `StatisticsError`, `LEAF_SIZE_BUCKETS`, `TREE_BUILDER`, `TREE_LAYOUT`, `RayOutcomes`, `RayStatistics`, `RayCheckError`, `RunStatistics`, `WorkerStatistics`, `TraversalStatistics`, `TraversalWork` |
| PNG | `write_png`, `png_len`, `PngError` |
| Debug views | `View`, `ParseViewError`, `render_view` (and `RenderProvenance::with_view`), `NORMALS_VIEW_ID`, `SHADING_NORMALS_VIEW_ID`, `DEPTH_VIEW_ID`, `PRIMITIVE_ID_VIEW_ID`, `MATERIAL_ID_VIEW_ID`; see [the command's debug views](cli.md#debug-views) and [ADR 0010](../adr/0010-debug-views.md) |

## The library's API is the crate root

Every type and function a front end, a `Tracer` or a
test needs is re-exported there: scene building (`Scene`, `SceneSpec`, `CameraPlacement`,
`Lighting`, `reference_scene`, `fixed_point_scene`, `canyon_scene`, and `BuiltinScene`, the registry of
named scenes with their identities that the command's `--scene` reads), rendering (`render`, `render_scheduled`, `render_traced`,
`render_traced_with_run` and `render_traced_with_traversal`, which also return what the workers
did and what Bunny's traversals did per kind of ray,
`Schedule`, `BandRows`, `RenderSettings`, `ImageSize`), the result (`Render`, `Rgba8Image` and
its `hash`), its provenance (`RenderProvenance`), a scene's statistics (`Scene::statistics`,
`SceneStatistics`, below), PNG encoding into any writer (`write_png`,
`png_len`), and the integrator's parts for custom tracers (`trace_path`, `pixel_radiance`,
`Tracer`, `CountingTracer`, `Hit`, `PrimitiveIndex`, `TraceError`, `PathError`). Only `lumen::reference` and `lumen::scenes` (the scenes'
content, as `reference` is) and `lumen::mesh` (the mesh types they use) are public modules. The SHA-256 and PNG checksum code
is internal. [ADR 0002](../adr/0002-library-api-surface.md) records the rule.

## The library touches nothing outside its arguments

It parses no arguments, opens no files,
reads no clock or environment, prints nothing and asks the host nothing: the caller passes the
thread count and the writer. It builds for `wasm32-unknown-unknown`. Threads are started only
by `render_pixels`, at the count the caller gave, and never for one thread.

## Provenance is a fact about the image; timing is a fact about the run

`RenderProvenance::named(name, &scene, size, settings)` (or `new(&scene, size, settings)` for
a scene without a name) gives every field that decides the image's bytes, and `fields()` lists
them by name in a fixed order: `scene` (the name), `integrator`, `shading`, `sampler`, `tone`,
`encoding`, `bunny`, `size`, `pixels`, `samples`, `seed`, `bounces`, `scene-content` (the
scene's content identity, below). New fields are added at the end. Two scheduled renders
(`render`, `render_scheduled`, or `render_traced` with the scene as its own tracer) with equal
provenance have equal bytes; a caller-supplied `Tracer` is not described by the provenance, so a
render through one is outside that promise. The thread count, the band height, the
output file and the wall-clock times do not change a pixel, so they are not provenance. The
command prints them from its own report (`crates/lumen/src/bin/lumen/report.rs`), after the
provenance, the ray count and the image hash.

## A scene has a content identity

`Scene::content_id()` is a `SceneContentId`, the
SHA-256 of a canonical byte encoding (`lumen-scene-content/2`) of the `SceneSpec` it was built
from: the triangles in primitive order, each one's material index, the albedos, the light
(present or absent), the sky, the camera placement and the vertex normals (none, or one entry
per triangle, each vertex's normal present or absent). It is computed once in `Scene::new`,
before the tree is built, and does not depend on the tree, the threads, the band height or the
platform; the image size, samples and seed are separate provenance fields, and a scene's name
is a label beside it, not part of it. It identifies the input, not the picture: moving a
triangle no ray reaches changes the identity and no pixel. The encoding is specified byte by
byte in [ADR 0005](../adr/0005-scene-content-identity.md), and a test rebuilds it by hand
from that table. `RenderProvenance` takes it from the scene, so provenance can no longer
describe a different scene under the same name. The built-in scenes' digests are golden
values in `crates/lumen/tests/golden/mod.rs`, beside their image hashes, and one more, for a
one-triangle scene, was computed outside lumen with Python's `hashlib`.

## A scene can be counted

`Scene::statistics()` returns a `SceneStatistics`, computed on
request from the built scene and never while a ray is traced: the triangles in all, by
material (`triangles_by_material`, one entry per albedo, summing to the total), with a vertex
normal and of zero area; the bounds (the tree's root box); the tree's shape (`TreeStatistics`,
in the terms of Bunny's uncompacted tree: nodes, interior nodes, leaves, the deepest leaf and
the sum of leaf depths with the root at depth 0, the triangles per leaf, fewest, most and how
many leaves hold 1, 2, 3, 4 or more, `LEAF_SIZE_BUCKETS`); and the bytes of each buffer
(`SceneMemory`: its length times this build's element size; not the `Scene` value, spare
capacity, the allocator's overhead or the build's dropped buffers). `TREE_BUILDER` and
`TREE_LAYOUT` name Bunny's builder and layout; `Scene::materials()` gives the albedos. The
tree's shape is read by a walk of Bunny's public compact nodes, with no Bunny arithmetic. Every
value is an integer and a function of the scene, except that the byte counts also depend on the
type sizes of the build. [ADR 0008](../adr/0008-statistics.md) records the design, including
the ray, run and traversal statistics (`RayStatistics` in every `Render`, `RunStatistics` from
`render_traced_with_run`, `TraversalStatistics` from `render_traced_with_traversal` through a
`CountingTracer`).

## The command owns everything at the edge

argument parsing and the usage text (`cli.rs`,
`cli/usage_error.rs`, with its tests in `cli/tests.rs`), the default thread count from
`available_parallelism`, the clock, the output file and the terminal (`main.rs`, `report.rs`).
Every way the command can fail is a variant of one typed `Failure` (`failure.rs`), which keeps
the library's or the system's error as its source; its `Display` and `exit_code` are the one
place that turns a failure into the message after `lumen: ` and the exit status (2 for a
refused command line, 1 for anything after it).

## Errors

Every refusal is a typed value that says what failed and carries what was observed. Each error's
`Display` includes the message of the error it wraps, so its text stands alone, and `source()`
returns that error, so a program can walk the chain; `tests/errors.rs` produces each variant
through the public API and checks its values and its chain.

- **A path** (`trace_path`, `pixel_radiance`) fails with a `PathError`: `NoRay` with the sample
  index and Bunny's camera sample for a pixel outside the camera's image; `Trace` for a failed
  ray query; and, for a hit a `Tracer` reported, `UnknownPrimitive` (the index and the scene's
  triangle count), `NoSurface` (a zero-area triangle) or `SurfaceHit` (the triangle and Bunny's
  `SecondaryRayError`). `BounceRay`, `RadianceOverflow` and `Pixel` are reached by no input,
  since Bunny's sampler and lights bound their values; a generated test over both scenes counts
  that none occurs. Lumen calls Bunny's fast `Option` forms and, only after a `None`, the `try_`
  form for the reason, so a render that succeeds pays nothing for it.
- **A render** (`render`, `render_scheduled`, `render_traced`) fails with a `RenderError`: the
  size, the scene or the camera refused, `Pixel { x, y, error }` for the first failing pixel in
  raster order with its `PathError`, `OutOfMemory { reserving, error }` with what was being
  reserved (`Reservation::Image { bytes }` or `Reservation::Workers { handles }`) and the
  allocator's `TryReserveError`, or `Spawn(SpawnError)` with the number of the worker the
  operating system refused (from 1) and its `io::Error` as the source. `render_pixels` reports
  the same through `WorkerError`.
- **A ray query** (`Tracer`) fails with a `TraceError`: `Traversal` with Bunny's
  `TraversalError`, named in lumen's text since Bunny's has none; `UnknownPrimitive` (the index
  and the triangle count) and `HitNotRepeatable` (the triangle and the distance) for a traversal
  that contradicts its tree. The scene's own tracer reaches only `Traversal`: `Scene::new`
  checks once that its tree names only its own triangles (`SceneError::TreeNamesUnknownPrimitive`
  otherwise, which no input reaches), so its traversal callbacks never meet a missing triangle,
  and a generated test over random scenes counts that neither of the other two occurs.
- **A scene** (`Scene::new`) is refused with a `SceneError` that carries what it saw: the
  triangle count and the limit (`TooManyTriangles`); the triangle, the vertex and its
  coordinates (`OutsideTraceDomain`, found by repeating Bunny's one documented check of
  `TraceTriangle::new` on the failure path, since Bunny returns no reason); both lengths
  (`MaterialCount`); the triangle, its material index and the material count
  (`UnknownMaterial`); the buffer, the item count and the allocator's error (`OutOfMemory`,
  with a `SceneBuffer`); Bunny's `CompactError` (`BvhCompact`, named in words, since Bunny's has
  no text) or the room given when `build_bvh_sah` refuses (`BvhBuild`); and the camera, mesh and
  constant errors as sources. `FixedPointError`, `CanyonError`, `BuiltinSceneError` and
  `PlaceError` return theirs from `source()` too; `HeightfieldError` names the parameter of a
  refused heightfield spec.
- **A PNG** (`write_png`) fails with a `PngError`: `Write` with the writer's `io::Error` as its
  source and its text; `ChunkTooLong` and `BlockTooLong` carry the length for the encoder's own
  length checks, which no image reaches.
- **A size** is refused with a `SizeError` carrying the width and height (or the pixel count)
  asked for and the limit; a buffer of the wrong length for `Rgba8Image::from_bytes` with a
  `LengthError` carrying both lengths. `ImageSize::byte_len` cannot fail: `ImageSize::new`
  computes and checks it once, at most `2^29`.
- **Arithmetic** on sizes, sample counts, band heights and ray counts is checked or
  saturating, each with the bound that keeps it exact stated beside it, and values reach wider
  types through `From` or `try_from`, not `as`. In the library the one `as` left on such a value
  is `widen` (`u32` to `usize`, lossless under a compile-time assertion, behind a scoped lint
  exception); the command converts the ray count to `f64` with `as` for the information-only
  `rays-per-second` and `ns-per-ray`, also behind a scoped exception, and `png.rs` builds its
  CRC table with one `as` on an index below 256.
- `RenderError`, `PathError`, `WorkerError`, `Reservation`, `SizeError`, `SceneError`,
  `SceneBuffer`, `TraceError`, `PlaceError`, `FixedPointError`, `CanyonError`,
  `HeightfieldError`, `BuiltinSceneError` and `PngError` are `#[non_exhaustive]` ([ADR 0002](../adr/0002-library-api-surface.md)).

