# ADR 0010: Debug views: selection, identity, provenance and stability

Status: accepted (flyingrobots/lumen#16). The mechanism and the `normals`, `shading-normals`,
`depth`, `primitive-id`, `material-id`, `wireframe` and `bvh-cost` views are built. The other views the issue lists are not built yet; [Views not yet
built](#views-not-yet-built) says how each fits.

## Context

Lumen renders one picture, the lit image of the scene's integrator (`lumen-diffuse-path/1` or
`lumen-smooth-diffuse-path/1`, ADR 0007). When that picture looks wrong (a dark gash, a missing
shadow, a slow region), there is no way to look at the geometry, the normals or the tree on their
own. flyingrobots/lumen#16 asks for debug views selected by an option, each a deterministic
function of the scene and the settings, with its own identity, computed only from Bunny results,
going through the same scheduler, and each with golden hashes.

The issue predates most of the current code. These parts no longer fit it as written:

- It names `crates/lumen/src/render.rs` as the area. Rendering is now a module (`render/`) and
  the scheduler is `workers.rs` (ADR 0003). A view lives in its own module, `view/`, and calls the
  scheduler's public `render_pixels`.
- It asks for `lit` to "keep its golden hashes". Since ADR 0005 and ADR 0007 a render's identity
  is its provenance: the scene's content digest, the integrator and the shading contracts. So
  `lit` must keep its provenance as well as its bytes, and a view must be told apart from the lit
  image there, not only by its hash.
- It defers smooth-normal views until Bunny has interpolated normals (flyingrobots/bunny#276).
  Bunny now has them (`smooth_normal`, `bunny-smooth-normal/1`), so `shading-normals` is built
  here.
- `object-id` has no source: a lumen scene has triangles and materials, not objects. The view
  that exists is `material-id`.
- `bvh-cost` needed Bunny's traversal counters (flyingrobots/bunny#335). They exist now and lumen
  calls them (`CountingTracer`, ADR 0008). The view itself is not built yet.
- The issue's blocker, flyingrobots/lumen#4, is closed.

## Decision

### Selection

`View` is a closed enum, `#[non_exhaustive]` so that views can be added: `Lit` (the default),
`Normals`, `ShadingNormals`, `Depth`, `PrimitiveId`, `MaterialId`, `Wireframe`, `BvhCost`. `render_view(scene, view, size, settings, schedule)` renders one.
`View::Lit` is `render_scheduled` with the same arguments, the same function call, so the lit
image cannot drift from the default render. The command takes `--view NAME` (the names are
`View::name`: `lit`, `normals`, `shading-normals`).

### Identity and provenance

A view is a different picture of the same scene, so it is a different integrator.
`RenderProvenance::with_view(view)` returns a copy that records the view. `RenderProvenance::named`
and `new` record `View::Lit`, and `with_view(View::Lit)` changes nothing, so the lit provenance is
field for field what it was (tested). For any other view:

- `integrator` is the view's identity, `lumen-view-<name>/1`.
- `shading` names the Bunny shading contracts the view's pixels use. That is `none` for `normals`
  (the geometric normal is the triangle's surface normal, which is part of the scene), for
  `depth`, `wireframe`, `bvh-cost` and the ID views (their colours come from Bunny's hash, not
  from shading). For
  `shading-normals` it is `bunny-smooth-normal/1`.
- `samples` is 1 and `bounces` 0 (see below).
- Every other field is unchanged: the scene, its content digest, the sampler, the tone, the
  encoding, the Bunny commit, the size and the seed.

The provenance keeps its 13 fields and their order (ADR 0005). A view adds no field: the
`integrator` field already says what drew the pixels.

### Stability: view identities are versioned like integrators

A view's identity is a promise that the same scene, size and Bunny commit give the same bytes on
every platform, at every thread count and band height. That is the same promise the lit image
makes, checked the same way: a golden hash per built-in scene at 96 by 54, checked at 1, 2, 4
and 18 threads and band heights 1 and 3. Any change to what a view draws (a colour map, a
threshold, which normal) is a new version, `lumen-view-<name>/2`, and new golden hashes. A view
is a debugging tool, but it is versioned anyway: a picture that silently changes cannot be
compared with an older one, and comparing pictures is what the views are for.

### Samples: the first-hit views are single-sample

A first-hit view's value at a hit is an integer label or a direction, not a radiance. Averaging
several of them would make colours that belong to no triangle. Each such view therefore traces
one camera ray per pixel, through the pixel's centre (Bunny's `PixelSample::centre`). It does
not depend on the seed and does not call the sampler. `render_view` ignores the settings'
sample count for these views, and their ray statistics count one sample per pixel. The command
makes the rule visible instead of silent: a single-sample view defaults to `--samples 1`, refuses
any other value with a usage error, and refuses `--stats`, whose statistics describe the lit
image's paths. A view derived from the lit integrator (`gray-box`, `direct`, `indirect`) would
take `--samples` as `lit` does.

### What a view may compute

A view uses Bunny's results only: `PinholeCamera::ray`, the scene's closest-hit query (Bunny's
compact search and `TraceRay::hit_triangle`), `TriangleSurface`'s frame, `smooth_normal`. Lumen's
own arithmetic is the integer map from a value to a colour:

- **normals** (`lumen-view-normals/1`): the geometric normal (`TriangleSurface::frame().normal()`)
  as `(red, green, blue) = 128 + round(a * (x, y, z))`, rounded half up and clamped to a byte. The
  amplitude `a` is 127 for a front face (`TriangleHit::front_face`) and 63 for a back face. A
  front-face colour is 127 from grey along the normal and a back-face colour 63 from it, so no
  colour can be both: back faces are visibly dimmer and greyer. A miss is black, `(0, 0, 0)`,
  which no normal reaches.
- **shading-normals** (`lumen-view-shading-normals/1`): on a triangle with all three vertex normals,
  Bunny's `smooth_normal` of them at the hit's exact barycentric numerators
  (`TriangleHit::barycentric_numerators`), the normal `SmoothHit` shades with before it turns it
  toward the arrival side. It goes through the same map, with the geometric face side choosing the
  amplitude. Where the three normals cancel (`smooth_normal` gives none), the pixel is magenta,
  `(255, 0, 255)`, which no unit normal reaches. A triangle without all three normals shows its
  geometric normal, as Bunny's smooth shading falls back to it.

- **depth** (`lumen-view-depth/1`): the first hit's distance `d` (`TriangleHit::distance`, raw
  Q32.32, in multiples of the camera ray's direction) as grey `255 - floor(239 * min(d, R) / R)`:
  white at the eye and grey 16 at the range `R` and beyond, so a far hit is never black, a miss.
  `R` is the largest difference, on any one axis, between the eye and a corner of the scene's box
  (`Scene::bounds`, the tree's root box). A camera ray's direction has no component above 1, so
  `d` is at least a hit's distance from the eye on every axis. The range is the scene's own: in a
  scene much larger than its subject (Fixed Point's table runs far behind the bunny), the subject
  sits near the dark end.
- **primitive-id** (`lumen-view-primitive-id/1`) and **material-id** (`lumen-view-material-id/1`):
  64 bits of Bunny's `bunny-sample-hash/1`, `PixelStream::new(seed, index, 0).bits(0, 0)`, of the
  hit's primitive index or of its triangle's material index (`Scene::material_index`). The seed
  is `0x7072696D69746976` (the ASCII "primitiv") for triangles and `0x6D6174657269616C`
  ("material") for materials, so a triangle and a material with the same index get unrelated
  colours. The three low bytes become red, green and blue, each `32 + byte * 224 / 256`, so no ID
  colour is black, a miss. Two indices can share a colour (24 bits of hash); a colour never
  depends on anything but the index.

- **wireframe** (`lumen-view-wireframe/1`): the hit's exact barycentric numerators
  (`TriangleHit::barycentric_numerators`: `u`, `v` and `det`, the weights being `(det - u - v) /
  det`, `u / det` and `v / det`). A hit is on an edge when its smallest weight's numerator times
  32 is below `det`, that is a weight below 1/32. It is exact in `i128`, since each value is below
  `2^117`. Edges are white, faces dark grey `(64, 64, 64)` and misses black. The band is a fraction
  of each triangle, so a dense mesh is mostly edge.
- **bvh-cost** (`lumen-view-bvh-cost/1`): the camera ray's traversal work, `box_tests +
  primitive_tests` from Bunny's counted search (`CountingTracer::closest_counted`, ADR 0008), for
  every pixel, the sky included (a ray that hits nothing still searches). The heat ramp: with
  `t = min(work, 512)`, `s = t / 128` and `f = 2 * (t % 128)`, the colour is black to blue
  `(0, 0, f)`, blue to red `(f, 0, 255 - f)`, red to yellow `(255, f, 0)`, yellow to white
  `(255, 255, f)`, and white from 512. The ramp is the same for every scene, so pictures of two
  scenes compare directly, and the colour can be read back as work.

### The same scheduler

A view's pixel function goes to `render_pixels`, the scheduler the lit image uses (ADR 0003). The
pixel is a function of the scene, the size and its coordinates alone, so the image is the same
bytes for every schedule.

## Views not yet built

Each of these fits the mechanism above without changing it: a variant, an identity, a colour map,
golden hashes and hand-derived tests.

- `gray-box`, `direct`, `indirect`: the lit integrator with one input changed. These are
  multi-sample and keep `bunny-diffuse-shading/1` in `shading`.

## Consequences

- The crate root gains `View`, `ParseViewError`, `render_view` and each view's identity constant
  (`NORMALS_VIEW_ID`, `SHADING_NORMALS_VIEW_ID`, `DEPTH_VIEW_ID`, `PRIMITIVE_ID_VIEW_ID`,
  `MATERIAL_ID_VIEW_ID`, `WIREFRAME_VIEW_ID`, `BVH_COST_VIEW_ID`). `Scene` gains `bounds` and `material_index`. `RenderProvenance` gains `with_view` and `view`.
- The command gains `--view NAME` and three usage errors (an unknown view, `--samples` other than
  1 with a debug view, `--stats` with a debug view).
- The default output, the lit provenance and every existing golden hash are unchanged: `lit` is
  the old render by construction, and the tests check the provenance field for field and the
  golden hash through `render_view`.

## Clarification, 2026-11-17: ADR 0016

ADR 0016 moves graphics evaluation, integrators included, to Bunny. It keeps this decision as an
explicit exception, unchanged; see its section "Exceptions, with their reasons".
