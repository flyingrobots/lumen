# ADR 0005: A scene's content identity

Status: accepted (flyingrobots/lumen#38); version 2 of the encoding, which adds vertex normals,
accepted for flyingrobots/lumen#39 (see [Version 2](#version-2-vertex-normals)).

## Context

`RenderProvenance` promises that equal provenance means equal RGBA8 bytes (ADR 0002), but the
scene enters it as a label: `RenderProvenance::new(scene: &'static str, ...)`. `SceneSpec` lets a
caller supply any triangles, materials, lighting and camera, so two different scenes can carry the
same label. For the two built-in scenes, versioned names and golden image hashes keep the promise;
for any other scene (one read from a Geordi artifact, a cached or distributed render) nothing
does. The scene needs an identity derived from its content.

## Decision

### What is identified

A scene's content identity is the SHA-256 digest of a canonical byte encoding of its
`SceneSpec`, as `Scene::new` admitted it: everything a pixel or a ray count depends on, and
nothing that is an execution choice.

Included, each as supplied in the `SceneSpec`:

- **The triangles, in primitive order**, each vertex's three raw Q32.32 coordinates. The order
  is part of the identity: of triangles hit at the same distance the lowest primitive index wins
  (`PrimitiveIndex`), so the same triangles in another order can render other pixels.
- **The material index of each triangle**, beside it.
- **Every albedo of the material table**, in table order, including any no triangle names. An
  unused albedo cannot change a pixel; it is hashed anyway because the identity is of what was
  supplied, and leaving entries out would need a rule about which ones count. The cost of
  including it is only that two scenes with equal pixels can have different identities, which is
  already true (below).
- **The directional light**, present or absent; when present, its direction toward the light and
  its irradiance, as given.
- **The sky's radiance.**
- **The camera placement**: position, look direction, up hint and the tangent of half the
  vertical field of view.
- **The vertex normals** (`SceneSpec::normals`), present or absent as a whole: none, or one entry
  per triangle, in primitive order, each of its three vertices' normals present or absent. They
  decide the shading normal of a smooth-shaded triangle, so they decide pixels.
- **The encoding's name and version**, `lumen-scene-content/2`, first, for domain separation.

Not included:

- **What `Scene::new` derives**: Bunny's `TraceTriangle` and `TriangleSurface` per triangle (a
  deterministic function of the three vertices) and the unit direction inside
  `DirectionalLight` (a deterministic function of the direction as given). Hashing the inputs is
  enough: equal inputs give equal derived data, and the renderer reads nothing else.
- **The acceleration structure.** Bunny's traversals return the answer of a test of every
  triangle on any tree over the same triangles, so the tree changes only the speed. There is one
  builder and one layout (`build_bvh_sah` converted by `compact_bvh`) and no public way to choose
  another; the identity is computed from the `SceneSpec` alone, before the tree is built, so no
  tree can enter it.
- **The image size, the samples per pixel and the seed**: separate provenance fields.
- **The thread count and the band height**: they change no pixel (ADR 0003).
- **The scene's human-readable name**, such as `lumen-reference-scene/3`. A name is a label a
  person chooses; it is carried beside the identity, not inside it.
- **The integrator, shading, sampler, tone and encoding identities and the Bunny commit**: they
  are provenance fields of their own.

The identity is of the input, not of the picture. A triangle no ray can reach changes the
identity and no pixel; a test shows exactly that. The promise runs one way: equal identities (with
equal other provenance) mean equal bytes; different identities do not mean different bytes.

### The canonical encoding, `lumen-scene-content/2`

Every integer is little-endian. Every count and length is a `u64`, so the encoding is the same
on 32-bit WebAssembly as on 64-bit hosts; no `usize` appears. A coordinate, albedo channel,
irradiance, radiance or tangent is the raw `i64` of its Q32.32 value (`FixedQ32_32::raw`). A
vector is its `x`, `y`, `z` in that order; a colour its red, green, blue.

| Order | Field | Bytes |
|---|---|---|
| 1 | length of the version string, `21` | `u64` |
| 2 | the version string `lumen-scene-content/2` | 21, ASCII |
| 3 | triangle count `T` | `u64` |
| 4 | per triangle, in primitive order: `v0`, `v1`, `v2` (nine `i64`), then its material index (`u32`) | `76 * T` |
| 5 | material count `M` | `u64` |
| 6 | per material, in table order: its albedo (three `i64`) | `24 * M` |
| 7 | light tag: `0` absent, `1` present | `u8` |
| 8 | only if the tag is `1`: the direction toward the light (three `i64`), then the irradiance (three `i64`) | 48 or 0 |
| 9 | the sky's radiance (three `i64`) | 24 |
| 10 | the camera: position, look, up (nine `i64`), then the tangent of half the field of view (`i64`) | 80 |
| 11 | vertex-normal count `N`: `0` for a scene without vertex normals, else `T` | `u64` |
| 12 | per entry, in primitive order, per vertex `v0`, `v1`, `v2`: a tag, `0` absent or `1` present, then only if `1` the unit normal (three `i64`, the raw components of the `FixedUnitVec3`) | 3 to 75 per entry |

The digest is SHA-256 (FIPS 180-4) of those bytes, nothing before or after. It is written
`sha256:` followed by 64 lowercase hexadecimal digits, as the image hash is.

The encoding is uniquely decodable from the front: every field's length is fixed or given by a
count or tag that precedes it, so two different scenes cannot produce the same stream. In
particular the counts prevent the concatenation collision of a naive encoding: without them,
seven all-zero triangles with one all-zero albedo and one all-zero triangle with twenty all-zero
albedos are both 556 zero bytes (`7 * 76 + 24 = 76 + 20 * 24`). A test builds exactly those two
scenes. The normals follow the same rule: their count says how many entries follow, and each
vertex's tag says whether 24 bytes follow it, so a scene with no normals (count `0`), one whose
every vertex has none (count `T`, `3T` zero tags) and one with normals are three different streams.

The bytes are fed to the hasher as they are produced, one triangle record at a time; no buffer
of the whole encoding exists. A second implementation can reproduce the digest from this table;
a test in `crates/lumen/src/scene/content_id/tests.rs` does so, building the stream by hand.

A change to anything in this section is a new version, `lumen-scene-content/2`, and changes
every digest.

### The API

- **`Scene::content_id()`** returns a `SceneContentId`, a 32-byte value type (`Copy`, `Eq`,
  `Ord`, `Hash`) with `to_bytes()` and a `Display` of `sha256:<hex>`. It is not a `String`.
- **It is computed once, in `Scene::new`**, after the input is validated and before the tree is
  built, from the `SceneSpec` only. It costs one pass over the input. Measured (Apple M5 Pro,
  `rust-dev:1.96.0` container, run alone, median of 11): the reference scene's 2.4 MB encoding
  (31 511 triangles) hashes in 6.8 ms in the release profile, against 33.7 ms for all of
  `Scene::new`; Fixed Point's (5 975 triangles) in 1.3 ms against 5.6 ms. In the unoptimized test
  profile lumen's own SHA-256 is slower: 120 ms of 154 ms for the reference scene. A render takes
  seconds, so the cost is paid once per scene where it is noticed least. Computing it lazily
  would need interior mutability or a copy of the raw input kept beside the prepared one, to
  save milliseconds only for a caller that never asks.
- **`SCENE_CONTENT_ENCODING`** is the version string, `lumen-scene-content/2`.
- **`RenderProvenance` is built from `&Scene`**: `RenderProvenance::new(&scene, size, settings)`
  for a scene without a name, `RenderProvenance::named(name, &scene, size, settings)` for one
  with a name. It takes the content identity from the scene, so it cannot be built from a bare
  string any more. The name is a label, not checked against the content. `name()` and
  `scene_content()` return the two.
- **The digest is the thirteenth field, `scene-content`** (`ProvenanceValue::Content`), at the
  end of `fields()`. ADR 0002's rule that new fields go at the end wins over printing the digest
  next to the name: every existing field keeps its position, and the command's report gains one
  line after `bounces`, every other line unchanged. The `scene` field is the name, or
  `ProvenanceValue::Unnamed` for a scene given none. `PROVENANCE_FIELDS` is 13. lumen is
  unreleased, so this changes the API without a deprecation step.
- **The built-in scenes' digests are golden values** (`tests/golden/mod.rs`), beside their image
  hashes, checked by the library test on every Determinism leg (WebAssembly included) and by
  the command's test. Two more goldens, for a one-triangle scene without and with vertex
  normals, were computed outside lumen (Python's `struct` and `hashlib`, from the table above),
  so the encoder and lumen's SHA-256 are not checked only against themselves.

### Version 2: vertex normals

`SceneSpec` gained `normals` (flyingrobots/lumen#39), which decides pixels, so the identity must
cover it. Two encodings were weighed:

- **An optional section whose absence encodes as before**, so that every scene without normals
  keeps its version 1 digest. It is unambiguous: a version 1 stream decodes to its end, so a
  stream with a section after the camera cannot equal one without it. It was rejected because it
  breaks this ADR's own rule, that a change to the encoding is a new version, and a
  reimplementation of version 1 would give a scene with normals the digest of the same scene
  without them.
- **A new version, `lumen-scene-content/2`, with the normals always encoded** (fields 11 and 12,
  a count of zero for a scene without them). This was chosen: the rule stands, the table is the
  whole contract with no condition outside it, and every field is decoded the same way as the
  others (a count, then tagged entries, like the light).

The cost is that every digest changes, including those of scenes without normals: both built-in
scenes' golden digests and the Python-computed golden (now two, without and with normals) were
recomputed. Image hashes do not change. Digests recorded under version 1 (in
`docs/measurements/`) stay what they were; they are version 1 digests.

## Consequences

- A scene's identity no longer depends on a caller's label. The built-in scenes keep their names
  (`BuiltinScene::id()`), which stay human-readable labels.
- Any change to a built-in scene's content changes its digest as well as its golden image hash.
- Equal provenance now implies equal bytes for any scene, not only for the built-in ones whose
  names are versioned: two scenes under one name have different provenance.
- A front end that recorded provenance before this change has no `scene-content` field.

## Rejected

- **Hashing the prepared scene** (`TraceTriangle`s, surfaces, the tree). The derived data is a
  function of the inputs; hashing it ties the identity to Bunny's internal representation and,
  for the tree, to an execution choice.
- **Hashing only what can reach a pixel** (dropping unused materials or unreachable triangles).
  Deciding reachability is a render, not an encoding.
- **Building the encoding in a buffer**, then hashing it: for the reference scene that is a
  second copy of 2.4 MB for nothing.
- **Including the name in the digest.** Two equal scenes under two names would get two
  identities, and a name is exactly the label the digest replaces.
- **`usize` counts, or counts only for the triangle list.** The first differs between 32-bit
  and 64-bit targets; the second leaves the material table's length to be inferred from what
  follows it.
