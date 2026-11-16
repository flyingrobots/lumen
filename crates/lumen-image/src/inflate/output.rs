//! This module owns the decoded bytes: appending literals, stored data and matches while
//! holding the caller's limit and the header's window.

use super::bits::BitReader;
use super::error::{InflateError, InflateErrorKind};

/// The first capacity requested, before the output doubles towards its limit.
const FIRST_CAPACITY: usize = 4_096;

/// The bytes decoded so far, never more than `limit`, in a buffer that never requests capacity
/// beyond `limit`.
pub(super) struct Output {
    bytes: Vec<u8>,
    limit: usize,
    window: usize,
}

impl Output {
    pub(super) fn new(limit: usize, window: usize) -> Self {
        Self { bytes: Vec::new(), limit, window }
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(super) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// Makes room for `count` more bytes, refusing (at `offset`) if they pass the limit.
    /// Capacity grows by doubling, capped at the limit.
    fn reserve(&mut self, count: usize, offset: usize) -> Result<(), InflateError> {
        let len = self.bytes.len();
        let needed = len
            .checked_add(count)
            .filter(|&n| n <= self.limit)
            .ok_or(InflateError::new(InflateErrorKind::OutputLimit { limit: self.limit }, offset))?;
        if needed > self.bytes.capacity() {
            let target = self.bytes.capacity().saturating_mul(2).max(FIRST_CAPACITY).max(needed);
            self.bytes.reserve_exact(target.min(self.limit) - len);
        }
        Ok(())
    }

    /// Appends one literal byte.
    pub(super) fn literal(&mut self, byte: u8, offset: usize) -> Result<(), InflateError> {
        self.reserve(1, offset)?;
        self.bytes.push(byte);
        Ok(())
    }

    /// Appends a stored block's `count` bytes from `input`, after checking the limit.
    pub(super) fn stored(&mut self, input: &mut BitReader<'_>, count: usize) -> Result<(), InflateError> {
        self.reserve(count, input.last_offset())?;
        input.copy(count, &mut self.bytes)
    }

    /// Appends the `length` bytes that start `distance` bytes back, which may overlap the bytes
    /// being written (a distance below the length repeats them).
    pub(super) fn copy_match(&mut self, distance: usize, length: usize, offset: usize) -> Result<(), InflateError> {
        let available = self.bytes.len();
        if distance > available {
            return Err(InflateError::new(InflateErrorKind::DistanceTooFar { distance, available }, offset));
        }
        if distance > self.window {
            let window = self.window;
            return Err(InflateError::new(InflateErrorKind::DistanceBeyondWindow { distance, window }, offset));
        }
        self.reserve(length, offset)?;
        let mut left = length;
        while left > 0 {
            // Each piece is at most `distance` long, so it lies wholly in the bytes written.
            let start = self.bytes.len() - distance;
            let piece = left.min(distance);
            self.bytes.extend_from_within(start..start + piece);
            left -= piece;
        }
        Ok(())
    }
}
