//! Unit tests of the PNG encoder, with the strict decoder other tests use as an oracle.

#![allow(
    clippy::cast_possible_truncation,
    reason = "test data and lengths are small and truncated on purpose"
)]

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
    /// The LEN field of each stored block, in stream order.
    pub(crate) block_lengths: Vec<usize>,
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
    let mut block_lengths = Vec::new();
    loop {
        let flags = stream[at];
        assert_eq!(flags & 0b110, 0, "stored block");
        let len = u16::from_le_bytes([stream[at + 1], stream[at + 2]]);
        let nlen = u16::from_le_bytes([stream[at + 3], stream[at + 4]]);
        assert_eq!(nlen, !len, "NLEN");
        let len = usize::from(len);
        block_lengths.push(len);
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
    Decoded { width, height, rgba, idat_chunks: idat.len(), block_lengths }
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

/// Oracle: RFC 1951, section 3.2.4: a stored block's LEN is 16 bits, so a block holds at most
/// 65 535 bytes, and the encoder fills each block before starting the next. The filtered length
/// is `height * (1 + 4 * width)`. At exactly 65 535 (1 by 13 107, 64 by 255 and 5 461 by 3) the
/// stream is one full block; the next sizes up hold one full block and the rest. No image has
/// exactly 65 536 filtered bytes: `1 + 4 * width` is odd and above 1, and 65 536 has no odd
/// factor but 1. The block headers are read by the strict decoder above, and each file's length
/// is `png_len`'s.
#[test]
fn a_full_stored_block_holds_65_535_bytes() {
    for (width, height, blocks) in [
        (1, 13_107, &[65_535][..]),
        (64, 255, &[65_535]),
        (5_461, 3, &[65_535]),
        (1, 13_108, &[65_535, 5]),
        (64, 256, &[65_535, 257]),
        (5_462, 3, &[65_535, 12]),
    ] {
        let image = image(width, height);
        let mut file = Vec::new();
        write_png(&mut file, &image).expect("writes to memory");
        let decoded = decode(&file);
        assert_eq!(decoded.block_lengths, blocks, "{width}x{height}");
        assert_eq!(decoded.idat_chunks, blocks.len(), "{width}x{height}");
        assert_eq!(file.len() as u64, png_len(&image), "{width}x{height}");
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
    for (kind, data) in [(*b"IHDR", &ihdr_data[..]), (*b"IDAT", &idat_data[..]), (*b"IEND", &[])] {
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
