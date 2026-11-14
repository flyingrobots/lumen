//! This module owns finding the destinations of a Markdown page's links: inline links and images
//! (`[text](dest)`, `![alt](dest)`) and reference definitions (`[name]: dest`, the destination on
//! the same line or the next), outside fenced code blocks and code spans. A destination in angle
//! brackets is read to its `>`, spaces included; otherwise to the first space, where a title
//! may follow.

use super::blocks::prose_lines;

/// The destinations of the links on `text`, with their lines (from 1), in order.
pub(crate) fn links(text: &str) -> Vec<(usize, String)> {
    let lines: Vec<(usize, &str)> = prose_lines(text).collect();
    let mut found = Vec::new();
    for (index, &(number, line)) in lines.iter().enumerate() {
        let line = without_code_spans(line);
        if let Some(definition) = reference_definition(&line) {
            // GFM lets the destination start on the line after `[name]:`.
            let next = lines.get(index + 1).filter(|(next, _)| *next == number + 1);
            match (definition, next) {
                (Definition::Here(dest), _) => found.push((number, dest)),
                (Definition::NextLine, Some(&(next, text))) => {
                    found.extend(bare_destination(text.trim()).map(|dest| (next, dest)));
                }
                (Definition::NextLine, None) => {}
            }
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

/// Where a reference definition's destination is.
enum Definition {
    /// On the definition's own line.
    Here(String),
    /// On the line after it: `[name]:` ends its line.
    NextLine,
}

/// Where the destination of `line` is, if `line` is a reference definition, `[name]: dest`.
fn reference_definition(line: &str) -> Option<Definition> {
    let rest = line.trim_start().strip_prefix('[')?;
    let (_, after) = rest.split_once("]:")?;
    Some(bare_destination(after.trim()).map_or(Definition::NextLine, Definition::Here))
}

/// The destination at the start of `text`: inside angle brackets as written, else to the first
/// space; `None` if there is none.
fn bare_destination(text: &str) -> Option<String> {
    let dest = match text.strip_prefix('<') {
        Some(bracketed) => bracketed.split('>').next().unwrap_or(""),
        None => text.split_whitespace().next().unwrap_or(""),
    };
    (!dest.is_empty()).then(|| dest.to_owned())
}

/// The destination at the start of `rest`, just after `](`, and what follows its `)`: inside
/// angle brackets as written, else up to a title after a space; parentheses inside it balanced.
fn destination(rest: &str) -> Option<(String, &str)> {
    let mut depth = 0_usize;
    for (at, c) in rest.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => {
                let dest = bare_destination(rest[..at].trim()).unwrap_or_default();
                return Some((dest, &rest[at + 1..]));
            }
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}
