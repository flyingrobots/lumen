//! The `depth`, `primitive-id` and `material-id` views, on a constructed scene.

use bunny_linalg::FixedVec3;
use bunny_optics::PixelStream;
use lumen::reference::{reference_lighting, reference_materials};
use lumen::{render_view, Scene, SceneSpec, View};

use super::{at, camera_on_axis, one_sample, rgba, schedule, size};

/// The ID views' seeds, as ADR 0010 states them: the ASCII words "primitiv" and "material".
const PRIMITIVE_SEED: u64 = 0x7072_696D_6974_6976;
const MATERIAL_SEED: u64 = 0x6D61_7465_7269_616C;

/// Two triangles: triangle 0 in z = -4 on the axis (material `near`), and triangle 1 far off to
/// the side in z = -12 (material 2), which widens the scene's box to x = 24. The camera is at
/// z = 4 looking along -z.
fn two_triangles(near: u32) -> Scene {
    let materials = reference_materials().expect("materials");
    let triangles: [[FixedVec3; 3]; 2] = [
        [at(-4, -1, -4), at(4, -1, -4), at(0, 3, -4)],
        [at(20, -1, -12), at(24, -1, -12), at(22, 3, -12)],
    ];
    Scene::new(&SceneSpec {
        triangles: &triangles,
        material_of: &[near, 2],
        materials: &materials,
        normals: &[],
        lighting: reference_lighting().expect("lighting"),
        camera: camera_on_axis(4, -1),
    })
    .expect("builds")
}

fn centre(scene: &Scene, view: View) -> [u8; 4] {
    let image =
        render_view(scene, view, size(9, 9), one_sample(), schedule(1, 1)).expect("renders").image;
    rgba(&image, 4, 4)
}

/// The ID colour of ADR 0010, written out here: 64 bits of Bunny's `PixelStream::new(seed,
/// index, 0).bits(0, 0)`, each of the three low bytes mapped to `32 + byte * 224 / 256`.
fn id_colour(seed: u64, index: u32) -> [u8; 4] {
    let bits = PixelStream::new(seed, index, 0).bits(0, 0);
    let c = |shift: u32| u8::try_from(32 + ((bits >> shift) & 0xFF) * 224 / 256).expect("byte");
    [c(0), c(8), c(16), 255]
}

/// Oracle: ADR 0010's depth rule by hand. The centre ray goes straight down -z and hits
/// triangle 0 at distance 8. The range is the largest axis difference between the eye (0, 0, 4)
/// and the box's corners: x reaches 24 (z only 16), so the range is 24, and the grey is
/// `255 - floor(239 * 8 / 24) = 255 - 79 = 176`. A miss is black.
#[test]
fn depth_is_grey_by_distance_over_the_scenes_range() {
    let scene = two_triangles(0);
    assert_eq!(centre(&scene, View::Depth), [176, 176, 176, 255]);
    let image = render_view(&scene, View::Depth, size(9, 9), one_sample(), schedule(2, 1))
        .expect("renders")
        .image;
    assert_eq!(rgba(&image, 0, 0), [0, 0, 0, 255], "a miss");
    // Triangle 0 and a small one hidden behind it at z = -12: x reaches 4, y 3 and z 16 from
    // the eye, so the range is 16, from the z axis, and the hit at 8 is half way: `255 - 119`.
    let deep = deep_scene();
    assert_eq!(centre(&deep, View::Depth), [136, 136, 136, 255]);
}

/// Triangle 0 of [`two_triangles`], and a small triangle hidden behind it in z = -12.
fn deep_scene() -> Scene {
    let materials = reference_materials().expect("materials");
    Scene::new(&SceneSpec {
        triangles: &[
            [at(-4, -1, -4), at(4, -1, -4), at(0, 3, -4)],
            [at(-1, -1, -12), at(1, -1, -12), at(0, 1, -12)],
        ],
        material_of: &[0, 0],
        materials: &materials,
        normals: &[],
        lighting: reference_lighting().expect("lighting"),
        camera: camera_on_axis(4, -1),
    })
    .expect("builds")
}

/// Oracle: the ID rule written out with Bunny's public hash. The centre pixel shows triangle 0:
/// its primitive colour, and its material's colour, which follows the material, not the
/// triangle.
#[test]
fn id_views_colour_by_primitive_and_by_material() {
    let scene = two_triangles(1);
    assert_eq!(centre(&scene, View::PrimitiveId), id_colour(PRIMITIVE_SEED, 0));
    assert_eq!(centre(&scene, View::MaterialId), id_colour(MATERIAL_SEED, 1));
    let other = two_triangles(3);
    assert_eq!(centre(&other, View::MaterialId), id_colour(MATERIAL_SEED, 3));
    assert_eq!(centre(&other, View::PrimitiveId), id_colour(PRIMITIVE_SEED, 0), "same triangle");
    assert_ne!(id_colour(MATERIAL_SEED, 1), id_colour(MATERIAL_SEED, 3));
}

/// The colour of pixel `(x, y)` of a 128 by 128 `view` of `scene`.
fn at_128(scene: &Scene, view: View, x: u32, y: u32) -> [u8; 4] {
    let image = render_view(scene, view, size(128, 128), one_sample(), schedule(4, 3))
        .expect("renders")
        .image;
    rgba(&image, x, y)
}

/// Oracle: the edge rule by hand on the camera's geometry. At 128 by 128 with a 90-degree view
/// from 8 units, a pixel centre in row `r` meets the plane z = -4 at `y = 8 - (r + 0.5) / 8`.
/// Row 64 meets it at y = -0.0625, where v2's weight is `(y + 1) / 4 = 0.23`: fill. Row 71 meets
/// it at y = -0.9375, a weight of `1 / 64`, below `1 / 32`: an edge. Row 73 is below the
/// triangle: a miss.
#[test]
fn wireframe_draws_edges_where_a_weight_is_below_a_thirty_second() {
    let scene = two_triangles(0);
    assert_eq!(at_128(&scene, View::Wireframe, 64, 64), [64, 64, 64, 255], "fill");
    assert_eq!(at_128(&scene, View::Wireframe, 64, 71), [255, 255, 255, 255], "edge");
    assert_eq!(at_128(&scene, View::Wireframe, 64, 73), [0, 0, 0, 255], "miss");
}

/// Oracle: Bunny's search by hand (`tests/traversal_counts.rs` derives the same counts). With
/// triangle 0 alone and the small one behind it, the tree is a root and two leaves; the centre
/// ray tests the root's box and both children's, visits the near leaf (one triangle test), hits
/// it at 8, and skips the far leaf: 3 + 1 = 4, `(0, 0, 8)` on the ramp. A ray that misses the
/// scene's box makes one box test: `(0, 0, 2)`.
#[test]
fn bvh_cost_shows_the_camera_rays_box_and_triangle_tests() {
    let scene = deep_scene();
    let shape = scene.statistics().expect("statistics").tree;
    assert_eq!((shape.interior_nodes, shape.leaves), (1, 2), "a root and two leaves");
    assert_eq!(at_128(&scene, View::BvhCost, 64, 64), [0, 0, 8, 255], "4 tests");
    assert_eq!(at_128(&scene, View::BvhCost, 0, 0), [0, 0, 2, 255], "1 box test");
}
