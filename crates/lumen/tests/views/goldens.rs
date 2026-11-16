//! The golden image hash of each debug view of each built-in scene, at 96 by 54 pixels, one ray
//! per pixel through its centre (ADR 0010), under the rules of `tests/golden/mod.rs`: one value
//! for every platform and build profile, changed only with the view's identity, the scene's
//! version or the Bunny commit (`GOLDEN_BUNNY_REV`), as that change's intended effect.

use lumen::{BuiltinScene, View};

/// Each scene's view and its golden hash. Recorded on aarch64 Linux (the rust-dev:1.96.0
/// container on an Apple M5 Pro) at Bunny 2cd551e248d55407a8f27e84dcb26d1b1d8768bd. The reference
/// scene has no vertex normals, so its two views are the same image.
///
/// Record of updates: the reference scene's four values were recorded for
/// `lumen-reference-scene/3` as
/// `sha256:a195e16887daf58df818e200ee9c18a853455d1e1d3e70c2c4d65b9ef9a89f28` (normals and
/// shading normals), `sha256:91fb137060a5eea8d384965c69a877a4b969338310b5938adee943213a0f656f`
/// (wireframe) and `sha256:5c8d36082f4119aad4bf9f58be30d02790f95e0f6e71bf4c700f569225cdb7a0`
/// (BVH cost). They changed when the scene became `lumen-reference-scene/4`
/// (flyingrobots/lumen#179: the bunny turned to face the camera), every view identity and the
/// Bunny commit unchanged. The new normals value was computed on aarch64 Linux and,
/// independently, by CI on x86-64 Linux before it was recorded.
pub(crate) const GOLDEN_VIEWS_96X54: &[(BuiltinScene, View, &str)] = &[
    (
        BuiltinScene::Reference,
        View::Normals,
        "sha256:c3e5b6454c86870bcfd68e078eb306e5ee16b9af058ecf681bc168c77a24b59a",
    ),
    (
        BuiltinScene::Reference,
        View::ShadingNormals,
        "sha256:c3e5b6454c86870bcfd68e078eb306e5ee16b9af058ecf681bc168c77a24b59a",
    ),
    (
        BuiltinScene::FixedPoint,
        View::Normals,
        "sha256:2bbc872cc3961fcdbb3c4b8edb82b795c6217dc85205e620c8d57241d3578755",
    ),
    (
        BuiltinScene::FixedPoint,
        View::ShadingNormals,
        "sha256:cd1ee76752b3f2652212988b549ccb41e6bbc3e17b96e0afa5087eee4de72325",
    ),
    (
        BuiltinScene::Canyon,
        View::Normals,
        "sha256:f4fde11366b357dc00d75ca418c11a9b1e38ba223150cb7f3c2101b634bbee4a",
    ),
    (
        BuiltinScene::Canyon,
        View::ShadingNormals,
        "sha256:340dbcf9e821f04caf1b1fe18d4e26cd49ad7cca1faa89574508cb8635028b2c",
    ),
    (
        BuiltinScene::Reference,
        View::Wireframe,
        "sha256:9aa1d1ed6eb072951ca0313c32e774ea23b498801a74f16a0751441565b52c37",
    ),
    (
        BuiltinScene::Reference,
        View::BvhCost,
        "sha256:8fa84aca323a33fe227046d44d250e1638bf26dc58866b758fbf7dfc0aab8d9c",
    ),
    (
        BuiltinScene::FixedPoint,
        View::Wireframe,
        "sha256:ab1bd4297f7ee42d65b663e4bdf3696502947e648d294d1c1c47c9427323c0c7",
    ),
    (
        BuiltinScene::FixedPoint,
        View::BvhCost,
        "sha256:5c2033330ad1dd4660eaf275368d22369fc0fd1f895829c708aec9214b54311e",
    ),
    (
        BuiltinScene::Canyon,
        View::Wireframe,
        "sha256:0a502eaf9b822476c6139b4481f50ef7e4a233b1a5c01385d8c215a2cc43ec10",
    ),
    (
        BuiltinScene::Canyon,
        View::BvhCost,
        "sha256:e9c2bb4f460a411ef703868acf304d88a27e3b8d67e8300039a7bda13c692dfb",
    ),
];
