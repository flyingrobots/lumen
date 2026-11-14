//! How a render is spread over threads: this module owns the checked thread count and band
//! height ([`Schedule`], [`BandRows`]) and the band geometry the workers follow.
//!
//! A schedule never changes a pixel. It decides only which worker computes which band and when.

use core::fmt;
use core::num::NonZeroU32;

use crate::image::ImageSize;

/// The most worker threads a render takes: 256, well above any core count lumen is run on, and
/// low enough that a mistyped `--threads` cannot ask the system for thousands of threads.
pub const MAX_THREADS: u32 = 256;

/// The default band: one row, chosen by measurement (README, "Performance").
pub const DEFAULT_BAND_ROWS: BandRows = BandRows(NonZeroU32::MIN);

// Pixel and band counts are `u32` (an image holds at most 2^27 pixels) and slice lengths are
// `usize`; widening one to the other is lossless on every target lumen builds for.
const _: () = assert!(usize::BITS >= u32::BITS);

/// `value` as a `usize`: lossless, by the assertion above.
#[allow(clippy::as_conversions, reason = "u32 to usize is lossless: usize has at least 32 bits")]
pub(crate) const fn widen(value: u32) -> usize {
    value as usize
}

/// Why a schedule was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleError {
    /// Zero threads were asked for.
    NoThreads,
    /// More than [`MAX_THREADS`] threads were asked for.
    TooManyThreads(u32),
    /// A band of zero rows was asked for.
    EmptyBand,
}

impl fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoThreads => f.write_str("threads must be at least 1"),
            Self::TooManyThreads(n) => write!(f, "threads must be at most {MAX_THREADS}, not {n}"),
            Self::EmptyBand => f.write_str("a band must be at least 1 row"),
        }
    }
}

impl std::error::Error for ScheduleError {}

/// A checked band height: the number of whole image rows one worker renders before taking the
/// next band.
///
/// Bands cover the image from the top, each as wide as the image; the last band is cut to the
/// image. A band taller than the image is the whole image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BandRows(NonZeroU32);

impl BandRows {
    /// Bands of `rows` rows.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::EmptyBand`] if `rows` is zero.
    pub const fn new(rows: u32) -> Result<Self, ScheduleError> {
        match NonZeroU32::new(rows) {
            Some(rows) => Ok(Self(rows)),
            None => Err(ScheduleError::EmptyBand),
        }
    }

    /// The rows in a band.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }

    /// How many bands an image of `size` has: its height divided by the band height, rounded up.
    #[must_use]
    pub const fn count(self, size: ImageSize) -> u32 {
        size.height().div_ceil(self.0.get())
    }

    /// The pixels in a full band of an image `width` pixels wide (at least one), saturated to
    /// `usize::MAX`: a band longer than the image is the whole image.
    pub(crate) fn pixels(self, width: usize) -> usize {
        width.max(1).saturating_mul(widen(self.0.get()))
    }
}

/// How a render is spread over threads: the number of worker threads and the band height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Schedule {
    threads: u32,
    band: BandRows,
}

impl Schedule {
    /// `threads` workers (`1..=MAX_THREADS`) taking bands of `band` rows.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::NoThreads`] for zero, [`ScheduleError::TooManyThreads`] above
    /// [`MAX_THREADS`].
    pub const fn new(threads: u32, band: BandRows) -> Result<Self, ScheduleError> {
        if threads == 0 {
            return Err(ScheduleError::NoThreads);
        }
        if threads > MAX_THREADS {
            return Err(ScheduleError::TooManyThreads(threads));
        }
        Ok(Self { threads, band })
    }

    /// One worker (the calling thread) and the default band.
    #[must_use]
    pub const fn single() -> Self {
        Self { threads: 1, band: DEFAULT_BAND_ROWS }
    }

    /// The threads asked for.
    #[must_use]
    pub const fn threads(self) -> u32 {
        self.threads
    }

    /// The band height.
    #[must_use]
    pub const fn band(self) -> BandRows {
        self.band
    }

    /// The same schedule with another band height.
    #[must_use]
    pub const fn with_band(self, band: BandRows) -> Self {
        Self { threads: self.threads, band }
    }

    /// The workers a render of `size` uses: the threads asked for, but never more than there
    /// are bands (an image has at least one). The calling thread is one of them.
    #[must_use]
    pub fn workers(self, size: ImageSize) -> u32 {
        self.threads.min(self.band.count(size)).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: counting by hand. The band count is the height over the band height, rounded up,
    /// and a band's pixel count saturates instead of wrapping.
    #[test]
    fn bands_are_counted_by_rounding_up() {
        let size = |w, h| ImageSize::new(w, h).expect("size");
        let rows = |n| BandRows::new(n).expect("rows");
        for (height, band, want) in [(9, 1, 9), (9, 2, 5), (9, 3, 3), (9, 9, 1), (9, 40, 1)] {
            assert_eq!(rows(band).count(size(13, height)), want, "{height} rows in {band}");
        }
        assert_eq!(rows(4).pixels(13), 52);
        assert_eq!(rows(u32::MAX).pixels(usize::MAX), usize::MAX);
        assert_eq!(BandRows::new(0), Err(ScheduleError::EmptyBand));
    }
}
