//! This module owns which underscores of a heading are emphasis, by the delimiter rule of
//! GitHub Flavored Markdown (spec 0.29, "Emphasis and strong emphasis"), so that the heading's
//! anchor is made from the text GitHub renders: underscores that open and close emphasis are not
//! in it; every other underscore is.
//!
//! A run of underscores is left-flanking when the character after it is not whitespace and is
//! not punctuation, or is punctuation preceded by whitespace or punctuation; right-flanking the
//! other way round. It can open emphasis when it is left-flanking and either not right-flanking
//! or preceded by punctuation, and close when it is right-flanking and either not left-flanking
//! or followed by punctuation. A closer is matched with the nearest earlier opener that has
//! underscores left, two at a time while both have two, else one; openers between them can no
//! longer match. What is not matched stays literal. The start and end of the text count as
//! whitespace. This is the rule's core; its "multiple of 3" clause concerns runs that both open
//! and close, which an underscore run does only between punctuation, and is not modelled.

/// One run of underscores: where it starts, how long it is, and whether it can open or close.
struct Run {
    start: usize,
    length: usize,
    opens: bool,
    closes: bool,
}

/// `chars` with the underscores that are emphasis delimiters removed.
pub(super) fn without_emphasis(chars: &[char]) -> String {
    let runs = runs(chars);
    let mut left: Vec<usize> = runs.iter().map(|run| run.length).collect();
    let mut usable: Vec<bool> = runs.iter().map(|run| run.opens).collect();
    for closer in 0..runs.len() {
        if !runs[closer].closes {
            continue;
        }
        while left[closer] > 0 {
            let Some(opener) = (0..closer).rev().find(|&o| usable[o] && left[o] > 0) else {
                break;
            };
            let taken = if left[opener] >= 2 && left[closer] >= 2 { 2 } else { 1 };
            left[opener] -= taken;
            left[closer] -= taken;
            usable[opener + 1..closer].fill(false);
        }
    }
    let mut out = String::new();
    let mut at = 0;
    for (run, kept) in runs.iter().zip(&left) {
        out.extend(&chars[at..run.start]);
        out.extend(core::iter::repeat_n('_', *kept));
        at = run.start + run.length;
    }
    out.extend(&chars[at..]);
    out
}

/// Every run of underscores in `chars`, in order, with what it can do.
fn runs(chars: &[char]) -> Vec<Run> {
    let is_space = |c: Option<&char>| c.is_none_or(|c| c.is_whitespace());
    let is_punct = |c: Option<&char>| c.is_some_and(|c| !c.is_alphanumeric() && !c.is_whitespace());
    let mut runs = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        if chars[at] != '_' {
            at += 1;
            continue;
        }
        let length = chars[at..].iter().take_while(|&&c| c == '_').count();
        let before = at.checked_sub(1).and_then(|b| chars.get(b));
        let after = chars.get(at + length);
        let left = !is_space(after) && (!is_punct(after) || is_space(before) || is_punct(before));
        let right = !is_space(before) && (!is_punct(before) || is_space(after) || is_punct(after));
        runs.push(Run {
            start: at,
            length,
            opens: left && (!right || is_punct(before)),
            closes: right && (!left || is_punct(after)),
        });
        at += length;
    }
    runs
}
