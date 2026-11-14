//! The golden image hash, shared by the render and thread tests.

/// The golden image hash of the reference scene.
///
/// Recorded for `lumen-reference-scene/3`, `lumen-diffuse-path/1` (Bunny's
/// `bunny-sobol-owen/1` and `bunny-diffuse-shading/1`), `bunny-tone-srgb/1` and
/// `bunny-rgba8-opaque/1`, at 96 by 54 pixels, 4 samples per pixel, seed 0, recorded against Bunny
/// dcd6bd37613738de93f7084e7c8aa0283de964ea, unchanged at
/// 3c395f9950ac63d5ecfbd2055c306954111daf9d, 3c0dab3e77c0d45de8904906a134e846dfe5ec6c and
/// d4fe51d38fa10f8d6496bd110955314edac30bbf. First recorded on aarch64 Linux (the
/// rust-dev:1.96.0 container on an Apple M5 Pro). It is the one expected value for every
/// platform and build profile: CI's `Determinism / ...` jobs check it on x86-64 and aarch64
/// Linux, aarch64 macOS, x86-64 Windows and `wasm32-wasip1`, each in the test and the release
/// profile. Never add a per-platform value.
///
/// Updating it is legitimate only when one of those identities, the settings above or the
/// pinned Bunny commit (`BUNNY_REV`) changes, and the change of hash is the intended effect of
/// that change. Any other change of this hash is a determinism bug: do not update it to make
/// the test pass.
///
/// Record of updates: recorded for `lumen-reference-scene/2` as
/// `sha256:3e90dbd46172423c9c9e1e60c0c6b15e35c6ceeefa0647ba53833b8d89e6bfa7`; updated when the
/// scene identity changed to `lumen-reference-scene/3` (flyingrobots/lumen#14: the Stanford bunny
/// added, the camera moved), every other identity, the settings and the Bunny commit unchanged.
#[allow(
    dead_code,
    reason = "each test crate that includes this module uses only the hashes it needs"
)]
pub(crate) const GOLDEN_96X54_4SPP: &str =
    "sha256:4f9d5436dde802b5956d1ad6b83fe9671021ae5838f54bc697ae18690ebb9a46";

/// The golden image hash of the Fixed Point scene, `lumen-fixed-point-scene/2`: 96 by 54 pixels,
/// 4 samples per pixel, seed 0, rendered from `fixed_point_scene()` (and by
/// `lumen render --scene fixed-point`).
///
/// Recorded for `lumen-fixed-point-scene/2`, `lumen-smooth-diffuse-path/1` (Bunny's
/// `bunny-sobol-owen/1`, `bunny-diffuse-shading/1`, `bunny-smooth-shading/1` and
/// `bunny-smooth-normal/1`), `bunny-tone-srgb/1` and `bunny-rgba8-opaque/1`, against Bunny
/// d4fe51d38fa10f8d6496bd110955314edac30bbf; first recorded on aarch64 Linux (the rust-dev:1.96.0
/// container on an Apple M5 Pro). Like the reference scene's, it is the one expected value for
/// every platform and build profile, checked by the same `Determinism / ...` jobs. Never add a
/// per-platform value.
///
/// Updating it is legitimate only when one of those identities, the settings above or the
/// pinned Bunny commit changes, and the new hash is the intended effect of that change. A
/// change to the scene's triangles, their order, its materials, normals, lighting or camera is a
/// new scene version and updates this hash with it. Any other change of this hash is a
/// determinism bug: do not update it to make the test pass.
///
/// Record of updates: `lumen-fixed-point-scene/1` (the decimated bunny, flat, under
/// `lumen-diffuse-path/1`) was
/// `sha256:0688b03cf16be6d0336f17eacbdaf83fe03d62e9c66b8370e8208bb9c879634e`;
/// version 2 (flyingrobots/lumen#39: the full-resolution bunny shaded smooth, the glasses'
/// right arm refitted, a white puck with a thin black rim and bolder letters, the sun moved to
/// the camera's side) replaced it.
#[allow(
    dead_code,
    reason = "each test crate that includes this module uses only the hashes it needs"
)]
pub(crate) const GOLDEN_FIXED_POINT_96X54_4SPP: &str =
    "sha256:da6cb05db0d366a2088f19e6eefed2ab017dd0d27c6c5e364a85121054d6093e";

/// The golden content identity (`Scene::content_id`, `lumen-scene-content/2`, ADR 0005) of the
/// reference scene, `lumen-reference-scene/3`. Like the image hashes it is one value for every
/// platform. It changes only with the scene's content (a new scene version) or the encoding (a
/// new `lumen-scene-content` version); any other change is a bug.
///
/// Record of updates: under `lumen-scene-content/1` it was
/// `sha256:84541be146ebac194b43dfc91f9205356818d517c71f52d947a3ca417178d356` (the reference
/// scene) and `sha256:10b0e9bd01ec5aa20c899d2efc8c45cca5d84a404c7c64ca48b44a38999b48a5` (Fixed
/// Point, below); both changed when the encoding became version 2, which adds the vertex
/// normals (flyingrobots/lumen#39). Neither scene's content or image hash changed.
#[allow(
    dead_code,
    reason = "each test crate that includes this module uses only the constants it needs"
)]
pub(crate) const GOLDEN_REFERENCE_CONTENT: &str =
    "sha256:c1e109477c09328421d76efefd923bd625d842fe3afb93dfadb21ae905bd0f66";

/// The golden content identity of the Fixed Point scene, `lumen-fixed-point-scene/2`, on the
/// same terms as [`GOLDEN_REFERENCE_CONTENT`]. Version 1's, under `lumen-scene-content/2`, was
/// `sha256:7094ced63e59606707dc9c172c1801426183ff0b1ceda06a1c65a6a11062a086`.
#[allow(
    dead_code,
    reason = "each test crate that includes this module uses only the constants it needs"
)]
pub(crate) const GOLDEN_FIXED_POINT_CONTENT: &str =
    "sha256:f80160bbd1ebb6e79f186bf5a86f2ce3419d4ef31168cc4cab0b6fd1f0f64e11";

/// The golden image hash of the canyon scene, `lumen-canyon-scene/1`: 96 by 54 pixels, 4
/// samples per pixel, seed 0, rendered from `canyon_scene()` (and by
/// `lumen render --scene canyon`).
///
/// Recorded for `lumen-canyon-scene/1` (terrain by `lumen-canyon-heightfield/1`),
/// `lumen-smooth-diffuse-path/1` (Bunny's `bunny-sobol-owen/1`, `bunny-diffuse-shading/1`,
/// `bunny-smooth-shading/1` and `bunny-smooth-normal/1`), `bunny-tone-srgb/1` and
/// `bunny-rgba8-opaque/1`, against Bunny d4fe51d38fa10f8d6496bd110955314edac30bbf; first
/// recorded on aarch64 Linux (the rust-dev:1.96.0 container on an Apple M5 Pro). The same rules
/// as the other scenes' hashes: one value for every platform and build profile, updated only
/// with an identity, these settings or the Bunny commit, and a change to the scene's triangles,
/// their order, its materials, normals, lighting or camera is a new scene version.
#[allow(
    dead_code,
    reason = "each test crate that includes this module uses only the hashes it needs"
)]
pub(crate) const GOLDEN_CANYON_96X54_4SPP: &str =
    "sha256:019fcdddaaff8761f3f6e64dfe28d1ce7d2b07200b8c69663345ca4a495466c5";

/// The golden content identity of the canyon scene, `lumen-canyon-scene/1`, on the same terms
/// as [`GOLDEN_REFERENCE_CONTENT`].
#[allow(
    dead_code,
    reason = "each test crate that includes this module uses only the constants it needs"
)]
pub(crate) const GOLDEN_CANYON_CONTENT: &str =
    "sha256:a809761e5cbc96944721e8e2095ae9e0eaa76d8f3ac8bf25e536446835eaf88d";
