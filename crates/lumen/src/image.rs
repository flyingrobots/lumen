//! The image lumen renders into: opaque RGBA8, row-major, row 0 at the top.
//!
//! This is lumen's own small type for now. Geordi's `RenderedImage` replaces it once lumen
//! renders Geordi scenes (flyingrobots/lumen#6), so it stays a plain byte buffer with a size.

use core::fmt;

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
}

/// Why an image size was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeError {
    /// The width or height is zero.
    Empty,
    /// The width or height exceeds [`MAX_DIMENSION`].
    DimensionTooLarge,
    /// The pixel count exceeds [`MAX_PIXELS`].
    TooManyPixels,
}

impl fmt::Display for SizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("image width and height must be at least 1"),
            Self::DimensionTooLarge => {
                write!(f, "image width and height must be at most {MAX_DIMENSION}")
            }
            Self::TooManyPixels => write!(f, "image must have at most {MAX_PIXELS} pixels"),
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
            return Err(SizeError::Empty);
        }
        if width > MAX_DIMENSION || height > MAX_DIMENSION {
            return Err(SizeError::DimensionTooLarge);
        }
        if u64::from(width) * u64::from(height) > MAX_PIXELS {
            return Err(SizeError::TooManyPixels);
        }
        Ok(Self { width, height })
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

    /// The buffer length, `width * height * 4` bytes; at most `2^29`, so it fits `usize` on
    /// every target with at least 32-bit pointers.
    #[must_use]
    pub fn byte_len(self) -> Option<usize> {
        usize::try_from(u64::from(self.width) * u64::from(self.height) * 4).ok()
    }
}

/// An opaque RGBA8 image: `width * height * 4` bytes, row-major, row 0 at the top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba8Image {
    size: ImageSize,
    bytes: Vec<u8>,
}

impl Rgba8Image {
    /// Wraps a buffer whose length is `size.byte_len()`, or returns `None`.
    #[must_use]
    pub fn from_bytes(size: ImageSize, bytes: Vec<u8>) -> Option<Self> {
        (Some(bytes.len()) == size.byte_len()).then_some(Self { size, bytes })
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
    /// let image = Rgba8Image::from_bytes(ImageSize::new(1, 1)?, vec![0, 0, 0, 255])
    ///     .ok_or("four bytes for one pixel")?;
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

    #[test]
    fn sizes_are_checked_in_order() {
        assert_eq!(ImageSize::new(0, 5), Err(SizeError::Empty));
        assert_eq!(ImageSize::new(5, 0), Err(SizeError::Empty));
        assert_eq!(ImageSize::new(32_769, 1), Err(SizeError::DimensionTooLarge));
        assert_eq!(ImageSize::new(1, 32_769), Err(SizeError::DimensionTooLarge));
        assert_eq!(ImageSize::new(32_768, 32_768), Err(SizeError::TooManyPixels));
        assert_eq!(ImageSize::new(16_384, 8_193), Err(SizeError::TooManyPixels));
        let largest = ImageSize::new(16_384, 8_192).expect("2^27 pixels is accepted");
        assert_eq!(largest.byte_len(), Some(1 << 29));
        assert!(ImageSize::new(15_360, 8_640).is_ok());
        assert!(ImageSize::new(32_768, 1).is_ok());
    }

    #[test]
    fn pixels_are_row_major_from_the_top_left() {
        let size = ImageSize::new(2, 3).expect("valid");
        let bytes: Vec<u8> = (0..24).collect();
        assert!(Rgba8Image::from_bytes(size, bytes[..20].to_vec()).is_none());
        let image = Rgba8Image::from_bytes(size, bytes).expect("right length");
        assert_eq!(image.pixel(0, 0), Some([0, 1, 2, 3]));
        assert_eq!(image.pixel(1, 0), Some([4, 5, 6, 7]));
        assert_eq!(image.pixel(0, 1), Some([8, 9, 10, 11]));
        assert_eq!(image.pixel(1, 2), Some([20, 21, 22, 23]));
        assert_eq!(image.pixel(2, 0), None);
        assert_eq!(image.pixel(0, 3), None);
    }
}
