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
mod paths;
#[cfg(test)]
mod tests;
pub(crate) mod tokens;
mod waivers;

pub(crate) use paths::{crate_aliases, CrateAliases};
use paths::{imports, macro_paths, paths, resolve};
use tokens::{is_float, tokens, Token};
use waivers::apply_waivers;
pub(crate) use waivers::{waiver_forms, BAN_GROUPS};

/// Every rule ID: Echo's `ban-nondeterminism` vocabulary, then Lumen's own.
pub(crate) const RULES: [&str; 21] = [
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
    "hidden-path",
];

/// The std modules that hold a banned item: a path a macro builds from a metavariable root
/// (`$root::fs`) through one of them is refused, since the scan cannot know the root.
const STD_MODULES: [&str; 16] = [
    "env",
    "fs",
    "process",
    "net",
    "os",
    "io",
    "thread",
    "time",
    "collections",
    "hash",
    "arch",
    "ffi",
    "panic",
    "any",
    "backtrace",
    "primitive",
];

/// The roots of the standard library.
const STD_ROOTS: [&str; 3] = ["std", "core", "alloc"];

/// The float types: `f16` and `f128` are unstable on the pinned toolchain, and named so that
/// a toolchain that stabilises them cannot bring one into the core unseen.
const FLOAT_TYPES: [&str; 4] = ["f16", "f32", "f64", "f128"];

/// Names banned wherever they are spelled, and their rules. `c_double` and `c_float` are std's
/// aliases of `f64` and `f32` (`core::ffi`, `std::ffi`, `std::os::raw`): a float type with no
/// float token.
const NAMES: [(&str, &str); 11] = [
    ("Instant", "time-instant"),
    ("SystemTime", "time-system"),
    ("UNIX_EPOCH", "time-system"),
    ("available_parallelism", "host-parallelism"),
    ("HashMap", "unordered-container"),
    ("HashSet", "unordered-container"),
    ("RandomState", "unordered-container"),
    ("FxHashMap", "unordered-container"),
    ("FxHashSet", "unordered-container"),
    ("c_double", "float-type"),
    ("c_float", "float-type"),
];

/// Banned path prefixes and their rules. Each is compared segment by segment after the path is
/// resolved through the file's imports ([`resolve`]); the `sleep` family is listed by name
/// because `sleep_ms` is not under `sleep`. The Bunny types are its float bridge, whose values
/// carry floats with no float token (`PlyVertex.x`).
const PATHS: [(&[&str], &str); 20] = [
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
    for (line, path) in paths(&toks) {
        let readings = resolve(&path, &imports, aliases);
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
