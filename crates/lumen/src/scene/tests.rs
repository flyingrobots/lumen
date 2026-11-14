//! The scene's unit tests: construction refusals and the tree's shape.

use bunny_optics::TAN_HALF_FOV_LIMIT_RAW;
use bunny_query::TraceRay;

use super::*;
use crate::reference::{per_256, reference_camera, reference_scene, REFERENCE_TRIANGLE_COUNT};
use crate::vertex_grid::{at, grid, U};

fn grey() -> Albedo {
    Albedo::new([per_256(128); 3]).expect("valid")
}

fn dark() -> Lighting {
    Lighting { sun: None, sky: SkyLight::new([FixedQ32_32::ZERO; 3]).expect("valid") }
}

/// A tree over n primitives has at most n - 1 interior nodes.
#[test]
fn the_reference_tree_has_fewer_interior_nodes_than_triangles() {
    let scene = reference_scene().expect("the reference scene builds");
    assert!(scene.tree.nodes.len() < REFERENCE_TRIANGLE_COUNT);
}

#[test]
fn scene_construction_refuses_bad_input() {
    let camera = reference_camera();
    let materials = [grey()];
    let spec = |triangles: &[[FixedVec3; 3]], material_of: &[u32]| {
        let spec =
            SceneSpec { triangles, material_of, materials: &materials, lighting: dark(), camera };
        Scene::new(&spec).err()
    };
    assert_eq!(spec(&[], &[]), Some(SceneError::Empty));
    let far = FixedVec3::new(grid(256 * U), FixedQ32_32::ZERO, FixedQ32_32::ZERO);
    let fine = at(0, 0, 0);
    let good = [fine, at(U, 0, 0), at(0, U, 0)];
    let triangles = [good, [fine, far, at(0, U, 0)]];
    assert_eq!(spec(&triangles, &[0, 0]), Some(SceneError::OutsideTraceDomain { primitive: 1 }));
    assert_eq!(spec(&[good], &[]), Some(SceneError::MaterialCount));
    assert_eq!(spec(&[good, good], &[0, 1]), Some(SceneError::UnknownMaterial { primitive: 1 }));
    // A zero-area triangle is accepted and has no surface.
    let flat = [fine, fine, at(U, 0, 0)];
    let built = Scene::new(&SceneSpec {
        triangles: &[good, flat],
        material_of: &[0, 0],
        materials: &materials,
        lighting: dark(),
        camera,
    })
    .expect("builds");
    // `good` winds counter-clockwise seen from +Z, so its unit normal is exactly +Z.
    let normal = |primitive| built.surface(primitive).map(|surface| surface.frame().normal());
    assert_eq!(normal(0), Some(at(0, 0, U)));
    assert_eq!(normal(1), None);
    assert_eq!(normal(2), None);
}

/// `Scene::new` refuses a camera placement that Bunny's `PinholeCamera::new` refuses, with
/// Bunny's reason, one invalid field at a time; the oracle is the refusal `PinholeCamera::new`
/// documents for that field. The limits themselves are accepted.
#[test]
fn scene_construction_refuses_each_invalid_camera_field() {
    let good = reference_camera();
    let triangle = [at(-U, -U, 0), at(U, -U, 0), at(0, U, 0)];
    let materials = [grey()];
    let refusal = |camera: CameraPlacement| {
        let spec = SceneSpec {
            triangles: &[triangle],
            material_of: &[0],
            materials: &materials,
            lighting: dark(),
            camera,
        };
        Scene::new(&spec).err()
    };
    let raw = FixedQ32_32::from_raw;
    let edge = 256 * U;
    let cases = [
        (
            CameraPlacement { position: at(edge, 0, 0), ..good },
            CameraError::PositionOutsideTraceDomain,
        ),
        (
            CameraPlacement { position: at(0, 0, -edge), ..good },
            CameraError::PositionOutsideTraceDomain,
        ),
        (CameraPlacement { look: at(0, 0, 0), ..good }, CameraError::ZeroLook),
        (CameraPlacement { up: at(0, 0, 0), ..good }, CameraError::UpParallelToLook),
        (CameraPlacement { up: good.look, ..good }, CameraError::UpParallelToLook),
        (
            CameraPlacement { tan_half_fov: FixedQ32_32::ZERO, ..good },
            CameraError::TangentNotPositive,
        ),
        (CameraPlacement { tan_half_fov: grid(-U), ..good }, CameraError::TangentNotPositive),
        (
            CameraPlacement { tan_half_fov: raw(TAN_HALF_FOV_LIMIT_RAW + 1), ..good },
            CameraError::TangentTooLarge,
        ),
    ];
    for (camera, reason) in cases {
        assert_eq!(refusal(camera), Some(SceneError::Camera(reason)), "{camera:?}");
    }
    let limits = CameraPlacement {
        position: at(edge - 1, 0, -(edge - 1)),
        tan_half_fov: raw(TAN_HALF_FOV_LIMIT_RAW),
        ..good
    };
    assert_eq!(refusal(limits), None);
}

/// A one-triangle scene is a one-leaf tree, which the compact layout stores in its root
/// with no compact node; tracing still works.
#[test]
fn a_one_triangle_scene_traces() {
    let triangle = [at(-U, -U, 0), at(U, -U, 0), at(0, U, 0)];
    let scene = Scene::new(&SceneSpec {
        triangles: &[triangle],
        material_of: &[0],
        materials: &[grey()],
        lighting: dark(),
        camera: reference_camera(),
    })
    .expect("builds");
    assert!(scene.tree.nodes.is_empty());
    let toward = TraceRay::new(at(0, 0, 4 * U), at(0, 0, -U)).expect("ray");
    let hit = scene.closest(&toward).expect("traces").expect("hits");
    assert_eq!((hit.primitive, hit.hit.distance, hit.hit.front_face), (0, grid(4 * U), true));
    assert!(scene.occluded(&toward, FAR).expect("traces"));
    assert!(!scene.occluded(&toward, grid(4 * U - 1)).expect("traces"));
    let away = TraceRay::new(at(0, 0, 4 * U), at(0, 0, U)).expect("ray");
    assert_eq!(scene.closest(&away).expect("traces"), None);
    assert!(!scene.occluded(&away, FAR).expect("traces"));
}
