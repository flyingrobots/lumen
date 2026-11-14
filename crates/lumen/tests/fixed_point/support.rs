//! Shared readings of the scene's content: triangles in grid steps, by part.

use bunny_linalg::FixedVec3;
use lumen::scenes::fixed_point::{fixed_point_content, FixedPointContent, PartKind};

/// One grid step in raw Q32.32 units.
pub(crate) const STEP_RAW: i64 = 1 << 16;

/// A point in grid steps.
pub(crate) type Steps = [i64; 3];

/// The scene's content; it builds.
pub(crate) fn content() -> FixedPointContent {
    fixed_point_content().expect("the Fixed Point content builds")
}

/// A placed vertex in grid steps; every scene coordinate is a whole number of steps.
pub(crate) fn steps(v: FixedVec3) -> Steps {
    [v.x, v.y, v.z].map(|c| {
        assert_eq!(c.raw() % STEP_RAW, 0, "{c:?} is off the 2^-16 grid");
        c.raw() / STEP_RAW
    })
}

/// The triangles of the part `kind`, in grid steps. The part exists and is not empty.
pub(crate) fn part(content: &FixedPointContent, kind: PartKind) -> Vec<[Steps; 3]> {
    let triangles = content.part(kind);
    assert!(!triangles.is_empty(), "{kind:?} has no triangles");
    triangles.iter().map(|t| t.map(steps)).collect()
}

/// The exact orientation of `d` against the plane through `a`, `b`, `c`: the determinant of
/// `(b - a, c - a, d - a)`.
pub(crate) fn orient(a: Steps, b: Steps, c: Steps, d: Steps) -> i128 {
    let minus = |from: Steps, to: Steps| [0, 1, 2].map(|i| i128::from(from[i] - to[i]));
    let (ab, ac, ad) = (minus(b, a), minus(c, a), minus(d, a));
    ab[0] * (ac[1] * ad[2] - ac[2] * ad[1]) - ab[1] * (ac[0] * ad[2] - ac[2] * ad[0])
        + ab[2] * (ac[0] * ad[1] - ac[1] * ad[0])
}
