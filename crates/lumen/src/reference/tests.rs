//! Tests of the reference scene's content.

use bunny_num::fixed_q32_32::ONE_RAW;
use bunny_num::FixedQ32_32;

use super::shapes::OCTAHEDRON_CENTRE;
use super::*;
use crate::vertex_grid::{at, GRID_RAW, U};

#[test]
fn the_reference_scene_has_the_stated_triangles_and_materials() {
    assert_eq!(reference_triangles().expect("the bunny places").len(), REFERENCE_TRIANGLE_COUNT);
    assert_eq!(BUNNY_FIRST_PRIMITIVE, 27_660);
    assert_eq!(REFERENCE_TRIANGLE_COUNT, 31_511);
    let scene = reference_scene().expect("the reference scene builds");
    assert_eq!(scene.triangles().len(), 31_511);
    let materials = reference_materials();
    for (primitive, want) in [(0, 0), (1, 0), (2, 1), (3, 1), (4, 2), (11, 2), (12, 3)] {
        assert_eq!(scene.albedo(primitive), Some(&materials[want]), "primitive {primitive}");
    }
    assert_eq!(scene.albedo(27_659), Some(&materials[3]));
    assert_eq!(scene.albedo(27_660), Some(&materials[4]));
    assert_eq!(scene.albedo(31_510), Some(&materials[4]));
    assert_eq!(scene.albedo(31_511), None);
    // Five distinct albedos.
    for (i, a) in REFERENCE_ALBEDO_256.iter().enumerate() {
        assert!(REFERENCE_ALBEDO_256[..i].iter().all(|b| b != a), "albedo {i} repeats");
    }
    let lighting = reference_lighting();
    assert_eq!(scene.lighting(), lighting);
    let sun = lighting.sun.expect("a sun");
    assert_eq!(sun.toward_light().z, FixedQ32_32::ONE);
}

#[test]
fn every_coordinate_is_on_the_f32_exact_grid_inside_the_trace_domain() {
    for triangle in reference_triangles().expect("the bunny places") {
        for vertex in triangle {
            for c in [vertex.x, vertex.y, vertex.z] {
                assert_eq!(c.raw() % GRID_RAW, 0, "{c:?} is off the 2^-16 grid");
                assert!(c.raw().abs() < 256 * ONE_RAW);
                // Exactly an f32 (a boundary conversion only: no float reaches a pixel).
                let narrowed = FixedQ32_32::try_from_f32(c.to_f32()).expect("finite");
                assert_eq!(narrowed, c);
            }
        }
    }
}

#[test]
fn every_surface_normal_points_out_of_its_object() {
    let scene = reference_scene().expect("the reference scene builds");
    let (cx, cy, cz) = OCTAHEDRON_CENTRE;
    // The bunny is not convex, so its faces have no centre to point away from; its winding
    // is checked by `reference::bunny::tests::the_bunny_winding_is_consistent_and_its_holes_are_counted`.
    let centre_of = |primitive: usize| match primitive {
        0..=3 => None,
        4..=11 => Some(at(cx, cy, cz)),
        12..BUNNY_FIRST_PRIMITIVE => Some(at(0, 0, 0)),
        _ => None,
    };
    for (primitive, triangle) in scene.triangles().iter().enumerate() {
        let index = u32::try_from(primitive).expect("fits");
        let surface = scene.surface(index).expect("no reference triangle is degenerate");
        let normal = surface.frame().normal();
        let expected_axis = match primitive {
            0 | 1 => Some(at(0, U, 0)),
            2 | 3 => Some(at(0, 0, U)),
            _ => None,
        };
        if let Some(axis) = expected_axis {
            assert_eq!(normal, axis, "triangle {primitive}");
        }
        if let Some(centre) = centre_of(primitive) {
            let [a, b, c] = triangle.vertices();
            let outward = a + b + c - centre - centre - centre;
            assert!(normal.dot(outward) > FixedQ32_32::ZERO, "triangle {primitive} faces in");
        }
    }
}

#[test]
fn octahedron_normals_are_the_diagonals() {
    let scene = reference_scene().expect("the reference scene builds");
    // 1 / sqrt(3) = 0.57735026918962576..., times 2^32 = 2479700524.8...
    let third = 2_479_700_525_i64;
    for primitive in 4..12 {
        let normal = scene.surface(primitive).expect("surface").frame().normal();
        for c in [normal.x, normal.y, normal.z] {
            assert!((c.raw().abs() - third).abs() <= 2, "{c:?}");
        }
    }
}
