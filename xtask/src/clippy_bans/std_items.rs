//! The std part of the ban list, derived from the pinned toolchain's documentation: the items of
//! the modules the bans cover, a decision for each, and the check that `clippy.toml` holds
//! every item decided banned.
//!
//! The source of truth is `xtask/bans/std-items.txt`, checked in: every top-level module of std
//! and every macro at its root; every public item of the modules decided item by item
//! (`std::env`, `fs`, `process`, `net`, `thread`, `time`, `io`, `hash`, `any` and `panic`); and
//! every method of `Duration` that takes or returns a float; each with its stability, as
//! [`extract`] reads them from rust-docs (`std/index.html`, `std/all.html` and each page). When
//! the toolchain moves, regenerate it (ADR 0006 gives the commands); [`check`] then fails on any
//! new module, root macro or item without a decision, on any item decided banned that
//! `clippy.toml` lacks, and on any decided banned by the scan that the scan does not refuse.

mod decisions;
#[cfg(test)]
mod tests;

use super::Entry;
pub(crate) use decisions::{decide, Decision};

/// The modules whose items the list holds, as directories of the std docs.
const MODULES: [&str; 10] = decisions::ENUMERATED;

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

/// Every reason the list and `clippy.toml` disagree: an item with no decision, or an item
/// decided banned with no `clippy.toml` entry under its rule.
pub(crate) fn check(items: &[Item], entries: &[Entry]) -> Vec<String> {
    check_with(items, entries, &crate::nondeterminism::scan_rule)
}

/// [`check`], with `scan` giving the rule under which the scan refuses a path (and whether it
/// is a macro's), if it does.
pub(crate) fn check_with(
    items: &[Item],
    entries: &[Entry],
    scan: &dyn Fn(&str, bool) -> Option<&'static str>,
) -> Vec<String> {
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
            Some(Decision::Scan(rule)) => {
                let is_macro = item.kind == "macro";
                if scan(&item.path, is_macro) != Some(rule) {
                    failures.push(format!(
                        "{} {} is banned by the scan ({rule}) but the scan does not refuse it",
                        item.kind, item.path
                    ));
                }
            }
            Some(Decision::Keep(_)) => {}
        }
    }
    failures
}

/// The kinds of item a list row may name.
const KINDS: [&str; 11] = [
    "fn", "struct", "enum", "trait", "type", "constant", "static", "macro", "method", "module",
    "derive",
];

/// The list's items, and a failure for each row that is not `<kind> std::<path>
/// stable|unstable`: a misspelt stability would otherwise make a banned item look unstable and
/// kept. `#` lines are comments and the `toolchain` line names the toolchain.
pub(crate) fn parse_list(list: &str) -> (Vec<Item>, Vec<String>) {
    let (mut items, mut failures) = (Vec::new(), Vec::new());
    for (n, line) in list.lines().enumerate() {
        if line.starts_with('#') || line.starts_with("toolchain ") || line.trim().is_empty() {
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            [kind, path, stability @ ("stable" | "unstable")]
                if KINDS.contains(kind) && path.starts_with("std::") =>
            {
                let stable = *stability == "stable";
                items.push(Item { kind: (*kind).to_owned(), path: (*path).to_owned(), stable });
            }
            _ => failures.push(format!(
                "{}, line {}: `{line}` is not `<kind> std::<path> stable|unstable`",
                super::STD_ITEMS,
                n + 1
            )),
        }
    }
    (items, failures)
}

/// The top-level modules in std's `index.html` and the macros at its root in `all.html`, then
/// the items of the [`MODULES`] in `all.html`, with the stability `page` gives each (`page`
/// reads a file relative to the std docs), then the float methods of `Duration`; an error
/// naming the first page that cannot be read, since guessing its stability would write a wrong
/// list.
pub(crate) fn extract(
    all_html: &str,
    page: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<Item>, String> {
    let links = |html: &str| -> Vec<String> {
        let found = html.split("href=\"").skip(1).filter_map(|rest| rest.split('"').next());
        let mut found: Vec<String> = found.map(str::to_owned).collect();
        found.sort_unstable();
        found.dedup();
        found
    };
    let mut items = Vec::new();
    let index = page("index.html").ok_or("cannot read the page index.html")?;
    for href in links(&index) {
        let Some(module) = href.strip_suffix("/index.html").filter(|m| !m.contains(['/', '.']))
        else {
            continue;
        };
        let text = page(&href).ok_or(format!("cannot read the page {href}"))?;
        let path = format!("std::{module}");
        items.push(Item { kind: "module".to_owned(), path, stable: heading_is_stable(&text) });
    }
    let all = links(all_html);
    let root_macros = all.iter().filter(|h| h.starts_with("macro.") && !h.contains('/'));
    let in_modules = all.iter().filter(|h| MODULES.iter().any(|m| h.starts_with(&format!("{m}/"))));
    for href in root_macros.chain(in_modules) {
        let href = href.as_str();
        let (dirs, file) = href.rsplit_once('/').unwrap_or(("", href));
        let Some((kind, name)) = file.strip_suffix(".html").and_then(|f| f.split_once('.')) else {
            continue;
        };
        let text = page(href).ok_or(format!("cannot read the page {href}"))?;
        let path = if dirs.is_empty() {
            format!("std::{name}")
        } else {
            format!("std::{}::{name}", dirs.replace('/', "::"))
        };
        items.push(Item { kind: kind.to_owned(), path, stable: heading_is_stable(&text) });
    }
    let duration = "time/struct.Duration.html";
    items.extend(float_methods(&page(duration).ok_or(format!("cannot read the page {duration}"))?));
    Ok(items)
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
        "# Every top-level module and root macro of std, the public items of the modules decided\n\
         # item by item, and the float methods of Duration (docs/adr/0006-nondeterminism-gate.md),\n\
         # from rust-docs {toolchain}:\n\
         # cargo run -p xtask -- std-items <rust-docs>/share/doc/rust/html/std {toolchain}\n\
         toolchain {toolchain}\n"
    );
    let lines = items.iter().map(|item| {
        let stability = if item.stable { "stable" } else { "unstable" };
        format!("{} {} {stability}\n", item.kind, item.path)
    });
    [header].into_iter().chain(lines).collect()
}

/// A failure when the list names no toolchain, or another than `rust-toolchain.toml` pins.
pub(crate) fn toolchain_matches(list: &str, toolchain_toml: &str) -> Option<String> {
    let Some(listed) = list.lines().find_map(|l| l.strip_prefix("toolchain ")) else {
        return Some(format!("{} has no toolchain line: regenerate it", super::STD_ITEMS));
    };
    let listed = listed.trim().to_owned();
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
