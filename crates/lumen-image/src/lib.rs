//! Lumen's image decoders: bytes of an image file in, pixels out, by stated integer rules.
//!
//! The crate has no dependency and no floating point, and it is part of Lumen's deterministic
//! core (docs/adr/0012-image-decoding.md, docs/adr/0006-nondeterminism-gate.md): the same file
//! decodes to the same bytes on every platform and in every build profile. Every decoder
//! follows the same rules: a typed error that names the refused feature or the byte offset,
//! every limit taken from the caller and checked before a large allocation, and no panic and
//! no read past the input on any bytes.
//!
//! - [`jpeg`]: baseline JPEG to RGB8, under the identity [`jpeg::JPEG_DECODE_ID`].

// No float in the crate: `clippy.toml` bans the float types; this catches arithmetic on a float
// whose type is only inferred (docs/adr/0006-nondeterminism-gate.md).
#![deny(clippy::float_arithmetic)]

pub mod jpeg;
