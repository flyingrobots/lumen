# Lumen

A deterministic ray tracer: a [Geordi](https://github.com/flyingrobots/geordi) runtime built on
[Bunny](https://github.com/flyingrobots/bunny)'s fixed-point math. The same scene renders to the same bytes on
every platform CI checks: x86-64 and aarch64 Linux, aarch64 macOS, x86-64 Windows and
WebAssembly (see [Determinism](#determinism)).

Work is tracked in GitHub Issues with `issue-dag/2` frontmatter.

## What lumen does today

Lumen renders a built-in scene, prints its image hash and timing, and writes it to a PNG file
if asked; `lumen stats` prints what a scene holds ([Statistics](#statistics)). `--scene reference` (the default) is the reference scene, which includes the Stanford
bunny; `--scene fixed-point` is [Fixed Point](#the-fixed-point-scene), a showpiece;
`--scene canyon` is [the canyon](#the-canyon-scene), a picture and a large-triangle-count
benchmark. The
reference render:

- in bands of rows on all cores (`--threads`, below), with the same bytes at every thread count;
- a power-of-two number of samples per pixel (1 to 1024, default 4), placed by Bunny's
  `bunny-sobol-owen/1` sampler from a seed;
- each sample one diffuse path (`lumen-diffuse-path/1`, below): the camera ray's nearest
  triangle, direct light from one directional light with a hard shadow, one bounce, and a uniform
  sky for rays that leave the scene;
- every step a Bunny function: the camera, the sampler, the traversals (a binned-SAH tree in
  Bunny's compact layout), the triangle test and the shading steps of
  `bunny-diffuse-shading/1` in that contract's order of operations;
- Bunny's pixel path: `PixelAccumulator` (the mean), the `bunny-tone-srgb/1` tone curve,
  `write_rgba8`.

It does not yet read scenes from files, such as a Geordi `geordi-ray-traced-scene/1` artifact
(flyingrobots/lumen#6). Materials other than diffuse, more than one
light, textures, smooth normals and more than one bounce are not supported.

## Render an image

The short way, from a checkout:

```sh
make demo            # the three built-in scenes at 1280x720, opened when done
make fixed-point     # just the Fixed Point scene
make canyon          # just the canyon scene
make 4k              # the reference scene at 3840x2160, 16 samples
make 4k-canyon       # the canyon scene at 3840x2160, 16 samples
make help            # every target and the variables you can override
```

Images land in `out/`. `make` runs the same container commands shown below, so it needs
`devrun` (set `DEVRUN=` if it is not at `~/git/devcontainers/bin/devrun`). Size, samples, seed
and the container's CPU limit are variables: `make fixed-point WIDTH=1920 HEIGHT=1080 CPUS=18`.

The long way:

Builds and runs happen in the shared containers through `devrun`, never on the host. Files
written under `/scratch/out/` in the container come back with `--export <name>` into a fresh
directory given by `--out`:

```sh
D=~/git/devcontainers/bin/devrun
python3 $D lumen ~/git/lumen --worktree --fetch --export reference.png --out "$(mktemp -d)" -- \
  bash -c 'mkdir -p /scratch/out && cargo run --locked --release -q -p lumen -- render --width 640 --height 360 --samples 64 --out /scratch/out/reference.png'
```

### Generated tests

A claim that quantifies over inputs (any split of a SHA-256 input, any image size, any order of a
pixel's samples) is tested over generated inputs from `crates/lumen/tests/generator/mod.rs`, a
seeded SplitMix64 (Lumen takes no property-testing crate). Each test fixes its seed, derives one
seed per case, prints the seed and the case seed in every failure message, keeps to a bounded
number of cases and states its oracle and run time in its comment. Replay one case with
`Gen::new(case_seed)`. A new generated test is shown to fail on a seeded mutation of the code it
checks before it merges. The tests are `tests/generated.rs`, `src/sha256.rs` and
`src/png/generated.rs`; the first also runs in the WebAssembly job, so none may assume a 64-bit
`usize`.

`--fetch` is needed on the first run in a worktree. The command prints:

```text
scene lumen-reference-scene/3
integrator lumen-diffuse-path/1
shading bunny-diffuse-shading/1
sampler bunny-sobol-owen/1
tone bunny-tone-srgb/1
encoding bunny-rgba8-opaque/1
bunny 2cd551e248d55407a8f27e84dcb26d1b1d8768bd
size 640x360
pixels 230400
samples 64
seed 0
bounces 1
scene-content sha256:c1e109477c09328421d76efefd923bd625d842fe3afb93dfadb21ae905bd0f66
threads <workers used>
threads-asked <--threads, or what the system reports available>
band-rows 1
rays <total> (closest <camera and bounce rays>, shadow <shadow rays>)
image sha256:<64 hex digits>
png /scratch/out/reference.png (922266 bytes)
scene-seconds <time> (information only)
render-seconds <time> (information only)
hash-seconds <time> (information only)
png-seconds <time> (information only)
total-seconds <time> (information only: scene, render, hash and png)
rays-per-second <rate> (information only: wall clock, all threads)
ns-per-ray <time> (information only: wall clock, all threads)
```

Usage: `lumen render [--scene reference|fixed-point|canyon] [--out FILE] [--width W] [--height H]
[--samples N] [--seed S] [--threads T] [--band-rows R]`; the defaults are the reference scene,
320 by 180, 4 samples per pixel, seed 0, bands of 1 row and as many threads as the system
reports available. Without `--out`
nothing is written and the report says `png none`: the hash and the timing are the whole
result. Width and height are each `1..=32768`
(the limit of Bunny's camera) with at most `2^27` pixels in all (a 512 MiB RGBA8 buffer, enough
for 16K at 15360 by 8640). The sample count must be a power of two from 1 to 1024: Bunny's
sampler states its stratification for aligned power-of-two blocks of samples, and its
accumulator takes at most 1024. The seed is `0..=4294967295`, the domain of Geordi's render
settings. Arguments are read as OS strings, so an output path that is not valid Unicode is
used as given; a non-Unicode number or option is refused like any other bad argument. Bad
arguments and every refused value (a size, samples, threads, band rows, a scene) exit with
status 2 and a message, with the usage text for those the parser refuses; an unwritable output
path or a failed render exits with status 1 and a message. The output file is published
atomically (see below), so `--out` names either the complete new PNG or what it was before,
whether the render or the write fails. `--threads` is `1..=256` (0 and anything larger are refused with
status 2); `--band-rows` is the number of whole image rows a worker takes at a time, at least 1
(0 is refused with status 2). There is no `--tile` option any more; it is refused as unknown.

### The output file

`--out FILE` is checked before the render starts: a destination that cannot be written (its
directory is missing or not writable, it is a directory or a link to one, it is a read-only file,
it has no file name) is refused with status 1 and a message that names the path, without computing a
pixel and without leaving anything behind.

The PNG is then written to a temporary file in the destination's directory, `.NAME.lumen-PID-N.tmp`
(created exclusively, so it never replaces another file and two runs never share one), flushed,
`sync_all`ed and renamed over the destination. If any step fails the temporary file is removed and
the destination is exactly as it was: a failed write (a full disk, a size limit) no longer leaves a
truncated file or destroys the one that was there. A temporary file that cannot be removed is
named in the error.

- Durability: on Unix the directory is synced after the rename, so the new name survives a crash;
  if that sync fails the error says the file was written but its directory could not be synced (the
  destination holds the new image). On Windows the standard library cannot sync a directory and
  none is attempted; the replacement itself is a single rename.
- An existing regular file is replaced by a new one: it gets the default mode for new files, not
  the old file's mode, owner or hard links. A read-only file is refused rather than replaced.
- A symbolic link is replaced itself; the file it pointed to is not written (this differs from
  writing through the link). A link to a directory is refused like a directory; a dangling link is
  replaced.
- A device, pipe or socket (`--out /dev/null`) is written in place, without a temporary file or
  a sync, since replacing it would destroy the node.
- The rename needs write permission on the directory, not on the old file.

Option values. Each value is the next argument or, for any option, part of the same argument
after an equals sign (`--out=FILE`, `--width=96`); only the first `=` splits, so `--out=a=b` names
`a=b`. A separate value that starts with `-`, has more after it and is not a negative number is
read as an option and refused with status 2 (`--out --width` is an error, not a file called
`--width`), the rule of `clap` and `argparse`; to use such a value, write `--out=--width`. A lone
`-` and a negative number such as `-3` are values (the latter is then refused as a number where a
number is needed). `--` is not an option: lumen has no positional arguments for it to separate.
The `=` form needs an argument that is valid Unicode; a non-Unicode path is passed as a separate
argument. Numbers are decimal digits with no sign, space or underscore; leading zeros are
accepted for every numeric option and change nothing (`--width 000096` is 96, never octal, and
the report prints `96`), because a refusal would only reject a value whose meaning is clear.
The report's `png` line gives the number of bytes written to the file, counted as the file
accepted them; the command checks that it equals the PNG's length, and fails if it does not.

The PNG is uncompressed (see below), so it is about four bytes per pixel: 922 266 bytes at
640 by 360, and 33 188 430 bytes (about 33 MB) at 3840 by 2160.

## Library and command

The crate has two targets. The library (`crates/lumen/src/lib.rs`) renders; the `lumen` command
(`crates/lumen/src/bin/lumen/`) is a front end over it.

- **The library's API is the crate root.** Every type and function a front end, a `Tracer` or a
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
  is internal. [ADR 0002](docs/adr/0002-library-api-surface.md) records the rule.
- **The library touches nothing outside its arguments.** It parses no arguments, opens no files,
  reads no clock or environment, prints nothing and asks the host nothing: the caller passes the
  thread count and the writer. It builds for `wasm32-unknown-unknown`. Threads are started only
  by `render_pixels`, at the count the caller gave, and never for one thread.
- **Provenance is a fact about the image; timing is a fact about the run.**
  `RenderProvenance::named(name, &scene, size, settings)` (or `new(&scene, size, settings)` for
  a scene without a name) gives every field that decides the image's bytes, and `fields()` lists
  them by name in a fixed order: `scene` (the name), `integrator`, `shading`, `sampler`, `tone`,
  `encoding`, `bunny`, `size`, `pixels`, `samples`, `seed`, `bounces`, `scene-content` (the
  scene's content identity, below). New fields are added at the end. Equal provenance means equal bytes. The thread count, the band height, the
  output file and the wall-clock times do not change a pixel, so they are not provenance. The
  command prints them from its own report (`crates/lumen/src/bin/lumen/report.rs`), after the
  provenance, the ray count and the image hash.
- **A scene has a content identity.** `Scene::content_id()` is a `SceneContentId`, the
  SHA-256 of a canonical byte encoding (`lumen-scene-content/2`) of the `SceneSpec` it was built
  from: the triangles in primitive order, each one's material index, the albedos, the light
  (present or absent), the sky, the camera placement and the vertex normals (none, or one entry
  per triangle, each vertex's normal present or absent). It is computed once in `Scene::new`,
  before the tree is built, and does not depend on the tree, the threads, the band height or the
  platform; the image size, samples and seed are separate provenance fields, and a scene's name
  is a label beside it, not part of it. It identifies the input, not the picture: moving a
  triangle no ray reaches changes the identity and no pixel. The encoding is specified byte by
  byte in [ADR 0005](docs/adr/0005-scene-content-identity.md), and a test rebuilds it by hand
  from that table. `RenderProvenance` takes it from the scene, so provenance can no longer
  describe a different scene under the same name. The built-in scenes' digests are golden
  values in `crates/lumen/tests/golden/mod.rs`, beside their image hashes, and one more, for a
  one-triangle scene, was computed outside lumen with Python's `hashlib`.
- **A scene can be counted.** `Scene::statistics()` returns a `SceneStatistics`, computed on
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
  type sizes of the build. [ADR 0008](docs/adr/0008-statistics.md) records the design, including
  the ray, run and traversal statistics that are not yet built.
- **The command owns everything at the edge**: argument parsing and the usage text (`cli.rs`,
  `cli/usage_error.rs`, with its tests in `cli/tests.rs`), the default thread count from
  `available_parallelism`, the clock, the output file and the terminal (`main.rs`, `report.rs`).
  Every way the command can fail is a variant of one typed `Failure` (`failure.rs`), which keeps
  the library's or the system's error as its source; its `Display` and `exit_code` are the one
  place that turns a failure into the message after `lumen: ` and the exit status (2 for a
  refused command line, 1 for anything after it).

### Errors

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
  types through `From` or `try_from`, not `as`; the one `as` left on such a value is `widen`
  (`u32` to `usize`, lossless under a compile-time assertion, behind a scoped lint exception).
- `RenderError`, `PathError`, `WorkerError`, `Reservation`, `SizeError`, `SceneError`,
  `SceneBuffer`, `TraceError`, `PlaceError`, `FixedPointError`, `CanyonError`,
  `HeightfieldError`, `BuiltinSceneError` and `PngError` are `#[non_exhaustive]` ([ADR 0002](docs/adr/0002-library-api-surface.md)).

## Statistics

`lumen stats [--scene NAME]` builds a scene and prints what it holds, without rendering:

```sh
python3 $D lumen ~/git/lumen --worktree -- cargo run --locked -q -p lumen -- stats --scene reference
```

The library computes the counts (`Scene::statistics()`, a `SceneStatistics`) and the command
formats them; nothing is counted while a ray is traced, and nothing in the scene changes.
`Scene::statistics` is described under [Library and command](#library-and-command);
[ADR 0008](docs/adr/0008-statistics.md) records what is counted and why it cannot change a pixel.
`render --stats` also reports what Bunny's traversals did for each kind of ray (box tests,
primitive tests and the rest of Bunny's `TraversalCounts`), through Bunny's counted searches;
without `--stats` the render calls the uncounted ones.

`lumen render ... --stats` prints the report unchanged, then the same scene block (every line of
`lumen stats` after its first three, which the report already gives), then what the rays found,
then what the traversals did, then how the work was spread over the workers.
Without `--stats` the output is the report alone, byte for byte as before statistics existed.
The command first checks the ray statistics against the size and settings (one record per pixel,
`--samples` samples per pixel, exactly one camera ray per sample, at most four rays per sample)
and fails with exit code 1 if they disagree, which no render the library produced does.

Each line is a key with no spaces, one space and a value: one token, or a vector of tokens
separated by single spaces. Keys are stable; new ones are added, never renamed. Fixed-point values
are printed in decimal with six places from their exact raw integers, rounded to nearest with ties
away from zero. Every value is a function of the scene alone, except the `memory.` byte counts,
which also depend on this build's type sizes (the values below are those of 64-bit targets).

| Key | Value |
| --- | --- |
| `scene`, `scene-content`, `integrator` | the scene's name, its content identity and its integrator, as in the render report |
| `scene.triangles` | every triangle, zero-area ones included |
| `scene.vertices` | three per triangle: lumen's triangles hold their own vertices and share none |
| `scene.triangles-smooth` | triangles with at least one vertex normal, which `lumen-smooth-diffuse-path/1` shades smooth |
| `scene.triangles-zero-area` | triangles of zero area: kept, never hit |
| `scene.materials` | the albedos in the material table, used or not |
| `scene.material.N.triangles` | the triangles naming material `N`; they sum to `scene.triangles` |
| `scene.material.N.albedo` | material `N`'s red, green and blue reflectance |
| `scene.bounds-min`, `scene.bounds-max` | the corners of the box around every triangle (the tree's root box) |
| `light.sun` | `present` or `none`; when present, `light.sun-toward` (the direction to the sun) and `light.sun-irradiance` (per channel) follow |
| `light.sky` | the sky's radiance per channel |
| `camera.position`, `camera.look`, `camera.up`, `camera.tan-half-fov` | the camera placement as given |
| `tree.builder`, `tree.layout` | `bunny_broadphase::build_bvh_sah` and `bunny_broadphase::compact_bvh`, at the Bunny commit of `lumen --version` |
| `tree.nodes` | every node of Bunny's tree, interior and leaf: `2 * leaves - 1`, at most `2 * triangles - 1` |
| `tree.interior-nodes`, `tree.leaves` | the two kinds; the compact layout stores one node per interior node and keeps leaves in their parent |
| `tree.leaf-depth-max` | the deepest leaf, the root being at depth 0; at most 60 (Bunny's `SAH_MAX_DEPTH`) |
| `tree.leaf-depth-total` | the sum of every leaf's depth |
| `tree.leaf-depth-mean` | `leaf-depth-total / leaves`, with two places, rounded to nearest, ties up |
| `tree.leaf-triangles` | the triangles of every leaf, summed: each triangle is in one leaf, so this is `scene.triangles` |
| `tree.leaf-triangles-min`, `tree.leaf-triangles-max` | the fewest and most triangles in a leaf |
| `tree.leaf-sizes` | five counts: leaves of 1, 2, 3 and 4 triangles, and of more (only at the depth cap) |
| `memory.NAME-bytes` | information only, a fact about this build (the line ends in `(information only: this build's type sizes)`): each buffer's length times its element size, which depends on the target's pointer width: `triangles` (Bunny's `TraceTriangle`), `surfaces` (`Option<TriangleSurface>`), `vertex-normals`, `material-indices`, `materials`, `tree-nodes` (compact nodes), `tree-primitive-indices`, and their `total` |

The ray lines (`render --stats`), each a function of the scene, the size and the settings, the
same at every thread count and band height:

| Key | Value |
| --- | --- |
| `rays.camera`, `rays.camera-hits`, `rays.camera-misses` | camera rays traced, and those that hit a triangle or left the scene: exactly one per sample |
| `rays.bounce`, `rays.bounce-hits`, `rays.bounce-misses` | bounce rays traced, one at each camera hit unless rejected, and their hits and misses |
| `rays.bounce-rejected` | bounces Bunny's smooth shading rejected as below the surface: no ray traced, so not in `rays.bounce` |
| `rays.shadow`, `rays.shadow-occluded`, `rays.shadow-unoccluded` | shadow rays traced at camera and bounce hits, and those that met a triangle (the sun blocked) or nothing |
| `rays.total` | every ray: the `rays` line's total |
| `rays.pixels`, `rays.samples-per-pixel`, `rays.samples` | pixels rendered, samples asked for per pixel, and samples traced in all |
| `rays.per-pixel-min`, `rays.per-pixel-max` | the fewest and most rays any one pixel traced |
| `rays.per-pixel-mean` | `rays.total / rays.pixels`, with two places, rounded to nearest, ties up |

The traversal lines (`render --stats`), after the ray lines: for each kind of ray, `KIND` being
`camera`, `bounce` (closest-hit searches) and `shadow` (any-hit searches, which stop at the first
hit), the counts of Bunny's `TraversalCounts` for every ray of that kind, from
`render_traced_with_traversal`. Each is a sum or a maximum over rays, so it is the same at every
thread count and band height; the image, the report and the ray lines are the same as without
counting.

| Key | Value |
| --- | --- |
| `traversal.KIND.COUNTER` | the counter summed over the kind's rays. `COUNTER` is `box-tests` (boxes tested: the root's, and both children's at every interior visit), `box-entries` (tests the ray passed, so the child was pushed), `popped-skipped` (entries popped and dropped because a nearer hit had been found since they were pushed), `interior-visits`, `leaf-visits` or `primitive-tests` (triangle tests) |
| `traversal.KIND.COUNTER-mean` | the sum over `rays.KIND`, with two places, rounded to nearest, ties up |
| `traversal.KIND.COUNTER-max` | the most any one ray of the kind took (each counter's may be a different ray's) |
| `traversal.KIND.peak-stack` | the most entries any one search's stack held at once (Bunny's limit is 64) |

The run lines (`render --stats`), last. The band height, the band count and the worker count follow
from the size and `--threads` and `--band-rows`; which worker took which band depends on timing, so
every per-worker line ends in `(information only: differs from run to run)`. The per-worker
figures add up to the totals, which do not differ. Workers report counts only: there is no
per-worker busy time, since the library reads no clock (ADR 0008); `render-seconds` is the whole
render's.

| Key | Value |
| --- | --- |
| `run.band-rows`, `run.bands` | the rows in a band and the bands the image is cut into (the height over the band height, rounded up) |
| `run.workers` | the workers that rendered, the `threads` line's count |
| `run.worker.N.bands`, `.rows`, `.pixels`, `.rays` | what worker `N` (0 is the calling thread) took and traced: information only |

Not counted in `memory.`: the `Scene` value and its vectors' headers, spare capacity, the
allocator's overhead, the buffers dropped after the build (the boxes and the uncompacted tree of
`2 * triangles` nodes) and what a render allocates. The output is machine-readable as it is
(`awk '$1 == "tree.nodes" { print $2 }'`); there is no JSON form. For the reference scene:

```text
scene.triangles 31511
scene.material.3.triangles 27648
scene.material.4.triangles 3851
scene.bounds-min -16.000000 -1.125000 -16.000000
scene.bounds-max 16.000000 2.000000 16.000000
tree.nodes 33095
tree.leaves 16548
tree.leaf-depth-max 20
tree.leaf-depth-mean 16.36
tree.leaf-sizes 2778 12708 931 131 0
memory.total-bytes 11432728 (information only: this build's type sizes)
```

## Debug views

`lumen render --view NAME` draws a debug view of the scene instead of the lit image, through the
same threads and bands, so a view is the same bytes at every thread count and band height:

```sh
python3 $D lumen ~/git/lumen --worktree -- cargo run --locked --release -q -p lumen -- \
  render --scene fixed-point --view shading-normals --width 640 --height 360 --out /scratch/out/n.png
```

| View | Identity | Pixel |
| --- | --- | --- |
| `lit` (default) | the scene's integrator | the lit image, exactly as without `--view` |
| `normals` | `lumen-view-normals/1` | the first hit's geometric normal `(x, y, z)` as `128 + round(127 * (x, y, z))` on a front face and `128 + round(63 * (x, y, z))` on a back face (dimmer and greyer); black where the ray hits nothing |
| `shading-normals` | `lumen-view-shading-normals/1` | the same map of the interpolated vertex normal (Bunny's `smooth_normal`) on a triangle with all three vertex normals, and of the geometric normal elsewhere; magenta where the vertex normals cancel |

A debug view traces one ray through each pixel's centre, so it takes `--samples 1` only (its
default) and does not depend on `--seed`. `--stats` describes the lit image's paths and is refused
with a debug view. The report's `integrator` line is the view's identity, `shading` the Bunny
contracts it uses (`none` for `normals`), and `samples 1` and `bounces 0`. Every other line is
as for the lit image. `make views` renders both views of every built-in scene into `out/`. The
library function is `render_view`, and `RenderProvenance::with_view` gives the provenance.
[ADR 0009](docs/adr/0009-debug-views.md) records the design and the views not built yet
(`depth`, the IDs, `wireframe`, `bvh-cost`, `gray-box` and the lighting terms).

## The reference scene

`lumen-reference-scene/3` is built in `crates/lumen/src/reference/`, entirely in Q32.32 integers
with no floating point and no randomness from the host. Four of its objects are defined in code;
the fifth, the Stanford bunny, is read from a mesh file embedded in the binary
(`assets/stanford-bunny/bun_zipper_res3.ply`, see [below](#the-stanford-bunny)). It is made of
triangles only, 31 511 of them, so that a Geordi `geordi-ray-traced-scene/1` artifact can describe
exactly the same scene (flyingrobots/lumen#6). In primitive order, with each object's diffuse
albedo (linear, red, green, blue, in 256ths):

1. the ground: two triangles at `y = -1.125`, spanning `-16..=16` on X and Z; albedo
   `(150, 145, 135)`, a warm light grey;
2. a back wall: two triangles at `z = -4`, `x = -3..=3`, `y = -1.125..=2`, facing the camera;
   albedo `(175, 165, 145)`, pale stone;
3. an octahedron of radius 1 centred at `(2.25, -0.125, 0.5)`, standing on a vertex; albedo
   `(50, 95, 200)`, blue;
4. a bumpy sphere around the origin, radius 1.0 to 1.04: a cube subdivided 48 by 48 per face and
   pushed out with Bunny's `FixedVec3::normalize`, adapted from Bunny's scene benchmark
   (27 648 triangles); albedo `(205, 60, 45)`, red;
5. the Stanford bunny: 3 851 triangles (every face of the file, none dropped), in the file's
   face order, standing on the ground left of the sphere and facing it, about 1.24 units long,
   1.21 tall and 0.96 deep; albedo `(70, 165, 85)`, green.

The bunny's triangles come after all the others, so primitives 0 to 27 659 are those of version
2 in the same order, and the bunny is 27 660 to 31 510.

The lighting is one directional light and one uniform sky, the shape of Geordi's scene profile
(at most one directional light, exactly one sky):

- the sun: toward `(0.5, 0.3125, 1.0)` (from the right, from the camera's side, about 15 degrees
  above the horizon; its largest component is exactly 1.0, as Bunny's `DirectionalLight`
  requires), irradiance `(1024, 970, 900) / 256`, about `(4.0, 3.8, 3.5)`. Bunny's convention: a
  light's value is the irradiance on a surface perpendicular to it, and a diffuse surface of
  albedo `a` lit at cosine `c` reflects `a * E * c / pi`;
- the sky: radiance `(64, 85, 120) / 256`, about `(0.25, 0.33, 0.47)`, a pale blue.

The sphere, the octahedron and the bunny cast long shadows back and to the left across the
ground, and the sphere's shadow climbs the foot of the wall.

The camera stands at `(-1, 2, 5)` looking along `(0.1875, -0.375, -1)` with up `+Y` and a
vertical half-angle whose tangent is 0.5. Versions 1 and 2 stood at `(0, 1, 5)` looking along
`(0, -0.25, -1)`; from there the bunny stood exactly in front of its own shadow, which the sun
throws along `(-0.5, -1)` in X and Z, so version 3 moves the camera one unit left and one up.
Every object and shadow of version 2 stays in the picture.

Every vertex coordinate is a multiple of `2^-16` below 256 units, so it is exactly an `f32` and
survives Geordi's `bunny-ply-binary-le-f32/1` mesh layout unchanged; the sphere's and the bunny's
vertices are floored to that grid.

The primitive order matters: of two triangles hit at the same distance, the lower index wins.
**Any change to the scene's triangles, their order, their materials, the lighting or the camera
placement is a new scene version, and changes every image hash.** Version 2
(`lumen-reference-scene/2`: the first four objects, the same materials and lighting, and the
camera at `(0, 1, 5)`) is retired. Version 1 (`lumen-reference-scene/1`: the same triangles and
camera with no materials or lights, shaded by the normal-colour debugging view
`lumen-normal-shading/1`) is retired too. Neither the old versions nor the normal view is kept: they
would have needed their own render paths, flags, golden hashes and tests to stay honest, and the
current render shows the same geometry.

### The Stanford bunny

The mesh is the Stanford Bunny from the Stanford 3D Scanning Repository, Stanford Computer
Graphics Laboratory (`https://graphics.stanford.edu/pub/3Dscanrep/bunny.tar.gz`,
`bunny/reconstruction/bun_zipper_res3.ply`). Lumen's copy is Geordi's checked-in asset, byte for
byte, with the same SHA-256 (`975e7f9b160b4ea15b0e225e21b10828ebcf678df020d2f6a46aa408fdcf5cd6`,
checked by a test with lumen's own `sha256` module). It is Stanford's data, used with attribution
as Geordi uses it; lumen's Apache-2.0 license does not cover it. `assets/stanford-bunny/README.md`
records the provenance and Stanford's terms, and Stanford's reconstruction README is kept beside
the file. `.gitattributes` keeps Git from rewriting its line endings.

**Reading it: Bunny's ASCII PLY reader.** `crates/lumen/src/mesh/loaded.rs` passes the embedded
bytes to `bunny_codec::parse_ascii_ply` (profile `bunny-ascii-ply-triangle-mesh/1`) and gathers
its vertices and faces into the vectors that placement reads. A refusal is Bunny's
`AsciiPlyError` with its kind and 1-based line number, carried unchanged in
`PlaceError::Ply` (and so in `SceneError::Mesh` and `FixedPointError::Bunny`). The decimal
coordinates are converted to Q32.32 by Bunny's rule `bunny-decimal-q32-32/1`: the exact value
times `2^32`, rounded to nearest with ties to even. Bunny's own tests cover the conversion's
ties, range edges and refusals and the reader's refusals; see its `docs/topics/ascii-ply/`.

Bunny's reader is looser than the interim reader that this replaced (`lumen-ascii-ply-q32.32/1`,
deleted): it accepts CRLF line endings, tabs and runs of spaces, blank lines, `comment` and
`obj_info` header lines, any other scalar vertex properties (skipped by declaration, not
checked as numbers) and `float` or `double` coordinates, and it sets no limit on the counts or
the line length (it reserves no memory from a declared count). The embedded file is fixed and its
bytes are checked, so none of this changes what Lumen renders: the converted asset (1 889 vertices,
3 851 faces) is pinned by a SHA-256 over every raw coordinate and index in order, recorded from
the interim reader, which also agreed with Bunny's reader on every vertex and face when the
interim reader was deleted. The file has one coordinate in exponent form, `8.20736e-05`, and
the manifest's bounds, converted by Bunny, equal the converted mesh's bounds exactly.

**Placing it** (`BUNNY_PLACEMENT` in `crates/lumen/src/reference/bunny.rs`, applied by
`crates/lumen/src/mesh/placement.rs`): every vertex is turned half a turn
about `+Y` (`x` and `z` negated, exact; the file's bunny faces `-X`), scaled by 8 (a shift of
three bits, exact), floored to the `2^-16` grid like the sphere, and moved by whole grid steps:
`(-2.375, -1.392303466796875, 0.5)`. The Y offset puts the lowest vertex (file vertex 1 748,
`y = 0.0334143`) exactly on the ground plane, so the bunny stands on the ground rather than a hair
above it; the touching surfaces are handled by Bunny's start-point rule. Placed, the bunny spans
`x = -2.8625..=-1.6201`, `y = -1.125..=0.0862` and `z = 0.0323..=0.9934`. A face that has zero area
once placed would be dropped and listed (`PlacedMesh::dropped`) so that no degenerate triangle
reaches Bunny's `TriangleSurface`; for this file and placement none is (the smallest face keeps
about `2.2e-5` square units), and a test asserts the count.

**What the mesh is like.** Its winding is consistent: every edge shared by exactly two faces is
traversed in opposite directions by them, and the faces wind counter-clockwise seen from outside
(the signed volume is positive). It is not a closed surface: as Stanford's README warns, the
decimation does not preserve topology. 60 edges belong to one face only (four holes, three in
the base, which rests on the ground, and a small one low on the front), 141 edges are shared by
three or more faces, and 83 faces repeat another face's three vertices. Lumen shades two-sided, so
none of this affects correctness; a repeated face is a coplanar tie that the lower primitive index
wins. In the 640 by 360 reference picture the holes do not show: the base faces the ground and the
small front hole is out of the camera's view, and no light leaks where the bunny meets the
ground.

**The full-resolution mesh.** `assets/stanford-bunny/bun_zipper.ply` is the zippered
reconstruction the decimated mesh was made from: 35 947 vertices and 69 451 faces, taken byte for
byte from Stanford's archive (retrieved 2026-11-14, archive SHA-256 checked before extracting;
file SHA-256 `b1acc63bece78444aa2e15bdcc72371a201279b98c6f5d4b74c993d02f0566fe`, 3 033 195
bytes, its trailing spaces kept). It is embedded as
`scenes::fixed_point::full_bunny::FULL_BUNNY_PLY` for Fixed Point version 2
(flyingrobots/lumen#39); no scene uses it yet, and the linker leaves it out of any program that
does not. Read by Bunny's reader, it is manifold and consistently wound, with no duplicate vertex
or face; its 223 boundary edges are five holes in its base, and 1 113 vertices are named by no
face. `assets/stanford-bunny/README.md` records its provenance, its measured topology and what it
costs in binary and repository size.

The scene's types (`SceneSpec`: triangles, a material index per triangle, diffuse albedos, an
optional directional light and a sky; `RenderSettings`: a power-of-two sample count and a `u32`
seed; at most one bounce) follow Geordi's ray-traced scene profile, so that loading an artifact
(flyingrobots/lumen#6) maps onto them. `SceneSpec` also carries optional vertex normals
(`normals`: empty, or three `Option<FixedUnitVec3>` per triangle, the input of Bunny's
`SmoothHit::new`), which `Scene::new` checks for count (`SceneError::NormalCount`) and keeps
(`Scene::vertex_normals`); they enter the content identity, and a scene with any of them is
shaded smooth ([below](#smooth-shading-lumen-smooth-diffuse-path1)).

## The Fixed Point scene

`lumen-fixed-point-scene/2` (`crates/lumen/src/scenes/fixed_point/`) is a showpiece: the
full-resolution Stanford bunny, smooth-shaded, in dark sunglasses at a craps table, with two identical dice and the ON puck on the 8. In
craps the point is the number the shooter must roll again, and the ON puck marks it. Both dice
show four pips on every face, so every roll is a hard eight and the point is fixed: a picture of
determinism, from a ray tracer built on fixed-point arithmetic.

Render it with the command (`--scene fixed-point`):

```sh
python3 $D lumen ~/git/lumen --worktree --fetch --export fixed-point.png --out "$(mktemp -d)" -- \
  bash -c 'mkdir -p /scratch/out && cargo run --locked --release -q -p lumen -- render --scene fixed-point --width 1280 --height 720 --samples 64 --out /scratch/out/fixed-point.png'
```

The library builds it with `BuiltinScene::FixedPoint.build()` (or `lumen::fixed_point_scene()`);
`fixed_point_content()` returns its
triangles with the part each belongs to (`PartKind`: felt, rail, numeral 8, left die's pips,
left lens and so on) and its material. `BuiltinScene::ALL` lists the scenes, `name()` is the
text `--scene` accepts and `id()` the identity that `lumen render` prints on its `scene` line.
`BuiltinScene::from_str` rejects any other name with a typed error that lists the valid ones;
the command turns it into a usage error with exit code 2. The scene keeps its own camera and lighting. The example program
(`crates/lumen/examples/fixed_point.rs`) renders the same scene through the library alone:

```sh
python3 $D lumen ~/git/lumen --worktree --fetch --export fixed-point.png --out "$(mktemp -d)" -- \
  bash -c 'mkdir -p /scratch/out && cargo run --locked --release -q -p lumen --example fixed_point -- /scratch/out/fixed-point.png 1280 720 64'
```

Arguments: `OUT [WIDTH HEIGHT [SAMPLES [SEED [THREADS]]]]`, defaulting to 480 by 270, 16
samples per pixel, seed 0 and the thread count the host reports (the example asks; the library
does not). The image does not depend on the thread count. It prints the scene identity, the
triangle count, the settings, the rays traced, the image hash and the render time. The 1280 by
720 render at 64 samples through the command has the image hash
`sha256:86cdfd58d7745297b52f63f443cedd789a773996c66ad57d45b5d8a6f67fb153`. Version 1 (the
decimated, flat bunny and the black-rimmed puck) is retired: its 1280 by 720 hash was
`sha256:e638f32442d90d5fb5bc8212044856433b1f4998a5d17ca549370747313d253c`.

**What it is made of.** Triangles only, 71 575 of them, every coordinate a whole number of
`2^-16` grid steps; a die is about half a unit wide. In primitive order:

1. the table: the green felt (`x = -7.5..=7.5`, `z = -12..=3.25`, at `y = 0`, 2 triangles); a
   padded rail swept round it with mitred corners (0.375 high at the felt, 0.75 at its crown,
   1.125 wide, 56 triangles); white layout lines 1/16 wide, painted `2^-10` above the felt (the
   number boxes' outlines and two long lines, 22 triangles); and a row of six number boxes, each
   1.75 wide and 3 deep, for 4, 5, 6, 8, 9 and 10, each with a seven-segment block numeral
   (cells 0.6875 wide and 1.375 deep, strokes 0.1875, 10 in two cells) in the box's near half,
   reading upright from the camera;
2. the ON puck in the far half of the 8 box, clear of the 8: a 24-sided disc of radius 0.5 and
   height 1/8 (a quarter of its radius, as a casino puck's), a white face of radius 15/32 inside
   a black rim 1/32 wide, a white side, and "ON" in bold black block letters (strokes 3/32,
   0.5625 by 0.4375 in all), so that from the camera's low view it reads as a white disc with
   black lettering (134 triangles);
3. the two dice, each a chamfered cube about 0.5 wide (32 800 grid steps, chamfer 2 625) with
   four raised white 12-sided pips on every face (908 triangles each): the same mesh under two
   rigid motions, the left die turned by `atan(3/4)` (about 36.9 degrees) and the right one by
   `-atan(7/24)` (about -16.3 degrees) about the vertical. Every coordinate of the die's mesh
   is a multiple of 25 grid steps, so both turns are exact: nothing is rounded. Raised pips
   mean each die rests on the four pips of its bottom face;
4. the full-resolution Stanford bunny (`bun_zipper.ply`, 69 451 triangles, none dropped):
   turned a quarter so it faces the camera, scaled by 32 (about 4.9 units tall, nearly ten
   dice), floored to the grid with its lowest vertex exactly on the felt, then turned by
   `-atan(5/12)` (about 22.6 degrees, toward the camera's right) about its nose, each
   coordinate floored to the grid, so it stands three-quarters on behind the number boxes with
   its tall ear and its back in profile. Each vertex is placed and posed once and the faces keep
   their indices, so its smooth-shading normals are Bunny's `VertexNormals` (area-weighted,
   `bunny-vertex-normals/1`) over the positions that are traced: computed after placement, they
   need no transform and agree with the floored geometry. No face gets zero area and every
   triangle has a normal at all three vertices; the 1 113 vertices no face names get none and
   are referred to by no triangle. The holes in its base face the felt and do not show;
5. the sunglasses (22 triangles): two flat wraparound wayfarer lenses, a bridge and two arms in
   near black. They are fitted by measurement: the nose tip is the vertex farthest along the
   facing direction in the upper half of the bunny's height, the lenses' centre line is 0.5
   above it, their inner edges 1/32 in front of it, and each lens slopes back by a quarter of
   its distance from the centre line; the arms run back along the head, the left one outside
   the base of the swept-back ear, the right one, refitted to the full mesh, just clear of the
   side of the head. The glasses are built facing the camera and then turned with the bunny.

Only the bunny is smooth: the dice, the table, the puck and the glasses keep their facets, so
the scene is rendered by `lumen-smooth-diffuse-path/1` (see
[Smooth shading](#smooth-shading-lumen-smooth-diffuse-path1)).

Materials (linear albedo in 256ths): felt `(14, 62, 32)`, leather `(45, 22, 14)`, layout paint
`(225, 225, 215)`, die red `(170, 12, 16)`, pips and the puck's face and side
`(240, 240, 232)`, puck rim and letters `(10, 10, 10)`, the bunny's light warm grey `(205, 200, 192)` and the glasses
`(6, 6, 8)`. The sun comes from high and nearly behind the camera, toward `(-0.25, 1.0, 1.0)`
(about 44 degrees up, 14 degrees left of the camera), irradiance `(1024, 985, 920) / 256`, so it
lights the bunny's face and chest straight on and the dice and the bunny cast their shadows back
across the felt (version 1's sun, from the side, put the smooth bunny's jagged shadow edges
across its cheek and chest); the sky is a
dim `(16, 16, 18) / 256`, a dark room behind the table. The camera stands low over the near rail
at `(0, 2.125, 4.25)`, looks along `(0, -0.140625, -1)` with up `+Y` and a vertical half-angle
whose tangent is `15/32`, framed for 16:9: the ear tip near the top, the puck and the 8 in the
middle, the dice near the bottom.

Lumen renders diffuse surfaces with one sun and a sky, so the lenses are plain black. Mirror
lenses, a glass of whisky on the rail and a lamp over the table wait for the materials and
lights they need. Shadowed fur is tinted green: that is the one-bounce light from the felt.
Where a shadow falls across the smooth bunny (under the ear, down the back), its edge
follows the mesh's triangles: shadow rays leave the geometric surface, so a facet tilted away
from the sun is in shadow even where its interpolated normal faces the sun. That is the shadow
terminator of interpolated normals on a mesh, and Bunny's `bunny-smooth-shading/1` keeps it by
design (its shadow ray is the geometric one). At 4K the edges show as small steps, a few pixels
each, under the ear and down the back where the ear's shadow falls; the frontal sun keeps them
off the face and chest.

**How it is tested** (`crates/lumen/tests/fixed_point/`, through the public API):

- every face of both dice has exactly four pips, counted from the triangles alone (top
  triangles at the pip height along each face axis, grouped by the centre vertex of each
  pip's fan), in the four-pip pattern;
- the right die is the left die under the motion the two poses state, vertex for vertex in
  exact integers, and the two are congruent independently of the poses (equal squared distances
  between corresponding vertices, equal orientations, so neither is a mirror image);
- each numeral lights exactly the segments of its digits in a seven-segment truth table written
  out in the test, inside its box; the puck lies inside the 8 box, clear of the 8, on the felt;
- each lens's centre has the face behind it within 0.25 units along the bunny's facing direction
  (Bunny's exact ray test), no lens vertex is inside the head, and no edge of the glasses or of
  the bunny crosses a triangle of the other (an exact integer test written in the test);
- the triangle counts of every part; every triangle in Bunny's trace domain and none degenerate;
  the dice, the puck and the bunny resting exactly on the felt (a tolerance of zero grid steps);
  every bunny triangle with a vertex normal at all three corners and no other part with any;
- the puck as the camera sees it: its face and side white and its rim and letters black, at
  least three times as many pixels of a 480 by 270 image showing white puck as black, and the
  pixels wholly inside each colour rendering bright and dark (means 230 and 53 when recorded);
- every camera, shadow and bounce ray of a 32 by 18 image at 2 samples equals a test of every
  triangle, with hits on the bunny, the dice and the layout;
- a golden hash: 96 by 54 pixels, 4 samples per pixel, seed 0,
  `sha256:da6cb05db0d366a2088f19e6eefed2ab017dd0d27c6c5e364a85121054d6093e`, under the same
  rules as the reference scene's: it changes only with an identity, the settings or the Bunny
  commit. Any change to the scene's triangles, their order, materials, lighting or camera is a
  new scene version and a new hash. Every platform in [Determinism](#determinism) checks it.

## The canyon scene

`lumen-canyon-scene/1` (`crates/lumen/src/scenes/canyon/`) is a large-triangle-count scene and a
picture: a fractal canyon of integer terrain, three small craft of lumen's own design flying
along it in a chase, the full-resolution Stanford bunny on a ledge, a low sun, a near-black sky
and a banded planet. Composition after a 1990s canyon-chase render; no image from it is in the
repository. The library builds it with `lumen::canyon_scene()`; `canyon_content()` returns its
566 005 triangles in primitive order (the terrain, each craft's hull and engines, the bunny, the
planet's bands) with the part each belongs to, its material and its vertex normals. Every
coordinate is a whole number of `2^-16` grid steps.

Render it with the command (`--scene canyon`):

```sh
python3 $D lumen ~/git/lumen --worktree --fetch --export canyon.png --out "$(mktemp -d)" -- \
  bash -c 'mkdir -p /scratch/out && cargo run --locked --release -q -p lumen -- render --scene canyon --width 1280 --height 720 --samples 64 --out /scratch/out/canyon.png'
```

The 1280 by 720 render at 64 samples has the image hash
`sha256:1bd910dcea9c3daaa3ea57e3972a639202aa341ba59e0e075a2e55184dc56446` (158 564 967 rays).
`BuiltinScene::Canyon` is the registry entry; its content identity is
`sha256:a809761e5cbc96944721e8e2095ae9e0eaa76d8f3ac8bf25e536446835eaf88d`.

**The terrain** (480 256 triangles) is a grid of 513 by 470 vertices, an eighth of a unit apart
across the canyon (X, from -40 to 24) and three eighths along it (Z, from 16 to -159.875), so
that a wall's facets are about as tall as they are long; two triangles per cell, wound up, none
of zero area (each projects onto half a cell), every coordinate a whole number of `2^-16` grid
steps. The spec (`terrain::CANYON_HEIGHTFIELD`, seed 2009): a floor 14 units across, walls of
four tiers each rising 3 units over one unit with a half-unit tread, a ragged plateau about 12
units up, eight noise layers (lattice spacings 128 to 1, amplitudes from 1.5 units, each three
quarters of the one before, the finest two halved again) weighted a quarter on the floor, twice
on the walls and one and a half times on the plateau, and a centre line that bends about 9 units
to the left between 30 and 60 units from `z = 12`. One shelf (`LEDGE`) cuts a level ledge 3
units up into the left wall near the canyon's mouth. Each cell is coloured by its rock stratum:
its four corners' mean height in bands 1.5 units thick picks material 0, 1, 0, 2 in turn (`STRATA`).

**The craft** (170 triangles, `scenes::canyon::craft`), shapes of lumen's own, each a painted
hull and dark engine blocks built from convex solids (`convex`: corners and faces, each face a
fan wound away from the solid's centre), drawn nose toward `-Z` in 64ths of 3 units and posed
by rational rotations (a bank about `+Z`, then a heading about `+Y`, each coordinate floored to
the grid): the skiff (a hexagonal hull with a pointed nose, straight swept wings and a fin,
orange) high at `(-2.5, 7, -10)`, banked by `-atan(7/24)` and turned by `atan(7/24)`; the tug (a
wedge tapering to a narrow nose between two pontoons joined by a short wing, cream) at
`(-7.5, 5.5, -15)`, banked and turned by `-atan(7/24)`; and the kite (a flat diamond wing with
two engine pods, teal) leading low at `(-5.5, 4, -24)`, banked by `atan(7/24)`. `push_craft`
appends them to a `CanyonContent` in that order, each its hull then its engines.

**The planet** (16 128 triangles, `scenes::canyon::planet`): a sphere of radius 22 centred at
`(-44, 48, -200)`, 128 longitudes by 64 latitude rows, its points from Bunny's table-based
`FixedAngle::sin_cos` (each within one raw unit) floored to the grid, its axis tilted a
thirty-second of a turn toward `-X`, in eight latitude bands (8, 6, 10, 6, 12, 6, 8 and 8 rows
from the south pole), each its own material. `push_planet` appends the bands.

**The bunny** (69 451 triangles, `scenes::canyon::bunny`): the full-resolution Stanford bunny
(`bun_zipper.ply`, none dropped), turned half a turn to face across the canyon (`+X`), scaled by
16 (about 2.4 units tall), floored to the grid and moved by whole grid steps so that its lowest
vertex is exactly on the ledge (3 units up) and the middle of its footprint is at column 220,
row 39 of the terrain grid (`x = -12.5`, `z = 1.375`). Its smooth-shading normals are Bunny's
`VertexNormals` over the placed positions, as in Fixed Point.

**The look** (`scenes::canyon::look`). Only the bunny is smooth, so the scene is rendered by
`lumen-smooth-diffuse-path/1` (ADR 0007): the terrain keeps its facets, which suit rock, cost a
flat hit each, and avoid the shadow-terminator steps of interpolated normals
(flyingrobots/bunny#340) that a smooth heightfield would show on every wall. Materials (linear
albedo in 256ths): rock `(150, 96, 64)`, `(178, 128, 86)` and `(118, 74, 52)`; skiff
`(225, 105, 25)`, kite `(30, 150, 165)`, tug `(215, 205, 180)`, engines `(36, 36, 40)`, bunny
`(205, 200, 192)`; planet bands from the south `(200, 170, 125)`, `(150, 95, 60)`,
`(225, 205, 165)`, `(175, 110, 70)`, `(235, 220, 185)`, `(120, 80, 55)`, `(205, 185, 150)` and
`(140, 160, 185)`. The sun comes from low behind the camera and a little to its right, toward
`(0.1875, 0.375, 1.0)` (about 20 degrees up), irradiance `(1152, 1075, 950) / 256`: the right
wall throws a long shadow across the floor, the craft throw theirs on it and the bunny's ears
theirs on the wall behind it. The sky is a near-black `(2, 2, 3) / 256`. The camera stands over
the floor at `(-4.5, 4.25, 12)` and looks along `(-0.03125, 0.1, -1)` (its Y is 6 553 grid
steps), up `+Y`, with a vertical half-angle whose tangent is 0.5.

`CanyonContent` (`scenes::canyon::content`) holds triangles in primitive order with each one's
material index, vertex normals and named part (`PartKind`: the terrain, a craft's hull or
engines, the bunny, a planet band).

Tests (`crates/lumen/tests/canyon/parts.rs`): every craft part's and band's triangle count,
derived by hand; every triangle inside the trace domain and none degenerate; one material per
part, eight distinct for the bands, no vertex normals; and every craft at least half a unit above
every terrain triangle under it (each triangle's vertices below the craft's lowest point, so the
whole triangle is), the three craft's boxes apart.

Tests of the assembled scene (`crates/lumen/tests/canyon/integrity.rs`, the content and the
scene built once per test binary): the parts in order covering all 566 005 primitives, 16
materials; every triangle inside the trace domain and none degenerate; only the bunny smooth,
so the integrator is the smooth one; and the bunny resting on its ledge to the grid step (its
lowest vertex at the ledge's height, no terrain vertex under it above that, and the cell under
its lowest vertex level at that height on all four corners). Building the scene takes about 0.7
seconds in the release profile and about 3 in the test profile.

Rendering tests (`crates/lumen/tests/canyon/rendering.rs`): the composition as pixels (of a 320
by 180 image, the camera ray through each pixel's centre meets the bunny in at least 900 pixels,
each craft in at least 40 and the planet in at least 800); every camera, shadow and bounce ray
of a 48 by 27 image at 1 sample (1 296 samples) equal to a test of all 566 005 triangles, a
large test that runs in the release profile only (about 12 seconds; the test profile skips it
with `#[cfg_attr(debug_assertions, ignore = ...)]`, and CI runs both profiles); and a golden
hash: 96 by 54 pixels, 4 samples per pixel, seed 0,
`sha256:019fcdddaaff8761f3f6e64dfe28d1ce7d2b07200b8c69663345ca4a495466c5`, under the same rules
as the other scenes', the same at every thread count and band height (`tests/threads.rs`). The
canyon's test binary is not in the WebAssembly job's list; that job checks the canyon's content
identity and a small render through `tests/library.rs`.

At 3840 by 2160 and 16 samples it renders in about 14.5 seconds with 18 CPUs, 2.7 times the
reference scene's time per ray: [the measurement record](docs/measurements/2026-11-14-canyon.md)
has the runs, the tree and what was not measured.

**Known limits of version 1.** The tug reads as a box from behind; the near right wall is a dark
mass in shadow, lit only by the bounce; the far right wall shows vertical fluting where the low
sun rakes a steep heightfield; and the ledge's back is a cut cliff, flat where the shelf sliced
the wall. A change for any of these is a new scene version.

Tests (`crates/lumen/tests/canyon/terrain.rs`): two triangles per cell in row order on the grid
points of their corners, inside Bunny's trace domain, none degenerate, each facing up (an exact
integer cross product); every cell's material from the stratum rule written out in the test,
each of the three used. The generator's bound and channel tests also run on the scene's spec
(the floor at the centre line at least 2.5 units below every plateau vertex of its row).

## Canyon terrain: `lumen-canyon-heightfield/1`

`lumen::scenes::canyon::heightfield` generates the canyon scene's terrain heights in integers only:
`generate(&HeightfieldSpec)` returns one height, in whole `2^-16` grid steps, for every vertex
of a grid of `columns` by `rows` (column `i` across the canyon, row `j` along it). A height is a
pure function of the spec and `(i, j)`: the same spec gives the same heights, bit for bit, on
every platform. The algorithm, specified exactly in the module's documentation so that it can be
reimplemented, is:

1. **Lattice hash**: the seed, a layer tag and a lattice point mixed with three odd constants,
   then the SplitMix64 output function; its top 16 bits scaled to the layer's amplitude.
2. **Noise**: value noise, one layer per `NoiseLayer` (lattice spacing in vertices, amplitude in
   grid steps), each the bilinear blend of its four lattice values, floored, summed.
3. **Centre line**: a one-dimensional noise layer (the meander) moves the canyon's centre from
   row to row, carried in 256ths of a column so that a wall stays smooth where the line bends
   by less than a column per row.
4. **Carve**: by distance from the centre line, a floor, walls of `Terraces` (each tier a steep
   riser and a level tread) and a plateau.
5. **Height**: the carve's profile plus the noise, weighted by zone (`NoiseWeights`, in
   quarters): little on the floor, most on the walls.
6. **Shelves**: each `Shelf` cuts the rock in its rectangle down to its height, leaving a level
   ledge whose edges stay rough where the rock was already lower.

`HeightfieldSpec::bounds` states the range every height lies in; `generate` refuses a spec
outside its stated ranges (`HeightfieldError`, naming the parameter) before computing anything,
and those ranges keep every product inside `i64`. Any change to the steps, their constants or
their order is a new version of the generator.

Tests (`crates/lumen/tests/canyon/heightfield.rs`): exact values on tiny grids, from SplitMix64's
published first output and from an independent Python model of the documented algorithm (a 3 by
3 grid, a hand-worked profile, and the sum, weighted sum, extremes and two vertices of a 64 by 96
canyon with every step in use); determinism; every height within the stated bounds; the channel
(the floor at the centre line below every plateau vertex of its row by a stated margin); a wall
smooth to within its slope where the centre line bends; the shelf's rule; and each refusal.
Seeded mutations of the noise blend, the carve's floor width, the centre line's fraction and the
shelf's rule each fail at least one of them.

## The integrator

`lumen-diffuse-path/1` (`crates/lumen/src/render.rs`, `trace_path`) is a composition of named
Bunny contracts with no arithmetic of its own. For sample `i` of pixel `(x, y)`, with
`sampler = PixelSampler::new(seed, x, y)` (`bunny-sobol-owen/1`), it follows the order of
operations of `bunny-diffuse-shading/1` (Bunny's `docs/topics/optics/README.md`, "Order Of
Operations") step for step:

1. camera ray: `ray0 = camera.ray(sampler.camera_sample(i))`, sampler pair 0;
2. primary: `closest(ray0)`; a miss is `sky.radiance()`, and the sample is done;
3. `h1 = SurfaceHit::new(surface[p1], ray0, hit1)`: the frame facing the ray and the start point
   of every secondary ray;
4. direct light `D1`: `s1 = h1.light_sample(sun)`; `D1 = s1.radiance(visible)`, where visible
   means the light sample has a shadow ray and an any-hit query of it with `SHADOW_T_MAX`
   finds nothing;
5. bounce: `ray1 = h1.bounce_ray(sampler.point(i, 1))`, sampler pair 1; `closest(ray1)`: a miss
   gives `B = sky.radiance()`; a hit gives `h2`, its direct light `D2` as in step 4 and
   `B = albedo[p2].reflect(D2, 0)`;
6. `L = albedo[p1].reflect(D1, B)`.

The queries are Bunny's `compact_closest_hit` and `compact_any_hit` with `TraceRay::hit_triangle`
as the primitive test: a minimum distance of zero for every ray and no primitive excluded,
`i64::MAX` raw as the limit for camera and bounce rays and `SHADOW_T_MAX` (512 units) for shadow
rays. Secondary rays start exactly at Bunny's start point (`SurfaceHit::point`), which is strictly
on the arrival side of the triangle's exact plane, so they never hit the triangle they leave and
need no distance epsilon. Every `TriangleSurface` is built once per triangle when the scene is
built.

A pixel's value is the mean of its samples in a Bunny `PixelAccumulator` (exact integer sums,
rounded once), toned by `bunny-tone-srgb/1` and written by `write_rgba8`
(`bunny-rgba8-opaque/1`). Each sample is a pure function of (scene, pixel, sample index, seed):
the per-pixel function holds no shared mutable state, so rendering bands on threads (below) is a
change of schedule only.

Geordi's integrator identity is meant to pin this same composition.

### Smooth shading: `lumen-smooth-diffuse-path/1`

A scene whose `SceneSpec::normals` gives at least one triangle a vertex normal is rendered by
`lumen-smooth-diffuse-path/1` (`SMOOTH_INTEGRATOR_ID`), and its provenance says so: the
`integrator` field is that name (`Integrator::for_scene`) and the `shading` field is
`bunny-diffuse-shading/1+bunny-smooth-shading/1+bunny-smooth-normal/1` (`SMOOTH_SHADING_IDS`, the
Bunny contracts its paths use, in the order they apply). Any other scene, the two built-in ones
included, keeps `lumen-diffuse-path/1`, `bunny-diffuse-shading/1` and its bytes. The smooth integrator is the same composition with one
change, made per triangle: at a hit on a triangle with a vertex normal, steps 3 to 5 use Bunny's
`bunny-smooth-shading/1`:

- `h = SmoothHit::new(surface_hit, hit.barycentric_numerators(), normals)`, which interpolates
  the three vertex normals at the exact hit point (`smooth_normal`, `bunny-smooth-normal/1`) and
  turns the shading frame to the arrival side, or keeps the geometric frame when a normal is
  missing or the interpolation is along the surface;
- `D = h.light_sample(sun)`: the same shadow ray and side test, with the cosine taken against
  the shading normal;
- `h.bounce(sampler.point(i, 1))`: a ray around the shading normal, or, when that direction is
  at or below the geometric surface, no ray at all and `B = 0`. Bunny does this on purpose (a
  ray along or under the surface would hit its own neighbourhood), and it darkens grazing,
  strongly tilted shading normals slightly; the ray count then has one closest-hit query fewer.

A triangle without a vertex normal in the same scene is shaded flat, exactly as by
`lumen-diffuse-path/1`. Lumen adds no arithmetic. [ADR 0007](docs/adr/0007-smooth-shading.md)
records why the integrator's name follows the scene and why the normals are stored per triangle.
`tests/smooth.rs` checks the choice of integrator, that the reference scene given its own face
normals at every vertex renders its flat bytes, a hand-derived value (a pyramid whose normals all
point at a sun straight above reflects `0.5 * FRAC_1_PI` rounded to even, raw 683 565 276, at every hit), the rejected bounce
(no ray, zero radiance), and every sample of a scene that mixes a smooth octahedron and a flat
floor against Bunny's documented steps composed in the test over brute-force queries.

### Traversal

Bunny offers two tree builders (`build_bvh`, `build_bvh_sah`) and two layouts (`BvhNode` with
`closest_hit`/`any_hit`, and `compact_bvh` with `compact_closest_hit`/`compact_any_hit`), all
giving the same answers. Lumen uses the binned-SAH tree in the compact layout, the fastest of the
four on a small render of the reference scene. The scene module keeps that choice in one file:
`crates/lumen/src/scene/mod.rs` holds the scene model (`Scene`, `SceneSpec`, `Lighting`,
`CameraPlacement`, `SceneError`), `scene/tracer.rs` the ray-query port (`Tracer`, `Hit`,
`TraceError`, `FAR`, and `CountingTracer`, the same two queries also returning Bunny's
`TraversalCounts` for the search that answered them), `scene/primitive_index.rs` the triangle's
name (`PrimitiveIndex`, its position in input order, which breaks ties and so is part of the
scene), and `scene/tree.rs` the adapter that builds Bunny's tree and answers both ports' queries
with it: `Tracer` with Bunny's `compact_closest_hit` and `compact_any_hit`, `CountingTracer` with
their counted forms, which return the same answers (`tests/traversal_counts.rs` checks the counts
by hand on constructed trees and against Bunny's counted searches called directly).
`crates/lumen/examples/traversal.rs` repeats the measurement, checks that all four give the same radiance bits, and prints the build times:

```sh
python3 $D lumen ~/git/lumen --worktree --exclusive -- \
  cargo run --locked --release -q -p lumen --example traversal -- 160 90 4 7
```

A test checks that the chosen structure returns the same hit as `closest_hit` (and the same
answer as `any_hit`) on a plain `build_bvh` tree for every camera ray of a small image.

One reading of that command (Apple M5 Pro, the `rust-dev:1.96.0` container, run alone with
`--exclusive`, release profile; the median of 7 rounds over 150 726 rays, every ray including its
share of sampling and shading), for information:

| Tree and layout | ns per ray | Build |
| --- | ---: | ---: |
| `build_bvh`, `closest_hit` / `any_hit` | 243.7 | 99.7 ms |
| `build_bvh_sah`, `closest_hit` / `any_hit` | 225.9 | 17.2 ms |
| `build_bvh`, compact layout | 205.5 | 99.7 ms |
| `build_bvh_sah`, compact layout (lumen) | 192.5 | 17.2 ms |

The table was measured on `lumen-reference-scene/2`. The 640 by 360 render at 64 samples per
pixel, in the same setting (the median of three runs of `lumen render`, one thread, all work
included):

| Scene | Triangles | Rays | Time | ns per ray |
| --- | ---: | ---: | ---: | ---: |
| `lumen-reference-scene/2` | 27 660 | 38 588 953 | 7.18 s | 186.1 |
| `lumen-reference-scene/3` | 31 511 | 39 478 487 | 7.90 s | 200.1 |

Version 3 traces 2.3 % more rays (the moved camera sees more ground and less sky) and each ray
costs about 7.5 % more, about 10 % more time in all. The scene has 14 % more triangles, and the
bunny's are small and dense; this measurement does not separate the bunny's cost from the
camera's.

## Threads and row bands

`render_scheduled` (and `lumen render`) cut the image into bands of whole rows, one row by
default (`--band-rows`; `crates/lumen/src/schedule.rs`), from the top; the last band is cut to
the image. The design, what is shared, the stop rule's proof and what was rejected are recorded
in [ADR 0003](docs/adr/0003-row-bands.md), which supersedes the tile design of
[ADR 0001](docs/adr/0001-tile-scheduling.md). The bands are `chunks_mut` slices of the one image
buffer: contiguous and disjoint, which the borrow checker proves, so a worker that takes a band
owns it and writes each pixel straight into the image. There is no per-worker buffer, no copy and
no lock on the image. One small mutex around the band iterator hands the next band to whichever
worker asks, so a region that costs more (the sphere, a shadow edge) or a slower core never
leaves the others idle. That matters on the Apple M5 Pro, whose 6 "super" and 12 "performance"
cores run at different speeds: a static split would make every worker wait for the slowest share.
Each worker sums its own ray count and keeps its own first failure and returns both when it
ends; the caller adds the counts and keeps the earliest failure after `join`
(`crates/lumen/src/workers.rs`, `render_pixels`, which runs any pixel function this way and knows
nothing of rays). The only value a worker writes that another reads is one atomic, the first row
no longer wanted. The code is safe Rust with the standard library only (`std::thread::scope`,
one mutex, one atomic); there is no `unsafe` and no thread-pool dependency.

- **Threads**: `--threads N`, `1..=256`; by default the number `std::thread::available_parallelism`
  reports, which inside a container is the CPU limit (`--cpus` of `devrun`). Only the `lumen`
  binary asks the host; the library's `Schedule::new` takes an explicit count. The calling thread is
  one of the workers, and a render never uses more workers than there are bands. With one thread
  no thread is started and the calling thread takes every band in order: the same code path with
  one worker, not a separate renderer. `lumen::render` is that one-thread schedule.
- **The scene is shared read-only.** It is built once (triangles, per-triangle surfaces, the SAH
  tree and its compact layout) before any worker starts, and every worker reads it through
  `&Scene`. A compile-time assertion in `render.rs` checks that `Scene`, and so everything
  reachable from it, is `Sync` as it is, with no lock or wrapper. Building it is timed separately
  (`scene-seconds`) from rendering (`render-seconds`).
- **Errors**: if a pixel fails (a traversal error, a ray outside Bunny's domain), the render
  returns `RenderError::Pixel` with the coordinates and the `PathError` of the first failing
  pixel in raster order, top row first: the error a
  one-pixel-at-a-time render in raster order meets, at every thread count and band height. When
  a pixel in row `y` fails, no worker begins a row from `y` down: every pixel before it is in an
  earlier row, or earlier in row `y`, which only the failing worker renders. No worker waits on
  another. A panic in the pixel function halts the other workers: each begins no row after it has
  seen the halt (a row already begun finishes; when it sees the halt depends on the scheduler, so
  this is not a time bound), and the panic reaches the caller once every worker has stopped, with
  no image.
  A scene is checked whole before any of this: `Scene::new` refuses bad triangles, material
  indices and a camera placement Bunny's camera refuses (`SceneError::Camera`), so a render of a
  built scene can be refused only for its size.
- **Progress** is not reported: there is nothing on stderr during a render.

### Why the image does not depend on the threads

Every pixel's samples are accumulated for that pixel alone, by the one worker that renders its
band, in a `PixelAccumulator` of its own, then resolved, toned and quantized; no sum ever crosses
pixels or threads, so there is no accumulation order between threads to get wrong. The worker
writes the finished bytes into its band at the position of the pixel's column, and the band is
the slice of the image that starts at its first row. The ray counts are sums of integers and the
chosen failure is a minimum, neither of which depends on order. Thread count, band height, which
worker takes which band and when, and timing therefore decide nothing about the bytes.

Tested in the test and release profiles, with no clock, no dependence on the host's core count
and no repeat-and-hope loop. Hand-out orders are explored deterministically: unit tests in
`crates/lumen/src/workers/tests.rs` run 1, 2, 3, 7 and 200 simulated workers on one thread through
the same take-a-band, render-a-row and combine steps the threads run, in orders chosen by 200
seeds of a generator and by two scripted worst cases (one worker takes everything; bands finish
in reverse order, so the top failing band is rendered last), on a 13 by 9 image with bands of 1,
2, 3, 4, 5, 9 and 20 rows (2, 4 and 5 leave a partial last band), each failure naming its order.
The image must equal a raster-order loop byte for byte, the pixel count must be exact, the error
must be the raster-first failure (also where column-major order would choose another), and no
pixel at or below the failing row may be computed after the failure; a step budget ends a
simulation that does not finish. The simulated workers run the threads' own `Worker::step`, under
`catch_unwind`, so a panic in the pixel function unwinds through the worker as on a thread: on a
64 by 512 image with one-row bands and a panic at `(0, 0)`, no pixel is computed after the panic
and the panic reaches the caller, under scripted and seeded orders; the pixels computed are
exactly the panicking one plus 64 for each whole row rendered before it (1 when the panicking
worker moves first, 6 401 after 100 rows of another, 32 705 when it moves last). `crates/lumen/tests/threads.rs` then runs real threads at fixed
counts:

- the golden 96 by 54 hash at 1, 2, 3, 4, 7 and 18 threads, each with bands of 1, 2, 4, 5, 6, 7,
  16, 27, 54 and 200 rows, with the same ray count as a raster-order loop over `pixel_radiance`
  that does not use the scheduler;
- a 320 by 180 image at 8 samples: one thread equals that raster-order loop byte for byte, and 18
  threads with one-row bands, 5 threads with bands of 7 and 16 threads with bands of 32 give the
  same image and ray count;
- errors, each test counting the work taken (pixels asked for, traversal queries) and failing
  past the whole image, never on a timeout. That limit catches a scheduler that repeats work or
  does not end; it does not check the stop rule, which a scheduler that never stops early would
  still pass. Only the simulation checks the stop rule. An injected pixel function that fails at chosen
  pixels returns the raster-first one at every thread count and band height, also when it is not
  the first in column-major order (`(40, 0)` before `(3, 5)`); on one thread a failure asks for
  exactly the pixels a raster-order loop asks for, up to and including the failing one, at every
  band height (on several threads, how much runs before the failure is recorded depends on
  scheduling, so the simulator checks the stop rule instead); and a tracer that fails real
  traversals in two regions with two different errors returns the one a raster-order loop over
  `trace_path` meets first, at every schedule.
- a refused worker thread: helper threads are started through a spawner the unit tests can
  replace, and one that refuses worker `n` (after starting workers 1 to `n - 1` on real threads)
  makes the render return `Spawn` for worker `n` with the operating system's error, once the
  started workers are joined. That the started workers begin no row after the refusal is checked
  in the simulator, through the spawn sequence the threads run (`start_helpers`) with a simulated
  start that refuses helper `n` after the others have run 0, 1, 3 or 10 steps each.
- a panic: a pixel function that panics at `(0, 0)` of a 64 by 512 image reaches the caller with
  its own payload, and no image, at 1, 2 and 4 threads. How much the other threads compute first
  depends on the operating system's scheduling, so it is not bounded here; the simulation bounds
  it.

Each of these mutations makes tests fail in the release profile (checked by hand, each mutant's
test run under a 120-second `timeout` as a guard, sources touched before every build so that no
stale build is tested): numbering a band's rows from one past its first row (pixels
written at the wrong row offset); dropping the last partial band; choosing the first failure by
`(x, y)` instead of `(y, x)`; dropping the calling worker's ray count; ignoring the stop row after a failure;
removing the halt a panicking worker's guard calls; removing the halt on a refused spawn; and the
guard halting on every drop, not only while unwinding.

## Performance

Timings are information, never part of an image, and never taken from a run that shared the
machine: every timing below ran in the `rust-dev:1.96.0` container through `devrun` with
`--exclusive`, release profile. The Docker VM on the Apple M5 Pro used here has 18 CPUs (the
host has 6 "super" and 12 "performance" cores); `devrun` limits a run to 4 CPUs unless told
otherwise, so a threaded timing must raise the limit. `--cpus` is a CPU-time quota over all 18 VM
CPUs, not a choice of cores: a run cannot be pinned to the fast cores.

### Reproduce

```sh
D=~/git/devcontainers/bin/devrun
# the 4K reference render on all 18 VM CPUs; prints every part of the time and the hash
python3 $D lumen ~/git/lumen --commit HEAD --fetch --exclusive --cpus 18 --memory 12g -- \
  bash -c 'cargo build --release --locked -q && mkdir -p /scratch/out && \
    /target/release/lumen render --width 3840 --height 2160 --samples 16 --out /scratch/out/4k.png'
# thread scaling on a mid-size image
python3 $D lumen ~/git/lumen --commit HEAD --exclusive --cpus 18 --memory 12g -- \
  bash -c 'cargo build --release --locked -q && for t in 1 2 4 6 8 12 18; do \
    /target/release/lumen render --width 1280 --height 720 --samples 16 --threads $t | grep -E "^(threads|render-seconds)"; done'
```

Add `--export 4k.png --out "$(mktemp -d)"` to take the PNG out of the container. The interval to
compare with a wall-clock target such as flyingrobots/lumen#5's 60 seconds is `total-seconds`:
the whole command after argument parsing, that is scene build, render, hashing and writing the
PNG; the report shows each part. `rays-per-second` and `ns-per-ray` divide the render's wall time
by the rays traced on all threads, so they are not CPU time per ray; the single-thread figure is a
run with `--threads 1`.

### Records

Dated measurement records are kept under [`docs/measurements/`](docs/measurements/). The record for
flyingrobots/lumen#5 is [the 4K reference scene at 16 samples per pixel, 2026-11-13](docs/measurements/2026-11-13-4k-reference.md),
taken with the tile scheduler. [The 4K reference scene with row bands and whole-program release
builds](docs/measurements/2026-11-13-4k-row-bands.md) repeats it for flyingrobots/lumen#28.
[Fixed Point version 2 at 4K and 64 samples per pixel, 2026-11-14](docs/measurements/2026-11-14-4k-fixed-point-v2.md)
records the showpiece for flyingrobots/lumen#39: the same image hash,
`sha256:d276602407edb72109ebfbc8cec9121230171cf156144f139fb2bc3ca1c13bef`, on every run, a mean
of 21.2 seconds with 18 CPUs, about 10 % more than version 1 (19.2 seconds).
[The canyon scene at 4K and 16 samples per pixel, 2026-11-14](docs/measurements/2026-11-14-canyon.md)
records flyingrobots/lumen#40: 566 005 triangles, a tree of 575 687 nodes, the image hash
`sha256:cbd020b9949ae7310b11fb21650e9336ca2e6ec99e0f2bcd950ff61d39469eab` on every run, a mean
render of 14.5 seconds with 18 CPUs, and 40.7 ns per ray against 15.0 for the reference scene
and 18.0 for Fixed Point at the same settings in the same session.
Each states its commit and conditions and applies to that commit only.
[Traversal work per ray in the three built-in scenes, 2026-11-14](docs/measurements/2026-11-14-traversal-cost.md)
gives, from `render --stats`, the mean box tests and primitive tests per camera, bounce and
shadow ray beside each scene's triangles and tree depth.

### Readings

Ray statistics cost nothing measurable: the outcomes `render --stats` reports are always
collected, and the render with them took 7.037 s at one thread and 0.547 s at 18 against 6.994 s
and 0.544 s on `main` (1280 by 720, 16 samples, medians of three interleaved runs), the same gap
as between two binaries running identical render code ([ADR 0008](docs/adr/0008-statistics.md)
has the runs).

Row bands with whole-program release builds (`lto = "fat"`, `codegen-units = 1`) against the
tile scheduler on `main` before flyingrobots/lumen#28 (0de5fe7, default release profile), in one
session, interleaved, six runs each (`lumen-reference-scene/3`, 1280 by 720, 16 samples per
pixel, seed 0; render seconds, median; the same image hash in every run):

| Threads | main, tiles | row bands, LTO | Difference |
| ---: | ---: | ---: | ---: |
| 1 | 8.483 | 7.503 | -11.6 % |
| 4 | 2.270 | 1.918 | -15.5 % |
| 18 | 0.661 | 0.623 | -5.8 % |

The gain is fat LTO (Bunny's kernels inline into the render loop), not the change of work unit:
row bands without LTO measured the same as tiles. That was also the cause of the 8 %
multi-thread slowdown after the scheduler was split out in 6a816ef. The sessions, the band-height
sweep and what was ruled out are in [ADR 0003](docs/adr/0003-row-bands.md). Across sessions the 18
thread gain ranged from 6 to 16 %; runs at 18 threads spread the most. The Fixed Point scene at
3840 by 2160 and 64 samples, also interleaved, took 20.734 s against 23.075 s (median of three,
18 threads, -10.1 %), with the same hash.

An earlier session, with the tile scheduler and the default release profile
(`lumen-reference-scene/3`, 1280 by 720, 16 samples per pixel, seed 0, 39 479 111 rays, 32 by 32
tiles; three runs each, render seconds), shows how the render scales with threads:

| Threads | Runs | Mean | Speedup | Per thread |
| ---: | --- | ---: | ---: | ---: |
| 1 | 8.164, 8.130, 8.170 | 8.155 | 1.00 | 1.00 |
| 2 | 4.118, 4.144, 4.066 | 4.109 | 1.98 | 0.99 |
| 4 | 2.064, 2.085, 2.103 | 2.084 | 3.91 | 0.98 |
| 6 | 1.543, 1.472, 1.476 | 1.497 | 5.45 | 0.91 |
| 8 | 1.168, 1.171, 1.177 | 1.172 | 6.96 | 0.87 |
| 12 | 0.853, 0.857, 0.840 | 0.850 | 9.59 | 0.80 |
| 18 | 0.627, 0.624, 0.627 | 0.626 | 13.03 | 0.72 |

Scaling is near linear to 4 threads and falls off after 6. The likeliest reason, not proven
here, is the two core classes: if the VM's busiest threads land on the 6 super cores first, the
threads beyond 6 run on the slower performance cores. Shared memory bandwidth may add to it; the
scene and its tree are small (31 511 triangles) and stay in cache, and the image is written once.

Band heights at 18 threads, with LTO, same image (render seconds, median of five): 1 row: 0.622;
2: 0.598; 4: 0.646; 8: 0.635. Without LTO, in another session: 1: 0.662; 2: 0.667; 4: 0.698;
8: 0.716; 16: 0.746; 45: 1.067. Up to 8 rows the differences are within the noise. From 16 rows
the last bands leave workers idle at the end of the render: a 16-row band is about 190 ms of
one core, and 45 rows make 16 bands for 18 workers. The default is one row: as fast as any, and
the finest grain (2 160 bands at 4K). In the earlier tile session, tiles from 4 by 4 to
64 by 64 and single rows measured the same (0.637 to 0.661 s at 18 threads), and 128 by 128 and
larger were slower for the same reason (0.832 and 1.761 s at 128 and 256).

## Image hash and PNG

The image hash is `sha256:` followed by the lowercase hex SHA-256 of the raw RGBA8 bytes
(row-major, row 0 at the top, alpha 255), as Geordi's ray-traced scene profile specifies. Width
and height are not part of it. The PNG file is only a container for those bytes; its own hash is
not a contract value.

**PNG encoder: hand-written, stored deflate blocks** (`crates/lumen/src/png.rs`). It writes
8-bit RGBA with filter type 0 on every row and the zlib stream as uncompressed stored blocks of
at most 65 535 bytes, one `IDAT` chunk each, every block but the last full. A unit test reads
the block headers at exactly 65 535 filtered bytes (1 by 13 107, 64 by 255, 5 461 by 3: one full
block) and at the next sizes up (a full block and the rest); no image has exactly 65 536, since
`height * (1 + 4 * width)` has an odd factor above 1. Reasons:

- the contract is the hash of the raw bytes, so compression buys only file size;
- it needs no dependency and about 200 lines with its documentation, CRC-32 and Adler-32 included;
- its output bytes are a fixed function of the pixels, with no compression level or library
  version that could change them;
- it streams, holding one block rather than a second copy of the image.

The cost is size: about four bytes per pixel. A compressing encoder crate would make smaller
files but bring a dependency tree whose output bytes can change between versions. If file size
starts to matter, compression can be added later without touching the image hash.

**SHA-256: hand-written** (`crates/lumen/src/sha256.rs`), for the same reasons: no dependency,
short, and checked against the published FIPS 180-2 vectors and digests from Python's `hashlib`
around the padding boundaries.

## Determinism

The claim: the same scene, size, sample count and seed give the same RGBA bytes, and so the same
image hash, on every run, in the test and the release profile, at every thread count and band
height, whatever order the pixels and samples are computed in, on each of these platforms and no
others: x86-64 Linux, aarch64 Linux, aarch64 macOS, x86-64 Windows (MSVC) and WebAssembly
(`wasm32-wasip1`, one worker thread). Other targets are expected to agree, since nothing from the
host enters the render, but they are not checked. Every value that decides a pixel is integer arithmetic from Bunny: the
sampler (a pure function of seed, pixel, sample and pair), camera rays, the trees and their
traversals, the exact triangle test, the shading steps and their order, the accumulator's exact
sums, the tone curve and quantization. There is no floating point in the render and nothing from
the host enters it.

How it is tested (`crates/lumen/tests/`):

- **Hand-derived radiance**: in constructed scenes, exact bits derived from the documented
  formulas with Bunny's own arithmetic: a ground lit straight from above (one tie to even in the
  final product), the same with the sun at 45 degrees (the cosine is carried), the same under an
  occluder (zero), a camera that sees only sky (the sky), a white surface under a sky with no sun
  (the sky, exactly), and a ceiling `2^-12` units above the ground (zero: no epsilon lets the sun
  through). Each holds for one sample and for several.
- **Shadows against exact geometry**: for an occluding triangle above the ground and the sun
  straight down, the set of ground pixels whose sample is shadowed equals the set predicted by an
  exact integer computation of the occluder's projection, with every sample at least `2^-22`
  units from the shadow's edge; a pixel in full shadow under a zero sky is exactly zero from 1 to
  1024 samples.
- **Against brute force**: for every sample of a 32 by 18 reference image, every traced ray
  (camera, shadow and bounce) gets the answer of a test of every triangle, including rays that
  hit the bunny; and every sample of a
  16 by 9 image equals an independent composition of the documented order of operations written
  in the test with brute-force queries. The camera uses sampler pair 0 and the bounce pair 1.
- **Order independence**: a pixel's samples accumulated forwards, backwards, shuffled and in
  merged partial accumulators give the same bits; an image whose pixels are computed in a
  scrambled order has the same hash.
- **Counts, against something other than the code that counts** (`crates/lumen/tests/counts.rs`):
  at 64 and 1024 samples a pixel equals a `PixelAccumulator` filled by that many direct
  `trace_path` calls, with the sum of their ray counts (and the mean of the first 32 samples
  differs, so a loop that stopped early would show); a tracer that wraps the scene and counts
  the queries it is asked, by kind (a closest-hit query from the camera's eye point is a camera
  ray), agrees with `Render::rays` for small renders of both built-in scenes at 1, 2 and 4
  threads, up to 1024 samples, and with `PathSample::rays` for every path of a 16 by 9 image;
  camera rays are exactly `width * height * samples` and the total lies between that and four
  times that; a camera that sees only sky traces exactly its camera rays; on a smooth floor
  Bunny rejects some bounces, a rejected bounce traces no ray and none is counted. Seeds that
  differ only in the top bit, or only in the lowest, give different images, each equal to a
  recomputation that builds every pixel's sampler with `PixelSampler::new` itself. Five
  mutations each fail these tests: the sample loop capped at 32, a shadow ray counted twice, the
  bounce ray not counted, bit 31 of the seed dropped and its lowest bit forced to 1.
- **Convergence** (a regression guard, not a proof): for six reference pixels the mean of 1024
  samples is within `5 / 256` of the mean of 256 per channel.
- **The bunny**: the asset's SHA-256; the converted mesh (counts, the first vertex's raw values and a
  SHA-256 over every raw coordinate and index, recorded from the interim reader that Bunny's
  replaced); the converted mesh's bounds equal Bunny's conversion of the manifest's bounds; a
  refusal keeps Bunny's kind and line number; the placed bounds derived by hand, with the lowest
  vertex exactly on the ground; no dropped and no degenerate triangle; the winding and hole
  counts; a camera ray through the bunny's pixels hits it; and a ground point behind it, derived
  from the light direction through a point inside its body, is in its shadow while a nearby point
  in front is lit.
- **Two runs**: the same image rendered twice in one process, and by two separate `lumen render`
  processes, compared byte for byte along with the printed hash and ray count.
- **Thread counts and band heights**: see "Why the image does not depend on the threads" above.
- **Golden hash**: the 96 by 54 reference image at 4 samples per pixel and seed 0 has a recorded
  hash, together with the scene, integrator, sampler, shading, tone and encoding identities and the
  Bunny commit. It is one constant (`crates/lumen/tests/golden/mod.rs`) that every platform must
  give; there is no per-platform value. It was first recorded on aarch64 Linux. Updating it is
  legitimate only when one of those identities, the settings or the Bunny commit changes.

**Proven in CI, and nothing else:** x86-64 Linux, aarch64 Linux, aarch64 macOS, x86-64 Windows
(MSVC) and WebAssembly (`wasm32-wasip1` under Node, one worker) each render both golden frames
(the reference scene and Fixed Point, 96 by 54, 4 samples, seed 0) and compare them with the two
shared constants in `crates/lumen/tests/golden/mod.rs`, in the test profile and in the release
profile (fat LTO). The WebAssembly leg was first run in CI; the container image has no
`wasm32-wasip1` target or Node. `wasm32-unknown-unknown` is compiled, not run. Which job proves
which platform (`.github/workflows/ci.yml`; each checks that the runner's `rustc` host is the
platform it is named for):

| Platform | Job | What it runs | Oracle |
|---|---|---|---|
| x86-64 Linux | `Determinism / Linux x86-64` | `cargo test --workspace` | both golden hashes (reference and Fixed Point), the recorded PNG bytes and every other exact test |
| aarch64 Linux | `Determinism / Linux aarch64` | `cargo test --workspace` | the same |
| aarch64 macOS | `Determinism / macOS aarch64` | `cargo test --workspace` | the same |
| x86-64 Windows (MSVC) | `Determinism / Windows x86-64` | `cargo test --workspace` | the same |
| WebAssembly (`wasm32-wasip1`, Node) | `Determinism / WebAssembly (wasm32-wasip1, Node)` | the library's unit tests, `tests/render.rs`, `tests/library.rs` and `tests/fixed_point/`, on one worker | the same constants; two workers must be `RenderError::Spawn` |

On WebAssembly the thread and command tests (`tests/threads.rs`, `tests/cli.rs`) do not run:
that target starts no threads or processes. The same job compiles the library for
`wasm32-unknown-unknown`, the browser target, without running it. Why WebAssembly is tested
this way is recorded in [ADR 0004](docs/adr/0004-webassembly-determinism-leg.md).

### Nondeterminism bans

The golden hashes find a difference only in the scenes they render, so the sources of one are
banned before they can reach a pixel ([ADR 0006](docs/adr/0006-nondeterminism-gate.md)). The
rules use Echo's `ban-nondeterminism` rule IDs, plus Lumen's `thread-spawn` and `float-type`:

| Rule | Banned | Enforced by |
|---|---|---|
| `time-system`, `time-instant` | `SystemTime`, `SystemTimeError`, `Instant` and their `now` and `elapsed` | `clippy.toml`, in `fmt, clippy` |
| `thread-sleep`, `host-parallelism`, `thread-spawn` | `std::thread::sleep`, `sleep_ms`, `park_timeout`, `park_timeout_ms`, `available_parallelism`, `spawn`, `scope`, `Builder` | the same |
| `unordered-container` | `HashMap`, `HashSet`, `RandomState` | the same |
| `std-env`, `std-fs`, `std-process`, `network` | every stable function and type of `std::env`, `std::fs`, `std::process` and `std::net` (except the address types), the `Path` methods that touch files, `env!` and `option_env!` | the same |
| `float-type` | `f32`, `f64`, the float methods of `Duration`, Bunny's float bridge (`Scalar`, `Vec3`, `Ray3`, `PlyVertex`, `to_f32`, `into_float`, ...); float arithmetic in the library (`clippy::float_arithmetic`) | the same, and `lib.rs` |
| `random`, `host-callback`, `json-serialization`, `wasm-serde`, crate forms of the rest | any crates.io crate | `deny.toml` (`allow-registry = []`), in `Gates / cargo-deny` |
| `float-type`, `float-literal`, `float-cast`, the names and `std` paths above, `hand-hashed-table` | in the core, also under a `cfg` that is not built and in macro bodies: `f32`, `f64`, float literals, `as f64`; `Instant`, `SystemTime`, `HashMap` and the other banned names wherever spelled; names with the word `f32` or `f64` (`as_secs_f64`); `std::env`, `fs`, `process`, `net` and `thread` sleep paths through `use` trees, renames and `extern crate` aliases; any line naming open addressing | `cargo run -p xtask -- nondeterminism`, in `Gates / nondeterminism` |

The bans apply to the whole workspace. The deterministic core is every file the library
compiles outside `#[cfg(test)]`, found by following its module tree from `src/lib.rs` as `rustc`
does, not by file name (ADR 0006); the command (the only clock and the only `available_parallelism`), the examples,
`tests/cli.rs` and `xtask` are outside it and each carries one crate-level waiver. Inside it, the
scheduler's threads (`render_pixels`, ADR 0003) and two unit tests carry their own. A waiver is
`#[expect(<clippy lints>, reason = "<rule-ids>: <why it cannot reach a pixel>")]` on the
smallest item that needs it: `expect`, so one that stops matching fails the build; the scan
fails an `allow` or `warn` of a ban lint, a ban lint's old name, any lowering of a lint group that
holds one (`clippy::all`, `clippy::style`, `clippy::restriction`, `warnings`), a lint list built
by a macro, and a reason that does not start with rule IDs. A scan finding is waived by the
comment `// nondeterminism: allow <rule-id> -- <reason>` on its line or the line above (the same
text in a string or a doc comment is not a waiver); the scan fails one with an unknown rule,
without a reason, or that waives nothing. There is none today.
Not covered: a `usize`-dependent value reaching the hash, which the WebAssembly leg (32-bit
`usize`) tests.

Run the scan with `make nondeterminism`, or:

```sh
python3 $D lumen ~/git/lumen --worktree -- cargo run --locked -q -p xtask -- nondeterminism
```

Clippy only warns about a `clippy.toml` path that does not resolve, so the `fmt, clippy` job also
runs `cargo run --locked -q -p xtask -- clippy-bans` (`make clippy-bans`). It runs Clippy on
`xtask/clippy-fixture`, a workspace member whose seeded uses of every banned item compile only
under `--cfg lumen_clippy_fixture`, and fails when an entry produces a diagnostic about
`clippy.toml` or never fires there, when a module marked `// seeded: <rule-id>` is not reported
under its rule or one marked `// accepted` is reported, when there is any Clippy configuration
file besides the root `clippy.toml` (a crate's own replaces it), and when the toolchain's lint
groups holding a ban lint change. A new ban is a line in `clippy.toml` and a use in the fixture.
The std part of the list is checked against `xtask/bans/std-items.txt`, every public item of the
banned std modules in the pinned toolchain's documentation: an item decided banned without an
entry, an item without a decision, or a list made for another toolchain fails the check. ADR 0006
gives the commands that regenerate the list when the toolchain moves
(`cargo run -p xtask -- std-items <rust-docs std dir> <version>`).

## Bunny dependency

Lumen uses Bunny's `bunny-num`, `bunny-linalg`, `bunny-geom`, `bunny-query`, `bunny-broadphase`,
`bunny-optics` and `bunny-codec` (with default features off, so it needs no crates.io crate) at
version 0.7.1. The crates come in as git dependencies pinned to the full commit
`2cd551e248d55407a8f27e84dcb26d1b1d8768bd` of `https://github.com/flyingrobots/bunny`, a commit of Bunny's
main after the 0.7.1 release (`bunny-codec` also brings in `bunny-mesh`). They move to the crates.io registry when a release holds what lumen
needs. `BUNNY_REV` in `crates/lumen/src/lib.rs`
records the commit, and a test checks that `Cargo.lock` resolves every Bunny crate to it and lists no registry crate.

Dependencies build with `opt-level = 2` even in debug builds and tests, so the brute-force tests
and a default-size render take seconds; lumen's own code keeps the debug profile.

To move the pin, change the `rev` of every Bunny dependency in `Cargo.toml` and `BUNNY_REV`,
then regenerate `Cargo.lock` in the container and apply the exported patch:

```sh
python3 $D lumen ~/git/lumen --worktree --network --export-patch --out "$(mktemp -d)" -- \
  bash -c 'mkdir -p /tmp/ch && CARGO_HOME=/tmp/ch cargo update -w'
```

## Build and test

Builds and tests run in the shared containers through `devrun`, never on the host
(see `~/git/devcontainers/README.md`). The toolchain is pinned to 1.96.0 in
`rust-toolchain.toml`, the version of the container image.

```sh
D=~/git/devcontainers/bin/devrun
python3 $D lumen ~/git/lumen --worktree --fetch -- cargo fmt --all --check
python3 $D lumen ~/git/lumen --worktree -- cargo clippy --workspace --all-targets --locked -- -D warnings
python3 $D lumen ~/git/lumen --worktree -- cargo test --locked --workspace
python3 $D lumen ~/git/lumen --worktree -- bash -c 'RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked'
python3 $D lumen ~/git/lumen --worktree -- cargo check -p lumen --lib --target wasm32-unknown-unknown --locked
```

The tests include the doc examples (one per workflow: build a scene, render it, hash it and
record its provenance, write a PNG into memory) and `crates/lumen/tests/library.rs`, which renders
through the library alone. The `wasm32-unknown-unknown` check shows the library builds without the
command.

The WebAssembly tests need the `wasm32-wasip1` target and Node 20 or later, which the container
image does not have; CI runs them (`Determinism / WebAssembly (wasm32-wasip1, Node)`). Where both
are installed, from the repository root:

```sh
rustup target add wasm32-wasip1
CARGO_TARGET_WASM32_WASIP1_RUNNER="node $PWD/scripts/wasi-run.mjs" \
  cargo test --locked -p lumen --target wasm32-wasip1 --lib --test render --test library \
    --test fixed_point --test generated
```

### Generated tests

A claim that quantifies over inputs (any split of a SHA-256 input, any image size, any order of a
pixel's samples) is tested over generated inputs from `crates/lumen/tests/generator/mod.rs`, a
seeded SplitMix64 (Lumen takes no property-testing crate). Each test fixes its seed, derives one
seed per case, prints the seed and the case seed in every failure message, keeps to a bounded
number of cases and states its oracle and run time in its comment. Replay one case with
`Gen::new(case_seed)`. A new generated test is shown to fail on a seeded mutation of the code it
checks before it merges. The tests are `tests/generated.rs`, `src/sha256.rs` and
`src/png/generated.rs`; the first also runs in the WebAssembly job, so none may assume a 64-bit
`usize`.

`--fetch` is needed on the first run in a worktree; it downloads the Bunny git dependency, and
later phases build offline. Use `--commit HEAD` instead of `--worktree` for evidence pinned to a
commit.

Lint posture mirrors Bunny: `unsafe` is forbidden, clippy `all` is denied and
`pedantic` warns, and CI runs with warnings denied.

### Supply-chain, size and coverage checks

`.github/workflows/gates.yml` runs five jobs on every pull request:

| Job | What it checks | Fails when |
|---|---|---|
| `Gates / cargo-deny` | `cargo deny --locked check` against `deny.toml` | an advisory, a yanked crate, a licence other than Apache-2.0, a second version of a crate, a wildcard version, any crate from a registry (crates.io included), a git repository other than `https://github.com/flyingrobots/bunny`, or a git dependency not pinned by `rev` |
| `Gates / cargo-audit` | `cargo audit --deny warnings` on `Cargo.lock` | a RustSec advisory or warning for a locked crate |
| `Gates / file length` | `cargo run -p xtask -- file-length` | a source file over 300 lines of code without a listed exception, or over 500 at all; a test file (one compiled only under `#[cfg(test)]`, or an integration test) over 750; a listed file grown past its exception; an exception no longer needed; an `include!`, or a `#[path]` to anything but a `.rs` file the gate reads |
| `Gates / nondeterminism` | `cargo run -p xtask -- nondeterminism` (see "Nondeterminism bans") | a float, a banned name or `std` path in the deterministic core, a malformed or unused scan waiver, a ban lint waived with `allow` or without rule IDs in its reason, or the `include!` and `#[path]` forms above |
| `Signal / coverage` | `cargo llvm-cov --workspace --summary-only`, in the job summary | never: coverage is reported, not gated |

A line of code is a line with a character outside a comment; doc comments are comments, and so
is a shebang line, which both gates skip exactly as `rustc` does (ADR 0006). The
exceptions live in `xtask/src/file_length/exceptions.rs`, each with the count it was granted at,
the date and the issue that splits the file.

Locally, in the container (it has `cargo-deny`; the advisory database needs the network and a
writable `CARGO_HOME`):

```sh
python3 $D lumen ~/git/lumen --worktree -- cargo run --locked -q -p xtask -- file-length
python3 $D lumen ~/git/lumen --worktree --network -- \
  bash -c 'mkdir -p /tmp/ch && CARGO_HOME=/tmp/ch cargo deny --locked check'
```

The container has neither `cargo-audit` nor `cargo-llvm-cov`; CI installs both as pinned
prebuilt binaries. To run them in the container, install them under `/tmp` first, for example
`cargo install --locked cargo-audit --version 0.22.2 --root /tmp/audit` with `--network`.

## Contributing

Run the gates under "Build and test" before opening a pull request, and add a line to
`CHANGELOG.md` under `Unreleased`. A nondeterminism waiver (see "Nondeterminism bans") is asked
for in the pull request: its reason says why the banned item cannot reach a pixel, a hash or a
digest, and review accepts or refuses it there.

### Tests

- **Size classes** (testing standard rule 9). Every integration test file says its class in the
  first line of its module comment, `//! Size class: small|medium`. Unit tests in `src/` are small.
  *Small*: in one process, no file, no network, no clock. The renderer's own worker threads are
  allowed because they are the code under test. *Medium*: spawns the `lumen` process, writes
  files, or runs real threads at fixed counts. Run one class by file: `cargo test --test cli
  --test threads --test counts` is the medium set; `cargo test --lib --test render --test library --test
  fixed_point --test statistics` the small set. There is no large class yet.
- **Hermetic files** (rule 8). A test that writes files makes a `Scratch` directory (see
  `crates/lumen/tests/cli.rs`) named for the test and the process id under `CARGO_TARGET_TMPDIR`,
  runs the command with it as the working directory, passes absolute paths and removes it when
  done, so the test and release profiles can run at once.
- **One behaviour per test**, named as a sentence about it, with no platform branch inside the
  body: a platform difference is a `#[cfg]` on its own test.

## License

Apache-2.0. See [LICENSE](LICENSE).

The Stanford Bunny mesh in `assets/stanford-bunny/` is not covered by that license: it is from the
Stanford 3D Scanning Repository, Stanford Computer Graphics Laboratory, and is used with
attribution. See `assets/stanford-bunny/README.md`.
