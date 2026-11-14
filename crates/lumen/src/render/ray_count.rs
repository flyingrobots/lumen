//! How many rays a path or a render traced, by query: [`RayCount`].

use core::ops::Add;

/// How many rays were traced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RayCount {
    /// Closest-hit queries: camera rays and bounce rays.
    pub closest: u64,
    /// Any-hit queries: shadow rays.
    pub shadow: u64,
}

impl RayCount {
    /// Every ray traced.
    #[must_use]
    pub const fn total(self) -> u64 {
        // At most 2^39 rays in all (see `Add`), so the sum never saturates.
        self.closest.saturating_add(self.shadow)
    }
}

impl Add for RayCount {
    type Output = Self;

    /// The rays of both. Ray counts of a render stay far below `u64::MAX` (an image has at most
    /// 2^27 pixels of at most 1024 samples of at most four rays: the camera ray, the bounce ray
    /// and a shadow ray at each of their hits, so at most 2^39 rays), so the sums never
    /// saturate.
    fn add(self, other: Self) -> Self {
        Self {
            closest: self.closest.saturating_add(other.closest),
            shadow: self.shadow.saturating_add(other.shadow),
        }
    }
}
