//! The nondeterminism scan: what Clippy's bans in `clippy.toml` cannot see
//! (docs/adr/0006-nondeterminism-gate.md).
//!
//! Clippy resolves paths exactly but only in code it compiles, and has no rule for a float
//! literal. This scan reads tokens instead, so it also reads code under a `cfg` that is not built
//! and macro bodies, and ignores comments and strings. In the deterministic core it reports
//! float types, float literals and casts to a float; the names `Instant`, `SystemTime`,
//! `UNIX_EPOCH`, `available_parallelism`, `HashMap`, `HashSet` and `RandomState`; the
//! `env!` and `option_env!` macros; and paths into `std::env`, `std::fs`, `std::process`,
//! `std::net` and `std::thread::sleep`, written out, through a `use` tree (groups, `self`,
//! renames) or through a name a `use` brought in. Any line that names open addressing is
//! `hand-hashed-table`. In every file it checks the form of Clippy waivers ([`waiver_forms`]).
//!
//! A finding is waived by a comment on its line or the line above:
//! `// nondeterminism: allow <rule-id> -- <reason>`. A waiver with an unknown rule or no
//! reason, or one that waives nothing, is itself a failure.

#[cfg(test)]
mod tests;
pub(crate) mod tokens;

use tokens::{is_float, tokens, Located, Token};

/// Every rule ID: Echo's `ban-nondeterminism` vocabulary, then Lumen's own.
pub(crate) const RULES: [&str; 20] = [
    "time-system",
    "time-instant",
    "thread-sleep",
    "host-parallelism",
    "random",
    "host-callback",
    "network",
    "unordered-container",
    "hand-hashed-table",
    "json-serialization",
    "wasm-serde",
    "float-sentinel",
    "float-op",
    "std-env",
    "std-fs",
    "std-process",
    "thread-spawn",
    "float-type",
    "float-literal",
    "float-cast",
];

/// Names banned wherever they are spelled, and their rules.
const NAMES: [(&str, &str); 9] = [
    ("Instant", "time-instant"),
    ("SystemTime", "time-system"),
    ("UNIX_EPOCH", "time-system"),
    ("available_parallelism", "host-parallelism"),
    ("HashMap", "unordered-container"),
    ("HashSet", "unordered-container"),
    ("RandomState", "unordered-container"),
    ("FxHashMap", "unordered-container"),
    ("FxHashSet", "unordered-container"),
];

/// Banned path prefixes and their rules.
const PATHS: [(&[&str], &str); 5] = [
    (&["std", "env"], "std-env"),
    (&["std", "fs"], "std-fs"),
    (&["std", "process"], "std-process"),
    (&["std", "net"], "network"),
    (&["std", "thread", "sleep"], "thread-sleep"),
];

/// The Clippy lints whose waivers must be `expect` with a rule-ID reason.
const BAN_LINTS: [&str; 4] =
    ["disallowed_methods", "disallowed_types", "disallowed_macros", "float_arithmetic"];

/// One finding: the 1-based line, the rule and what was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Finding {
    /// The line it is on.
    pub(crate) line: usize,
    /// Its rule ID, or `waiver` for a malformed or unused waiver.
    pub(crate) rule: &'static str,
    /// What was found.
    pub(crate) what: String,
}

/// The findings in a deterministic-core file, at most one per rule and line, waivers applied:
/// the result also holds a `waiver` finding for each malformed or unused waiver.
pub(crate) fn core_findings(text: &str) -> Vec<Finding> {
    let toks = tokens(text);
    let mut found = Vec::new();
    let mut add = |line, rule, what: &str| {
        if !found.iter().any(|f: &Finding| f.line == line && f.rule == rule) {
            found.push(Finding { line, rule, what: what.to_owned() });
        }
    };
    for (i, (line, token)) in toks.iter().enumerate() {
        let prev = i.checked_sub(1).and_then(|p| toks.get(p)).map(|(_, t)| t);
        match token {
            Token::Ident(name) if name == "f32" || name == "f64" => {
                let cast = prev == Some(&Token::Ident("as".to_owned()));
                add(*line, if cast { "float-cast" } else { "float-type" }, name);
            }
            Token::Num(number) if is_float(number) => add(*line, "float-literal", number),
            Token::Ident(name) => {
                if let Some((_, rule)) = NAMES.iter().find(|(n, _)| n == name) {
                    add(*line, rule, name);
                }
                let bang = matches!(toks.get(i + 1), Some((_, Token::Punct('!'))));
                if bang && (name == "env" || name == "option_env") {
                    add(*line, "std-env", &format!("{name}!"));
                }
            }
            Token::Num(_) | Token::Str(_) | Token::Punct(_) => {}
        }
    }
    let imports = imports(&toks);
    for (line, path) in paths(&toks) {
        let resolved = imports
            .iter()
            .find(|(alias, _)| path.first() == Some(alias))
            .map_or_else(|| path.clone(), |(_, full)| [&full[..], &path[1..]].concat());
        for (prefix, rule) in PATHS {
            let under =
                |p: &[String]| p.len() >= prefix.len() && p.iter().zip(prefix).all(|(a, b)| a == b);
            if under(&path) || under(&resolved) {
                add(line, rule, &path.join("::"));
            }
        }
    }
    for (n, text_line) in text.lines().enumerate() {
        let lower = text_line.to_lowercase();
        if lower.contains("open addressing") || lower.contains("open-addressing") {
            add(n + 1, "hand-hashed-table", "open addressing");
        }
    }
    apply_waivers(text, found)
}

/// Every `::` path in the tokens with its line, `use` trees flattened into full paths.
fn paths(toks: &[Located]) -> Vec<(usize, Vec<String>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some((line, _)) = toks.get(i) {
        if let Some((end, leaves)) = use_at(toks, i) {
            out.extend(leaves.into_iter().map(|(p, _)| (*line, p)));
            i = end;
            continue;
        }
        let mut path = Vec::new();
        let mut j = i;
        while let Some((_, Token::Ident(segment))) = toks.get(j) {
            path.push(segment.clone());
            if !matches!(
                (toks.get(j + 1), toks.get(j + 2)),
                (Some((_, Token::Punct(':'))), Some((_, Token::Punct(':'))))
            ) {
                break;
            }
            j += 3;
        }
        if path.len() > 1 {
            out.push((*line, path));
        }
        i = j.max(i) + 1;
    }
    out
}

/// The names `use` declarations bring in (the last segment, or the rename) and their paths.
fn imports(toks: &[Located]) -> Vec<(String, Vec<String>)> {
    let leaves = (0..toks.len()).filter_map(|i| use_at(toks, i)).flat_map(|(_, leaves)| leaves);
    leaves
        .filter_map(|(path, alias)| {
            let full: Vec<String> = path.into_iter().filter(|s| s != "self").collect();
            Some((alias.or_else(|| full.last().cloned())?, full))
        })
        .collect()
}

/// If a `use` declaration starts at `i`: the index of its `;` and its leaves ([`use_tree`]).
fn use_at(toks: &[Located], i: usize) -> Option<(usize, Vec<Leaf>)> {
    if toks.get(i)?.1 != Token::Ident("use".to_owned()) {
        return None;
    }
    let end =
        toks[i..].iter().position(|(_, t)| *t == Token::Punct(';')).map_or(toks.len(), |e| i + e);
    Some((end, use_tree(&toks[i + 1..end])))
}

/// A leaf of a `use` tree: its full path and its rename, if it has one.
type Leaf = (Vec<String>, Option<String>);

/// The leaves of a `use` tree.
fn use_tree(toks: &[Located]) -> Vec<Leaf> {
    let (mut out, mut stack, mut path) = (Vec::new(), Vec::new(), Vec::<String>::new());
    let mut alias = None;
    let mut iter = toks.iter().map(|(_, t)| t);
    while let Some(token) = iter.next() {
        match token {
            Token::Ident(word) if word == "as" => {
                alias = iter.next().and_then(|t| {
                    if let Token::Ident(a) = t {
                        Some(a.clone())
                    } else {
                        None
                    }
                });
            }
            Token::Ident(word) => path.push(word.clone()),
            Token::Punct('*') => path.push("*".to_owned()),
            Token::Punct('{') => stack.push(path.len()),
            Token::Punct(',' | '}') => {
                if path.len() > stack.last().copied().unwrap_or(0) {
                    out.push((path.clone(), alias.take()));
                }
                let keep =
                    if *token == Token::Punct('}') { stack.pop() } else { stack.last().copied() };
                path.truncate(keep.unwrap_or(0));
            }
            _ => {}
        }
    }
    if !path.is_empty() {
        out.push((path, alias));
    }
    out
}

/// Removes the findings a waiver covers and adds a `waiver` finding for each waiver that is
/// malformed or covers nothing.
fn apply_waivers(text: &str, mut found: Vec<Finding>) -> Vec<Finding> {
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
