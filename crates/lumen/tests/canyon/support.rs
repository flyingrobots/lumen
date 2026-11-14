//! Shared readings of the canyon in grid steps, and the terrain's heights, generated once for
//! the whole test binary.

use std::sync::OnceLock;

use bunny_linalg::FixedVec3;
use lumen::scenes::canyon::heightfield::Heightfield;
use lumen::scenes::canyon::terrain::canyon_heights;

/// One grid step in raw Q32.32 units.
pub(crate) const STEP_RAW: i64 = 1 << 16;

/// A point in grid steps.
pub(crate) type Steps = [i64; 3];

/// A vertex in grid steps; every scene coordinate is a whole number of steps.
pub(crate) fn steps(v: FixedVec3) -> Steps {
    [v.x, v.y, v.z].map(|c| {
        assert_eq!(c.raw() % STEP_RAW, 0, "{c:?} is off the 2^-16 grid");
        c.raw() / STEP_RAW
    })
}

/// The terrain's heights; they generate.
pub(crate) fn heights() -> &'static Heightfield {
    static HEIGHTS: OnceLock<Heightfield> = OnceLock::new();
    HEIGHTS.get_or_init(|| canyon_heights().expect("the canyon's heights generate"))
}
