//! Convex solids from listed corners and faces, posed by rational rotations: the building
//! blocks of the craft.

use core::fmt;

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

/// Why a rotation was refused or a point could not be posed.
///
/// Non-exhaustive: a later version may refuse for a new reason, so a `match` outside this crate
/// needs a wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PoseError {
    /// [`Rational::new`] was given a `hyp` that is not positive, or numerators with `cos^2 +
    /// sin^2` other than `hyp^2` (or squares outside `i64`).
    NotARotation {
        /// The cosine's numerator given.
        cos: i64,
        /// The sine's numerator given.
        sin: i64,
        /// The denominator given.
        hyp: i64,
    },
    /// A coordinate of `point` posed, or a sum of posed corners at `point`, is outside `i64`.
    Overflow {
        /// The point being posed or summed.
        point: Steps,
    },
}

impl fmt::Display for PoseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotARotation { cos, sin, hyp } => write!(
                f,
                "({cos}, {sin}) over {hyp} is not a rotation: hyp must be positive and \
                 cos^2 + sin^2 equal hyp^2"
            ),
            Self::Overflow { point: [x, y, z] } => {
                write!(f, "posing ({x}, {y}, {z}) overflows a grid coordinate")
            }
        }
    }
}

impl std::error::Error for PoseError {}

/// A rotation whose cosine is `cos / hyp` and sine `sin / hyp`, with `hyp` positive and
/// `cos^2 + sin^2 = hyp^2`: built only by [`Rational::new`], which checks both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rational {
    cos: i64,
    sin: i64,
    hyp: i64,
}

impl Rational {
    /// No turn.
    pub const NONE: Self = Self { cos: 1, sin: 0, hyp: 1 };

    /// The rotation whose cosine is `cos / hyp` and sine `sin / hyp`.
    ///
    /// # Errors
    ///
    /// [`PoseError::NotARotation`] unless `hyp` is positive and `cos^2 + sin^2 = hyp^2` exactly,
    /// every square and their sum inside `i64`.
    pub const fn new(cos: i64, sin: i64, hyp: i64) -> Result<Self, PoseError> {
        let refused = Err(PoseError::NotARotation { cos, sin, hyp });
        if hyp <= 0 {
            return refused;
        }
        let (Some(cos2), Some(sin2), Some(hyp2)) =
            (cos.checked_mul(cos), sin.checked_mul(sin), hyp.checked_mul(hyp))
        else {
            return refused;
        };
        match cos2.checked_add(sin2) {
            Some(sum) if sum == hyp2 => Ok(Self { cos, sin, hyp }),
            _ => refused,
        }
    }

    /// The cosine's numerator.
    #[must_use]
    pub const fn cos(self) -> i64 {
        self.cos
    }

    /// The sine's numerator.
    #[must_use]
    pub const fn sin(self) -> i64 {
        self.sin
    }

    /// The common denominator, positive.
    #[must_use]
    pub const fn hyp(self) -> i64 {
        self.hyp
    }

    /// `(u, v)` turned from `+u` toward `+v`, each coordinate floored to a grid step; `None`
    /// if a product or sum overflows.
    fn turn(self, u: i64, v: i64) -> Option<(i64, i64)> {
        let turned_u = self.cos.checked_mul(u)?.checked_sub(self.sin.checked_mul(v)?)?;
        let turned_v = self.sin.checked_mul(u)?.checked_add(self.cos.checked_mul(v)?)?;
        Some((turned_u.checked_div_euclid(self.hyp)?, turned_v.checked_div_euclid(self.hyp)?))
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
    /// `point` posed.
    ///
    /// # Errors
    ///
    /// [`PoseError::Overflow`] if a coordinate of the turned or moved point is outside `i64`.
    pub fn place(&self, point: Steps) -> Result<Steps, PoseError> {
        let overflow = PoseError::Overflow { point };
        let [x, y, z] = point;
        let (x, y) = self.bank.turn(x, y).ok_or(overflow)?;
        let (z, x) = self.heading.turn(z, x).ok_or(overflow)?;
        let [dx, dy, dz] = self.offset;
        let moved = (x.checked_add(dx), y.checked_add(dy), z.checked_add(dz));
        let (Some(x), Some(y), Some(z)) = moved else { return Err(overflow) };
        Ok([x, y, z])
    }
}

/// Appends the faces of `solid`, posed by `pose`, as triangles: each face a fan from its first
/// corner, every triangle wound to face away from the solid's centre. A solid with no corners,
/// and a face naming none of them, adds nothing.
pub(crate) fn push_convex(
    out: &mut Vec<StepTriangle>,
    solid: Convex<'_>,
    pose: &Pose,
) -> Result<(), PoseError> {
    let corners =
        solid.corners.iter().map(|&p| pose.place(p)).collect::<Result<Vec<Steps>, _>>()?;
    let Some((&corner, others)) = corners.split_first() else { return Ok(()) };
    let centre = mean(corner, others)?;
    for face in solid.faces {
        let points: Vec<Steps> = face.iter().filter_map(|&k| corners.get(k).copied()).collect();
        let Some((&first, rest)) = points.split_first() else { continue };
        let middle = mean(first, rest)?;
        let away = [0, 1, 2].map(|k| middle[k].checked_sub(centre[k]));
        let [Some(ax), Some(ay), Some(az)] = away else {
            return Err(PoseError::Overflow { point: middle });
        };
        for pair in rest.windows(2) {
            if let [b, c] = *pair {
                out.push(facing(first, b, c, [ax, ay, az]));
            }
        }
    }
    Ok(())
}

/// The mean of `first` and `rest`, floored.
fn mean(first: Steps, rest: &[Steps]) -> Result<Steps, PoseError> {
    let mut sum = first;
    for &point in rest {
        let added = [0, 1, 2].map(|k| sum[k].checked_add(point[k]));
        let [Some(x), Some(y), Some(z)] = added else { return Err(PoseError::Overflow { point }) };
        sum = [x, y, z];
    }
    let count = i64::try_from(rest.len()).ok().and_then(|n| n.checked_add(1));
    let count = count.ok_or(PoseError::Overflow { point: first })?;
    Ok(sum.map(|c| c.div_euclid(count)))
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
