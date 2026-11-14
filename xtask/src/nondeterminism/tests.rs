//! Seeded violations and look-alikes, driven through [`core_findings`] and [`waiver_forms`].
//!
//! Oracle: each form is written by hand from the rule's definition in ADR 0006 and the module
//! documentation, with the rule and line that must be reported; none is derived from the scan.
//! The same 50 violations and 10 look-alikes were run through Echo's patterns and through Clippy
//! (ADR 0006). The four Clippy alone catches (`Path::exists` and three thread spawns) are not here.

use super::tokens::is_float;
use super::{core_findings, denies_float_arithmetic, waiver_forms};

/// A seeded violation: a label, the source, the rule and the line that must be reported.
type Form = (&'static str, &'static str, &'static str, usize);

fn reported(source: &str) -> Vec<(usize, &'static str)> {
    core_findings(source).iter().map(|f| (f.line, f.rule)).collect()
}

const REJECTED: [Form; 67] = [
    ("instant: qualified call", "pub fn f() { let _ = std::time::Instant::now(); }", "time-instant", 1),
    ("instant: use then bare call", "use std::time::Instant;\npub fn f() { let _ = Instant::now(); }", "time-instant", 1),
    ("instant: use group", "use std::time::{Duration, Instant};\npub fn f() -> Duration { Instant::now().elapsed() }", "time-instant", 1),
    ("instant: renamed import", "use std::time::Instant as Tick;\npub fn f() { let _ = Tick::now(); }", "time-instant", 1),
    ("instant: module alias", "use std::time as clock;\npub fn f() { let _ = clock::Instant::now(); }", "time-instant", 2),
    ("instant: glob import", "use std::time::*;\npub fn f() { let _ = Instant::now(); }", "time-instant", 2),
    ("instant: in format! body", "pub fn f() -> String { format!(\"{:?}\", std::time::Instant::now()) }", "time-instant", 1),
    ("instant: in a local macro_rules", "macro_rules! now { () => { std::time::Instant::now() } }\npub fn f() { let _ = now!(); }", "time-instant", 1),
    ("instant: type position", "pub fn f(_at: std::time::Instant) {}", "time-instant", 1),
    ("system time: group import", "use std::time::{SystemTime, UNIX_EPOCH};\npub fn f() { let _ = SystemTime::now().duration_since(UNIX_EPOCH); }", "time-system", 1),
    ("system time: epoch elapsed", "use std::time::UNIX_EPOCH;\npub fn f() { let _ = UNIX_EPOCH.elapsed(); }", "time-system", 1),
    ("sleep: module import", "use std::thread;\npub fn f() { thread::sleep(core::time::Duration::ZERO); }", "thread-sleep", 2),
    ("sleep: renamed import", "use std::thread::sleep as nap;\npub fn f() { nap(core::time::Duration::ZERO); }", "thread-sleep", 1),
    ("cores: qualified call", "pub fn f() { let _ = std::thread::available_parallelism(); }", "host-parallelism", 1),
    ("cores: renamed import", "use std::thread::available_parallelism as cores;\npub fn f() { let _ = cores(); }", "host-parallelism", 1),
    ("map: renamed import", "use std::collections::HashMap as Map;\npub fn f() { let _ = Map::<u8, u8>::new(); }", "unordered-container", 1),
    ("set: use group", "use std::collections::{BTreeMap, HashSet};\npub fn f() { let _ = (BTreeMap::<u8, u8>::new(), HashSet::<u8>::new()); }", "unordered-container", 1),
    ("map: qualified", "pub fn f() { let _ = std::collections::HashMap::<u8, u8>::new(); }", "unordered-container", 1),
    ("set: in vec! body", "pub fn f() { let _ = vec![std::collections::HashSet::<u8>::new()]; }", "unordered-container", 1),
    ("map: type alias", "pub type Table = std::collections::HashMap<u32, u32>;", "unordered-container", 1),
    ("map: glob import", "use std::collections::*;\npub fn f() { let _ = HashMap::<u8, u8>::new(); }", "unordered-container", 2),
    ("random state: qualified", "pub fn f() { let _ = std::hash::RandomState::new(); }", "unordered-container", 1),
    ("env: module import", "use std::env;\npub fn f() { let _ = env::var(\"X\"); }", "std-env", 1),
    ("env: std group", "use std::{env, fs};\npub fn f() { let _ = (env::var_os(\"X\"), fs::read(\"x\")); }", "std-env", 1),
    ("env: in format! body", "pub fn f() -> String { format!(\"{:?}\", std::env::args().count()) }", "std-env", 1),
    ("env: module rename", "use std::env as host;\npub fn f() { let _ = host::var(\"X\"); }", "std-env", 2),
    ("env: option_env! macro", "pub fn f() -> Option<&'static str> { option_env!(\"USER\") }", "std-env", 1),
    ("fs: use type then call", "use std::fs::File;\npub fn f() { let _ = File::open(\"x\"); }", "std-fs", 1),
    ("fs: qualified read", "pub fn f() { let _ = std::fs::read(\"x\"); }", "std-fs", 1),
    ("fs: glob then bare call", "use std::fs::*;\npub fn f() { let _ = read_to_string(\"x\"); }", "std-fs", 1),
    ("process: command", "use std::process::Command;\npub fn f() { let _ = Command::new(\"x\"); }", "std-process", 1),
    ("process: module rename exit", "use std::process as p;\npub fn f() { p::exit(1) }", "std-process", 2),
    ("net: tcp connect", "use std::net::TcpStream;\npub fn f() { let _ = TcpStream::connect(\"x:1\"); }", "network", 1),
    ("net: self in group", "use std::net::{self};\npub fn f() { let _ = net::UdpSocket::bind(\"x:1\"); }", "network", 1),
    ("float: const with type and literal", "pub const HALF: f64 = 0.5;", "float-type", 1),
    ("float: literal cast in const", "pub const K: u32 = 1.5 as u32;", "float-literal", 1),
    ("float: behind a type alias", "pub type Real = f32;\npub fn f(x: Real) -> Real { x }", "float-type", 1),
    ("float: cast to f64", "pub fn f(x: u32) -> u64 { (x as f64) as u64 }", "float-cast", 1),
    ("float: literal in format! body", "pub fn f() -> String { format!(\"{}\", 1.5) }", "float-literal", 1),
    ("float: sentinel", "pub fn f() -> bool { f64::NAN.is_nan() }", "float-type", 1),
    ("float: suffix literal", "pub fn f() -> u32 { 1f32.to_bits() }", "float-literal", 1),
    ("float: exponent literal", "pub fn f() -> u64 { 1e3 as u64 }", "float-literal", 1),
    ("float: in cfg-gated code", "#[cfg(target_arch = \"wasm32\")]\npub fn f() -> u32 { (2.0_f64).sqrt() as u32 }", "float-literal", 2),
    ("instant: in cfg-gated code", "#[cfg(feature = \"never\")]\npub fn f() { let _ = std::time::Instant::now(); }", "time-instant", 2),
    ("map: in an unused macro_rules", "#[allow(unused_macros)]\nmacro_rules! table { () => { std::collections::HashMap::<u8, u8>::new() } }", "unordered-container", 2),
    ("open addressing: comment", "// open addressing with linear probing\npub struct Table([u32; 8]);", "hand-hashed-table", 1),
    ("env: env! macro", "pub fn f() -> &'static str { env!(\"HOME\") }", "std-env", 1),
    ("float: negative exponent literal", "pub const K: u32 = 1e-3 as u32;", "float-literal", 1),
    ("float: range bound after two dots", "pub fn f() -> core::ops::RangeTo<u32> { ..1.5 as u32 }", "float-literal", 1),
    ("float: underscore after the exponent sign", "pub const K: u32 = 1e+_3 as u32;", "float-literal", 1),
    ("float: behind a shebang that opens a comment", "#!/bin/x /*\npub const K: u32 = 1.5 as u32;", "float-literal", 2),
    ("env: raw path segment", "pub fn f() { let _ = std::r#env::var(\"X\"); }", "std-env", 1),
    ("float: raw type", "pub fn f(x: u32) -> r#f64 { r#f64::from(x) }", "float-type", 1),
    ("env: extern crate alias", "extern crate std as host;\npub fn f() { let _ = host::env::var(\"X\"); }", "std-env", 2),
    ("fs: unlisted exists through an alias", "extern crate std as host;\npub fn f() -> bool { host::fs::exists(\"x\").is_ok() }", "std-fs", 2),
    ("env: unlisted set_current_dir through an alias", "extern crate std as host;\npub fn f() { let _ = host::env::set_current_dir(\"/\"); }", "std-env", 2),
    ("env: a constant through an alias", "extern crate std as host;\npub fn f() -> &'static str { host::env::consts::OS }", "std-env", 2),
    ("env: an alias of an alias", "extern crate std as host;\nuse host::env as e;\npub fn f() { let _ = e::var(\"X\"); }", "std-env", 3),
    ("fs: unlisted copy behind a raw segment", "pub fn f() { let _ = std::r#fs::copy(\"a\", \"b\"); }", "std-fs", 1),
    ("sleep: deprecated sleep_ms", "#[allow(deprecated)]\npub fn f() { std::thread::sleep_ms(1); }", "thread-sleep", 2),
    ("float: Duration methods without a float token", "pub fn f(d: core::time::Duration) -> core::time::Duration { d.mul_f64(d.as_secs_f64()) }", "float-type", 1),
    ("float: from_secs_f64 of a parsed string", "pub fn f(t: &str) -> Option<core::time::Duration> { Some(core::time::Duration::from_secs_f64(t.parse().ok()?)) }", "float-type", 1),
    ("env: an alias shadowed by a sibling module's", "mod a {\n    extern crate core as host;\n    pub type D = host::time::Duration;\n}\nmod b {\n    extern crate std as host;\n    pub const OS: &str = host::env::consts::OS;\n}", "std-env", 7),
    ("float: Bunny's to_f32", "pub fn f(x: bunny_num::FixedQ32_32) -> u32 { x.to_f32().to_bits() }", "float-type", 1),
    ("float: Bunny's into_float", "pub fn f(r: bunny_geom::FixedRay3) -> bool { r.into_float() == r.into_float() }", "float-type", 1),
    ("float: a Bunny float vector by import", "use bunny_linalg::Vec3;\npub fn f(v: Vec3) -> Vec3 { v }", "float-type", 1),
    ("float: a codec vertex's field", "pub fn f(v: bunny_codec::PlyVertex) -> bool { v.x == v.y }", "float-type", 1),
];

const ACCEPTED: [(&str, &str); 18] = [
    ("sin and sqrt methods on a fixed-point type", "pub struct Fx(pub i64);\nimpl Fx { pub fn sin(self) -> Self { self } pub fn sqrt(self) -> Self { self } }\npub fn f() -> Fx { Fx(1).sin().sqrt() }"),
    ("HashMap in a comment and a doc comment", "// a HashMap would iterate in a random order\n/// not a std::collections::HashMap\npub fn f() {}"),
    ("HashMap and f64 in strings", "pub fn f() -> (&'static str, &'static str) { (\"std::collections::HashMap\", r#\"f64::NAN 0.5\"#) }"),
    ("identifiers instant and system_time", "pub fn f(instant: u32, system_time: u32) -> u32 { instant + system_time }"),
    ("nested tuple index", "pub fn f(p: ((u8, u8), u8)) -> u8 { p.0.1 }"),
    ("range and method on an integer literal", "pub fn f() -> u32 { (0..10).map(|i: u32| 1.max(i)).sum() }"),
    ("hex literal with f64 digits", "pub const H: u32 = 0xf64 + 0x1f32;"),
    ("core Duration and BTreeMap", "use std::collections::BTreeMap;\npub fn f() -> (core::time::Duration, BTreeMap<u8, u8>) { (core::time::Duration::ZERO, BTreeMap::new()) }"),
    ("an f32 grid in a doc comment", "/// exactly an f32, a value like 0.5\npub fn f() {}"),
    ("char literals", "pub fn f() -> [char; 2] { ['.', '1'] }"),
    ("a raw identifier r#type", "pub fn f(r#type: u32) -> u32 { r#type }"),
    ("an integer range and a suffixed sum", "pub fn f() -> usize { (1..2).len() + 1usize+2 }"),
    ("hex with an e digit before a sign", "pub const H: u32 = 0x1e+3;"),
    ("a module named env of the crate's own", "mod env { pub fn var() -> u32 { 1 } }\npub fn f() -> u32 { env::var() }"),
    ("core renamed by extern crate", "extern crate core as c;\npub fn f() -> c::time::Duration { c::time::Duration::ZERO }"),
    ("f64 inside a longer word", "pub fn buff64() -> u32 { 0 }\npub fn of32x() -> u32 { buff64() }"),
    ("the float_arithmetic lint denied", "#![deny(clippy::float_arithmetic)]\npub fn f() {}"),
    ("Bunny's fixed-point vector and an error named Float", "pub fn f(v: bunny_linalg::FixedVec3) -> Result<bunny_linalg::FixedVec3, bunny_num::FloatConversionError> { Ok(v) }"),
];

#[test]
fn every_seeded_violation_is_reported_under_its_rule_at_its_line() {
    let missed: Vec<String> = REJECTED
        .iter()
        .filter(|(_, source, rule, line)| !reported(source).contains(&(*line, *rule)))
        .map(|(label, source, rule, line)| {
            format!("{label}: expected [{rule}] at line {line}, got {:?}", reported(source))
        })
        .collect();
    assert!(
        missed.is_empty(),
        "{} of {} slipped through:\n{}",
        missed.len(),
        REJECTED.len(),
        missed.join("\n")
    );
}

#[test]
fn no_look_alike_is_reported() {
    let rejected: Vec<String> = ACCEPTED
        .iter()
        .filter(|(_, source)| !reported(source).is_empty())
        .map(|(label, source)| format!("{label}: {:?}", reported(source)))
        .collect();
    assert!(rejected.is_empty(), "look-alikes reported:\n{}", rejected.join("\n"));
}

#[test]
fn float_literals_are_decimal_with_a_fraction_an_exponent_or_a_float_suffix() {
    for float in ["0.5", "1.", "1e3", "1E-3", "2.5e+7", "1f32", "2_f64", "1_000.0"] {
        assert!(is_float(float), "{float}");
    }
    for integer in ["0", "1usize", "2isize", "0xf64", "0x1E", "0b1", "0o7", "1_000u64"] {
        assert!(!is_float(integer), "{integer}");
    }
}

#[test]
fn a_waiver_on_the_line_or_the_line_above_removes_its_rules_finding() {
    let above = "// nondeterminism: allow float-literal -- a constant the PNG format defines\nconst A: u32 = 1e3 as u32;";
    assert_eq!(reported(above), []);
    let same = "const A: u32 = 1e3 as u32; // nondeterminism: allow float-literal -- the format's constant";
    assert_eq!(reported(same), []);
}

#[test]
fn a_waiver_for_another_rule_or_two_lines_up_leaves_the_finding() {
    let other = "// nondeterminism: allow float-type -- not this rule\nconst A: u32 = 1e3 as u32;";
    assert_eq!(reported(other), [(2, "float-literal"), (1, "waiver")]);
    let far = "// nondeterminism: allow float-literal -- too far up\n\nconst A: u32 = 1e3 as u32;";
    assert_eq!(reported(far), [(3, "float-literal"), (1, "waiver")]);
}

#[test]
fn a_waiver_without_a_reason_or_for_an_unknown_rule_fails() {
    let bare = "// nondeterminism: allow float-literal\nconst A: u32 = 1e3 as u32;";
    assert_eq!(reported(bare), [(2, "float-literal"), (1, "waiver")]);
    let empty = "// nondeterminism: allow float-literal --  \nconst A: u32 = 1e3 as u32;";
    assert_eq!(reported(empty), [(2, "float-literal"), (1, "waiver")]);
    let unknown = "// nondeterminism: allow floats -- a reason\nconst A: u32 = 1e3 as u32;";
    assert_eq!(reported(unknown), [(2, "float-literal"), (1, "waiver")]);
}

#[test]
fn a_waiver_that_waives_nothing_fails() {
    assert_eq!(
        reported("// nondeterminism: allow float-literal -- nothing here\nconst A: u32 = 1;"),
        [(1, "waiver")]
    );
}

#[test]
fn a_ban_lint_waiver_must_be_an_expect_whose_reason_starts_with_rule_ids() {
    let forms = |text: &str| waiver_forms(text).iter().map(|f| f.line).collect::<Vec<_>>();
    assert_eq!(
        forms("#[expect(clippy::disallowed_types, reason = \"unordered-container: counts only\")]"),
        []
    );
    assert_eq!(
        forms("#![expect(clippy::disallowed_methods, reason = \"std-env, std-fs: tooling\")]"),
        []
    );
    assert_eq!(
        forms("#[allow(clippy::disallowed_types, reason = \"unordered-container: counts only\")]"),
        [1]
    );
    assert_eq!(forms("\n#[expect(clippy::disallowed_methods)]"), [2]);
    assert_eq!(forms("#[expect(clippy::float_arithmetic, reason = \"no rule named\")]"), [1]);
    assert_eq!(
        forms("#[expect(clippy::disallowed_macros, reason = \"std-env, floats: one unknown\")]"),
        [1]
    );
    assert_eq!(forms("#[allow(dead_code)]\n#[expect(clippy::cast_possible_truncation)]"), []);
}

#[test]
fn a_crate_root_denies_float_arithmetic_only_in_an_inner_deny_or_forbid() {
    for root in [
        "//! Docs.\n#![deny(clippy::float_arithmetic)]\nmod a;",
        "#![forbid(clippy::float_arithmetic)]",
        "#![deny(missing_docs, clippy::float_arithmetic)]",
    ] {
        assert!(denies_float_arithmetic(root), "{root}");
    }
    for root in [
        "#[deny(clippy::float_arithmetic)]\nfn f() {}",
        "// #![deny(clippy::float_arithmetic)]",
        "#![warn(clippy::float_arithmetic)]",
        "m![deny(clippy::float_arithmetic)];",
        "#![deny(clippy::float_cmp)]",
        "#![deny(missing_docs)] const S: &str = \"clippy::float_arithmetic\";",
        "#![deny(missing_docs)]\n#![allow(clippy::float_arithmetic)]",
        "",
    ] {
        assert!(!denies_float_arithmetic(root), "{root}");
    }
}
