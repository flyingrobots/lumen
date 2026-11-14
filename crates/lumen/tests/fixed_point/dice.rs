//! The dice: four pips on every face of both, and the two congruent.

use std::collections::BTreeMap;

use lumen::scenes::fixed_point::dice::{DiePose, DICE, DIE_HALF, PIP_OFFSET, PIP_RISE};
use lumen::scenes::fixed_point::{Die, PartKind};

use crate::support::{content, orient, part, Steps};

/// The six outward face axes of a die turned by `pose`, scaled by the turn's denominator:
/// `(axis, sign)` with the axis in whole numbers.
fn face_axes(pose: &DiePose) -> [[i64; 3]; 6] {
    let (cos, sin, hyp) = (pose.turn.cos, pose.turn.sin, pose.turn.hyp);
    let (east, up, south) = ([cos, 0, sin], [0, hyp, 0], [-sin, 0, cos]);
    let neg = |axis: [i64; 3]| axis.map(|v| -v);
    [east, neg(east), up, neg(up), south, neg(south)]
}

/// `(p - centre) . axis`, which is `hyp` times the distance along the axis.
fn along(p: Steps, centre: Steps, axis: [i64; 3]) -> i64 {
    (0..3).map(|i| (p[i] - centre[i]) * axis[i]).sum()
}

/// For each face of `die`, the pips found on it, each as its centre's two face coordinates:
/// a pip is a set of top triangles (all three vertices exactly `DIE_HALF + PIP_RISE` out along
/// the face's axis) sharing one vertex, the centre of its fan.
fn pips_by_face(die: Die) -> BTreeMap<usize, Vec<(i64, i64)>> {
    let (_, pose) = DICE.iter().find(|(d, _)| *d == die).expect("the die exists");
    let axes = face_axes(pose);
    let top = (DIE_HALF + PIP_RISE) * pose.turn.hyp;
    let mut centres: BTreeMap<usize, BTreeMap<Steps, usize>> = BTreeMap::new();
    for triangle in part(&content(), PartKind::Pips(die)) {
        for (face, &axis) in axes.iter().enumerate() {
            if triangle.iter().all(|&p| along(p, pose.centre, axis) == top) {
                for vertex in triangle {
                    *centres.entry(face).or_default().entry(vertex).or_default() += 1;
                }
            }
        }
    }
    // A fan's centre is in every one of its 12 triangles; its rim vertices in two.
    let mut found = BTreeMap::new();
    for (face, counts) in centres {
        let (u_axis, v_axis) = (axes[(face + 2) % 6], axes[(face + 4) % 6]);
        let pips = counts
            .into_iter()
            .filter(|&(_, n)| n == 12)
            .map(|(p, _)| (along(p, pose.centre, u_axis), along(p, pose.centre, v_axis)));
        found.insert(face, pips.collect());
    }
    found
}

/// Oracle: the rule of the scene, four pips on every face, in the pattern of a die's four
/// (one pip near each corner, `PIP_OFFSET` from the centre on both face axes), counted from
/// the triangles alone.
#[test]
fn every_face_of_both_dice_has_exactly_four_pips() {
    for die in [Die::Left, Die::Right] {
        let found = pips_by_face(die);
        assert_eq!(found.len(), 6, "{die:?}: pips on {} faces", found.len());
        let (_, pose) = DICE.iter().find(|(d, _)| *d == die).expect("the die exists");
        let o = PIP_OFFSET * pose.turn.hyp;
        for (face, pips) in &found {
            let mut pips = pips.clone();
            pips.sort_unstable();
            assert_eq!(pips, vec![(-o, -o), (-o, o), (o, -o), (o, o)], "{die:?} face {face}");
        }
        let total: usize = found.values().map(Vec::len).sum();
        assert_eq!(total, 24, "{die:?}");
    }
}

/// Every vertex of both dice (body then pips), in primitive order.
fn vertices(die: Die) -> Vec<Steps> {
    let content = content();
    let mut all = part(&content, PartKind::DieBody(die));
    all.extend(part(&content, PartKind::Pips(die)));
    all.into_iter().flatten().collect()
}

/// Oracle: the motion stated by the two poses, from the left die to the right one, written
/// out in integers: `p_right - c_right = R(right) R(left)^-1 (p_left - c_left)`, where the
/// combined turn has cosine `(cR cL + sR sL)` and sine `(sR cL - cR sL)` over `hR hL`.
#[test]
fn the_right_die_is_the_left_die_under_the_stated_rigid_motion() {
    let ((_, left), (_, right)) = (DICE[0], DICE[1]);
    let (cl, sl, hl) = (left.turn.cos, left.turn.sin, left.turn.hyp);
    let (cr, sr, hr) = (right.turn.cos, right.turn.sin, right.turn.hyp);
    let (cos, sin, hyp) = (cr * cl + sr * sl, sr * cl - cr * sl, hr * hl);
    let (a, b) = (vertices(Die::Left), vertices(Die::Right));
    assert_eq!(a.len(), b.len());
    assert_eq!(a.len(), (44 + 24 * 36) * 3);
    for (p, q) in a.iter().zip(&b) {
        let (dx, dy, dz) = (p[0] - left.centre[0], p[1] - left.centre[1], p[2] - left.centre[2]);
        let moved = [cos * dx - sin * dz, hyp * dy, sin * dx + cos * dz];
        let target = [0, 1, 2].map(|i| hyp * (q[i] - right.centre[i]));
        assert_eq!(moved, target, "{p:?} -> {q:?}");
    }
}

/// Oracle: congruence itself, independent of the stated poses: every pair of corresponding
/// vertices is the same squared distance apart in both dice, and every triangle has the same
/// orientation against the first vertex (no mirror image).
#[test]
fn the_two_dice_are_congruent_and_not_mirrored() {
    let (a, b) = (vertices(Die::Left), vertices(Die::Right));
    let distance =
        |p: Steps, q: Steps| -> i128 { (0..3).map(|i| i128::from(p[i] - q[i]).pow(2)).sum() };
    let mut pairs = 0_u64;
    for i in (0..a.len()).step_by(3) {
        for j in (i + 1..a.len()).step_by(7) {
            assert_eq!(distance(a[i], a[j]), distance(b[i], b[j]), "vertices {i} and {j}");
            pairs += 1;
        }
    }
    assert!(pairs > 100_000, "{pairs} pairs compared");
    for t in (0..a.len()).step_by(3) {
        let left = orient(a[t], a[t + 1], a[t + 2], a[0]);
        let right = orient(b[t], b[t + 1], b[t + 2], b[0]);
        assert_eq!(left, right, "triangle {}", t / 3);
    }
}
