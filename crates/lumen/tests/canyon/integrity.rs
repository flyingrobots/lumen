//! The assembled scene's integrity: its identity, its parts in order, the trace domain, no
//! degenerate triangle, only the bunny smooth, and the bunny resting on its ledge.

use bunny_num::fixed_q32_32::ONE_RAW;
use lumen::scenes::canyon::bunny::ledge_bunny;
use lumen::scenes::canyon::terrain::LEDGE_Y;
use lumen::scenes::canyon::{canyon_lighting, canyon_materials, PartKind};
use lumen::{PrimitiveIndex, CANYON_SCENE_ID};

use crate::support::{bounds, content, scene, steps, terrain_under, Steps};

/// The bunny's triangles: Stanford's 69 451 faces, none dropped (measured, and listed by
/// `ledge_bunny`).
const BUNNY: usize = 69_451;

/// Every triangle: the terrain's 480 256, the craft's 170, the bunny's and the planet's 16 128.
const TRIANGLES: usize = 480_256 + 170 + BUNNY + 16_128;

/// Oracle: the order of assembly: the terrain, each craft's hull and engines, the bunny and the
/// planet's eight bands, covering every primitive once; 566 005 triangles and 16 materials.
#[test]
fn the_parts_cover_every_primitive_in_order() {
    let content = content();
    assert_eq!(content.triangles.len(), TRIANGLES);
    assert_eq!(TRIANGLES, 566_005);
    assert_eq!(content.part(PartKind::Bunny).len(), BUNNY);
    assert_eq!(ledge_bunny().expect("places").dropped.len(), 0);
    assert_eq!(content.parts.len(), 1 + 6 + 1 + 8);
    assert_eq!(content.parts.first().map(|p| p.kind), Some(PartKind::Terrain));
    assert_eq!(content.parts.get(7).map(|p| p.kind), Some(PartKind::Bunny));
    let mut next = 0;
    for part in &content.parts {
        assert_eq!(part.primitives.start, next, "{:?}", part.kind);
        next = part.primitives.end;
    }
    assert_eq!(next, TRIANGLES);
    assert_eq!(content.material_of.len(), TRIANGLES);
    assert_eq!(canyon_materials().expect("valid").len(), 16);
    assert!(content.material_of.iter().all(|&m| m < 16));
}

/// Oracle: Bunny's trace domain (`|raw| < 2^40`, 256 units) and its surface rule: a triangle
/// has a `TriangleSurface` exactly when its exact normal is nonzero; and the scene keeps the
/// lighting it was given, the sun's largest component exactly 1.0.
#[test]
fn every_triangle_is_in_the_trace_domain_and_none_is_degenerate() {
    let scene = scene();
    assert_eq!(CANYON_SCENE_ID, "lumen-canyon-scene/1");
    let mut coordinates = 0_usize;
    for (primitive, triangle) in scene.triangles().iter().enumerate() {
        for vertex in triangle.vertices() {
            for c in [vertex.x, vertex.y, vertex.z] {
                assert!(c.raw().abs() < 256 * ONE_RAW, "primitive {primitive}: {c:?}");
                coordinates += 1;
            }
        }
        let index = PrimitiveIndex::new(u32::try_from(primitive).expect("fits"));
        assert!(scene.surface(index).is_some(), "primitive {primitive} is degenerate");
    }
    assert_eq!(coordinates, TRIANGLES * 9);
    let lighting = canyon_lighting().expect("valid");
    assert_eq!(scene.lighting(), lighting);
    let sun = lighting.sun.expect("a sun");
    assert_eq!(sun.toward_light().z.raw(), ONE_RAW, "the sun's largest component is 1.0");
}

/// Oracle: Bunny's `VertexNormals` rule (a vertex no face names has no normal): every bunny
/// triangle has a normal at all three vertices and no other part has any, so the scene is
/// rendered by the smooth integrator and the terrain and craft keep their facets.
#[test]
fn only_the_bunny_is_smooth() {
    let content = content();
    for part in &content.parts {
        for normals in &content.normals[part.primitives.clone()] {
            if part.kind == PartKind::Bunny {
                assert!(normals.iter().all(Option::is_some), "a bunny vertex has no normal");
            } else {
                assert_eq!(normals, &[None; 3], "{:?} has a vertex normal", part.kind);
            }
        }
    }
    assert_eq!(scene().smooth_triangles(), BUNNY);
    assert_eq!(lumen::Integrator::for_scene(scene()).id(), "lumen-smooth-diffuse-path/1");
}

/// Oracle: resting, to the grid step (a tolerance of zero): the bunny's lowest vertex is at the
/// ledge's height, no terrain vertex under the bunny is above it (not sunk), and the cell
/// under that lowest vertex is level at the ledge's height on all four corners (not floating).
#[test]
fn the_bunny_rests_on_its_ledge() {
    let bunny: Vec<Steps> =
        content().part(PartKind::Bunny).iter().flatten().map(|&v| steps(v)).collect();
    let [low, high] = bounds(bunny.iter().copied());
    assert_eq!(low[1], LEDGE_Y, "the lowest vertex");
    let under = terrain_under(low, high);
    assert!(under.len() > 100, "{} terrain vertices", under.len());
    assert!(under.iter().all(|p| p[1] <= LEDGE_Y), "terrain above the ledge under the bunny");
    let lowest = bunny.iter().find(|p| p[1] == low[1]).copied().expect("a lowest vertex");
    let cell = terrain_under(lowest, lowest);
    assert_eq!(cell.len(), 4, "the cell under the lowest vertex");
    assert!(cell.iter().all(|p| p[1] == LEDGE_Y), "{cell:?}");
}
