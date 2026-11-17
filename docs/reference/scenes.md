# The built-in scenes

Reference for the three scenes built into lumen: every object in primitive order, its material,
the lighting, the camera, and what the tests check about each. `lumen render --scene NAME` and
`BuiltinScene` choose between them; [the command reference](cli.md) has the options.

## The reference scene

`lumen-reference-scene/4` is built in `crates/lumen/src/reference/`, entirely in Q32.32 integers
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
   face order, standing on the ground left of the sphere and facing the camera, about 0.96 units
   wide, 1.21 tall and 1.24 deep; albedo `(70, 165, 85)`, green.

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

The primitive order matters: of two triangles hit at the same distance, the lower index wins. **Any
change to the scene's triangles, their order, their materials, the lighting or the camera placement
is a new scene version, and changes every image hash.** Version 3 (`lumen-reference-scene/3`: the
same scene with the bunny turned half a turn, facing the sphere, so the camera saw it in profile) is
retired. Version 2 (`lumen-reference-scene/2`: the first four objects, the same materials and
lighting, and the camera at `(0, 1, 5)`) is retired. Version 1 (`lumen-reference-scene/1`: the same
triangles and camera with no materials or lights, shaded by the normal-colour debugging view
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
`crates/lumen/src/mesh/placement.rs`): every vertex is turned a quarter turn about `+Y` (`(x, y, z)`
to `(z, y, -x)`, exact; the file's bunny faces `-X`, so it turns to face `+Z`, the camera), scaled
by 8 (a shift of three bits, exact), floored to the `2^-16` grid like the sphere, and moved by whole
grid steps: `(-2.22845458984375, -1.392303466796875, 0.3791046142578125)`, which keeps the centre of
the bunny's box within a grid step of where version 3 had it. The Y offset puts the lowest vertex
(file vertex 1 748, `y = 0.0334143`) exactly on the ground plane, so the bunny stands on the ground
rather than a hair above it; the touching surfaces are handled by Bunny's start-point rule. Placed,
the bunny spans `x = -2.7218..=-1.7607`, `y = -1.125..=0.0862` and `z = -0.1084..=1.1340`. A face
that has zero area once placed would be dropped and listed (`PlacedMesh::dropped`) so that no
degenerate triangle reaches Bunny's `TriangleSurface`; for this file and placement none is (the
smallest face keeps about `2.2e-5` square units), and a test asserts the count.

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
shaded smooth ([smooth shading](../explanation.md#smooth-shading-lumen-smooth-diffuse-path1)).

## The Fixed Point scene

`lumen-fixed-point-scene/2` (`crates/lumen/src/scenes/fixed_point/`) is a showpiece: the
full-resolution Stanford bunny, smooth-shaded, in dark sunglasses at a craps table, with two identical dice and the ON puck on the 8. In
craps the point is the number the shooter must roll again, and the ON puck marks it. Both dice
show four pips on every face, so every roll is a hard eight and the point is fixed: a picture of
determinism, from a ray tracer built on fixed-point arithmetic.

Render it with the command (`--scene fixed-point`):

```sh
cargo run --release -p lumen -- render --scene fixed-point --width 1280 --height 720 --samples 64 --out fixed-point.png
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
cargo run --release -p lumen --example fixed_point -- fixed-point.png 1280 720 64
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
[smooth shading](../explanation.md#smooth-shading-lumen-smooth-diffuse-path1)).

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
  new scene version and a new hash. Every platform in [Determinism](../explanation.md#determinism) checks it.

## The canyon scene

`lumen-canyon-scene/1` (`crates/lumen/src/scenes/canyon/`) is a large-triangle-count scene and a
picture: a fractal canyon of integer terrain, three small craft of lumen's own design flying
along it in a chase, the full-resolution Stanford bunny on a ledge, a low sun, a near-black sky
and a banded planet. Composition after a 1990s canyon-chase render; the
[README](../../README.md#the-three-built-in-scenes) shows its render. The library builds it with
`lumen::canyon_scene()`; `canyon_content()` returns its
566 005 triangles in primitive order (the terrain, each craft's hull and engines, the bunny, the
planet's bands) with the part each belongs to, its material and its vertex normals. Every
coordinate is a whole number of `2^-16` grid steps.

Render it with the command (`--scene canyon`):

```sh
cargo run --release -p lumen -- render --scene canyon --width 1280 --height 720 --samples 64 --out canyon.png
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
the grid; a `Rational` is built only by `Rational::new`, which refuses a denominator that is not
positive or numerators off the rule `cos^2 + sin^2 = hyp^2`, and `Pose::place` refuses a point
whose posed coordinates leave `i64`, both with a `PoseError`): the skiff (a hexagonal hull with a pointed nose, straight swept wings and a fin,
orange) high at `(-2.5, 7, -10)`, banked by `-atan(7/24)` and turned by `atan(7/24)`; the tug (a
wedge tapering to a narrow nose between two pontoons joined by a short wing, cream) at
`(-7.5, 5.5, -15)`, banked and turned by `-atan(7/24)`; and the kite (a flat diamond wing with
two engine pods, teal) leading low at `(-5.5, 4, -24)`, banked by `atan(7/24)`. `push_craft`
appends them to a `CanyonContent` in that order, each its hull then its engines
(`CanyonError::Craft` if a corner does not pose; a test shows every one does). Tests
(`crates/lumen/tests/canyon/pose.rs`): the rotations admitted and refused on triples worked by
hand, every craft's rotation built by `Rational::new`, points turned, floored and moved by hand,
and a point whose coordinates would leave `i64` refused.

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
large test that runs in the release profile only (tens of seconds, by machine; the test profile skips it
with `#[cfg_attr(debug_assertions, ignore = ...)]`, and CI runs both profiles); and a golden
hash: 96 by 54 pixels, 4 samples per pixel, seed 0,
`sha256:019fcdddaaff8761f3f6e64dfe28d1ce7d2b07200b8c69663345ca4a495466c5`, under the same rules
as the other scenes', the same at every thread count and band height (`tests/threads.rs`). The
WebAssembly job runs this golden test on its own (its own step, in both profiles), not the rest of
the canyon's test binary: the brute-force test would take far too long there.

At 3840 by 2160 and 16 samples it renders in about 14.5 seconds with 18 CPUs, 2.7 times the
reference scene's time per ray: [the measurement record](../measurements/2026-11-14-canyon.md)
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

### Canyon terrain: `lumen-canyon-heightfield/1`

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

`HeightfieldSpec::bounds` states the range every height lies in and `HeightfieldSpec::centre_256`
the centre line of a row; `generate` refuses a spec outside its stated ranges
(`HeightfieldError`, naming the parameter) before computing anything, and so do `bounds` and
`centre_256`, with the same error. Those ranges keep every product inside `i64`. Any change to the steps, their constants or
their order is a new version of the generator.

Tests (`crates/lumen/tests/canyon/heightfield.rs`): exact values on tiny grids, from SplitMix64's
published first output and from an independent Python model of the documented algorithm (a 3 by
3 grid, a hand-worked profile, and the sum, weighted sum, extremes and two vertices of a 64 by 96
canyon with every step in use); determinism; every height within the stated bounds; the channel
(the floor at the centre line below every plateau vertex of its row by a stated margin); a wall
smooth to within its slope where the centre line bends; the shelf's rule; the bounds worked by
hand where the plateau's weight is the largest and where the spread is not a whole step; no
height past the grid's edge. `crates/lumen/tests/canyon/limits.rs`: each refusal, every
`HeightfieldError` variant but `OutOfMemory` (which needs the allocator to fail), reached by a
constructed spec and given alike by `generate`, `bounds` and `centre_256`; a spec exactly at
each limit accepted; and the bounds worked by hand at the largest spec the ranges allow. Seeded
mutations of the noise blend, the carve's floor width, the centre line's fraction, the shelf's
rule, the bounds' weight and floor, the grid's column check and the helpers' range check each
fail at least one of them.
