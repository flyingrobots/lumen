//! Generated evidence for the PNG encoder's "for all sizes" claims (testing standard rule 5).
//!
//! Oracle: the strict decoder of `png::tests` (every chunk CRC bit by bit, every stored
//! block's complement, the Adler-32 from its definition, every filter byte) and the block
//! count `ceil(filtered length / 65 535)` computed here from the format's definition.

use crate::generator::Gen;
use crate::image::{ImageSize, Rgba8Image};
use crate::png::tests::decode;
use crate::png::{png_len, write_png};

const SEED: u64 = 0x5EED_0000_09A6;

/// The largest stored block, in bytes (RFC 1951, section 3.2.4).
const BLOCK: u64 = 65_535;

/// Sizes to try: 40 random ones (a side up to 211, or up to 1 009 with the other under 40) and,
/// for each of 1 to 3 blocks and heights 1 to 4, the widths whose filtered length falls just
/// under, on or just over the block boundary.
fn sizes() -> Vec<(u32, u32)> {
    let mut rng = Gen::new(SEED);
    let mut sizes: Vec<(u32, u32)> = (0..40)
        .map(|case| {
            let (long, short) = (rng.between(1, 1_009), rng.between(1, 39));
            let (width, height) = match case % 4 {
                0 => (long, short),
                1 => (short, long),
                _ => (rng.between(1, 211), rng.between(1, 211)),
            };
            (u32::try_from(width).expect("small"), u32::try_from(height).expect("small"))
        })
        .collect();
    for blocks in 1..=3_u64 {
        for height in 1..=4_u64 {
            let width = (blocks * BLOCK / height).saturating_sub(1) / 4;
            for near in [width.saturating_sub(1), width, width + 1] {
                let size = (
                    u32::try_from(near.max(1)).expect("small"),
                    u32::try_from(height).expect("small"),
                );
                // A side over the 32 768 limit is no image.
                if ImageSize::new(size.0, size.1).is_ok() {
                    sizes.push(size);
                }
            }
        }
    }
    sizes
}

/// For all sizes: the file decodes to the same size and pixels, its length is `png_len`, and
/// it holds the number of blocks the format's definition gives. 73 sizes, the largest about
/// 197 000 bytes; 0.2 s in the test profile.
#[test]
fn files_of_any_size_decode_to_the_same_pixels_and_have_the_length_png_len_gives() {
    for (case, (width, height)) in sizes().into_iter().enumerate() {
        let case_seed = Gen::case_seed(SEED, case as u64);
        let size = ImageSize::new(width, height).expect("a valid size");
        let bytes = Gen::new(case_seed).bytes(size.byte_len());
        let image = Rgba8Image::from_bytes(size, bytes).expect("right length");
        let mut file = Vec::new();
        write_png(&mut file, &image).expect("writes to memory");
        let at = format!("{width}x{height}, seed {SEED:#x}, case {case}, case seed {case_seed:#x}");
        assert_eq!(file.len() as u64, png_len(&image), "length, {at}");
        let decoded = decode(&file);
        assert_eq!((decoded.width, decoded.height), (width, height), "size, {at}");
        assert_eq!(decoded.rgba, image.bytes(), "pixels, {at}");
        let filtered = u64::from(height) * (1 + 4 * u64::from(width));
        assert_eq!(decoded.idat_chunks as u64, filtered.div_ceil(BLOCK), "blocks, {at}");
    }
}
