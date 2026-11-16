//! Lumen's image decoders, written without a dependency and without floating point
//! ([ADR 0012](https://github.com/flyingrobots/lumen/blob/main/docs/adr/0012-image-decoding.md)).
//!
//! The crate is part of Lumen's deterministic core: the nondeterminism gates of ADR 0006 read it
//! as they read `lumen`. It holds, so far, one decoder: [`inflate_zlib`], which decompresses a
//! zlib stream (RFC 1950 framing over RFC 1951 deflate blocks) into at most a caller-given
//! number of bytes. [`inflate_zlib_chunks`] does the same for a stream split across several
//! slices, as a PNG file splits its image data across `IDAT` chunks.
//!
//! Every input is either decoded or refused with an [`InflateError`] that names the fault and
//! the byte offset where it was found; no input makes the decoder panic, read past its input or
//! allocate past the caller's limit.
//!
//! # Example
//!
//! ```
//! use lumen_image::{inflate_zlib, InflateErrorKind, OutputLimit};
//!
//! // The zlib stream of "abc" from a standard encoder, with its Adler-32 trailer.
//! let stream = [0x78, 0x9c, 0x4b, 0x4c, 0x4a, 0x06, 0x00, 0x02, 0x4d, 0x01, 0x27];
//! assert_eq!(inflate_zlib(&stream, OutputLimit::new(3))?, b"abc");
//!
//! let refused = inflate_zlib(&stream, OutputLimit::new(2)).unwrap_err();
//! assert_eq!(refused.kind(), &InflateErrorKind::OutputLimit { limit: 2 });
//! # Ok::<(), lumen_image::InflateError>(())
//! ```

// No float in the crate: `clippy.toml` bans the float types; this catches arithmetic on a float
// whose type is only inferred (docs/adr/0006-nondeterminism-gate.md).
#![deny(clippy::float_arithmetic)]

mod inflate;

pub use inflate::{
    inflate_zlib, inflate_zlib_chunks, Alphabet, InflateError, InflateErrorKind, OutputLimit,
    INFLATE_ID,
};
