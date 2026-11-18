# ADR 0016: All graphics in Bunny, the integrators included

Status: accepted, decided by the maintainer on 2026-11-17. Extends ADR 0011. Nothing in this record
is built yet: the integrators named below are still Lumen's code.

## Context

ADR 0011 put every computation that can change a pixel in Bunny as a versioned contract, and left
Lumen the scene model, storage, formats and threading. It was written for hit tests, lights and
the lens. The plan has since grown to glass, textures, bump mapping, caustics, denoising and the
glass-teapot scene (flyingrobots/lumen#275).

One part of light transport is still implemented in Lumen: the integrator, the loop that follows a
path from hit to hit. Lumen has two, each with an identity that a render's provenance records:
`lumen-diffuse-path/1` and `lumen-smooth-diffuse-path/1` (`crates/lumen/src/render.rs`, selected
by `Integrator::for_scene`; ADR 0007 states the naming rule). Each is a documented composition of
Bunny contracts, and Bunny's `bunny-diffuse-shading/1` already fixes the order of operations within
a hit. What Lumen implements is the composition: which sampler pair feeds which decision, when a
path continues, when it ends. The planned `lumen-path/1` (flyingrobots/lumen#203) would add a third
loop in Lumen: mirror first, with later metal and glass extensions.

Those decisions change results as surely as any multiplication does. The maintainer decided on
2026-11-17 that the complete light-transport algorithm, continuation and termination included, is
Bunny's, so that it is implemented and maintained beside the operations it composes and another
caller can use it without rebuilding it.

## Decision

### The rule

**Bunny defines reusable graphics evaluation. Lumen supplies the authored inputs, selects the
contracts, schedules the work and records the results.**

"Changes a pixel" alone does not decide ownership: moving the camera or choosing a material
changes pixels too, and both are Lumen's. The line is between choosing inputs and defining the
algorithm that interprets them. pbrt's layout is a useful comparison but not the rule: pbrt's
tiled parallel loop is part of its integrator, and here threading stays in Lumen.

| Concern | Bunny | Lumen |
| --- | --- | --- |
| Materials | What scattering, reflection, refraction and their samples mean | Which material and parameters a scene uses |
| Camera | How camera inputs produce rays | Where the camera is and what it frames |
| Integration | Path evaluation, continuation, termination, the sample dimension each decision uses | Which integrator and its settings (bounces, samples) |
| Geometry | Hit tests, the tree and traversal, tessellation, vertex normals, mesh offsets and checks | Scene organisation, assets and their provenance, scene identities |
| Lights | Light kinds, light sampling, environment lookup, photon maps | Which lights a scene has |
| Textures | Lookup, filtering, procedural patterns, shading normals from maps | Which texture a material names |
| Image processing | Tone curve, reconstruction filters, the denoiser | Which stages run, and keeping their results |
| Evidence | The contracts and the observations a caller needs (ray outcomes, errors) | Provenance, hashes, statistics, golden values, gates |
| Running | Nothing | The command, job files, JSON Lines, threading, row bands, progress |

### Exceptions, with their reasons

- **Debug views stay in Lumen (ADR 0010, unchanged).** A view's integer map from a normal, a depth,
  an ID or a work count to a colour encodes evidence for a person to read. It interprets no light,
  and it uses only Bunny's results for the values it maps. If another caller needs the same views,
  moving them to Bunny is a new record.
- **Image decoding stays in `lumen-image` (ADR 0012, unchanged).** It reconstructs raster data
  from files without evaluating a scene. The JPEG standard leaves the IDCT's precision and chroma
  upsampling open, so `lumen-jpeg-decode/1` fixes the arithmetic choices needed for reproducible
  decoded pixels; it keeps that identity and its place. The decoder is kept whole, in one crate; it is not split
  between repositories by step. Mesh decoding is already Bunny's (`bunny-ascii-ply-triangle-mesh/1`)
  and stays there.

### What moves, and how

- **New integrators are Bunny's.** flyingrobots/lumen#203 is re-scoped: the path loop with a mirror
  is a Bunny integrator, and Lumen's task selects it and wires its settings, provenance and
  statistics. The sampling rule that #185 asks for (which sample dimensions each decision at each
  depth consumes) is part of that integrator's contract.
- **Bunny reaches the scene through a small interface, not Lumen's `Scene`.** As Lumen already owns
  the tree's storage and calls Bunny's construction and traversal, Lumen adapts its scene to a
  Bunny trait for closest-hit and any-hit queries and for a hit's material. Bunny does not learn
  the command, job files, scene files or provenance. The trait answers queries; a callback that
  only observes (for statistics) must not influence sampling, branching or termination. ADR 0002's
  equal-provenance promise keeps its present scope: it does not extend to a caller's own tracer.
- **The existing diffuse integrators move later, and only as a move.** An identity is a promise
  about results, not about which repository holds the code. `lumen-diffuse-path/1` and
  `lumen-smooth-diffuse-path/1` keep their names and meaning; when Bunny's implementation replaces
  Lumen's, it must preserve, not just the image:
  - the image bytes, on the golden corpus and every Determinism platform;
  - the linear radiance of each path, compared before the tone curve and quantisation;
  - flat, smooth and mixed scenes, for both identities;
  - the ray outcomes Lumen reports (camera hits, bounces, shadow rays, rejected bounces);
  - the typed failures, with their context;
  - the tracer's declared query behaviour and tie-breaking;
  - the public API and provenance fields, or an explicit, recorded transition.

  A golden image witnesses the image only. Measured traversal work (ADR 0008's counts) may change
  with a different traversal and is recorded as such, not treated as a broken promise. The
  independent composition in `crates/lumen/tests/render.rs`, written with brute-force queries, stays
  independent: it is never rewritten to call the implementation it checks. A move is not combined
  with a numerical change; a change of result is a new identity, separately reviewed.
- **The tone curve is already Bunny's.** `ToneCurve::apply` and `write_rgba8` are Bunny's;
  Lumen's `TONE_CURVE` only selects `bunny-tone-srgb/1` and records it. Extracting the diffuse
  integrators preserves the selected curve, the output bytes and the provenance.

## Alternatives rejected

| Alternative | Why not |
| --- | --- |
| Keep integrators in Lumen (ADR 0011 as written) | They are named and recorded already, but the composition would stay in a repository other than the operations it orders, and every new material would add transport code to Lumen that no other caller can reuse. |
| Split by "kernel in Bunny, loop in Lumen" | The termination rule and branch choice change results as much as any kernel; the split would be arbitrary. |
| Move the scene model, scene files or evidence into Bunny | ADR 0011's reason stands: storage, identities, file formats and provenance are a renderer's business. |
| Move debug views and image decoders to Bunny now | Coherent, but neither evaluates light; the move would churn accepted decisions and open work (flyingrobots/lumen#272, #273) for no caller that needs it. |

## Consequences

- Lumen's arithmetic narrows to integer bookkeeping, the debug-view colour maps (ADR 0010) and image
  decoding (ADR 0012). A reviewer can refuse any other Lumen change that defines how light, geometry
  or a texture is evaluated.
- Bunny grows from a library of primitives into one that also offers complete integrators. It
  stays modular: a caller that needs only a box test does not take on the integrators' dependencies.
  Bunny's README says it does not try to be a renderer; the first Bunny integrator task updates it.
- Each feature still takes a Bunny pull request first and a move of Lumen's pin (ADR 0011).
- The first Bunny integrator is small: #203's mirror facing a diffuse card, with bounded termination
  and stated sampling. Glass, media, photon maps and denoising are not its prerequisites.
- Issues filed before this record that put such work in Lumen are re-scoped when picked up, and
  their Bunny prerequisite is added to their frontmatter.
