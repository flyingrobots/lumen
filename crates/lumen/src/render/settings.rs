//! The checked render settings, [`RenderSettings`], and why settings were refused,
//! [`SettingsError`].

use core::fmt;

use bunny_optics::{PixelSampler, MAX_SAMPLES_PER_PIXEL};

/// Checked render settings: the sample count per pixel and the seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderSettings {
    samples_per_pixel: u32,
    seed: u32,
}

/// Why render settings were refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsError {
    /// The sample count is not a power of two (zero included). Bunny's sampler states its
    /// stratification for aligned power-of-two blocks only.
    SamplesNotPowerOfTwo(u32),
    /// The sample count is above Bunny's `MAX_SAMPLES_PER_PIXEL` (1024).
    TooManySamples(u32),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SamplesNotPowerOfTwo(n) => {
                write!(f, "samples per pixel must be a power of two (1, 2, 4, ..., 1024), not {n}")
            }
            Self::TooManySamples(n) => {
                write!(f, "samples per pixel must be at most {MAX_SAMPLES_PER_PIXEL}, not {n}")
            }
        }
    }
}

impl std::error::Error for SettingsError {}

impl RenderSettings {
    /// Settings with `samples_per_pixel` samples, a power of two in `1..=1024`, and `seed`.
    ///
    /// The seed is a `u32`, the domain of Geordi's render settings; Bunny's sampler widens it
    /// to `u64`.
    ///
    /// # Errors
    ///
    /// [`SettingsError::TooManySamples`] above 1024, then
    /// [`SettingsError::SamplesNotPowerOfTwo`] for zero or any other count that is not a power
    /// of two.
    pub const fn new(samples_per_pixel: u32, seed: u32) -> Result<Self, SettingsError> {
        if samples_per_pixel > MAX_SAMPLES_PER_PIXEL {
            return Err(SettingsError::TooManySamples(samples_per_pixel));
        }
        if !samples_per_pixel.is_power_of_two() {
            return Err(SettingsError::SamplesNotPowerOfTwo(samples_per_pixel));
        }
        Ok(Self { samples_per_pixel, seed })
    }

    /// The samples per pixel.
    #[must_use]
    pub const fn samples_per_pixel(self) -> u32 {
        self.samples_per_pixel
    }

    /// The seed.
    #[must_use]
    pub const fn seed(self) -> u32 {
        self.seed
    }

    /// Bunny's sampler for pixel `(x, y)` under this seed.
    #[must_use]
    pub fn sampler(self, x: u32, y: u32) -> PixelSampler {
        PixelSampler::new(u64::from(self.seed), x, y)
    }
}
