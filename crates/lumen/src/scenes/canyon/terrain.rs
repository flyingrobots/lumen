//! The canyon's terrain: the heightfield spec the scene uses, where its grid lies in the scene,
//! the triangle mesh over it, and the rock stratum each triangle is coloured by.

use bunny_linalg::FixedVec3;

use super::heightfield::{
    generate, Heightfield, HeightfieldError, HeightfieldSpec, NoiseLayer, NoiseWeights, Shelf,
    Terraces,
};
use crate::vertex_grid::{at, U};

/// A grid cell's sides in `2^-16` grid steps: an eighth of a unit across the canyon (X) and
/// three eighths along it (Z), so a wall, which rises across the canyon, has facets about as
/// tall as they are long.
pub const CELL: (i64, i64) = (U / 8, U * 3 / 8);

/// Vertices across the canyon (X): 513, so 512 cells from `x = -40` to `x = 24`.
pub const COLUMNS: u32 = 513;

/// Vertices along the canyon (-Z): 470, so 469 cells from `z = 16` to `z = -159.875`.
pub const ROWS: u32 = 470;

/// The X of column 0 and the Z of row 0, in grid steps; column `i` is at `X0 + i * CELL.0` and
/// row `j` at `Z0 - j * CELL.1`.
pub const ORIGIN: (i64, i64) = (-40 * U, 16 * U);

/// The height of the bunny's ledge in grid steps: the top of the walls' first tier.
pub const LEDGE_Y: i64 = 3 * U;

/// The bunny's ledge: a level shelf cut into the left wall near the camera, at the height of
/// the first tier's tread.
pub const LEDGE: Shelf = Shelf { columns: [210, 231], rows: [20, 51], height: LEDGE_Y };

/// Where the middle of the bunny's footprint stands on the ledge: column 219, row 39.
pub const BUNNY_SPOT: (u32, u32) = (220, 39);

/// The noise layers: lattice spacings from 128 vertices down to 1, each three quarters of the
/// amplitude of the one before from 1.5 units (the finest two halved again), so the walls
/// and the rim are rough at every scale.
pub const LAYERS: [NoiseLayer; 8] = [
    NoiseLayer { spacing: 128, amplitude: 3 * U / 2 },
    NoiseLayer { spacing: 64, amplitude: 9 * U / 8 },
    NoiseLayer { spacing: 32, amplitude: 27 * U / 32 },
    NoiseLayer { spacing: 16, amplitude: 81 * U / 128 },
    NoiseLayer { spacing: 8, amplitude: 243 * U / 512 },
    NoiseLayer { spacing: 4, amplitude: 729 * U / 2048 },
    NoiseLayer { spacing: 2, amplitude: 729 * U / 4096 },
    NoiseLayer { spacing: 1, amplitude: 729 * U / 8192 },
];

/// The scene's heightfield: a floor 14 units across, walls of four tiers each rising 3 units
/// over 1 unit, then a 0.5 unit tread, and a ragged plateau about 12 units up; the centre
/// line bends about 9 units to the left between 30 and 60 units from the camera.
pub const CANYON_HEIGHTFIELD: HeightfieldSpec<'static> = HeightfieldSpec {
    seed: 2009,
    columns: COLUMNS,
    rows: ROWS,
    layers: &LAYERS,
    centre: 320,
    meander: NoiseLayer { spacing: 86, amplitude: 128 },
    floor_half_width: 56,
    terraces: Terraces { count: 4, width: 12, riser: 8, height: 3 * U },
    weights: NoiseWeights { floor: 1, wall: 8, plateau: 6 },
    shelves: &[LEDGE],
};

/// The thickness of a rock stratum in grid steps: 1.5 units.
pub const STRATUM: i64 = 3 * U / 2;

/// The rock material of each stratum, from the floor up, repeating: indices into the scene's
/// material table (`look::CANYON_ALBEDO_256`).
pub const STRATA: [u32; 4] = [0, 1, 0, 2];

/// The point of column `column`, row `row`, at `height` grid steps.
#[must_use]
pub fn grid_point(column: u32, row: u32, height: i64) -> FixedVec3 {
    let (x0, z0) = ORIGIN;
    at(x0 + i64::from(column) * CELL.0, height, z0 - i64::from(row) * CELL.1)
}

/// The terrain's heights: [`generate`] of [`CANYON_HEIGHTFIELD`].
///
/// # Errors
///
/// A [`HeightfieldError`] if the spec is refused; a test shows it is not.
pub fn canyon_heights() -> Result<Heightfield, HeightfieldError> {
    generate(&CANYON_HEIGHTFIELD)
}

/// The material of a terrain cell, and of both its triangles: the [`STRATA`] entry of the
/// stratum its four corners' mean height falls in, `(h00 + h10 + h01 + h11).div_euclid(4 *
/// STRATUM)` modulo 4 (heights in grid steps). A whole cell takes one colour, so a steep face
/// shows level bands, not a saw-tooth of alternating triangles.
#[must_use]
pub fn stratum_material(corners: [i64; 4]) -> u32 {
    let band = corners.iter().sum::<i64>().div_euclid(4 * STRATUM).rem_euclid(4);
    usize::try_from(band).ok().and_then(|band| STRATA.get(band)).copied().unwrap_or(0)
}

/// The terrain mesh and each triangle's material: for each cell, row by row and across each
/// row, the two triangles `(i, j), (i + 1, j), (i + 1, j + 1)` and `(i, j), (i + 1, j + 1),
/// (i, j + 1)`, wound counter-clockwise from above, both of the cell's [`stratum_material`].
/// Their projections onto the ground are half cells, so no triangle has zero area.
#[must_use]
pub fn terrain_triangles(field: &Heightfield) -> (Vec<[FixedVec3; 3]>, Vec<u32>) {
    let cells = |n: u32| n.saturating_sub(1);
    let mut triangles = Vec::new();
    let mut materials = Vec::new();
    for row in 0..cells(field.rows()) {
        for column in 0..cells(field.columns()) {
            let corner = |i: u32, j: u32| field.height(i, j).map(|h| (grid_point(i, j, h), h));
            let corners = [
                corner(column, row),
                corner(column + 1, row),
                corner(column + 1, row + 1),
                corner(column, row + 1),
            ];
            if let [Some((a, ha)), Some((b, hb)), Some((c, hc)), Some((d, hd))] = corners {
                triangles.push([a, b, c]);
                triangles.push([a, c, d]);
                let material = stratum_material([ha, hb, hc, hd]);
                materials.extend([material, material]);
            }
        }
    }
    (triangles, materials)
}
