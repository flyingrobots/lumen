//! This module owns which lines of a Markdown page are outside fenced code blocks.

/// The lines of `text` outside fenced code blocks, with their numbers from 1. A block opens at
/// a fence and closes at a fence of the same character, at least as long, with only whitespace
/// after it.
pub(super) fn prose_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut open: Option<(char, usize)> = None;
    text.lines().enumerate().filter_map(move |(index, line)| {
        match (open, fence(line)) {
            (None, Some((mark, length, _))) => open = Some((mark, length)),
            (Some((mark, length)), Some((closing, run, after)))
                if closing == mark && run >= length && after.trim().is_empty() =>
            {
                open = None;
            }
            (None, None) => return Some((index + 1, line)),
            _ => {}
        }
        None
    })
}

/// A fence, if `line` is one: at most three spaces, then three or more backticks or tildes.
/// Returns its character, its length and what follows it.
fn fence(line: &str) -> Option<(char, usize, &str)> {
    let rest = line.trim_start_matches(' ');
    if line.len() - rest.len() > 3 {
        return None;
    }
    let mark = rest.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let length = rest.chars().take_while(|&c| c == mark).count();
    (length >= 3).then(|| (mark, length, &rest[length..]))
}
