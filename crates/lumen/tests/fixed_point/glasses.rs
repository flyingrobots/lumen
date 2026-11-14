//! The sunglasses' fit: each lens sits just in front of the bunny's face, and no part of the
//! glasses touches the bunny.

use std::collections::BTreeSet;

use bunny_linalg::FixedVec3;
use bunny_num::fixed_q32_32::ONE_RAW;
use bunny_num::FixedQ32_32;
use bunny_query::{TraceRay, TraceTriangle};
use lumen::scene::FAR;
use lumen::scenes::fixed_point::player::BUNNY_TURN;
use lumen::scenes::fixed_point::sunglasses::LENS_MAX_GAP;
use lumen::scenes::fixed_point::{PartKind, Side};

use crate::support::{content, orient, part, Steps, STEP_RAW};

const GLASSES: [PartKind; 5] = [
    PartKind::Lens(Side::Left),
    PartKind::Lens(Side::Right),
    PartKind::Bridge,
    PartKind::Arm(Side::Left),
    PartKind::Arm(Side::Right),
];

fn point(p: Steps) -> FixedVec3 {
    let [x, y, z] = p.map(|c| FixedQ32_32::from_raw(c * STEP_RAW));
    FixedVec3::new(x, y, z)
}

/// The bunny's facing direction divided by 16: it faced `+Z` before its turn, which takes
/// `(0, 1)` in X and Z to `(-sin, cos) / hyp`. Its length is `hyp / 16`.
fn facing_16(sign: i64) -> FixedVec3 {
    let sixteenth = |n: i64| FixedQ32_32::from_raw(sign * n * (ONE_RAW / 16));
    FixedVec3::new(sixteenth(-BUNNY_TURN.sin), FixedQ32_32::ZERO, sixteenth(BUNNY_TURN.cos))
}

/// The nearest bunny triangle a ray from `from` along `direction` hits, as a distance in
/// raw units of `2^-32` units of length.
fn nearest_bunny(bunny: &[TraceTriangle], from: Steps, direction: FixedVec3) -> Option<i128> {
    let ray = TraceRay::new(point(from), direction).expect("a valid ray");
    let t = bunny
        .iter()
        .filter_map(|tri| ray.hit_triangle(tri, FixedQ32_32::ZERO, FAR))
        .map(|h| h.distance)
        .min()?;
    // The direction is `hyp / 16` units long.
    Some(i128::from(t.raw()) * i128::from(BUNNY_TURN.hyp) / 16)
}

/// Oracle: the fit stated by the scene, measured on the mesh with Bunny's exact ray test:
/// from each lens's centre (the vertex shared by all its fan's triangles), looking back along
/// the bunny's facing direction, the face is behind the lens at a distance in
/// `0..=LENS_MAX_GAP`; and from every lens vertex, looking forward, there is no bunny (the
/// lens is not inside the head).
#[test]
fn each_lens_sits_just_in_front_of_the_face() {
    let content = content();
    let bunny: Vec<TraceTriangle> = content
        .part(PartKind::Bunny)
        .iter()
        .map(|&[a, b, c]| TraceTriangle::new(a, b, c).expect("in the domain"))
        .collect();
    let limit = i128::from(LENS_MAX_GAP) * i128::from(STEP_RAW);
    for side in [Side::Left, Side::Right] {
        let lens = part(&content, PartKind::Lens(side));
        let mut shared: BTreeSet<Steps> = lens[0].iter().copied().collect();
        for t in &lens {
            shared = shared.intersection(&t.iter().copied().collect()).copied().collect();
        }
        let centre = *shared.first().expect("a fan has a centre");
        assert_eq!(shared.len(), 1, "{side:?}");
        let gap = nearest_bunny(&bunny, centre, facing_16(-1)).expect("the face is behind");
        assert!(0 < gap && gap <= limit, "{side:?}: gap {gap} raw, limit {limit}");
        let rim: BTreeSet<Steps> =
            lens.iter().flatten().copied().filter(|&p| p != centre).collect();
        assert_eq!(rim.len(), 8, "{side:?}");
        for p in rim {
            assert_eq!(nearest_bunny(&bunny, p, facing_16(1)), None, "{side:?}: {p:?} is inside");
        }
    }
}

/// Whether the segment `p..q` and the triangle `t` share a point (touching counts), exactly;
/// a segment in the triangle's plane counts as touching.
fn crosses(p: Steps, q: Steps, t: &[Steps; 3]) -> bool {
    let (sp, sq) = (orient(t[0], t[1], t[2], p), orient(t[0], t[1], t[2], q));
    if (sp > 0 && sq > 0) || (sp < 0 && sq < 0) {
        return false;
    }
    if sp == 0 && sq == 0 {
        return true;
    }
    let s = [orient(p, q, t[0], t[1]), orient(p, q, t[1], t[2]), orient(p, q, t[2], t[0])];
    s.iter().all(|&v| v >= 0) || s.iter().all(|&v| v <= 0)
}

fn bounds(t: &[Steps; 3]) -> [[i64; 3]; 2] {
    let lo = [0, 1, 2].map(|i| t.iter().map(|p| p[i]).min().unwrap_or(0));
    let hi = [0, 1, 2].map(|i| t.iter().map(|p| p[i]).max().unwrap_or(0));
    [lo, hi]
}

/// Oracle: two triangles share a point exactly when an edge of one meets the other, tested
/// with exact integer orientations (a test-side predicate, not Bunny's ray test).
#[test]
fn no_part_of_the_glasses_touches_the_bunny() {
    let content = content();
    let bunny = part(&content, PartKind::Bunny);
    let mut compared = 0_u64;
    for kind in GLASSES {
        for g in part(&content, kind) {
            let [glo, ghi] = bounds(&g);
            for b in &bunny {
                let [blo, bhi] = bounds(b);
                if (0..3).any(|i| ghi[i] < blo[i] || bhi[i] < glo[i]) {
                    continue;
                }
                compared += 1;
                let edges = |t: &[Steps; 3]| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])];
                let touch = edges(&g).iter().any(|&(p, q)| crosses(p, q, b))
                    || edges(b).iter().any(|&(p, q)| crosses(p, q, &g));
                assert!(!touch, "{kind:?} {g:?} touches the bunny's {b:?}");
            }
        }
    }
    assert!(compared > 50, "only {compared} nearby pairs were compared");
}
