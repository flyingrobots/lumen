# Lumen

A deterministic ray tracer: a [Geordi](https://github.com/flyingrobots/geordi) runtime built on
[Bunny](https://github.com/flyingrobots/bunny)'s fixed-point math. The same scene renders to the same bytes on
every platform CI checks: x86-64 and aarch64 Linux, aarch64 macOS, x86-64 Windows and
WebAssembly (see [Determinism](#determinism)).

Work is tracked in GitHub Issues with `issue-dag/2` frontmatter.

## What lumen does today

Lumen renders a built-in scene, prints its image hash and timing, and writes it to a PNG file
if asked. `--scene reference` (the default) is the reference scene, which includes the Stanford
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
bunny 3c0dab3e77c0d45de8904906a134e846dfe5ec6c
size 640x360
pixels 230400
samples 64
seed 0
bounces 1
scene-content sha256:84541be146ebac194b43dfc91f9205356818d517c71f52d947a3ca417178d356
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

Usage: `lumen render [--scene reference|fixed-point] [--out FILE] [--width W] [--height H]
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
arguments exit with status 2 and the usage text; a refused size or an unwritable output path
exits with status 1 and a message. The output file is written after the render, so nothing is
written when the render fails. `--threads` is `1..=256` (0 and anything larger are refused with
status 2); `--band-rows` is the number of whole image rows a worker takes at a time, at least 1
(0 is refused with status 2). There is no `--tile` option any more; it is refused as unknown.

The PNG is uncompressed (see below), so it is about four bytes per pixel: 922 266 bytes at
640 by 360, and 33 188 430 bytes (about 33 MB) at 3840 by 2160.

## Library and command

The crate has two targets. The library (`crates/lumen/src/lib.rs`) renders; the `lumen` command
(`crates/lumen/src/bin/lumen/`) is a front end over it.

- **The library's API is the crate root.** Every type and function a front end, a `Tracer` or a
  test needs is re-exported there: scene building (`Scene`, `SceneSpec`, `CameraPlacement`,
  `Lighting`, `reference_scene`, `fixed_point_scene`, and `BuiltinScene`, the registry of
  named scenes with their identities that the command's `--scene` reads), rendering (`render`, `render_scheduled`, `render_traced`,
  `Schedule`, `BandRows`, `RenderSettings`, `ImageSize`), the result (`Render`, `Rgba8Image` and
  its `hash`), its provenance (`RenderProvenance`), PNG encoding into any writer (`write_png`,
  `png_len`), and the integrator's parts for custom tracers (`trace_path`, `pixel_radiance`,
  `Tracer`, `Hit`, `PrimitiveIndex`, `TraceError`). Only `lumen::reference` and `lumen::scenes` (the scenes'
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
  SHA-256 of a canonical byte encoding (`lumen-scene-content/1`) of the `SceneSpec` it was built
  from: the triangles in primitive order, each one's material index, the albedos, the light
  (present or absent), the sky and the camera placement. It is computed once in `Scene::new`,
  before the tree is built, and does not depend on the tree, the threads, the band height or the
  platform; the image size, samples and seed are separate provenance fields, and a scene's name
  is a label beside it, not part of it. It identifies the input, not the picture: moving a
  triangle no ray reaches changes the identity and no pixel. The encoding is specified byte by
  byte in [ADR 0005](docs/adr/0005-scene-content-identity.md), and a test rebuilds it by hand
  from that table. `RenderProvenance` takes it from the scene, so provenance can no longer
  describe a different scene under the same name. The built-in scenes' digests are golden
  values in `crates/lumen/tests/golden/mod.rs`, beside their image hashes, and one more, for a
  one-triangle scene, was computed outside lumen with Python's `hashlib`.
- **The command owns everything at the edge**: argument parsing and the usage text (`cli.rs`,
  `cli/usage_error.rs`, with its tests in `cli/tests.rs`), the default thread count from
  `available_parallelism`, the clock, the output file and the terminal (`main.rs`, `report.rs`).

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

The scene's types (`SceneSpec`: triangles, a material index per triangle, diffuse albedos, an
optional directional light and a sky; `RenderSettings`: a power-of-two sample count and a `u32`
seed; at most one bounce) follow Geordi's ray-traced scene profile, so that loading an artifact
(flyingrobots/lumen#6) maps onto them.

## The Fixed Point scene

`lumen-fixed-point-scene/1` (`crates/lumen/src/scenes/fixed_point/`) is a showpiece: the Stanford
bunny in dark sunglasses at a craps table, with two identical dice and the ON puck on the 8. In
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
720 render at 64 samples traces 132 650 051 rays and took 8.1 s with 4 threads (Apple M5 Pro, `rust-dev:1.96.0` container limited to 4 CPUs, run alone with `--exclusive`, release profile; one reading, for information); its image hash, `sha256:e638f32442d90d5fb5bc8212044856433b1f4998a5d17ca549370747313d253c`, is the same at 1 thread (29.9 s).

**What it is made of.** Triangles only, 5 975 of them, every coordinate a whole number of
`2^-16` grid steps; a die is about half a unit wide. In primitive order:

1. the table: the green felt (`x = -7.5..=7.5`, `z = -12..=3.25`, at `y = 0`, 2 triangles); a
   padded rail swept round it with mitred corners (0.375 high at the felt, 0.75 at its crown,
   1.125 wide, 56 triangles); white layout lines 1/16 wide, painted `2^-10` above the felt (the
   number boxes' outlines and two long lines, 22 triangles); and a row of six number boxes, each
   1.75 wide and 3 deep, for 4, 5, 6, 8, 9 and 10, each with a seven-segment block numeral
   (cells 0.6875 wide and 1.375 deep, strokes 0.1875, 10 in two cells) in the box's near half,
   reading upright from the camera;
2. the puck in the far half of the 8 box, clear of the 8: a 24-sided disc of radius 0.5 and
   height 3/16, a white face of radius 0.4 inside a black rim, and "ON" in black block letters
   (134 triangles);
3. the two dice, each a chamfered cube about 0.5 wide (32 800 grid steps, chamfer 2 625) with
   four raised white 12-sided pips on every face (908 triangles each): the same mesh under two
   rigid motions, the left die turned by `atan(3/4)` (about 36.9 degrees) and the right one by
   `-atan(7/24)` (about -16.3 degrees) about the vertical. Every coordinate of the die's mesh
   is a multiple of 25 grid steps, so both turns are exact: nothing is rounded. Raised pips
   mean each die rests on the four pips of its bottom face;
4. the Stanford bunny (3 851 triangles, none dropped): turned a quarter so it faces the camera,
   scaled by 32 (about 4.85 units tall, nearly ten dice), floored to the grid with its lowest
   vertex exactly on the felt, then turned by `-atan(5/12)` (about 22.6 degrees, toward the
   camera's right) about its nose, each coordinate floored to the grid, so it stands
   three-quarters on behind the number boxes with its tall ear and its back in profile;
5. the sunglasses (22 triangles): two flat wraparound wayfarer lenses, a bridge and two arms in
   near black. They are fitted by measurement: the nose tip is the vertex farthest along the
   facing direction in the upper half of the bunny's height, the lenses' centre line is 0.5
   above it, their inner edges 1/32 in front of it, and each lens slopes back by a quarter of
   its distance from the centre line; the arms run back along the head, the left one outside
   the base of the swept-back ear. The glasses are built facing the camera and then turned with
   the bunny.

Materials (linear albedo in 256ths): felt `(14, 62, 32)`, leather `(45, 22, 14)`, layout paint
`(225, 225, 215)`, die red `(170, 12, 16)`, pips and puck face `(240, 240, 232)`, puck rim and
letters `(10, 10, 10)`, the bunny's light warm grey `(205, 200, 192)` and the glasses
`(6, 6, 8)`. The sun comes from high on the camera's left front, toward `(-0.5, 1.0, 0.875)`
(about 45 degrees up), irradiance `(1024, 985, 920) / 256`, so it lights the bunny's face and
the dice and the bunny cast their shadows back and to the right across the felt; the sky is a
dim `(16, 16, 18) / 256`, a dark room behind the table. The camera stands low over the near rail
at `(0, 2.125, 4.25)`, looks along `(0, -0.140625, -1)` with up `+Y` and a vertical half-angle
whose tangent is `15/32`, framed for 16:9: the ear tip near the top, the puck and the 8 in the
middle, the dice near the bottom.

Lumen renders diffuse surfaces with one sun and a sky, so the lenses are plain black. Mirror
lenses, a glass of whisky on the rail and a lamp over the table wait for the materials and
lights they need. Shadowed fur is tinted green: that is the one-bounce light from the felt.

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
- every camera, shadow and bounce ray of a 32 by 18 image at 2 samples equals a test of every
  triangle, with hits on the bunny, the dice and the layout;
- a golden hash: 96 by 54 pixels, 4 samples per pixel, seed 0,
  `sha256:0688b03cf16be6d0336f17eacbdaf83fe03d62e9c66b8370e8208bb9c879634e`, under the same
  rules as the reference scene's: it changes only with an identity, the settings or the Bunny
  commit. Any change to the scene's triangles, their order, materials, lighting or camera is a
  new scene version and a new hash. Every platform in [Determinism](#determinism) checks it.

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
  returns the typed error of the first failing pixel in raster order, top row first: the error a
  one-pixel-at-a-time render in raster order meets, at every thread count and band height. When
  a pixel in row `y` fails, no worker begins a row from `y` down: every pixel before it is in an
  earlier row, or earlier in row `y`, which only the failing worker renders. No worker panics or
  waits on another; a panic in the pixel function reaches the caller after every worker stops.
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
simulation that does not finish. `crates/lumen/tests/threads.rs` then runs real threads at fixed
counts:

- the golden 96 by 54 hash at 1, 2, 3, 4, 7 and 18 threads, each with bands of 1, 2, 4, 5, 6, 7,
  16, 27, 54 and 200 rows, with the same ray count as a raster-order loop over `pixel_radiance`
  that does not use the scheduler;
- a 320 by 180 image at 8 samples: one thread equals that raster-order loop byte for byte, and 18
  threads with one-row bands, 5 threads with bands of 7 and 16 threads with bands of 32 give the
  same image and ray count;
- errors, each test bounded by a count of work taken (pixels asked for, traversal queries) that
  fails it if exceeded, never by a timeout: an injected pixel function that fails at chosen
  pixels returns the raster-first one at every thread count and band height, also when it is not
  the first in column-major order (`(40, 0)` before `(3, 5)`); on one thread a failure asks for
  exactly the pixels a raster-order loop asks for, up to and including the failing one, at every
  band height (on several threads, how much runs before the failure is recorded depends on
  scheduling, so the simulator checks the stop rule instead); and a tracer that fails real
  traversals in two regions with two different errors returns the one a raster-order loop over
  `trace_path` meets first, at every schedule.

Each of these mutations makes tests fail in the release profile (checked by hand, each mutant's
test run under a 120-second `timeout` as a guard, sources touched before every build so that no
stale build is tested): numbering a band's rows from one past its first row (pixels
written at the wrong row offset); dropping the last partial band; choosing the first failure by
`(x, y)` instead of `(y, x)`; dropping the calling worker's ray count; and ignoring the stop row after a failure.

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
Each states its commit and conditions and applies to that commit only.

### Readings

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

## Bunny dependency

Lumen uses Bunny's `bunny-num`, `bunny-linalg`, `bunny-geom`, `bunny-query`, `bunny-broadphase`,
`bunny-optics` and `bunny-codec` (with default features off, so it needs no crates.io crate) at
version 0.7.1. The crates come in as git dependencies pinned to the full commit
`3c0dab3e77c0d45de8904906a134e846dfe5ec6c` of `https://github.com/flyingrobots/bunny`, a commit of Bunny's
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
    --test fixed_point
```

`--fetch` is needed on the first run in a worktree; it downloads the Bunny git dependency, and
later phases build offline. Use `--commit HEAD` instead of `--worktree` for evidence pinned to a
commit.

Lint posture mirrors Bunny: `unsafe` is forbidden, clippy `all` is denied and
`pedantic` warns, and CI runs with warnings denied.

### Supply-chain, size and coverage checks

`.github/workflows/gates.yml` runs four jobs on every pull request:

| Job | What it checks | Fails when |
|---|---|---|
| `Gates / cargo-deny` | `cargo deny --locked check` against `deny.toml` | an advisory, a yanked crate, a licence other than Apache-2.0, a second version of a crate, a wildcard version, a registry other than crates.io, a git repository other than `https://github.com/flyingrobots/bunny`, or a git dependency not pinned by `rev` |
| `Gates / cargo-audit` | `cargo audit --deny warnings` on `Cargo.lock` | a RustSec advisory or warning for a locked crate |
| `Gates / file length` | `cargo run -p xtask -- file-length` | a source file over 300 lines of code without a listed exception, or over 500 at all; a test file (under `tests/` or named `tests.rs`) over 750; a listed file grown past its exception; an exception no longer needed |
| `Signal / coverage` | `cargo llvm-cov --workspace --summary-only`, in the job summary | never: coverage is reported, not gated |

A line of code is a line with a character outside a comment; doc comments are comments. The
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
`CHANGELOG.md` under `Unreleased`.

### Tests

- **Size classes** (testing standard rule 9). Every integration test file says its class in the
  first line of its module comment, `//! Size class: small|medium`. Unit tests in `src/` are small.
  *Small*: in one process, no file, no network, no clock. The renderer's own worker threads are
  allowed because they are the code under test. *Medium*: spawns the `lumen` process, writes
  files, or runs real threads at fixed counts. Run one class by file: `cargo test --test cli
  --test threads` is the medium set; `cargo test --lib --test render --test library --test
  fixed_point` the small set. There is no large class yet.
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
