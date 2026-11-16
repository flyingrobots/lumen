//! The reader's lexical level: a cursor over the bytes that reads one string, number or word.

use super::error::{JsonError, JsonErrorKind};

/// A cursor over the input.
pub(super) struct Scanner<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) pos: usize,
}

type Scan<T> = Result<T, JsonError>;

impl<'a> Scanner<'a> {
    pub(super) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    pub(super) fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    pub(super) fn skip_white_space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    pub(super) fn error<T>(&self, offset: usize, kind: JsonErrorKind) -> Scan<T> {
        Err(JsonError::at(self.bytes, offset, kind))
    }

    /// The error for what is at the cursor: the end of the input, or a byte that cannot come.
    pub(super) fn unexpected<T>(&self, expected: &'static str) -> Scan<T> {
        match self.peek() {
            None => self.error(self.pos, JsonErrorKind::UnexpectedEnd { expected }),
            Some(byte) => self.error(self.pos, JsonErrorKind::UnexpectedByte { byte, expected }),
        }
    }

    /// The word `true`, `false` or `null`, whose first byte is at the cursor.
    pub(super) fn word(&mut self, word: &'static [u8]) -> Scan<()> {
        for &expected in word {
            if self.peek() != Some(expected) {
                return self.unexpected("the rest of a word such as true, false or null");
            }
            self.pos += 1;
        }
        Ok(())
    }

    fn digits(&mut self) -> Scan<()> {
        if !matches!(self.peek(), Some(b'0'..=b'9')) {
            return self.unexpected("a digit");
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        Ok(())
    }

    /// A number token, validated against the JSON grammar and returned as written.
    pub(super) fn number(&mut self) -> Scan<String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        if self.peek() == Some(b'0') {
            self.pos += 1;
        } else {
            self.digits()?;
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            self.digits()?;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            self.digits()?;
        }
        let token = self.bytes.get(start..self.pos).unwrap_or_default();
        // The token is ASCII by construction; the check only keeps this function total.
        match core::str::from_utf8(token) {
            Ok(text) => Ok(text.to_owned()),
            Err(_) => self.error(start, JsonErrorKind::InvalidUtf8),
        }
    }

    /// Four hexadecimal digits after `\u`, as a code unit.
    fn hex4(&mut self) -> Scan<u16> {
        let mut unit = 0u16;
        for _ in 0..4 {
            let digit = match self.peek() {
                Some(b @ b'0'..=b'9') => b - b'0',
                Some(b @ b'a'..=b'f') => b - b'a' + 10,
                Some(b @ b'A'..=b'F') => b - b'A' + 10,
                _ => return self.unexpected("a hexadecimal digit"),
            };
            unit = unit * 16 + u16::from(digit);
            self.pos += 1;
        }
        Ok(unit)
    }

    /// The character of a `\u` escape whose backslash is at `backslash` and whose four digits
    /// were read: the character itself, or a pair's second escape too for a high surrogate.
    fn unicode_escape(&mut self, backslash: usize) -> Scan<char> {
        let unit = self.hex4()?;
        let lone = JsonErrorKind::LoneSurrogate { unit };
        let combined = match unit {
            0xD800..=0xDBFF => {
                if self.peek().is_none() {
                    return self.unexpected("the low half of a surrogate pair");
                }
                let second = self.pos;
                if self.bytes.get(second..second + 2) != Some(b"\\u") {
                    return self.error(backslash, lone);
                }
                self.pos += 2;
                let low = self.hex4()?;
                if !(0xDC00..=0xDFFF).contains(&low) {
                    return self.error(backslash, lone);
                }
                0x1_0000 + ((u32::from(unit) - 0xD800) << 10) + (u32::from(low) - 0xDC00)
            }
            0xDC00..=0xDFFF => return self.error(backslash, lone),
            _ => u32::from(unit),
        };
        match char::from_u32(combined) {
            Some(c) => Ok(c),
            None => self.error(backslash, lone),
        }
    }

    /// A string whose opening quote is at the cursor, with its escapes decoded.
    pub(super) fn string(&mut self) -> Scan<String> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            let run = self.pos;
            while let Some(b) = self.peek() {
                if b == b'"' || b == b'\\' || b < 0x20 {
                    break;
                }
                self.pos += 1;
            }
            let raw = self.bytes.get(run..self.pos).unwrap_or_default();
            match core::str::from_utf8(raw) {
                Ok(text) => out.push_str(text),
                Err(e) => return self.error(run + e.valid_up_to(), JsonErrorKind::InvalidUtf8),
            }
            match self.peek() {
                None => return self.unexpected("the closing quote of a string"),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    let backslash = self.pos;
                    self.pos += 1;
                    let c = match self.peek() {
                        Some(b'"') => '"',
                        Some(b'\\') => '\\',
                        Some(b'/') => '/',
                        Some(b'b') => '\u{8}',
                        Some(b'f') => '\u{c}',
                        Some(b'n') => '\n',
                        Some(b'r') => '\r',
                        Some(b't') => '\t',
                        Some(b'u') => {
                            self.pos += 1;
                            out.push(self.unicode_escape(backslash)?);
                            continue;
                        }
                        _ => return self.unexpected("an escape: one of \" \\ / b f n r t u"),
                    };
                    self.pos += 1;
                    out.push(c);
                }
                Some(byte) => {
                    return self.error(self.pos, JsonErrorKind::ControlCharacter { byte });
                }
            }
        }
    }
}
