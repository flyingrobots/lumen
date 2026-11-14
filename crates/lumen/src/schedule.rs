//! How a render is spread over threads: this module owns the checked thread count and tile
//! size ([`Schedule`], [`TileSize`]) and the tile geometry the workers follow.
//!
//! A schedule never changes a pixel. It decides only which worker computes which tile and when.

use core::fmt;

use crate::image::ImageSize;

/// The most worker threads a render takes: 256, well above any core count lumen is run on, and
/// low enough that a mistyped `--threads` cannot ask the system for thousands of threads.
pub const MAX_THREADS: u32 = 256;

/// The default tile: 32 by 32 pixels, chosen by measurement (README, "Performance").
pub const DEFAULT_TILE: TileSize = TileSize { width: 32, height: 32 };

// Pixel and tile counts are `u32` (an image holds at most 2^27 pixels) and byte offsets are
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
    /// A tile width or height is zero.
    EmptyTile,
}

impl fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoThreads => f.write_str("threads must be at least 1"),
            Self::TooManyThreads(n) => write!(f, "threads must be at most {MAX_THREADS}, not {n}"),
            Self::EmptyTile => f.write_str("tile width and height must be at least 1"),
        }
    }
}

impl std::error::Error for ScheduleError {}

/// A checked tile size: the rectangle of pixels one worker renders before taking the next.
///
/// Tiles cover the image in row-major order from the top left; the tiles in the last column and
/// the last row are cut to the image. A tile larger than the image is the whole image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileSize {
    width: u32,
    height: u32,
}

impl TileSize {
    /// A tile of `width` by `height` pixels.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::EmptyTile`] if either is zero.
    pub const fn new(width: u32, height: u32) -> Result<Self, ScheduleError> {
        if width == 0 || height == 0 {
            return Err(ScheduleError::EmptyTile);
        }
        Ok(Self { width, height })
    }

    /// The tile width.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    /// The tile height.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }
}

/// How a render is spread over threads: the number of worker threads and the tile size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Schedule {
    threads: u32,
    tile: TileSize,
}

impl Schedule {
    /// `threads` workers (`1..=MAX_THREADS`) taking tiles of `tile`.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::NoThreads`] for zero, [`ScheduleError::TooManyThreads`] above
    /// [`MAX_THREADS`].
    pub const fn new(threads: u32, tile: TileSize) -> Result<Self, ScheduleError> {
        if threads == 0 {
            return Err(ScheduleError::NoThreads);
        }
        if threads > MAX_THREADS {
            return Err(ScheduleError::TooManyThreads(threads));
        }
        Ok(Self { threads, tile })
    }

    /// One worker (the calling thread) and the default tile.
    #[must_use]
    pub const fn single() -> Self {
        Self { threads: 1, tile: DEFAULT_TILE }
    }

    /// The threads asked for.
    #[must_use]
    pub const fn threads(self) -> u32 {
        self.threads
    }

    /// The tile size.
    #[must_use]
    pub const fn tile(self) -> TileSize {
        self.tile
    }

    /// The same schedule with another tile size.
    #[must_use]
    pub const fn with_tile(self, tile: TileSize) -> Self {
        Self { threads: self.threads, tile }
    }

    /// The workers a render of `size` uses: the threads asked for, but never more than there
    /// are tiles, and at least one. The calling thread is one of them.
    #[must_use]
    pub fn workers(self, size: ImageSize) -> u32 {
        Tiles::new(size, self.tile)
            .map_or(self.threads, |tiles| self.threads.min(tiles.count()))
            .max(1)
    }
}

/// The tiles of an image, numbered row-major from the top left.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tiles {
    size: ImageSize,
    tile: TileSize,
    across: u32,
    count: u32,
}

/// One tile's pixels: columns `x0..x1` of rows `y0..y1`, never empty, inside the image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TileRect {
    pub(crate) x0: u32,
    pub(crate) y0: u32,
    pub(crate) x1: u32,
    pub(crate) y1: u32,
}

impl TileRect {
    /// The tile's width in pixels.
    pub(crate) const fn width(self) -> u32 {
        self.x1 - self.x0
    }
}

impl Tiles {
    /// The tiles of `size` in tiles of `tile`, or `None` if their count does not fit `u32`.
    ///
    /// `across * down` is at most the pixel count (each tile holds at least one pixel), which
    /// [`ImageSize`] bounds by 2^27, so `None` does not happen for a checked size.
    pub(crate) fn new(size: ImageSize, tile: TileSize) -> Option<Self> {
        let across = size.width().div_ceil(tile.width);
        let down = size.height().div_ceil(tile.height);
        let count = u32::try_from(u64::from(across) * u64::from(down)).ok()?;
        Some(Self { size, tile, across, count })
    }

    /// How many tiles there are.
    pub(crate) const fn count(self) -> u32 {
        self.count
    }

    /// The index of the last tile (an image has at least one).
    pub(crate) const fn last_index(self) -> u32 {
        self.count.saturating_sub(1)
    }

    /// The image size.
    pub(crate) const fn size(self) -> ImageSize {
        self.size
    }

    /// The bytes of the largest tile: the tile size cut to the image, four bytes a pixel, or
    /// `None` if that overflows `usize`.
    pub(crate) fn max_tile_bytes(self) -> Option<usize> {
        let width = widen(self.tile.width.min(self.size.width()));
        let height = widen(self.tile.height.min(self.size.height()));
        width.checked_mul(height)?.checked_mul(4)
    }

    /// Tile `index`, or `None` past the last tile.
    ///
    /// For a tile in column `c < across`, `c * tile.width < width`, so the products are checked
    /// but never fail; an end past `u32::MAX` is past the image, so it is cut to the image.
    pub(crate) fn rect(self, index: u32) -> Option<TileRect> {
        if index >= self.count {
            return None;
        }
        let x0 = (index % self.across).checked_mul(self.tile.width)?;
        let y0 = (index / self.across).checked_mul(self.tile.height)?;
        let (width, height) = (self.size.width(), self.size.height());
        Some(TileRect {
            x0,
            y0,
            x1: x0.checked_add(self.tile.width).map_or(width, |end| end.min(width)),
            y1: y0.checked_add(self.tile.height).map_or(height, |end| end.min(height)),
        })
    }

    /// The index of the last tile in the same row of tiles as tile `index`, at most the last
    /// tile. `index % across <= index` and `across >= 1`, so neither subtraction underflows.
    pub(crate) fn row_end(self, index: u32) -> u32 {
        let row_start = index - index % self.across;
        row_start
            .checked_add(self.across - 1)
            .map_or(self.last_index(), |end| end.min(self.last_index()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiles(width: u32, height: u32, tile_width: u32, tile_height: u32) -> Tiles {
        let size = ImageSize::new(width, height).expect("size");
        Tiles::new(size, TileSize::new(tile_width, tile_height).expect("tile")).expect("tiles")
    }

    /// Oracle: every pixel of the image lies in exactly one tile, and the tiles are in
    /// row-major order of their top-left corners (checked by counting, pixel by pixel).
    #[test]
    fn tiles_cover_every_pixel_once_in_row_major_order() {
        let cases = [(13, 9, 5, 4), (13, 9, 1, 1), (13, 9, 13, 9), (13, 9, 40, 40), (32, 32, 8, 8)];
        for (width, height, tw, th) in cases {
            let t = tiles(width, height, tw, th);
            let mut covered = vec![0_u32; widen(width * height)];
            let mut previous = None;
            for index in 0..t.count() {
                let rect = t.rect(index).expect("in range");
                assert!(rect.x0 < rect.x1 && rect.y0 < rect.y1, "{rect:?}");
                assert!(previous.is_none_or(|p: TileRect| (p.y0, p.x0) < (rect.y0, rect.x0)));
                previous = Some(rect);
                for y in rect.y0..rect.y1 {
                    for x in rect.x0..rect.x1 {
                        covered[widen(y * width + x)] += 1;
                    }
                }
                let end = t.rect(t.row_end(index)).expect("row end exists");
                assert_eq!((end.y0, end.x1), (rect.y0, width), "{index}");
            }
            assert!(covered.iter().all(|&n| n == 1), "{width}x{height} in {tw}x{th}");
            assert_eq!(t.rect(t.count()), None);
        }
    }
}
