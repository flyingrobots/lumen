# ADR 0007: Smooth shading: the integrator's identity and the vertex normals

Status: accepted (flyingrobots/lumen#39); the `shading` field's rule amended before it landed
(see [The `shading` field follows the scene too](#the-shading-field-follows-the-scene-too)).

## Context

Fixed Point version 2 shades the full-resolution Stanford bunny with interpolated vertex normals,
which Bunny provides as `bunny-vertex-normals/1` (`VertexNormals::new`), `bunny-smooth-normal/1`
(`smooth_normal`) and `bunny-smooth-shading/1` (`SmoothHit`). The dice, the table and the glasses
stay flat, so one scene mixes flat and smooth triangles. Three things had to be decided: where
the normals live in the scene, what the integrator is called, and how the reference scene, which
has no normals, keeps `lumen-diffuse-path/1` and every pixel. The content digest's part is ADR
0005's version 2.

## Decision

### The normals: per triangle, as `SmoothHit::new` takes them

`SceneSpec::normals` is empty or holds one `[Option<FixedUnitVec3>; 3]` per triangle, in
primitive and vertex order, the exact argument of `SmoothHit::new`. `Scene::new` checks the
count and copies them; the integrator reads a triangle's entry with no further lookup.

Measured (`size_of`, the rust-dev:1.96.0 container): an entry is 96 bytes; a triangle already
costs the scene about 360 (a `TraceTriangle` of 72, an `Option<TriangleSurface>` of 224, about 56
of compact tree, its material and primitive indices). An indexed form (32 bytes per vertex normal
and three `u32` indices per triangle) would cost about 29 bytes per triangle on the full bunny
(0.52 vertices per face), saving about 4.8 MB on its 69 451 faces, at the price of a second index
space in `SceneSpec`, a check of every index and an indirection per hit. Lumen's triangles are not
indexed either (each holds its three positions), so the per-triangle form keeps one shape for
the whole scene.

`None` is a vertex without a normal: Bunny's `VertexNormals` gives one to a vertex whose faces
cancel, and `SmoothHit` falls back to the geometric frame for a triangle with any vertex missing.

### Which triangles are smooth

A triangle is shaded smooth exactly when at least one of its three normals is present; a
triangle whose entry is `[None; 3]`, or a scene with no entries, is shaded flat by the steps of
`bunny-diffuse-shading/1`, never through `SmoothHit`. So the flat path is the same code, call for
call, as before, and a scene with no normals cannot change a bit.

### The integrator's name follows the scene

`trace_path` is one function. Its identity, the provenance's `integrator` field, is chosen by the
scene (`Integrator::for_scene`, from `Scene::smooth_triangles`):

- `lumen-diffuse-path/1` (`INTEGRATOR_ID`) when no triangle is smooth;
- `lumen-smooth-diffuse-path/1` (`SMOOTH_INTEGRATOR_ID`) when at least one is, mixed scenes
  included. It is defined per triangle: `bunny-smooth-shading/1` at smooth triangles (its light
  sample, and its bounce, which may be rejected as below the surface: then no ray and `B = 0`),
  and `lumen-diffuse-path/1`'s steps everywhere else, in Bunny's order of operations.

The reference scene and Fixed Point version 1 therefore keep `lumen-diffuse-path/1`, their
golden image hashes and their provenance (but for the content digest, ADR 0005 version 2).

### The `shading` field follows the scene too

The `shading` field names the Bunny shading contracts the scene's paths actually use
(`Integrator::shading_id`), so that it is never false:

- a flat scene: `bunny-diffuse-shading/1`, exactly as before, so a flat scene's provenance,
  and the command's printed report of it, does not change by a byte;
- a scene with any smooth triangle: `SMOOTH_SHADING_IDS`,
  `bunny-diffuse-shading/1+bunny-smooth-shading/1+bunny-smooth-normal/1`.

**Form.** Bunny's identities, each exactly as Bunny's constant spells it (`SHADING_ID`,
`SMOOTH_SHADING_ID`, `SMOOTH_NORMAL_ID`), joined by `+` with no spaces, in the order they apply
at a smooth hit: the diffuse contract first (every hit's `SurfaceHit`, start point, shadow ray
and reflection, and every flat hit), then the smooth shading that wraps it, then the normal
interpolation `SmoothHit::new` calls. `+` appears in no Bunny identity, so the value splits back
into its parts. A test holds the string to Bunny's three constants, so a Bunny pin that versions
one of them fails until the value is updated, and the field stays one `Identity`: thirteen
fields, in their order, with nothing appended.

**`bunny-vertex-normals/1` is not in it.** Vertex normals are an input the scene supplies, like
its triangles: a scene may compute them with Bunny's `VertexNormals`, another rule or by hand,
and the render cannot tell. What decides pixels is their values, which the content digest
covers (ADR 0005, version 2). How a scene made them is part of the scene's own definition (its
versioned name), not of the render's shading.

## Consequences

- A front end reports the right integrator without knowing about normals: the provenance takes
  it from the scene, as it takes the content digest.
- Equal provenance still means equal bytes: the integrator and the content digest together say
  whether, and with which normals, a scene was shaded smooth.
- Giving every triangle its own face normal at all three vertices goes through `SmoothHit`
  everywhere and renders the flat bytes: Bunny's shared-normal rule returns the normal unchanged,
  the frame is the face's, and a cosine-weighted direction around the face normal is never below
  the face. A test shows it on the whole reference scene. On an exactly planar mesh, Bunny's
  `VertexNormals` gave every corner the face's normal bit for bit (150 of 150 corners, a plane of
  normal `(1, 5, 3)`), but Bunny bounds it only to a raw unit, so that is a measurement, not a
  promise.
- The memory cost is 96 bytes per triangle of a scene with normals, flat triangles included.

## Rejected

- **One identity for every scene** (`lumen-diffuse-path/2`, or the smooth name for all): it
  changes the reference scene's provenance while its bytes do not change, and it would name a
  smooth composition for a scene that has nothing smooth in it.
- **The `shading` field fixed at `bunny-diffuse-shading/1`** (this ADR's first form): every
  smooth scene's provenance would then omit the contracts that shade its smooth hits.
- **A provenance field for the smooth contract** (`smooth-shading`): it needs a value meaning
  "not used" for every flat scene and a fourteenth field in every report, to say what the
  integrator's identity already says.
- **Sending flat triangles through `SmoothHit` with no normals**: it falls back to the geometric
  frame, but costs a smooth hit (about 171 ns against 67 flat, Bunny's measurement) and makes the
  flat path depend on that fallback rather than being the flat code itself.
