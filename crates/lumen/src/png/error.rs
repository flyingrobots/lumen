//! Why a PNG could not be written: [`PngError`].

use core::fmt;
use std::io;

/// Why [`write_png`](super::write_png) failed.
///
/// Non-exhaustive: a later version may report a new failure, so a `match` outside this crate
/// needs a wildcard arm. Not `Clone` or `PartialEq`, since it holds the writer's `io::Error`.
#[derive(Debug)]
#[non_exhaustive]
pub enum PngError {
    /// The writer refused bytes. Its text is the writer's error, which `source()` returns.
    Write(io::Error),
    /// A chunk's data is longer than the `u32` its length field holds. The encoder's chunks are
    /// at most 65 546 bytes, so no image reaches this.
    ChunkTooLong {
        /// The data length.
        length: usize,
    },
    /// A stored deflate block is longer than the 65 535 bytes its `u16` length field holds. The
    /// encoder fills blocks to at most that, so no image reaches this.
    BlockTooLong {
        /// The block length.
        length: usize,
    },
}

impl fmt::Display for PngError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Write(error) => write!(f, "{error}"),
            Self::ChunkTooLong { length } => {
                write!(f, "a PNG chunk of {length} bytes is longer than its length field holds")
            }
            Self::BlockTooLong { length } => {
                write!(f, "a stored block of {length} bytes is longer than 65535")
            }
        }
    }
}

impl std::error::Error for PngError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Write(error) => Some(error),
            Self::ChunkTooLong { .. } | Self::BlockTooLong { .. } => None,
        }
    }
}

impl From<io::Error> for PngError {
    fn from(error: io::Error) -> Self {
        Self::Write(error)
    }
}
