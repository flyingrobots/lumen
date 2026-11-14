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
//! `hand-hashed-table`, and an attribute built from a macro metavariable is a `waiver` finding.
//! In every file it checks each lint-level attribute that could turn a Clippy ban off
//! ([`waiver_forms`]).
//!
//! A finding is waived by a plain line comment, read by the lexer, on its line or the line above:
//! `// nondeterminism: allow <rule-id> -- <reason>`; the same text in a string or a doc comment is
//! not a waiver. A waiver with an unknown rule or no reason, or one that waives nothing, is
//! itself a failure.

mod paths;
#[cfg(test)]
mod tests;
pub(crate) mod tokens;
mod waivers;

use paths::{imports, paths};
use tokens::{is_float, tokens, Token};
use waivers::apply_waivers;
pub(crate) use waivers::waiver_forms;

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
    found.extend(waivers::macro_attributes(&toks));
    apply_waivers(text, found)
}
