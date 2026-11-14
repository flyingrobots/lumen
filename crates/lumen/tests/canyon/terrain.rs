//! The terrain mesh over the heightfield: its triangles and their rock strata.

use bunny_num::fixed_q32_32::ONE_RAW;
use bunny_optics::TriangleSurface;
use bunny_query::TraceTriangle;
use lumen::scenes::canyon::terrain::{
    grid_point, stratum_material, terrain_triangles, COLUMNS, ROWS, STRATA, STRATUM,
};

use crate::support::{heights, steps};

/// Oracle: the construction, two triangles per cell of the 512 by 469 cell grid in row order,
/// each on the grid point of its corners; Bunny's trace domain (`|raw| < 2^40`); and Bunny's
/// surface rule (a `TriangleSurface` exactly when the exact normal is nonzero), with the normal
/// pointing up (each projects onto half a cell, wound counter-clockwise from above).
#[test]
fn the_terrain_is_two_upward_triangles_per_cell() {
    let (triangles, materials) = terrain_triangles(heights());
    assert_eq!(triangles.len(), 2 * 512 * 469);
    assert_eq!(materials.len(), triangles.len());
    assert_eq!((COLUMNS, ROWS), (513, 470));
    let corner = |i: u32, j: u32| grid_point(i, j, heights().height(i, j).expect("on the grid"));
    for (cell, pair) in triangles.chunks(2).enumerate() {
        let cell = u32::try_from(cell).expect("fits");
        let (i, j) = (cell % 512, cell / 512);
        let quad = [corner(i, j), corner(i + 1, j), corner(i + 1, j + 1), corner(i, j + 1)];
        assert_eq!(
            pair,
            [[quad[0], quad[1], quad[2]], [quad[0], quad[2], quad[3]]],
            "cell ({i}, {j})"
        );
        for triangle in pair {
            for c in triangle.iter().flat_map(|v| [v.x, v.y, v.z]) {
                assert!(c.raw().abs() < 256 * ONE_RAW, "cell ({i}, {j}): {c:?}");
            }
            let traced = TraceTriangle::new(triangle[0], triangle[1], triangle[2]).expect("inside");
            assert!(TriangleSurface::new(&traced).is_some(), "cell ({i}, {j}) is degenerate");
            // The Y of the exact normal (b - a) x (c - a): its projection's doubled signed area.
            let [p0, p1, p2] = triangle.map(steps);
            let up = (p1[2] - p0[2]) * (p2[0] - p0[0]) - (p1[0] - p0[0]) * (p2[2] - p0[2]);
            assert!(up > 0, "cell ({i}, {j}) faces down");
        }
    }
}

/// Oracle: the stratum rule written out here: a cell's four corners' mean height, in bands of
/// `STRATUM`, picks `STRATA[band mod 4]`, and both of the cell's triangles take it. Witness:
/// each of the three rock materials covers more than 10 000 triangles.
#[test]
fn every_cell_is_coloured_by_its_stratum() {
    let (_, materials) = terrain_triangles(heights());
    let mut used = [0_usize; 3];
    for (cell, pair) in materials.chunks(2).enumerate() {
        let cell = u32::try_from(cell).expect("fits");
        let (i, j) = (cell % 512, cell / 512);
        let h = |i, j| heights().height(i, j).expect("on the grid");
        let sum = h(i, j) + h(i + 1, j) + h(i + 1, j + 1) + h(i, j + 1);
        let band = usize::try_from(sum.div_euclid(4 * STRATUM).rem_euclid(4)).expect("small");
        assert_eq!(pair, [STRATA[band]; 2], "cell ({i}, {j})");
        used[usize::try_from(STRATA[band]).expect("small")] += 2;
    }
    assert_eq!(STRATA, [0, 1, 0, 2]);
    assert!(used.iter().all(|&n| n > 10_000), "{used:?}");
    assert!(steps(grid_point(0, 0, 0)) == [-40 * 65_536, 0, 16 * 65_536]);
}

/// Oracle: the stratum rule worked by hand at its band edges (`4 * STRATUM = 393 216`): a sum
/// of 0 is band 0, 393 216 band 1, 786 432 band 2 and -1 band 3, floored. At the extremes the
/// sum leaves `i64`: `4 * i64::MAX = 2^65 - 4` over 393 216 floors to `(2^48 - 1) / 3`, which is
/// 1 modulo 4, and `4 * i64::MIN = -2^65` to `-(2^48 + 2) / 3`, which is 2 modulo 4.
#[test]
fn every_band_and_both_extremes_have_a_stratum() {
    let band = 4 * STRATUM;
    assert_eq!(band, 393_216);
    assert_eq!(stratum_material([0; 4]), STRATA[0]);
    assert_eq!(stratum_material([band, 0, 0, 0]), STRATA[1]);
    assert_eq!(stratum_material([band, band - 1, 0, 1]), STRATA[2]);
    assert_eq!(stratum_material([0, 0, -1, 0]), STRATA[3]);
    assert_eq!(stratum_material([i64::MAX; 4]), STRATA[1]);
    assert_eq!(stratum_material([i64::MIN; 4]), STRATA[2]);
}
