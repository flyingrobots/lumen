# ADR 0011: Analytic surfaces, point lights and the thin lens

Status: accepted (flyingrobots/lumen#183), decided 2026-11-15.

## Context

A Lumen scene holds triangles, one directional light, a sky and a pinhole camera. The planned
work adds six analytic shapes (sphere, box, capped cylinder, capped cone, torus, disc), point
lights and a thin-lens camera. Each needs arithmetic that can change a pixel: a ray hit with a
shape, the shading at a point light, a ray through a lens. Each also needs a place in the scene
model, its content identity and its statistics.

Lumen's documents say each sample is built from Bunny's contracts with no arithmetic of Lumen's
own. The camera, the lights, the shading and the ray-triangle test are all Bunny's today (ADR 0002,
ADR 0007). A render's provenance names the Bunny contracts that produced it, so a hit test written
in Lumen would have no such name. The question is where the new code lives.

## Decision

### Arithmetic lives in Bunny

Every computation that can change a pixel is a versioned contract in Bunny: the ray hit for each
shape, the thin lens and the point light. Lumen adds none. Each contract has an identity that
the render's provenance names, so a change to the arithmetic is a new identity, not a silent
change of bytes.

The contracts planned, each in the Bunny issue that builds it:

| Contract identity | Computes | Bunny issue |
| --- | --- | --- |
| `bunny-shape-frame/1` | a trace ray into a shape's object frame, a hit normal back to world space | #357 |
| `bunny-ray-sphere/1` | nearest hit distance and outward normal of a sphere | #358 |
| `bunny-ray-box/1` | entry (or, from inside, exit) distance and face normal of an axis-aligned box | #359 |
| `bunny-ray-cylinder/1` | hit with a capped cylinder: side and two caps | #360 |
| `bunny-ray-cone/1` | hit with a capped cone: side and base | #361 |
| `bunny-ray-torus/1` | hit with a torus: the quartic solved in integers by interval isolation | #362 |
| `bunny-ray-disc/1` | hit with a disc: plane hit, then an exact radius test | #363 |
| `bunny-thin-lens/1` | a ray from a lens-disc point through the pixel's focus-plane point | #364 |
| `bunny-point-light/1` | irradiance at a hit with inverse-square falloff, and a shadow ray bounded at the light | #365 |

These names are the plan, taken from the issues' scope. Bunny's issues fix each contract's exact
behaviour and rounding; if one changes its name, that issue changes it, and this table is then
corrected by a dated clarification.

The same rule covers the material and texture work (flyingrobots/bunny#366 to #368 and #370 to
#372: the sRGB decode, bilinear texels, normal mapping, mirror, GGX and dielectric samples). Their
scene types and ADRs are not this record's subject; each states its own decision under the rule
made here.

### Lumen owns the scene model

Lumen owns what a scene is, how it is stored, how it is hashed, file formats and threading.

- **One list of surfaces.** `SceneSpec` holds a surface enum, with a triangle kind and a kind for
  each analytic shape, in one ordered list. Primitive order stays the tie-break rule among hits at
  equal distance (ADR 0005).
- **One tree over all of them.** The tree is built over the bounding boxes of every surface, and
  the leaf dispatches on the kind to the Bunny hit for that shape. The brute-force oracle that
  tests the tree is extended to each kind.
- **Lights and lens.** Lighting becomes an optional sun, the sky and a list of point lights, one
  shadow ray per light; `CameraSpec` gains an optional lens. The built-in scenes keep the pinhole
  and have no point lights.
- **Statistics** count surfaces by kind, and lights (ADR 0008).

Which task owns which scene type:

| Scene type | Lumen issue |
| --- | --- |
| The surface enum, the shared tree, tagged content records, sphere | #187 |
| Box | #188 |
| Capped cylinder | #189 |
| Capped cone | #190 |
| Torus | #191 |
| Disc | #192 |
| Point lights | #193 |
| Thin lens | #194 |

All of it lives in `crates/lumen/src/scene/`, beside the existing tree, content identity and
statistics modules.

### Content identity: `lumen-scene-content/3`

ADR 0005's encoding lists triangles with no kind. Once a scene holds several kinds, the encoding
needs a record per surface that says which kind it is. Version 3 therefore writes tagged records:
a kind tag first, then that kind's fields, as raw Q32.32 values. Lights and the lens enter the
encoding the same way, as tagged records, in the tasks above. The encoding's name and version stay
the first bytes, for domain separation.

Triangles-only scenes keep a stated relation to their version 2 identity; #187 states it and tests
it. The built-in scenes do not change in the pull requests of this plan. This record does not fix
the byte layout of version 3: #187 does, and a dated clarification here records it.

## Alternatives rejected

| Alternative | Why not |
| --- | --- |
| Write the hit tests, lights and lens in Lumen | Fewer pull requests, but it ends the "no arithmetic of Lumen's own" property, and a hit test would have no contract identity to put in the provenance. |
| A third crate or repository for rendering math | One more thing to pin and gate, with no reader who needs it apart from Bunny. |
| Put the scene model in Bunny as well | Bunny is a math library. Scene storage, identities and file formats are a renderer's business. |
| A separate list and tree per kind | Every ray would be traced once per kind. |

## Consequences

- Each feature takes two pull requests, one in each repository, and a move of Lumen's Bunny pin.
  The Bunny pull request merges first.
- A new shape, light or lens changes no existing render: provenance names its contract, and a
  scene that does not use it keeps its pixels.
- Moving code between the repositories later is mechanical but changes contract identities, so the
  choice is cheap to reverse only before the first kernel merges.
- ADR 0005's version 2 stays the record of the encoding it describes. Version 3 supersedes it for
  scene content; ADR 0005 is not rewritten.
