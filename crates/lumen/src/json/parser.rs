//! The reader's grammar level: [`parse_json`], an iterative parser with an explicit stack.

use std::collections::BTreeSet;

use super::error::{JsonError, JsonErrorKind};
use super::limits::JsonLimits;
use super::scan::Scanner;
use super::value::JsonValue;

/// A container being read.
enum Frame {
    Array(Vec<JsonValue>),
    Object { members: Vec<(String, JsonValue)>, keys: BTreeSet<String>, key: String },
}

/// Reads `bytes` as one JSON document under `limits`.
///
/// The input is checked against the size limit before anything is read, and each container
/// against the depth limit before anything is stored for it. The tree is built with an explicit
/// stack of at most `limits.max_depth()` frames, so no input recurses. See the
/// [module](self) for what is accepted and refused.
///
/// # Errors
///
/// A [`JsonError`] with the position of the first fault, in input order.
///
/// # Examples
///
/// ```
/// use lumen::{parse_json, JsonLimits, JsonValue};
///
/// let value = parse_json(br#"{"width": 320, "scale": 1.50}"#, JsonLimits::default())?;
/// assert_eq!(value.get("scale"), Some(&JsonValue::Number("1.50".to_owned())));
/// let error = parse_json(b"[1,\n 2,]", JsonLimits::default()).unwrap_err();
/// assert_eq!((error.line(), error.column()), (2, 4));
/// # Ok::<(), lumen::JsonError>(())
/// ```
pub fn parse_json(bytes: &[u8], limits: JsonLimits) -> Result<JsonValue, JsonError> {
    if bytes.len() > limits.max_bytes() {
        let kind = JsonErrorKind::TooLarge { length: bytes.len(), limit: limits.max_bytes() };
        return Err(JsonError::at(bytes, limits.max_bytes(), kind));
    }
    let mut scan = Scanner::new(bytes);
    let mut stack: Vec<Frame> = Vec::new();
    loop {
        scan.skip_white_space();
        let start = scan.pos;
        let mut value = match scan.peek() {
            Some(b'[') => {
                if stack.len() >= limits.max_depth() {
                    return scan.error(start, JsonErrorKind::TooDeep { limit: limits.max_depth() });
                }
                scan.pos += 1;
                scan.skip_white_space();
                if scan.peek() == Some(b']') {
                    scan.pos += 1;
                    JsonValue::Array(Vec::new())
                } else {
                    stack.push(Frame::Array(Vec::new()));
                    continue;
                }
            }
            Some(b'{') => {
                if stack.len() >= limits.max_depth() {
                    return scan.error(start, JsonErrorKind::TooDeep { limit: limits.max_depth() });
                }
                scan.pos += 1;
                scan.skip_white_space();
                if scan.peek() == Some(b'}') {
                    scan.pos += 1;
                    JsonValue::Object(Vec::new())
                } else {
                    let mut keys = BTreeSet::new();
                    let key = read_key(&mut scan, &mut keys, "a string key or '}'")?;
                    stack.push(Frame::Object { members: Vec::new(), keys, key });
                    continue;
                }
            }
            Some(b'"') => JsonValue::String(scan.string()?),
            Some(b't') => scan.word(b"true").map(|()| JsonValue::Bool(true))?,
            Some(b'f') => scan.word(b"false").map(|()| JsonValue::Bool(false))?,
            Some(b'n') => scan.word(b"null").map(|()| JsonValue::Null)?,
            Some(b'-' | b'0'..=b'9') => JsonValue::Number(scan.number()?),
            _ => return scan.unexpected("a value"),
        };
        // `value` is complete: attach it to the container it ends in, closing containers
        // whose last element it was, until one wants another element or none is open.
        loop {
            let Some(frame) = stack.last_mut() else {
                scan.skip_white_space();
                return if scan.pos == bytes.len() {
                    Ok(value)
                } else {
                    scan.unexpected("the end of the input")
                };
            };
            scan.skip_white_space();
            match frame {
                Frame::Array(items) => {
                    items.push(value);
                    match scan.peek() {
                        Some(b',') => {
                            scan.pos += 1;
                            break;
                        }
                        Some(b']') => {
                            scan.pos += 1;
                            value = JsonValue::Array(core::mem::take(items));
                        }
                        _ => return scan.unexpected("',' or ']'"),
                    }
                }
                Frame::Object { members, keys, key } => {
                    members.push((core::mem::take(key), value));
                    match scan.peek() {
                        Some(b',') => {
                            scan.pos += 1;
                            scan.skip_white_space();
                            *key = read_key(&mut scan, keys, "a string key")?;
                            break;
                        }
                        Some(b'}') => {
                            scan.pos += 1;
                            value = JsonValue::Object(core::mem::take(members));
                        }
                        _ => return scan.unexpected("',' or '}'"),
                    }
                }
            }
            stack.pop();
        }
    }
}

/// A member's key and the colon after it, the key's first byte at the cursor. A key already in
/// `keys` is refused at its opening quote.
fn read_key(
    scan: &mut Scanner<'_>,
    keys: &mut BTreeSet<String>,
    expected: &'static str,
) -> Result<String, JsonError> {
    if scan.peek() != Some(b'"') {
        return scan.unexpected(expected);
    }
    let start = scan.pos;
    let key = scan.string()?;
    if !keys.insert(key.clone()) {
        return scan.error(start, JsonErrorKind::DuplicateKey { key });
    }
    scan.skip_white_space();
    if scan.peek() != Some(b':') {
        return scan.unexpected("':'");
    }
    scan.pos += 1;
    Ok(key)
}
