//! This module owns reading JSON text into a tree, enough for `cargo metadata`'s output: objects,
//! arrays, strings with their escapes, numbers kept as written, `true`, `false` and `null`.

/// A JSON value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Json {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number, as written.
    Num(String),
    /// A string, escapes decoded.
    Str(String),
    /// An array.
    Arr(Vec<Json>),
    /// An object's members in order.
    Obj(Vec<(String, Json)>),
}

impl Json {
    /// The member `key` of an object, or `None`.
    pub(crate) fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The text of a string, or `None`.
    pub(crate) fn str(&self) -> Option<&str> {
        match self {
            Json::Str(text) => Some(text),
            _ => None,
        }
    }

    /// The items of an array, or an empty slice for anything else.
    pub(crate) fn items(&self) -> &[Json] {
        match self {
            Json::Arr(items) => items,
            _ => &[],
        }
    }
}

/// The value `text` holds, or where and why it is not JSON.
pub(crate) fn parse(text: &str) -> Result<Json, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut reader = Reader { chars: &chars, at: 0 };
    let value = reader.value(0)?;
    reader.space();
    match reader.chars.get(reader.at) {
        None => Ok(value),
        Some(c) => Err(format!("character {}: `{c}` after the value", reader.at)),
    }
}

/// The deepest nesting read (64 arrays or objects): `cargo metadata` nests a few levels;
/// anything deeper is refused rather than risk the stack.
const MAX_DEPTH: usize = 64;

/// The characters and the index of the next one.
struct Reader<'a> {
    chars: &'a [char],
    at: usize,
}

impl Reader<'_> {
    /// Skips whitespace.
    fn space(&mut self) {
        while self.chars.get(self.at).is_some_and(char::is_ascii_whitespace) {
            self.at += 1;
        }
    }

    /// A failure at the current character.
    fn fail(&self, what: &str) -> String {
        format!("character {}: {what}", self.at)
    }

    /// Takes `c` after whitespace, or fails.
    fn expect(&mut self, c: char) -> Result<(), String> {
        self.space();
        if self.chars.get(self.at) == Some(&c) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.fail(&format!("expected `{c}`")))
        }
    }

    /// The value at the current character, nested `depth` deep.
    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth >= MAX_DEPTH {
            return Err(self.fail("nested too deep"));
        }
        self.space();
        match self.chars.get(self.at) {
            Some('{') => self.object(depth),
            Some('[') => self.array(depth),
            Some('"') => self.string().map(Json::Str),
            Some(c) if *c == '-' || c.is_ascii_digit() => Ok(self.number()),
            _ => self.word(),
        }
    }

    /// An object, its `{` next.
    fn object(&mut self, depth: usize) -> Result<Json, String> {
        self.at += 1;
        let mut members = Vec::new();
        self.space();
        if self.chars.get(self.at) == Some(&'}') {
            self.at += 1;
            return Ok(Json::Obj(members));
        }
        loop {
            self.space();
            let key = self.string()?;
            self.expect(':')?;
            members.push((key, self.value(depth + 1)?));
            self.space();
            match self.chars.get(self.at) {
                Some(',') => self.at += 1,
                Some('}') => {
                    self.at += 1;
                    return Ok(Json::Obj(members));
                }
                _ => return Err(self.fail("expected `,` or `}`")),
            }
        }
    }

    /// An array, its `[` next.
    fn array(&mut self, depth: usize) -> Result<Json, String> {
        self.at += 1;
        let mut items = Vec::new();
        self.space();
        if self.chars.get(self.at) == Some(&']') {
            self.at += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.space();
            match self.chars.get(self.at) {
                Some(',') => self.at += 1,
                Some(']') => {
                    self.at += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return Err(self.fail("expected `,` or `]`")),
            }
        }
    }

    /// A string, its `"` next, with its escapes decoded (a `\u` pair of surrogates included).
    fn string(&mut self) -> Result<String, String> {
        if self.chars.get(self.at) != Some(&'"') {
            return Err(self.fail("expected a string"));
        }
        self.at += 1;
        let mut out = String::new();
        loop {
            let c = *self.chars.get(self.at).ok_or_else(|| self.fail("an unterminated string"))?;
            self.at += 1;
            match c {
                '"' => return Ok(out),
                '\\' => out.push(self.escape()?),
                c => out.push(c),
            }
        }
    }

    /// The character an escape stands for, its `\` read.
    fn escape(&mut self) -> Result<char, String> {
        let c = *self.chars.get(self.at).ok_or_else(|| self.fail("an unterminated escape"))?;
        self.at += 1;
        Ok(match c {
            '"' | '\\' | '/' => c,
            'b' => '\u{8}',
            'f' => '\u{c}',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'u' => {
                let high = self.hex4()?;
                let code = if (0xD800..0xDC00).contains(&high) {
                    if self.chars.get(self.at..self.at + 2) != Some(&['\\', 'u']) {
                        return Err(self.fail("a lone high surrogate"));
                    }
                    self.at += 2;
                    let low = self.hex4()?;
                    if !(0xDC00..0xE000).contains(&low) {
                        return Err(self.fail("a high surrogate without its low one"));
                    }
                    0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)
                } else {
                    high
                };
                char::from_u32(code).ok_or_else(|| self.fail("an escape that is no character"))?
            }
            _ => return Err(self.fail("an unknown escape")),
        })
    }

    /// Four hexadecimal digits.
    fn hex4(&mut self) -> Result<u32, String> {
        let digits: String = self.chars.get(self.at..self.at + 4).unwrap_or(&[]).iter().collect();
        self.at += 4;
        u32::from_str_radix(&digits, 16).map_err(|_| self.fail("expected four hex digits"))
    }

    /// A number, kept as written.
    fn number(&mut self) -> Json {
        let start = self.at;
        while self.chars.get(self.at).is_some_and(|c| "+-.eE".contains(*c) || c.is_ascii_digit()) {
            self.at += 1;
        }
        Json::Num(self.chars[start..self.at].iter().collect())
    }

    /// `true`, `false` or `null`.
    fn word(&mut self) -> Result<Json, String> {
        for (word, value) in
            [("true", Json::Bool(true)), ("false", Json::Bool(false)), ("null", Json::Null)]
        {
            let len = word.chars().count();
            if self
                .chars
                .get(self.at..self.at + len)
                .is_some_and(|w| w.iter().copied().eq(word.chars()))
            {
                self.at += len;
                return Ok(value);
            }
        }
        Err(self.fail("expected a value"))
    }
}
