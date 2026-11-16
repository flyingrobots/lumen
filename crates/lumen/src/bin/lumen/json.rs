//! The JSON the command writes (ADR 0015): objects, arrays, strings, integers, booleans and
//! `null`, and no float, so that every message is the same text on every platform.
//!
//! This is a writer only. Members keep the order they were added in, and a string is written
//! as RFC 8259 requires: `"` and `\` escaped, the control characters U+0000 to U+001F escaped
//! (`\b`, `\t`, `\n`, `\f` and `\r` in their short forms, the rest as `\u00XX` with lowercase
//! hex), and every other character, U+007F and non-ASCII included, written as itself in UTF-8.

use core::fmt::Write as _;

/// A JSON value, as the command writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Value {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A non-negative integer: every count, size, seed and time the command prints is one.
    Integer(u64),
    /// A string.
    String(String),
    /// An array.
    Array(Vec<Value>),
    /// An object.
    Object(Object),
}

/// A JSON object: its members in the order they were added.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Object(Vec<(String, Value)>);

impl Object {
    /// An object with no members.
    pub(crate) fn new() -> Self {
        Self(Vec::new())
    }

    /// This object with `key` added last, holding `value`.
    #[must_use]
    pub(crate) fn with(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.0.push((key.into(), value.into()));
        self
    }

    /// The members, in order.
    pub(crate) fn members(&self) -> &[(String, Value)] {
        &self.0
    }

    /// The object as one line of JSON, without a line end.
    pub(crate) fn to_json(&self) -> String {
        let mut out = String::new();
        write_object(&mut out, self);
        out
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<u64> for Value {
    fn from(value: u64) -> Self {
        Self::Integer(value)
    }
}

impl From<u32> for Value {
    fn from(value: u32) -> Self {
        Self::Integer(u64::from(value))
    }
}

impl From<usize> for Value {
    /// Lossless on every target Lumen builds for, none of whose `usize` is wider than 64 bits.
    fn from(value: usize) -> Self {
        Self::Integer(u64::try_from(value).unwrap_or(u64::MAX))
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<Object> for Value {
    fn from(value: Object) -> Self {
        Self::Object(value)
    }
}

impl From<Vec<Value>> for Value {
    fn from(value: Vec<Value>) -> Self {
        Self::Array(value)
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    /// `None` is `null`.
    fn from(value: Option<T>) -> Self {
        value.map_or(Self::Null, Into::into)
    }
}

/// An array of the values `items` gives.
pub(crate) fn array<T: Into<Value>>(items: impl IntoIterator<Item = T>) -> Value {
    Value::Array(items.into_iter().map(Into::into).collect())
}

fn write_value(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        // Writing to a `String` cannot fail.
        Value::Integer(number) => {
            let _ = write!(out, "{number}");
        }
        Value::String(text) => write_string(out, text),
        Value::Array(items) => {
            out.push('[');
            for (n, item) in items.iter().enumerate() {
                if n > 0 {
                    out.push(',');
                }
                write_value(out, item);
            }
            out.push(']');
        }
        Value::Object(object) => write_object(out, object),
    }
}

fn write_object(out: &mut String, object: &Object) {
    out.push('{');
    for (n, (key, value)) in object.0.iter().enumerate() {
        if n > 0 {
            out.push(',');
        }
        write_string(out, key);
        out.push(':');
        write_value(out, value);
    }
    out.push('}');
}

/// `text` as a JSON string, quotes included.
pub(crate) fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            '\u{0}'..='\u{1f}' => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests;
