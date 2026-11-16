//! This module owns the deflate blocks of RFC 1951 section 3.2: their headers, the stored,
//! fixed-Huffman and dynamic-Huffman forms, and the literal, length and distance symbols.

use super::bits::BitReader;
use super::error::{Alphabet, InflateError, InflateErrorKind};
use super::huffman::{CodeFault, Huffman};
use super::output::Output;

/// The base length of length symbols 257 to 285, and their extra bits (section 3.2.5).
const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] =
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];

/// The base distance of distance symbols 0 to 29, and their extra bits (section 3.2.5).
const DISTANCE_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DISTANCE_EXTRA: [u8; 30] =
    [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// The order in which a dynamic block lists the code-length code's lengths (section 3.2.7).
const CODE_LENGTH_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// The end-of-block symbol.
const END_OF_BLOCK: u16 = 256;

/// Decodes blocks into `output` until the one marked final.
pub(super) fn inflate(input: &mut BitReader<'_>, output: &mut Output) -> Result<(), InflateError> {
    loop {
        let last = input.bits(1)? == 1;
        match input.bits(2)? {
            0 => stored(input, output)?,
            1 => {
                let (literals, distances) = fixed();
                codes(input, output, &literals, &distances)?;
            }
            2 => {
                let (literals, distances) = dynamic(input)?;
                codes(input, output, &literals, &distances)?;
            }
            _ => return Err(InflateError::new(InflateErrorKind::ReservedBlockType, input.last_offset())),
        }
        if last {
            return Ok(());
        }
    }
}

/// A stored block (section 3.2.4): to the byte boundary, `LEN`, `NLEN`, then `LEN` bytes.
fn stored(input: &mut BitReader<'_>, output: &mut Output) -> Result<(), InflateError> {
    input.align();
    let mut field = || -> Result<u16, InflateError> {
        let low = input.byte()?;
        Ok(u16::from_le_bytes([low, input.byte()?]))
    };
    let (len, nlen) = (field()?, field()?);
    if len != !nlen {
        let kind = InflateErrorKind::StoredLengthMismatch { len, nlen };
        return Err(InflateError::new(kind, input.last_offset()));
    }
    output.stored(input, usize::from(len))
}

/// The fixed codes (section 3.2.6): literal/length 0-143 of 8 bits, 144-255 of 9, 256-279 of 7,
/// 280-287 of 8; all 32 distance codes of 5 bits. Both are complete.
fn fixed() -> (Huffman, Huffman) {
    let mut lengths = [8_u8; 288];
    lengths[144..256].fill(9);
    lengths[256..280].fill(7);
    (Huffman::canonical(Alphabet::LiteralLength, &lengths), Huffman::canonical(Alphabet::Distance, &[5; 32]))
}

/// A dynamic block's header (section 3.2.7): the counts, the code-length code, then the
/// literal/length and distance code lengths it encodes.
fn dynamic(input: &mut BitReader<'_>) -> Result<(Huffman, Huffman), InflateError> {
    let literal_count = 257 + input.bits(5)?;
    let distance_count = 1 + input.bits(5)?;
    let length_count = 4 + input.bits(4)?;
    let too_many = |alphabet, count: u32| {
        let count = u16::try_from(count).unwrap_or(u16::MAX);
        InflateError::new(InflateErrorKind::TooManyCodes { alphabet, count }, input.last_offset())
    };
    if literal_count > 286 {
        return Err(too_many(Alphabet::LiteralLength, literal_count));
    }
    if distance_count > 30 {
        return Err(too_many(Alphabet::Distance, distance_count));
    }
    let mut code_lengths = [0_u8; 19];
    for &symbol in CODE_LENGTH_ORDER.iter().take(usize::try_from(length_count).unwrap_or(19)) {
        code_lengths[symbol] = u8::try_from(input.bits(3)?).unwrap_or(0);
    }
    let code_lengths = build(input, Alphabet::CodeLength, &code_lengths)?;
    let (literal_count, total) = (literal_count as usize, (literal_count + distance_count) as usize);
    let mut lengths = [0_u8; 286 + 30];
    let mut filled = 0;
    while filled < total {
        let symbol = code_lengths.decode(input)?;
        let (value, repeat) = match symbol {
            0..=15 => (u8::try_from(symbol).unwrap_or(0), 1),
            16 => {
                let previous = filled.checked_sub(1).and_then(|i| lengths.get(i)).copied();
                let previous = previous.ok_or(InflateError::new(InflateErrorKind::RepeatWithoutLength, input.last_offset()))?;
                (previous, 3 + input.bits(2)?)
            }
            17 => (0, 3 + input.bits(3)?),
            _ => (0, 11 + input.bits(7)?),
        };
        let end = filled + repeat as usize;
        let run = lengths.get_mut(filled..end).filter(|_| end <= total);
        run.ok_or(InflateError::new(InflateErrorKind::CodeLengthsOverrun, input.last_offset()))?.fill(value);
        filled = end;
    }
    if lengths[usize::from(END_OF_BLOCK)] == 0 {
        return Err(InflateError::new(InflateErrorKind::MissingEndOfBlock, input.last_offset()));
    }
    let literals = build(input, Alphabet::LiteralLength, &lengths[..literal_count])?;
    let distances = build(input, Alphabet::Distance, &lengths[literal_count..total])?;
    Ok((literals, distances))
}

/// The code of `lengths` in `alphabet`, or the error for its fault at the current offset.
fn build(input: &BitReader<'_>, alphabet: Alphabet, lengths: &[u8]) -> Result<Huffman, InflateError> {
    Huffman::new(alphabet, lengths).map_err(|fault| {
        let kind = match fault {
            CodeFault::OverSubscribed => InflateErrorKind::OverSubscribedCode { alphabet },
            CodeFault::Incomplete => InflateErrorKind::IncompleteCode { alphabet },
        };
        InflateError::new(kind, input.last_offset())
    })
}

/// A Huffman-coded block's symbols (section 3.2.5), to its end-of-block symbol.
fn codes(input: &mut BitReader<'_>, output: &mut Output, literals: &Huffman, distances: &Huffman) -> Result<(), InflateError> {
    loop {
        let symbol = literals.decode(input)?;
        let invalid = |alphabet, symbol| InflateError::new(InflateErrorKind::InvalidSymbol { alphabet, symbol }, input.last_offset());
        match symbol {
            0..=255 => output.literal(u8::try_from(symbol).unwrap_or(0), input.last_offset())?,
            END_OF_BLOCK => return Ok(()),
            _ => {
                let index = usize::from(symbol - 257);
                let (Some(&base), Some(&extra)) = (LENGTH_BASE.get(index), LENGTH_EXTRA.get(index)) else {
                    return Err(invalid(Alphabet::LiteralLength, symbol));
                };
                let length = usize::from(base) + input.bits(u32::from(extra))? as usize;
                let symbol = distances.decode(input)?;
                let index = usize::from(symbol);
                let (Some(&base), Some(&extra)) = (DISTANCE_BASE.get(index), DISTANCE_EXTRA.get(index)) else {
                    return Err(invalid(Alphabet::Distance, symbol));
                };
                let distance = usize::from(base) + input.bits(u32::from(extra))? as usize;
                output.copy_match(distance, length, input.last_offset())?;
            }
        }
    }
}
