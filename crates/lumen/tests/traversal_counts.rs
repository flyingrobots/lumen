//! Size class: small (in process, one thread, no file, no network).
//!
//! The scene's counted queries (`CountingTracer`, ADR 0008) against something other than the code
//! that counts: traversal counts derived by hand from Bunny's documented search on scenes built to
//! a known tree, and Bunny's counted searches called directly on a tree rebuilt here from the
//! scene's triangles. Both kinds also check that the counted answer is the uncounted one.

use bunny_broadphase::{
    build_bvh_sah, compact_any_hit_counted, compact_bvh, compact_closest_hit_counted, BvhNode,
    CompactChild, CompactNode, CompactView, TraversalCounts,
};
use bunny_geom::FixedAabb3;
use bunny_linalg::FixedVec3;
use bunny_num::FixedQ32_32;
use bunny_query::{TraceRay, TraceTriangle};
use lumen::reference::{reference_camera, reference_lighting, reference_materials};
use lumen::{BuiltinScene, CountingTracer, RenderSettings, Scene, SceneSpec, Tracer, FAR};

const ONE: i64 = 1 << 32;

fn at(x: i64, y: i64, z: i64) -> FixedVec3 {
    let unit = |n: i64| FixedQ32_32::from_raw(n * ONE);
    FixedVec3::new(unit(x), unit(y), unit(z))
}

/// A point in halves of a unit.
fn half(x: i64, y: i64, z: i64) -> FixedVec3 {
    let unit = |n: i64| FixedQ32_32::from_raw(n * ONE / 2);
    FixedVec3::new(unit(x), unit(y), unit(z))
}

/// A scene of `triangles`, all of the reference scene's first material.
fn scene_of(triangles: &[[FixedVec3; 3]]) -> Scene {
    let materials = reference_materials().expect("materials");
    let material_of = vec![0_u32; triangles.len()];
    Scene::new(&SceneSpec {
        triangles,
        material_of: &material_of,
        materials: &materials,
        normals: &[],
        lighting: reference_lighting().expect("lighting"),
        camera: reference_camera(),
    })
    .expect("builds")
}

/// The counts as a tuple, in the order of `TraversalCounts`' fields: box tests, boxes entered,
/// popped entries skipped, interior visits, leaf visits, primitive tests, deepest stack.
fn fields(c: TraversalCounts) -> [u64; 7] {
    [
        c.box_tests,
        c.box_entries,
        c.popped_skipped,
        c.interior_visits,
        c.leaf_visits,
        c.primitive_tests,
        c.peak_stack,
    ]
}

fn ray(origin: FixedVec3, direction: FixedVec3) -> TraceRay {
    TraceRay::new(origin, direction).expect("ray")
}

/// The triangle facing +z at depth `z`: x from -4 to 4 along y = -1, apex at (0, 3).
fn facing(z: i64) -> [FixedVec3; 3] {
    [at(-4, -1, z), at(4, -1, z), at(0, 3, z)]
}

/// Oracle: Bunny's search, by hand (its README, "Counting traversals"). A tree of one triangle is
/// a root whose child is a leaf: one box test (the root's), then, if the ray enters the box, one
/// push, one leaf visit and one primitive test; no interior node, nothing skipped. A ray that hits
/// the triangle, one that passes through its box beside it, and one that misses the box, each for
/// both searches; the answers are the uncounted queries'.
#[test]
fn one_triangle_counts_one_box_test_and_at_most_one_primitive_test() {
    let scene = scene_of(&[facing(-4)]);
    let shape = scene.statistics().expect("statistics").tree;
    assert_eq!((shape.nodes, shape.leaves), (1, 1), "the tree is one leaf");
    let toward = at(0, 0, -1);
    let cases = [
        // Through (0, 0, -4), inside the triangle.
        ("hit", ray(at(0, 0, 4), toward), true, [1, 1, 0, 0, 1, 1, 1]),
        // Through (3.5, 2.5, -4): inside the box (x -4..4, y -1..3), outside the triangle,
        // whose half-width at y = 2.5 is 0.5.
        ("beside", ray(half(7, 5, 8), toward), false, [1, 1, 0, 0, 1, 1, 1]),
        // Away from the triangle: the root's box is not entered.
        ("away", ray(at(0, 0, 4), at(0, 0, 1)), false, [1, 0, 0, 0, 0, 0, 0]),
    ];
    for (name, ray, hits, want) in cases {
        let (found, counts) = scene.closest_counted(&ray).expect(name);
        assert_eq!(found, scene.closest(&ray).expect(name), "{name}: the closest answer");
        assert_eq!(found.is_some(), hits, "{name}: hit");
        assert_eq!(fields(counts), want, "{name}: closest-hit counts");
        let (blocked, counts) = scene.occluded_counted(&ray, FAR).expect(name);
        assert_eq!(blocked, scene.occluded(&ray, FAR).expect(name), "{name}: the any answer");
        assert_eq!(blocked, hits, "{name}: occluded");
        assert_eq!(fields(counts), want, "{name}: any-hit counts");
    }
}

/// Oracle: Bunny's search, by hand, on a tree whose shape is checked first. One triangle at depth
/// -4 and four copies of it at depth -8, all with the same x and y, so their centroids differ only
/// in z and the builder must split five triangles (more than a leaf's four) at the gap: a root
/// with two leaves, of 1 and 4 triangles. A ray down the axis from z = 4 tests the root's box,
/// visits the root (both children's boxes tested and entered, two entries on the stack), visits
/// the near leaf and hits its triangle at 8, which lowers the limit to 8; the far leaf's entry
/// (12) is then above the limit and is popped and skipped, so its four triangles are never
/// tested. The any-hit search stops at the near hit, leaving the far entry unpopped.
#[test]
fn a_nearer_hit_prunes_the_farther_subtree() {
    let near = facing(-4);
    let far = facing(-8);
    let scene = scene_of(&[far, near, far, far, far]);
    let shape = scene.statistics().expect("statistics").tree;
    assert_eq!((shape.nodes, shape.interior_nodes, shape.leaves), (3, 1, 2), "root and 2 leaves");
    assert_eq!(shape.leaf_sizes, [1, 0, 0, 1, 0], "leaves of one and four triangles");
    let down = ray(at(0, 0, 4), at(0, 0, -1));
    let (found, counts) = scene.closest_counted(&down).expect("traces");
    assert_eq!(found, scene.closest(&down).expect("traces"));
    let hit = found.expect("hits the near triangle");
    assert_eq!(hit.primitive.get(), 1, "the near triangle");
    assert_eq!(hit.hit.distance, FixedQ32_32::from_raw(8 * ONE));
    assert_eq!(fields(counts), [3, 3, 1, 1, 1, 1, 2], "closest-hit counts");
    let (blocked, counts) = scene.occluded_counted(&down, FAR).expect("traces");
    assert!(blocked);
    assert_eq!(fields(counts), [3, 3, 0, 1, 1, 1, 2], "any-hit counts");
}

/// Bunny's builder and compaction over the scene's triangles' boxes, as `Scene::new` calls them:
/// the compact nodes, the root and the primitive indices, built here from the scene's public
/// triangles.
struct Rebuilt {
    nodes: Vec<CompactNode>,
    root: Option<bunny_broadphase::CompactRoot>,
    indices: Vec<u32>,
}

impl Rebuilt {
    fn of(scene: &Scene) -> Self {
        let boxes: Vec<FixedAabb3> = scene.triangles().iter().map(TraceTriangle::bounds).collect();
        let empty = FixedAabb3::new(at(0, 0, 0), at(0, 0, 0));
        let blank = BvhNode { bounds: empty, first_child_or_prim_idx: 0, prim_count: 0 };
        let mut tree = vec![blank; 2 * boxes.len()];
        let mut indices = vec![0_u32; boxes.len()];
        let used = build_bvh_sah(&mut tree, &mut indices, &boxes).expect("builds");
        tree.truncate(used);
        let compact_blank = CompactNode {
            bounds: [empty, empty],
            children: [CompactChild::default(), CompactChild::default()],
        };
        let mut nodes = vec![compact_blank; used / 2];
        let (root, written) = compact_bvh(&tree, &mut nodes).expect("compacts").expect("root");
        nodes.truncate(written);
        Self { nodes, root: Some(root), indices }
    }

    fn view(&self) -> CompactView<'_> {
        CompactView { root: self.root, nodes: &self.nodes, prim_indices: &self.indices }
    }
}

/// Oracle: Bunny's counted searches called directly, on a tree rebuilt here, with the primitive
/// tests Bunny's README states for a triangle scene. For every pixel centre's camera ray of a 32
/// by 18 image of each built-in scene, the scene's counted queries give the same counts as the
/// direct calls, and the same answers as the uncounted queries. Witness: some rays hit, some
/// miss, and some searches skip popped entries, so every kind of count is compared nonzero.
#[test]
fn the_scenes_counts_are_bunnys_counted_searches_on_its_tree() {
    for builtin in BuiltinScene::ALL {
        let scene = builtin.build().expect("builds");
        let rebuilt = Rebuilt::of(&scene);
        let camera = scene.camera().camera(32, 18).expect("camera");
        let settings = RenderSettings::new(1, 0).expect("settings");
        let mut seen = TraversalCounts::default();
        let mut hits = 0;
        for (x, y) in (0..18).flat_map(|y| (0..32).map(move |x| (x, y))) {
            let ray = camera.ray(settings.sampler(x, y).camera_sample(0)).expect("ray");
            let test = |primitive: u32, limit| {
                let triangle = &scene.triangles()[primitive as usize];
                ray.hit_triangle(triangle, FixedQ32_32::ZERO, limit).map(|hit| hit.distance)
            };
            let (direct, want) =
                compact_closest_hit_counted(rebuilt.view(), &ray, FAR, test).expect("searches");
            let (found, counts) = scene.closest_counted(&ray).expect("traces");
            let at = format!("{} pixel ({x}, {y})", builtin.name());
            assert_eq!(counts, want, "{at}: closest-hit counts");
            assert_eq!(found, scene.closest(&ray).expect("traces"), "{at}: the answer");
            assert_eq!(found.map(|h| h.primitive.get()), direct.map(|d| d.primitive), "{at}");
            hits += usize::from(found.is_some());
            seen += counts;
            let any = |primitive: u32| {
                let triangle = &scene.triangles()[primitive as usize];
                ray.hit_triangle(triangle, FixedQ32_32::ZERO, FAR).is_some()
            };
            let (direct, want) =
                compact_any_hit_counted(rebuilt.view(), &ray, FAR, any).expect("searches");
            let (blocked, counts) = scene.occluded_counted(&ray, FAR).expect("traces");
            assert_eq!((blocked, counts), (direct, want), "{at}: any-hit");
            assert_eq!(blocked, scene.occluded(&ray, FAR).expect("traces"), "{at}");
        }
        let name = builtin.name();
        assert!(hits > 0 && hits < 32 * 18, "{name}: hits and misses ({hits})");
        assert!(fields(seen).iter().all(|&n| n > 0), "{name}: every count seen: {seen:?}");
    }
}
