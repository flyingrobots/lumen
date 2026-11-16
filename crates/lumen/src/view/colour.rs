//! The colours of the views: integer maps from a Bunny value to RGBA8, and nothing else.

use bunny_broadphase::TraversalCounts;
use bunny_linalg::FixedVec3;
use bunny_query::BarycentricNumerators;

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

/// The grey of the nearest depth, at distance zero.
const DEPTH_NEAR: i128 = 255;

/// The grey of the farthest depth, at the range and beyond: above black, so a far hit is never
/// read as a miss.
const DEPTH_FAR: i128 = 16;

/// The grey of a hit at `distance` (raw Q32.32, at least zero) over a depth range of `range` raw
/// units (positive): `255 - floor(239 * distance / range)`, the distance clamped to the range, so
/// 255 at the eye and 16 at the range and beyond.
pub(super) fn depth(distance: i64, range: i128) -> [u8; 4] {
    let range = range.max(1);
    let near = i128::from(distance).clamp(0, range);
    let span = DEPTH_NEAR - DEPTH_FAR;
    let grey = DEPTH_NEAR - span * near / range;
    let g = u8::try_from(grey.clamp(DEPTH_FAR, DEPTH_NEAR)).unwrap_or(u8::MAX);
    [g, g, g, 255]
}

/// The darkest an ID colour's channel is: 32, so no ID colour is black, a miss.
const ID_FLOOR: u64 = 32;

/// A colour from 64 hashed bits: the three low bytes, each mapped to `32 + byte * 224 / 256`.
pub(super) fn id(bits: u64) -> [u8; 4] {
    let channel = |shift: u32| {
        let byte = (bits >> shift) & 0xFF;
        u8::try_from(ID_FLOOR + byte * (256 - ID_FLOOR) / 256).unwrap_or(u8::MAX)
    };
    [channel(0), channel(8), channel(16), 255]
}

/// A wireframe edge: white.
const WIRE: [u8; 4] = [255, 255, 255, 255];

/// A wireframe face's fill: dark grey.
const FILL: [u8; 4] = [64, 64, 64, 255];

/// The edge band: a hit is on an edge when its smallest barycentric weight is below
/// `1 / WIRE_FRACTION` of the triangle's.
const WIRE_FRACTION: i128 = 32;

/// Whether the hit with exact barycentric numerators `n` is on an edge: its smallest weight's
/// numerator (`det - u - v`, `u` or `v`) times [`WIRE_FRACTION`] is below `det`. Exact: each is
/// below `2^117`, so the product is below `2^122`.
pub(super) fn is_edge(n: &BarycentricNumerators) -> bool {
    let (u, v, det) = (n.u(), n.v(), n.det());
    let smallest = (det - u - v).min(u).min(v);
    smallest * WIRE_FRACTION < det
}

/// The wireframe colour of a hit: white on an edge, dark grey inside.
pub(super) fn wire(n: &BarycentricNumerators) -> [u8; 4] {
    if is_edge(n) {
        WIRE
    } else {
        FILL
    }
}

/// The traversal work the heat ramp reaches white at: box tests plus primitive tests.
pub(super) const COST_WHITE: u64 = 512;

/// The camera ray's work (`box_tests + primitive_tests`) through the heat ramp: with
/// `t = min(cost, 512)`, `s = t / 128` and `f = 2 * (t % 128)`, black to blue (`s = 0`:
/// `(0, 0, f)`), blue to red (`(f, 0, 255 - f)`), red to yellow (`(255, f, 0)`), yellow to white
/// (`(255, 255, f)`), and white from 512 on.
pub(super) fn cost(counts: &TraversalCounts) -> [u8; 4] {
    let work = counts.box_tests.saturating_add(counts.primitive_tests).min(COST_WHITE);
    let byte = |n: u64| u8::try_from(n.min(255)).unwrap_or(u8::MAX);
    let f = byte(2 * (work % 128));
    match work / 128 {
        0 => [0, 0, f, 255],
        1 => [f, 0, 255 - f, 255],
        2 => [255, f, 0, 255],
        3 => [255, 255, f, 255],
        _ => [255, 255, 255, 255],
    }
}

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

    /// Oracle: `255 - floor(239 d / range)` by hand, clamped at both ends.
    #[test]
    fn depth_runs_from_white_at_the_eye_to_dark_grey_at_the_range() {
        let range = 8 * i128::from(ONE);
        assert_eq!(depth(0, range), [255, 255, 255, 255]);
        assert_eq!(depth(4 * ONE, range), [136, 136, 136, 255], "half: 255 - 119");
        assert_eq!(depth(8 * ONE, range), [16, 16, 16, 255]);
        assert_eq!(depth(80 * ONE, range), [16, 16, 16, 255], "beyond the range");
        assert_eq!(depth(1, range)[0], 255, "one raw unit floors to no change");
    }

    fn numerators(u: i128, v: i128, det: i128) -> BarycentricNumerators {
        BarycentricNumerators::new(u, v, det).expect("on the triangle")
    }

    /// Oracle: the rule `min * 32 < det` by hand, at its boundary on each weight.
    #[test]
    fn an_edge_is_a_weight_below_a_thirty_second() {
        let det = 3200;
        assert!(!is_edge(&numerators(1000, 1000, det)), "centre-ish: fill");
        assert!(!is_edge(&numerators(100, 1000, det)), "u = det / 32 exactly: fill");
        assert!(is_edge(&numerators(99, 1000, det)), "u one below: edge");
        assert!(is_edge(&numerators(1000, 99, det)), "v one below: edge");
        assert!(is_edge(&numerators(1000, 2101, det)), "w0 = 99: edge");
        assert!(!is_edge(&numerators(1000, 2100, det)), "w0 = 100: fill");
        assert_eq!(wire(&numerators(0, 0, det)), WIRE);
        assert_eq!(wire(&numerators(1000, 1000, det)), FILL);
    }

    fn work(boxes: u64, primitives: u64) -> [u8; 4] {
        let mut counts = TraversalCounts::default();
        (counts.box_tests, counts.primitive_tests) = (boxes, primitives);
        cost(&counts)
    }

    /// Oracle: the ramp by hand at each segment's start and inside it.
    #[test]
    fn the_cost_ramp_runs_black_blue_red_yellow_white() {
        assert_eq!(work(0, 0), [0, 0, 0, 255]);
        assert_eq!(work(1, 1), [0, 0, 4, 255]);
        assert_eq!(work(127, 0), [0, 0, 254, 255]);
        assert_eq!(work(100, 28), [0, 0, 255, 255], "128: blue");
        assert_eq!(work(200, 0), [144, 0, 111, 255], "200: f = 144");
        assert_eq!(work(256, 0), [255, 0, 0, 255], "256: red");
        assert_eq!(work(384, 0), [255, 255, 0, 255], "384: yellow");
        assert_eq!(work(511, 0), [255, 255, 254, 255]);
        assert_eq!(work(512, 0), [255, 255, 255, 255], "512: white");
        assert_eq!(work(90_000, 9), [255, 255, 255, 255], "beyond: white");
    }

    /// Oracle: `32 + byte * 224 / 256` per byte, by hand.
    #[test]
    fn id_colours_take_the_low_three_bytes_and_are_never_black() {
        assert_eq!(id(0), [32, 32, 32, 255]);
        assert_eq!(id(0x00FF_8001), [32, 144, 255, 255]);
        assert_eq!(id(0xFFFF_FFFF_FF00_0000), [32, 32, 32, 255], "only the low bytes");
    }
}
