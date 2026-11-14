//! The nondeterminism scan: what Clippy's bans in `clippy.toml` cannot see
//! (docs/adr/0006-nondeterminism-gate.md).
//!
//! Clippy resolves paths exactly but only in code it compiles, and has no rule for a float
//! literal. This scan reads tokens instead, so it also reads code under a `cfg` that is not built
//! and macro bodies, and ignores comments and strings. In the deterministic core it reports
//! float types (`f16`, `f32`, `f64`, `f128`, and std's aliases `c_double` and `c_float`), float
//! literals and casts to a float; the names `Instant`, `SystemTime`,
//! `UNIX_EPOCH`, `available_parallelism`, `HashMap`, `HashSet` and `RandomState`; the
//! `env!` and `option_env!` macros; names with the word `f16`, `f32`, `f64` or `f128`, or ending `_float`
//! (`as_secs_f64`, `into_float`); Bunny's float-bridge types (`bunny_linalg::Vec3`, `bunny_codec::PlyVertex`); and
//! paths into `std::env`, `std::fs`, `std::process`, `std::net` and the `std::thread` sleeps,
//! written out, raw (`std::r#fs`), through a `use` tree (groups, `self`, renames), through a name
//! a `use` brought in, or through an `extern crate std as ...` alias, chained, the aliases of
//! every core file included ([`CrateAliases`]: a crate-root `extern crate`, a `pub use std as x`
//! reached as `crate::m::x`). A glob import of a std module and a std path built from a macro
//! metavariable (`std::$m::var`, `$root::fs`) are `hidden-path`: the scan cannot follow them.
//! Any line that names open addressing is `hand-hashed-table`, and an attribute built from a
//! macro metavariable is a `waiver` finding. None of this depends on `cfg`: code under any
//! `cfg` is read the same.
//! In every file it checks each lint-level attribute that could turn a Clippy ban off
//! ([`waiver_forms`]).
//!
//! A finding is waived by a plain line comment, read by the lexer, on its line or the line above:
//! `// nondeterminism: allow <rule-id> -- <reason>`; the same text in a string or a doc comment is
//! not a waiver. A waiver with an unknown rule or no reason, or one that waives nothing, is
//! itself a failure.

#[cfg(test)]
mod crate_alias_tests;
mod host;
#[cfg(test)]
mod host_tests;
mod paths;
mod rules;
#[cfg(test)]
mod tests;
pub(crate) mod tokens;
mod waivers;

pub(crate) use paths::{crate_aliases, CrateAliases};
use paths::{imports, macro_paths, paths, resolve};
pub(crate) use rules::RULES;
use rules::{FLOAT_TYPES, NAMES, PATHS, STD_MODULES, STD_ROOTS};
use tokens::{is_float, tokens, Token};
use waivers::apply_waivers;
pub(crate) use waivers::{waiver_forms, BAN_GROUPS};

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
/// the result also holds a `waiver` finding for each malformed or unused waiver. `aliases` are
/// those of every core file ([`crate_aliases`]), so an alias declared in another file is
/// followed.
pub(crate) fn core_findings(text: &str, aliases: &CrateAliases) -> Vec<Finding> {
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
            Token::Ident(name) if FLOAT_TYPES.contains(&name.as_str()) => {
                let cast = prev == Some(&Token::Ident("as".to_owned()));
                add(*line, if cast { "float-cast" } else { "float-type" }, name);
            }
            Token::Num(number) if is_float(number) => add(*line, "float-literal", number),
            // A name with the word `f16`, `f32`, `f64` or `f128` (`as_secs_f64`, `to_f32`), or ending `_float`
            // (Bunny's `into_float`): a float passes through it even when no float type or
            // literal is written. Not `float_arithmetic`, the lint `lib.rs` denies.
            Token::Ident(name)
                if name.split('_').any(|w| FLOAT_TYPES.contains(&w))
                    || name.ends_with("_float") =>
            {
                add(*line, "float-type", name);
            }
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
    let std_root = |p: &Vec<String>| p.first().is_some_and(|r| STD_ROOTS.contains(&r.as_str()));
    for (line, path) in macro_paths(&toks) {
        let known: Vec<String> = path.iter().take_while(|s| !s.starts_with('$')).cloned().collect();
        let from_std = !known.is_empty() && resolve(&known, &imports, aliases).iter().any(std_root);
        let built_root = path[0].starts_with('$')
            && path[1..].iter().any(|s| s.starts_with('$') || STD_MODULES.contains(&s.as_str()));
        if from_std || built_root {
            add(line, "hidden-path", &format!("{}: a std path a macro builds", path.join("::")));
        }
    }
    for (line, rule, what) in host::host_findings(&toks) {
        add(line, rule, &what);
    }
    for (line, path) in paths(&toks) {
        // `core` and `alloc` are std's own modules under another root (`core::thread` does not
        // exist, `core::any::TypeId` is `std::any::TypeId`), so each is read as `std`.
        let readings: Vec<Vec<String>> = resolve(&path, &imports, aliases)
            .into_iter()
            .map(|mut p| {
                if p.first().is_some_and(|r| r == "core" || r == "alloc") {
                    "std".clone_into(&mut p[0]);
                }
                p
            })
            .collect();
        if path.last().is_some_and(|s| s == "*") && readings.iter().any(std_root) {
            add(line, "hidden-path", &format!("{}: a glob of a std module", path.join("::")));
        }
        for (prefix, rule) in PATHS {
            let under =
                |p: &[String]| p.len() >= prefix.len() && p.iter().zip(prefix).all(|(a, b)| a == b);
            if readings.iter().any(|p| under(p)) {
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

/// The rule under which the scan refuses the std item `path` (`std::os::unix`, `std::file` for
/// the macro when `is_macro`), if it does: the check that a decision to leave an item to the
/// scan is one the scan keeps ([`crate::clippy_bans::std_items`]).
pub(crate) fn scan_rule(path: &str, is_macro: bool) -> Option<&'static str> {
    let segments: Vec<&str> = path.split("::").collect();
    if is_macro {
        return host::macro_rule(segments.last()?);
    }
    let under = |prefix: &&[&str]| {
        segments.len() >= prefix.len() && segments.iter().zip(*prefix).all(|(a, b)| a == b)
    };
    PATHS.iter().find(|(prefix, _)| under(prefix)).map(|(_, rule)| *rule)
}

/// Whether a crate root's text denies (or forbids) `clippy::float_arithmetic` in an inner
/// attribute, `#![deny(.., clippy::float_arithmetic, ..)]`: each deterministic crate must, since
/// the lint catches arithmetic on a float whose type is only inferred and is not a workspace
/// lint (the binary, tests and tooling use floats).
pub(crate) fn denies_float_arithmetic(text: &str) -> bool {
    let toks = tokens(text);
    let word =
        |t: Option<&(usize, Token)>, w: &str| matches!(t, Some((_, Token::Ident(x))) if x == w);
    (0..toks.len()).any(|i| {
        let inner = toks.get(i).map(|(_, t)| t) == Some(&Token::Punct('#'))
            && toks.get(i + 1).map(|(_, t)| t) == Some(&Token::Punct('!'))
            && toks.get(i + 2).map(|(_, t)| t) == Some(&Token::Punct('['));
        let raises = word(toks.get(i + 3), "deny") || word(toks.get(i + 3), "forbid");
        let list =
            toks[(i + 4).min(toks.len())..].iter().take_while(|(_, t)| *t != Token::Punct(')'));
        let names: Vec<&Token> = list.map(|(_, t)| t).collect();
        inner
            && raises
            && names.windows(4).any(|w| {
                matches!(w, [Token::Ident(c), Token::Punct(':'), Token::Punct(':'), Token::Ident(l)]
                    if c == "clippy" && l == "float_arithmetic")
            })
    })
}
