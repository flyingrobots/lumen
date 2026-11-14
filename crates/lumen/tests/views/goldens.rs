//! The golden image hash of each debug view of each built-in scene, at 96 by 54 pixels, one ray
//! per pixel through its centre (ADR 0010), under the rules of `tests/golden/mod.rs`: one value
//! for every platform and build profile, changed only with the view's identity, the scene's
//! version or the Bunny commit (`GOLDEN_BUNNY_REV`), as that change's intended effect.

use lumen::{BuiltinScene, View};

/// Each scene's view and its golden hash. Recorded on aarch64 Linux (the rust-dev:1.96.0
/// container on an Apple M5 Pro) at Bunny 2cd551e248d55407a8f27e84dcb26d1b1d8768bd. The reference
/// scene has no vertex normals, so its two views are the same image.
pub(crate) const GOLDEN_VIEWS_96X54: &[(BuiltinScene, View, &str)] = &[
    (
        BuiltinScene::Reference,
        View::Normals,
        "sha256:a195e16887daf58df818e200ee9c18a853455d1e1d3e70c2c4d65b9ef9a89f28",
    ),
    (
        BuiltinScene::Reference,
        View::ShadingNormals,
        "sha256:a195e16887daf58df818e200ee9c18a853455d1e1d3e70c2c4d65b9ef9a89f28",
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
];
