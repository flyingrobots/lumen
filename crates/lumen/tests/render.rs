//! Render tests: the BVH against brute force, hand-derived pixels, determinism and the golden
//! image hash.

use bunny_linalg::FixedVec3;
use bunny_num::fixed_q32_32::ONE_RAW;
use bunny_num::FixedQ32_32;
use bunny_optics::{PinholeCamera, PixelSample};
use bunny_query::TraceRay;
use lumen::render::{render, trace_pixel};
use lumen::scene::{reference_camera, CameraPlacement, Hit};
use lumen::{reference_scene, render_reference, ImageSize, Rgba8Image, Scene, BUNNY_REV};

/// `n` whole units.
fn units(n: i64) -> FixedQ32_32 {
    FixedQ32_32::from_raw(n * ONE_RAW)
}

fn point(x: i64, y: i64, z: i64) -> FixedVec3 {
    FixedVec3::new(units(x), units(y), units(z))
}

/// The hand-derived codes. sRGB encoding of `(n + 1) / 2` for normal components `n`:
/// `n = 0` gives 0.5, encoded `1.055 * 0.5^(1 / 2.4) - 0.055 = 0.73536`, times 255 is 187.52,
/// code 188; `n = 1` gives 1.0, code 255; `n = -1` gives 0, code 0. The sky's blue, linear
/// 0.25, encodes to 0.53710, times 255 is 136.96, code 137. For an octahedron face,
/// `n = +-1 / sqrt(3)` gives 0.78868 (229.66, code 230) or 0.21132 (126.75, code 127).
const SKY: [u8; 4] = [0, 0, 137, 255];
const FACING_PLUS_Z: [u8; 4] = [188, 188, 255, 255];
const FACING_PLUS_Y: [u8; 4] = [188, 255, 188, 255];

/// The nearest hit by testing every triangle: the smallest distance, then the lowest index.
fn brute_force(scene: &Scene, ray: &TraceRay) -> Option<(u32, FixedQ32_32, bool)> {
    let far = FixedQ32_32::from_raw(i64::MAX);
    let mut best: Option<(u32, FixedQ32_32, bool)> = None;
    for (index, triangle) in scene.triangles().iter().enumerate() {
        if let Some(hit) = ray.hit_triangle(triangle, FixedQ32_32::ZERO, far) {
            if best.is_none_or(|(_, distance, _)| hit.distance < distance) {
                let index = u32::try_from(index).expect("fits");
                best = Some((index, hit.distance, hit.front_face));
            }
        }
    }
    best
}

fn camera(scene: &Scene, width: u32, height: u32) -> PinholeCamera {
    scene.camera().camera(width, height).expect("the camera accepts the size")
}

/// The ray of pixel `(x, y)`, built here from Bunny's camera, independently of the renderer.
fn ray(camera: &PinholeCamera, x: u32, y: u32) -> TraceRay {
    camera.ray(PixelSample::centre(x, y)).expect("inside the image")
}

/// Compares the renderer's hit for every pixel with brute force, at a wide and a square size
/// (on a square image a renderer that swapped `x` and `y` would still get a ray per pixel,
/// so only the comparison of hits can catch it).
#[test]
fn every_pixel_hit_equals_a_test_of_every_triangle() {
    let scene = reference_scene().expect("builds");
    for (width, height) in [(16, 16), (32, 18)] {
        let camera = camera(&scene, width, height);
        let (mut hits, mut misses) = (0, 0);
        let mut objects = [0_u32; 4];
        for y in 0..height {
            for x in 0..width {
                let found = trace_pixel(&scene, &camera, x, y).expect("traces");
                let expected = brute_force(&scene, &ray(&camera, x, y));
                let found_triple =
                    found.map(|hit: Hit| (hit.primitive, hit.distance, hit.front_face));
                assert_eq!(found_triple, expected, "{width}x{height}, pixel ({x}, {y})");
                match found {
                    None => misses += 1,
                    Some(hit) => {
                        hits += 1;
                        objects[match hit.primitive {
                            0 | 1 => 0,
                            2 | 3 => 1,
                            4..=11 => 2,
                            _ => 3,
                        }] += 1;
                    }
                }
            }
        }
        // The comparison means something only if the picture holds every object and sky.
        assert!(misses > 0 && hits > 0, "{width}x{height}: {hits} hits, {misses} misses");
        assert!(objects.iter().all(|&n| n > 0), "{width}x{height}: per object {objects:?}");
    }
}

#[test]
fn reference_pixels_have_their_hand_derived_colours() {
    let scene = reference_scene().expect("builds");
    let (width, height) = (64, 36);
    let size = ImageSize::new(width, height).expect("valid");
    let image = render(&scene, size).expect("renders");
    let camera = camera(&scene, width, height);
    // The top-left ray rises to the left of the wall and over the ground: sky.
    assert_eq!(image.pixel(0, 0), Some(SKY));
    // The bottom-centre ray falls steeply and meets the ground in front of the sphere.
    assert_eq!(image.pixel(width / 2, height - 1), Some(FACING_PLUS_Y));
    let mut seen = [0_u32; 4];
    for y in 0..height {
        for x in 0..width {
            let hit = brute_force(&scene, &ray(&camera, x, y));
            let pixel = image.pixel(x, y).expect("inside");
            let (kind, want) = match hit {
                None => (0, SKY),
                Some((0 | 1, _, front)) => {
                    assert!(front, "the camera is above the ground");
                    (1, FACING_PLUS_Y)
                }
                Some((2 | 3, _, front)) => {
                    assert!(front, "the camera is in front of the wall");
                    (2, FACING_PLUS_Z)
                }
                Some((primitive @ 4..=11, _, _)) => {
                    let normal = scene.normals()[primitive as usize];
                    let code = |c: FixedQ32_32| if c.raw() > 0 { 230 } else { 127 };
                    (3, [code(normal.x), code(normal.y), code(normal.z), 255])
                }
                Some(_) => continue,
            };
            seen[kind] += 1;
            assert_eq!(pixel, want, "pixel ({x}, {y})");
        }
    }
    assert!(seen.iter().all(|&count| count > 0), "pixels per kind: {seen:?}");
}

/// One triangle facing `+Z` (or `-Z` when `reversed`) at `z = 0`, seen from `(0, 0, 4)`
/// looking down `-Z` with a 3 by 3 image.
fn one_triangle(reversed: bool) -> Scene {
    let (a, b, c) = (point(-1, -1, 0), point(1, -1, 0), point(0, 1, 0));
    let triangle = if reversed { [a, c, b] } else { [a, b, c] };
    let camera = CameraPlacement {
        position: point(0, 0, 4),
        look: point(0, 0, -1),
        up: point(0, 1, 0),
        tan_half_fov: FixedQ32_32::from_raw(ONE_RAW / 2),
    };
    Scene::new(&[triangle], camera).expect("builds")
}

/// By hand: with tangent 0.5 and a square image, every corner direction is within 1.0, so
/// Bunny's scale shift is 0 and the centre pixel's direction is exactly `(0, 0, -1)`. It
/// meets the triangle at `(0, 0, 0)`, distance exactly 4. Every other pixel centre lands
/// 4/3 units off-axis at `z = 0` (for example `(0, 4/3)` or `(-4/3, 0)`), outside the
/// triangle, whose top is at `y = 1` and whose width at `y = 0` is `-0.5..=0.5`.
#[test]
fn a_hand_checked_one_triangle_scene() {
    for reversed in [false, true] {
        let scene = one_triangle(reversed);
        let size = ImageSize::new(3, 3).expect("valid");
        let image = render(&scene, size).expect("renders");
        let camera = camera(&scene, 3, 3);
        assert_eq!(camera.scale_shift(), 0);
        assert_eq!(ray(&camera, 1, 1).direction(), point(0, 0, -1));
        let hit = trace_pixel(&scene, &camera, 1, 1).expect("traces");
        let front_face = !reversed;
        assert_eq!(hit, Some(Hit { primitive: 0, distance: units(4), front_face }));
        // The normal is +Z, or -Z for the reversed winding; either way it is turned to face
        // the ray, which travels along -Z, so the colour is that of +Z.
        let mut want = Vec::new();
        for index in 0..9 {
            want.extend_from_slice(if index == 4 { &FACING_PLUS_Z } else { &SKY });
        }
        assert_eq!(image.bytes(), want, "reversed: {reversed}");
    }
}

/// Bunny's camera refuses no reference size from 1 by 1 to the largest axis.
#[test]
fn extreme_sizes_have_a_camera() {
    let placement = reference_camera();
    for (width, height) in [(1, 1), (1, 32_768), (32_768, 1), (16_384, 8_192)] {
        assert!(placement.camera(width, height).is_ok(), "{width}x{height}");
    }
}

#[test]
fn rendering_twice_in_one_process_gives_the_same_bytes() {
    let first = render_reference(48, 27).expect("renders");
    let second = render_reference(48, 27).expect("renders");
    assert_eq!(first.bytes(), second.bytes());
    assert_eq!(first.hash(), second.hash());
    // A different size is a different image.
    let other = render_reference(27, 48).expect("renders");
    assert_ne!(first.hash(), other.hash());
}

/// The golden image hash of the reference scene.
///
/// Recorded for `lumen-reference-scene/1`, `lumen-normal-shading/1`, `bunny-tone-srgb/1`,
/// one sample per pixel at the pixel centre, 96 by 54 pixels, built against Bunny
/// 7fdd2e4926041b0a226567e0a6e1ec51e522304f. First recorded on aarch64 Linux (the
/// rust-dev:1.96.0 container on an Apple M5 Pro); CI checks it on x86-64 Linux in both the
/// test and the release profile. It must be identical on every platform and build profile.
///
/// Updating it is legitimate only when the scene version (`REFERENCE_SCENE_ID`), the shading
/// rule version (`SHADING_ID`), the tone curve or the pinned Bunny commit (`BUNNY_REV`)
/// changes, and the change of hash is the intended effect of that change. Any other change of
/// this hash is a determinism bug: do not update it to make the test pass.
const GOLDEN_96X54: &str =
    "sha256:280df5901599a4097f4ccc4d333b7a733a974f33c44b2c41180610cc342b59c8";

#[test]
fn the_reference_image_has_its_golden_hash() {
    assert_eq!(lumen::REFERENCE_SCENE_ID, "lumen-reference-scene/1");
    assert_eq!(lumen::SHADING_ID, "lumen-normal-shading/1");
    assert_eq!(lumen::render::TONE_CURVE.identity(), "bunny-tone-srgb/1");
    assert_eq!(BUNNY_REV, "7fdd2e4926041b0a226567e0a6e1ec51e522304f");
    let image: Rgba8Image = render_reference(96, 54).expect("renders");
    assert_eq!(image.hash(), GOLDEN_96X54);
}

#[test]
fn the_lockfile_resolves_bunny_to_the_recorded_commit() {
    let lock = include_str!("../../../Cargo.lock");
    let pinned = format!("?rev={BUNNY_REV}#{BUNNY_REV}\"");
    let bunny_sources: Vec<&str> =
        lock.lines().filter(|line| line.contains("flyingrobots/bunny")).collect();
    assert_eq!(bunny_sources.len(), 6, "six Bunny crates: {bunny_sources:?}");
    assert!(bunny_sources.iter().all(|line| line.ends_with(&pinned)), "{bunny_sources:?}");
}
