# The built-in scenes

Reference for the two scenes built into lumen: every object in primitive order, its material,
the lighting, the camera, and what the tests check about each. `lumen render --scene NAME` and
`BuiltinScene` choose between them; [the command reference](cli.md) has the options.

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
shaded smooth ([smooth shading](../../README.md#smooth-shading-lumen-smooth-diffuse-path1)).

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
[smooth shading](../../README.md#smooth-shading-lumen-smooth-diffuse-path1)).

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
  new scene version and a new hash. Every platform in [Determinism](../../README.md#determinism) checks it.

