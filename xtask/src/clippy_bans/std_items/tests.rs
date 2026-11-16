//! The std item list: reading rustdoc's pages, the decisions, and the check against
//! `clippy.toml`.
//!
//! Oracle: fragments of the real pages of rust-docs 1.96.0 (`std/all.html`, the main heading of
//! a stable and of an unstable item, `Duration`'s `mul_f64` and `as_millis_f64` sections), and
//! the decisions as ADR 0006 states them, written out by hand.

use super::super::Entry;
use super::{
    check, check_with, decide, extract, parse_list, render, toolchain_matches, Decision, Item,
};

const INDEX: &str = r#"<a class="mod" href="env/index.html">env</a><a class="mod" href="os/index.html">os</a><a href="../core/index.html">core</a><a href="all.html">All</a>"#;

const ALL: &str = r#"<li><a href="macro.file.html">file</a></li><li><a href="fs/fn.copy.html">fs::copy</a></li><li><a href="fs/struct.Dir.html">fs::Dir</a></li><li><a href="env/consts/constant.OS.html">env::consts::OS</a></li><li><a href="io/fn.stdin.html">io::stdin</a></li><li><a href="fs/fn.copy.html">fs::copy</a></li>"#;

const STABLE: &str = r#"<div class="main-heading"><h1>Function <span class="fn">copy</span></h1><rustdoc-toolbar></rustdoc-toolbar><span class="sub-heading"><span class="since" title="Stable since Rust version 1.0.0">1.0.0</span></span></div><pre class="rust item-decl"><code>pub fn copy"#;

const UNSTABLE: &str = r#"<div class="main-heading"><h1>Struct <span class="struct">Dir</span></h1></div><pre class="rust item-decl"><code>pub struct Dir { /* private fields */ }</code></pre><span class="item-info"><div class="stab unstable">"#;

const DURATION: &str = r##"<section id="method.mul_f64" class="method"><span class="rightside"><span class="since" title="Stable since Rust version 1.38.0">1.38.0</span></span><h4 class="code-header">pub fn <a href="#method.mul_f64" class="fn">mul_f64</a>(self, rhs: <a class="primitive" href="../primitive.f64.html">f64</a>) -&gt; Duration</h4></section><section id="method.as_millis_f64" class="method"><h4 class="code-header">pub const fn <a href="#method.as_millis_f64" class="fn">as_millis_f64</a>(&amp;self) -&gt; <a class="primitive" href="../primitive.f64.html">f64</a></h4></section><section id="method.as_secs" class="method"><span class="since" title="Stable since Rust version 1.3.0">1.3.0</span><h4 class="code-header">pub const fn <a href="#method.as_secs" class="fn">as_secs</a>(&amp;self) -&gt; u64</h4></section>"##;

fn item(kind: &str, path: &str, stable: bool) -> Item {
    Item { kind: kind.to_owned(), path: path.to_owned(), stable }
}

fn entry(path: &str, rule: &str) -> Entry {
    Entry { line: 1, path: path.to_owned(), rule: rule.to_owned() }
}

#[test]
fn items_are_read_from_all_html_with_the_stability_of_their_pages() {
    let page = |href: &str| match href {
        "index.html" => Some(INDEX.to_owned()),
        "fs/fn.copy.html"
        | "env/consts/constant.OS.html"
        | "io/fn.stdin.html"
        | "env/index.html"
        | "macro.file.html" => Some(STABLE.to_owned()),
        "os/index.html" | "fs/struct.Dir.html" => Some(UNSTABLE.to_owned()),
        "time/struct.Duration.html" => Some(DURATION.to_owned()),
        _ => None,
    };
    assert_eq!(
        extract(ALL, &page).expect("every page is there"),
        [
            item("module", "std::env", true),
            item("module", "std::os", false),
            item("macro", "std::file", true),
            item("constant", "std::env::consts::OS", true),
            item("fn", "std::fs::copy", true),
            item("struct", "std::fs::Dir", false),
            item("fn", "std::io::stdin", true),
            item("method", "std::time::Duration::mul_f64", true),
            item("method", "std::time::Duration::as_millis_f64", false),
        ]
    );
}

#[test]
fn every_module_rule_decides_as_adr_0006_says() {
    let cases = [
        (item("fn", "std::env::set_var", true), Decision::Ban("std-env")),
        (item("struct", "std::fs::DirEntry", true), Decision::Ban("std-fs")),
        (item("trait", "std::process::Termination", true), Decision::Ban("std-process")),
        (item("trait", "std::net::ToSocketAddrs", true), Decision::Ban("network")),
        (item("fn", "std::thread::sleep_ms", true), Decision::Ban("thread-sleep")),
        (item("struct", "std::thread::Builder", true), Decision::Ban("thread-spawn")),
        (item("fn", "std::thread::available_parallelism", true), Decision::Ban("host-parallelism")),
        (item("struct", "std::time::SystemTimeError", true), Decision::Ban("time-system")),
        (item("struct", "std::time::Instant", true), Decision::Ban("time-instant")),
        (item("method", "std::time::Duration::mul_f64", true), Decision::Ban("float-type")),
        (item("fn", "std::thread::current", true), Decision::Ban("thread-identity")),
        (item("struct", "std::thread::LocalKey", true), Decision::Ban("shared-state")),
        (item("fn", "std::io::stdin", true), Decision::Ban("host-io")),
        (item("trait", "std::io::IsTerminal", true), Decision::Ban("host-io")),
        (item("struct", "std::hash::DefaultHasher", true), Decision::Ban("std-hasher")),
        (item("struct", "std::any::TypeId", true), Decision::Ban("type-identity")),
        (item("struct", "std::panic::Location", true), Decision::Ban("source-path")),
        (item("fn", "std::panic::set_hook", true), Decision::Ban("shared-state")),
        (item("module", "std::os", true), Decision::Scan("std-os")),
        (item("module", "std::arch", true), Decision::Scan("host-cpu")),
        (item("macro", "std::file", true), Decision::Scan("source-path")),
        (item("macro", "std::cfg_select", true), Decision::Scan("platform-branch")),
        (item("macro", "std::println", true), Decision::Scan("host-io")),
        (item("macro", "std::thread_local", true), Decision::Ban("shared-state")),
    ];
    for (item, decision) in cases {
        assert_eq!(decide(&item), Some(decision), "{item:?}");
    }
    for kept in [
        item("struct", "std::net::Ipv4Addr", true),
        item("fn", "std::thread::yield_now", true),
        item("struct", "std::time::Duration", true),
        item("constant", "std::env::consts::OS", true),
        item("struct", "std::fs::Dir", false),
        item("trait", "std::io::Write", true),
        item("trait", "std::hash::Hasher", true),
        item("fn", "std::panic::catch_unwind", true),
        item("module", "std::vec", true),
        item("module", "std::io", true),
        item("module", "std::random", false),
        item("macro", "std::line", true),
        item("macro", "std::module_path", true),
    ] {
        assert!(matches!(decide(&kept), Some(Decision::Keep(_))), "{kept:?}");
    }
    assert_eq!(decide(&item("module", "std::quantum", true)), None, "a new module has no decision");
    assert_eq!(decide(&item("macro", "std::new_macro", true)), None, "a new root macro has none");
    assert_eq!(decide(&item("fn", "std::thread::sleep_for_a_while", true)), None);
    assert_eq!(decide(&item("struct", "std::time::Stopwatch", true)), None);
}

#[test]
fn the_check_fails_a_missing_entry_an_entry_under_another_rule_and_an_undecided_item() {
    let items = [
        item("fn", "std::fs::copy", true),
        item("fn", "std::env::var", true),
        item("fn", "std::thread::sleep_for_a_while", true),
        item("struct", "std::net::Ipv4Addr", true),
    ];
    let failures = check(&items, &[entry("std::env::var", "std-fs")]);
    assert_eq!(failures.len(), 3, "{failures:?}");
    assert!(
        failures[0].starts_with("fn std::fs::copy is banned (std-fs) but clippy.toml has no entry")
    );
    assert!(failures[1]
        .starts_with("fn std::env::var is banned (std-env) but clippy.toml has no entry"));
    assert!(failures[2].starts_with("fn std::thread::sleep_for_a_while has no decision"));
    let entries = [entry("std::fs::copy", "std-fs"), entry("std::env::var", "std-env")];
    assert_eq!(check(&items[..2], &entries), Vec::<String>::new());
}

#[test]
fn the_list_reads_back_what_it_renders_and_names_its_toolchain() {
    let items = [item("fn", "std::fs::copy", true), item("struct", "std::fs::Dir", false)];
    let list = render(&items, "1.96.0");
    assert_eq!(parse_list(&list), (items.to_vec(), Vec::new()));
    assert_eq!(toolchain_matches(&list, "[toolchain]\nchannel = \"1.96.0\"\n"), None);
    let moved = toolchain_matches(&list, "[toolchain]\nchannel = \"1.97.0\"\n");
    assert!(moved
        .is_some_and(|m| m.contains("is for toolchain 1.96.0, rust-toolchain.toml pins 1.97.0")));
}

#[test]
fn a_malformed_row_a_missing_page_and_a_missing_toolchain_line_fail() {
    let (_, bad) = parse_list("toolchain 1.96.0\nfn std::fs::copy stabel\nfn std::fs::read\nfn std::fs::write stable extra\n");
    assert_eq!(bad.len(), 3, "{bad:?}");
    assert!(bad[0].contains("line 2"), "{bad:?}");
    let missing = |href: &str| (href != "fs/fn.copy.html").then(|| STABLE.to_owned());
    let refused = extract(ALL, &missing);
    assert!(refused.is_err_and(|e| e.contains("fs/fn.copy.html")));
    let toolchain = "[toolchain]\nchannel = \"1.96.0\"\n";
    assert!(toolchain_matches("fn std::fs::copy stable\n", toolchain)
        .is_some_and(|m| m.contains("no toolchain line")));
    assert!(toolchain_matches("", toolchain).is_some());
}

#[test]
fn an_item_left_to_the_scan_must_be_one_the_scan_refuses() {
    let items = [
        item("module", "std::os", true),
        item("module", "std::backtrace", true),
        item("macro", "std::file", true),
        item("macro", "std::cfg_select", true),
        item("macro", "std::is_x86_feature_detected", true),
        item("macro", "std::eprintln", true),
    ];
    assert_eq!(check(&items, &[]), Vec::<String>::new());
    let failures = check_with(&items[..1], &[], &|_, _| None);
    assert_eq!(failures.len(), 1, "a scan that refuses nothing: {failures:?}");
    assert!(failures[0].starts_with(
        "module std::os is banned by the scan (std-os) but the scan does not refuse it"
    ));
}
