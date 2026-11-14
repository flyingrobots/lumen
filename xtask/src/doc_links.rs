//! This module owns the relative-link check of the repository's Markdown: every link to a file
//! names a file or directory that exists, and every `#anchor` into a Markdown page names one of
//! that page's headings.
//!
//! Links are inline links and images (`[text](dest)`, `![alt](dest)`) and reference
//! definitions (`[name]: dest`), outside fenced code blocks and code spans. A destination with a
//! scheme (`https:`, `mailto:`) is not checked: this gate is offline. A destination starting
//! with `/` is taken from the repository root, as GitHub takes it. Anchors are the ones GitHub
//! gives headings, made from the heading's rendered text (code spans as written, a link as its
//! text, HTML tags and emphasis underscores dropped): lowercased, every character that is not a
//! letter, a digit, `-`, `_` or a space dropped, spaces turned into `-`; an anchor already taken
//! gets the first free `-1`, `-2`, ... A fenced block ends only at a fence of its own character,
//! at least as long, with nothing after it.

use std::collections::BTreeMap;

/// A link that leads nowhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Broken {
    /// The page the link is on, relative to the repository root.
    pub(crate) page: String,
    /// The link's line, from 1.
    pub(crate) line: usize,
    /// The destination as written.
    pub(crate) link: String,
    /// What is wrong with it.
    pub(crate) why: &'static str,
}

/// The destinations of the links on `text`, with their lines (from 1), in order.
pub(crate) fn links(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (number, line) in prose_lines(text) {
        let line = without_code_spans(line);
        if let Some(dest) = reference_definition(&line) {
            found.push((number, dest));
            continue;
        }
        let mut rest = line.as_str();
        while let Some(at) = rest.find("](") {
            rest = &rest[at + 2..];
            if let Some((dest, after)) = destination(rest) {
                found.push((number, dest));
                rest = after;
            }
        }
    }
    found
}

/// The anchors GitHub gives the headings of `text`, in order. As GitHub's slugger does, a
/// heading keeps its base anchor unless that anchor is already taken; then the base's own
/// counter goes up until `base-N` is free.
pub(crate) fn anchors(text: &str) -> Vec<String> {
    let mut made: Vec<String> = Vec::new();
    let mut counters: BTreeMap<String, usize> = BTreeMap::new();
    for (_, line) in prose_lines(text) {
        let Some(title) = heading(line) else { continue };
        let base = slug(&heading_text(title));
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

/// GitHub's anchor for rendered heading text: lowercased, every character that is not a letter,
/// a digit, `-`, `_` or a space dropped, each space turned into `-`.
fn slug(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ' '))
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

/// The text a heading renders to: each code span's content as written, and the rest through
/// [`prose_text`].
fn heading_text(title: &str) -> String {
    let mut out = String::new();
    let mut rest = title;
    while let Some(start) = rest.find('`') {
        let ticks = rest[start..].bytes().take_while(|b| *b == b'`').count();
        let open = &rest[start + ticks..];
        let Some(end) = open.find(&"`".repeat(ticks)) else { break };
        out.push_str(&prose_text(&rest[..start]));
        out.push_str(&open[..end]);
        rest = &open[end + ticks..];
    }
    out.push_str(&prose_text(rest));
    out
}

/// Heading text outside code spans as it renders: an HTML tag dropped, a link's `(destination)`
/// dropped, and a run of underscores dropped unless it stands between two letters or digits
/// (`snake_case` keeps its underscore; `_emphasis_` loses both).
fn prose_text(prose: &str) -> String {
    let chars: Vec<char> = prose.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    while let Some(&c) = chars.get(at) {
        let skip_to =
            |close: char| chars[at..].iter().position(|&d| d == close).map(|n| at + n + 1);
        let skip = match c {
            '<' => skip_to('>'),
            ']' if chars.get(at + 1) == Some(&'(') => skip_to(')').inspect(|_| out.push(']')),
            '_' => {
                let run = chars[at..].iter().take_while(|&&d| d == '_').count();
                let before = at.checked_sub(1).and_then(|b| chars.get(b));
                let inside = before.is_some_and(|b| b.is_alphanumeric())
                    && chars.get(at + run).is_some_and(|a| a.is_alphanumeric());
                if inside {
                    out.extend(&chars[at..at + run]);
                }
                Some(at + run)
            }
            _ => None,
        };
        if let Some(next) = skip {
            at = next;
        } else {
            out.push(c);
            at += 1;
        }
    }
    out
}

/// Every broken relative link on `pages` (each a path relative to the repository root, with `/`
/// separators, and its text). `exists` says whether a repository-relative path names a file or
/// a directory; an anchor is checked against the page's text when the target is one of `pages`.
pub(crate) fn check(pages: &[(String, String)], exists: impl Fn(&str) -> bool) -> Vec<Broken> {
    let mut broken = Vec::new();
    for (page, text) in pages {
        for (line, link) in links(text) {
            if let Some(why) = judge(pages, &exists, page, &link) {
                broken.push(Broken { page: page.clone(), line, link, why });
            }
        }
    }
    broken
}

/// What is wrong with `link` on `page`, if anything.
fn judge(
    pages: &[(String, String)],
    exists: &impl Fn(&str) -> bool,
    page: &str,
    link: &str,
) -> Option<&'static str> {
    if link.contains(':') && !link.starts_with('#') {
        return None;
    }
    let (path, anchor) = link.split_once('#').unwrap_or((link, ""));
    let target = if path.is_empty() {
        page.to_owned()
    } else {
        let Some(target) = resolve(page, path) else { return Some("leaves the repository") };
        if !exists(&target) {
            return Some("no such file or directory");
        }
        target
    };
    let text = pages.iter().find(|(name, _)| *name == target).map(|(_, text)| text);
    match text {
        Some(text) if !anchor.is_empty() && !anchors(text).iter().any(|a| a == anchor) => {
            Some("no such heading")
        }
        _ => None,
    }
}

/// `path` taken from the directory of `page` (or from the repository root when it starts with
/// `/`), with `.` and `..` resolved and a trailing `/` dropped, or `None` if it climbs out of the
/// repository.
fn resolve(page: &str, path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    if !path.starts_with('/') {
        parts.extend(page.split('/'));
        parts.pop();
    }
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// The lines of `text` outside fenced code blocks, with their numbers from 1. A block opens at
/// a fence and closes at a fence of the same character, at least as long, with only whitespace
/// after it.
fn prose_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
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

/// `line` with every code span (a run of backticks to the next run of the same length) blanked.
fn without_code_spans(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find('`') {
        out.push_str(&rest[..start]);
        let ticks = rest[start..].bytes().take_while(|b| *b == b'`').count();
        let after = &rest[start + ticks..];
        let close = "`".repeat(ticks);
        let Some(end) = after.find(&close) else {
            out.push_str(&rest[start..]);
            return out;
        };
        rest = &after[end + ticks..];
    }
    out.push_str(rest);
    out
}

/// The destination of a reference definition, `[name]: dest`, if `line` is one.
fn reference_definition(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix('[')?;
    let (_, after) = rest.split_once("]:")?;
    after.split_whitespace().next().map(|dest| dest.trim_matches(['<', '>']).to_owned())
}

/// The destination at the start of `rest`, just after `](`, and what follows its `)`: inside
/// angle brackets as written, else up to a title after a space; parentheses inside it balanced.
fn destination(rest: &str) -> Option<(String, &str)> {
    let mut depth = 0_usize;
    for (at, c) in rest.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => {
                let inner = rest[..at].trim();
                let dest = match inner.strip_prefix('<') {
                    Some(bracketed) => bracketed.split('>').next().unwrap_or(""),
                    None => inner.split_whitespace().next().unwrap_or(""),
                };
                return Some((dest.to_owned(), &rest[at + 1..]));
            }
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// The title of an ATX heading (`# Title`, up to six `#`, closing `#`s dropped), if `line` is one.
fn heading(line: &str) -> Option<&str> {
    let hashes = line.bytes().take_while(|b| *b == b'#').count();
    let title = line.get(hashes..)?.strip_prefix(' ')?;
    (1..=6).contains(&hashes).then(|| title.trim_end().trim_end_matches('#').trim_end())
}

#[cfg(test)]
mod tests;
