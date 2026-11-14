//! The craft and the planet: their triangle counts, materials, the trace domain, and the craft
//! clear of the rock and of each other.

use std::sync::OnceLock;

use bunny_num::fixed_q32_32::ONE_RAW;
use bunny_optics::TriangleSurface;
use bunny_query::TraceTriangle;
use lumen::scenes::canyon::craft::push_craft;
use lumen::scenes::canyon::planet::push_planet;
use lumen::scenes::canyon::{CanyonContent, Craft, PartKind, PLANET_BANDS};

use crate::support::{bounds, steps, terrain_under, Steps};

/// The craft and the planet, appended to empty content in that order.
fn parts() -> &'static CanyonContent {
    static PARTS: OnceLock<CanyonContent> = OnceLock::new();
    PARTS.get_or_init(|| {
        let mut content = CanyonContent::default();
        push_craft(&mut content);
        push_planet(&mut content);
        content
    })
}

/// The triangles of the part `kind`, in grid steps. The part exists and is not empty.
fn part(kind: PartKind) -> Vec<[Steps; 3]> {
    let triangles = parts().part(kind);
    assert!(!triangles.is_empty(), "{kind:?} has no triangles");
    triangles.iter().map(|t| t.map(steps)).collect()
}

/// Oracle: counts derived by hand from the construction: two triangles per quad, a fan of
/// `n - 2` per `n`-sided face, and 128 longitudes per latitude row, one triangle each at the
/// poles; every triangle inside Bunny's trace domain and none degenerate.
#[test]
fn every_craft_and_band_has_its_derived_triangle_count() {
    let mut expected = vec![
        // Hull: 6 side quads, 6 nose triangles, a hexagonal tail (4); two wings of 6 quads;
        // a fin of 2 triangles and 3 quads.
        (PartKind::Hull(Craft::Skiff), 12 + 6 + 4 + 2 * 12 + 2 + 6),
        (PartKind::Engine(Craft::Skiff), 12),
        // A diamond: 8 triangles.
        (PartKind::Hull(Craft::Kite), 8),
        (PartKind::Engine(Craft::Kite), 2 * 12),
        // The wedge, the wing and two pontoons: four boxes' worth.
        (PartKind::Hull(Craft::Tug), 4 * 12),
        (PartKind::Engine(Craft::Tug), 2 * 12),
    ];
    // Bands of 8, 6, 10, 6, 12, 6, 8 and 8 latitude rows; the first and last end at a pole.
    let rows = [8, 6, 10, 6, 12, 6, 8, 8];
    for (band, rows) in (0..PLANET_BANDS).zip(rows) {
        let poles = usize::from(band == 0 || band == PLANET_BANDS - 1);
        expected.push((PartKind::Band(band), rows * 2 * 128 - poles * 128));
    }
    let content = parts();
    for &(kind, n) in &expected {
        assert_eq!(content.part(kind).len(), n, "{kind:?}");
    }
    assert_eq!(content.parts.len(), expected.len());
    assert_eq!(content.triangles.len(), 170 + 16_128);
    for (primitive, t) in content.triangles.iter().enumerate() {
        assert!(t.iter().flat_map(|v| [v.x, v.y, v.z]).all(|c| c.raw().abs() < 256 * ONE_RAW));
        let traced = TraceTriangle::new(t[0], t[1], t[2]).expect("inside the domain");
        assert!(TriangleSurface::new(&traced).is_some(), "primitive {primitive} is degenerate");
    }
}

/// Oracle: one material per part, as `PartKind::material` states, the planet's eight bands each
/// its own; and no vertex normals (craft and planet are shaded flat).
#[test]
fn every_part_is_made_of_its_own_material() {
    let content = parts();
    for part in &content.parts {
        let materials = &content.material_of[part.primitives.clone()];
        assert!(materials.iter().all(|&m| m == part.kind.material()), "{:?}", part.kind);
    }
    let bands: std::collections::BTreeSet<u32> =
        (0..PLANET_BANDS).map(|band| PartKind::Band(band).material()).collect();
    assert_eq!(bands.len(), usize::from(PLANET_BANDS));
    assert!(content.normals.iter().all(|n| n == &[None; 3]));
}

/// Oracle: clearance by heights. Every terrain triangle that overlaps a craft's box on X and Z
/// is below the box (its vertices, and so the whole triangle, under the craft's lowest point),
/// by at least half a unit; and the three craft's boxes do not overlap.
#[test]
fn the_craft_fly_clear_of_the_rock_and_each_other() {
    let boxes: Vec<[Steps; 2]> = Craft::ALL
        .iter()
        .map(|&craft| {
            let hull = part(PartKind::Hull(craft)).into_iter().flatten();
            let engine = part(PartKind::Engine(craft)).into_iter().flatten();
            bounds(hull.chain(engine))
        })
        .collect();
    for (craft, &[low, high]) in Craft::ALL.iter().zip(&boxes) {
        let under = terrain_under(low, high);
        assert!(under.len() > 50, "{craft:?}: {} terrain vertices", under.len());
        let top = under.iter().map(|p| p[1]).max().expect("terrain");
        assert!(low[1] - top >= 1 << 15, "{craft:?}: rock at {top}, craft from {}", low[1]);
    }
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            let apart = (0..3).any(|k| a[1][k] < b[0][k] || b[1][k] < a[0][k]);
            assert!(apart, "{a:?} and {b:?} overlap");
        }
    }
}
