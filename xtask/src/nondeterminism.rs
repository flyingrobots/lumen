//! The nondeterminism scan: what Clippy's bans in `clippy.toml` cannot see
//! (docs/adr/0006-nondeterminism-gate.md).
//!
//! Clippy resolves paths exactly but only in code it compiles, and has no rule for a float
//! literal. This scan reads tokens instead, so it also reads code under a `cfg` that is not built
//! and macro bodies, and ignores comments and strings. In the deterministic core it reports
//! float types, float literals and casts to a float; the names `Instant`, `SystemTime`,
//! `UNIX_EPOCH`, `available_parallelism`, `HashMap`, `HashSet` and `RandomState`; the
//! `env!` and `option_env!` macros; names with the word `f32` or `f64` or ending `_float`
//! (`as_secs_f64`, `into_float`); Bunny's float-bridge types (`bunny_linalg::Vec3`, `bunny_codec::PlyVertex`); and
//! paths into `std::env`, `std::fs`, `std::process`, `std::net` and the `std::thread` sleeps,
//! written out, raw (`std::r#fs`), through a `use` tree (groups, `self`, renames), through a name
//! a `use` brought in, or through an `extern crate std as ...` alias, chained. Any line that names open addressing is
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

use paths::{imports, paths, resolve};
use tokens::{is_float, tokens, Token};
use waivers::apply_waivers;
pub(crate) use waivers::{waiver_forms, BAN_GROUPS};

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

/// Banned path prefixes and their rules. Each is compared segment by segment after the path is
/// resolved through the file's imports ([`resolve`]); the `sleep` family is listed by name
/// because `sleep_ms` is not under `sleep`. The Bunny paths are its float bridge, whose values
/// carry floats with no float token (`PlyVertex.x`), and `bunny_num::is_finite`, which takes one
/// and whose name has no float word.
const PATHS: [(&[&str], &str); 21] = [
    (&["std", "env"], "std-env"),
    (&["std", "fs"], "std-fs"),
    (&["std", "process"], "std-process"),
    (&["std", "net"], "network"),
    (&["std", "thread", "sleep"], "thread-sleep"),
    (&["std", "thread", "sleep_ms"], "thread-sleep"),
    (&["std", "thread", "sleep_until"], "thread-sleep"),
    (&["std", "thread", "park_timeout"], "thread-sleep"),
    (&["std", "thread", "park_timeout_ms"], "thread-sleep"),
    (&["bunny_num", "Scalar"], "float-type"),
    (&["bunny_num", "is_finite"], "float-type"),
    (&["bunny_linalg", "Vec2"], "float-type"),
    (&["bunny_linalg", "Vec3"], "float-type"),
    (&["bunny_geom", "Ray3"], "float-type"),
    (&["bunny_geom", "Aabb3"], "float-type"),
    (&["bunny_geom", "Sphere3"], "float-type"),
    (&["bunny_codec", "ObjMesh"], "float-type"),
    (&["bunny_codec", "ObjVertex"], "float-type"),
    (&["bunny_codec", "ObjVertices"], "float-type"),
    (&["bunny_codec", "PlyBinaryMesh"], "float-type"),
    (&["bunny_codec", "PlyVertex"], "float-type"),
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
            // A name with the word `f32` or `f64` (`as_secs_f64`, `to_f32`), or ending `_float`
            // (Bunny's `into_float`): a float passes through it even when no float type or
            // literal is written. Not `float_arithmetic`, the lint `lib.rs` denies.
            Token::Ident(name)
                if name.split('_').any(|w| w == "f32" || w == "f64")
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
    for (line, path) in paths(&toks) {
        let readings = resolve(&path, &imports);
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
