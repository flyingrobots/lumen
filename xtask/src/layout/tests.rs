//! The module tree on constructed crates: roles, the deterministic core, and the forms that load
//! a file the gates do not read.
//!
//! Oracle: the Rust reference's "Module source filenames" and "The path attribute" rules, with
//! each expected file and role written by hand; `#[path = concat!(..)]` is refused by `rustc`
//! 1.96.0 ("malformed `path` attribute input"), and a `path` inside `cfg_attr` is accepted by it.

use super::{Layout, Role};

const LIB: &str = "crates/lumen/src/lib.rs";
const CRATES: [&str; 2] = ["crates/lumen", "xtask"];

/// The layout of `files` in the two crates.
fn layout(files: &[(&str, &str)]) -> Layout {
    let files: Vec<(String, String)> =
        files.iter().map(|(p, t)| ((*p).to_owned(), (*t).to_owned())).collect();
    Layout::new(&files, &CRATES.map(str::to_owned))
}

/// The failures of `files`, as (path, line).
fn failures(files: &[(&str, &str)]) -> Vec<(String, usize)> {
    layout(files).failures.into_iter().map(|f| (f.path, f.line)).collect()
}

#[test]
fn a_test_file_is_one_reached_only_through_cfg_test() {
    let tree = layout(&[
        (LIB, "mod render;\nmod scene;\n#[cfg(test)]\nmod generated;\n"),
        ("crates/lumen/src/render.rs", "pub(crate) fn f() {}"),
        ("crates/lumen/src/scene/mod.rs", "#[cfg(test)]\npub(crate) mod tests;"),
        ("crates/lumen/src/scene/tests.rs", ""),
        ("crates/lumen/src/generated.rs", ""),
        ("crates/lumen/tests/render.rs", "mod golden;"),
        ("crates/lumen/tests/golden/mod.rs", ""),
        ("xtask/src/main.rs", "#[cfg(test)]\nmod tests;"),
        ("xtask/src/tests.rs", ""),
    ]);
    for (path, role) in [
        ("crates/lumen/src/render.rs", Role::Source),
        ("crates/lumen/src/scene/mod.rs", Role::Source),
        ("crates/lumen/src/scene/tests.rs", Role::Test),
        ("crates/lumen/src/generated.rs", Role::Test),
        ("crates/lumen/tests/render.rs", Role::Test),
        ("crates/lumen/tests/golden/mod.rs", Role::Test),
        ("xtask/src/tests.rs", Role::Test),
        ("xtask/src/main.rs", Role::Source),
    ] {
        assert_eq!(tree.role(path), role, "{path}");
    }
    assert!(tree.is_core("crates/lumen/src/render.rs"));
    assert!(!tree.is_core("crates/lumen/src/scene/tests.rs"));
    assert!(!tree.is_core("xtask/src/main.rs"));
}

#[test]
fn a_module_named_tests_without_cfg_test_is_source_and_core() {
    let tree = layout(&[
        (LIB, "mod tests;\nmod other;"),
        ("crates/lumen/src/tests/mod.rs", "mod helpers;"),
        ("crates/lumen/src/tests/helpers.rs", ""),
        ("crates/lumen/src/other.rs", "mod tests;"),
        ("crates/lumen/src/other/tests.rs", ""),
    ]);
    for path in [
        "crates/lumen/src/tests/mod.rs",
        "crates/lumen/src/tests/helpers.rs",
        "crates/lumen/src/other/tests.rs",
    ] {
        assert_eq!(tree.role(path), Role::Source, "{path}");
        assert!(tree.is_core(path), "{path}");
    }
}

#[test]
fn cfg_test_on_an_inline_module_or_the_file_itself_makes_a_test_file() {
    let tree = layout(&[
        (LIB, "#[cfg(test)]\nmod t {\n    mod deep;\n}\nmod only;\nmod after;"),
        ("crates/lumen/src/t/deep.rs", ""),
        ("crates/lumen/src/only.rs", "#![cfg(test)]\nmod inner;"),
        ("crates/lumen/src/only/inner.rs", ""),
        ("crates/lumen/src/after.rs", ""),
    ]);
    assert_eq!(tree.role("crates/lumen/src/t/deep.rs"), Role::Test);
    assert_eq!(tree.role("crates/lumen/src/only.rs"), Role::Test);
    assert_eq!(tree.role("crates/lumen/src/only/inner.rs"), Role::Test);
    assert_eq!(tree.role("crates/lumen/src/after.rs"), Role::Source);
}

#[test]
fn paths_resolve_as_the_reference_says() {
    let n = "#[cfg(test)]\nmod t;\n#[cfg(test)]\n#[path = \"x.rs\"]\nmod y;\n\
             mod inl {\n    #[cfg(test)]\n    #[path = \"p.rs\"]\n    mod q;\n}";
    let tree = layout(&[
        (LIB, "#[path = \"a/b.rs\"]\nmod b;\nmod n;"),
        // A file loaded by `#[path]` is `mod.rs`-like: its `mod c;` is `a/c.rs`, not `a/b/c.rs`.
        ("crates/lumen/src/a/b.rs", "#[cfg(test)]\nmod c;"),
        ("crates/lumen/src/a/c.rs", ""),
        // In the non-`mod.rs` file `n.rs`, `mod t;` is `n/t.rs`; a `#[path]` is relative to its
        // directory, and inside an inline module to `n/` and the inline module's directory.
        ("crates/lumen/src/n.rs", n),
        ("crates/lumen/src/n/t.rs", ""),
        ("crates/lumen/src/x.rs", ""),
        ("crates/lumen/src/n/inl/p.rs", ""),
    ]);
    for path in [
        "crates/lumen/src/a/c.rs",
        "crates/lumen/src/n/t.rs",
        "crates/lumen/src/x.rs",
        "crates/lumen/src/n/inl/p.rs",
    ] {
        assert_eq!(tree.role(path), Role::Test, "{path}");
    }
    assert_eq!(tree.role("crates/lumen/src/a/b.rs"), Role::Source);
}

#[test]
fn a_library_module_outside_src_is_core_and_source() {
    let tree = layout(&[
        (LIB, "#[path = \"../tests/evil.rs\"]\nmod evil;"),
        ("crates/lumen/tests/evil.rs", ""),
        ("crates/lumen/src/bin/lumen/main.rs", "mod cli;"),
        ("crates/lumen/src/bin/lumen/cli.rs", "#[cfg(test)]\nmod tests;"),
        ("crates/lumen/src/bin/lumen/cli/tests.rs", ""),
        ("crates/lumen/src/orphan.rs", ""),
        ("crates/lumen/tests/orphan.rs", ""),
    ]);
    assert!(tree.is_core("crates/lumen/tests/evil.rs"));
    assert_eq!(tree.role("crates/lumen/tests/evil.rs"), Role::Source);
    assert!(!tree.is_core("crates/lumen/src/bin/lumen/cli.rs"));
    assert_eq!(tree.role("crates/lumen/src/bin/lumen/cli/tests.rs"), Role::Test);
    assert!(tree.is_core("crates/lumen/src/orphan.rs"), "an unreached library file is core");
    assert_eq!(tree.role("crates/lumen/tests/orphan.rs"), Role::Test);
    assert!(tree.failures.is_empty(), "{:?}", tree.failures);
}

#[test]
fn a_form_that_loads_a_file_the_gates_do_not_read_fails() {
    let inc = ("crates/lumen/src/x.inc", "");
    for (source, line) in [
        ("pub fn f() -> u32 { include!(\"x.inc\") }", 1),
        ("pub fn f() -> u32 {\n    core::include!(\"x.inc\")\n}", 2),
        ("#[path = \"x.inc\"]\nmod hidden;", 2),
        ("#[path = \"../../../../outside.rs\"]\nmod hidden;", 2),
        ("#[path = \"missing.rs\"]\nmod hidden;", 2),
        ("#[cfg_attr(all(), path = \"x.rs\")]\nmod hidden;", 1),
    ] {
        assert_eq!(failures(&[(LIB, source), inc]), [(LIB.to_owned(), line)], "{source:?}");
    }
    let back_in = "#[path = \"../../../../crates/lumen/src/x.rs\"]\nmod hidden;";
    let x = ("crates/lumen/src/x.rs", "");
    assert_eq!(
        failures(&[(LIB, back_in), x]),
        [(LIB.to_owned(), 2)],
        "a path above the repository"
    );
    let tests = "crates/lumen/tests/t.rs";
    assert_eq!(
        failures(&[(tests, "const X: u32 = include!(\"x.inc\");")]),
        [(tests.to_owned(), 1)]
    );
}

#[test]
fn look_alikes_of_loading_forms_do_not_fail() {
    let source = "pub const PLY: &str = include_str!(\"bunny.ply\");\n\
                  pub fn f(include: u32) -> u32 { include }\n\
                  // include!(\"x.inc\") in a comment\n\
                  pub const S: &str = \"include!(\\\"x.inc\\\")\";\n\
                  #[cfg_attr(test, allow(dead_code))]\nmod a;";
    assert_eq!(failures(&[(LIB, source), ("crates/lumen/src/a.rs", "")]), []);
}

#[test]
fn a_circular_path_declaration_terminates() {
    let tree = layout(&[
        (LIB, "#[path = \"lib.rs\"]\nmod again;\n#[cfg(test)]\nmod t;"),
        ("crates/lumen/src/t.rs", ""),
    ]);
    assert_eq!(tree.role(LIB), Role::Source);
    assert_eq!(tree.role("crates/lumen/src/t.rs"), Role::Test);
}
