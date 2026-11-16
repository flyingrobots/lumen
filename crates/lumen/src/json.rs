//! A JSON reader that keeps numbers as text (`lumen-json-reader/1`, flyingrobots/lumen#211).
//!
//! [`parse_json`] reads a document from bytes into a [`JsonValue`] tree. It exists so that job
//! files (ADR 0015) and, later, scene files are read by Lumen's own code (R-DEP-1: no third-party
//! crate), and it contains no arithmetic and no float. A number is not converted: the tree holds
//! the token as written, after it has been checked against the grammar of RFC 8259, so a later
//! layer converts a decimal exactly (Bunny's `bunny-decimal-q32-32/1`) and never through a float.
//! The reader reads no file and asks the host nothing; the caller passes the bytes.
//!
//! What it decides, for a document that is valid JSON by RFC 8259:
//!
//! - The input is bytes and must be UTF-8. A byte order mark, a UTF-16 document and any invalid
//!   sequence are refused. `\u` escapes are decoded, a surrogate pair into one character, and a
//!   lone surrogate is refused.
//! - A duplicate key in one object is refused (RFC 8259 leaves it open). Keys are compared after
//!   their escapes are decoded. An object keeps its members in document order.
//! - A number is `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?` and nothing else. Its
//!   size and magnitude are not limited beyond the document's size: `1E400` and a 1 000-digit
//!   integer are tokens like any other, and `-0` stays `-0`.
//! - The document is one value with optional white space (space, tab, line feed, carriage
//!   return) around it; anything after it is refused.
//! - Limits are parameters ([`JsonLimits`]). A document longer than `max_bytes` is refused
//!   before it is read, and a container that would nest deeper than `max_depth` is refused
//!   before anything is stored for it. Depth counts open containers, so `max_depth` 2 admits
//!   `[[]]` and refuses `[[[]]]`; a scalar has depth 0. The reader does not recurse, and the
//!   depth limit has a ceiling ([`JSON_MAX_DEPTH_CEILING`]) because dropping or comparing a deep
//!   tree does recurse.
//! - Every error carries a byte offset, a 1-based line and a 1-based column ([`JsonError`]). A
//!   column counts characters (Unicode scalar values), not bytes; a line ends at each line feed
//!   (a carriage return is a character of its line).
//! - No input panics: every index is checked and every arithmetic step is bounded by the
//!   document's length.

mod error;
mod limits;
mod parser;
mod scan;
mod value;

#[cfg(test)]
mod conformance;
#[cfg(test)]
mod generated;
#[cfg(test)]
mod tests;

pub use error::{JsonError, JsonErrorKind};
pub use limits::{
    JsonLimitError, JsonLimits, JSON_DEFAULT_MAX_BYTES, JSON_DEFAULT_MAX_DEPTH,
    JSON_MAX_DEPTH_CEILING,
};
pub use parser::parse_json;
pub use value::JsonValue;

/// The identity of this reader's contract: the grammar, the UTF-8 rule, the refusals above and
/// the position rule. A change to any of them is a new identity.
pub const JSON_READER_ID: &str = "lumen-json-reader/1";
