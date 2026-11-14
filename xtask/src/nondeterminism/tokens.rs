//! Rust source as tokens, on the file-length gate's lexer: comments dropped, string literals
//! kept whole, numbers kept whole with their float form, raw identifiers without their `r#`,
//! everything else one character. A leading byte-order mark and shebang line are skipped, as
//! `rustc` skips them ([`code_start`]).

#[cfg(test)]
mod tests;

use crate::file_length::count::{code_start, step, State};

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
    let (mut state, mut at, mut literal) = (State::Code, code_start(&chars), String::new());
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
        let starts_word = after.is_some_and(|a| a.is_alphabetic() || a == '_');
        if c == '#' && self.word == "r" && starts_word {
            // A raw identifier, `r#fs`, is the identifier `fs`.
            self.word.clear();
            return;
        }
        let number = self.word.starts_with(|d: char| d.is_ascii_digit());
        let joins = if c.is_alphanumeric() || c == '_' {
            true
        } else if number && c == '.' {
            // `1.5` and `1.` join; `1..2`, `1.max(2)`, `1._x` and a tuple index `p.0.1` do not.
            !self.after_field_dot()
                && split_digits(&self.word).1.is_empty()
                && after.is_none_or(|a| a != '.' && !(a.is_alphabetic() || a == '_'))
        } else {
            // The sign of an exponent: `1e-3`, `2.5E+7`, `1e+_3`.
            matches!(c, '+' | '-')
                && awaits_exponent_sign(&self.word)
                && after.is_some_and(|a| a.is_ascii_digit() || a == '_')
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

    /// Whether the number being read follows a lone `.` (a field access, so a tuple index),
    /// not the `..` of a range or `...`.
    fn after_field_dot(&self) -> bool {
        let before = |back: usize| self.out.len().checked_sub(back).map(|i| &self.out[i].1);
        before(1) == Some(&Token::Punct('.')) && before(2) != Some(&Token::Punct('.'))
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

/// Whether a number literal starts `0x`, `0o` or `0b`: it is then an integer whatever follows.
fn radix_prefixed(number: &str) -> bool {
    ["0x", "0o", "0b"].iter().any(|prefix| number.starts_with(prefix))
}

/// The decimal digits and `_`s at the start of `text`, and the rest.
fn split_digits(text: &str) -> (&str, &str) {
    text.split_at(text.find(|c: char| !(c.is_ascii_digit() || c == '_')).unwrap_or(text.len()))
}

/// Whether `word` is a decimal literal up to an exponent's `e`, so a sign may follow:
/// `1e`, `1_e`, `2.5E`, but not `1usize` or `0x1e`.
fn awaits_exponent_sign(word: &str) -> bool {
    let (_, rest) = split_digits(word);
    let rest = rest.strip_prefix('.').map_or(rest, |fraction| split_digits(fraction).1);
    word.starts_with(|d: char| d.is_ascii_digit()) && matches!(rest, "e" | "E")
}

/// Whether a number literal is a float, by the Rust reference's grammar: a decimal literal with a
/// fraction (`1.5`, `1.`), with an exponent (`1e3`, `2.5E-7`, `1e+_3`), or with the suffix `f32`
/// or `f64` (`1f32`, `2_f64`). A literal starting `0x`, `0o` or `0b` never is.
pub(crate) fn is_float(number: &str) -> bool {
    if radix_prefixed(number) {
        return false;
    }
    let (_, rest) = split_digits(number);
    if rest.starts_with('.') {
        return true;
    }
    let exponent = rest.strip_prefix(['e', 'E']).map(|e| e.strip_prefix(['+', '-']).unwrap_or(e));
    if let Some(digits) = exponent {
        return split_digits(digits).0.contains(|d: char| d.is_ascii_digit());
    }
    matches!(rest, "f32" | "f64")
}
