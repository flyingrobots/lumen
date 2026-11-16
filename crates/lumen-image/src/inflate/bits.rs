//! This module owns reading the input: bytes from a sequence of slices, and bits from those
//! bytes least significant first (RFC 1951 section 3.1.1).

use super::error::{InflateError, InflateErrorKind};

/// A reader of bits and bytes over the concatenation of its chunks.
///
/// Invariant: `held < 8` between calls, so the last byte taken holds every buffered bit, and
/// the bit most recently read is in byte `taken - 1`.
pub(super) struct BitReader<'a> {
    chunks: &'a [&'a [u8]],
    /// The chunk being read, and the position in it.
    chunk: usize,
    at: usize,
    /// Bytes taken from the input so far.
    taken: usize,
    /// Bits taken but not yet read, the next one lowest, and how many.
    buffer: u32,
    held: u32,
}

impl<'a> BitReader<'a> {
    pub(super) fn new(chunks: &'a [&'a [u8]]) -> Self {
        Self { chunks, chunk: 0, at: 0, taken: 0, buffer: 0, held: 0 }
    }

    /// The offset of the byte that holds the last bit read (zero before any).
    pub(super) fn last_offset(&self) -> usize {
        self.taken.saturating_sub(1)
    }

    /// The offset of the next byte to be taken.
    pub(super) fn next_offset(&self) -> usize {
        self.taken
    }

    /// The next whole byte, after the bits already read are discarded by [`Self::align`].
    pub(super) fn byte(&mut self) -> Result<u8, InflateError> {
        while let Some(chunk) = self.chunks.get(self.chunk) {
            if let Some(&byte) = chunk.get(self.at) {
                self.at += 1;
                self.taken = self.taken.saturating_add(1);
                return Ok(byte);
            }
            self.chunk += 1;
            self.at = 0;
        }
        Err(InflateError::new(InflateErrorKind::Truncated, self.taken))
    }

    /// The next `count` bits (at most 16) as an integer, the first read lowest.
    pub(super) fn bits(&mut self, count: u32) -> Result<u32, InflateError> {
        debug_assert!(count <= 16, "at most 16 bits at a time");
        while self.held < count {
            // `held < count <= 16`, so the byte lands below bit 24.
            self.buffer |= u32::from(self.byte()?) << self.held;
            self.held += 8;
        }
        let value = self.buffer & ((1 << count) - 1);
        self.buffer >>= count;
        self.held -= count;
        Ok(value)
    }

    /// Discards the bits left in the current byte, so the next read starts on a byte boundary.
    pub(super) fn align(&mut self) {
        self.buffer = 0;
        self.held = 0;
    }

    /// Copies the next `count` whole bytes into `out`, as a stored block's data. Call after
    /// [`Self::align`].
    pub(super) fn copy(&mut self, count: usize, out: &mut Vec<u8>) -> Result<(), InflateError> {
        let mut left = count;
        while left > 0 {
            let Some(chunk) = self.chunks.get(self.chunk) else {
                return Err(InflateError::new(InflateErrorKind::Truncated, self.taken));
            };
            let rest = chunk.get(self.at..).unwrap_or_default();
            let take = rest.len().min(left);
            out.extend_from_slice(rest.get(..take).unwrap_or_default());
            self.at += take;
            self.taken = self.taken.saturating_add(take);
            left -= take;
            if left > 0 {
                self.chunk += 1;
                self.at = 0;
            }
        }
        Ok(())
    }

    /// The bytes not yet taken, in every chunk.
    pub(super) fn remaining(&self) -> usize {
        let current = self.chunks.get(self.chunk).map_or(0, |c| c.len().saturating_sub(self.at));
        let later = self.chunks.get(self.chunk + 1..).unwrap_or_default();
        later.iter().fold(current, |sum, c| sum.saturating_add(c.len()))
    }
}
