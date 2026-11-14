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
pub const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

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
pub struct Crc32(u32);

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

impl Crc32 {
    /// The CRC of no bytes.
    #[must_use]
    pub const fn new() -> Self {
        Self(0xFFFF_FFFF)
    }

    /// Adds bytes.
    #[allow(clippy::cast_possible_truncation, reason = "the low byte of the CRC indexes the table")]
    pub fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = CRC_TABLE[usize::from((self.0 as u8) ^ byte)] ^ (self.0 >> 8);
        }
    }

    /// The CRC of every byte added.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.0 ^ 0xFFFF_FFFF
    }
}

/// A running Adler-32 as zlib streams use it.
#[derive(Clone, Copy, Debug)]
pub struct Adler32 {
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
    pub const fn new() -> Self {
        Self { a: 1, b: 0 }
    }

    /// Adds bytes.
    pub fn update(&mut self, bytes: &[u8]) {
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
    pub const fn value(self) -> u32 {
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
/// # Errors
///
/// Any error of `out`.
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
#[allow(
    clippy::cast_possible_truncation,
    reason = "test data and lengths are small and truncated on purpose"
)]
pub(crate) mod tests {
    use super::*;
    use crate::image::ImageSize;

    /// CRC-32 bit by bit from the polynomial, with no table: an independent computation.
    fn crc_bitwise(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFF_u32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    /// Adler-32 from its definition, with a reduction after every byte.
    fn adler_definition(bytes: &[u8]) -> u32 {
        let (mut a, mut b) = (1_u32, 0_u32);
        for &byte in bytes {
            a = (a + u32::from(byte)) % 65_521;
            b = (b + a) % 65_521;
        }
        (b << 16) | a
    }

    fn crc(bytes: &[u8]) -> u32 {
        let mut crc = Crc32::new();
        crc.update(bytes);
        crc.value()
    }

    fn adler(bytes: &[u8]) -> u32 {
        let mut adler = Adler32::new();
        adler.update(bytes);
        adler.value()
    }

    #[test]
    fn checksums_match_published_values() {
        // The CRC-32 check value of "123456789", and the CRC every PNG's IEND chunk carries.
        assert_eq!(crc(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc(b"IEND"), 0xAE42_6082);
        assert_eq!(crc(b""), 0);
        // The worked example of Adler-32.
        assert_eq!(adler(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler(b""), 1);
    }

    #[test]
    fn checksums_match_independent_computations() {
        // 0xFF bytes push the Adler sums hardest; lengths cross the 5552-byte reduction run.
        for len in [1, 255, 5551, 5552, 5553, 20_000, 70_000] {
            let ones = vec![0xFF_u8; len];
            let mixed: Vec<u8> = (0..len).map(|i| (i * 7 + i / 251) as u8).collect();
            for data in [&ones, &mixed] {
                assert_eq!(crc(data), crc_bitwise(data), "crc, length {len}");
                assert_eq!(adler(data), adler_definition(data), "adler, length {len}");
            }
        }
    }

    /// A decoded PNG: its size and its RGBA bytes.
    pub(crate) struct Decoded {
        pub(crate) width: u32,
        pub(crate) height: u32,
        pub(crate) rgba: Vec<u8>,
        pub(crate) idat_chunks: usize,
    }

    fn be32(bytes: &[u8]) -> u32 {
        u32::from_be_bytes(bytes.try_into().expect("four bytes"))
    }

    /// A strict decoder for exactly what `write_png` should produce: checks the signature,
    /// every chunk CRC (bit by bit), the chunk order, the header fields, the zlib header,
    /// every stored block's length and its complement, the Adler-32 (from its definition)
    /// and every row's filter byte.
    pub(crate) fn decode(file: &[u8]) -> Decoded {
        assert_eq!(file[..8], SIGNATURE, "signature");
        let mut at = 8;
        let mut chunks: Vec<([u8; 4], &[u8])> = Vec::new();
        while at < file.len() {
            let len = be32(&file[at..at + 4]) as usize;
            let kind: [u8; 4] = file[at + 4..at + 8].try_into().expect("four bytes");
            let data = &file[at + 8..at + 8 + len];
            let stored_crc = be32(&file[at + 8 + len..at + 12 + len]);
            assert_eq!(stored_crc, crc_bitwise(&file[at + 4..at + 8 + len]), "chunk crc");
            chunks.push((kind, data));
            at += 12 + len;
        }
        assert_eq!(at, file.len(), "no trailing bytes");
        let (first, rest) = chunks.split_first().expect("chunks");
        let (last, idat) = rest.split_last().expect("chunks");
        assert_eq!(&first.0, b"IHDR");
        assert_eq!(&last.0, b"IEND");
        assert!(last.1.is_empty());
        assert!(!idat.is_empty() && idat.iter().all(|(kind, _)| kind == b"IDAT"));
        let header = first.1;
        assert_eq!(header.len(), 13);
        let (width, height) = (be32(&header[..4]), be32(&header[4..8]));
        assert_eq!(header[8..], [8, 6, 0, 0, 0], "8-bit RGBA, no interlace");

        let stream: Vec<u8> = idat.iter().flat_map(|(_, data)| data.iter().copied()).collect();
        assert_eq!(stream[..2], ZLIB_HEADER);
        assert_eq!((u32::from(stream[0]) * 256 + u32::from(stream[1])) % 31, 0, "FCHECK");
        let mut at = 2;
        let mut filtered = Vec::new();
        loop {
            let flags = stream[at];
            assert_eq!(flags & 0b110, 0, "stored block");
            let len = u16::from_le_bytes([stream[at + 1], stream[at + 2]]);
            let nlen = u16::from_le_bytes([stream[at + 3], stream[at + 4]]);
            assert_eq!(nlen, !len, "NLEN");
            let len = usize::from(len);
            filtered.extend_from_slice(&stream[at + 5..at + 5 + len]);
            at += 5 + len;
            if flags & 1 == 1 {
                break;
            }
        }
        assert_eq!(be32(&stream[at..at + 4]), adler_definition(&filtered), "adler-32");
        assert_eq!(at + 4, stream.len(), "nothing after the adler-32");

        let row = 1 + 4 * width as usize;
        assert_eq!(filtered.len(), row * height as usize);
        let mut rgba = Vec::new();
        for line in filtered.chunks_exact(row) {
            assert_eq!(line[0], 0, "filter type none");
            rgba.extend_from_slice(&line[1..]);
        }
        Decoded { width, height, rgba, idat_chunks: idat.len() }
    }

    fn image(width: u32, height: u32) -> Rgba8Image {
        let size = ImageSize::new(width, height).expect("valid");
        let bytes = (0..u64::from(width) * u64::from(height) * 4)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
            .collect();
        Rgba8Image::from_bytes(size, bytes).expect("right length")
    }

    #[test]
    fn written_files_decode_to_the_pixels() {
        // Sizes around the stored-block limit of 65 535 bytes: a 16 383-pixel row is 65 533
        // filtered bytes (one block) and a 16 384-pixel row 65 537 (two); 128 by 128 is
        // 65 664 (two); 300 by 200 is 240 200 (four).
        for (width, height, blocks) in
            [(1, 1, 1), (3, 2, 1), (128, 128, 2), (16_383, 1, 1), (16_384, 1, 2), (300, 200, 4)]
        {
            let image = image(width, height);
            let mut file = Vec::new();
            write_png(&mut file, &image).expect("writes to memory");
            assert_eq!(file.len() as u64, png_len(&image), "{width}x{height}");
            let decoded = decode(&file);
            assert_eq!((decoded.width, decoded.height), (width, height));
            assert_eq!(decoded.rgba, image.bytes(), "{width}x{height}");
            assert_eq!(decoded.idat_chunks, blocks, "{width}x{height}");
        }
    }

    #[test]
    fn a_one_pixel_file_is_byte_for_byte_as_constructed_by_hand() {
        let size = ImageSize::new(1, 1).expect("valid");
        let image = Rgba8Image::from_bytes(size, vec![1, 2, 3, 255]).expect("right length");
        let mut file = Vec::new();
        write_png(&mut file, &image).expect("writes to memory");
        let ihdr_data = [0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0];
        // zlib header, final stored block of 5 bytes (00 01 02 03 FF), Adler-32 of them.
        // Adler: a = 1 + 0 + 1 + 2 + 3 + 255 = 262; b = 1 + 2 + 4 + 7 + 262 = 276.
        let idat_data =
            [0x78, 0x01, 0x01, 0x05, 0x00, 0xFA, 0xFF, 0, 1, 2, 3, 255, 0x01, 0x14, 0x01, 0x06];
        let mut want = SIGNATURE.to_vec();
        for (kind, data) in
            [(*b"IHDR", &ihdr_data[..]), (*b"IDAT", &idat_data[..]), (*b"IEND", &[])]
        {
            want.extend_from_slice(&(data.len() as u32).to_be_bytes());
            want.extend_from_slice(&kind);
            want.extend_from_slice(data);
            let mut covered = kind.to_vec();
            covered.extend_from_slice(data);
            want.extend_from_slice(&crc_bitwise(&covered).to_be_bytes());
        }
        assert_eq!(file, want);
        assert_eq!(file[file.len() - 4..], [0xAE, 0x42, 0x60, 0x82], "IEND's well-known CRC");
    }
}
