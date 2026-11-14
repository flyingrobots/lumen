//! This module owns the relative-link check of the repository's Markdown: every link to a file
//! names a file or directory that exists, and every `#anchor` into a Markdown page names one of
//! that page's headings.
//!
//! Links are inline links and images (`[text](dest)`, `![alt](dest)`) and reference
//! definitions (`[name]: dest`), outside fenced code blocks and code spans. A destination with a
//! scheme (`https:`, `mailto:`) is not checked: this gate is offline. Anchors are the ones GitHub
//! gives headings: lowercased, every character that is not a letter, a digit, `-`, `_` or a
//! space dropped, spaces turned into `-`, and a repeated anchor numbered `-1`, `-2`, ...

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

/// The anchors GitHub gives the headings of `text`, in order.
pub(crate) fn anchors(text: &str) -> Vec<String> {
    let mut made: Vec<String> = Vec::new();
    for (_, line) in prose_lines(text) {
        let Some(title) = heading(line) else { continue };
        let base: String = title
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ' '))
            .map(|c| if c == ' ' { '-' } else { c })
            .collect();
        let repeats = made.iter().filter(|anchor| strip_count(anchor, &base)).count();
        made.push(if repeats == 0 { base } else { format!("{base}-{repeats}") });
    }
    made
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

/// `path` taken from the directory of `page`, with `.` and `..` resolved and a trailing `/`
/// dropped, or `None` if it climbs out of the repository.
fn resolve(page: &str, path: &str) -> Option<String> {
    let mut parts: Vec<&str> = page.split('/').collect();
    parts.pop();
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

/// The lines of `text` outside fenced code blocks, with their numbers from 1.
fn prose_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut fence: Option<&str> = None;
    text.lines().enumerate().filter_map(move |(index, line)| {
        let trimmed = line.trim_start();
        let marker = ["```", "~~~"].into_iter().find(|marker| trimmed.starts_with(marker));
        match (fence, marker) {
            (None, Some(marker)) => fence = Some(marker),
            (Some(open), Some(marker)) if open == marker => fence = None,
            (None, None) => return Some((index + 1, line)),
            _ => {}
        }
        None
    })
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

/// Whether `anchor` is `base` or `base` with a repeat number.
fn strip_count(anchor: &str, base: &str) -> bool {
    anchor.strip_prefix(base).is_some_and(|rest| {
        rest.is_empty()
            || rest
                .strip_prefix('-')
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    })
}

#[cfg(test)]
mod tests;
