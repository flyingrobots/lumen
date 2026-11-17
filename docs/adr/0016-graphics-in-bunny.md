# ADR 0016: All graphics in Bunny, the integrators included

Status: accepted, decided by the maintainer on 2026-11-17. Extends ADR 0011.

## Context

ADR 0011 put every computation that can change a pixel in Bunny as a versioned contract, and left
Lumen the scene model, storage, formats and threading. It was written for hit tests, lights and
the lens. The plan has since grown to glass, textures, bump mapping, caustics, denoising and the
glass-teapot scene (flyingrobots/lumen#275), and one piece of pixel-deciding code still lives in
Lumen: the integrator, the loop that follows a path from hit to hit.

Today `crates/lumen/src/render/shade.rs` and the path code beside it call Bunny's per-hit steps
(`bunny-diffuse-shading/1`, `bunny-smooth-shading/1`, the light sample, the bounce), but the loop
that chains them, and the rule for when a path ends, is Lumen's. The planned `lumen-path/1`
(flyingrobots/lumen#203) would add a second such loop in Lumen, with mirror, metal and glass
branches. Each of those is a light-transport decision, the kind of code a renderer core such as
pbrt holds, and under ADR 0011's rule it has no contract identity of its own.

The maintainer stated the rule in full on 2026-11-17: all computer graphics is Bunny's.

## Decision

### The test

A piece of code belongs in Bunny when it decides how light or geometry behaves: if pbrt keeps it
in its core, it is Bunny's. It belongs in Lumen when it decides which picture to make, how to run
the job, or how to prove the result.

### Bunny owns

- **Numbers and geometry:** fixed-point arithmetic, vectors, rays, ray hits with triangles and
  shapes, bounding boxes, the acceleration tree and its traversal.
- **Optics:** reflection and refraction, Fresnel, every material's scattering and its sample
  (diffuse, mirror, rough metal, glass, rough glass, coated diffuse), absorption and dispersion in
  a medium, the rules for nested media, subsurface scattering.
- **Lights:** directional, point and area lights, the sky and environment images, light
  sampling and multiple importance sampling, photon emission and the photon map.
- **Integrators:** the path loop and its termination rule, each a contract with an identity
  (for example `bunny-path/1`), which a render's provenance names.
- **Textures:** lookup, filtering and mip chains, procedural patterns, and the shading normal
  from a normal or height map.
- **Meshes:** decoding mesh formats, tessellating curved patches, vertex normals (with a crease
  rule), and any mesh repair a scene relies on.
- **Sampling:** the deterministic sample sequences.
- **Image filters that decide pixels:** reconstruction filters and the denoiser.

### Lumen owns

- **Scenes:** the scene model (ADR 0011), built-in scenes, their identities and versions (ADR
  0005), assets and their provenance, scene files.
- **Running a render:** the command, job files and JSON Lines output (ADR 0015), threading and
  row bands (ADR 0003), progress and statistics (ADR 0008).
- **Results:** the image bytes and their hash (ADR 0009), provenance, golden values and the
  determinism gates (ADR 0006).
- **Choosing:** which integrator, materials, sample count and filter a render uses, as settings
  that name Bunny contracts.

### What moves

- **`lumen-path/1` becomes a Bunny integrator.** flyingrobots/lumen#203 is re-scoped: the loop and
  its bounce rule are a Bunny contract, and Lumen's task selects it, wires settings, provenance and
  statistics. A Bunny issue is filed for the loop.
- **The existing diffuse integrators move later.** `lumen-diffuse-path/1` stays in Lumen until a
  Bunny integrator reproduces its golden hashes bit for bit; then Lumen calls Bunny's and deletes
  its own. A move that changes a pixel is a new integrator identity.
- **Asset preparation that computes geometry** (tessellation, mesh repair, vertex normals) is a
  Bunny function that Lumen calls; Lumen keeps the files and their provenance.

## Alternatives rejected

| Alternative | Why not |
| --- | --- |
| Keep integrators in Lumen (ADR 0011 as written) | A path loop decides pixels; in Lumen it would have no contract identity, and every new material would change Lumen's arithmetic. |
| Split by "kernel in Bunny, loop in Lumen" | The termination rule and branch choice change pixels as much as any kernel; the split would be arbitrary. |
| Move the scene model into Bunny too | ADR 0011's reason stands: storage, identities and file formats are a renderer's business. |

## Consequences

- Lumen's own arithmetic shrinks to integer bookkeeping: scheduling, hashing, encoding. A
  reviewer can reject any Lumen change that computes a colour, a direction or a distance.
- Each feature still takes a Bunny pull request first and a pin move in Lumen (ADR 0011).
- Moving the diffuse integrators is gated on bit-identical output, so no golden hash changes by
  accident.
- Issues filed before this record that put such work in Lumen are re-scoped when picked up; the
  task's Bunny prerequisite is added to its frontmatter.
