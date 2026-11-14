//! Aliases across files and paths built by macros (flyingrobots/lumen#157), through
//! [`core_findings`] with the aliases of every file of a constructed crate.
//!
//! Oracle: each case is written by hand from Rust's name resolution (an `extern crate` at the
//! crate root is in every module's scope; a `pub use` is reachable through its module's path)
//! with the rule and line that must be reported in the file read. The `cfg` on each form is
//! one a Linux Clippy run does not build; the scan must not care.

use super::{core_findings, crate_aliases};

/// The other files of the crate, the file read, the rule and line that must be reported in it.
pub(super) type Case = (&'static str, &'static [&'static str], &'static str, &'static str, usize);

const ROOT_HOST: &str = "extern crate std as host;\nmod probe_a;\nmod probe_b;";
const REEXPORT: &str = "pub use std as reexported;";

pub(super) const CASES: &[Case] = &[
    ("a crate-root extern crate alias under cfg(windows)", &[ROOT_HOST], "#[cfg(windows)]\npub fn f() -> bool { host::env::var_os(\"X\").is_some() }", "std-env", 2),
    ("a pub use std re-export under cfg(wasm32)", &[REEXPORT], "#[cfg(target_arch = \"wasm32\")]\npub fn f() -> bool { crate::probe_b::reexported::fs::metadata(\"x\").is_ok() }", "std-fs", 2),
    ("a private use std at the root", &["use std as s;"], "#[cfg(windows)]\npub fn f() -> bool { crate::s::env::var_os(\"X\").is_some() }", "std-env", 2),
    ("pub use ::std with a leading ::", &["pub use ::std as x;"], "pub fn f() -> bool { crate::probe_b::x::env::var_os(\"X\").is_some() }", "std-env", 1),
    ("an alias of the root alias", &["pub extern crate std as host;", "pub use host as h2;"], "pub fn f() -> bool { crate::probe_b::h2::fs::exists(\"x\").is_ok() }", "std-fs", 1),
    ("a module re-export, then a sleep", &["pub use std::thread as t;"], "#[cfg(windows)]\npub fn f() { crate::probe_b::t::sleep(core::time::Duration::ZERO) }", "thread-sleep", 2),
    ("super::super to the root alias", &[ROOT_HOST], "pub mod inner {\n    #[cfg(target_os = \"macos\")]\n    pub fn f() -> bool { super::super::host::fs::read(\"x\").is_ok() }\n}", "std-fs", 3),
    ("a fn-scoped rename of a cross-file re-export", &[REEXPORT], "pub fn f() -> bool {\n    use crate::probe_b::reexported as r;\n    r::env::var_os(\"X\").is_some()\n}", "std-env", 3),
    ("$crate and the root alias in a macro", &[ROOT_HOST], "macro_rules! home { () => { $crate::host::env::var_os(\"HOME\").is_some() } }", "std-env", 1),
    ("a re-export of std from another deterministic crate", &["pub use std as x;"], "pub fn f() -> bool { probe_dep::x::env::var_os(\"X\").is_some() }", "std-env", 1),
    ("std::$m::var_os in a macro (flyingrobots/lumen#157)", &[], "macro_rules! built { ($m:ident) => { std::$m::var_os(\"X\").is_some() } }", "hidden-path", 1),
    ("a metavariable root, $root::fs", &[], "macro_rules! built { ($root:path) => { $root::fs::read(\"x\").is_ok() } }", "hidden-path", 1),
    ("every segment a metavariable", &[], "macro_rules! call {\n    ($a:ident $b:ident $c:ident) => { $a::$b::$c(\"X\").is_ok() }\n}", "hidden-path", 2),
];

/// The (line, rule) findings of `read` with the aliases of `others` and `read` together.
fn findings(others: &[&str], read: &str) -> Vec<(usize, &'static str)> {
    let mut texts = others.to_vec();
    texts.push(read);
    let aliases = crate_aliases(&texts, &["lumen".to_owned(), "probe_dep".to_owned()]);
    core_findings(read, &aliases).iter().map(|f| (f.line, f.rule)).collect()
}

#[test]
fn an_alias_declared_in_another_file_is_followed() {
    let missed: Vec<String> = CASES
        .iter()
        .filter(|(_, others, read, rule, line)| !findings(others, read).contains(&(*line, *rule)))
        .map(|(label, others, read, rule, line)| {
            format!("{label}: expected [{rule}] at {line}, got {:?}", findings(others, read))
        })
        .collect();
    assert!(
        missed.is_empty(),
        "{} of {} missed:\n{}",
        missed.len(),
        CASES.len(),
        missed.join("\n")
    );
}

#[test]
fn the_same_paths_without_the_other_file_are_not_followed() {
    for (label, others, read, rule, line) in
        CASES.iter().filter(|(_, others, ..)| !others.is_empty())
    {
        let alone = findings(&[], read);
        assert!(!alone.contains(&(*line, *rule)), "{label}: {alone:?} (with {others:?})");
    }
}

#[test]
fn a_glob_of_a_std_module_is_a_hidden_path() {
    for glob in
        ["pub use std::*;", "use core::ops::*;", "extern crate std as host;\nuse host::thread::*;"]
    {
        let lines = glob.lines().count();
        assert!(findings(&[], glob).contains(&(lines, "hidden-path")), "{glob}");
    }
}

#[test]
fn macros_and_crate_paths_that_resolve_to_nothing_banned_pass() {
    let others = ["pub use crate::schedule::DEFAULT_BAND_ROWS as ROWS;\npub mod env { pub const X: u32 = 1; }", "use std::io::Write;"];
    for read in [
        "pub fn f() -> u32 { crate::probe_b::ROWS.get() + crate::probe_b::env::X }",
        "macro_rules! twice { ($x:expr) => { $x + $x } }\nmacro_rules! make { ($t:ty) => { <$t>::default() } }",
        "macro_rules! new { ($t:ident) => { $t::new() } }",
        "macro_rules! widen { ($x:expr) => { $crate::schedule::widen($x) } }",
        "use super::*;\nuse crate::scene::*;",
        "pub fn f(w: &mut dyn crate::probe_b::Write) {}",
    ] {
        assert_eq!(findings(&others, read), [], "{read}");
    }
}
