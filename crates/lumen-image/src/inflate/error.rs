//! This module owns the decoder's errors: what was refused, and where.

use core::fmt;

/// A refused zlib stream: the fault, and the byte offset where it was found (see the
/// [crate documentation](crate) for how offsets are counted).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InflateError {
    kind: InflateErrorKind,
    offset: usize,
}

impl InflateError {
    pub(super) const fn new(kind: InflateErrorKind, offset: usize) -> Self {
        Self { kind, offset }
    }

    /// What was refused.
    #[must_use]
    pub const fn kind(&self) -> &InflateErrorKind {
        &self.kind
    }

    /// The offset, from the start of the stream, of the byte where the fault was found: the
    /// byte holding the last bit read, or for [`InflateErrorKind::Truncated`] the stream's
    /// length, and for [`InflateErrorKind::TrailingBytes`] the first byte after the trailer.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }
}

/// One of the three Huffman alphabets of a deflate block (RFC 1951 section 3.2.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alphabet {
    /// The code lengths' own code, of 19 symbols, that a dynamic block's header uses.
    CodeLength,
    /// Literal bytes, the end of the block and match lengths: 286 symbols, 288 codes.
    LiteralLength,
    /// Match distances: 30 symbols, 32 codes.
    Distance,
}

/// Why a zlib stream was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InflateErrorKind {
    /// The two header bytes, read as a big-endian number, are not a multiple of 31.
    HeaderCheck {
        /// The first header byte (`CMF`).
        cmf: u8,
        /// The second header byte (`FLG`).
        flg: u8,
    },
    /// The compression method is not 8 (deflate).
    CompressionMethod {
        /// The method the header names.
        method: u8,
    },
    /// The window size field is above 7 (a window over 32 768 bytes).
    WindowSize {
        /// The field (`CINFO`).
        cinfo: u8,
    },
    /// The header asks for a preset dictionary (`FDICT`), which no caller supplies.
    PresetDictionary,
    /// A block has the reserved type 3.
    ReservedBlockType,
    /// A stored block's length is not the complement of its check field.
    StoredLengthMismatch {
        /// The length field (`LEN`).
        len: u16,
        /// The check field (`NLEN`).
        nlen: u16,
    },
    /// A dynamic block declares more literal/length or distance codes than the alphabet has
    /// symbols (over 286 or 30).
    TooManyCodes {
        /// The alphabet.
        alphabet: Alphabet,
        /// How many codes the block declares.
        count: u16,
    },
    /// A code's lengths give more codes than the code space holds.
    OverSubscribedCode {
        /// The alphabet.
        alphabet: Alphabet,
    },
    /// A code's lengths leave part of the code space unused, outside the shapes the decoder
    /// admits (one symbol of length 1; no distance symbol at all).
    IncompleteCode {
        /// The alphabet.
        alphabet: Alphabet,
    },
    /// A repeat of the previous code length (symbol 16) comes first.
    RepeatWithoutLength,
    /// The code lengths run past the number of codes the block declares.
    CodeLengthsOverrun,
    /// The end-of-block symbol (256) has no code.
    MissingEndOfBlock,
    /// The bits read are the unused pattern of an incomplete code.
    UnusedCode {
        /// The alphabet.
        alphabet: Alphabet,
    },
    /// A fixed block's code for a symbol outside the alphabet: literal/length 286 or 287, or
    /// distance 30 or 31.
    InvalidSymbol {
        /// The alphabet.
        alphabet: Alphabet,
        /// The symbol.
        symbol: u16,
    },
    /// A match reaches back past the first decoded byte.
    DistanceTooFar {
        /// The match's distance.
        distance: usize,
        /// The bytes decoded before the match.
        available: usize,
    },
    /// A match reaches back past the window the header declares.
    DistanceBeyondWindow {
        /// The match's distance.
        distance: usize,
        /// The window size in bytes.
        window: usize,
    },
    /// The stream ends before its last block or its trailer does.
    Truncated,
    /// The Adler-32 trailer does not match the decoded bytes.
    Adler32Mismatch {
        /// The trailer's value.
        stored: u32,
        /// The Adler-32 of the decoded bytes.
        computed: u32,
    },
    /// Bytes follow the Adler-32 trailer.
    TrailingBytes {
        /// How many.
        count: usize,
    },
    /// The output would pass the caller's limit.
    OutputLimit {
        /// The limit in bytes.
        limit: usize,
    },
}

impl fmt::Display for Alphabet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::CodeLength => "code-length",
            Self::LiteralLength => "literal/length",
            Self::Distance => "distance",
        })
    }
}

impl fmt::Display for InflateErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HeaderCheck { cmf, flg } => {
                write!(f, "header check failed: {cmf:#04x} {flg:#04x} is not a multiple of 31")
            }
            Self::CompressionMethod { method } => write!(f, "compression method {method} is not deflate"),
            Self::WindowSize { cinfo } => write!(f, "window size field {cinfo} is over 7"),
            Self::PresetDictionary => f.write_str("a preset dictionary is not supported"),
            Self::ReservedBlockType => f.write_str("reserved block type 3"),
            Self::StoredLengthMismatch { len, nlen } => {
                write!(f, "stored block length {len} does not match its complement {nlen}")
            }
            Self::TooManyCodes { alphabet, count } => write!(f, "{count} {alphabet} codes declared"),
            Self::OverSubscribedCode { alphabet } => write!(f, "over-subscribed {alphabet} code"),
            Self::IncompleteCode { alphabet } => write!(f, "incomplete {alphabet} code"),
            Self::RepeatWithoutLength => f.write_str("code length repeat with no previous length"),
            Self::CodeLengthsOverrun => f.write_str("code lengths run past the declared codes"),
            Self::MissingEndOfBlock => f.write_str("the end-of-block symbol has no code"),
            Self::UnusedCode { alphabet } => write!(f, "unused {alphabet} code"),
            Self::InvalidSymbol { alphabet, symbol } => write!(f, "invalid {alphabet} symbol {symbol}"),
            Self::DistanceTooFar { distance, available } => {
                write!(f, "distance {distance} reaches before the output's {available} bytes")
            }
            Self::DistanceBeyondWindow { distance, window } => {
                write!(f, "distance {distance} is beyond the {window}-byte window")
            }
            Self::Truncated => f.write_str("the stream is truncated"),
            Self::Adler32Mismatch { stored, computed } => {
                write!(f, "adler-32 {stored:#010x} does not match the output's {computed:#010x}")
            }
            Self::TrailingBytes { count } => write!(f, "{count} bytes after the trailer"),
            Self::OutputLimit { limit } => write!(f, "the output passes its limit of {limit} bytes"),
        }
    }
}

impl fmt::Display for InflateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "zlib stream refused at byte {}: {}", self.offset, self.kind)
    }
}

impl core::error::Error for InflateError {}
