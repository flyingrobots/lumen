//! This module owns the anchors GitHub gives a Markdown page's headings. A heading is an ATX
//! heading (`# Title`) or a Setext heading (a paragraph underlined by `===` or `---`). Its anchor
//! is made from the text it renders to: code spans as written, a link as its text, HTML tags
//! dropped, and the underscores that mark emphasis dropped, judged over the whole heading
//! ([`super::emphasis`]); then lowercased, every character that is not a letter, a digit, `-`,
//! `_` or a space dropped, spaces turned into `-`. As GitHub's slugger does, an anchor already
//! taken gets its base's next free `-1`, `-2`, ...

use std::collections::BTreeMap;

use super::blocks::prose_lines;
use super::emphasis;

/// Stands in for a code span while emphasis is judged: a character that is neither a letter, a
/// digit nor whitespace, as the span's backtick is.
const CODE: char = '\u{E000}';

/// The anchors GitHub gives the headings of `text`, in order.
pub(crate) fn anchors(text: &str) -> Vec<String> {
    let mut made: Vec<String> = Vec::new();
    let mut counters: BTreeMap<String, usize> = BTreeMap::new();
    for title in titles(text) {
        let base = slug(&heading_text(&title));
        let mut anchor = base.clone();
        while made.contains(&anchor) {
            let counter = counters.entry(base.clone()).or_insert(0);
            *counter += 1;
            anchor = format!("{base}-{counter}");
        }
        made.push(anchor);
    }
    made
}

/// The titles of `text`'s headings, in order: each ATX heading's, and each Setext heading's
/// paragraph lines joined by spaces. A paragraph is consecutive non-blank prose lines that do
/// not start a list item, a table row, a block quote or a heading.
fn titles(text: &str) -> Vec<String> {
    let mut titles = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let mut last = 0;
    for (number, line) in prose_lines(text) {
        if number != last + 1 {
            paragraph.clear();
        }
        last = number;
        if let Some(title) = atx_heading(line) {
            titles.push(title.to_owned());
            paragraph.clear();
        } else if setext_underline(line) && !paragraph.is_empty() {
            titles.push(paragraph.iter().map(|l| l.trim()).collect::<Vec<_>>().join(" "));
            paragraph.clear();
        } else if line.trim().is_empty()
            || setext_underline(line)
            || (paragraph.is_empty() && starts_a_block(line))
        {
            paragraph.clear();
        } else {
            paragraph.push(line);
        }
    }
    titles
}

/// The title of an ATX heading (`# Title`, up to six `#`, closing `#`s dropped), if `line` is one.
fn atx_heading(line: &str) -> Option<&str> {
    let hashes = line.bytes().take_while(|b| *b == b'#').count();
    let title = line.get(hashes..)?.strip_prefix(' ')?;
    (1..=6).contains(&hashes).then(|| title.trim_end().trim_end_matches('#').trim_end())
}

/// Whether `line` underlines a Setext heading: at most three spaces, then only `=` or only `-`.
fn setext_underline(line: &str) -> bool {
    let rest = line.trim_start_matches(' ');
    let mark = rest.chars().next();
    line.len() - rest.len() <= 3
        && mark.is_some_and(|m| matches!(m, '=' | '-'))
        && rest.trim_end().chars().all(|c| Some(c) == mark)
}

/// Whether `line` starts a block that is not a paragraph: a list item, a table row, a block
/// quote or a heading.
fn starts_a_block(line: &str) -> bool {
    let rest = line.trim_start();
    let ordered = rest.split_once(". ").is_some_and(|(n, _)| n.bytes().all(|b| b.is_ascii_digit()));
    ["- ", "* ", "+ ", "|", ">", "#"].iter().any(|start| rest.starts_with(start)) || ordered
}

/// GitHub's anchor for rendered heading text.
fn slug(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ' '))
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

/// The text a heading renders to. Code spans stand as [`CODE`] while HTML tags, link
/// destinations and emphasis are taken out of the rest, so that emphasis is matched across the
/// whole heading, then each span's content goes back as written.
fn heading_text(title: &str) -> String {
    let mut kept = Vec::new();
    let mut spans = Vec::new();
    let mut rest = title;
    while let Some(start) = rest.find('`') {
        let ticks = rest[start..].bytes().take_while(|b| *b == b'`').count();
        let open = &rest[start + ticks..];
        let Some(end) = open.find(&"`".repeat(ticks)) else { break };
        prose_into(&rest[..start], &mut kept);
        kept.push(CODE);
        spans.push(&open[..end]);
        rest = &open[end + ticks..];
    }
    prose_into(rest, &mut kept);
    let mut spans = spans.into_iter();
    let mut out = String::new();
    for c in emphasis::without_emphasis(&kept).chars() {
        match c {
            CODE => out.push_str(spans.next().unwrap_or("")),
            c => out.push(c),
        }
    }
    out
}

/// Adds `prose` to `kept` as it renders, but for emphasis: an HTML tag and a link's
/// `(destination)` dropped.
fn prose_into(prose: &str, kept: &mut Vec<char>) {
    let chars: Vec<char> = prose.chars().collect();
    let mut at = 0;
    while let Some(&c) = chars.get(at) {
        let skip_to =
            |close: char| chars[at..].iter().position(|&d| d == close).map(|n| at + n + 1);
        let skip = match c {
            '<' => skip_to('>'),
            ']' if chars.get(at + 1) == Some(&'(') => skip_to(')').inspect(|_| kept.push(']')),
            _ => None,
        };
        if let Some(next) = skip {
            at = next;
        } else {
            kept.push(c);
            at += 1;
        }
    }
}
