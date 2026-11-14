# ADR 0002: The library's public API and the render's provenance

Status: accepted (flyingrobots/lumen#19)

## Context

Until flyingrobots/lumen#19, `lumen` exposed every module, including `cli` (argument parsing, the
usage text and a command carrying a `PathBuf`), `sha256` and the PNG checksum types. Any consumer
of the library, such as a Geordi runtime adapter or a WASM build, got the command line with it.
The record of what was rendered (the scene, integrator, shading, sampler, tone and encoding
identities, the Bunny commit and the settings) existed only as text assembled in `main.rs`, so a
second front end would have had to rebuild that list by hand.

## Decision

- **The crate root is the API.** `lib.rs` re-exports every public item, and the implementation
  modules (`image`, `png`, `provenance`, `render`, `scene`, `schedule`, `sha256`, `workers`) are
  private. Code outside the crate imports `lumen::Item`, never `lumen::render::Item`. A new
  public item is added to the root re-exports on purpose. An item nobody outside the crate needs
  is `pub(crate)`.
- **Two public modules remain**: `reference`, the reference scene's content (its triangles,
  materials, lighting and camera, as named constants and builders), and `mesh`, the mesh types
  it uses. `mesh` stays public because its placement and error types appear in the public
  signatures of `reference` and in `SceneError`. The reader is Bunny's `bunny_codec::parse_ascii_ply`,
  gathered by `mesh::read_ascii_ply`; the interim reader of flyingrobots/lumen#27 is gone.
  `scenes` (flyingrobots/lumen#15) holds further scenes' content on the same terms as
  `reference`: `scenes::fixed_point`, whose builder and identity are also re-exported from the
  root.
- **Internal**: the SHA-256 implementation (only `Rgba8Image::hash` uses it), the PNG signature,
  and the CRC-32 and Adler-32 types (only `write_png` uses them).
- **The library has no edge.** No argument parsing, file access, clock, environment, process,
  terminal output or host probe. The caller passes the thread count and the writer. The command
  (`src/bin/lumen/`) owns all of that. The one host resource the library uses is threads, started
  by `render_pixels` at the count the caller gave (ADR 0003).
- **`RenderProvenance` is the render's identity.** It holds the scene's content identity and
  optional name (ADR 0005), the size and the settings. With the identities fixed in this build,
  `fields()` returns thirteen named fields in a stable order: `scene`, `integrator`, `shading`,
  `sampler`, `tone`, `encoding`, `bunny`, `size`, `pixels`, `samples`, `seed`, `bounces`,
  `scene-content` (the last added by flyingrobots/lumen#38). New fields go at the end. Equal
  provenance means equal RGBA8 bytes.
- **The errors can grow.** `SceneError` and `TraceError` (flyingrobots/lumen#20), `RenderError`,
  `PathError`, `WorkerError`, `Reservation`, `SizeError`, `SceneBuffer`, `PlaceError`,
  `FixedPointError`, `BuiltinSceneError` and `PngError` (flyingrobots/lumen#21) are
  `#[non_exhaustive]`: a
  new reason to refuse a scene, a mesh, a ray, a path, a size or a render is not a breaking
  change, and a `match` on any of them outside the crate needs a wildcard arm. Code outside the crate may still construct their
  variants, as a custom `Tracer` does.
- **An error says what failed and keeps its cause** (flyingrobots/lumen#21). Each variant carries
  the values it saw (an index and a count, Bunny's reason, a pixel's coordinates), not a bare
  name. An error that wraps another includes the inner message in its `Display`, so the text
  stands alone and the command prints it unchanged, and returns the inner error from
  `source()`, so a program can walk the chain; a reporter that prints the whole chain repeats
  the inner text, which is the price of the first. A path's failure (`PathError`, from
  `trace_path` and `pixel_radiance`) is its own type, not a `RenderError`, because a path cannot
  fail for a size, a scene or a thread; the render wraps it with the pixel
  (`RenderError::Pixel { x, y, error }`). Where Bunny has a fast `Option` form and a typed
  `try_` form of the same check, lumen calls the `Option` form and the `try_` form only after a
  `None`, so carrying the reason costs a render that succeeds nothing.
- **The render's errors keep the system's errors and stay comparable** (flyingrobots/lumen#21).
  `RenderError::OutOfMemory` holds the allocator's `TryReserveError` and `Spawn` a `SpawnError`
  holding the operating system's `io::Error`, so neither enum is `Copy` any more. The `io::Error`
  sits behind an `Arc`, so the errors stay `Clone`, and `SpawnError` compares the worker, the
  error's kind, its operating-system code and its text, so they stay `PartialEq` and tests can
  compare whole results. A refusal that cannot happen is made impossible rather than reported:
  the workers render into a buffer `Rgba8Image` reserved at the size's length, so no length
  check remains to fail, and `ImageSize::byte_len` is computed once by `ImageSize::new`. A scene is validated whole when it is
  built: `Scene::new` refuses a camera placement Bunny's camera refuses
  (`SceneError::Camera`), so at render time only the image size can be refused.
- **A triangle is named by its `PrimitiveIndex`** (flyingrobots/lumen#20): a `u32`, the
  triangle's zero-based position in `SceneSpec::triangles` in the order the caller gave them,
  the width of Bunny's primitive-index buffer. `PrimitiveIndex::to_usize` cannot fail
  (flyingrobots/lumen#21): lumen builds only where `usize` has at least 32 bits, which a
  compile-time assertion checks. `Hit`, `Scene::surface`, `Scene::albedo` and the
  primitive-naming `SceneError` variants use it instead of a bare `u32` or `usize`. The order is
  part of the scene's identity, not an incidental layout: of triangles hit at the same distance
  the lowest index wins, so the same triangles in another order can render other pixels. A
  content digest of a scene (flyingrobots/lumen#38) must therefore cover the triangles, and each
  one's material index, in primitive order.
- **Run facts are not provenance.** The thread count, the band height, the output file and the
  wall-clock times change no pixel (ADR 0003), so a front end reports them separately. The ray
  count and the image hash are results of the render, carried by `Render` and
  `Rgba8Image::hash`. Provenance is known before the render runs.

## Consequences

- Moving an item between private modules does not change the API. Removing or renaming a root
  re-export does.
- `RenderProvenance` takes the scene itself, and its content identity from it (ADR 0005); the
  caller may add a name. For lumen's own scenes `BuiltinScene::id()` gives the name, so the
  `lumen` command takes it from the scene it chose. The `&'static str` type of names may need
  revisiting when scenes are read from Geordi artifacts (flyingrobots/lumen#6).
- A branch written against the old module paths (`lumen::render::trace_path`,
  `lumen::scene::SceneSpec`, `lumen::png::write_png`) must import from the root instead.

## Rejected

- **Keeping the modules public as well as re-exporting from the root.** That gives every item
  two paths, and the module layout becomes API by accident. flyingrobots/lumen#20 is about to
  split `scene.rs`, which would then break callers.
- **`Display` for `RenderProvenance`.** That would put the command's text format into the
  library. `fields()` gives a front end the values and the order. The text, JSON or other format
  is the front end's choice.
- **Storing the scene's identity in `Scene`.** That would change `SceneSpec`, a struct every
  scene builder constructs. It waits for scenes that carry their own identity (flyingrobots/lumen#6).
  ADR 0005 (flyingrobots/lumen#38) gives `Scene` a content identity derived from its
  `SceneSpec`, which needs no change to `SceneSpec`; a scene's name is still not stored.
