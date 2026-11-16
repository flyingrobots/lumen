//! This module owns the Adler-32 checksum of RFC 1950 section 8.

/// The largest prime below 2^16.
const MODULUS: u32 = 65_521;

/// The most bytes summed before reducing: the largest `n` with
/// `255 n (n + 1) / 2 + (n + 1) (MODULUS - 1) < 2^32`, so neither sum can overflow (zlib's
/// `NMAX`).
const RUN: usize = 5_552;

/// The Adler-32 of `bytes`: the high half is the sum of the running sums, the low half one
/// plus the sum of the bytes, both modulo 65 521.
pub(super) fn adler32(bytes: &[u8]) -> u32 {
    let (mut low, mut high) = (1_u32, 0_u32);
    for run in bytes.chunks(RUN) {
        for &byte in run {
            low += u32::from(byte);
            high += low;
        }
        low %= MODULUS;
        high %= MODULUS;
    }
    high << 16 | low
}
