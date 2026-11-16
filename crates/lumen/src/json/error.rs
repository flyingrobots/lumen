//! Why a document was refused, and where: [`JsonError`].

use core::fmt;

/// What was wrong with a document.
///
/// Non-exhaustive: a later version may report a new failure, so a `match` outside this crate
/// needs a wildcard arm.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum JsonErrorKind {
    /// The input is longer than the size limit. The position is that of the first byte past it;
    /// nothing was read.
    TooLarge {
        /// The input's length in bytes.
        length: usize,
        /// The limit in bytes.
        limit: usize,
    },
    /// A container opens deeper than the depth limit. The position is its opening bracket.
    TooDeep {
        /// The limit.
        limit: usize,
    },
    /// The input ended where more was needed.
    UnexpectedEnd {
        /// What was needed.
        expected: &'static str,
    },
    /// A byte that cannot come here.
    UnexpectedByte {
        /// The byte.
        byte: u8,
        /// What could come instead.
        expected: &'static str,
    },
    /// A control character (below U+0020) inside a string, which must be escaped.
    ControlCharacter {
        /// The byte.
        byte: u8,
    },
    /// Bytes inside a string that are not UTF-8. The position is the first bad byte.
    InvalidUtf8,
    /// A `\u` escape of a surrogate that is not half of a pair. The position is the escape's
    /// backslash (the first one, for a high surrogate not followed by a low one).
    LoneSurrogate {
        /// The escape's code unit.
        unit: u16,
    },
    /// A key that an object already has. The position is the second key's opening quote.
    DuplicateKey {
        /// The key, decoded.
        key: String,
    },
}

/// A refused document: what was wrong ([`JsonError::kind`]) and where.
///
/// The position is a byte offset, a line and a column, all pointing at the same place. Lines
/// and columns start at 1; a line ends at each line feed; a column counts characters (Unicode
/// scalar values) from the start of its line, so a character of several bytes is one column.
/// The offset of an error at the end of the input is the input's length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonError {
    kind: JsonErrorKind,
    offset: usize,
    line: usize,
    column: usize,
}

impl JsonError {
    /// An error of `kind` at byte `offset` of `bytes`, with its line and column worked out.
    pub(super) fn at(bytes: &[u8], offset: usize, kind: JsonErrorKind) -> Self {
        let before = bytes.get(..offset).unwrap_or(bytes);
        let line = 1 + before.iter().filter(|&&b| b == b'\n').count();
        let line_start = before.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        let column = 1 + before
            .get(line_start..)
            .unwrap_or_default()
            .iter()
            .filter(|&&b| b & 0xC0 != 0x80)
            .count();
        Self { kind, offset, line, column }
    }

    /// What was wrong.
    #[must_use]
    pub const fn kind(&self) -> &JsonErrorKind {
        &self.kind
    }

    /// The byte offset, from 0.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// The line, from 1.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }

    /// The column in characters, from 1.
    #[must_use]
    pub const fn column(&self) -> usize {
        self.column
    }
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, column {}: ", self.line, self.column)?;
        match &self.kind {
            JsonErrorKind::TooLarge { length, limit } => {
                write!(f, "the input is {length} bytes, over the limit of {limit}")
            }
            JsonErrorKind::TooDeep { limit } => {
                write!(f, "nesting deeper than the limit of {limit}")
            }
            JsonErrorKind::UnexpectedEnd { expected } => {
                write!(f, "the input ends where {expected} was expected")
            }
            JsonErrorKind::UnexpectedByte { byte, expected } => {
                write!(f, "byte 0x{byte:02X} where {expected} was expected")
            }
            JsonErrorKind::ControlCharacter { byte } => {
                write!(f, "control character 0x{byte:02X} in a string; escape it")
            }
            JsonErrorKind::InvalidUtf8 => write!(f, "bytes that are not UTF-8"),
            JsonErrorKind::LoneSurrogate { unit } => {
                write!(f, "the escape of surrogate U+{unit:04X} is not half of a pair")
            }
            JsonErrorKind::DuplicateKey { key } => write!(f, "the key {key:?} appears twice"),
        }
    }
}

impl std::error::Error for JsonError {}
