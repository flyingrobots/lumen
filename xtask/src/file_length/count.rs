//! Counting the lines of a Rust file that hold code.

/// Where the scanner is: in code, in a `//` comment, in a `/* */` comment (nested to a depth),
/// in a string, or in a raw string with a number of `#`s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
    Code,
    LineComment,
    BlockComment(u32),
    Str,
    RawStr(usize),
}

/// The lines of `text` that hold code: at least one character that is not whitespace and is
/// not inside a comment. Doc comments are comments. A line inside a string literal counts when
/// it has a character that is not whitespace.
pub(crate) fn code_lines(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let (mut state, mut at, mut count, mut has_code) = (State::Code, code_start(&chars), 0, false);
    while let Some(&c) = chars.get(at) {
        if c == '\n' {
            count += usize::from(has_code);
            has_code = false;
            if state == State::LineComment {
                state = State::Code;
            }
            at += 1;
            continue;
        }
        let (next, taken, code) = step(state, &chars[at..]);
        has_code |= code;
        state = next;
        at += taken;
    }
    count + usize::from(has_code)
}

/// Where a file's tokens start, as `rustc` reads it: after a byte-order mark, and after a
/// shebang line. A first line starting `#!` is a shebang unless the first token after the `#!`,
/// past whitespace and comments that are not doc comments, is `[` (then it is an inner
/// attribute). The index returned is that of the shebang's newline, so the line still counts.
pub(crate) fn code_start(chars: &[char]) -> usize {
    let bom = usize::from(chars.first() == Some(&'\u{feff}'));
    let rest = &chars[bom..];
    if !rest.starts_with(&['#', '!']) || attribute_follows(&rest[2..]) {
        return bom;
    }
    bom + rest.iter().position(|&c| c == '\n').unwrap_or(rest.len())
}

/// Whether the first token of `rest`, past whitespace and comments that are not doc comments,
/// is `[`.
fn attribute_follows(rest: &[char]) -> bool {
    let mut at = 0;
    loop {
        let tail = &rest[at..];
        let doc = |third: char| tail.get(2) == Some(&third) && tail.get(3) != Some(&third);
        at += match tail {
            [c, ..] if c.is_whitespace() => 1,
            ['/', '/' | '*', '!', ..] => return false,
            ['/', '/', ..] if !doc('/') => tail.iter().take_while(|&&c| c != '\n').count(),
            ['/', '*', '*', '/', ..] => 4,
            ['/', '*', ..] if !doc('*') => match block_comment_len(tail) {
                Some(len) => len,
                None => return false,
            },
            ['[', ..] => return true,
            _ => return false,
        };
    }
}

/// The length of the (nested) block comment that starts `rest`, if it is closed.
fn block_comment_len(rest: &[char]) -> Option<usize> {
    let (mut state, mut at) = (State::BlockComment(1), 2);
    while state != State::Code {
        let (next, taken, _) = step(state, rest.get(at..).filter(|r| !r.is_empty())?);
        (state, at) = (next, at + taken);
    }
    Some(at)
}

/// One step from `state` at the start of `rest` (never at a newline): the next state, how many
/// characters were taken (at least one, never a newline past the first) and whether they were
/// code. The nondeterminism scan's tokenizer steps with it too.
pub(crate) fn step(state: State, rest: &[char]) -> (State, usize, bool) {
    let starts = |s: &str| s.chars().zip(rest).filter(|(a, b)| a == *b).count() == s.len();
    let blank = rest[0].is_whitespace();
    match state {
        State::Code if blank => (State::Code, 1, false),
        State::Code if starts("//") => (State::LineComment, 2, false),
        State::Code if starts("/*") => (State::BlockComment(1), 2, false),
        State::Code => code_step(rest),
        State::LineComment => (State::LineComment, 1, false),
        State::BlockComment(depth) if starts("*/") => {
            (if depth == 1 { State::Code } else { State::BlockComment(depth - 1) }, 2, false)
        }
        State::BlockComment(depth) if starts("/*") => (State::BlockComment(depth + 1), 2, false),
        State::BlockComment(_) => (state, 1, false),
        State::Str if rest[0] == '\\' && rest.get(1).is_some_and(|&c| c != '\n') => {
            (State::Str, 2, true)
        }
        State::Str if rest[0] == '"' => (State::Code, 1, true),
        State::RawStr(hashes) if raw_end(rest, hashes) => (State::Code, 1 + hashes, true),
        State::Str | State::RawStr(_) => (state, 1, !blank),
    }
}

/// A step in code that is not whitespace or the start of a comment: a string, a raw string, a
/// character literal or any other token's character.
fn code_step(rest: &[char]) -> (State, usize, bool) {
    match rest[0] {
        '"' => (State::Str, 1, true),
        'r' => match raw_start(rest) {
            Some(hashes) => (State::RawStr(hashes), 2 + hashes, true),
            None => (State::Code, 1, true),
        },
        '\'' => (State::Code, char_literal_len(rest), true),
        _ => (State::Code, 1, true),
    }
}

/// The number of `#`s of a raw string starting at `rest` (`r"`, `r#"`, ...), if one does.
fn raw_start(rest: &[char]) -> Option<usize> {
    let hashes = rest[1..].iter().take_while(|&&c| c == '#').count();
    (rest.get(1 + hashes) == Some(&'"')).then_some(hashes)
}

/// Whether `rest` closes a raw string with `hashes` `#`s.
fn raw_end(rest: &[char], hashes: usize) -> bool {
    rest[0] == '"' && rest.len() > hashes && rest[1..=hashes].iter().all(|&c| c == '#')
}

/// The length of a character literal at `rest` (`'x'`, `'\n'`, `'\u{..}'`), or 1 for a lifetime's
/// quote. Never past the end of the line.
fn char_literal_len(rest: &[char]) -> usize {
    match rest.get(1) {
        Some('\\') => rest
            .iter()
            .skip(3)
            .take_while(|&&c| c != '\n')
            .position(|&c| c == '\'')
            .map_or(1, |end| end + 4),
        Some(&c) if c != '\n' && rest.get(2) == Some(&'\'') => 3,
        _ => 1,
    }
}
