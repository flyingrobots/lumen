//! Values that depend on the host, the schedule or the address-space layout
//! (flyingrobots/lumen#155), through [`core_findings`].
//!
//! Oracle: each form is written by hand from the rule's definition in ADR 0006's amendment of
//! 2026-11-14 and std's documentation of the item, with the rule and line that must be
//! reported; the look-alikes are deterministic code a lexical rule could mistake for them.

use super::{core_findings, CrateAliases};

/// A label, the source, the rule and the line that must be reported.
type Form = (&'static str, &'static str, &'static str, usize);

fn reported(source: &str) -> Vec<(usize, &'static str)> {
    core_findings(source, &CrateAliases::default()).iter().map(|f| (f.line, f.rule)).collect()
}

const REJECTED: [Form; 41] = [
    ("a: thread::current().name() (issue)", "pub fn f() -> usize { std::thread::current().name().map_or(0, str::len) }", "thread-identity", 1),
    ("b: ptr.addr() (issue)", "pub fn f(x: &u32) -> usize { core::ptr::from_ref(x).addr() }", "address", 1),
    ("d: cfg!(target_os) (issue)", "pub fn f() -> u32 { u32::from(cfg!(target_os = \"macos\")) }", "platform-branch", 1),
    ("e: file!() (issue)", "pub fn f() -> usize { file!().len() }", "source-path", 1),
    ("i: thread_local! (issue)", "thread_local! {\n    static SEEN: core::cell::Cell<u32> = const { core::cell::Cell::new(0) };\n}", "shared-state", 1),
    ("i: a static atomic (issue)", "static CALLS: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);", "shared-state", 1),
    ("j: is_x86_feature_detected! (issue)", "#[cfg(target_arch = \"x86_64\")]\npub fn f() -> bool { std::is_x86_feature_detected!(\"avx2\") }", "host-cpu", 2),
    ("q: os::unix::process::parent_id (issue)", "#[cfg(unix)]\npub fn f() -> u32 { std::os::unix::process::parent_id() }", "std-os", 2),
    ("r: io::stdin (issue)", "pub fn f() -> usize { std::io::stdin().lines().count() }", "host-io", 1),
    ("s: os::unix::net (issue)", "#[cfg(unix)]\npub fn f() -> bool { std::os::unix::net::UnixStream::connect(\"/run/x\").is_ok() }", "std-os", 2),
    ("t: hash::DefaultHasher (issue)", "pub fn f() -> u64 { let h = std::hash::DefaultHasher::new(); h.finish() }", "std-hasher", 1),
    ("thread::current().id()", "use std::thread;\npub fn f() -> bool { format!(\"{:?}\", thread::current().id()).len() > 3 }", "thread-identity", 2),
    ("ThreadId by name", "pub fn f(t: ThreadId) -> ThreadId { t }", "thread-identity", 1),
    ("stdout().is_terminal()", "use std::io::IsTerminal;\npub fn f() -> bool { x().is_terminal() }", "host-io", 2),
    ("expose_provenance", "pub fn f(x: &u32) -> usize { core::ptr::from_ref(x).expose_provenance() }", "address", 1),
    ("a pointer cast to usize", "pub fn f(x: &u32) -> usize { x as *const u32 as usize }", "address", 1),
    ("as_ptr() cast to u64", "pub fn f(x: &[u8]) -> u64 { x.as_ptr() as u64 }", "address", 1),
    ("{:p} formats an address", "pub fn f(x: &u32) -> usize { format!(\"{x:p}\").len() }", "address", 1),
    ("{:p} in write!", "pub fn f(w: &mut String, x: &u32) -> core::fmt::Result {\n    core::fmt::Write::write_fmt(w, format_args!(\"{:p}\", x))\n}", "address", 2),
    ("{:#p} and a width", "pub fn f(x: &u32) -> String { format!(\"at {0:>18p}\", x) }", "address", 1),
    ("is_aligned and align_offset", "pub fn f(x: &[u8]) -> usize { usize::from(x.as_ptr().is_aligned()) + x.as_ptr().align_offset(8) }", "address", 1),
    ("cfg!(target_pointer_width)", "pub fn f() -> u32 { if cfg!(target_pointer_width = \"64\") { 8 } else { 4 } }", "platform-branch", 1),
    ("#[cfg(target_endian)] branch", "#[cfg(target_endian = \"big\")]\npub const ORDER: u32 = 1;", "platform-branch", 1),
    ("cfg!(debug_assertions)", "pub fn f() -> u32 { u32::from(cfg!(debug_assertions)) }", "platform-branch", 1),
    ("#![cfg(windows)] on a file", "#![cfg(windows)]\npub fn f() {}", "platform-branch", 1),
    ("cfg_attr(unix, ...) predicate", "#[cfg_attr(unix, inline)]\npub fn f() {}", "platform-branch", 1),
    ("cfg(all(test, unix)): unix is not neutral", "#[cfg(all(test, unix))]\nconst T: u32 = 1;", "platform-branch", 1),
    ("a custom cfg set by rustflags", "#[cfg(lumen_probe_fast)]\npub const K: u32 = 3;", "platform-branch", 1),
    ("panic::Location::caller()", "pub fn f() -> usize { core::panic::Location::caller().file().len() }", "source-path", 1),
    ("a static Mutex", "pub static M: std::sync::Mutex<u32> = std::sync::Mutex::new(0);", "shared-state", 1),
    ("a static behind a type alias", "type Counter = core::sync::atomic::AtomicU32;\nstatic C: Counter = Counter::new(0);", "shared-state", 2),
    ("a static mut", "static mut N: u32 = 0;", "shared-state", 1),
    ("a static in a function body", "pub fn f() -> u64 { static N: AtomicU64 = AtomicU64::new(0); N.load(Relaxed) }", "shared-state", 1),
    ("hash_map::DefaultHasher in BuildHasherDefault", "use std::collections::hash_map::DefaultHasher;\npub type B = std::hash::BuildHasherDefault<DefaultHasher>;", "std-hasher", 1),
    ("any::TypeId::of", "pub fn f() -> usize { format!(\"{:?}\", core::any::TypeId::of::<u32>()).len() }", "type-identity", 1),
    ("any::type_name through a use", "use core::any::type_name;\npub fn f() -> usize { type_name::<u32>().len() }", "type-identity", 1),
    ("backtrace::Backtrace::capture", "pub fn f() -> usize { format!(\"{}\", std::backtrace::Backtrace::capture()).len() }", "address", 1),
    ("is_aarch64_feature_detected!", "pub fn f() -> bool { std::arch::is_aarch64_feature_detected!(\"neon\") }", "host-cpu", 1),
    ("core::arch intrinsics", "#[cfg(target_arch = \"x86_64\")]\npub fn f() -> i64 { core::arch::x86_64::_rdtsc() as i64 }", "host-cpu", 2),
    ("io::stdin through a self import", "use std::io::{self, Read};\npub fn f() -> usize { io::stdin().bytes().count() }", "host-io", 2),
    ("dbg! prints the source path to stderr", "pub fn f(x: u32) -> u32 { dbg!(x) }", "host-io", 1),
];

const ACCEPTED: [(&str, &str); 14] = [
    ("modules named env, os and thread of the core's own", "mod env { pub fn var() -> u32 { 1 } }\nmod os { pub fn name() -> u32 { 2 } }\nmod thread { pub fn current() -> u32 { 3 } }\npub fn f() -> u32 { env::var() + os::name() + thread::current() }"),
    ("identifiers addr and file, a field addr", "pub struct Slot { pub addr: u32 }\npub fn f(s: &Slot) -> u32 { let addr = s.addr; let file = addr + 1; file * 2 }"),
    ("line!, column! and module_path!", "pub fn f() -> u32 { line!() + column!() + u32::try_from(module_path!().len()).unwrap_or(0) }"),
    ("the words in strings and comments", "// std::os::unix, c_double, cfg!(target_os), file!(), thread_local!, static X:\n/// is_x86_feature_detected! and DefaultHasher\npub const S: &str = \"std::io::stdin() {} {:?} {{:p}} {x:>8}\";"),
    ("'static lifetimes and a const", "pub const K: u32 = 3;\npub fn f<T: 'static>(x: &'static str, _t: T) -> &'static str { x }"),
    ("cfg(test), cfg!(test), cfg(not(test)) and a feature", "pub fn f() -> u32 { u32::from(cfg!(test)) }\n#[cfg(test)]\nconst T: u32 = 1;\n#[cfg(not(test))]\nconst U: u32 = 1;\n#[cfg(feature = \"x\")]\nconst V: u32 = 1;\n#[cfg_attr(test, allow(dead_code))]\nconst W: u32 = 1;"),
    ("std::io::Write and io::Result", "use std::io::{self, Write};\npub fn f(w: &mut dyn Write) -> io::Result<()> { w.write_all(b\"x\") }"),
    ("std::thread::panicking and the scheduler's scope types", "use std::thread::{Scope, ScopedJoinHandle};\npub fn f() -> bool { std::thread::panicking() }"),
    ("an integer cast and a deref cast", "pub fn f(x: u32, y: &u8) -> usize { x as usize + (*y as usize) + usize::from(y.count_ones() as u8) }"),
    ("a field and a method named name and id", "pub struct W { pub id: u32 }\npub fn f(w: &W) -> u32 { w.id + w.name() }"),
    ("BTreeMap and core::hash::Hash", "use core::hash::Hash;\npub fn f<T: Hash>(_t: &T) -> std::collections::BTreeMap<u8, u8> { std::collections::BTreeMap::new() }"),
    ("a 'static reference type in a where clause", "pub fn f<T>() where &'static T: Sized {}"),
    ("{:p} in a string that is not a format", "pub const S: &str = \"{:p}\";"),
    ("an atomic field, not a static", "pub struct Shared { end: core::sync::atomic::AtomicU32 }"),
];

#[test]
fn every_host_dependent_form_is_reported_under_its_rule_at_its_line() {
    let missed: Vec<String> = REJECTED
        .iter()
        .filter(|(_, source, rule, line)| !reported(source).contains(&(*line, *rule)))
        .map(|(label, source, rule, line)| {
            format!("{label}: expected [{rule}] at {line}, got {:?}", reported(source))
        })
        .collect();
    assert!(
        missed.is_empty(),
        "{} of {} missed:\n{}",
        missed.len(),
        REJECTED.len(),
        missed.join("\n")
    );
}

#[test]
fn no_host_look_alike_is_reported() {
    let rejected: Vec<String> = ACCEPTED
        .iter()
        .filter(|(_, source)| !reported(source).is_empty())
        .map(|(label, source)| format!("{label}: {:?}", reported(source)))
        .collect();
    assert!(rejected.is_empty(), "look-alikes reported:\n{}", rejected.join("\n"));
}
