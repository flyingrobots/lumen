//! The image lumen renders into: opaque RGBA8, row-major, row 0 at the top.
//!
//! This is lumen's own small type for now. Geordi's `RenderedImage` replaces it once lumen
//! renders Geordi scenes (flyingrobots/lumen#6), so it stays a plain byte buffer with a size.

use core::fmt;
use std::collections::TryReserveError;

use crate::schedule::widen;

/// The largest image width or height: Bunny's camera limit, `bunny_optics::MAX_IMAGE_DIMENSION`.
pub const MAX_DIMENSION: u32 = bunny_optics::MAX_IMAGE_DIMENSION;

/// The most pixels an image may hold: `2^27` (134 217 728), a 512 MiB RGBA8 buffer.
///
/// That covers a 16K image (15 360 by 8 640 is 132 710 400 pixels) and keeps the buffer
/// below what a developer machine can hold beside the scene, while refusing the 4 GiB a
/// 32 768 by 32 768 image would need before any memory is reserved.
pub const MAX_PIXELS: u64 = 1 << 27;

/// A checked image size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageSize {
    width: u32,
    height: u32,
    /// `width * height * 4`, at most `4 * MAX_PIXELS` (`2^29`): computed once, checked, by
    /// [`ImageSize::new`], so [`ImageSize::byte_len`] cannot fail.
    bytes: u32,
}

/// Why an image size was refused. Each variant carries the size asked for and the limit.
///
/// Non-exhaustive: a later version may refuse a size for a new reason, so a `match` outside
/// this crate needs a wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SizeError {
    /// The width or height is zero.
    Empty {
        /// The width asked for.
        width: u32,
        /// The height asked for.
        height: u32,
    },
    /// The width or height exceeds [`MAX_DIMENSION`].
    DimensionTooLarge {
        /// The width asked for.
        width: u32,
        /// The height asked for.
        height: u32,
        /// The largest width or height, [`MAX_DIMENSION`].
        limit: u32,
    },
    /// The pixel count exceeds [`MAX_PIXELS`].
    TooManyPixels {
        /// The pixel count asked for, `width * height`.
        pixels: u64,
        /// The most pixels, [`MAX_PIXELS`].
        limit: u64,
    },
}

impl fmt::Display for SizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty { .. } => f.write_str("image width and height must be at least 1"),
            Self::DimensionTooLarge { limit, .. } => {
                write!(f, "image width and height must be at most {limit}")
            }
            Self::TooManyPixels { limit, .. } => {
                write!(f, "image must have at most {limit} pixels")
            }
        }
    }
}

impl std::error::Error for SizeError {}

impl ImageSize {
    /// Checks a width and height against [`MAX_DIMENSION`] and [`MAX_PIXELS`].
    ///
    /// # Errors
    ///
    /// [`SizeError::Empty`], [`SizeError::DimensionTooLarge`] or [`SizeError::TooManyPixels`],
    /// checked in that order.
    pub fn new(width: u32, height: u32) -> Result<Self, SizeError> {
        if width == 0 || height == 0 {
            return Err(SizeError::Empty { width, height });
        }
        if width > MAX_DIMENSION || height > MAX_DIMENSION {
            return Err(SizeError::DimensionTooLarge { width, height, limit: MAX_DIMENSION });
        }
        // Two `u32` factors always fit a `u64` product, so saturating never saturates.
        let pixels = u64::from(width).saturating_mul(u64::from(height));
        let too_many = SizeError::TooManyPixels { pixels, limit: MAX_PIXELS };
        if pixels > MAX_PIXELS {
            return Err(too_many);
        }
        // At most 2^27 pixels of 4 bytes: 2^29, which fits a `u32`.
        let bytes = pixels.checked_mul(4).and_then(|n| u32::try_from(n).ok()).ok_or(too_many)?;
        Ok(Self { width, height, bytes })
    }

    /// The width in pixels.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    /// The height in pixels.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }

    /// The buffer length, `width * height * 4` bytes: at most `2^29`, computed and checked by
    /// [`ImageSize::new`].
    #[must_use]
    pub const fn byte_len(self) -> usize {
        widen(self.bytes)
    }
}

/// Why [`Rgba8Image::from_bytes`] refused a buffer: its length is not the size's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LengthError {
    /// The image size.
    pub size: ImageSize,
    /// The length the size needs, [`ImageSize::byte_len`].
    pub expected: usize,
    /// The length given.
    pub given: usize,
}

impl fmt::Display for LengthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { size, expected, given } = self;
        let (width, height) = (size.width, size.height);
        write!(f, "a {width}x{height} RGBA8 image needs {expected} bytes, not {given}")
    }
}

impl std::error::Error for LengthError {}

/// An opaque RGBA8 image: `width * height * 4` bytes, row-major, row 0 at the top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba8Image {
    size: ImageSize,
    bytes: Vec<u8>,
}

impl Rgba8Image {
    /// Wraps a buffer whose length is `size.byte_len()`.
    ///
    /// # Errors
    ///
    /// [`LengthError`], with both lengths, for a buffer of any other length.
    pub fn from_bytes(size: ImageSize, bytes: Vec<u8>) -> Result<Self, LengthError> {
        let (expected, given) = (size.byte_len(), bytes.len());
        if given != expected {
            return Err(LengthError { size, expected, given });
        }
        Ok(Self { size, bytes })
    }

    /// An all-zero image of `size`, reserved without aborting on failure: the buffer the
    /// workers render into, so it has the size's length by construction.
    pub(crate) fn zeroed(size: ImageSize) -> Result<Self, TryReserveError> {
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(size.byte_len())?;
        bytes.resize(size.byte_len(), 0);
        Ok(Self { size, bytes })
    }

    /// The RGBA8 bytes, to render into. The length cannot change.
    pub(crate) fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }

    /// The image size.
    #[must_use]
    pub const fn size(&self) -> ImageSize {
        self.size
    }

    /// The width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.size.width
    }

    /// The height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.size.height
    }

    /// The RGBA8 bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The four bytes of pixel `(x, y)`, or `None` outside the image.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.size.width || y >= self.size.height {
            return None;
        }
        let index = (usize::try_from(y).ok()? * usize::try_from(self.size.width).ok()?
            + usize::try_from(x).ok()?)
            * 4;
        self.bytes.get(index..index + 4)?.try_into().ok()
    }

    /// `sha256:` and the lowercase hex SHA-256 of the RGBA8 bytes: Geordi's image hash.
    ///
    /// # Examples
    ///
    /// One opaque black pixel, whose four bytes `00 00 00 ff` have a published SHA-256:
    ///
    /// ```
    /// use lumen::{ImageSize, Rgba8Image};
    ///
    /// let image = Rgba8Image::from_bytes(ImageSize::new(1, 1)?, vec![0, 0, 0, 255])?;
    /// assert_eq!(
    ///     image.hash(),
    ///     "sha256:e3820096cb82366b860b8a4e668453a7aaaf423af03bdf289fa308ea03a79332"
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn hash(&self) -> String {
        format!("sha256:{}", crate::sha256::hex(&crate::sha256::digest(&self.bytes)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the limits, by hand. Each refusal carries the size asked for and the limit.
    #[test]
    fn sizes_are_checked_in_order() {
        assert_eq!(ImageSize::new(0, 5), Err(SizeError::Empty { width: 0, height: 5 }));
        assert_eq!(ImageSize::new(5, 0), Err(SizeError::Empty { width: 5, height: 0 }));
        let large = |width, height| SizeError::DimensionTooLarge { width, height, limit: 32_768 };
        assert_eq!(ImageSize::new(32_769, 1), Err(large(32_769, 1)));
        assert_eq!(ImageSize::new(1, 32_769), Err(large(1, 32_769)));
        let many = |pixels| SizeError::TooManyPixels { pixels, limit: 1 << 27 };
        assert_eq!(ImageSize::new(32_768, 32_768), Err(many(1 << 30)));
        assert_eq!(ImageSize::new(16_384, 8_193), Err(many(16_384 * 8_193)));
        let largest = ImageSize::new(16_384, 8_192).expect("2^27 pixels is accepted");
        assert_eq!(largest.byte_len(), 1 << 29);
        assert_eq!(ImageSize::new(3, 5).expect("valid").byte_len(), 60);
        assert!(ImageSize::new(15_360, 8_640).is_ok());
        assert!(ImageSize::new(32_768, 1).is_ok());
    }

    #[test]
    fn pixels_are_row_major_from_the_top_left() {
        let size = ImageSize::new(2, 3).expect("valid");
        let bytes: Vec<u8> = (0..24).collect();
        let short = Rgba8Image::from_bytes(size, bytes[..20].to_vec());
        assert_eq!(short, Err(LengthError { size, expected: 24, given: 20 }));
        let refusal = short.expect_err("refused").to_string();
        assert_eq!(refusal, "a 2x3 RGBA8 image needs 24 bytes, not 20");
        let image = Rgba8Image::from_bytes(size, bytes).expect("right length");
        assert_eq!(image.pixel(0, 0), Some([0, 1, 2, 3]));
        assert_eq!(image.pixel(1, 0), Some([4, 5, 6, 7]));
        assert_eq!(image.pixel(0, 1), Some([8, 9, 10, 11]));
        assert_eq!(image.pixel(1, 2), Some([20, 21, 22, 23]));
        assert_eq!(image.pixel(2, 0), None);
        assert_eq!(image.pixel(0, 3), None);
    }
}
