//! A seeded generator for the generated tests (testing standard rule 5).
//!
//! `SplitMix64` (Steele, Lea and Flood, "Fast splittable pseudorandom number generators", 2014),
//! chosen because it is eight lines, has no dependency and gives the same stream on every
//! target, including 32-bit ones. Every generated test runs a fixed seed, derives one seed per
//! case from it and names both in its failure messages, so a failing case replays alone with
//! `Gen::new(case_seed)`.
#![allow(dead_code, reason = "each test crate that includes this file uses some of it")]

/// A deterministic stream of pseudo-random numbers.
pub(crate) struct Gen(u64);

impl Gen {
    pub(crate) const fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// The seed of case `case` of a test with seed `seed`.
    pub(crate) fn case_seed(seed: u64, case: u64) -> u64 {
        Self::new(seed ^ case.wrapping_mul(0xD1B5_4A32_D192_ED03)).next_u64()
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n` (`n > 0`); the modulo bias is below 2^-40 for the small `n` used.
    pub(crate) fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "an empty range");
        self.next_u64() % n
    }

    /// A number in `low..=high`.
    pub(crate) fn between(&mut self, low: u64, high: u64) -> u64 {
        low + self.below(high - low + 1)
    }

    /// `len` random bytes.
    pub(crate) fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next_u64().to_le_bytes()[0]).collect()
    }

    /// A uniformly random permutation (Fisher-Yates).
    pub(crate) fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = usize::try_from(self.below(i as u64 + 1)).expect("a small index");
            items.swap(i, j);
        }
    }
}
