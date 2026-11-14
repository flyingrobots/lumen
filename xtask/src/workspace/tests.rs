//! The workspace boundary on constructed `cargo metadata` output, and the JSON reader.
//!
//! Oracle: each metadata text is written by hand in the shape `cargo metadata --no-deps
//! --format-version 1` prints (cargo 1.96.0: `packages[].dependencies[].{name, source, kind,
//! path}`, `kind` `null` for a normal dependency, `targets[].{kind, src_path}`), with the
//! deterministic crates and failures each case must give written out.

use super::json::{parse, Json};
use super::{boundary, packages, Boundary};

const REV: &str = "d4fe51d38fa10f8d6496bd110955314edac30bbf";

/// A dependency: name, kind (`None` for normal), path (relative to /w) or source.
type D<'a> = (&'a str, Option<&'a str>, Option<&'a str>, Option<&'a str>);

/// A package at `/w/<dir>` with its target kinds and dependencies, as metadata JSON.
fn package(name: &str, dir: &str, kinds: &[&str], deps: &[D<'_>]) -> String {
    let targets: Vec<String> = kinds
        .iter()
        .map(|k| format!(r#"{{"kind":["{k}"],"name":"{name}","src_path":"/w/{dir}/src/{k}.rs"}}"#))
        .collect();
    let deps: Vec<String> = deps
        .iter()
        .map(|(n, kind, path, source)| {
            let quote = |v: Option<&str>| v.map_or("null".to_owned(), |v| format!("\"{v}\""));
            let path = path.map_or("null".to_owned(), |p| {
                if p.starts_with('/') {
                    format!("\"{p}\"")
                } else {
                    format!("\"/w/{p}\"")
                }
            });
            format!(
                r#"{{"name":"{n}","kind":{},"path":{path},"source":{}}}"#,
                quote(*kind),
                quote(*source)
            )
        })
        .collect();
    format!(
        r#"{{"name":"{name}","id":"path+file:///w/{dir}#{name}@0.1.0","manifest_path":"/w/{dir}/Cargo.toml","targets":[{}],"dependencies":[{}]}}"#,
        targets.join(","),
        deps.join(",")
    )
}

/// Metadata holding `packages`, every one a workspace member.
fn metadata(packages: &[String]) -> String {
    let ids: Vec<String> = packages
        .iter()
        .filter_map(|p| {
            p.split(r#""id":""#).nth(1)?.split('"').next().map(|id| format!("\"{id}\""))
        })
        .collect();
    format!(
        r#"{{"packages":[{}],"workspace_members":[{}],"workspace_root":"/w","version":1}}"#,
        packages.join(","),
        ids.join(",")
    )
}

/// The boundary of `packages`.
fn judged(packages_json: &[String]) -> Boundary {
    boundary(&packages(&metadata(packages_json)).expect("metadata reads"))
}

/// The tooling members, as they are today.
fn tooling() -> Vec<String> {
    vec![
        package("xtask", "xtask", &["bin"], &[]),
        package("lumen-clippy-fixture", "xtask/clippy-fixture", &["lib", "custom-build"], &[]),
    ]
}

/// Bunny's repository at the pinned commit, as metadata gives a git dependency's source.
fn bunny() -> String {
    format!("git+https://github.com/flyingrobots/bunny?rev={REV}")
}

#[test]
fn the_deterministic_crates_are_those_the_library_reaches_by_normal_and_build_edges() {
    let source = bunny();
    let mut all = tooling();
    all.push(package(
        "lumen",
        "crates/lumen",
        &["lib", "bin"],
        &[
            ("bunny-num", None, None, Some(&source)),
            ("mid", None, Some("crates/mid"), None),
            ("helper", Some("build"), Some("crates/helper"), None),
            ("test-only", Some("dev"), Some("crates/test-only"), None),
        ],
    ));
    all.push(package("mid", "crates/mid", &["lib"], &[("leaf", None, Some("crates/leaf"), None)]));
    all.push(package("leaf", "crates/leaf", &["lib"], &[]));
    all.push(package("helper", "crates/helper", &["lib"], &[]));
    all.push(package("test-only", "crates/test-only", &["lib"], &[]));
    let judged = judged(&all);
    let names: Vec<&str> = judged.libraries.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["lumen", "mid", "leaf", "helper"], "dev edges do not reach");
    assert_eq!(judged.libraries[0].root, "crates/lumen/src/lib.rs");
    assert_eq!(judged.libraries[0].dir, "crates/lumen");
    assert_eq!(judged.failures.len(), 1, "{:?}", judged.failures);
    assert!(judged.failures[0].contains("`test-only` (crates/test-only) is not classified"));
}

#[test]
fn todays_workspace_passes() {
    let mut all = tooling();
    let source = bunny();
    all.push(package(
        "lumen",
        "crates/lumen",
        &["lib", "bin"],
        &[("bunny-num", None, None, Some(&source))],
    ));
    let judged = judged(&all);
    assert_eq!(judged.failures, Vec::<String>::new());
    assert_eq!(judged.libraries.len(), 1);
}

#[test]
fn a_tooling_crate_the_library_depends_on_fails() {
    let mut all = tooling();
    all.push(package("lumen", "crates/lumen", &["lib"], &[("xtask", None, Some("xtask"), None)]));
    let failures = judged(&all).failures;
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0].contains("`xtask` is classified as tooling, but the library depends on it"));
}

#[test]
fn dependencies_from_outside_the_workspace_and_bunny_fail() {
    let cases = [
        (("outside", None, Some("/elsewhere/outside"), None), "a path outside the workspace"),
        (("rand", None, None, Some("registry+https://github.com/rust-lang/crates.io-index")), "only Bunny"),
        (("other", None, None, Some("git+https://github.com/someone/else?rev=d4fe51d38fa10f8d6496bd110955314edac30bbf")), "only Bunny"),
        (("short", None, None, Some("git+https://github.com/flyingrobots/bunny?rev=d4fe51d")), "only Bunny"),
        (("branch", None, None, Some("git+https://github.com/flyingrobots/bunny?branch=main")), "only Bunny"),
        (("upper", None, None, Some("git+https://github.com/flyingrobots/bunny?rev=D4FE51D38FA10F8D6496BD110955314EDAC30BBF")), "only Bunny"),
        (("nowhere", Some("dev"), None, None), "neither a path nor a source"),
    ];
    for (dep, what) in cases {
        let mut all = tooling();
        all.push(package("lumen", "crates/lumen", &["lib"], &[dep]));
        let failures = judged(&all).failures;
        assert_eq!(failures.len(), 1, "{}: {failures:?}", dep.0);
        assert!(failures[0].contains(what), "{}: {failures:?}", dep.0);
    }
}

#[test]
fn a_build_script_or_a_procedural_macro_in_a_deterministic_crate_fails() {
    for (kinds, what) in [
        (&["lib", "custom-build"][..], "a build script"),
        (&["proc-macro"][..], "a procedural macro"),
    ] {
        let mut all = tooling();
        all.push(package(
            "lumen",
            "crates/lumen",
            &["lib"],
            &[("dep", Some("build"), Some("crates/dep"), None)],
        ));
        all.push(package("dep", "crates/dep", kinds, &[]));
        let failures = judged(&all).failures;
        assert_eq!(failures.len(), 1, "{what}: {failures:?}");
        assert!(failures[0].contains(&format!("`dep` is a deterministic crate and has {what}")));
    }
    let mut own = tooling();
    own.push(package("lumen", "crates/lumen", &["lib", "custom-build"], &[]));
    assert_eq!(judged(&own).failures.len(), 1, "lumen's own build script");
}

#[test]
fn only_workspace_members_are_read_and_no_library_package_fails() {
    let outsider = r#"{"name":"serde","id":"registry+x#serde@1.0.0","manifest_path":"/r/serde/Cargo.toml","targets":[],"dependencies":[]}"#;
    let text = metadata(&tooling()).replace(r#""packages":["#, &format!(r#""packages":[{outsider},"#));
    let read = packages(&text).expect("reads");
    assert_eq!(read.len(), 2);
    assert_eq!(read[1].dir, "xtask/clippy-fixture");
    assert_eq!(
        read[1].targets[1],
        (vec!["custom-build".to_owned()], "xtask/clippy-fixture/src/custom-build.rs".to_owned())
    );
    assert_eq!(boundary(&read).failures, ["the workspace has no package `lumen`"]);
}

#[test]
fn json_reads_escapes_numbers_and_words_and_refuses_what_is_not_json() {
    let text = r#" {"a": [1, -2.5e3, true, false, null], "b": "x\"\\\/\n\u00e9\ud83d\ude00"} "#;
    let expected = Json::Obj(vec![
        (
            "a".to_owned(),
            Json::Arr(vec![
                Json::Num("1".to_owned()),
                Json::Num("-2.5e3".to_owned()),
                Json::Bool(true),
                Json::Bool(false),
                Json::Null,
            ]),
        ),
        ("b".to_owned(), Json::Str("x\"\\/\n\u{e9}\u{1f600}".to_owned())),
    ]);
    assert_eq!(parse(text), Ok(expected));
    let bad = [
        r#"{"a":1} x"#,
        r#""\ud83d""#,
        r#""\ud83dx""#,
        r#""\ud83d\u0041""#,
        r#""\ud83d\ue000""#,
        r#""\udc00""#,
        r#"{"a" 1}"#,
        "[1 2]",
        r#""open"#,
        "nul",
        r#""\q""#,
    ];
    for text in bad {
        assert!(parse(text).is_err(), "{text}");
    }
    let deep = format!("{}{}", "[".repeat(65), "]".repeat(65));
    assert!(parse(&deep).is_err(), "nested 65 deep");
    let fine = format!("{}{}", "[".repeat(64), "]".repeat(64));
    assert!(parse(&fine).is_ok(), "nested 64 deep");
}
