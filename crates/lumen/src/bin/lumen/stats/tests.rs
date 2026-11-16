//! The statistics messages: the decimal form of a fixed-point value, and every member for a
//! scene small enough to count by hand.

use bunny_optics::{Albedo, DirectionalLight, SkyLight};
use lumen::{CameraPlacement, Lighting, SceneSpec};

use super::*;
use crate::json::Value;

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

/// The `scene_statistics` line of the one-triangle scene.
fn one_triangle_line() -> String {
    let scene = one_triangle();
    scene_statistics("a-name/1", &scene, &scene.statistics().expect("counts")).to_json()
}

/// Oracle: counting by hand. One triangle is one leaf at depth 0 with no compact node; its box
/// is its vertices' extent; the second material names no triangle. The memory member follows,
/// a fact about this build, checked by the next test.
#[test]
fn every_member_of_a_one_triangle_scene() {
    let scene = one_triangle();
    let want = format!(
        concat!(
            r#"{{"schema":"lumen-message/1","type":"scene_statistics","scene":"a-name/1","#,
            r#""scene_content":"{}","integrator":"lumen-diffuse-path/1","#,
            r#""triangles":1,"vertices":3,"triangles_smooth":0,"triangles_zero_area":0,"#,
            r#""material_triangles":[1,0],"#,
            r#""material_albedo":[["0.500000","0.500000","0.500000"],["1.000000","0.000000","0.000000"]],"#,
            r#""bounds_min":["-1.000000","-1.000000","0.000000"],"#,
            r#""bounds_max":["1.000000","1.000000","0.000000"],"#,
            r#""light":{{"sun":{{"toward":["0.000000","1.000000","0.000000"],"#,
            r#""irradiance":["1.000000","2.000000","3.000000"]}},"#,
            r#""sky":["0.500000","0.500000","1.000000"]}},"#,
            r#""camera":{{"position":["0.000000","0.000000","4.000000"],"#,
            r#""look":["0.000000","0.000000","-1.000000"],"up":["0.000000","1.000000","0.000000"],"#,
            r#""tan_half_fov":"1.000000"}},"#,
            r#""tree":{{"builder":"bunny_broadphase::build_bvh_sah","#,
            r#""layout":"bunny_broadphase::compact_bvh","nodes":1,"interior_nodes":0,"leaves":1,"#,
            r#""leaf_depth_max":0,"leaf_depth_total":0,"leaf_triangles":1,"#,
            r#""leaf_triangles_min":1,"leaf_triangles_max":1,"leaf_sizes":[1,0,0,0,0]}},"#,
            r#""memory":{{"#
        ),
        scene.content_id()
    );
    let line = one_triangle_line();
    assert!(line.starts_with(&want), "{line}\nwant {want}");
}

/// Oracle: the sizes the one-triangle scene's buffers have on a 64-bit target (ADR 0007's
/// measurements: a `TraceTriangle` is 72 bytes, an `Option<TriangleSurface>` 224; an albedo is
/// three `i64`s; an index a `u32`). Another pointer width may differ, which is why the schema
/// marks the member information only.
#[cfg(target_pointer_width = "64")]
#[test]
fn the_memory_of_a_one_triangle_scene_on_a_64_bit_target() {
    let want = concat!(
        r#""memory":{"triangles_bytes":72,"surfaces_bytes":224,"vertex_normals_bytes":0,"#,
        r#""material_indices_bytes":4,"materials_bytes":48,"tree_nodes_bytes":0,"#,
        r#""tree_primitive_indices_bytes":4,"total_bytes":352}}"#
    );
    let line = one_triangle_line();
    assert!(line.ends_with(want), "{line}");
}

/// Oracle: a scene without a sun has `"sun":null`, and no sun direction.
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
    let line = scene_statistics("x/1", &scene, &scene.statistics().expect("counts")).to_json();
    assert!(line.contains(r#""light":{"sun":null,"sky":["#), "{line}");
    assert!(!line.contains("toward"), "{line}");
}

/// The command reference, `docs/reference/cli.md`, as it is in the repository.
const REFERENCE: &str = include_str!("../../../../../../docs/reference/cli.md");

/// The section of the command reference about messages of type `kind`: from its heading,
/// ``### `kind` ``, to the next heading.
fn section(kind: &str) -> &'static str {
    let heading = format!("### `{kind}`\n");
    let at = REFERENCE.find(&heading).unwrap_or_else(|| panic!("no section for {kind}"));
    let rest = &REFERENCE[at + heading.len()..];
    &rest[..rest.find("\n#").unwrap_or(rest.len())]
}

/// Every member path of `object`: its keys, a nested object's keys after a dot, and the keys of
/// an array's objects after `[].`.
fn paths(object: &Object, prefix: &str, out: &mut Vec<String>) {
    for (key, value) in object.members() {
        let path = format!("{prefix}{key}");
        match value {
            Value::Object(inner) => paths(inner, &format!("{path}."), out),
            Value::Array(items) => {
                for item in items {
                    if let Value::Object(inner) = item {
                        paths(inner, &format!("{path}[]."), out);
                    }
                }
            }
            _ => {}
        }
        out.push(path);
    }
}

/// Whether the section for messages of type `kind` documents `path`. A traversal member is
/// `KIND.COUNTER` there, with `KIND` and every counter named, and its maximum `COUNTER_max`.
fn documented(kind: &str, path: &str) -> bool {
    let text = section(kind);
    let named = |name: &str| text.contains(&format!("`{name}`"));
    if matches!(path, "schema" | "type") {
        return true;
    }
    match path.split_once('.') {
        Some((ray, counter)) if kind == "traversal_statistics" => {
            named(ray) && (named(counter) || named(counter.trim_end_matches("_max")))
        }
        _ => named(path),
    }
}

/// Oracle: the command reference's member tables. Every member path the statistics messages
/// print, for a scene with a sun, two materials, a rendered run on two workers and the
/// traversal message, is documented in its message's section.
/// Witness count: the paths checked, more than the 40 the scene message alone holds.
#[test]
fn every_printed_member_is_in_the_command_reference() {
    let scene = one_triangle();
    let size = lumen::ImageSize::new(4, 3).expect("size");
    let settings = lumen::RenderSettings::new(2, 0).expect("settings");
    let schedule = lumen::Schedule::new(2, lumen::DEFAULT_BAND_ROWS).expect("schedule");
    let (render, run) =
        lumen::render_traced_with_run(&scene, &scene, size, settings, schedule).expect("renders");
    let messages = [
        ("scene_statistics", scene_statistics("a/1", &scene, &scene.statistics().expect("counts"))),
        ("ray_statistics", ray_statistics(&render.statistics, settings)),
        ("traversal_statistics", traversal_statistics(&lumen::TraversalStatistics::default())),
        ("run_statistics", run_statistics(&run)),
    ];
    let mut checked = 0;
    for (kind, message) in &messages {
        let mut found = Vec::new();
        paths(message, "", &mut found);
        let missing: Vec<&String> = found.iter().filter(|path| !documented(kind, path)).collect();
        assert!(missing.is_empty(), "{kind}: not in docs/reference/cli.md: {missing:?}");
        checked += found.len();
    }
    assert!(checked > 40, "only {checked} members checked");
}

/// Oracle: the line, written by hand from the values set. Camera rays: box tests total 10 (most
/// 5), leaf visits 2 (most 1), peak stack 4; the other counters and kinds are zero. Distinct
/// values per counter, so a counter written under another's name, or a total written as the
/// most, is seen.
#[test]
fn the_traversal_line_of_known_counts() {
    let mut traversal = lumen::TraversalStatistics::default();
    let camera = &mut traversal.camera;
    (camera.total.box_tests, camera.most.box_tests) = (10, 5);
    (camera.total.leaf_visits, camera.most.leaf_visits) = (2, 1);
    camera.total.peak_stack = 4;
    traversal.shadow.total.primitive_tests = 7;
    let zero = concat!(
        r#"{"box_tests":0,"box_entries":0,"popped_skipped":0,"interior_visits":0,"#,
        r#""leaf_visits":0,"primitive_tests":0,"box_tests_max":0,"box_entries_max":0,"#,
        r#""popped_skipped_max":0,"interior_visits_max":0,"leaf_visits_max":0,"#,
        r#""primitive_tests_max":0,"peak_stack":0}"#
    );
    let want = format!(
        concat!(
            r#"{{"schema":"lumen-message/1","type":"traversal_statistics","#,
            r#""camera":{{"box_tests":10,"box_entries":0,"popped_skipped":0,"interior_visits":0,"#,
            r#""leaf_visits":2,"primitive_tests":0,"box_tests_max":5,"box_entries_max":0,"#,
            r#""popped_skipped_max":0,"interior_visits_max":0,"leaf_visits_max":1,"#,
            r#""primitive_tests_max":0,"peak_stack":4}},"bounce":{},"shadow":{}}}"#
        ),
        zero,
        zero.replace(r#""primitive_tests":0"#, r#""primitive_tests":7"#)
    );
    assert_eq!(traversal_statistics(&traversal).to_json(), want);
}
