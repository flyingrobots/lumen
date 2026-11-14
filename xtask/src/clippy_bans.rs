//! The compiler layer's own check (`cargo run -p xtask -- clippy-bans`): every `clippy.toml`
//! ban resolves and fires, the fixture's seeded forms are each reported under their rule and its
//! look-alikes are not, and the lint groups that hold a ban lint are the ones the scan refuses.
//!
//! Clippy reports a `clippy.toml` path that resolves to nothing only as a warning about the file
//! itself, which `-D warnings` does not make an error, so a misspelt ban was a silent hole. The
//! check runs Clippy on `xtask/clippy-fixture` (with `--cfg lumen_clippy_fixture`) and fails on
//! any diagnostic located in `clippy.toml`, on an entry that never fires on the fixture (which
//! also catches a path that resolves to some other item than the one used), and on a fixture
//! that does not compile. It also fails when `clippy.toml` lacks a std item that
//! [`std_items`] decides banned, or the item list is for another toolchain. This module parses
//! and judges; [`run`] runs Clippy.

pub(crate) mod run;
pub(crate) mod std_items;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use crate::nondeterminism::BAN_GROUPS;

/// The fixture's sources, relative to the repository root.
pub(crate) const FIXTURE_SRC: &str = "xtask/clippy-fixture/src";

/// The std item list, relative to the repository root ([`std_items`]).
pub(crate) const STD_ITEMS: &str = "xtask/bans/std-items.txt";

/// The Clippy lints of the bans, as `cargo clippy -- -W help` spells them.
const BAN_LINTS: [&str; 4] = [
    "clippy::disallowed-methods",
    "clippy::disallowed-types",
    "clippy::disallowed-macros",
    "clippy::float-arithmetic",
];

/// One ban in `clippy.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    /// Its 1-based line.
    pub(crate) line: usize,
    /// The banned item's path, as written.
    pub(crate) path: String,
    /// The rule ID its reason starts with.
    pub(crate) rule: String,
}

/// One diagnostic of Clippy's `--message-format=short` output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Diagnostic {
    /// The file, as Clippy prints it (relative to the workspace root, or absolute).
    pub(crate) file: String,
    /// Its 1-based line.
    pub(crate) line: usize,
    /// `error`, `warning`, or a code such as `error[E0433]`.
    pub(crate) level: String,
    /// The message.
    pub(crate) message: String,
}

/// A module of the fixture marked `// seeded: <rule>` (or `// accepted`, with no rule).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Form {
    /// The file, relative to the repository root.
    pub(crate) file: String,
    /// The module's name.
    pub(crate) name: String,
    /// Its first and last line.
    pub(crate) lines: (usize, usize),
    /// The rule it must be reported under; `None` for a look-alike.
    pub(crate) rule: Option<String>,
}

/// The bans of `clippy.toml`: each line with a `path = "..."` and a `reason = "<rule>: ..."`.
pub(crate) fn entries(toml: &str) -> Vec<Entry> {
    let quoted = |line: &str, key: &str| {
        let rest = &line[line.find(&format!("{key} = \""))? + key.len() + 4..];
        Some(rest[..rest.find('"')?].to_owned())
    };
    let lines = toml.lines().enumerate();
    lines
        .filter_map(|(n, line)| {
            let path = quoted(line, "path")?;
            let reason = quoted(line, "reason").unwrap_or_default();
            let rule = reason.split_once(':').map_or(reason.as_str(), |(r, _)| r).to_owned();
            Some(Entry { line: n + 1, path, rule })
        })
        .collect()
}

/// The diagnostics in Clippy's short output: lines `file:line:col: level: message`.
pub(crate) fn diagnostics(output: &str) -> Vec<Diagnostic> {
    output
        .lines()
        .filter_map(|line| {
            let (at, rest) = line.split_once(": ")?;
            let (level, message) = rest.split_once(": ")?;
            let mut parts = at.rsplitn(3, ':');
            let (_col, line, file) = (parts.next()?, parts.next()?, parts.next()?);
            let known = level.starts_with("error") || level == "warning";
            let line = line.parse().ok().filter(|_| known)?;
            Some(Diagnostic {
                file: file.to_owned(),
                line,
                level: level.to_owned(),
                message: message.to_owned(),
            })
        })
        .collect()
}

/// The marked modules of a fixture file: a `// seeded: <rule>` or `// accepted` line, then a
/// `mod name {` line, up to the next line that is exactly `}`.
pub(crate) fn forms(file: &str, text: &str) -> Vec<Form> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (n, line) in lines.iter().enumerate() {
        let rule = match line.trim() {
            "// accepted" => None,
            marker => match marker.strip_prefix("// seeded: ") {
                Some(rule) => Some(rule.to_owned()),
                None => continue,
            },
        };
        let Some(name) = lines.get(n + 1).and_then(|l| l.strip_prefix("mod ")?.strip_suffix(" {"))
        else {
            continue;
        };
        let end = lines[n + 1..].iter().position(|l| *l == "}").map_or(lines.len(), |e| n + 2 + e);
        out.push(Form { file: file.to_owned(), name: name.to_owned(), lines: (n + 2, end), rule });
    }
    out
}

/// The lint groups that hold a ban lint, from `cargo clippy -- -W help`'s group table.
pub(crate) fn ban_groups(help: &str) -> BTreeSet<String> {
    help.lines()
        .filter_map(|line| {
            let (group, members) = line.trim().split_once(char::is_whitespace)?;
            let members: Vec<&str> = members.trim().split(", ").collect();
            let holds = members.iter().any(|m| BAN_LINTS.contains(m));
            (group.starts_with("clippy::") && holds).then(|| group.to_owned())
        })
        .collect()
}

/// The rule of a ban finding and the entry it names: the entry whose path a message "use of a
/// disallowed method" (or type, or macro) quotes, or `float-type` and no entry for
/// `clippy::float_arithmetic`; `None` for any other diagnostic.
fn finding_rule<'a>(message: &str, entries: &'a [Entry]) -> Option<(&'a str, Option<&'a str>)> {
    if message.starts_with("floating-point arithmetic detected") {
        return Some(("float-type", None));
    }
    let named = ["method", "type", "macro"].iter().find_map(|kind| {
        let rest = message.strip_prefix(&format!("use of a disallowed {kind} `"))?;
        rest.split('`').next()
    })?;
    let entry = entries.iter().find(|e| e.path == named)?;
    Some((entry.rule.as_str(), Some(entry.path.as_str())))
}

/// Every reason the compiler layer's check fails, given `clippy.toml`'s entries, the fixture's
/// forms, Clippy's diagnostics on the fixture and the groups that hold a ban lint.
pub(crate) fn judge(
    entries: &[Entry],
    forms: &[Form],
    found: &[Diagnostic],
    groups: &BTreeSet<String>,
) -> Vec<String> {
    let mut failures = Vec::new();
    let (mut fired, mut reported) = (BTreeSet::new(), Vec::new());
    for d in found {
        if d.file.ends_with("clippy.toml") {
            failures.push(format!("clippy.toml:{}: {}", d.line, d.message));
        } else if d.level.starts_with("error[") {
            failures.push(format!(
                "the fixture does not compile: {}:{}: {}",
                d.file, d.line, d.message
            ));
        } else if let Some((rule, path)) = finding_rule(&d.message, entries) {
            fired.extend(path);
            let form = forms
                .iter()
                .find(|f| f.file == d.file && (f.lines.0..=f.lines.1).contains(&d.line));
            match form {
                Some(f) if f.rule.is_none() => {
                    failures.push(format!("look-alike {} is reported: {}", f.name, d.message));
                }
                Some(f) => reported.push((f.name.as_str(), rule)),
                None => failures.push(format!(
                    "{}:{}: a finding outside every marked form: {}",
                    d.file, d.line, d.message
                )),
            }
        }
    }
    for form in forms {
        let Some(rule) = &form.rule else { continue };
        if !reported.contains(&(form.name.as_str(), rule.as_str())) {
            failures.push(format!("seeded form {} is not reported under {rule}", form.name));
        }
    }
    for e in entries.iter().filter(|e| !fired.contains(e.path.as_str())) {
        failures.push(format!(
            "clippy.toml:{}: `{}` never fires on the fixture: misspelt, resolving to another item, or not used there",
            e.line, e.path
        ));
    }
    let expected: BTreeSet<String> =
        BAN_GROUPS.iter().filter(|g| g.starts_with("clippy::")).map(|g| (*g).to_owned()).collect();
    if *groups != expected {
        failures.push(format!("the groups holding a ban lint are {groups:?}; the scan refuses {expected:?}: update BAN_GROUPS"));
    }
    if forms.iter().all(|f| f.rule.is_none()) {
        failures.push("the fixture has no seeded form".to_owned());
    }
    failures
}
