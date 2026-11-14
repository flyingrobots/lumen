//! Waivers that are not comments, and the lint-level attributes that turn a Clippy ban off.
//!
//! Oracle: what Clippy 0.1.96 does, run on a fixture crate with Lumen's lint levels
//! (`clippy::all` denied, `float_arithmetic` denied) and bans: `allow` of `clippy::all`, of
//! `clippy::style` and of the renamed `clippy::disallowed_method`, `allow` inside `cfg_attr`, and
//! `warn` of a ban lint or of `clippy::all` under `allow(warnings)` each silenced a
//! `disallowed_methods` finding; `allow(clippy::restriction)` silenced `float_arithmetic`;
//! `allow(warnings)` alone silenced nothing. The groups are from `cargo clippy -- -W help`.

use super::super::{core_findings, CrateAliases};
use super::waiver_forms;

fn reported(source: &str) -> Vec<(usize, &'static str)> {
    core_findings(source, &CrateAliases::default()).iter().map(|f| (f.line, f.rule)).collect()
}

fn forms(text: &str) -> Vec<usize> {
    waiver_forms(text).iter().map(|f| f.line).collect()
}

#[test]
fn a_waiver_inside_a_string_or_a_doc_comment_waives_nothing() {
    let in_string = "pub const S: &str = \"// nondeterminism: allow float-literal -- a string\"; pub const K: u32 = 1.5 as u32;";
    assert_eq!(reported(in_string), [(1, "float-literal")]);
    let in_raw = "pub const S: &str = r#\"\n// nondeterminism: allow float-literal -- a string\n\"#; pub const K: u32 = 1.5 as u32;";
    assert_eq!(reported(in_raw), [(3, "float-literal")]);
    let in_doc =
        "/// nondeterminism: allow float-literal -- a doc comment\npub const K: u32 = 1.5 as u32;";
    assert_eq!(reported(in_doc), [(2, "float-literal")]);
    let in_block = "/* nondeterminism: allow float-literal -- a block comment */\npub const K: u32 = 1.5 as u32;";
    assert_eq!(reported(in_block), [(2, "float-literal")]);
}

#[test]
fn a_waiver_comment_after_code_or_alone_still_waives() {
    let trailing =
        "pub const K: u32 = 1.5 as u32; // nondeterminism: allow float-literal -- a reason";
    assert_eq!(reported(trailing), []);
    let after_string = "pub const S: &str = \"//\"; // nondeterminism: allow float-type -- a reason\npub type F = f64;";
    assert_eq!(reported(after_string), []);
}

#[test]
fn lint_groups_that_hold_a_ban_lint_fail_at_any_lowering_level() {
    for attribute in [
        "#[allow(clippy::all)]",
        "#[allow(clippy::style)]",
        "#![allow(clippy::restriction)]",
        "#[allow(dead_code, clippy::all)]",
        "#[expect(clippy::all, reason = \"std-env: grouped\")]",
        "#![allow(warnings)]",
        "#[warn(clippy::all)]",
        "#[cfg_attr(all(), allow(clippy::style))]",
    ] {
        assert_eq!(forms(&format!("\n{attribute}\npub fn f() {{}}")), [2], "{attribute}");
    }
}

#[test]
fn a_ban_lint_lowered_to_warn_or_under_its_old_name_fails() {
    for attribute in [
        "#[warn(clippy::disallowed_methods)]",
        "#[warn(clippy::disallowed_methods, reason = \"std-env: a well-formed reason\")]",
        "#[allow(clippy::disallowed_method)]",
        "#[expect(clippy::disallowed_type, reason = \"unordered-container: old name\")]",
    ] {
        assert_eq!(forms(attribute), [1], "{attribute}");
    }
}

#[test]
fn a_lint_list_built_by_a_macro_fails() {
    let built = "macro_rules! quiet {\n    ($lint:ident) => {\n        #[allow(clippy::$lint)]\n        pub fn f() {}\n    };\n}";
    assert_eq!(forms(built), [3]);
    let level = "macro_rules! level {\n    ($kind:ident) => {\n        #[$kind(clippy::all)]\n        pub fn f() {}\n    };\n}";
    assert_eq!(reported(level), [(3, "waiver")]);
    let passed = "macro_rules! with {\n    ($m:meta) => {\n        #[$m]\n        pub fn f() {}\n    };\n}\nwith!(allow(clippy::all));";
    assert_eq!(reported(passed), [(3, "waiver")]);
    assert_eq!(forms(passed), [7]);
}

#[test]
fn raising_levels_and_other_lints_pass() {
    for text in [
        "#[allow(clippy::cast_possible_truncation, reason = \"n < 256\")]",
        "#[expect(clippy::disallowed_methods, reason = \"std-env: the command's edge\")]",
        "#![deny(clippy::all)]\n#![forbid(warnings)]",
        "#[warn(clippy::pedantic)]",
        "pub const W: &str = \"allow(warnings) allow(clippy::all)\";",
        "macro_rules! id {\n    ($e:expr) => {\n        $e\n    };\n}",
    ] {
        assert_eq!(forms(text), Vec::<usize>::new(), "{text}");
        assert_eq!(reported(text), [], "{text}");
    }
}

#[test]
fn a_lint_level_built_by_a_macro_fails_in_every_file() {
    let level = "macro_rules! level {\n    ($kind:ident) => {\n        #[$kind(clippy::disallowed_types)]\n        pub fn f() {}\n    };\n}\nlevel!(allow);";
    assert_eq!(forms(level), [3]);
    let forwarded =
        "macro_rules! with {\n    ($m:meta) => {\n        #[$m]\n        pub fn f() {}\n    };\n}";
    assert_eq!(
        forms(forwarded),
        Vec::<usize>::new(),
        "forwarding a whole attribute is the core's rule"
    );
}

#[test]
fn functions_and_methods_named_like_levels_are_not_lint_lists() {
    for text in [
        "pub fn warn(warnings: usize) -> usize { warnings }",
        "pub fn allow(all: u8) -> u8 { all }",
        "pub fn f(o: Option<u8>, warnings: &str) -> u8 { o.expect(warnings) }",
    ] {
        assert_eq!(forms(text), Vec::<usize>::new(), "{text}");
    }
    assert_eq!(forms("quiet!(allow(clippy::all));"), [1], "a lint list passed to a macro");
}
