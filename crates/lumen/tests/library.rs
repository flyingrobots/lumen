//! Size class: small (in process, no file, no network).
//!
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
use golden::{
    GOLDEN_96X54_4SPP, GOLDEN_CANYON_CONTENT, GOLDEN_FIXED_POINT_CONTENT, GOLDEN_REFERENCE_CONTENT,
};

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
/// `RenderError::Spawn` for worker 1, the first helper, not a panic. This is also what shows that the one-worker renders on
/// that target start no thread: if they did, they would fail the same way.
#[cfg(target_family = "wasm")]
#[test]
fn on_webassembly_a_second_worker_is_a_spawn_error() {
    let scene = reference_scene().expect("the reference scene builds");
    let size = ImageSize::new(96, 54).expect("valid size");
    let settings = RenderSettings::new(4, 0).expect("valid settings");
    let schedule = Schedule::new(2, BandRows::new(8).expect("valid band")).expect("valid");
    let refused = render_scheduled(&scene, size, settings, schedule).map(|render| render.rays);
    assert!(
        matches!(&refused, Err(lumen::RenderError::Spawn(error)) if error.worker() == 1),
        "{refused:?}"
    );
}

#[test]
fn the_provenance_lists_what_the_command_prints_in_its_order() {
    let scene = reference_scene().expect("the reference scene builds");
    let size = ImageSize::new(40, 24).expect("valid size");
    let settings = RenderSettings::new(2, 5).expect("valid settings");
    let provenance = RenderProvenance::named(REFERENCE_SCENE_ID, &scene, size, settings);
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
            ("scene-content", ProvenanceValue::Content(scene.content_id())),
        ]
    );
    let unnamed = RenderProvenance::new(&scene, size, settings);
    assert_eq!(unnamed.fields()[0], ("scene", ProvenanceValue::Unnamed));
    assert_eq!(unnamed.fields()[1..], provenance.fields()[1..]);
}

/// The issue this field closes (flyingrobots/lumen#38): two scenes under one label used to have
/// equal provenance. Now the content identity tells them apart.
#[test]
fn two_scenes_under_one_name_have_different_provenance() {
    let size = ImageSize::new(8, 4).expect("valid size");
    let settings = RenderSettings::new(1, 0).expect("valid settings");
    let under_one_name = |scene: BuiltinScene| {
        let built = scene.build().expect("the scene builds");
        RenderProvenance::named("one-label", &built, size, settings)
    };
    let (reference, fixed_point) =
        (under_one_name(BuiltinScene::Reference), under_one_name(BuiltinScene::FixedPoint));
    assert_eq!(reference.name(), fixed_point.name());
    assert_ne!(reference, fixed_point);
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
        let provenance = RenderProvenance::named(scene.id(), &built, size, settings);
        assert_eq!(provenance.name(), Some(scene.id()));
        let golden = match scene {
            BuiltinScene::Reference => GOLDEN_REFERENCE_CONTENT,
            BuiltinScene::FixedPoint => GOLDEN_FIXED_POINT_CONTENT,
            BuiltinScene::Canyon => GOLDEN_CANYON_CONTENT,
        };
        assert_eq!(provenance.scene_content().to_string(), golden, "{}", scene.name());
        ids.insert(scene.id());
        names.insert(scene.name());
        hashes.insert(rendered.image.hash());
    }
    let count = BuiltinScene::ALL.len();
    assert_eq!((ids.len(), names.len(), hashes.len()), (count, count, count));
}

/// `lumen::mesh` is public for its placement and error types; its reader is Bunny's, and a
/// refusal reaches the caller as Bunny's typed error with its line number, unchanged.
#[test]
fn the_mesh_module_reads_with_bunnys_reader_and_refuses_with_its_error() {
    let text = b"ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\n\
property float z\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n\
0 0 0\n1 0 0\n0 1 0\n3 0 1 2\n";
    let mesh = lumen::mesh::read_ascii_ply(text).expect("a triangle reads");
    assert_eq!((mesh.vertices.len(), mesh.faces), (3, vec![[0, 1, 2]]));
    let refused = lumen::mesh::read_ascii_ply(&text[..text.len() - 8]).expect_err("truncated");
    assert_eq!(refused.kind(), bunny_codec::AsciiPlyErrorKind::PayloadTooShort);
    assert_eq!(lumen::mesh::PlaceError::Ply(refused), lumen::mesh::PlaceError::Ply(refused));
}

/// One triangle, one albedo, a sun, a sky and a camera, without and with vertex normals, with the
/// content identities computed outside lumen: Python built the byte streams from the table in ADR
/// 0005 (`lumen-scene-content/2`) and hashed them with `hashlib`, so this does not rest on lumen's
/// SHA-256 or its encoder agreeing with themselves. The script, with `ONE = 1 << 32` and
/// `i64 = lambda *v: struct.pack('<%dq' % len(v), *v)`:
///
/// ```text
/// b = struct.pack('<Q', 21) + b'lumen-scene-content/2'
/// b += struct.pack('<Q', 1)                                        # one triangle
/// b += i64(0, 0, 0, ONE, 0, 0, 0, ONE, 0) + struct.pack('<I', 0)   # its vertices, material 0
/// b += struct.pack('<Q', 1) + i64(ONE // 2, ONE // 2, ONE // 2)    # one albedo
/// b += b'\x01' + i64(0, 0, ONE) + i64(ONE, ONE, ONE)               # sun: direction, irradiance
/// b += i64(ONE // 4, ONE // 4, ONE // 4)                           # sky
/// b += i64(0, 0, 5 * ONE, 0, 0, -ONE, 0, ONE, 0, ONE // 2)         # camera
/// flat = b + struct.pack('<Q', 0)                                  # no vertex normals
/// hashlib.sha256(flat).hexdigest()  # 306 bytes
/// smooth = b + struct.pack('<Q', 1)                                # one entry: +Z, none, +Y
/// smooth += b'\x01' + i64(0, 0, ONE) + b'\x00' + b'\x01' + i64(0, ONE, 0)
/// hashlib.sha256(smooth).hexdigest()  # 357 bytes
/// ```
#[test]
fn a_one_triangle_scene_has_the_content_identity_python_computed() {
    use bunny_linalg::{FixedUnitVec3, FixedVec3};
    use bunny_num::FixedQ32_32;
    use bunny_optics::{Albedo, DirectionalLight, SkyLight};
    use lumen::{CameraPlacement, Lighting, Scene, SceneSpec};
    let raw = FixedQ32_32::from_raw;
    let one = 1_i64 << 32;
    let at = |x: i64, y: i64, z: i64| FixedVec3::new(raw(x), raw(y), raw(z));
    let scene = |normals: &[[Option<FixedUnitVec3>; 3]]| {
        Scene::new(&SceneSpec {
            triangles: &[[at(0, 0, 0), at(one, 0, 0), at(0, one, 0)]],
            material_of: &[0],
            materials: &[Albedo::new([raw(one / 2); 3]).expect("valid albedo")],
            normals,
            lighting: Lighting {
                sun: Some(DirectionalLight::new(at(0, 0, one), [raw(one); 3]).expect("valid sun")),
                sky: SkyLight::new([raw(one / 4); 3]).expect("valid sky"),
            },
            camera: CameraPlacement {
                position: at(0, 0, 5 * one),
                look: at(0, 0, -one),
                up: at(0, one, 0),
                tan_half_fov: raw(one / 2),
            },
        })
    };
    let id = |normals: &[[Option<FixedUnitVec3>; 3]]| {
        scene(normals).expect("the scene builds").content_id().to_string()
    };
    assert_eq!(id(&[]), "sha256:9241f863968f50ae6c6ad6d828e0d5b57d91781e5b27be76f1bbd906087b0f6b");
    let normals = [Some(FixedUnitVec3::UNIT_Z), None, Some(FixedUnitVec3::UNIT_Y)];
    assert_eq!(
        id(&[normals]),
        "sha256:74ee72e21d66eb96e9efece03eb7722895785dd46bf1af23d80cded65dd457c5"
    );
}

/// The crate root's source and the library reference, as they are in the repository.
const LIB_RS: &str = include_str!("../src/lib.rs");
const LIBRARY_REFERENCE: &str = include_str!("../../../docs/reference/library.md");

/// The names `lib.rs` makes public: every name a `pub use` brings in (the last path segment of
/// each entry), and every `pub const` and `pub mod`.
fn crate_root_names() -> Vec<&'static str> {
    let mut names = Vec::new();
    for (at, _) in LIB_RS.match_indices("\npub use ") {
        let rest = &LIB_RS[at + "\npub use ".len()..];
        let item = &rest[..rest.find(';').expect("a use declaration ends")];
        let list = item.split_once('{').map_or(item, |(_, list)| list.trim_end_matches('}'));
        let entries = list.split(',').map(|entry| entry.trim().rsplit("::").next().unwrap_or(""));
        names.extend(entries.filter(|name| !name.is_empty()));
    }
    for keyword in ["\npub const ", "\npub mod "] {
        for (at, _) in LIB_RS.match_indices(keyword) {
            let rest = &LIB_RS[at + keyword.len()..];
            let end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(rest.len());
            names.push(&rest[..end]);
        }
    }
    names
}

/// Oracle: the crate root's own source. Every name `lib.rs` makes public is written, in
/// backticks, in `docs/reference/library.md`, so the page lists the whole surface the rustdoc
/// documents. Witness count: the 82 names of the crate root when this test was written, or more.
#[test]
fn the_library_reference_names_every_item_of_the_crate_root() {
    let names = crate_root_names();
    let missing: Vec<&&str> =
        names.iter().filter(|name| !LIBRARY_REFERENCE.contains(&format!("`{name}`"))).collect();
    assert!(missing.is_empty(), "not in docs/reference/library.md: {missing:?}");
    assert!(names.len() >= 82, "only {} names found in lib.rs", names.len());
}
