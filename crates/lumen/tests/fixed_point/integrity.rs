//! The scene's integrity: identities, counts, the trace domain, no degenerate triangle, and
//! everything resting on the felt.

use bunny_num::fixed_q32_32::ONE_RAW;
use lumen::scenes::fixed_point::player::{placed_bunny, NOSE_XZ};
use lumen::scenes::fixed_point::table::{FELT, FELT_Y};
use lumen::scenes::fixed_point::{
    fixed_point_lighting, fixed_point_materials, Die, Material, PartKind, Side,
};
use lumen::{fixed_point_scene, FIXED_POINT_SCENE_ID};

use crate::support::{content, part, steps};

/// Oracle: counts derived by hand from the construction: two triangles per rectangle, 24-gon
/// discs, 12-sided pips, and the bunny's 3 851 faces with none dropped.
#[test]
fn every_part_has_its_derived_triangle_count() {
    let content = content();
    let count = |kind| content.part(kind).len();
    let expected = [
        (PartKind::Felt, 2),
        // Four sides of seven strips.
        (PartKind::Rail, 4 * 7 * 2),
        // Two row lines, seven dividers, two long lines.
        (PartKind::LayoutLines, (2 + 7 + 2) * 2),
        // Segments: 4 has 4, 5 has 5, 6 has 6, 8 has 7, 9 has 6, 10 has 2 + 6.
        (PartKind::Numeral(4), 4 * 2),
        (PartKind::Numeral(5), 5 * 2),
        (PartKind::Numeral(6), 6 * 2),
        (PartKind::Numeral(8), 7 * 2),
        (PartKind::Numeral(9), 6 * 2),
        (PartKind::Numeral(10), 8 * 2),
        (PartKind::PuckFace, 24),
        // The ring on top and the side: 24 quads each.
        (PartKind::PuckRim, 2 * 24 * 2),
        // O: four rectangles; N: two and a diagonal.
        (PartKind::PuckLetters, 7 * 2),
        // 6 faces, 12 chamfers, 8 corners; 24 pips of 12 top and 24 wall triangles.
        (PartKind::DieBody(Die::Left), 6 * 2 + 12 * 2 + 8),
        (PartKind::Pips(Die::Left), 24 * 36),
        (PartKind::DieBody(Die::Right), 44),
        (PartKind::Pips(Die::Right), 864),
        (PartKind::Bunny, 3851),
        (PartKind::Lens(Side::Left), 8),
        (PartKind::Lens(Side::Right), 8),
        (PartKind::Bridge, 2),
        (PartKind::Arm(Side::Left), 2),
        (PartKind::Arm(Side::Right), 2),
    ];
    for (kind, n) in expected {
        assert_eq!(count(kind), n, "{kind:?}");
    }
    assert_eq!(content.parts.len(), expected.len());
    assert_eq!(content.triangles.len(), 5975);
    assert_eq!(placed_bunny().expect("places").dropped, Vec::<usize>::new());
    // The parts cover every primitive once, in order, each with its part's material.
    let mut next = 0;
    for part in &content.parts {
        assert_eq!(part.primitives.start, next, "{:?}", part.kind);
        next = part.primitives.end;
        let material = part.kind.material().index();
        assert!(content.material_of[part.primitives.clone()].iter().all(|&m| m == material));
    }
    assert_eq!(next, content.triangles.len());
    assert_eq!(content.material_of.len(), next);
}

/// Oracle: Bunny's trace domain (`|raw| < 2^40`, 256 units) and its surface rule: a triangle
/// has a `TriangleSurface` exactly when its exact normal is nonzero.
#[test]
fn every_triangle_is_in_the_trace_domain_and_none_is_degenerate() {
    let scene = fixed_point_scene().expect("the scene builds");
    assert_eq!(FIXED_POINT_SCENE_ID, "lumen-fixed-point-scene/1");
    let mut coordinates = 0_u64;
    for (primitive, triangle) in scene.triangles().iter().enumerate() {
        for vertex in triangle.vertices() {
            for c in [vertex.x, vertex.y, vertex.z] {
                assert!(c.raw().abs() < 256 * ONE_RAW, "primitive {primitive}: {c:?}");
                coordinates += 1;
            }
        }
        let index = u32::try_from(primitive).expect("fits");
        assert!(scene.surface(index).is_some(), "primitive {primitive} is degenerate");
    }
    assert_eq!(coordinates, 5975 * 9);
    assert_eq!(fixed_point_materials().expect("valid").len(), 8);
    assert_eq!(Material::Shades.index(), 7);
    let lighting = fixed_point_lighting().expect("valid");
    assert_eq!(scene.lighting(), lighting);
    let sun = lighting.sun.expect("a sun");
    assert_eq!(sun.toward_light().y.raw(), ONE_RAW, "the sun's largest component is 1.0");
}

/// Oracle: resting means the lowest point is exactly on the supporting surface, to the grid
/// step (a tolerance of zero steps): no gap under it and nothing below it, over the felt.
#[test]
fn the_dice_the_puck_and_the_bunny_rest_on_the_felt() {
    let content = content();
    let resting =
        [PartKind::Pips(Die::Left), PartKind::Pips(Die::Right), PartKind::PuckRim, PartKind::Bunny];
    for kind in resting {
        let vertices: Vec<_> = part(&content, kind).into_iter().flatten().collect();
        let lowest = vertices.iter().map(|p| p[1]).min();
        assert_eq!(lowest, Some(FELT_Y), "{kind:?}");
        for p in vertices.iter().filter(|p| p[1] == FELT_Y) {
            assert!(FELT.contains(p[0], p[2]), "{kind:?}: {p:?} is off the felt");
        }
    }
    // The die bodies stand on their bottom pips: above the felt by exactly the pips' rise.
    for die in [Die::Left, Die::Right] {
        let body = part(&content, PartKind::DieBody(die));
        let lowest = body.iter().flatten().map(|p| p[1]).min();
        assert_eq!(lowest, Some(FELT_Y + lumen::scenes::fixed_point::dice::PIP_RISE), "{die:?}");
    }
    // The nose, measured on the mesh, is a vertex of the placed bunny where it was put.
    let bunny = placed_bunny().expect("places");
    let nose =
        bunny.triangles.iter().flatten().map(|&v| steps(v)).find(|p| (p[0], p[2]) == NOSE_XZ);
    assert!(nose.is_some(), "no bunny vertex at the nose position");
}
