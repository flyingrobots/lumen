//! Size class: small (in process, one thread, no file, no network).
//!
//! A scene's statistics (`Scene::statistics`, ADR 0008) against something other than the code
//! that counts them: the scene's own inputs, a tree rebuilt here with Bunny's builder and walked
//! recursively over Bunny's uncompacted nodes, Bunny's documented bounds on that tree, and
//! scenes built to hold known counts.

use bunny_broadphase::bvh::{SAH_MAX_DEPTH, SAH_MAX_LEAF_SIZE};
use bunny_broadphase::{build_bvh_sah, BvhNode};
use bunny_geom::FixedAabb3;
use bunny_linalg::{FixedUnitVec3, FixedVec3};
use bunny_num::FixedQ32_32;
use bunny_query::TraceTriangle;
use lumen::reference::{reference_material_of, REFERENCE_TRIANGLE_COUNT};
use lumen::{BuiltinScene, Scene, SceneSpec, TreeStatistics, LEAF_SIZE_BUCKETS};

const ONE: i64 = 1 << 32;

fn at(x: i64, y: i64, z: i64) -> FixedVec3 {
    let unit = |n: i64| FixedQ32_32::from_raw(n * ONE);
    FixedVec3::new(unit(x), unit(y), unit(z))
}

/// A tree's shape as this test counts it, every field from zero: the oracle's own record, so no
/// field of the value under test can stand in for its expected value.
#[derive(Debug)]
struct Shape {
    nodes: usize,
    interior_nodes: usize,
    leaves: usize,
    max_leaf_depth: u32,
    leaf_depth_total: u64,
    leaf_triangles: u64,
    min_leaf_triangles: u32,
    max_leaf_triangles: u32,
    leaf_sizes: [usize; LEAF_SIZE_BUCKETS],
}

impl Shape {
    const EMPTY: Self = Self {
        nodes: 0,
        interior_nodes: 0,
        leaves: 0,
        max_leaf_depth: 0,
        leaf_depth_total: 0,
        leaf_triangles: 0,
        min_leaf_triangles: u32::MAX,
        max_leaf_triangles: 0,
        leaf_sizes: [0; LEAF_SIZE_BUCKETS],
    };

    /// Every field of `tree` against this shape's.
    fn assert_matches(&self, tree: &TreeStatistics, name: &str) {
        assert_eq!(tree.nodes, self.nodes, "{name}: nodes");
        assert_eq!(tree.interior_nodes, self.interior_nodes, "{name}: interior nodes");
        assert_eq!(tree.leaves, self.leaves, "{name}: leaves");
        assert_eq!(tree.max_leaf_depth, self.max_leaf_depth, "{name}: deepest leaf");
        assert_eq!(tree.leaf_depth_total, self.leaf_depth_total, "{name}: leaf depths");
        assert_eq!(tree.leaf_triangles, self.leaf_triangles, "{name}: leaf triangles");
        assert_eq!(tree.min_leaf_triangles, self.min_leaf_triangles, "{name}: smallest leaf");
        assert_eq!(tree.max_leaf_triangles, self.max_leaf_triangles, "{name}: largest leaf");
        assert_eq!(tree.leaf_sizes, self.leaf_sizes, "{name}: leaf sizes");
    }
}

/// The shape of a Bunny `BvhNode` tree, counted by recursion from node `index` at `depth`: an
/// independent walk of the representation the compact layout was converted from.
fn walk(nodes: &[BvhNode], index: usize, depth: u32, shape: &mut Shape) {
    let node = nodes[index];
    shape.nodes += 1;
    if node.prim_count == 0 {
        shape.interior_nodes += 1;
        let left = node.first_child_or_prim_idx as usize;
        walk(nodes, left, depth + 1, shape);
        walk(nodes, left + 1, depth + 1, shape);
        return;
    }
    let triangles = node.prim_count;
    shape.leaves += 1;
    shape.max_leaf_depth = shape.max_leaf_depth.max(depth);
    shape.leaf_depth_total += u64::from(depth);
    shape.leaf_triangles += u64::from(triangles);
    shape.min_leaf_triangles = shape.min_leaf_triangles.min(triangles);
    shape.max_leaf_triangles = shape.max_leaf_triangles.max(triangles);
    shape.leaf_sizes[(triangles as usize - 1).min(LEAF_SIZE_BUCKETS - 1)] += 1;
}

/// Bunny's builder over the scene's triangles' boxes, as `Scene::new` calls it: its nodes and
/// their count. The builder is a deterministic function of the boxes, so this is the scene's
/// tree before compaction.
fn rebuilt(scene: &Scene) -> Vec<BvhNode> {
    let boxes: Vec<FixedAabb3> = scene.triangles().iter().map(TraceTriangle::bounds).collect();
    let empty = FixedAabb3::new(at(0, 0, 0), at(0, 0, 0));
    let blank = BvhNode { bounds: empty, first_child_or_prim_idx: 0, prim_count: 0 };
    let mut nodes = vec![blank; 2 * boxes.len()];
    let mut indices = vec![0_u32; boxes.len()];
    let used = build_bvh_sah(&mut nodes, &mut indices, &boxes).expect("builds");
    nodes.truncate(used);
    nodes
}

/// Oracle: an independent recursive walk of the tree Bunny's builder makes from the same boxes,
/// and Bunny's documented bounds: at most `2n - 1` nodes, one more leaf than interior nodes,
/// every triangle in exactly one leaf, no leaf deeper than `SAH_MAX_DEPTH`, and a leaf above
/// `SAH_MAX_LEAF_SIZE` only at that depth. The bounds are the triangles' extent, folded here.
#[test]
fn the_tree_statistics_of_every_built_in_scene_match_an_independent_walk() {
    for name in BuiltinScene::ALL {
        let scene = name.build().expect("builds");
        let stats = scene.statistics().expect("counts");
        let nodes = rebuilt(&scene);
        let mut want = Shape::EMPTY;
        walk(&nodes, 0, 0, &mut want);
        let tree = stats.tree;
        want.assert_matches(&tree, &format!("{name:?}"));
        let n = scene.triangles().len();
        assert_eq!(tree.nodes, nodes.len(), "{name:?}");
        assert!(tree.nodes < 2 * n, "{name:?}: {} nodes for {n} triangles", tree.nodes);
        assert_eq!(tree.leaves, tree.interior_nodes + 1, "{name:?}");
        assert_eq!(tree.leaf_triangles, n as u64, "{name:?}");
        assert_eq!(tree.leaf_sizes.iter().sum::<usize>(), tree.leaves, "{name:?}");
        assert!(tree.max_leaf_depth as usize <= SAH_MAX_DEPTH, "{name:?}");
        if tree.max_leaf_triangles as usize > SAH_MAX_LEAF_SIZE {
            assert_eq!(tree.max_leaf_depth as usize, SAH_MAX_DEPTH, "{name:?}");
        }
        assert_eq!(stats.bounds, extent(&scene), "{name:?}");
        // Witness: both trees are deep enough for depth to be counted, not assumed.
        assert!(tree.max_leaf_depth > 10 && tree.interior_nodes > 1000, "{name:?}: {tree:?}");
    }
}

/// The smallest box around every vertex of the scene, by a fold over the raw coordinates.
fn extent(scene: &Scene) -> FixedAabb3 {
    let raw = |v: FixedVec3| [v.x.raw(), v.y.raw(), v.z.raw()];
    let (mut min, mut max) = ([i64::MAX; 3], [i64::MIN; 3]);
    for vertex in scene.triangles().iter().flat_map(TraceTriangle::vertices).map(raw) {
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex[axis]);
            max[axis] = max[axis].max(vertex[axis]);
        }
    }
    let point = |c: [i64; 3]| c.map(FixedQ32_32::from_raw);
    let ([x0, y0, z0], [x1, y1, z1]) = (point(min), point(max));
    FixedAabb3::new(FixedVec3::new(x0, y0, z0), FixedVec3::new(x1, y1, z1))
}

/// Oracle: the reference scene's own material table, counted here from
/// `reference_material_of`. The Fixed Point scene's counts sum to its triangles.
#[test]
fn triangles_by_material_count_the_scenes_material_indices() {
    let reference = lumen::reference_scene().expect("builds").statistics().expect("counts");
    let mut want = vec![0_usize; 5];
    for material in reference_material_of() {
        want[material as usize] += 1;
    }
    assert_eq!(reference.triangles, REFERENCE_TRIANGLE_COUNT);
    assert_eq!(reference.triangles_by_material, want);
    // Witness: every material is used, so a count landing in the wrong entry is seen.
    assert!(want.iter().all(|&n| n > 0), "{want:?}");
    let fixed_point = BuiltinScene::FixedPoint.build().expect("builds");
    let stats = fixed_point.statistics().expect("counts");
    assert_eq!(stats.triangles_by_material.len(), fixed_point.materials().len());
    assert_eq!(stats.triangles_by_material.iter().sum::<usize>(), stats.triangles);
    assert_eq!(stats.triangles, fixed_point.triangles().len());
}

/// Three triangles: one with a vertex normal, one of zero area (its vertices on a line), one
/// plain; three materials, the last unused.
fn three_triangles() -> Scene {
    let reference = lumen::reference_scene().expect("builds");
    let triangles = [
        [at(-3, -1, -4), at(-1, -1, -4), at(-2, 1, -4)],
        [at(0, 0, -4), at(1, 0, -4), at(2, 0, -4)],
        [at(1, -1, -4), at(3, -1, -4), at(2, 1, -4)],
    ];
    Scene::new(&SceneSpec {
        triangles: &triangles,
        material_of: &[1, 0, 1],
        materials: &reference.materials()[..3],
        normals: &[[Some(FixedUnitVec3::UNIT_Z), None, None], [None; 3], [None; 3]],
        lighting: reference.lighting(),
        camera: reference.camera(),
    })
    .expect("builds")
}

/// Oracle: a scene built to hold known counts ([`three_triangles`]).
#[test]
fn smooth_zero_area_and_unused_materials_are_counted_as_built() {
    let stats = three_triangles().statistics().expect("counts");
    assert_eq!(stats.triangles, 3);
    assert_eq!(stats.smooth_triangles, 1);
    assert_eq!(stats.zero_area_triangles, 1);
    assert_eq!(stats.triangles_by_material, [1, 2, 0]);
    assert_eq!(stats.tree.leaf_triangles, 3);
}

/// Bytes per element on a 64-bit target: Bunny documents a compact node as 112 bytes, ADR 0007
/// measured a `TraceTriangle` at 72, an `Option<TriangleSurface>` at 224 and a triangle's three
/// optional vertex normals at 96; a material index and a primitive index are a `u32`, 4 bytes,
/// and an albedo three Q32.32 values, 24.
#[cfg(target_pointer_width = "64")]
const BYTES: [usize; 7] = [72, 224, 96, 4, 24, 112, 4];

/// Oracle: [`BYTES`] times the counts, each buffer written out, and the total their sum, all
/// seven terms, vertex normals included: on the reference scene, which has no vertex normals
/// (`n` triangles, 5 materials), on Fixed Point, whose every triangle has an entry of three
/// normals (Fixed Point keeps one per triangle, flat ones included), and on a three-triangle
/// scene with one normal, where the total is worked by hand.
#[cfg(target_pointer_width = "64")]
#[test]
fn memory_is_each_buffers_length_times_its_element_size() {
    let [triangle, surface, normals, index, albedo, node, primitive] = BYTES;
    let check = |scene: &Scene, with_normals: usize, materials: usize| {
        let stats = scene.statistics().expect("counts");
        let (n, memory) = (stats.triangles, stats.memory);
        let want = [
            n * triangle,
            n * surface,
            with_normals * normals,
            n * index,
            materials * albedo,
            stats.tree.interior_nodes * node,
            n * primitive,
        ];
        let got = [
            memory.triangles,
            memory.surfaces,
            memory.vertex_normals,
            memory.material_indices,
            memory.materials,
            memory.tree_nodes,
            memory.tree_primitive_indices,
        ];
        assert_eq!(got, want);
        assert_eq!(memory.total(), want.iter().sum::<usize>());
        memory.total()
    };
    let reference = lumen::reference_scene().expect("builds");
    check(&reference, 0, 5);
    let fixed_point = BuiltinScene::FixedPoint.build().expect("builds");
    let n = fixed_point.triangles().len();
    check(&fixed_point, n, fixed_point.materials().len());
    assert!(fixed_point.smooth_triangles() > 0, "Fixed Point has vertex normals");
    // Three triangles, one interior node or none: 3 * (72 + 224 + 96 + 4 + 4) + 3 * 24 = 1 272,
    // plus 112 per interior node.
    let small = three_triangles();
    let interior = small.statistics().expect("counts").tree.interior_nodes;
    assert_eq!(check(&small, 3, 3), 1_272 + 112 * interior);
}

/// Oracle: the statistics are read from the scene, not stored by a render. On
/// [`three_triangles`], each value worked by hand (the box around every vertex is `(-3, -1, -4)`
/// to `(3, 1, -4)`) or counted here from the scene's own buffers, and the tree walked here: the
/// first call, a call after a render of the scene, and a call on a second build all give them.
#[test]
fn statistics_are_a_function_of_the_scene() {
    let first = three_triangles();
    let second = three_triangles();
    let before = first.statistics().expect("counts");
    let size = lumen::ImageSize::new(4, 4).expect("size");
    let settings = lumen::RenderSettings::new(1, 0).expect("settings");
    lumen::render(&first, size, settings).expect("renders");
    let mut shape = Shape::EMPTY;
    walk(&rebuilt(&first), 0, 0, &mut shape);
    let primitives = (0..3).map(lumen::PrimitiveIndex::new);
    let zero_area = primitives.filter(|&p| first.surface(p).is_none()).count();
    assert_eq!(zero_area, 1, "the second triangle has no surface");
    let after = first.statistics().expect("counts");
    for (stats, when) in [
        (before, "first"),
        (after, "after a render"),
        (second.statistics().expect("counts"), "second build"),
    ] {
        assert_eq!(stats.triangles, 3, "{when}");
        assert_eq!(stats.triangles_by_material, [1, 2, 0], "{when}");
        assert_eq!((stats.smooth_triangles, stats.zero_area_triangles), (1, zero_area), "{when}");
        assert_eq!(stats.bounds, FixedAabb3::new(at(-3, -1, -4), at(3, 1, -4)), "{when}");
        shape.assert_matches(&stats.tree, when);
    }
}
