//! The tree the reader returns: [`JsonValue`].

/// A JSON value. A number is its source text, validated and unconverted.
///
/// An object keeps its members in document order, with no key twice (the reader refuses a
/// duplicate). Equality is structural, so `1.0` and `1` are different numbers here: the reader
/// has no opinion on what a token means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JsonValue {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number token exactly as written, for example `-0`, `1.50` or `1E400`.
    Number(String),
    /// A string with its escapes decoded.
    String(String),
    /// An array's elements in order.
    Array(Vec<JsonValue>),
    /// An object's members in document order.
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    /// The member of an object named `key`, or `None` for another kind of value or a missing key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The text of a string, or `None` for another kind of value.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    /// The source text of a number, or `None` for another kind of value.
    #[must_use]
    pub fn as_number(&self) -> Option<&str> {
        match self {
            Self::Number(text) => Some(text),
            _ => None,
        }
    }

    /// The elements of an array, or `None` for another kind of value.
    #[must_use]
    pub fn as_array(&self) -> Option<&[Self]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    /// The members of an object in document order, or `None` for another kind of value.
    #[must_use]
    pub fn as_object(&self) -> Option<&[(String, Self)]> {
        match self {
            Self::Object(members) => Some(members),
            _ => None,
        }
    }
}
