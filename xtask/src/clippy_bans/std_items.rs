//! The std part of the ban list, derived from the pinned toolchain's documentation: the items of
//! the modules the bans cover, a decision for each, and the check that `clippy.toml` holds
//! every item decided banned.
//!
//! The source of truth is `xtask/bans/std-items.txt`, checked in: every public item of
//! `std::env`, `std::fs`, `std::process`, `std::net`, `std::thread` and `std::time` and every
//! method of `Duration` that takes or returns a float, each with its stability, as
//! [`extract`] reads them from rust-docs (`std/all.html` and each item's page). When the
//! toolchain moves, regenerate it (ADR 0006 gives the commands); [`check`] then fails on any
//! new item without a decision and on any banned item missing from `clippy.toml`.

#[cfg(test)]
mod tests;

use super::Entry;

/// The modules whose items the list holds, as directories of the std docs.
const MODULES: [&str; 6] = ["env", "fs", "process", "net", "thread", "time"];

/// One documented item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Item {
    /// `fn`, `struct`, `enum`, `trait`, `type`, `constant`, `static`, `macro` or `method`.
    pub(crate) kind: String,
    /// Its path, `std::env::var`.
    pub(crate) path: String,
    /// Whether it is stable on the pinned toolchain.
    pub(crate) stable: bool,
}

/// What the bans do with an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    /// Banned in `clippy.toml` under this rule.
    Ban(&'static str),
    /// Not in `clippy.toml`, for this reason.
    Keep(&'static str),
}

/// The items of `std::thread` the bans cover, and their rules.
const THREAD_BANS: [(&str, &str); 8] = [
    ("sleep", "thread-sleep"),
    ("sleep_ms", "thread-sleep"),
    ("park_timeout", "thread-sleep"),
    ("park_timeout_ms", "thread-sleep"),
    ("spawn", "thread-spawn"),
    ("scope", "thread-spawn"),
    ("Builder", "thread-spawn"),
    ("available_parallelism", "host-parallelism"),
];

/// The items of `std::thread` kept: scheduling and identity, no time and no host value.
const THREAD_KEPT: [&str; 12] = [
    "current",
    "panicking",
    "park",
    "yield_now",
    "AccessError",
    "JoinHandle",
    "LocalKey",
    "Scope",
    "ScopedJoinHandle",
    "Thread",
    "ThreadId",
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

/// The decision for `item`, or `None` for an item no rule decides (a new `std::thread` or
/// `std::time` item, which needs one).
pub(crate) fn decide(item: &Item) -> Option<Decision> {
    let rest = item.path.strip_prefix("std::")?;
    let (module, name) = (rest.split("::").next()?, rest.rsplit("::").next()?);
    if !item.stable {
        return Some(Decision::Keep("unstable on the pinned toolchain, so it cannot be used"));
    }
    if item.kind == "constant" || item.kind == "static" {
        return Some(Decision::Keep("a constant: Clippy cannot ban one; the token scan does"));
    }
    let thread = |n: &str| THREAD_BANS.iter().find(|(t, _)| *t == n).map(|(_, r)| Decision::Ban(r));
    match module {
        "env" => Some(Decision::Ban("std-env")),
        "fs" => Some(Decision::Ban("std-fs")),
        "process" => Some(Decision::Ban("std-process")),
        "net" if NET_KEPT.contains(&name) => Some(Decision::Keep("core's address data")),
        "net" => Some(Decision::Ban("network")),
        "thread" if THREAD_KEPT.contains(&name) => Some(Decision::Keep("scheduling, no time")),
        "thread" => thread(name),
        "time" => match (item.kind.as_str(), name) {
            ("method", _) => Some(Decision::Ban("float-type")),
            (_, "SystemTime" | "SystemTimeError") => Some(Decision::Ban("time-system")),
            (_, "Instant") => Some(Decision::Ban("time-instant")),
            (_, "Duration" | "TryFromFloatSecsError") => Some(Decision::Keep("integer durations")),
            _ => None,
        },
        _ => None,
    }
}

/// Every reason the list and `clippy.toml` disagree: an item with no decision, or an item
/// decided banned with no `clippy.toml` entry under its rule.
pub(crate) fn check(items: &[Item], entries: &[Entry]) -> Vec<String> {
    let mut failures = Vec::new();
    for item in items {
        match decide(item) {
            None => failures.push(format!(
                "{} {} has no decision: ban it or keep it in xtask/src/clippy_bans/std_items.rs",
                item.kind, item.path
            )),
            Some(Decision::Ban(rule)) => {
                if !entries.iter().any(|e| e.path == item.path && e.rule == rule) {
                    failures.push(format!(
                        "{} {} is banned ({rule}) but clippy.toml has no entry for it",
                        item.kind, item.path
                    ));
                }
            }
            Some(Decision::Keep(_)) => {}
        }
    }
    failures
}

/// The list's items: lines `<kind> <path> stable|unstable`; `#` lines are comments and the
/// `toolchain` line names the toolchain.
pub(crate) fn parse(list: &str) -> Vec<Item> {
    list.lines()
        .filter(|line| !line.starts_with('#') && !line.starts_with("toolchain "))
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let (kind, path, stability) = (words.next()?, words.next()?, words.next()?);
            Some(Item {
                kind: kind.to_owned(),
                path: path.to_owned(),
                stable: stability == "stable",
            })
        })
        .collect()
}

/// The items of the banned modules in std's `all.html`, with the stability `page` gives each
/// (`page` reads a file relative to the std docs), then the float methods of `Duration`.
pub(crate) fn extract(all_html: &str, page: &dyn Fn(&str) -> Option<String>) -> Vec<Item> {
    let mut hrefs: Vec<&str> = all_html
        .split("href=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .filter(|href| MODULES.iter().any(|m| href.starts_with(&format!("{m}/"))))
        .collect();
    hrefs.sort_unstable();
    hrefs.dedup();
    let mut items: Vec<Item> = hrefs
        .into_iter()
        .filter_map(|href| {
            let (dirs, file) = href.rsplit_once('/')?;
            let (kind, name) = file.strip_suffix(".html")?.split_once('.')?;
            let path = format!("std::{}::{name}", dirs.replace('/', "::"));
            let stable = page(href).is_some_and(|p| heading_is_stable(&p));
            Some(Item { kind: kind.to_owned(), path, stable })
        })
        .collect();
    items.extend(page("time/struct.Duration.html").map_or_else(Vec::new, |p| float_methods(&p)));
    items
}

/// Whether an item page's main heading says "Stable since".
fn heading_is_stable(page: &str) -> bool {
    let start = page.find("class=\"main-heading\"").unwrap_or(0);
    let end = page[start..].find("item-decl").map_or(page.len(), |e| start + e);
    page[start..end].contains("Stable since Rust version")
}

/// The methods on `Duration`'s page whose signature names `f32` or `f64`.
fn float_methods(page: &str) -> Vec<Item> {
    page.split("<section id=\"method.")
        .skip(1)
        .filter_map(|section| {
            let (name, rest) = section.split_once('"')?;
            let header = &rest[..rest.find("</h4>")?];
            let float =
                header.contains("primitive.f64.html") || header.contains("primitive.f32.html");
            float.then(|| Item {
                kind: "method".to_owned(),
                path: format!("std::time::Duration::{name}"),
                stable: header.contains("Stable since Rust version"),
            })
        })
        .collect()
}

/// The list file for `items`: a header naming `toolchain` and how the list was made, then a
/// line per item.
pub(crate) fn render(items: &[Item], toolchain: &str) -> String {
    let header = format!(
        "# The public items of the std modules the nondeterminism bans cover, and the float\n\
         # methods of Duration (docs/adr/0006-nondeterminism-gate.md), from rust-docs {toolchain}:\n\
         # cargo run -p xtask -- std-items <rust-docs>/share/doc/rust/html/std {toolchain}\n\
         toolchain {toolchain}\n"
    );
    let lines = items.iter().map(|item| {
        let stability = if item.stable { "stable" } else { "unstable" };
        format!("{} {} {stability}\n", item.kind, item.path)
    });
    [header].into_iter().chain(lines).collect()
}

/// A failure when the list was made for another toolchain than `rust-toolchain.toml` pins.
pub(crate) fn toolchain_matches(list: &str, toolchain_toml: &str) -> Option<String> {
    let listed = list.lines().find_map(|l| l.strip_prefix("toolchain "))?.trim().to_owned();
    let pinned = toolchain_toml
        .lines()
        .find_map(|l| l.trim().strip_prefix("channel = \""))
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_default();
    (listed != pinned).then(|| {
        format!(
            "{} is for toolchain {listed}, rust-toolchain.toml pins {pinned}: regenerate it",
            super::STD_ITEMS
        )
    })
}
