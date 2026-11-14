//! A PNG encoder for opaque RGBA8 images, with stored (uncompressed) deflate blocks.
//!
//! The PNG is a container for the RGBA bytes, not a contract value: the image hash is the
//! SHA-256 of the raw RGBA bytes. Storing instead of compressing keeps the encoder short,
//! dependency-free and byte-deterministic by construction, at the cost of file size (the
//! file is the raw bytes plus one filter byte per row and about 0.01 % framing).
//!
//! Layout: the signature; `IHDR` (8-bit RGBA, no interlace); one `IDAT` chunk per stored
//! block of at most 65 535 bytes, the first starting with the zlib header `78 01` and the
//! last ending with the Adler-32 of the uncompressed data; `IEND`. Every row has filter
//! type 0 (none). The encoder streams: it holds one block, never a second copy of the image.

use std::io::{self, Write};

use crate::image::Rgba8Image;

/// The PNG file signature.
pub(crate) const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// The largest stored deflate block, in bytes.
const MAX_STORED: usize = 65_535;

/// The zlib header: deflate with a 32 KiB window (`0x78`), no dictionary, fastest level
/// (`0x01`); `0x7801` is a multiple of 31 as the header check requires.
const ZLIB_HEADER: [u8; 2] = [0x78, 0x01];

/// The CRC-32 table of PNG (ISO 3309 / ITU-T V.42, reflected polynomial `0xEDB88320`).
const CRC_TABLE: [u32; 256] = crc_table();

#[allow(clippy::cast_possible_truncation, reason = "n < 256")]
const fn crc_table() -> [u32; 256] {
    let mut table = [0_u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 == 1 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
}

/// A running CRC-32 as PNG chunks use it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Crc32(u32);

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

impl Crc32 {
    /// The CRC of no bytes.
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self(0xFFFF_FFFF)
    }

    /// Adds bytes.
    #[allow(clippy::cast_possible_truncation, reason = "the low byte of the CRC indexes the table")]
    pub(crate) fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = CRC_TABLE[usize::from((self.0 as u8) ^ byte)] ^ (self.0 >> 8);
        }
    }

    /// The CRC of every byte added.
    #[must_use]
    pub(crate) const fn value(self) -> u32 {
        self.0 ^ 0xFFFF_FFFF
    }
}

/// A running Adler-32 as zlib streams use it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Adler32 {
    a: u32,
    b: u32,
}

impl Default for Adler32 {
    fn default() -> Self {
        Self::new()
    }
}

impl Adler32 {
    const MOD: u32 = 65_521;

    /// The checksum of no bytes.
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self { a: 1, b: 0 }
    }

    /// Adds bytes.
    pub(crate) fn update(&mut self, bytes: &[u8]) {
        // 5552 bytes is the longest run whose sums cannot overflow u32 before reduction.
        for run in bytes.chunks(5552) {
            for &byte in run {
                self.a += u32::from(byte);
                self.b += self.a;
            }
            self.a %= Self::MOD;
            self.b %= Self::MOD;
        }
    }

    /// The checksum of every byte added.
    #[must_use]
    pub(crate) const fn value(self) -> u32 {
        (self.b << 16) | self.a
    }
}

/// Writes one chunk: length, type, data, CRC of type and data.
fn write_chunk<W: Write>(out: &mut W, kind: [u8; 4], data: &[u8]) -> io::Result<()> {
    let length = u32::try_from(data.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "PNG chunk too long"))?;
    let mut crc = Crc32::new();
    crc.update(&kind);
    crc.update(data);
    out.write_all(&length.to_be_bytes())?;
    out.write_all(&kind)?;
    out.write_all(data)?;
    out.write_all(&crc.value().to_be_bytes())
}

/// Collects the zlib stream into stored blocks, one `IDAT` chunk each.
struct StoredBlocks<'w, W: Write> {
    out: &'w mut W,
    block: Vec<u8>,
    chunk: Vec<u8>,
    adler: Adler32,
    remaining: u64,
    first: bool,
}

impl<W: Write> StoredBlocks<'_, W> {
    fn push(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        self.adler.update(bytes);
        while !bytes.is_empty() {
            let take = (MAX_STORED - self.block.len()).min(bytes.len());
            let (head, rest) = bytes.split_at(take);
            self.block.extend_from_slice(head);
            bytes = rest;
            if self.block.len() == MAX_STORED {
                self.flush()?;
            }
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        let len = u16::try_from(self.block.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "stored block too long"))?;
        self.remaining = self.remaining.saturating_sub(u64::from(len));
        let last = self.remaining == 0;
        self.chunk.clear();
        if self.first {
            self.chunk.extend_from_slice(&ZLIB_HEADER);
            self.first = false;
        }
        self.chunk.push(u8::from(last));
        self.chunk.extend_from_slice(&len.to_le_bytes());
        self.chunk.extend_from_slice(&(!len).to_le_bytes());
        self.chunk.extend_from_slice(&self.block);
        if last {
            self.chunk.extend_from_slice(&self.adler.value().to_be_bytes());
        }
        self.block.clear();
        write_chunk(self.out, *b"IDAT", &self.chunk)
    }
}

/// The length of the uncompressed PNG data: one filter byte and `width * 4` bytes per row.
fn filtered_len(image: &Rgba8Image) -> u64 {
    u64::from(image.height()) * (1 + 4 * u64::from(image.width()))
}

/// The length of the PNG file [`write_png`] writes for `image`.
#[must_use]
pub fn png_len(image: &Rgba8Image) -> u64 {
    let data = filtered_len(image);
    let blocks = data.div_ceil(MAX_STORED as u64);
    // Signature, IHDR (12 + 13), IEND (12); per IDAT chunk 12 bytes of framing and 5 of block
    // header; the zlib header and the Adler-32 once.
    8 + 25 + 12 + blocks * 17 + data + 2 + 4
}

/// Writes `image` as a PNG file to `out`.
///
/// This performs no I/O of its own: `out` may be a file, a socket or memory.
///
/// # Errors
///
/// Any error of `out`.
///
/// # Examples
///
/// ```
/// use lumen::{png_len, write_png, ImageSize, Rgba8Image};
///
/// let image = Rgba8Image::from_bytes(ImageSize::new(2, 1)?, vec![255, 0, 0, 255, 0, 0, 255, 255])
///     .ok_or("eight bytes for two pixels")?;
/// let mut png = Vec::new();
/// write_png(&mut png, &image)?;
/// assert_eq!(png[..8], *b"\x89PNG\r\n\x1a\n");
/// assert_eq!(u64::try_from(png.len())?, png_len(&image));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn write_png<W: Write>(out: &mut W, image: &Rgba8Image) -> io::Result<()> {
    out.write_all(&SIGNATURE)?;
    let mut header = [0_u8; 13];
    header[..4].copy_from_slice(&image.width().to_be_bytes());
    header[4..8].copy_from_slice(&image.height().to_be_bytes());
    // Bit depth 8, colour type 6 (RGBA), compression 0, filter method 0, no interlace.
    header[8..].copy_from_slice(&[8, 6, 0, 0, 0]);
    write_chunk(out, *b"IHDR", &header)?;
    let mut blocks = StoredBlocks {
        out,
        block: Vec::with_capacity(MAX_STORED),
        chunk: Vec::with_capacity(MAX_STORED + 11),
        adler: Adler32::new(),
        remaining: filtered_len(image),
        first: true,
    };
    let row_len = image.bytes().len() / usize::try_from(image.height()).unwrap_or(usize::MAX);
    for row in image.bytes().chunks_exact(row_len.max(1)) {
        blocks.push(&[0])?;
        blocks.push(row)?;
    }
    if !blocks.block.is_empty() {
        blocks.flush()?;
    }
    write_chunk(out, *b"IEND", &[])
}

#[cfg(test)]
pub(crate) mod tests;
