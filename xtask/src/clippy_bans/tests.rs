//! The compiler layer's check on Clippy's own output.
//!
//! Oracle: lines captured from Clippy 0.1.96 (`--message-format=short`) in the container: a ban
//! finding on the fixture, `float_arithmetic`, the two warnings a bad `clippy.toml` path gets
//! (a misspelt `std::path::Path::exits`; `std::fs`, a module, as a method), a compile error,
//! and rows of `cargo clippy -- -W help`'s group table.

use std::collections::BTreeSet;

use super::{ban_groups, diagnostics, entries, forms, judge, Diagnostic, Entry, Form};

const FIXTURE: &str = "xtask/clippy-fixture/src/entries.rs";

const TOML: &str = "disallowed-methods = [\n    { path = \"std::env::var\", reason = \"std-env: the library reads no environment\" },\n    { path = \"std::path::Path::exits\", reason = \"std-fs: a typo\" },\n]\n";

const CAPTURED: &str = "\
    Checking lumen-clippy-fixture v0.1.0 (/work/xtask/clippy-fixture)
/work/clippy.toml:3:5: warning: `std::path::Path::exits` does not refer to a reachable function
/work/exp/a/clippy.toml:4:3: warning: expected a function, found a module
xtask/clippy-fixture/src/entries.rs:8:42: error: use of a disallowed method `std::env::var`
xtask/clippy-fixture/src/entries.rs:21:17: error: floating-point arithmetic detected
src/lib.rs:1:22: error[E0433]: cannot find module or crate `nothere` in this scope: use of unresolved module or unlinked crate `nothere`
error: could not compile `lumen-clippy-fixture` (lib) due to 3 previous errors
";

const HELP: &str = "\
                                   clippy::all  clippy::almost-complete-range, clippy::disallowed-fields, clippy::disallowed-macros, clippy::disallowed-methods, clippy::disallowed-types
                              clippy::pedantic  clippy::assigning-clones, clippy::ignore-without-reason
                           clippy::restriction  clippy::absolute-paths, clippy::float-arithmetic, clippy::as-conversions
                                 clippy::style  clippy::assertions-on-constants, clippy::disallowed-methods, clippy::disallowed-types, clippy::disallowed-macros
                                 clippy::disallowed-methods  warn     use of a disallowed method call
";

fn groups(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|g| (*g).to_owned()).collect()
}

fn the_groups() -> BTreeSet<String> {
    groups(&["clippy::all", "clippy::restriction", "clippy::style"])
}

fn entry(line: usize, path: &str, rule: &str) -> Entry {
    Entry { line, path: path.to_owned(), rule: rule.to_owned() }
}

fn finding(line: usize, message: &str) -> Diagnostic {
    Diagnostic {
        file: FIXTURE.to_owned(),
        line,
        level: "error".to_owned(),
        message: message.to_owned(),
    }
}

/// A seeded `std-env` form on lines 4 to 7 and a look-alike on lines 10 to 12.
fn two_forms() -> Vec<Form> {
    let text = "//! doc\n\n// seeded: std-env\nmod std_env {\n    pub fn f() {}\n\n}\n\n// accepted\nmod looks_alike {\n    pub fn g() {}\n}\n";
    forms(FIXTURE, text)
}

#[test]
fn entries_are_read_with_their_line_path_and_rule() {
    assert_eq!(
        entries(TOML),
        [entry(2, "std::env::var", "std-env"), entry(3, "std::path::Path::exits", "std-fs")]
    );
}

#[test]
fn diagnostics_are_read_from_clippys_short_output() {
    let found = diagnostics(CAPTURED);
    let summary: Vec<(&str, usize, &str)> =
        found.iter().map(|d| (d.file.as_str(), d.line, d.level.as_str())).collect();
    assert_eq!(
        summary,
        [
            ("/work/clippy.toml", 3, "warning"),
            ("/work/exp/a/clippy.toml", 4, "warning"),
            (FIXTURE, 8, "error"),
            (FIXTURE, 21, "error"),
            ("src/lib.rs", 1, "error[E0433]"),
        ]
    );
    assert_eq!(found[2].message, "use of a disallowed method `std::env::var`");
}

#[test]
fn forms_span_from_their_mod_line_to_their_closing_brace() {
    let forms = two_forms();
    let spans: Vec<(&str, (usize, usize), Option<&str>)> =
        forms.iter().map(|f| (f.name.as_str(), f.lines, f.rule.as_deref())).collect();
    assert_eq!(spans.len(), 2, "{forms:?}");
    assert_eq!(spans[0], ("std_env", (4, 7), Some("std-env")));
    assert_eq!(spans[1], ("looks_alike", (10, 12), None));
}

#[test]
fn the_groups_holding_a_ban_lint_are_read_from_the_help_table() {
    assert_eq!(ban_groups(HELP), the_groups());
}

#[test]
fn a_run_where_every_entry_fires_in_its_form_passes() {
    let entries = [entry(2, "std::env::var", "std-env")];
    let found = [finding(5, "use of a disallowed method `std::env::var`")];
    assert_eq!(judge(&entries, &two_forms(), &found, &the_groups()), Vec::<String>::new());
}

#[test]
fn a_misspelt_entry_fails_by_its_warning_and_by_never_firing() {
    let entries = entries(TOML);
    let mut found = diagnostics(CAPTURED);
    found.retain(|d| d.file == "/work/clippy.toml");
    found.push(finding(5, "use of a disallowed method `std::env::var`"));
    let failures = judge(&entries, &two_forms(), &found, &the_groups());
    assert_eq!(failures.len(), 2, "{failures:?}");
    assert!(
        failures[0].starts_with("clippy.toml:3: `std::path::Path::exits` does not refer"),
        "{failures:?}"
    );
    assert!(
        failures[1].starts_with("clippy.toml:3: `std::path::Path::exits` never fires"),
        "{failures:?}"
    );
}

#[test]
fn an_unreported_form_a_reported_look_alike_and_a_stray_finding_fail() {
    let entries = [entry(2, "std::env::var", "std-env")];
    let var = "use of a disallowed method `std::env::var`";
    let failures =
        judge(&entries, &two_forms(), &[finding(11, var), finding(14, var)], &the_groups());
    assert_eq!(failures.len(), 3, "{failures:?}");
    assert!(failures[0].starts_with("look-alike looks_alike is reported"), "{failures:?}");
    assert!(failures[1].contains("a finding outside every marked form"), "{failures:?}");
    assert!(
        failures[2].starts_with("seeded form std_env is not reported under std-env"),
        "{failures:?}"
    );
}

#[test]
fn a_finding_under_another_rule_does_not_report_a_form() {
    let entries = [entry(2, "std::env::var", "std-env"), entry(3, "std::fs::read", "std-fs")];
    let found = [
        finding(5, "use of a disallowed method `std::env::var`"),
        finding(6, "use of a disallowed method `std::fs::read`"),
    ];
    let forms = forms(FIXTURE, "// seeded: std-fs\nmod std_env {\n    pub fn f() {}\n\n\n\n}\n");
    let only_env = judge(&entries[..1], &forms, &found[..1], &the_groups());
    assert_eq!(only_env.len(), 1, "{only_env:?}");
    assert!(only_env[0].starts_with("seeded form std_env is not reported under std-fs"));
    assert_eq!(judge(&entries, &forms, &found, &the_groups()), Vec::<String>::new());
}

#[test]
fn a_fixture_that_does_not_compile_fails() {
    let mut found = diagnostics(CAPTURED);
    found.retain(|d| d.level.starts_with("error["));
    found.push(finding(5, "use of a disallowed method `std::env::var`"));
    let failures =
        judge(&[entry(2, "std::env::var", "std-env")], &two_forms(), &found, &the_groups());
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0].starts_with("the fixture does not compile"), "{failures:?}");
}

#[test]
fn a_toolchain_that_moves_a_ban_lint_to_another_group_fails() {
    let entries = [entry(2, "std::env::var", "std-env")];
    let found = [finding(5, "use of a disallowed method `std::env::var`")];
    let moved = groups(&["clippy::all", "clippy::restriction", "clippy::suspicious"]);
    assert_eq!(judge(&entries, &two_forms(), &found, &moved).len(), 1);
}

#[test]
fn float_arithmetic_reports_a_float_type_form() {
    let forms = forms(FIXTURE, "// seeded: float-type\nmod inferred {\n    pub fn f() {}\n}\n");
    let found = [finding(3, "floating-point arithmetic detected")];
    assert_eq!(judge(&[], &forms, &found, &the_groups()), Vec::<String>::new());
    assert_eq!(judge(&[], &forms, &[], &the_groups()).len(), 1);
}

#[test]
fn a_fixture_with_only_look_alikes_fails() {
    let forms = forms(FIXTURE, "// accepted\nmod looks_alike {\n    pub fn g() {}\n}\n");
    let failures = judge(&[], &forms, &[], &the_groups());
    assert_eq!(failures, ["the fixture has no seeded form"]);
}
