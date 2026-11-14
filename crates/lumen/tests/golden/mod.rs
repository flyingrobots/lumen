//! The golden image hash, shared by the render and thread tests.

/// The golden image hash of the reference scene.
///
/// Recorded for `lumen-reference-scene/3`, `lumen-diffuse-path/1` (Bunny's
/// `bunny-sobol-owen/1` and `bunny-diffuse-shading/1`), `bunny-tone-srgb/1` and
/// `bunny-rgba8-opaque/1`, at 96 by 54 pixels, 4 samples per pixel, seed 0, built against Bunny
/// dcd6bd37613738de93f7084e7c8aa0283de964ea. First recorded on aarch64 Linux (the
/// rust-dev:1.96.0 container on an Apple M5 Pro); CI checks it on x86-64 Linux in both the
/// test and the release profile. It must be identical on every platform and build profile.
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
pub(crate) const GOLDEN_96X54_4SPP: &str =
    "sha256:4f9d5436dde802b5956d1ad6b83fe9671021ae5838f54bc697ae18690ebb9a46";
