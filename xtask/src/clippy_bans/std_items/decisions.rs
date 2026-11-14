//! This module owns the decision for each std item the list holds: banned under a rule, or kept
//! with a reason.

use super::Item;

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
