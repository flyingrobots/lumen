//! The library on its own: a whole render through the crate root's API, in one process, with no
//! file, clock, environment or terminal. The oracles are the golden image hash, the identities
//! `lumen render` prints, and the PNG bytes `lumen render` wrote before the command moved out of
//! the library (flyingrobots/lumen#19).

use lumen::{
    png_len, reference_scene, render, render_scheduled, write_png, BandRows, BuiltinScene,
    ImageSize, ProvenanceValue, RenderProvenance, RenderSettings, Schedule, BUNNY_REV,
    REFERENCE_SCENE_ID,
};

mod golden;
use golden::GOLDEN_96X54_4SPP;

/// The PNG `lumen render --width 5 --height 3 --samples 1 --seed 4294967295` wrote at commit
/// d8deec4 (before flyingrobots/lumen#19), in hex: 131 bytes, SHA-256
/// `b929b603b6ca1e843d21e5c416e6f7e557b98fd9f3151e937f9fe8a851734833`. It changes only when the
/// golden image hash does, or the PNG layout does.
const TINY_PNG_HEX: [&str; 5] = [
    "89504e470d0a1a0a0000000d49484452000000050000000308060000005b36c5",
    "f80000004a494441547801013f00c0ff00899cb6fff2e6d1ff899cb6ff899cb6",
    "ff899cb6ff009fa4abff6b7888ffa1615eff9fa4abff9fa4abff009fa4abff9f",
    "a4abff9fa4abff807e7cffd1c6b2ff52622b296c3b23f30000000049454e44ae",
    "426082",
];

fn from_hex(lines: &[&str]) -> Vec<u8> {
    let hex: String = lines.concat();
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
        .collect()
}

/// Three worker threads. WebAssembly here has no threads, so on that target the same request is
/// a typed refusal (below) and the golden image is checked on one worker by the next test.
#[cfg(not(target_family = "wasm"))]
#[test]
fn the_reference_image_renders_through_the_library_alone() {
    let scene = reference_scene().expect("the reference scene builds");
    let size = ImageSize::new(96, 54).expect("valid size");
    let settings = RenderSettings::new(4, 0).expect("valid settings");
    let schedule = Schedule::new(3, BandRows::new(8).expect("valid band")).expect("valid");
    let render = render_scheduled(&scene, size, settings, schedule).expect("renders");
    assert_eq!(render.image.hash(), GOLDEN_96X54_4SPP);
}

/// One worker: no thread is started, so this runs wherever the library builds, WebAssembly
/// included, and gives the one golden hash every platform must give.
#[test]
fn the_reference_image_renders_on_one_worker_through_the_library_alone() {
    let scene = reference_scene().expect("the reference scene builds");
    let size = ImageSize::new(96, 54).expect("valid size");
    let settings = RenderSettings::new(4, 0).expect("valid settings");
    let render = render_scheduled(&scene, size, settings, Schedule::single()).expect("renders");
    assert_eq!(render.image.hash(), GOLDEN_96X54_4SPP);
}

/// On WebAssembly a thread cannot be started, so asking for more than one worker is
/// `RenderError::Spawn`, not a panic. This is also what shows that the one-worker renders on
/// that target start no thread: if they did, they would fail the same way.
#[cfg(target_family = "wasm")]
#[test]
fn on_webassembly_a_second_worker_is_a_spawn_error() {
    let scene = reference_scene().expect("the reference scene builds");
    let size = ImageSize::new(96, 54).expect("valid size");
    let settings = RenderSettings::new(4, 0).expect("valid settings");
    let schedule = Schedule::new(2, BandRows::new(8).expect("valid band")).expect("valid");
    let refused = render_scheduled(&scene, size, settings, schedule).map(|render| render.rays);
    assert_eq!(refused, Err(lumen::RenderError::Spawn));
}

#[test]
fn the_provenance_lists_what_the_command_prints_in_its_order() {
    let size = ImageSize::new(40, 24).expect("valid size");
    let settings = RenderSettings::new(2, 5).expect("valid settings");
    let provenance = RenderProvenance::new(REFERENCE_SCENE_ID, size, settings);
    let id = ProvenanceValue::Identity;
    let number = ProvenanceValue::Number;
    assert_eq!(
        provenance.fields(),
        [
            ("scene", id("lumen-reference-scene/3")),
            ("integrator", id("lumen-diffuse-path/1")),
            ("shading", id("bunny-diffuse-shading/1")),
            ("sampler", id("bunny-sobol-owen/1")),
            ("tone", id("bunny-tone-srgb/1")),
            ("encoding", id("bunny-rgba8-opaque/1")),
            ("bunny", id(BUNNY_REV)),
            ("size", ProvenanceValue::Size(size)),
            ("pixels", number(960)),
            ("samples", number(2)),
            ("seed", number(5)),
            ("bounces", number(1)),
        ]
    );
}

#[test]
fn a_png_written_to_memory_has_the_bytes_the_command_wrote() {
    let scene = reference_scene().expect("the reference scene builds");
    let size = ImageSize::new(5, 3).expect("valid size");
    let settings = RenderSettings::new(1, u32::MAX).expect("valid settings");
    let image = render(&scene, size, settings).expect("renders").image;
    let mut png = Vec::new();
    write_png(&mut png, &image).expect("writes to memory");
    assert_eq!(png, from_hex(&TINY_PNG_HEX));
    assert_eq!(u64::try_from(png.len()).expect("fits"), png_len(&image));
}

#[test]
fn every_builtin_scene_builds_renders_and_has_its_own_identity() {
    let size = ImageSize::new(8, 4).expect("valid size");
    let settings = RenderSettings::new(1, 0).expect("valid settings");
    let mut ids = std::collections::BTreeSet::new();
    let mut names = std::collections::BTreeSet::new();
    let mut hashes = std::collections::BTreeSet::new();
    for scene in BuiltinScene::ALL {
        let built = scene.build().expect("the scene builds");
        let schedule = Schedule::new(1, BandRows::new(4).expect("valid band")).expect("valid");
        let rendered = render_scheduled(&built, size, settings, schedule).expect("renders");
        assert_eq!(scene.name().parse(), Ok(scene), "the name round-trips");
        assert_eq!(RenderProvenance::new(scene.id(), size, settings).scene(), scene.id());
        ids.insert(scene.id());
        names.insert(scene.name());
        hashes.insert(rendered.image.hash());
    }
    let count = BuiltinScene::ALL.len();
    assert_eq!((ids.len(), names.len(), hashes.len()), (count, count, count));
}
