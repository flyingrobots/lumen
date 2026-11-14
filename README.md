# Lumen

A deterministic ray tracer: a [Geordi](https://github.com/flyingrobots/geordi) runtime built on
[Bunny](https://github.com/flyingrobots/bunny)'s fixed-point math. The same scene renders to the same bytes on
every machine.

Work is tracked in GitHub Issues with `issue-dag/2` frontmatter.

## What lumen does today

Lumen renders one built-in reference scene, prints its image hash and timing, and writes it to
a PNG file if asked:

- in tiles on all cores (`--threads`, below), with the same bytes at every thread count;
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

Builds and runs happen in the shared containers through `devrun`, never on the host. Files
written under `/scratch/out/` in the container come back with `--export <name>` into a fresh
directory given by `--out`:

```sh
D=~/git/devcontainers/bin/devrun
python3 $D lumen ~/git/lumen --worktree --fetch --export reference.png --out "$(mktemp -d)" -- \
  bash -c 'mkdir -p /scratch/out && cargo run --locked --release -q -p lumen -- render --width 640 --height 360 --samples 64 --out /scratch/out/reference.png'
```

`--fetch` is needed on the first run in a worktree. The command prints:

```text
scene lumen-reference-scene/2
integrator lumen-diffuse-path/1
shading bunny-diffuse-shading/1
sampler bunny-sobol-owen/1
tone bunny-tone-srgb/1
encoding bunny-rgba8-opaque/1
bunny dcd6bd37613738de93f7084e7c8aa0283de964ea
size 640x360
pixels 230400
samples 64
seed 0
bounces 1
threads <workers used>
threads-asked <--threads, or what the system reports available>
tile 32x32
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

Usage: `lumen render [--out FILE] [--width W] [--height H] [--samples N] [--seed S]
[--threads T] [--tile N | --tile WxH]`; the defaults are 320 by 180, 4 samples per pixel,
seed 0, 32 by 32 tiles and as many threads as the system reports available. Without `--out`
nothing is written and the report says `png none`: the hash and the timing are the whole
result. Width and height are each `1..=32768`
(the limit of Bunny's camera) with at most `2^27` pixels in all (a 512 MiB RGBA8 buffer, enough
for 16K at 15360 by 8640). The sample count must be a power of two from 1 to 1024: Bunny's
sampler states its stratification for aligned power-of-two blocks of samples, and its
accumulator takes at most 1024. The seed is `0..=4294967295`, the domain of Geordi's render
settings. Arguments are read as OS strings, so an output path that is not valid Unicode is
used as given; a non-Unicode number or option is refused like any other bad argument. Bad
arguments exit with status 2 and the usage text; a refused size or an unwritable output path
exits with status 1 and a message. The output file is written after the render, so nothing is
written when the render fails. `--threads` is `1..=256` (0 and anything larger are refused with
status 2); a tile is `N` (N by N pixels) or `WxH`, each side at least 1.

The PNG is uncompressed (see below), so it is about four bytes per pixel: 922 266 bytes at
640 by 360, and 33 188 430 bytes (about 33 MB) at 3840 by 2160.

## The reference scene

`lumen-reference-scene/2` is defined in code in `crates/lumen/src/scene.rs`, entirely in Q32.32
integers with no floating point and no randomness from the host. It is made of triangles only,
so that a Geordi `geordi-ray-traced-scene/1` artifact can describe exactly the same scene
(flyingrobots/lumen#6). In primitive order, with each object's diffuse albedo (linear, red, green,
blue, in 256ths):

1. the ground: two triangles at `y = -1.125`, spanning `-16..=16` on X and Z; albedo
   `(150, 145, 135)`, a warm light grey;
2. a back wall: two triangles at `z = -4`, `x = -3..=3`, `y = -1.125..=2`, facing the camera;
   albedo `(175, 165, 145)`, pale stone;
3. an octahedron of radius 1 centred at `(2.25, -0.125, 0.5)`, standing on a vertex; albedo
   `(50, 95, 200)`, blue;
4. a bumpy sphere around the origin, radius 1.0 to 1.04: a cube subdivided 48 by 48 per face and
   pushed out with Bunny's `FixedVec3::normalize`, adapted from Bunny's scene benchmark
   (27 648 triangles); albedo `(205, 60, 45)`, red.

The lighting is one directional light and one uniform sky, the shape of Geordi's scene profile
(at most one directional light, exactly one sky):

- the sun: toward `(0.5, 0.3125, 1.0)` (from the right, from the camera's side, about 15 degrees
  above the horizon; its largest component is exactly 1.0, as Bunny's `DirectionalLight`
  requires), irradiance `(1024, 970, 900) / 256`, about `(4.0, 3.8, 3.5)`. Bunny's convention: a
  light's value is the irradiance on a surface perpendicular to it, and a diffuse surface of
  albedo `a` lit at cosine `c` reflects `a * E * c / pi`;
- the sky: radiance `(64, 85, 120) / 256`, about `(0.25, 0.33, 0.47)`, a pale blue.

The sphere and the octahedron cast long shadows back and to the left across the ground, and the
sphere's shadow climbs the foot of the wall.

The camera stands at `(0, 1, 5)` looking along `(0, -0.25, -1)` with up `+Y` and a vertical
half-angle whose tangent is 0.5. Every vertex coordinate is a multiple of `2^-16` below 256
units, so it is exactly an `f32` and survives Geordi's `bunny-ply-binary-le-f32/1` mesh layout
unchanged; the sphere's vertices are floored to that grid.

The primitive order matters: of two triangles hit at the same distance, the lower index wins.
**Any change to the scene's triangles, their order, their materials, the lighting or the camera
placement is a new scene version, and changes every image hash.** Version 1
(`lumen-reference-scene/1`: the same triangles and camera with no materials or lights, shaded by
the normal-colour debugging view `lumen-normal-shading/1`) is retired. Neither it nor the normal
view is kept: the view would have needed its own render path, flag, golden hash and tests to stay
honest, and the lit render shows the same geometry.

The scene's types (`SceneSpec`: triangles, a material index per triangle, diffuse albedos, an
optional directional light and a sky; `RenderSettings`: a power-of-two sample count and a `u32`
seed; at most one bounce) follow Geordi's ray-traced scene profile, so that loading an artifact
(flyingrobots/lumen#6) maps onto them.

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
the per-pixel function holds no shared mutable state, so rendering tiles on threads (below) is a
change of schedule only.

Geordi's integrator identity is meant to pin this same composition.

### Traversal

Bunny offers two tree builders (`build_bvh`, `build_bvh_sah`) and two layouts (`BvhNode` with
`closest_hit`/`any_hit`, and `compact_bvh` with `compact_closest_hit`/`compact_any_hit`), all
giving the same answers. Lumen uses the binned-SAH tree in the compact layout, the fastest of the
four on a small render of the reference scene. `crates/lumen/examples/traversal.rs` repeats the
measurement, checks that all four give the same radiance bits, and prints the build times:

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

The 640 by 360 render at 64 samples per pixel traces 38 588 953 rays in about 6.75 s on one
thread in the same setting (about 175 ns per ray, 458 ns per sample, all work included).

## Threads and tiles

`render_scheduled` (and `lumen render`) split the image into tiles, 32 by 32 pixels by default
(`--tile`; `crates/lumen/src/schedule.rs`), counted row-major from the top left; the tiles of
the last column and row are cut to the image. The design, what is shared and what was rejected
are recorded in [ADR 0001](docs/adr/0001-tile-scheduling.md). A shared atomic counter hands the next tile to whichever worker asks, so a region that
costs more (the sphere, a shadow edge) or a slower core never leaves the others idle. That matters
on the Apple M5 Pro, whose 6 "super" and 12 "performance" cores run at different speeds: a
static split would make every worker wait for the slowest share. Each worker renders a whole tile
into its own buffer and then copies the finished tile into the image at the tile's place, under a
lock held only for the copy (`crates/lumen/src/workers.rs`, `render_pixels`, which runs any pixel
function this way and knows nothing of rays). The code is safe Rust with the standard library
only (`std::thread::scope`, atomics, mutexes); there is no `unsafe` and no thread-pool
dependency.

- **Threads**: `--threads N`, `1..=256`; by default the number `std::thread::available_parallelism`
  reports, which inside a container is the CPU limit (`--cpus` of `devrun`). Only the `lumen`
  binary asks the host; the library's `Schedule::new` takes an explicit count. The calling thread is
  one of the workers, and a render never uses more workers than there are tiles. With one thread
  no thread is started and the calling thread takes every tile in order: the same code path with
  one worker, not a separate renderer. `lumen::render` is that one-thread schedule.
- **The scene is shared read-only.** It is built once (triangles, per-triangle surfaces, the SAH
  tree and its compact layout) before any worker starts, and every worker reads it through
  `&Scene`. A compile-time assertion in `render.rs` checks that `Scene`, and so everything
  reachable from it, is `Sync` as it is, with no lock or wrapper. Building it is timed separately
  (`scene-seconds`) from rendering (`render-seconds`).
- **Errors**: if a pixel fails (a traversal error, a ray outside Bunny's domain), the render
  returns the typed error of the first failing pixel in raster order, top row first: the error a
  one-pixel-at-a-time render in raster order meets, at every thread count and tile size. Every
  pixel before it lies in the same row of tiles or an earlier one, so after a failure workers take
  no tile past the end of that row of tiles and abandon such a tile at its next row of pixels. No
  worker panics or waits on another.
- **Progress** is not reported: there is nothing on stderr during a render.

### Why the image does not depend on the threads

Every pixel's samples are accumulated for that pixel alone, by the one worker that renders its
tile, in a `PixelAccumulator` of its own, then resolved, toned and quantized; no sum ever crosses
pixels or threads, so there is no accumulation order between threads to get wrong. The worker
copies the finished bytes to a position fixed by the tile's index. The ray counts are sums of
integers. Thread count, tile size, which worker takes which tile and when, and timing therefore
decide nothing about the bytes.

Tested in the test and release profiles, with no clock, no dependence on the host's core count
and no repeat-and-hope loop. Hand-out orders are explored deterministically: unit tests in
`crates/lumen/src/workers/tests.rs` run 1, 2, 3, 7 and 200 simulated workers on one thread through
the same claim, render-row, store and finish steps the threads run, in orders chosen by 200 seeds
of a generator and by two scripted worst cases (one worker takes everything; tiles finish in
reverse order, so the earliest failing tile is rendered last), each failure naming its order. The
image must equal a raster-order loop byte for byte and the error must be the raster-first
failure; a step budget ends a simulation that does not finish. `crates/lumen/tests/threads.rs`
then runs real threads at fixed counts:

- the golden 96 by 54 hash at 1, 2, 3, 4, 7 and 18 threads, each with tiles of
  1 by 1, 7 by 5, 16 by 16, 32 by 32, 96 by 1, 1 by 54, 13 by 54 and 200 by 200 (larger than the
  image), with the same ray count as a raster-order loop over `pixel_radiance` that does not use
  the scheduler;
- a 320 by 180 image at 8 samples: one thread equals that raster-order loop byte for byte, and 18
  threads with 32 by 32 tiles, 5 threads with 24 by 10 tiles and 16 threads with one-row tiles
  give the same image and ray count;
- errors, each test bounded by a count of work taken (pixels asked for, traversal queries) that
  fails it if exceeded, never by a timeout: an injected pixel function that
  fails at chosen pixels returns the raster-first one at every thread count and tile size, also
  when it is not in the lowest-numbered failing tile (`(40, 0)` in tile 1 before `(3, 5)` in tile
  0); on one thread a failure in the first pixel renders exactly the rest of the first row of
  tiles and nothing more, and on several threads at most that plus one tile per worker; and a
  tracer that fails real traversals in two regions with two different errors returns the one a
  raster-order loop over `trace_path` meets first, at every schedule.

Each of these mutations makes tests fail in the release profile (checked by hand, each mutant's
test run under a 120-second `timeout` as a guard for the checking script, sources touched before
every build so that no stale build is tested):
copying a tile to the wrong column offset; skipping the last tile; dropping the last partial row
of tiles; sharing one accumulator between two neighbouring pixels (thread-local state);
stopping after the failing tile instead of the end of its row of tiles; and choosing the first
failure in column-major order.

## Image hash and PNG

The image hash is `sha256:` followed by the lowercase hex SHA-256 of the raw RGBA8 bytes
(row-major, row 0 at the top, alpha 255), as Geordi's ray-traced scene profile specifies. Width
and height are not part of it. The PNG file is only a container for those bytes; its own hash is
not a contract value.

**PNG encoder: hand-written, stored deflate blocks** (`crates/lumen/src/png.rs`). It writes
8-bit RGBA with filter type 0 on every row and the zlib stream as uncompressed stored blocks of
at most 65 535 bytes, one `IDAT` chunk each. Reasons:

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
image hash, on every run, every platform and every build profile, at every thread count and tile
size, whatever order the pixels and samples are computed in. Every value that decides a pixel is integer arithmetic from Bunny: the
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
  (camera, shadow and bounce) gets the answer of a test of every triangle; and every sample of a
  16 by 9 image equals an independent composition of the documented order of operations written
  in the test with brute-force queries. The camera uses sampler pair 0 and the bounce pair 1.
- **Order independence**: a pixel's samples accumulated forwards, backwards, shuffled and in
  merged partial accumulators give the same bits; an image whose pixels are computed in a
  scrambled order has the same hash.
- **Convergence** (a regression guard, not a proof): for four reference pixels the mean of 1024
  samples is within `5 / 256` of the mean of 256 per channel.
- **Two runs**: the same image rendered twice in one process, and by two separate `lumen render`
  processes, compared byte for byte along with the printed hash and ray count.
- **Thread counts and tile sizes**: see "Why the image does not depend on the threads" above.
- **Golden hash**: the 96 by 54 reference image at 4 samples per pixel and seed 0 has a recorded
  hash, together with the scene, integrator, sampler, shading, tone and encoding identities and the
  Bunny commit. It must be identical on every platform: it was first recorded on aarch64 Linux,
  and CI checks it on x86-64 Linux in both the test and release profiles. Updating it is
  legitimate only when one of those identities, the settings or the Bunny commit changes.

## Bunny dependency

Lumen uses Bunny's `bunny-num`, `bunny-linalg`, `bunny-geom`, `bunny-query`, `bunny-broadphase`
and `bunny-optics` at version 0.7.0. That version is tagged but not yet published on crates.io,
so the crates come in as git dependencies pinned to the full commit
`dcd6bd37613738de93f7084e7c8aa0283de964ea` of `https://github.com/flyingrobots/bunny`. They move
to the crates.io registry once 0.7.0 is published there. `BUNNY_REV` in `crates/lumen/src/lib.rs`
records the commit, and a test checks that `Cargo.lock` resolves every Bunny crate to it.

Dependencies build with `opt-level = 2` even in debug builds and tests, so the brute-force tests
and a default-size render take seconds; lumen's own code keeps the debug profile.

To move the pin, change the `rev` of every Bunny dependency in `Cargo.toml` and `BUNNY_REV`,
then regenerate `Cargo.lock` in the container and apply the exported patch:

```sh
python3 $D lumen ~/git/lumen --worktree --network --export-patch --out "$(mktemp -d)" -- \
  bash -c 'mkdir -p /tmp/ch && CARGO_HOME=/tmp/ch cargo update -p bunny-num -p bunny-linalg -p bunny-geom -p bunny-query -p bunny-broadphase -p bunny-optics'
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
```

`--fetch` is needed on the first run in a worktree; it downloads the Bunny git dependency, and
later phases build offline. Use `--commit HEAD` instead of `--worktree` for evidence pinned to a
commit.

Lint posture mirrors Bunny: `unsafe` is forbidden, clippy `all` is denied and
`pedantic` warns, and CI runs with warnings denied.

## License

Apache-2.0. See [LICENSE](LICENSE).
