//! Waivers: the scan's own comments that waive a finding, and the form of Clippy's ban waivers.

use super::tokens::{tokens, Token};
use super::{Finding, RULES};

/// The Clippy lints whose waivers must be `expect` with a rule-ID reason.
const BAN_LINTS: [&str; 4] =
    ["disallowed_methods", "disallowed_types", "disallowed_macros", "float_arithmetic"];

/// Removes the findings a waiver covers and adds a `waiver` finding for each waiver that is
/// malformed or covers nothing.
pub(super) fn apply_waivers(text: &str, mut found: Vec<Finding>) -> Vec<Finding> {
    const MARK: &str = "// nondeterminism: allow ";
    let mut failures = Vec::new();
    for (n, text_line) in text.lines().enumerate() {
        let Some(at) = text_line.find(MARK) else { continue };
        let (line, waiver) = (n + 1, &text_line[at + MARK.len()..]);
        let (rule, reason) = waiver
            .split_once(" -- ")
            .map_or((waiver.trim(), ""), |(r, why)| (r.trim(), why.trim()));
        let bad = |what: &str| Finding { line, rule: "waiver", what: format!("{what}: {waiver}") };
        if !RULES.contains(&rule) {
            failures.push(bad("a waiver for an unknown rule"));
        } else if reason.is_empty() {
            failures.push(bad("a waiver without a reason"));
        } else {
            let before = found.len();
            found.retain(|f| !(f.rule == rule && (f.line == line || f.line == line + 1)));
            if found.len() == before {
                failures.push(bad("a waiver that waives nothing"));
            }
        }
    }
    found.extend(failures);
    found
}

/// The Clippy waivers in any file that are not of the required form: an `allow` of a ban lint,
/// or an `expect` of one whose reason does not start with rule IDs and a colon.
pub(crate) fn waiver_forms(text: &str) -> Vec<Finding> {
    let toks = tokens(text);
    let mut out = Vec::new();
    for (i, (line, token)) in toks.iter().enumerate() {
        let Token::Ident(kind) = token else { continue };
        let opens = matches!(toks.get(i + 1), Some((_, Token::Punct('('))));
        if !(opens && (kind == "allow" || kind == "expect")) {
            continue;
        }
        let end = toks[i..]
            .iter()
            .position(|(_, t)| *t == Token::Punct(')'))
            .map_or(toks.len(), |e| i + e);
        let inside = &toks[i..end];
        let bans = inside
            .iter()
            .any(|(_, t)| matches!(t, Token::Ident(l) if BAN_LINTS.contains(&l.as_str())));
        let reason =
            inside
                .iter()
                .find_map(|(_, t)| if let Token::Str(s) = t { Some(s.as_str()) } else { None });
        let ruled = reason
            .and_then(|r| r.split_once(':'))
            .is_some_and(|(ids, _)| ids.split(", ").all(|id| RULES.contains(&id)));
        if bans && kind == "allow" {
            out.push(Finding {
                line: *line,
                rule: "waiver",
                what: "a ban lint waived with allow: use expect".to_owned(),
            });
        } else if bans && !ruled {
            out.push(Finding {
                line: *line,
                rule: "waiver",
                what: "a ban lint's reason must start with its rule IDs and a colon".to_owned(),
            });
        }
    }
    out
}
