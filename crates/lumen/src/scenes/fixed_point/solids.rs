//! Exact construction of flat shapes and prisms from points in whole `2^-16` grid steps.
//!
//! This module owns the small vocabulary every part of the Fixed Point scene is built from:
//! points in grid steps ([`Steps`]), triangles turned to face a stated side, rectangles,
//! fans, prism walls and polygon outlines from an integer cosine table. Nothing here rounds
//! except [`ring`], whose floor to a multiple of a stated step is part of its contract.

/// A point in whole grid steps of `2^-16` units: `[x, y, z]`.
pub(crate) type Steps = [i64; 3];

/// A triangle in grid steps.
pub(crate) type StepTriangle = [Steps; 3];

/// One unit in grid steps of `2^-16`.
pub const UNIT: i64 = 1 << 16;

/// `cos(15 k degrees) * 2^16`, rounded to nearest, for `k` in `0..=6` (0 to 90 degrees).
/// The 24 directions of [`ring`] are built from it by symmetry, so no trigonometry runs.
const COS_15_DEG: [i64; 7] = [65_536, 63_303, 56_756, 46_341, 32_768, 16_962, 0];

/// The exact cross product of `b - a` and `c - a`.
fn normal(a: Steps, b: Steps, c: Steps) -> [i128; 3] {
    let wide = |p: Steps| p.map(i128::from);
    let (a, b, c) = (wide(a), wide(b), wide(c));
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]]
}

/// The triangle `a, b, c`, its order swapped if needed so that it winds counter-clockwise
/// seen from the side `toward` points to (its normal has a positive dot product with it).
pub(crate) fn facing(a: Steps, b: Steps, c: Steps, toward: Steps) -> StepTriangle {
    let n = normal(a, b, c);
    let dot: i128 = n.iter().zip(toward).map(|(n, t)| n * i128::from(t)).sum();
    if dot < 0 {
        [a, c, b]
    } else {
        [a, b, c]
    }
}

/// Appends the quad `a, b, c, d` (in order around its edge, planar) as two triangles facing
/// `toward`.
pub(crate) fn quad(out: &mut Vec<StepTriangle>, [a, b, c, d]: [Steps; 4], toward: Steps) {
    out.push(facing(a, b, c, toward));
    out.push(facing(a, c, d, toward));
}

/// Appends the rectangle `x0..=x1` by `z0..=z1` at height `y`, facing up.
pub(crate) fn flat_rect(out: &mut Vec<StepTriangle>, [x0, z0, x1, z1]: [i64; 4], y: i64) {
    quad(out, [[x0, y, z0], [x1, y, z0], [x1, y, z1], [x0, y, z1]], [0, 1, 0]);
}

/// Appends a fan from `centre` over the closed outline `ring`, every triangle facing `toward`.
pub(crate) fn fan(out: &mut Vec<StepTriangle>, centre: Steps, ring: &[Steps], toward: Steps) {
    for (i, &a) in ring.iter().enumerate() {
        if let Some(&b) = ring.get(i + 1).or_else(|| ring.first()) {
            out.push(facing(centre, a, b, toward));
        }
    }
}

/// Appends the walls between the closed outline `base` and `base` moved by `rise`, each wall
/// facing away from `centre`.
pub(crate) fn walls(out: &mut Vec<StepTriangle>, base: &[Steps], rise: Steps, centre: Steps) {
    let lift = |p: Steps| [p[0] + rise[0], p[1] + rise[1], p[2] + rise[2]];
    for (i, &a) in base.iter().enumerate() {
        if let Some(&b) = base.get(i + 1).or_else(|| base.first()) {
            let mid = [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
            let away = [mid[0] - 2 * centre[0], mid[1] - 2 * centre[1], mid[2] - 2 * centre[2]];
            quad(out, [a, b, lift(b), lift(a)], away);
        }
    }
}

/// The `n`-gon (`n` = 24, 12 or 8) of radius `radius` steps around the origin of a plane, as
/// `(u, v)` offsets, counter-clockwise from `+u`. Each offset is `radius * cos` from the
/// table, floored to a multiple of `step` steps, so a turned copy can stay exact (see
/// `dice`).
pub(crate) fn ring(radius: i64, n: usize, step: i64) -> Vec<(i64, i64)> {
    let stride = 24 / n.clamp(1, 24);
    let cos = |k: usize| -> i64 {
        let k = k % 24;
        let (index, sign) = match k {
            0..=6 => (k, 1),
            7..=12 => (12 - k, -1),
            13..=18 => (k - 12, -1),
            _ => (24 - k, 1),
        };
        sign * COS_15_DEG.get(index).copied().unwrap_or(0)
    };
    let floor = |value: i64| ((radius * value) >> 16).div_euclid(step) * step;
    (0..24).step_by(stride).map(|k| (floor(cos(k)), floor(cos(k + 18)))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table is `cos(15 k degrees) * 2^16` rounded, from the exact values of the
    /// cosines at 0, 30, 45, 60 and 90 degrees and the two that are not exact.
    #[test]
    fn the_cosine_table_holds_the_rounded_cosines() {
        // cos 15 = (sqrt 6 + sqrt 2) / 4 = 0.96592582628..., times 2^16 = 63302.7...
        // cos 75 = (sqrt 6 - sqrt 2) / 4 = 0.25881904510..., times 2^16 = 16961.9...
        // cos 30 = sqrt 3 / 2, times 2^16 = 56755.8...; cos 45 = sqrt 2 / 2: 46340.95...
        assert_eq!(COS_15_DEG, [65_536, 63_303, 56_756, 46_341, 32_768, 16_962, 0]);
        let square = |v: i64| v * v;
        // Pairs 15 degrees apart from 90 sum to 1 within the rounding.
        for k in 0..=6 {
            let total = square(COS_15_DEG[k]) + square(COS_15_DEG[6 - k]);
            assert!((total - (1 << 32)).abs() < 2 * 65_536, "{k}");
        }
    }

    /// The ring's points are the table scaled and floored, in counter-clockwise order.
    #[test]
    fn a_ring_is_the_scaled_table_counter_clockwise() {
        let points = ring(1 << 16, 8, 1);
        assert_eq!(points.len(), 8);
        assert_eq!(points[0], (65_536, 0));
        assert_eq!(points[1], (46_341, 46_341));
        assert_eq!(points[2], (0, 65_536));
        assert_eq!(points[4], (-65_536, 0));
        assert_eq!(points[6], (0, -65_536));
        assert_eq!(ring(1000, 12, 25)[1], (850, 500));
    }

    #[test]
    fn facing_swaps_a_triangle_that_faces_away() {
        let (a, b, c) = ([0, 0, 0], [1, 0, 0], [0, 0, 1]);
        // Normal of (a, b, c) is (0, -1, 0).
        assert_eq!(facing(a, b, c, [0, 1, 0]), [a, c, b]);
        assert_eq!(facing(a, b, c, [0, -1, 0]), [a, b, c]);
    }
}
