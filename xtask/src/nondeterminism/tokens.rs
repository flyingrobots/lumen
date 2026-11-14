//! Rust source as tokens, on the file-length gate's lexer: comments dropped, string literals
//! kept whole, numbers kept whole with their float form, everything else one character.

use crate::file_length::count::{step, State};

/// One token of a Rust file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Token {
    /// An identifier or keyword.
    Ident(String),
    /// A number literal, suffix included (`0.5`, `1e3`, `2f64`, `0xf64`).
    Num(String),
    /// The contents of a string literal, raw or not, escapes as written.
    Str(String),
    /// Any other character outside whitespace, comments and character literals.
    Punct(char),
}

/// A token and the 1-based line it starts on.
pub(crate) type Located = (usize, Token);

/// The tokens of `text`. Character literals and lifetimes' quotes are dropped; a tuple index
/// (`p.0.1`) is a number, a `.` and a number, never a float.
pub(crate) fn tokens(text: &str) -> Vec<Located> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Lexer { out: Vec::new(), word: String::new(), line: 1, word_line: 1 };
    let (mut state, mut at, mut literal) = (State::Code, 0, String::new());
    while let Some(&c) = chars.get(at) {
        if c == '\n' {
            out.flush();
            if matches!(state, State::Str | State::RawStr(_)) {
                literal.push(c);
            } else if state == State::LineComment {
                state = State::Code;
            }
            out.line += 1;
            at += 1;
            continue;
        }
        let rest = &chars[at..];
        let (next, taken, _) = step(state, rest);
        match (state, next) {
            (State::Code, State::Code) if taken == 1 => out.code(c, rest.get(1).copied()),
            (State::Str | State::RawStr(_), State::Code) => {
                out.out.push((out.word_line, Token::Str(core::mem::take(&mut literal))));
            }
            (State::Str | State::RawStr(_), _) => literal.extend(&rest[..taken]),
            (State::Code, State::Str | State::RawStr(_)) => {
                out.flush();
                out.word_line = out.line;
            }
            _ => out.flush(),
        }
        state = next;
        at += taken;
    }
    out.flush();
    out.out
}

/// The tokens so far and the identifier or number being read.
struct Lexer {
    out: Vec<Located>,
    word: String,
    line: usize,
    word_line: usize,
}

impl Lexer {
    /// One character of code, with the character after it.
    fn code(&mut self, c: char, after: Option<char>) {
        let number = self.word.starts_with(|d: char| d.is_ascii_digit());
        let digit_after = after.is_some_and(|a| a.is_ascii_digit());
        let joins = if c.is_alphanumeric() || c == '_' {
            true
        } else if number && c == '.' {
            // `1.5` and `1.` join; `1..2`, `1.max(2)` and a tuple index `p.0.1` do not.
            let tuple_index = matches!(self.out.last(), Some((_, Token::Punct('.'))));
            !tuple_index
                && !self.word.contains('.')
                && after.is_none_or(|a| digit_after || !(a == '.' || a.is_alphabetic() || a == '_'))
        } else {
            // The sign of an exponent: `1e-3`.
            number && matches!(c, '+' | '-') && self.word.ends_with(['e', 'E']) && digit_after
        };
        if joins {
            if self.word.is_empty() {
                self.word_line = self.line;
            }
            self.word.push(c);
            return;
        }
        self.flush();
        if !c.is_whitespace() {
            self.out.push((self.line, Token::Punct(c)));
        }
    }

    /// Ends the word being read, if any.
    fn flush(&mut self) {
        if self.word.is_empty() {
            return;
        }
        let word = core::mem::take(&mut self.word);
        let token = if word.starts_with(|d: char| d.is_ascii_digit()) {
            Token::Num(word)
        } else {
            Token::Ident(word)
        };
        self.out.push((self.word_line, token));
    }
}

/// Whether a number literal is a float: decimal, with a fraction, an exponent or a float suffix.
pub(crate) fn is_float(number: &str) -> bool {
    let digits = number.replace('_', "");
    if digits.starts_with("0x") || digits.starts_with("0o") || digits.starts_with("0b") {
        return false;
    }
    let exponent = digits.char_indices().any(|(i, c)| {
        matches!(c, 'e' | 'E')
            && digits[i + 1..].starts_with(|d: char| d.is_ascii_digit() || d == '+' || d == '-')
    });
    digits.contains('.') || digits.ends_with("f32") || digits.ends_with("f64") || exponent
}
