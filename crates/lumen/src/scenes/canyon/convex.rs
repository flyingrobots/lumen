//! Convex solids from listed corners and faces, posed by rational rotations: the building
//! blocks of the craft.

use crate::scenes::fixed_point::solids::{facing, StepTriangle, Steps};

/// A convex solid: its corners in grid steps and its faces, each a convex polygon of corner
/// indices in order around its edge.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Convex<'a> {
    /// The corners.
    pub(crate) corners: &'a [Steps],
    /// The faces.
    pub(crate) faces: &'a [&'a [usize]],
}

/// A rotation whose cosine is `cos / hyp` and sine `sin / hyp`, with `cos^2 + sin^2 = hyp^2`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rational {
    /// The cosine's numerator.
    pub cos: i64,
    /// The sine's numerator.
    pub sin: i64,
    /// The common denominator.
    pub hyp: i64,
}

impl Rational {
    /// No turn.
    pub const NONE: Self = Self { cos: 1, sin: 0, hyp: 1 };

    /// `(u, v)` turned from `+u` toward `+v`, each coordinate floored to a grid step.
    const fn turn(self, u: i64, v: i64) -> (i64, i64) {
        let turned_u = (self.cos * u - self.sin * v).div_euclid(self.hyp);
        let turned_v = (self.sin * u + self.cos * v).div_euclid(self.hyp);
        (turned_u, turned_v)
    }
}

/// How a solid is placed: banked about `+Z` (from `+X` toward `+Y`), then turned about `+Y`
/// (from `+Z` toward `+X`), then moved by `offset` grid steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pose {
    /// The bank about `+Z`.
    pub bank: Rational,
    /// The heading about `+Y`.
    pub heading: Rational,
    /// The position of the solid's origin.
    pub offset: Steps,
}

impl Pose {
    /// `p` posed.
    #[must_use]
    pub const fn place(&self, [x, y, z]: Steps) -> Steps {
        let (x, y) = self.bank.turn(x, y);
        let (z, x) = self.heading.turn(z, x);
        [x + self.offset[0], y + self.offset[1], z + self.offset[2]]
    }
}

/// Appends the faces of `solid`, posed by `pose`, as triangles: each face a fan from its first
/// corner, every triangle wound to face away from the solid's centre.
pub(crate) fn push_convex(out: &mut Vec<StepTriangle>, solid: Convex<'_>, pose: &Pose) {
    let corners: Vec<Steps> = solid.corners.iter().map(|&p| pose.place(p)).collect();
    let count = i64::try_from(corners.len()).unwrap_or(1).max(1);
    let centre = mean(corners.iter().copied(), count);
    for face in solid.faces {
        let points: Vec<Steps> = face.iter().filter_map(|&k| corners.get(k).copied()).collect();
        let size = i64::try_from(points.len()).unwrap_or(1).max(1);
        let middle = mean(points.iter().copied(), size);
        let away = [0, 1, 2].map(|k| middle[k] - centre[k]);
        if let Some((&first, rest)) = points.split_first() {
            for pair in rest.windows(2) {
                if let [b, c] = *pair {
                    out.push(facing(first, b, c, away));
                }
            }
        }
    }
}

/// The mean of `points`, floored, given their count.
fn mean(points: impl Iterator<Item = Steps>, count: i64) -> Steps {
    let sum = points.fold([0; 3], |s, p| [s[0] + p[0], s[1] + p[1], s[2] + p[2]]);
    sum.map(|c| c.div_euclid(count))
}

/// The faces of a box whose corner `k` has the high `x` if bit 0 of `k` is set, the high `y`
/// for bit 1 and the high `z` for bit 2: low and high `z`, low and high `y`, low and high `x`.
pub(crate) const BOX_FACES: [&[usize]; 6] =
    [&[0, 1, 3, 2], &[4, 5, 7, 6], &[0, 1, 5, 4], &[2, 3, 7, 6], &[0, 2, 6, 4], &[1, 3, 7, 5]];

/// The eight corners of the box from `low` to `high`, in [`BOX_FACES`] order.
pub(crate) fn box_corners(low: Steps, high: Steps) -> [Steps; 8] {
    let pick = |k: usize| {
        [
            if k & 1 == 0 { low[0] } else { high[0] },
            if k & 2 == 0 { low[1] } else { high[1] },
            if k & 4 == 0 { low[2] } else { high[2] },
        ]
    };
    [pick(0), pick(1), pick(2), pick(3), pick(4), pick(5), pick(6), pick(7)]
}
