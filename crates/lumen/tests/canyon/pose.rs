//! The craft's rotations and poses: `Rational::new` admits exactly the rotations, `Pose::place`
//! turns and moves a point exactly, floored, and refuses a point it cannot pose.

use lumen::scenes::canyon::craft::CRAFT_POSES;
use lumen::scenes::canyon::{CanyonError, Pose, PoseError, Rational};

/// The 3-4-5 rotation, `cos = 4/5` and `sin = 3/5`.
fn three_four_five() -> Rational {
    Rational::new(4, 3, 5).expect("a Pythagorean triple")
}

/// Oracle: the rule `hyp > 0` and `cos^2 + sin^2 = hyp^2`, on triples worked by hand: the
/// 3-4-5 and 7-24-25 triangles and no turn are admitted with their numerators; a zero or
/// negative denominator, a near miss and squares outside `i64` are refused, naming the triple.
#[test]
fn a_rotation_is_admitted_only_with_its_pythagorean_rule() {
    let turn = three_four_five();
    assert_eq!((turn.cos(), turn.sin(), turn.hyp()), (4, 3, 5));
    assert_eq!(Rational::new(-24, 7, 25).map(Rational::sin), Ok(7));
    assert_eq!(Rational::new(1, 0, 1), Ok(Rational::NONE));
    for (cos, sin, hyp) in [(1, 0, 0), (0, 0, 0), (-1, 0, -1), (4, 3, 6), (i64::MAX, 0, i64::MAX)] {
        assert_eq!(Rational::new(cos, sin, hyp), Err(PoseError::NotARotation { cos, sin, hyp }));
    }
    let craft = [(24, -7), (24, 7), (24, 7), (1, 0), (24, -7), (24, -7)];
    let posed = CRAFT_POSES.iter().flat_map(|pose| [pose.bank, pose.heading]);
    for (turn, (cos, sin)) in posed.zip(craft) {
        assert_eq!(Ok(turn), Rational::new(cos, sin, if cos == 1 { 1 } else { 25 }));
    }
}

/// Oracle: the turns worked by hand. Banked by 3-4-5, `(5, 0)` goes to `(4, 3)` and `(-1, 0)`
/// to `(-0.8, -0.6)`, floored to `(-1, -1)`; headed by 3-4-5, `z = 5` goes to `z = 4`, `x = 3`.
/// Then the offset is added.
#[test]
fn a_point_is_turned_floored_and_moved() {
    let banked = Pose { bank: three_four_five(), heading: Rational::NONE, offset: [10, 20, 30] };
    assert_eq!(banked.place([5, 0, 7]), Ok([14, 23, 37]));
    assert_eq!(banked.place([-1, 0, 0]), Ok([9, 19, 30]));
    let headed = Pose { bank: Rational::NONE, heading: three_four_five(), offset: [0; 3] };
    assert_eq!(headed.place([0, 2, 5]), Ok([3, 2, 4]));
}

/// Oracle: `i64`'s range. A coordinate at `i64::MAX` poses unchanged with no turn and no
/// offset; one more step of offset, or a 3-4-5 turn's product `4 * i64::MAX`, is outside it, and
/// the refusal names the point. At `x = 2^61` only the cosine's product, `4 * 2^61 = 2^63`, is
/// outside (the sine's, `3 * 2^61`, fits), so that one product alone is checked.
#[test]
fn a_point_that_would_leave_i64_is_refused() {
    let edge = [i64::MAX, i64::MIN, 0];
    let still = Pose { bank: Rational::NONE, heading: Rational::NONE, offset: [0; 3] };
    assert_eq!(still.place(edge), Ok(edge));
    let refused = Err(PoseError::Overflow { point: edge });
    assert_eq!(Pose { offset: [1, 0, 0], ..still }.place(edge), refused);
    assert_eq!(Pose { offset: [0, -1, 0], ..still }.place(edge), refused);
    assert_eq!(Pose { bank: three_four_five(), ..still }.place(edge), refused);
    let far = [1 << 61, 0, 0];
    let banked = Pose { bank: three_four_five(), ..still };
    assert_eq!(banked.place(far), Err(PoseError::Overflow { point: far }));
    assert_eq!(
        Pose { heading: three_four_five(), ..still }.place([0, 0, i64::MAX]),
        Err(PoseError::Overflow { point: [0, 0, i64::MAX] })
    );
}

/// Oracle: the messages, written out.
#[test]
fn a_pose_refusal_says_what_was_given() {
    let not = PoseError::NotARotation { cos: 1, sin: 0, hyp: 0 };
    assert_eq!(
        not.to_string(),
        "(1, 0) over 0 is not a rotation: hyp must be positive and cos^2 + sin^2 equal hyp^2"
    );
    let overflow = PoseError::Overflow { point: [1, -2, 3] };
    assert_eq!(overflow.to_string(), "posing (1, -2, 3) overflows a grid coordinate");
    assert_eq!(
        CanyonError::Craft(overflow).to_string(),
        "a craft could not be posed: posing (1, -2, 3) overflows a grid coordinate"
    );
}
