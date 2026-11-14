//! This module owns the decision for each std item the list holds: banned under a rule, or kept
//! with a reason.

use super::Item;

/// What the bans do with an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    /// Banned in `clippy.toml` under this rule.
    Ban(&'static str),
    /// Banned by the token scan alone, under this rule: a whole module of platform items the
    /// fixture cannot use on every host (`std::os`, `std::arch`), or a macro Clippy does not
    /// catch (`file!`, `cfg!`).
    Scan(&'static str),
    /// Not banned, for this reason.
    Keep(&'static str),
}

use Decision::{Ban, Keep, Scan};

/// The items of `std::thread` the bans cover, and their rules.
const THREAD_BANS: [(&str, &str); 12] = [
    ("sleep", "thread-sleep"),
    ("sleep_ms", "thread-sleep"),
    ("park_timeout", "thread-sleep"),
    ("park_timeout_ms", "thread-sleep"),
    ("spawn", "thread-spawn"),
    ("scope", "thread-spawn"),
    ("Builder", "thread-spawn"),
    ("available_parallelism", "host-parallelism"),
    ("current", "thread-identity"),
    ("Thread", "thread-identity"),
    ("ThreadId", "thread-identity"),
    ("LocalKey", "shared-state"),
];

/// The items of `std::thread` kept: scheduling, with no time, identity or host value.
const THREAD_KEPT: [&str; 8] = [
    "panicking",
    "park",
    "yield_now",
    "AccessError",
    "JoinHandle",
    "Scope",
    "ScopedJoinHandle",
    "Result",
];

/// The items of `std::net` kept: core's address data, with no socket and no lookup.
const NET_KEPT: [&str; 8] = [
    "AddrParseError",
    "IpAddr",
    "Ipv4Addr",
    "Ipv6Addr",
    "Shutdown",
    "SocketAddr",
    "SocketAddrV4",
    "SocketAddrV6",
];

/// The items of `std::io` banned (`host-io`): the process's own streams and pipes, and whether
/// a stream is a terminal. The rest reads or writes what the caller passes.
const IO_BANS: [&str; 13] = [
    "stdin",
    "Stdin",
    "StdinLock",
    "stdout",
    "Stdout",
    "StdoutLock",
    "stderr",
    "Stderr",
    "StderrLock",
    "IsTerminal",
    "pipe",
    "PipeReader",
    "PipeWriter",
];

/// The std modules whose items the list holds and decides one by one ([`super::MODULES`]).
pub(super) const ENUMERATED: [&str; 10] =
    ["env", "fs", "process", "net", "thread", "time", "io", "hash", "any", "panic"];

/// The std modules banned whole by the scan, and their rules.
const MODULE_SCAN: [(&str, &str); 3] =
    [("os", "std-os"), ("arch", "host-cpu"), ("backtrace", "address")];

/// The std modules kept whole, each with why nothing in it needs a ban beyond the named ones.
const MODULE_KEPT: [(&[&str], &str); 9] = [
    (&["i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize"], "integer constants and their modules"),
    (&["f32", "f64", "primitive"], "the float modules and primitives: the scan's float-type names them"),
    (&["array", "slice", "str", "string", "vec", "boxed", "rc", "borrow", "char", "ascii"], "values and their containers"),
    (&["clone", "cmp", "convert", "default", "marker", "ops", "option", "result", "iter", "range", "pin", "prelude"], "the language's traits and types"),
    (&["fmt", "ptr", "mem", "cell", "sync", "alloc"], "data and memory; addresses ({:p}, addr), statics and thread state are refused by rule, not by module"),
    (&["collections"], "BTreeMap and the rest; HashMap, HashSet and hash_map are refused by name"),
    (&["error", "num", "hint", "future", "task"], "errors, number types, hints and futures: no host value"),
    (&["ffi"], "C types and strings; c_double and c_float are float-type"),
    (&["path"], "path text; the Path methods that read the file system are std-fs"),
];

/// The macros at the root of std, and their decisions.
const ROOT_MACROS: [(&str, Decision); 9] = [
    ("env", Ban("std-env")),
    ("option_env", Ban("std-env")),
    ("thread_local", Ban("shared-state")),
    ("cfg", Scan("platform-branch")),
    ("cfg_select", Scan("platform-branch")),
    ("file", Scan("source-path")),
    ("is_x86_feature_detected", Scan("host-cpu")),
    ("include", Keep("include! fails the gates' source layout")),
    ("dbg", Scan("host-io")),
];

/// The root macros that print to the process's streams.
const PRINTING: [&str; 4] = ["print", "println", "eprint", "eprintln"];

/// The root macros kept: assertions, formatting into a value, and values of the source text
/// alone (`line!`, `column!`, `module_path!`).
const MACROS_KEPT: [&str; 27] = [
    "assert",
    "assert_eq",
    "assert_matches",
    "assert_ne",
    "column",
    "compile_error",
    "concat",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_matches",
    "debug_assert_ne",
    "format",
    "format_args",
    "include_bytes",
    "include_str",
    "line",
    "matches",
    "module_path",
    "panic",
    "stringify",
    "todo",
    "try",
    "unimplemented",
    "unreachable",
    "vec",
    "write",
    "writeln",
];

/// The decision for `item`, or `None` for an item no rule decides: a new module, a new root
/// macro, or a new item of a module decided item by item where the module has no default.
pub(crate) fn decide(item: &Item) -> Option<Decision> {
    let rest = item.path.strip_prefix("std::")?;
    let (module, name) = (rest.split("::").next()?, rest.rsplit("::").next()?);
    if !item.stable {
        return Some(Keep("unstable on the pinned toolchain, so it cannot be used"));
    }
    if item.kind == "module" {
        return decide_module(module);
    }
    if item.kind == "macro" && !rest.contains("::") {
        return decide_macro(name);
    }
    if item.kind == "constant" || item.kind == "static" {
        return Some(Keep("a constant: Clippy cannot ban one; the token scan does"));
    }
    let thread = |n: &str| THREAD_BANS.iter().find(|(t, _)| *t == n).map(|(_, r)| Ban(r));
    match module {
        "env" => Some(Ban("std-env")),
        "fs" => Some(Ban("std-fs")),
        "process" => Some(Ban("std-process")),
        "net" if NET_KEPT.contains(&name) => Some(Keep("core's address data")),
        "net" => Some(Ban("network")),
        "thread" if THREAD_KEPT.contains(&name) => Some(Keep("scheduling, no time or identity")),
        "thread" => thread(name),
        "time" => match (item.kind.as_str(), name) {
            ("method", _) => Some(Ban("float-type")),
            (_, "SystemTime" | "SystemTimeError") => Some(Ban("time-system")),
            (_, "Instant") => Some(Ban("time-instant")),
            (_, "Duration" | "TryFromFloatSecsError") => Some(Keep("integer durations")),
            _ => None,
        },
        "io" if IO_BANS.contains(&name) => Some(Ban("host-io")),
        "io" => Some(Keep("I/O on a reader or writer the caller passes")),
        "hash" => match name {
            "DefaultHasher" | "SipHasher" => Some(Ban("std-hasher")),
            "RandomState" => Some(Ban("unordered-container")),
            _ => Some(Keep("the hashing traits, with no seed of their own")),
        },
        "any" => match name {
            "TypeId" | "type_name" | "type_name_of_val" => Some(Ban("type-identity")),
            _ => Some(Keep("dynamic typing, with no value of its own")),
        },
        "panic" => match name {
            "Location" => Some(Ban("source-path")),
            "set_hook" | "take_hook" => Some(Ban("shared-state")),
            _ => Some(Keep("unwinding, with no host value")),
        },
        _ => None,
    }
}

/// The decision for the std module `name`: enumerated, banned whole by the scan, or kept.
fn decide_module(name: &str) -> Option<Decision> {
    if ENUMERATED.contains(&name) {
        return Some(Keep("its items are listed and decided one by one"));
    }
    if let Some((_, rule)) = MODULE_SCAN.iter().find(|(m, _)| *m == name) {
        return Some(Scan(rule));
    }
    MODULE_KEPT.iter().find(|(names, _)| names.contains(&name)).map(|(_, why)| Keep(why))
}

/// The decision for the root macro `name`.
fn decide_macro(name: &str) -> Option<Decision> {
    if let Some((_, decision)) = ROOT_MACROS.iter().find(|(m, _)| *m == name) {
        return Some(*decision);
    }
    if PRINTING.contains(&name) {
        return Some(Scan("host-io"));
    }
    MACROS_KEPT.contains(&name).then_some(Keep("assertions, formatting and the source text"))
}
