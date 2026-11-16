//! A JSON parser and a JSON Schema validator for the tests, written from RFC 8259 and JSON Schema
//! 2020-12 and sharing no code with the command's writer (`src/bin/lumen/json.rs`), so that a
//! message the writer gets wrong is not read back the same wrong way.
//!
//! The parser takes the whole grammar of RFC 8259: any whitespace, every escape including
//! surrogate pairs, and numbers with fractions and exponents (kept as text, since the messages
//! hold none). It refuses what RFC 8259 refuses, and also a repeated key in one object, which
//! the RFC allows but no message may have.
//!
//! The validator takes the subset of JSON Schema 2020-12 that `schemas/lumen-message-1.schema.json`
//! uses, and refuses a schema with any other keyword, so the schema cannot use a keyword this
//! validator would silently skip. The subset: `type` (one name or a list), `const`, `enum`,
//! `properties`, `required`, `additionalProperties` (`true` or `false`), `items`, `minItems`,
//! `maxItems`, `minimum`, `maximum`, `minLength`, `maxLength`, `allOf`, `if` with `then`,
//! `$ref` to `#/$defs/NAME`, and the annotations `$schema`, `$id`, `$defs`, `title` and
//! `description`. `integer` means a number written without a fraction or an exponent, which is
//! stricter than JSON Schema (which also counts `1.0`), and is how the command writes them.

#![allow(dead_code, reason = "each test crate that includes this module uses only what it needs")]

/// A parsed JSON value. An object keeps its members in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    /// A number written as an integer, if it fits an `i128`.
    Integer(i128),
    /// Any other number, as written.
    Number(String),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    /// The member `key` of an object, or `None`.
    pub(crate) fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The member at `path`, keys separated by `.`; panics with the path if there is none.
    pub(crate) fn at(&self, path: &str) -> &Json {
        path.split('.').fold(self, |value, key| {
            value.get(key).unwrap_or_else(|| panic!("no member {path:?} in {value:?}"))
        })
    }

    /// The string at `path`.
    pub(crate) fn str(&self, path: &str) -> &str {
        match self.at(path) {
            Json::String(text) => text,
            other => panic!("{path}: not a string: {other:?}"),
        }
    }

    /// The integer at `path`.
    pub(crate) fn int(&self, path: &str) -> i128 {
        match self.at(path) {
            Json::Integer(n) => *n,
            other => panic!("{path}: not an integer: {other:?}"),
        }
    }

    /// The array at `path`.
    pub(crate) fn array(&self, path: &str) -> &[Json] {
        match self.at(path) {
            Json::Array(items) => items,
            other => panic!("{path}: not an array: {other:?}"),
        }
    }

    /// The member names of an object, in order.
    pub(crate) fn keys(&self) -> Vec<&str> {
        match self {
            Json::Object(members) => members.iter().map(|(k, _)| k.as_str()).collect(),
            _ => Vec::new(),
        }
    }
}

/// Parses `text` as exactly one JSON value, with optional whitespace around it.
pub(crate) fn parse(text: &str) -> Result<Json, String> {
    let mut parser = Parser { bytes: text.as_bytes(), at: 0 };
    parser.space();
    let value = parser.value(0)?;
    parser.space();
    if parser.at != parser.bytes.len() {
        return Err(format!("text after the value at byte {}", parser.at));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}

/// Deeper nesting than any message has; a guard against runaway recursion.
const MAX_DEPTH: usize = 64;

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn fail<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("{what} at byte {}", self.at))
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> Result<(), String> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            self.fail(&format!("expected {:?}", char::from(byte)))
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            self.fail("unknown literal")
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > MAX_DEPTH {
            return self.fail("too deep");
        }
        match self.peek() {
            Some(b'n') => self.literal("null", Json::Null),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'"') => self.string().map(Json::String),
            Some(b'[') => self.array(depth),
            Some(b'{') => self.object(depth),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => self.fail("expected a value"),
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, String> {
        self.eat(b'[')?;
        self.space();
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Json::Array(items));
        }
        loop {
            self.space();
            items.push(self.value(depth + 1)?);
            self.space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Json::Array(items));
                }
                _ => return self.fail("expected , or ]"),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, String> {
        self.eat(b'{')?;
        self.space();
        let mut members: Vec<(String, Json)> = Vec::new();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Json::Object(members));
        }
        loop {
            self.space();
            if self.peek() != Some(b'"') {
                return self.fail("expected a key");
            }
            let key = self.string()?;
            if members.iter().any(|(k, _)| *k == key) {
                return self.fail(&format!("repeated key {key:?}"));
            }
            self.space();
            self.eat(b':')?;
            self.space();
            let value = self.value(depth + 1)?;
            members.push((key, value));
            self.space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Json::Object(members));
                }
                _ => return self.fail("expected , or }"),
            }
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.at;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        self.at - start
    }

    /// `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`
    fn number(&mut self) -> Result<Json, String> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        match self.peek() {
            Some(b'0') => {
                self.at += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return self.fail("leading zero");
                }
            }
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return self.fail("expected a digit"),
        }
        let mut integer = true;
        if self.peek() == Some(b'.') {
            self.at += 1;
            integer = false;
            if self.digits() == 0 {
                return self.fail("expected a fraction digit");
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            integer = false;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if self.digits() == 0 {
                return self.fail("expected an exponent digit");
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).expect("ASCII");
        Ok(match (integer, text.parse::<i128>()) {
            (true, Ok(n)) => Json::Integer(n),
            _ => Json::Number(text.to_owned()),
        })
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let Some(digits) = self.bytes.get(self.at..self.at + 4) else {
            return self.fail("short \\u escape");
        };
        let mut value = 0;
        for &d in digits {
            let digit = match d {
                b'0'..=b'9' => d - b'0',
                b'a'..=b'f' => d - b'a' + 10,
                b'A'..=b'F' => d - b'A' + 10,
                _ => return self.fail("bad hex digit"),
            };
            value = value * 16 + u32::from(digit);
        }
        self.at += 4;
        Ok(value)
    }

    fn string(&mut self) -> Result<String, String> {
        self.eat(b'"')?;
        let mut out = String::new();
        loop {
            let Some(byte) = self.peek() else { return self.fail("unterminated string") };
            match byte {
                b'"' => {
                    self.at += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.at += 1;
                    let Some(escape) = self.peek() else { return self.fail("bad escape") };
                    self.at += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.unicode_escape()?),
                        _ => return self.fail("unknown escape"),
                    }
                }
                0x00..=0x1f => return self.fail("unescaped control character"),
                _ => {
                    // One UTF-8 character: the input is a `&str`, so it is well formed.
                    let rest = std::str::from_utf8(&self.bytes[self.at..]).expect("utf-8");
                    let c = rest.chars().next().expect("a character");
                    out.push(c);
                    self.at += c.len_utf8();
                }
            }
        }
    }

    /// The character of a `\uXXXX` escape, or of a surrogate pair of two.
    fn unicode_escape(&mut self) -> Result<char, String> {
        let first = self.hex4()?;
        let code = match first {
            0xd800..=0xdbff => {
                if !self.bytes[self.at..].starts_with(b"\\u") {
                    return self.fail("lone high surrogate");
                }
                self.at += 2;
                let second = self.hex4()?;
                if !(0xdc00..=0xdfff).contains(&second) {
                    return self.fail("high surrogate without a low one");
                }
                0x10000 + ((first - 0xd800) << 10) + (second - 0xdc00)
            }
            0xdc00..=0xdfff => return self.fail("lone low surrogate"),
            code => code,
        };
        char::from_u32(code).map_or_else(|| self.fail("not a scalar value"), Ok)
    }
}

mod schema;

pub(crate) use schema::{validate, Schema};

/// The committed schema of every line on standard output.
pub(crate) const MESSAGE_SCHEMA: &str = include_str!("../../../../schemas/lumen-message-1.schema.json");

/// The message types the command prints, written out here from ADR 0015 rather than read from
/// the schema or the command, so that a line of any other type fails.
pub(crate) const MESSAGE_TYPES: [&str; 9] = [
    "provenance",
    "result",
    "timings",
    "scene_statistics",
    "ray_statistics",
    "traversal_statistics",
    "run_statistics",
    "help",
    "version",
];

/// Every line of `stdout`, parsed and checked: the text is empty or ends in a line end; each
/// line is one JSON object whose `schema` is `lumen-message/1` and whose `type` is one of
/// [`MESSAGE_TYPES`]; and each validates against the committed schema. Panics on the first line
/// that does not, with the line.
pub(crate) fn messages(stdout: &str) -> Vec<Json> {
    let schema = Schema::new(MESSAGE_SCHEMA).expect("the committed schema is in the subset");
    assert!(stdout.is_empty() || stdout.ends_with('\n'), "no line end at the end: {stdout:?}");
    stdout
        .split_terminator('\n')
        .map(|line| {
            let value = parse(line).unwrap_or_else(|error| panic!("{error}: {line:?}"));
            assert!(matches!(value, Json::Object(_)), "not an object: {line}");
            assert_eq!(value.get("schema"), Some(&Json::String("lumen-message/1".into())), "{line}");
            let kind = value.str("type");
            assert!(MESSAGE_TYPES.contains(&kind), "unknown type {kind:?}: {line}");
            let errors = validate(&schema, &value);
            assert!(errors.is_empty(), "{line}\nbreaks the schema: {errors:#?}");
            value
        })
        .collect()
}

/// The one message of type `kind` in `messages`; panics if there is none or more than one.
pub(crate) fn one<'a>(messages: &'a [Json], kind: &str) -> &'a Json {
    let found: Vec<&Json> = messages.iter().filter(|m| m.str("type") == kind).collect();
    assert_eq!(found.len(), 1, "{} messages of type {kind} in {messages:?}", found.len());
    found[0]
}

/// The types of `messages`, in order.
pub(crate) fn types(messages: &[Json]) -> Vec<&str> {
    messages.iter().map(|m| m.str("type")).collect()
}
