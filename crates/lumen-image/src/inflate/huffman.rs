//! This module owns the canonical Huffman codes of RFC 1951 section 3.2.2: building a code from
//! its lengths, with the completeness rules of the [module above](super), and decoding a symbol
//! bit by bit.

use super::bits::BitReader;
use super::error::{Alphabet, InflateError, InflateErrorKind};

/// The longest code any alphabet uses.
const MAX_BITS: usize = 15;

/// The most codes in any alphabet (the fixed literal/length code's 288).
const MAX_SYMBOLS: usize = 288;

/// A canonical Huffman code: how many codes there are of each length, and the symbols in code
/// order (by length, then by symbol).
pub(super) struct Huffman {
    alphabet: Alphabet,
    counts: [u16; MAX_BITS + 1],
    symbols: [u16; MAX_SYMBOLS],
    longest: usize,
}

/// Why a set of lengths makes no code the decoder admits.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum CodeFault {
    OverSubscribed,
    Incomplete,
}

impl Huffman {
    /// The code whose symbol `s` has length `lengths[s]` (0: no code), refused if it is
    /// over-subscribed or incomplete beyond the shapes [the module above](super) admits.
    /// `lengths` holds at most 288 entries, each at most 15: the callers' alphabets guarantee
    /// both.
    pub(super) fn new(alphabet: Alphabet, lengths: &[u8]) -> Result<Self, CodeFault> {
        let code = Self::canonical(alphabet, lengths);
        // The code space left after each length; negative means more codes than room.
        let mut left: i32 = 1;
        for &count in code.counts.iter().skip(1) {
            left = 2 * left - i32::from(count);
            if left < 0 {
                return Err(CodeFault::OverSubscribed);
            }
        }
        let used: u16 = code.counts.iter().sum();
        let single = used == 1 && code.counts[1] == 1 && alphabet != Alphabet::CodeLength;
        let none = used == 0 && alphabet == Alphabet::Distance;
        if left > 0 && !single && !none {
            return Err(CodeFault::Incomplete);
        }
        Ok(code)
    }

    /// The canonical code of `lengths`, unchecked: for the fixed codes, which are complete.
    pub(super) fn canonical(alphabet: Alphabet, lengths: &[u8]) -> Self {
        let mut code = Self { alphabet, counts: [0; MAX_BITS + 1], symbols: [0; MAX_SYMBOLS], longest: 0 };
        for &length in lengths.iter().filter(|&&l| l != 0) {
            if let Some(count) = code.counts.get_mut(usize::from(length)) {
                *count += 1;
                code.longest = code.longest.max(usize::from(length));
            }
        }
        // The first slot of each length in `symbols`, then each symbol in its slot.
        let mut next = [0_u16; MAX_BITS + 1];
        for length in 1..MAX_BITS {
            next[length + 1] = next[length] + code.counts[length];
        }
        for (symbol, &length) in (0_u16..).zip(lengths) {
            // Each slot is below the number of lengths, so below 288.
            if let Some(slot) = next.get_mut(usize::from(length)).filter(|_| length != 0) {
                if let Some(entry) = code.symbols.get_mut(usize::from(*slot)) {
                    *entry = symbol;
                }
                *slot += 1;
            }
        }
        code
    }

    /// The next symbol from `input`, reading the code most significant bit first.
    pub(super) fn decode(&self, input: &mut BitReader<'_>) -> Result<u16, InflateError> {
        // `code` is the bits read so far; `first` the first code of the current length and
        // `index` its slot in `symbols`.
        let (mut code, mut first, mut index) = (0_u32, 0_u32, 0_u32);
        for &count in self.counts.iter().take(self.longest + 1).skip(1) {
            code |= input.bits(1)?;
            let count = u32::from(count);
            if code < first + count {
                let slot = usize::try_from(index + code - first).unwrap_or(MAX_SYMBOLS);
                if let Some(&symbol) = self.symbols.get(slot) {
                    return Ok(symbol);
                }
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        let kind = InflateErrorKind::UnusedCode { alphabet: self.alphabet };
        Err(InflateError::new(kind, input.last_offset()))
    }
}
