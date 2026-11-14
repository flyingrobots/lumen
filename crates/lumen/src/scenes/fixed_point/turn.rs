//! Rational turns about `+Y`: rotations whose cosine and sine are fractions with a common
//! denominator, applied to points in grid steps with integers only.

use super::solids::Steps;

/// A rotation about `+Y` by the angle whose cosine is `cos / hyp` and sine `sin / hyp`, where
/// `cos^2 + sin^2 = hyp^2`: `(x, z)` goes to `((cos x - sin z) / hyp, (sin x + cos z) / hyp)`.
/// It takes `+X` toward `+Z` for a positive sine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RationalTurn {
    /// The cosine's numerator.
    pub cos: i64,
    /// The sine's numerator.
    pub sin: i64,
    /// The common denominator.
    pub hyp: i64,
}

impl RationalTurn {
    /// The two numerators of `p` turned about `pivot`, before division by `hyp`, as
    /// `(x, z)` offsets from the pivot; `None` on overflow.
    fn numerators(self, [x, _, z]: Steps, [px, _, pz]: Steps) -> Option<(i64, i64)> {
        let (dx, dz) = (x.checked_sub(px)?, z.checked_sub(pz)?);
        let turned_x = self.cos.checked_mul(dx)?.checked_sub(self.sin.checked_mul(dz)?)?;
        let turned_z = self.sin.checked_mul(dx)?.checked_add(self.cos.checked_mul(dz)?)?;
        Some((turned_x, turned_z))
    }

    /// `p` turned about the vertical line through `pivot`, exactly: `None` unless both new
    /// coordinates are whole grid steps (or on overflow).
    #[must_use]
    pub fn exact(self, p: Steps, pivot: Steps) -> Option<Steps> {
        let (x, z) = self.numerators(p, pivot)?;
        let whole =
            |n: i64| (n.checked_rem(self.hyp)? == 0).then(|| n.checked_div(self.hyp)).flatten();
        Some([whole(x)?.checked_add(pivot[0])?, p[1], whole(z)?.checked_add(pivot[2])?])
    }

    /// `p` turned about the vertical line through `pivot`, each new coordinate floored to a
    /// whole grid step; `None` on overflow or a zero `hyp`.
    #[must_use]
    pub fn floored(self, p: Steps, pivot: Steps) -> Option<Steps> {
        let (x, z) = self.numerators(p, pivot)?;
        let floor = |n: i64| n.checked_div_euclid(self.hyp);
        Some([floor(x)?.checked_add(pivot[0])?, p[1], floor(z)?.checked_add(pivot[2])?])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the 3-4-5 triangle: turning `(5, 0)` by the angle with cosine 4/5 and sine 3/5
    /// gives `(4, 3)`, and `(0, 5)` gives `(-3, 4)`.
    #[test]
    fn a_three_four_five_turn_is_exact_on_multiples_of_five() {
        let turn = RationalTurn { cos: 4, sin: 3, hyp: 5 };
        let pivot = [100, 7, -100];
        assert_eq!(turn.exact([105, 1, -100], pivot), Some([104, 1, -97]));
        assert_eq!(turn.exact([100, 1, -95], pivot), Some([97, 1, -96]));
        // (1, 0) turns to (0.8, 0.6): not whole steps, so `exact` refuses and `floored` floors.
        assert_eq!(turn.exact([101, 0, -100], pivot), None);
        assert_eq!(turn.floored([101, 0, -100], pivot), Some([100, 0, -100]));
        // (-1, 0) turns to (-0.8, -0.6), which floors to (-1, -1).
        assert_eq!(turn.floored([99, 0, -100], pivot), Some([99, 0, -101]));
        assert_eq!(RationalTurn { hyp: 0, ..turn }.floored([1, 0, 0], [0; 3]), None);
    }
}
