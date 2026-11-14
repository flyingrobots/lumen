# How Lumen renders, and why the image is the same everywhere

This page explains what happens when Lumen renders a scene and why the result is the same bytes
on every run, at every thread count and on every platform CI checks. It ends with a
[map from each of those claims to the tests and jobs that check it](#requirements-and-their-evidence).
For the options, see the [command reference](reference/cli.md); for the scenes, the
[scene reference](reference/scenes.md).

## What a render is

Lumen renders a built-in scene, prints its image hash and timing, and writes it to a PNG file
if asked; `lumen stats` prints what a scene holds ([statistics](reference/cli.md#statistics)). `--scene reference` (the default) is the reference scene, which includes the Stanford
bunny; `--scene fixed-point` is [Fixed Point](reference/scenes.md#the-fixed-point-scene), a showpiece;
`--scene canyon` is [the canyon](reference/scenes.md#the-canyon-scene), a picture and a
large-triangle-count benchmark. The
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
`lumen-diffuse-path/1`. Lumen adds no arithmetic. [ADR 0007](adr/0007-smooth-shading.md)
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
`TraceError`, `FAR`), `scene/primitive_index.rs` the triangle's name (`PrimitiveIndex`, its
position in input order, which breaks ties and so is part of the scene), and `scene/tree.rs` the
adapter that builds Bunny's tree and answers the port's queries with it.
`crates/lumen/examples/traversal.rs` repeats the measurement, checks that all four give the same radiance bits, and prints the build times:

```sh
cargo run --locked --release -q -p lumen --example traversal -- 160 90 4 7
```

Any timing it prints is a reading for information; for one worth comparing, run it alone on the
machine ([measure performance](how-to/measure-performance.md)).

A test checks that the chosen structure returns the same hit as `closest_hit` (and the same
answer as `any_hit`) on a plain `build_bvh` tree for every camera ray of a small image.

Readings of that command and of the scene's cost per ray, which this page does not repeat, are
in [the README readings record](measurements/2026-11-13-readme-readings.md).

## Threads and row bands

`render_scheduled` (and `lumen render`) cut the image into bands of whole rows, one row by
default (`--band-rows`; `crates/lumen/src/schedule.rs`), from the top; the last band is cut to
the image. The design, what is shared, the stop rule's proof and what was rejected are recorded
in [ADR 0003](adr/0003-row-bands.md), which supersedes the tile design of
[ADR 0001](adr/0001-tile-scheduling.md). The bands are `chunks_mut` slices of the one image
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
  one-pixel-at-a-time render in raster order meets, at every thread count and band height
  (`every_hand_out_order_reports_the_raster_first_failure` in the simulation,
  `the_first_failing_pixel_in_raster_order_is_reported_at_every_schedule` and
  `a_traversal_failure_is_the_same_typed_error_at_every_schedule` on threads). When a pixel in
  row `y` fails, the failing worker lowers one shared atomic to `y`, and a worker that has
  observed it begins no row and takes no band from `y` down
  (`after_a_failure_no_pixel_from_its_row_down_is_rendered`,
  `after_a_failure_no_band_from_its_row_down_is_handed_out`): every pixel before the failing one
  is in an earlier row, or earlier in row `y`, which only the failing worker renders. When
  another worker observes it depends on the scheduler, so this is not a time bound: on several
  threads the work done after a failure is not bounded by any test; on one thread it is exact
  (`workers_stop_after_a_failure`). No worker waits on
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

## Image hash and PNG

[ADR 0009](adr/0009-image-hash.md) records what is hashed and why.

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
(MSVC) and WebAssembly (`wasm32-wasip1` under Node, one worker) each render the golden frames
(96 by 54, 4 samples, seed 0) and compare them with the shared constants in
`crates/lumen/tests/golden/mod.rs`, in the test profile and in the release profile (fat LTO):
the native jobs all three scenes, the WebAssembly job the reference scene and Fixed Point (the
canyon's test binary is not in its list; it checks the canyon's content identity and a small
render through `tests/library.rs`). The WebAssembly leg was first run in CI; the container image has no
`wasm32-wasip1` target or Node. `wasm32-unknown-unknown` is compiled, not run. Which job proves
which platform (`.github/workflows/ci.yml`; each checks that the runner's `rustc` host is the
platform it is named for):

| Platform | Job | What it runs | Oracle |
|---|---|---|---|
| x86-64 Linux | `Determinism / Linux x86-64` | `cargo test --workspace` | the three golden hashes (reference, Fixed Point and canyon), the recorded PNG bytes and every other exact test |
| aarch64 Linux | `Determinism / Linux aarch64` | `cargo test --workspace` | the same |
| aarch64 macOS | `Determinism / macOS aarch64` | `cargo test --workspace` | the same |
| x86-64 Windows (MSVC) | `Determinism / Windows x86-64` | `cargo test --workspace` | the same |
| WebAssembly (`wasm32-wasip1`, Node) | `Determinism / WebAssembly (wasm32-wasip1, Node)` | the library's unit tests, `tests/render.rs`, `tests/library.rs`, `tests/fixed_point/`, `tests/generated.rs` and `tests/smooth.rs`, on one worker | the same constants; two workers must be `RenderError::Spawn` |

On WebAssembly the thread and command tests (`tests/threads.rs`, `tests/cli.rs`) do not run:
that target starts no threads or processes. The same job compiles the library for
`wasm32-unknown-unknown`, the browser target, without running it. Why WebAssembly is tested
this way is recorded in [ADR 0004](adr/0004-webassembly-determinism-leg.md).

### Nondeterminism bans

The golden hashes find a difference only in the scenes they render, so the sources of one are
banned before they can reach a pixel ([ADR 0006](adr/0006-nondeterminism-gate.md)). The
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
cargo run --locked -q -p xtask -- nondeterminism
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


## Requirements and their evidence

Every determinism claim on this page, in the README, the reference pages and the rustdoc maps to
one row here. A row's evidence is the narrowest executable check; test names are written
`file::test`, files relative to `crates/lumen/` unless they start with `xtask/` or `.github/`.
**Covered** means the named checks run in CI on every pull request. **Not verified** marks what
no check covers; those rows say so wherever the claim is made. The IDs are stable: a new
requirement gets a new number, and a removed one keeps its number unused.

| ID | Claim | Executable evidence | Fixture or input | Exact oracle | Status |
| --- | --- | --- | --- | --- | --- |
| R-DET-1 | The same scene, size, samples and seed give the same RGBA8 bytes, and image hash, on every run | `tests/render.rs::rendering_twice_in_one_process_gives_the_same_bytes`; `tests/cli.rs::two_processes_write_the_same_file_and_print_the_same_hash`; the golden tests below on every run | the reference scene; two `lumen render` processes | byte equality of the images and files; equal printed hash and ray count | covered |
| R-DET-2 | The bytes do not depend on the thread count, the band height or the hand-out order | `src/workers/tests.rs::every_hand_out_order_gives_the_raster_image`; `tests/threads.rs::the_golden_hashes_hold_at_every_thread_count_and_band_height`, `::a_larger_image_is_the_same_on_one_thread_and_on_many`; `tests/cli.rs::the_hash_and_ray_count_are_the_same_at_every_thread_count_and_band_height`; `tests/run_statistics.rs::both_tallies_render_the_golden_images_and_the_same_statistics` | simulated: 1, 2, 3, 7 and 200 workers, bands of 1, 2, 3, 4, 5, 9 and 20 rows, 200 seeded and 2 scripted orders; threads: 1, 2, 3, 4, 7 and 18 at ten band heights | the raster-order loop's bytes; `GOLDEN_96X54_4SPP`, `GOLDEN_FIXED_POINT_96X54_4SPP` and `GOLDEN_CANYON_96X54_4SPP` | covered for the counts and heights tried; other counts follow from the argument in [Why the image does not depend on the threads](#why-the-image-does-not-depend-on-the-threads) |
| R-DET-3 | The bytes are the same on x86-64 and aarch64 Linux, aarch64 macOS and x86-64 Windows (MSVC), in the test and the release profile | CI jobs `Determinism / Linux x86-64`, `Linux aarch64`, `macOS aarch64`, `Windows x86-64`, each `cargo test --workspace` and `--release`, after checking the runner's host | the three scenes at 96 by 54, 4 samples, seed 0 | `GOLDEN_96X54_4SPP`, `GOLDEN_FIXED_POINT_96X54_4SPP` and `GOLDEN_CANYON_96X54_4SPP` (`tests/golden/mod.rs`, one value for every platform) through `tests/render.rs::the_reference_image_has_its_golden_hash`, `tests/fixed_point/rendering.rs::the_fixed_point_image_has_its_golden_hash` and `tests/canyon/rendering.rs::the_canyon_image_has_its_golden_hash` | covered; any other target is **not verified** |
| R-DET-4 | The bytes are the same under WebAssembly | CI job `Determinism / WebAssembly (wasm32-wasip1, Node)`: the library's unit tests, `tests/render.rs`, `tests/library.rs`, `tests/fixed_point/`, `tests/generated.rs` and `tests/smooth.rs`, both profiles; `tests/library.rs::on_webassembly_a_second_worker_is_a_spawn_error` | `wasm32-wasip1`, one worker | the reference and Fixed Point golden constants; the canyon's content digest; two workers must be `RenderError::Spawn` | covered for `wasm32-wasip1`, except that the canyon's golden image hash is **not verified** there (its test binary is not in the job's list); `wasm32-unknown-unknown` is compiled, not run: **not verified** |
| R-DET-5 | A pixel does not depend on the order its samples are computed or accumulated, nor the image on the order of its pixels | `tests/render.rs::a_pixel_does_not_depend_on_the_order_of_its_samples`, `::an_image_does_not_depend_on_the_order_of_its_pixels`; `tests/generated.rs::a_pixel_does_not_depend_on_the_order_its_samples_are_evaluated_accumulated_or_merged` | forwards, backwards, shuffled and merged partial accumulators; generated permutations and merge trees, seeded | equal bits; equal image hash | covered |
| R-PROV-1 | Equal provenance means equal bytes, for a render of the scene as its own tracer (`render`, `render_scheduled`, `render_traced` given the scene) | R-DET-1 to R-DET-4 and R-PROV-2; `tests/library.rs::the_provenance_lists_what_the_command_prints_in_its_order` | the built-in scenes | as R-DET-1 to R-DET-4 | covered for the three built-in scenes; for any other scene it follows from R-PROV-2 and is not tested scene by scene; a caller-supplied `Tracer` is outside the claim |
| R-PROV-2 | Provenance binds the scene's content: the content identity is the SHA-256 of ADR 0005's encoding, the same on every platform, and changes with every input that can change a pixel | `src/scene/content_id/tests.rs::the_digest_is_the_sha256_of_the_adr_encoding_built_by_hand`, `::every_single_change_to_the_content_changes_the_digest`, `::the_counts_keep_lists_of_different_lengths_apart`, `::the_same_spec_gives_the_same_digest_and_the_tree_is_no_input`; `tests/library.rs::two_scenes_under_one_name_have_different_provenance`, `::a_one_triangle_scene_has_the_content_identity_python_computed`, `::every_builtin_scene_builds_renders_and_has_its_own_identity` (on every Determinism job) | the encoding built by hand from ADR 0005's table; a one-triangle scene with and without normals | digests computed outside lumen with Python's `hashlib`; `GOLDEN_REFERENCE_CONTENT`, `GOLDEN_FIXED_POINT_CONTENT`, `GOLDEN_CANYON_CONTENT` | covered |
| R-HASH-1 | The image hash is `sha256:` and the lowercase hex SHA-256 of the raw RGBA8 bytes, row-major from the top, with no width or height ([ADR 0009](adr/0009-image-hash.md)) | the doc example of `Rgba8Image::hash` (`src/image.rs`); `src/sha256.rs::published_test_vectors`, `::padding_boundaries_match_an_independent_implementation`; `tests/cli.rs::the_printed_hash_ray_count_and_file_are_the_librarys` | one opaque black pixel; FIPS 180-2 vectors | the published SHA-256 of `00 00 00 ff`; the published vectors and Python's `hashlib` | covered |
| R-PNG-1 | The PNG's bytes are a fixed function of the pixels | `tests/library.rs::a_png_written_to_memory_has_the_bytes_the_command_wrote`; `src/png/tests.rs::a_one_pixel_file_is_byte_for_byte_as_constructed_by_hand`, `::written_files_decode_to_the_pixels`; `src/png/generated.rs::files_of_any_size_decode_to_the_same_pixels_and_have_the_length_png_len_gives` | a 5 by 3 render; one pixel; 73 generated sizes | the 131 bytes recorded before flyingrobots/lumen#19; a file built by hand; a strict decoder | covered |
| R-PUB-1 | `--out` names the complete new PNG or what was there before, whether the render or the write fails | `src/bin/lumen/publish/tests.rs::a_write_that_fails_part_way_leaves_the_old_file_and_no_temporary_file`, `::a_failed_sync_or_rename_leaves_the_old_file_and_no_temporary_file`, `::two_publications_to_one_destination_each_leave_a_complete_file`, `::a_count_that_differs_from_the_expected_length_publishes_nothing`; `tests/cli.rs::an_unwritable_destination_is_refused_before_any_pixel_is_computed`, `::a_render_replaces_an_existing_file_and_leaves_no_temporary_file` | an injected file system that fails at each step | the old file's bytes unchanged; no temporary file left | covered for failures the process sees; survival of a power loss after the rename is **not verified** (no crash test) |
| R-STOP-1 | A failed render reports the first failing pixel in raster order, at every thread count, band height and hand-out order | `src/workers/tests.rs::every_hand_out_order_reports_the_raster_first_failure`; `tests/threads.rs::the_first_failing_pixel_in_raster_order_is_reported_at_every_schedule`, `::a_traversal_failure_is_the_same_typed_error_at_every_schedule`; `tests/run_statistics.rs::both_tallies_report_the_same_failure` | failing pixels chosen so that column-major order would pick another | the failing pixel picked by inspection; a raster-order loop over `trace_path` | covered |
| R-STOP-2 | After a failure in row `y`, a worker that has observed it begins no row and takes no band from `y` down | `src/workers/tests.rs::after_a_failure_no_pixel_from_its_row_down_is_rendered`, `::after_a_failure_no_band_from_its_row_down_is_handed_out`; `tests/threads.rs::workers_stop_after_a_failure` | the simulation, where a worker observes the failure at its next step; one real thread | no pixel at or below `y` after the failure; the bands starting above `y`, by hand; exactly the raster prefix on one thread | covered in the simulation and on one thread; how much other threads compute before they observe it is **not verified** and not bounded |
| R-STOP-3 | A panic in the pixel function or a refused worker thread halts the other workers, and the panic or the refusal reaches the caller with no image | `src/workers/tests.rs::after_a_halt_no_row_is_begun`, `::a_panic_in_the_pixel_function_stops_every_worker_at_its_next_row`, `::after_a_refused_start_no_started_worker_begins_a_row`, `::a_refused_worker_is_reported_with_its_number_and_its_error`; `tests/threads.rs::a_panic_in_the_pixel_function_reaches_the_caller_without_an_image` | a 64 by 512 image, a panic at `(0, 0)`; a spawner that refuses worker `n` | `1 + 64 * rows` pixels computed in the simulation; the panic's own payload; `Spawn` for worker `n` | covered in the simulation; on real threads the work done before the halt is seen is **not verified** |
| R-STAT-1 | Every statistic but the per-worker split and the memory bytes is a function of the scene and settings, the same at every schedule | `tests/statistics.rs::statistics_are_a_function_of_the_scene`; `tests/run_statistics.rs::the_workers_add_up_to_the_render_at_every_schedule` | two calls and two builds of a scene; a 23 by 17 render at 1, 2, 4 and 18 threads and bands of 1, 2 and 5 rows | equal statistics; the render equal to the one-thread render; per-worker figures summing to its totals | covered |
| R-DEP-1 | Lumen takes no third-party crate: Bunny's crates from one git repository, pinned by commit, and nothing from a registry | CI job `Gates / cargo-deny` (`deny.toml`: `allow-registry = []`, one allowed git repository, `rev` required); `tests/render.rs::the_lockfile_resolves_bunny_to_the_recorded_commit` | `Cargo.lock` | no registry source in `Cargo.lock`; every Bunny crate at `BUNNY_REV` | covered |
| R-NDET-1 | No clock, sleep, core count, thread spawn, hash map, environment, file, process, socket or float reaches the deterministic core | CI job `fmt, clippy` (the `clippy.toml` bans, and `cargo run -p xtask -- clippy-bans`, which checks on `xtask/clippy-fixture` that every ban resolves and fires); CI job `Gates / nondeterminism` (`cargo run -p xtask -- nondeterminism`, its forms in `xtask/src/nondeterminism/tests.rs`); `#![deny(clippy::float_arithmetic)]` in `src/lib.rs` | seeded forms and look-alikes in the fixture and the scan's tests | each seeded form reported under its rule; no look-alike reported; no finding on the core | covered; a `usize`-dependent value reaching the hash is caught only by R-DET-4's 32-bit leg ([ADR 0006](adr/0006-nondeterminism-gate.md)) |
