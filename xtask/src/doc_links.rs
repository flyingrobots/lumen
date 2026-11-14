//! This module owns the relative-link check of the repository's Markdown: every link to a file
//! names a file or directory that exists, and every `#anchor` into a Markdown page names one of
//! that page's headings.
//!
//! The links are found by [`links`] and the headings' anchors made by [`anchors`], both outside
//! fenced code blocks ([`blocks`]). A destination that starts with a URI scheme (`https:`,
//! `mailto:`: a letter, then letters, digits, `+`, `-` or `.`, then `:`) is not checked: this
//! gate is offline. A colon anywhere else, as in `page.md#step:1`, is no scheme. The path is
//! percent-decoded (`my%20guide.md` is `my guide.md`) and, when it starts with `/`, taken from
//! the repository root, as GitHub takes it.

mod anchors;
mod blocks;
mod emphasis;
mod links;

pub(crate) use anchors::anchors;
pub(crate) use links::links;

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
    if has_scheme(link) {
        return None;
    }
    let (path, anchor) = link.split_once('#').unwrap_or((link, ""));
    let target = if path.is_empty() {
        page.to_owned()
    } else {
        let Some(target) = resolve(page, &percent_decoded(path)) else {
            return Some("leaves the repository");
        };
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

/// Whether `link` starts with a URI scheme: a letter, then letters, digits, `+`, `-` or `.`, then
/// `:` (RFC 3986, section 3.1).
fn has_scheme(link: &str) -> bool {
    let Some((scheme, _)) = link.split_once(':') else { return false };
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// `path` with each `%XX` (two hexadecimal digits) turned into its byte, or as written if the
/// bytes are not UTF-8. A `%` without two digits after it stays.
fn percent_decoded(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        let pair = bytes
            .get(at + 1..at + 3)
            .filter(|hex| hex.iter().all(u8::is_ascii_hexdigit))
            .and_then(|hex| std::str::from_utf8(hex).ok());
        if let Some(decoded) =
            pair.filter(|_| byte == b'%').and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(decoded);
            at += 3;
        } else {
            out.push(byte);
            at += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| path.to_owned())
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

#[cfg(test)]
mod tests;
