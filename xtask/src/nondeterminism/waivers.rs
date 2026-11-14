//! Waivers: the scan's own comments that waive a finding, and the form of every lint-level
//! attribute that could turn off a Clippy ban.

#[cfg(test)]
mod tests;

use super::tokens::{line_comments, tokens, Located, Token};
use super::{Finding, RULES};

/// The Clippy lints of the bans, and the two renamed names (`disallowed_method`,
/// `disallowed_type`) that `rustc` still applies to them.
const BAN_LINTS: [&str; 6] = [
    "disallowed_methods",
    "disallowed_types",
    "disallowed_macros",
    "float_arithmetic",
    "disallowed_method",
    "disallowed_type",
];

/// The lint groups that hold a ban lint for the pinned toolchain (`cargo clippy -- -W help`,
/// Clippy 0.1.96): `clippy::all` and `clippy::style` hold the three `disallowed_*` lints and
/// `clippy::restriction` holds `float_arithmetic`. `warnings` holds every lint at `warn`, so
/// with a ban lowered to `warn` it turns the ban off.
const BAN_GROUPS: [&str; 4] = ["clippy::all", "clippy::style", "clippy::restriction", "warnings"];

/// The lint levels that can turn a ban off: everything below `deny`.
const LOWERING: [&str; 3] = ["allow", "expect", "warn"];

/// Removes the findings a waiver covers and adds a `waiver` finding for each waiver that is
/// malformed or covers nothing. A waiver is a plain line comment, read by the lexer, that starts
/// ` nondeterminism: allow `; the same text in a string or a doc comment is not one.
pub(super) fn apply_waivers(text: &str, mut found: Vec<Finding>) -> Vec<Finding> {
    const MARK: &str = " nondeterminism: allow ";
    let mut failures = Vec::new();
    for (line, body) in line_comments(text) {
        let Some(waiver) = body.strip_prefix(MARK) else { continue };
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

/// The lint-level attributes in any file that could turn a ban off: `allow` or `warn` of a ban
/// lint, a renamed ban lint, any level below `deny` of a group that holds one, a lint list built
/// by a macro (`$`), or an `expect` of a ban lint whose reason does not start with rule IDs and
/// a colon. They are found as tokens, so also inside `cfg_attr` and macro bodies.
pub(crate) fn waiver_forms(text: &str) -> Vec<Finding> {
    let toks = tokens(text);
    let mut out = Vec::new();
    for (i, (line, token)) in toks.iter().enumerate() {
        let Token::Ident(level) = token else { continue };
        let opens = matches!(toks.get(i + 1), Some((_, Token::Punct('('))));
        if !(opens && LOWERING.contains(&level.as_str())) {
            continue;
        }
        if let Some(what) = lint_list_fault(level, &toks[i + 1..]) {
            out.push(Finding { line: *line, rule: "waiver", what: what.to_owned() });
        }
    }
    out
}

/// What is wrong with the lint list after a lowering `level` (`rest` starts at its `(`), if
/// anything.
fn lint_list_fault(level: &str, rest: &[Located]) -> Option<&'static str> {
    let end = closing_paren(rest);
    let inside = &rest[1..end];
    if inside.iter().any(|(_, t)| *t == Token::Punct('$')) {
        return Some("a lint list built by a macro: write the lint names out");
    }
    let names = lint_names(inside);
    if names.iter().any(|n| BAN_GROUPS.contains(&n.as_str())) {
        return Some("a lint group that holds a ban lint: name the lints themselves");
    }
    let last = |n: &String| n.rsplit("::").next().unwrap_or_default().to_owned();
    let bans: Vec<String> =
        names.iter().map(last).filter(|n| BAN_LINTS.contains(&n.as_str())).collect();
    let reason =
        inside.iter().find_map(|(_, t)| if let Token::Str(s) = t { Some(s) } else { None });
    let ruled = reason
        .and_then(|r| r.split_once(':'))
        .is_some_and(|(ids, _)| ids.split(", ").all(|id| RULES.contains(&id)));
    if bans.is_empty() {
        None
    } else if bans.iter().any(|b| b == "disallowed_method" || b == "disallowed_type") {
        Some("a ban lint's old name: use disallowed_methods or disallowed_types")
    } else if level == "allow" {
        Some("a ban lint waived with allow: use expect")
    } else if level == "warn" {
        Some("a ban lint lowered to warn")
    } else if ruled {
        None
    } else {
        Some("a ban lint's reason must start with its rule IDs and a colon")
    }
}

/// The index in `rest` of the first `)`, which closes the lint list that opens at its start (a
/// lint list holds no parentheses), or its last index.
fn closing_paren(rest: &[Located]) -> usize {
    rest.iter()
        .position(|(_, t)| *t == Token::Punct(')'))
        .unwrap_or(rest.len().saturating_sub(1))
        .max(1)
}

/// The lint names in a lint list, tool-qualified as written (`clippy::all`, `warnings`); the
/// `reason = "..."` key is not a name.
fn lint_names(inside: &[Located]) -> Vec<String> {
    let mut names = Vec::new();
    let mut i = 0;
    while let Some((_, token)) = inside.get(i) {
        let at = |k: usize| inside.get(i + k).map(|(_, t)| t);
        match (token, at(1), at(2), at(3)) {
            (
                Token::Ident(tool),
                Some(Token::Punct(':')),
                Some(Token::Punct(':')),
                Some(Token::Ident(lint)),
            ) => {
                names.push(format!("{tool}::{lint}"));
                i += 4;
            }
            (Token::Ident(word), Some(Token::Punct('=')), _, _) if word == "reason" => i += 2,
            (Token::Ident(lint), _, _, _) => {
                names.push(lint.clone());
                i += 1;
            }
            _ => i += 1,
        }
    }
    names
}

/// In the deterministic core: each attribute that holds a macro metavariable (`#[$kind(...)]`,
/// `#[cfg_attr($c, ...)]`), which can build a lint attribute out of the scan's sight.
pub(super) fn macro_attributes(toks: &[Located]) -> Vec<Finding> {
    let mut out = Vec::new();
    for (i, (line, token)) in toks.iter().enumerate() {
        let inner = matches!(toks.get(i + 1), Some((_, Token::Punct('!'))));
        let open = i + 1 + usize::from(inner);
        let opens = matches!(toks.get(open), Some((_, Token::Punct('['))));
        if *token != Token::Punct('#') || !opens {
            continue;
        }
        let mut depth = 0_usize;
        for (_, t) in &toks[open..] {
            match t {
                Token::Punct('[') => depth += 1,
                Token::Punct(']') if depth <= 1 => break,
                Token::Punct(']') => depth -= 1,
                Token::Punct('$') => {
                    out.push(Finding {
                        line: *line,
                        rule: "waiver",
                        what: "an attribute built by a macro in the core: write it out".to_owned(),
                    });
                    break;
                }
                _ => {}
            }
        }
    }
    out
}
