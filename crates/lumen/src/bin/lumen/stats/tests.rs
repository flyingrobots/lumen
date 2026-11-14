//! The statistics text: the decimal forms, and every line for a scene small enough to count by
//! hand.

use bunny_optics::{Albedo, DirectionalLight, SkyLight};
use lumen::{CameraPlacement, Lighting, SceneSpec};

use super::*;

const ONE: i64 = 1 << 32;

fn fixed(raw: i64) -> String {
    Fixed(FixedQ32_32::from_raw(raw)).to_string()
}

/// A point in whole units.
fn at(x: i64, y: i64, z: i64) -> FixedVec3 {
    let unit = |n: i64| FixedQ32_32::from_raw(n * ONE);
    FixedVec3::new(unit(x), unit(y), unit(z))
}

/// Oracle: the exact decimal value of `raw / 2^32`, worked by hand. 1/128 is 0.0078125, a tie
/// at six places, which goes away from zero; -2^-32 rounds to zero and prints no sign.
#[test]
fn fixed_point_values_print_in_decimal_with_six_places() {
    assert_eq!(fixed(ONE), "1.000000");
    assert_eq!(fixed(-3 * ONE / 2), "-1.500000");
    assert_eq!(fixed(ONE / 128), "0.007813");
    assert_eq!(fixed(-ONE / 128), "-0.007813");
    assert_eq!(fixed(ONE / 128 - 1), "0.007812");
    assert_eq!(fixed(-1), "0.000000");
    assert_eq!(fixed(i64::MAX), "2147483648.000000");
    assert_eq!(fixed(i64::MIN), "-2147483648.000000");
}

/// Oracle: long division by hand, two places, ties up.
#[test]
fn ratios_print_with_two_places_rounded_to_nearest() {
    assert_eq!(Ratio(0, 0).to_string(), "0.00");
    assert_eq!(Ratio(7, 7).to_string(), "1.00");
    assert_eq!(Ratio(2, 3).to_string(), "0.67");
    assert_eq!(Ratio(1, 8).to_string(), "0.13");
    assert_eq!(Ratio(1, 200).to_string(), "0.01");
    assert_eq!(Ratio(1, 201).to_string(), "0.00");
    assert_eq!(Ratio(426_614, 27_639).to_string(), "15.44");
}

/// One triangle under a sun straight above, two materials (the second unused), seen from four
/// units in front.
fn one_triangle() -> Scene {
    let half = |n: i64| FixedQ32_32::from_raw(n * ONE / 2);
    let albedo = |a, b, c| Albedo::new([half(a), half(b), half(c)]).expect("albedo");
    let sun = DirectionalLight::new(at(0, 1, 0), [half(2), half(4), half(6)]).expect("sun");
    let sky = SkyLight::new([half(1), half(1), half(2)]).expect("sky");
    Scene::new(&SceneSpec {
        triangles: &[[at(-1, -1, 0), at(1, -1, 0), at(0, 1, 0)]],
        material_of: &[0],
        materials: &[albedo(1, 1, 1), albedo(2, 0, 0)],
        normals: &[],
        lighting: Lighting { sun: Some(sun), sky },
        camera: CameraPlacement {
            position: at(0, 0, 4),
            look: at(0, 0, -1),
            up: at(0, 1, 0),
            tan_half_fov: FixedQ32_32::from_raw(ONE),
        },
    })
    .expect("builds")
}

/// Oracle: counting by hand. One triangle is one leaf at depth 0 with no compact node; its box
/// is its vertices' extent. The byte counts are the sizes Bunny's and lumen's types have on a
/// 64-bit target (ADR 0007's measurements: a `TraceTriangle` is 72 bytes, an
/// `Option<TriangleSurface>` 224; an albedo is three `i64`s; an index a `u32`).
#[test]
fn every_line_of_a_one_triangle_scene() {
    let scene = one_triangle();
    let mut text = Vec::new();
    write_scene_block(&mut text, &scene, &scene.statistics().expect("counts")).expect("writes");
    let want = "\
scene.triangles 1
scene.vertices 3
scene.triangles-smooth 0
scene.triangles-zero-area 0
scene.materials 2
scene.material.0.triangles 1
scene.material.0.albedo 0.500000 0.500000 0.500000
scene.material.1.triangles 0
scene.material.1.albedo 1.000000 0.000000 0.000000
scene.bounds-min -1.000000 -1.000000 0.000000
scene.bounds-max 1.000000 1.000000 0.000000
light.sun present
light.sun-toward 0.000000 1.000000 0.000000
light.sun-irradiance 1.000000 2.000000 3.000000
light.sky 0.500000 0.500000 1.000000
camera.position 0.000000 0.000000 4.000000
camera.look 0.000000 0.000000 -1.000000
camera.up 0.000000 1.000000 0.000000
camera.tan-half-fov 1.000000
tree.builder bunny_broadphase::build_bvh_sah
tree.layout bunny_broadphase::compact_bvh
tree.nodes 1
tree.interior-nodes 0
tree.leaves 1
tree.leaf-depth-max 0
tree.leaf-depth-total 0
tree.leaf-depth-mean 0.00
tree.leaf-triangles 1
tree.leaf-triangles-min 1
tree.leaf-triangles-max 1
tree.leaf-sizes 1 0 0 0 0
memory.triangles-bytes 72
memory.surfaces-bytes 224
memory.vertex-normals-bytes 0
memory.material-indices-bytes 4
memory.materials-bytes 48
memory.tree-nodes-bytes 0
memory.tree-primitive-indices-bytes 4
memory.total-bytes 352
";
    assert_eq!(String::from_utf8(text).expect("utf-8"), want);
}

/// Oracle: the header is the scene's name, its content identity and its integrator, as the
/// render report prints them.
#[test]
fn the_stats_command_starts_with_the_scene_it_describes() {
    let scene = one_triangle();
    let mut text = Vec::new();
    write_stats(&mut text, "a-name/1", &scene, &scene.statistics().expect("counts"))
        .expect("writes");
    let text = String::from_utf8(text).expect("utf-8");
    let want = format!(
        "scene a-name/1\nscene-content {}\nintegrator lumen-diffuse-path/1\nscene.triangles 1\n",
        scene.content_id()
    );
    assert!(text.starts_with(&want), "{text}");
}

/// Oracle: a scene without a sun has one `light.sun none` line and no sun direction.
#[test]
fn a_scene_without_a_sun_says_so() {
    let base = lumen::reference_scene().expect("builds");
    let triangles = [[at(-1, -1, 0), at(1, -1, 0), at(0, 1, 0)]];
    let sky = base.lighting().sky;
    let scene = Scene::new(&SceneSpec {
        triangles: &triangles,
        material_of: &[0],
        materials: base.materials(),
        normals: &[],
        lighting: Lighting { sun: None, sky },
        camera: base.camera(),
    })
    .expect("builds");
    let mut text = Vec::new();
    write_scene_block(&mut text, &scene, &scene.statistics().expect("counts")).expect("writes");
    let text = String::from_utf8(text).expect("utf-8");
    assert!(text.contains("\nlight.sun none\nlight.sky "), "{text}");
    assert!(!text.contains("light.sun-toward"), "{text}");
}
