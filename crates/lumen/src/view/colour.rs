//! The colours of the views: integer maps from a Bunny value to RGBA8, and nothing else.

use bunny_linalg::FixedVec3;

/// A pixel no ray hit: black, which no normal colour reaches.
pub(super) const MISS: [u8; 4] = [0, 0, 0, 255];

/// A shading normal Bunny could not form (the vertex normals cancel at the hit): magenta, which
/// no normal colour reaches (it would need the normal `(1, -1, 1)`).
pub(super) const NO_NORMAL: [u8; 4] = [255, 0, 255, 255];

/// The amplitude of a front face's normal: each component `c` in `[-1, 1]` becomes
/// `128 + round(127 c)`, in `1..=255`.
const FRONT: i128 = 127;

/// The amplitude of a back face's normal: `128 + round(63 c)`, in `65..=191`. A front face's
/// colour is 127 from grey along the normal and a back face's 63, so no colour is both.
const BACK: i128 = 63;

/// One component of a unit normal (raw Q32.32, at most `2^32 + 1` in magnitude) as a channel:
/// `128 + round(amplitude * raw / 2^32)`, rounded half up, clamped to a byte.
fn channel(raw: i64, amplitude: i128) -> u8 {
    let scaled = (i128::from(raw) * amplitude + (1 << 31)) >> 32;
    u8::try_from((scaled + 128).clamp(0, 255)).unwrap_or(u8::MAX)
}

/// The colour of `normal` at a hit on the triangle's front face or its back face: x as red, y as
/// green, z as blue.
pub(super) fn normal(normal: FixedVec3, front_face: bool) -> [u8; 4] {
    let amplitude = if front_face { FRONT } else { BACK };
    let c = |v: bunny_num::FixedQ32_32| channel(v.raw(), amplitude);
    [c(normal.x), c(normal.y), c(normal.z), 255]
}

#[cfg(test)]
mod tests {
    use bunny_num::FixedQ32_32;

    use super::*;

    const ONE: i64 = 1 << 32;

    fn v(x: i64, y: i64, z: i64) -> FixedVec3 {
        let r = FixedQ32_32::from_raw;
        FixedVec3::new(r(x), r(y), r(z))
    }

    /// Oracle: `128 + round(127 c)` and `128 + round(63 c)` by hand.
    #[test]
    fn axes_map_to_the_ends_of_their_ranges() {
        assert_eq!(normal(v(ONE, 0, 0), true), [255, 128, 128, 255]);
        assert_eq!(normal(v(0, -ONE, 0), true), [128, 1, 128, 255]);
        assert_eq!(normal(v(0, 0, ONE), false), [128, 128, 191, 255]);
        assert_eq!(normal(v(-ONE, 0, 0), false), [65, 128, 128, 255]);
        // One raw unit over 1 (Bunny's unit vectors may be) still maps to 255.
        assert_eq!(normal(v(ONE + 1, 0, 0), true)[0], 255);
        // Half: 63.5 rounds up to 64 (front), 31.5 to 32 (back).
        assert_eq!(normal(v(ONE / 2, 0, 0), true)[0], 192);
        assert_eq!(normal(v(ONE / 2, 0, 0), false)[0], 160);
    }
}
