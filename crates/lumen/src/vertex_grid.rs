//! The vertex grid of the reference scene: every coordinate is a whole number of `2^-16` steps.
//!
//! This module owns the grid's step, the constructors of values in grid steps and the floor to
//! the grid ([`snap`]).

use bunny_linalg::FixedVec3;
use bunny_num::FixedQ32_32;

/// One raw unit of the vertex grid: `2^-16`. See [`snap`].
pub(crate) const GRID_RAW: i64 = 1 << 16;

/// A value of `units / 2^16` units: every scene constant is a whole number of grid steps.
pub(crate) const fn grid(units_times_2_16: i64) -> FixedQ32_32 {
    FixedQ32_32::from_raw(units_times_2_16 * GRID_RAW)
}

/// A point from three coordinates in grid steps of `2^-16` units.
pub(crate) const fn at(x: i64, y: i64, z: i64) -> FixedVec3 {
    FixedVec3::new(grid(x), grid(y), grid(z))
}

/// Whole units in grid steps.
pub(crate) const U: i64 = 1 << 16;

/// Floors a raw coordinate to a multiple of `2^16` raw units (`2^-16` units).
///
/// Every coordinate of the reference scene is below 256 units in magnitude and a multiple
/// of `2^-16`, so it has at most 24 significant bits and is exactly an `f32`. A mesh in the
/// `bunny-ply-binary-le-f32/1` layout (Geordi's scene profile) can therefore carry the
/// scene's vertices without changing a bit.
pub(crate) const fn snap(value: FixedQ32_32) -> FixedQ32_32 {
    FixedQ32_32::from_raw(value.raw() & !(GRID_RAW - 1))
}
