//! The banded planet: a sphere of latitude rows and longitude columns, tilted, far up the
//! canyon in the sky, each latitude band its own material.
//!
//! The sphere's points come from Bunny's table-based `FixedAngle::sin_cos` (each within one raw
//! unit of the exact value), so no float is involved; each coordinate is floored to the grid.

use bunny_num::FixedAngle;

use super::content::{CanyonContent, PartKind, PLANET_BANDS};
use crate::scenes::fixed_point::solids::{facing, StepTriangle, Steps};
use crate::vertex_grid::{at, U};

/// The planet's centre in grid steps: up and to the left, far along the canyon.
pub const PLANET_CENTRE: Steps = [-44 * U, 48 * U, -200 * U];

/// The planet's radius in grid steps: 22 units.
pub const PLANET_RADIUS: i64 = 22 * U;

/// Longitude columns: 128, each `1/128` turn.
pub const LONGITUDES: u32 = 128;

/// Latitude rows from the south pole to the north: 64, each `1/128` turn.
pub const LATITUDES: u32 = 64;

/// The latitude rows of each band, from the south pole: 64 in all.
pub const BAND_ROWS: [u32; PLANET_BANDS as usize] = [8, 6, 10, 6, 12, 6, 8, 8];

/// The tilt of the planet's axis from `+Y` toward `-X`: `1/32` turn, about 11 degrees.
const TILT_RAW: u64 = 1 << 59;

/// `a * b` for two Q32.32 raw values, floored.
///
/// Every call has one factor at most `2^34` in magnitude (a sine or cosine, at most `2^32`, a
/// sum of two products of them, at most `2^33`, or [`PLANET_RADIUS`], below `2^21`) and the
/// other at most `2^34` too, so the product is below `2^68` and the result below `2^36`: the
/// conversion always succeeds and the fallback is never taken. A test checks the result exact up
/// to factors far past those.
fn mul(a: i64, b: i64) -> i64 {
    i64::try_from((i128::from(a) * i128::from(b)) >> 32).unwrap_or(0)
}

/// The point at latitude row `row` (0 the south pole) and longitude `column`, in grid steps.
fn point(row: u32, column: u32) -> Steps {
    let quarter = FixedAngle::QUARTER_TURN.raw();
    let latitude = FixedAngle::from_raw((u64::from(row) << 57).wrapping_sub(quarter));
    let longitude = FixedAngle::from_raw(u64::from(column) << 57);
    let (sin_lat, cos_lat) = latitude.sin_cos();
    let (sin_lon, cos_lon) = longitude.sin_cos();
    let (sin_t, cos_t) = FixedAngle::from_raw(TILT_RAW).sin_cos();
    let x = mul(cos_lat.raw(), cos_lon.raw());
    let y = sin_lat.raw();
    let z = mul(cos_lat.raw(), sin_lon.raw());
    let (tx, ty) =
        (mul(x, cos_t.raw()) - mul(y, sin_t.raw()), mul(x, sin_t.raw()) + mul(y, cos_t.raw()));
    let [cx, cy, cz] = PLANET_CENTRE;
    [cx + mul(PLANET_RADIUS, tx), cy + mul(PLANET_RADIUS, ty), cz + mul(PLANET_RADIUS, z)]
}

/// The triangles of latitude row `row`: a quad per longitude, or one triangle at a pole, each
/// facing out of the sphere.
fn row_triangles(row: u32) -> Vec<StepTriangle> {
    let mut out = Vec::new();
    let away = |p: Steps| [0, 1, 2].map(|k| p[k] - PLANET_CENTRE[k]);
    for column in 0..LONGITUDES {
        let next = (column + 1) % LONGITUDES;
        let (a, b) = (point(row, column), point(row, next));
        let (c, d) = (point(row + 1, next), point(row + 1, column));
        if row > 0 {
            out.push(facing(a, b, d, away(a)));
        }
        if row + 1 < LATITUDES {
            out.push(facing(b, c, d, away(b)));
        }
    }
    out
}

/// Appends the planet to `content`: each band's rows, from the south pole, as
/// [`PartKind::Band`] parts.
pub fn push_planet(content: &mut CanyonContent) {
    let mut row = 0;
    for (band, &rows) in (0..PLANET_BANDS).zip(&BAND_ROWS) {
        let mut triangles = Vec::new();
        for r in row..row + rows {
            triangles.extend(row_triangles(r).iter().map(|t| t.map(|[x, y, z]| at(x, y, z))));
        }
        content.push(PartKind::Band(band), &triangles);
        row += rows;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: exact products worked by hand, at and past the largest factors [`mul`]'s callers
    /// pass (`2^34`): one times one, a negative product floored, `2^34 * 2^34`, and `i64::MAX`
    /// times one, the largest result that fits.
    #[test]
    fn the_planets_product_is_exact_past_every_factor_it_is_given() {
        let one = 1_i64 << 32;
        assert_eq!(mul(one, one), one);
        assert_eq!(mul(-1, 1), -1);
        assert_eq!(mul(1 << 34, 1 << 34), 1 << 36);
        assert_eq!(mul(i64::MAX, one), i64::MAX);
    }
}
