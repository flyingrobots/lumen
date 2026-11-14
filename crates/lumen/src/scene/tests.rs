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
        let spec = SceneSpec {
            triangles,
            material_of,
            materials: &materials,
            normals: &[],
            lighting: dark(),
            camera,
        };
        Scene::new(&spec).err()
    };
    assert_eq!(spec(&[], &[]), Some(SceneError::Empty));
    let far = FixedVec3::new(grid(256 * U), FixedQ32_32::ZERO, FixedQ32_32::ZERO);
    let fine = at(0, 0, 0);
    let good = [fine, at(U, 0, 0), at(0, U, 0)];
    let triangles = [good, [fine, far, at(0, U, 0)]];
    let second = PrimitiveIndex::new(1);
    // Oracle: the vertices as written. The second triangle's vertex 1 is 256 units out on x.
    assert_eq!(
        spec(&triangles, &[0, 0]),
        Some(SceneError::OutsideTraceDomain { primitive: second, vertex: 1, point: far })
    );
    let refused = SceneError::MaterialCount { triangles: 1, material_indices: 0 };
    assert_eq!(spec(&[good], &[]), Some(refused));
    assert_eq!(
        spec(&[good, good], &[0, 1]),
        Some(SceneError::UnknownMaterial { primitive: second, material: 1, materials: 1 })
    );
    // A zero-area triangle is accepted and has no surface.
    let flat = [fine, fine, at(U, 0, 0)];
    let built = Scene::new(&SceneSpec {
        triangles: &[good, flat],
        material_of: &[0, 0],
        materials: &materials,
        normals: &[],
        lighting: dark(),
        camera,
    })
    .expect("builds");
    // `good` winds counter-clockwise seen from +Z, so its unit normal is exactly +Z.
    let normal = |primitive| {
        built.surface(PrimitiveIndex::new(primitive)).map(|surface| surface.frame().normal())
    };
    assert_eq!(normal(0), Some(at(0, 0, U)));
    assert_eq!(normal(1), None);
    assert_eq!(normal(2), None);
}

/// Vertex normals are none or one entry per triangle, and a scene returns each triangle's as
/// given.
#[test]
fn a_scene_keeps_one_entry_of_vertex_normals_per_triangle_or_none() {
    use bunny_linalg::FixedUnitVec3;
    let materials = [grey()];
    let good = [at(0, 0, 0), at(U, 0, 0), at(0, U, 0)];
    let up = Some(FixedUnitVec3::UNIT_Z);
    let build = |normals: &[[Option<FixedUnitVec3>; 3]]| {
        Scene::new(&SceneSpec {
            triangles: &[good, good],
            material_of: &[0, 0],
            materials: &materials,
            normals,
            lighting: dark(),
            camera: reference_camera(),
        })
    };
    let refused = build(&[[up; 3]]).err();
    assert_eq!(refused, Some(SceneError::NormalCount { triangles: 2, normals: 1 }));
    let smooth = build(&[[up, None, up], [None; 3]]).expect("builds");
    assert_eq!(smooth.vertex_normals(PrimitiveIndex::new(0)), Some(&[up, None, up]));
    assert_eq!(smooth.vertex_normals(PrimitiveIndex::new(1)), Some(&[None; 3]));
    assert_eq!(smooth.vertex_normals(PrimitiveIndex::new(2)), None);
    let flat = build(&[]).expect("builds");
    assert_eq!(flat.vertex_normals(PrimitiveIndex::new(0)), None);
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
            normals: &[],
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
        normals: &[],
        lighting: dark(),
        camera: reference_camera(),
    })
    .expect("builds");
    assert!(scene.tree.nodes.is_empty());
    let toward = TraceRay::new(at(0, 0, 4 * U), at(0, 0, -U)).expect("ray");
    let hit = scene.closest(&toward).expect("traces").expect("hits");
    let first = PrimitiveIndex::new(0);
    assert_eq!((hit.primitive, hit.hit.distance, hit.hit.front_face), (first, grid(4 * U), true));
    assert!(scene.occluded(&toward, FAR).expect("traces"));
    assert!(!scene.occluded(&toward, grid(4 * U - 1)).expect("traces"));
    let away = TraceRay::new(at(0, 0, 4 * U), at(0, 0, U)).expect("ray");
    assert_eq!(scene.closest(&away).expect("traces"), None);
    assert!(!scene.occluded(&away, FAR).expect("traces"));
}

/// Of two triangles hit at the same distance the lower primitive index wins, so the order of
/// `SceneSpec::triangles` is part of what a scene is: the same two coincident triangles, each
/// with its own material, listed in the other order give the hit the other triangle's albedo.
/// The oracle is the `Tracer::closest` contract and the materials as given.
#[test]
fn coincident_triangles_resolve_to_the_lowest_index_so_order_matters() {
    let red = Albedo::new([per_256(200), per_256(0), per_256(0)]).expect("valid");
    let blue = Albedo::new([per_256(0), per_256(0), per_256(200)]).expect("valid");
    let big = [at(-U, -U, 0), at(U, -U, 0), at(0, U, 0)];
    let small = [at(-U / 2, -U / 2, 0), at(U / 2, -U / 2, 0), at(0, U / 2, 0)];
    let toward = TraceRay::new(at(0, 0, 4 * U), at(0, 0, -U)).expect("ray");
    let first_hit = |triangles: &[[FixedVec3; 3]], material_of: &[u32]| {
        let scene = Scene::new(&SceneSpec {
            triangles,
            material_of,
            materials: &[red, blue],
            normals: &[],
            lighting: dark(),
            camera: reference_camera(),
        })
        .expect("builds");
        let hit = scene.closest(&toward).expect("traces").expect("hits");
        (hit.primitive, hit.hit.distance, scene.albedo(hit.primitive).copied())
    };
    let first = PrimitiveIndex::new(0);
    // Big is red, small is blue, in either order.
    assert_eq!(first_hit(&[big, small], &[0, 1]), (first, grid(4 * U), Some(red)));
    assert_eq!(first_hit(&[small, big], &[1, 0]), (first, grid(4 * U), Some(blue)));
}
