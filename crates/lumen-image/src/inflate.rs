//! This module owns zlib decompression: the RFC 1950 frame (a two-byte header, deflate blocks,
//! an Adler-32 trailer) and the entry points.
//!
//! The rules the decoder holds to, beyond the RFCs' own:
//!
//! - **A preset dictionary is refused** (`FDICT` set), by name: no caller supplies one.
//! - **Distances are checked twice**: against the bytes decoded so far, and against the window
//!   the header declares (`CINFO`, 256 to 32 768 bytes), as zlib's own decoder does.
//! - **Incomplete codes**: a literal/length or distance code whose lengths do not fill the code
//!   space is refused, except a code of exactly one symbol of length 1 (RFC 1951 section 3.2.7
//!   allows one distance code of one bit; zlib accepts the same shape in either alphabet) and a
//!   distance code with no symbols at all (a block of literals only). Reading the unused bit
//!   pattern of such a code is refused. The code-length code must be complete.
//! - **Trailing bytes are an error**: the stream must end at its Adler-32 trailer.
//!
//! Byte offsets in errors count from the start of the stream (for [`inflate_zlib_chunks`], of
//! the chunks' concatenation). A fault is reported at the byte holding the last bit read when
//! it was found; a truncation at the stream's length, the first byte that is missing.

mod adler32;
mod bits;
mod blocks;
mod error;
mod huffman;
mod output;
#[cfg(test)]
mod tests;

pub use error::{Alphabet, InflateError, InflateErrorKind};

use bits::BitReader;
use output::Output;

/// The identity of the decoder's contract: which zlib streams it accepts (the rules above and
/// in [`InflateErrorKind`]) and what it returns for them. An accepted stream's output is fixed
/// by RFC 1951; a change to what is refused makes a new identity.
pub const INFLATE_ID: &str = "lumen-zlib-inflate/1";

/// The most bytes a decode may produce, from the caller.
///
/// A stream whose output would pass it is refused with [`InflateErrorKind::OutputLimit`] as soon
/// as a block declares or decodes the first byte beyond it, and the output's buffer never
/// requests capacity beyond it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputLimit {
    bytes: usize,
}

impl OutputLimit {
    /// A limit of `bytes` decoded bytes; zero admits only an empty stream.
    #[must_use]
    pub const fn new(bytes: usize) -> Self {
        Self { bytes }
    }

    /// The limit in bytes.
    #[must_use]
    pub const fn bytes(self) -> usize {
        self.bytes
    }
}

/// Decompresses the zlib stream `stream` into at most `limit` bytes.
///
/// Allocates the returned vector, at most `limit` bytes of capacity; reads nothing outside
/// `stream`.
///
/// # Errors
///
/// An [`InflateError`] for a malformed or truncated stream, a bad Adler-32 check, bytes after
/// the trailer, or an output beyond `limit`; its [`kind`](InflateError::kind) names the fault.
///
/// # Panics
///
/// Does not panic.
pub fn inflate_zlib(stream: &[u8], limit: OutputLimit) -> Result<Vec<u8>, InflateError> {
    inflate_zlib_chunks(&[stream], limit)
}

/// Decompresses one zlib stream stored as the concatenation of `chunks`, in order, into at most
/// `limit` bytes: a PNG file's `IDAT` chunk data, passed without copying them together. Empty
/// chunks are allowed, and a chunk boundary may fall anywhere, even inside a code. Error offsets
/// count from the start of the first chunk.
///
/// # Errors
///
/// As [`inflate_zlib`].
///
/// # Panics
///
/// Does not panic.
pub fn inflate_zlib_chunks(
    chunks: &[&[u8]],
    limit: OutputLimit,
) -> Result<Vec<u8>, InflateError> {
    let mut input = BitReader::new(chunks);
    let window = header(&mut input)?;
    let mut output = Output::new(limit.bytes, window);
    blocks::inflate(&mut input, &mut output)?;
    trailer(&mut input, output.bytes())?;
    Ok(output.into_bytes())
}

/// Reads the two header bytes (RFC 1950 section 2.2) and returns the window size they declare.
fn header(input: &mut BitReader<'_>) -> Result<usize, InflateError> {
    let cmf = input.byte()?;
    let flg = input.byte()?;
    let at = |kind| InflateError::new(kind, input.last_offset());
    if (u16::from(cmf) << 8 | u16::from(flg)) % 31 != 0 {
        return Err(at(InflateErrorKind::HeaderCheck { cmf, flg }));
    }
    if cmf & 0x0f != 8 {
        return Err(at(InflateErrorKind::CompressionMethod { method: cmf & 0x0f }));
    }
    let cinfo = cmf >> 4;
    if cinfo > 7 {
        return Err(at(InflateErrorKind::WindowSize { cinfo }));
    }
    if flg & 0x20 != 0 {
        return Err(at(InflateErrorKind::PresetDictionary));
    }
    Ok(1 << (cinfo + 8))
}

/// Reads the Adler-32 trailer after the last block (RFC 1950 section 2.2), checks it against
/// `decoded` and refuses anything after it.
fn trailer(input: &mut BitReader<'_>, decoded: &[u8]) -> Result<(), InflateError> {
    input.align();
    let mut stored = 0_u32;
    for _ in 0..4 {
        stored = stored << 8 | u32::from(input.byte()?);
    }
    let computed = adler32::adler32(decoded);
    if stored != computed {
        let kind = InflateErrorKind::Adler32Mismatch { stored, computed };
        return Err(InflateError::new(kind, input.last_offset()));
    }
    let trailing = input.remaining();
    if trailing != 0 {
        let kind = InflateErrorKind::TrailingBytes { count: trailing };
        return Err(InflateError::new(kind, input.next_offset()));
    }
    Ok(())
}
