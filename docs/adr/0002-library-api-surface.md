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
  materials, lighting and camera, as named constants and builders), and `mesh`, the mesh reader
  it uses. `mesh` stays public because its error types are part of `SceneError`. It is an
  interim reader, to be replaced by Bunny's (flyingrobots/bunny#247).
  `scenes` (flyingrobots/lumen#15) holds further scenes' content on the same terms as
  `reference`: `scenes::fixed_point`, whose builder and identity are also re-exported from the
  root.
- **Internal**: the SHA-256 implementation (only `Rgba8Image::hash` uses it), the PNG signature,
  and the CRC-32 and Adler-32 types (only `write_png` uses them).
- **The library has no edge.** No argument parsing, file access, clock, environment, process,
  terminal output or host probe. The caller passes the thread count and the writer. The command
  (`src/bin/lumen/`) owns all of that. The one host resource the library uses is threads, started
  by `render_pixels` at the count the caller gave (ADR 0001).
- **`RenderProvenance` is the render's identity.** It holds the scene's identity, the size and
  the settings. With the identities fixed in this build, `fields()` returns twelve named fields in
  a stable order: `scene`, `integrator`, `shading`, `sampler`, `tone`, `encoding`, `bunny`,
  `size`, `pixels`, `samples`, `seed`, `bounces`. New fields go at the end. Equal provenance
  means equal RGBA8 bytes.
- **Run facts are not provenance.** The thread count, the tile size, the output file and the
  wall-clock times change no pixel (ADR 0001), so a front end reports them separately. The ray
  count and the image hash are results of the render, carried by `Render` and
  `Rgba8Image::hash`. Provenance is known before the render runs.

## Consequences

- Moving an item between private modules does not change the API. Removing or renaming a root
  re-export does.
- The caller names the scene in `RenderProvenance::new`, because a `Scene` does not carry its
  identity yet. For lumen's own scenes `BuiltinScene::id()` gives it, so the `lumen` command takes
  it from the scene it chose. When scenes are read from Geordi artifacts (flyingrobots/lumen#6), the identity
  should come from the scene. The `&'static str` type of identities will then need revisiting.
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
