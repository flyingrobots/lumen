# Lumen

A deterministic ray tracer: a [Geordi](https://github.com/flyingrobots/geordi) runtime built on
[Bunny](https://github.com/flyingrobots/bunny)'s fixed-point math. The same scene renders to the same bytes on
every machine.

Work is tracked in GitHub Issues with `issue-dag/2` frontmatter.

## What lumen does today

Lumen renders one built-in reference scene, which includes the Stanford bunny, to a PNG file:

- one thread;
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

It does not yet use more than one thread (flyingrobots/lumen#4) or read scenes from files, such
as a Geordi `geordi-ray-traced-scene/1` artifact (#6). Materials other than diffuse, more than one
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
scene lumen-reference-scene/3
integrator lumen-diffuse-path/1
shading bunny-diffuse-shading/1
sampler bunny-sobol-owen/1
tone bunny-tone-srgb/1
encoding bunny-rgba8-opaque/1
bunny dcd6bd37613738de93f7084e7c8aa0283de964ea
size 640x360
samples 64
seed 0
bounces 1
rays <total> (closest <camera and bounce rays>, shadow <shadow rays>)
image sha256:<64 hex digits>
png /scratch/out/reference.png (922266 bytes)
render-seconds <time> (information only)
ns-per-ray <time> (information only)
```

Usage: `lumen render --out FILE [--width W] [--height H] [--samples N] [--seed S]`; the
defaults are 320 by 180, 4 samples per pixel and seed 0. Width and height are each `1..=32768`
(the limit of Bunny's camera) with at most `2^27` pixels in all (a 512 MiB RGBA8 buffer, enough
for 16K at 15360 by 8640). The sample count must be a power of two from 1 to 1024: Bunny's
sampler states its stratification for aligned power-of-two blocks of samples, and its
accumulator takes at most 1024. The seed is `0..=4294967295`, the domain of Geordi's render
settings. Arguments are read as OS strings, so an output path that is not valid Unicode is
used as given; a non-Unicode number or option is refused like any other bad argument. Bad
arguments exit with status 2 and the usage text; a refused size or an unwritable output path
exits with status 1 and a message. The output file is written after the render, so nothing is
written when the render fails.

The PNG is uncompressed (see below), so it is about four bytes per pixel: 922 266 bytes at
640 by 360, and 33 188 430 bytes (about 33 MB) at 3840 by 2160.

## The reference scene

`lumen-reference-scene/3` is built in `crates/lumen/src/scene.rs`, entirely in Q32.32 integers
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

**Reading it: an interim, `lumen-ascii-ply-q32.32/1`.** Lumen's rule is that numerical code
belongs in Bunny, but Bunny's PLY reader handles only binary little-endian files with exactly
`x y z` vertices, and this file is ASCII with two more vertex columns. Until Bunny has an ASCII
reader (flyingrobots/bunny#247), `crates/lumen/src/ply.rs` reads exactly this file's profile and
refuses anything else with a typed error (`PlyError`, `DecimalError`): a header of exactly
`format ascii 1.0`, `element vertex`, the properties `x y z confidence intensity` as `float`,
`element face` with `property list uchar int vertex_indices`, then five numbers per vertex line,
`3 a b c` per face line with every index in range, `\n` line endings, and nothing after the last
face. `confidence` and `intensity` are checked but not used. Each coordinate
`[-]digits[.digits][(e|E)[+|-]digits]` is converted with integers only: the digits form a mantissa
`m` and the point and exponent a power of ten `e`, and the Q32.32 raw value is the exact rational
`m * 2^32 * 10^e` rounded to the nearest integer, **ties to even**, computed in `u128`. The file
has one coordinate in exponent form, `8.20736e-05`.

**Placing it** (`crates/lumen/src/mesh.rs`, `BUNNY_PLACEMENT`): every vertex is turned half a turn
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
the per-pixel function holds no shared mutable state, so rendering tiles on threads
(flyingrobots/lumen#4) is a change of schedule only.

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
image hash, on every run, every platform and every build profile, whatever order the pixels and
samples are computed in. Every value that decides a pixel is integer arithmetic from Bunny: the
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
- **Convergence** (a regression guard, not a proof): for six reference pixels the mean of 1024
  samples is within `5 / 256` of the mean of 256 per channel.
- **The bunny**: the asset's SHA-256; exact decimal conversion (hand-derived raw values for the
  first vertex, the bounds and the file's one exponent, exact ties, and 20 000 generated decimals
  checked against the definition of rounding to nearest); the converted mesh's bounds equal the
  exact conversion of the manifest's bounds; the PLY reader's refusals (a binary format line, a
  missing property, a four-index face, an index out of range, a truncated file, a second decimal
  point and more), each with its typed error; the placed bounds derived by hand, with the lowest
  vertex exactly on the ground; no dropped and no degenerate triangle; the winding and hole
  counts; a camera ray through the bunny's pixels hits it; and a ground point behind it, derived
  from the light direction through a point inside its body, is in its shadow while a nearby point
  in front is lit.
- **Two runs**: the same image rendered twice in one process, and by two separate `lumen render`
  processes, compared byte for byte along with the printed hash and ray count.
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

The Stanford Bunny mesh in `assets/stanford-bunny/` is not covered by that license: it is from the
Stanford 3D Scanning Repository, Stanford Computer Graphics Laboratory, and is used with
attribution. See `assets/stanford-bunny/README.md`.
