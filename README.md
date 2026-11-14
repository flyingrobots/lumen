# Lumen

A deterministic ray tracer: a [Geordi](https://github.com/flyingrobots/geordi) runtime built on
[Bunny](https://github.com/flyingrobots/bunny)'s fixed-point math. The same scene renders to the same bytes on
every platform CI checks: x86-64 and aarch64 Linux, aarch64 macOS, x86-64 Windows and
WebAssembly (see [Determinism](#determinism)).

Work is tracked in GitHub Issues with `issue-dag/2` frontmatter.

## What lumen does today

Lumen renders a built-in scene, prints its image hash and timing, and writes it to a PNG file
if asked; `lumen stats` prints what a scene holds ([Statistics](#statistics)). `--scene reference` (the default) is the reference scene, which includes the Stanford
bunny; `--scene fixed-point` is [Fixed Point](#the-fixed-point-scene), a showpiece. The
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
make demo            # both built-in scenes at 1280x720, opened when done
make fixed-point     # just the Fixed Point scene
make 4k              # the reference scene at 3840x2160, 16 samples
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

The command's report, its options and their values, its exit status and how `--out` publishes the
file are in the [command reference](docs/reference/cli.md).

## Library and command

The library's public surface, its errors and the command's edge are in the
[library reference](docs/reference/library.md).

## Statistics

`lumen stats` and `lumen render --stats`, with every key they print, are in the
[command reference](docs/reference/cli.md#statistics).

## The reference scene

The two built-in scenes, `lumen-reference-scene/3` and `lumen-fixed-point-scene/2`, triangle by
triangle, with their materials, lighting, cameras and tests, are in the
[scene reference](docs/reference/scenes.md).

## The Fixed Point scene

See the [scene reference](docs/reference/scenes.md#the-fixed-point-scene).

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
`TraceError`, `FAR`), `scene/primitive_index.rs` the triangle's name (`PrimitiveIndex`, its
position in input order, which breaks ties and so is part of the scene), and `scene/tree.rs` the
adapter that builds Bunny's tree and answers the port's queries with it.
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
Each states its commit and conditions and applies to that commit only.

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
`d4fe51d38fa10f8d6496bd110955314edac30bbf` of `https://github.com/flyingrobots/bunny`, a commit of Bunny's
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

`.github/workflows/gates.yml` runs six jobs on every pull request:

| Job | What it checks | Fails when |
|---|---|---|
| `Gates / cargo-deny` | `cargo deny --locked check` against `deny.toml` | an advisory, a yanked crate, a licence other than Apache-2.0, a second version of a crate, a wildcard version, any crate from a registry (crates.io included), a git repository other than `https://github.com/flyingrobots/bunny`, or a git dependency not pinned by `rev` |
| `Gates / cargo-audit` | `cargo audit --deny warnings` on `Cargo.lock` | a RustSec advisory or warning for a locked crate |
| `Gates / file length` | `cargo run -p xtask -- file-length` | a source file over 300 lines of code without a listed exception, or over 500 at all; a test file (one compiled only under `#[cfg(test)]`, or an integration test) over 750; a listed file grown past its exception; an exception no longer needed; an `include!`, or a `#[path]` to anything but a `.rs` file the gate reads |
| `Gates / nondeterminism` | `cargo run -p xtask -- nondeterminism` (see "Nondeterminism bans") | a float, a banned name or `std` path in the deterministic core, a malformed or unused scan waiver, a ban lint waived with `allow` or without rule IDs in its reason, or the `include!` and `#[path]` forms above |
| `Gates / doc links` | `cargo run -p xtask -- doc-links` | a relative link in a Markdown file to a file or directory that does not exist or out of the repository, or a `#anchor` that is no heading of its page (GitHub's anchors); links with a scheme are not followed |
| `Signal / coverage` | `cargo llvm-cov --workspace --summary-only`, in the job summary | never: coverage is reported, not gated |

A line of code is a line with a character outside a comment; doc comments are comments, and so
is a shebang line, which both gates skip exactly as `rustc` does (ADR 0006). The
exceptions live in `xtask/src/file_length/exceptions.rs`, each with the count it was granted at,
the date and the issue that splits the file.

Locally, in the container (it has `cargo-deny`; the advisory database needs the network and a
writable `CARGO_HOME`):

```sh
python3 $D lumen ~/git/lumen --worktree -- cargo run --locked -q -p xtask -- file-length
python3 $D lumen ~/git/lumen --worktree -- cargo run --locked -q -p xtask -- doc-links
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
