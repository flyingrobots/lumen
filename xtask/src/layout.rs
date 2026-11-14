//! Which files each crate compiles, and in which role: the module tree, followed from each
//! crate root through its `mod` declarations the way `rustc` resolves them.
//!
//! A file is a test file when every declaration that reaches it is under `#[cfg(test)]` (on the
//! `mod` item, on an enclosing inline module, or as the file's own `#![cfg(test)]`), or, when no
//! root reaches it, when it is under the crate's `tests/` directory; its name does not matter.
//! The deterministic core is every file the `lumen` library compiles outside tests, and every
//! other source file under its `src/` but outside `src/bin/`.
//!
//! The gates read only `.rs` files under the roots, so the forms that make the compiler read
//! anything else fail ([`Layout::failures`]): `include!`, a `#[path]` naming a file the gates do
//! not read, and a `path` inside `cfg_attr`, whose file cannot be known without the `cfg`.

mod parse;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use parse::parse;

/// The `lumen` library's crate root, whose non-test modules are the deterministic core.
pub(crate) const LIBRARY: &str = "crates/lumen/src/lib.rs";

/// What a file is compiled as. `Source` outranks `Test`: a file reached both ways is source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Role {
    /// Compiled only into tests.
    Test,
    /// Compiled into a library, a binary, an example or a build script.
    Source,
}

/// A file that makes the compiler read something the gates do not.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Failure {
    /// The file it is in.
    pub(crate) path: String,
    /// The 1-based line.
    pub(crate) line: usize,
    /// What was found.
    pub(crate) what: String,
}

/// Every file's role, the deterministic core and the failures.
#[derive(Debug)]
pub(crate) struct Layout {
    roles: BTreeMap<String, Role>,
    library: BTreeSet<String>,
    /// Each a form that pulls in a file the gates do not read.
    pub(crate) failures: BTreeSet<Failure>,
}

/// A `mod` declaration without a body.
struct Decl {
    line: usize,
    name: String,
    /// Under `#[cfg(test)]`, itself or through an enclosing inline module.
    test: bool,
    /// Its `#[path]`, if it has one.
    path: Option<String>,
    /// The enclosing inline modules, outermost first.
    inline: Vec<InlineDir>,
}

/// What an enclosing inline module does to the directory of the modules it declares: adds its
/// name, or, with a `#[path]`, is that directory, relative to the current one.
#[derive(Clone, Debug, PartialEq, Eq)]
enum InlineDir {
    /// `mod name { ... }`.
    Name(String),
    /// `#[path = "dir"] mod name { ... }`.
    Path(String),
}

/// A file's declarations, whether it is `#![cfg(test)]`, and its failures by line.
struct Parsed {
    test: bool,
    decls: Vec<Decl>,
    failures: Vec<(usize, String)>,
}

impl Layout {
    /// The layout of `files` (path and text, paths relative to the repository root with `/`):
    /// every `.rs` file the gates read, and nothing else. Its crates are the directories
    /// `crates` (each holding a `Cargo.toml`).
    pub(crate) fn new(files: &[(String, String)], crates: &[String]) -> Self {
        let text: BTreeMap<&str, &str> =
            files.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
        let mut walk = Walk {
            text: &text,
            roles: BTreeMap::new(),
            seen: BTreeMap::new(),
            failures: BTreeSet::new(),
        };
        walk.visit(LIBRARY, Role::Source, true);
        let library = walk.roles.iter().filter(|(_, r)| **r == Role::Source);
        let library: BTreeSet<String> = library.map(|(p, _)| p.clone()).collect();
        for (path, _) in files {
            if let Some(role) = crates.iter().find_map(|c| root_role(c, path)) {
                walk.visit(path, role, true);
            }
        }
        for (path, _) in files {
            if walk.roles.contains_key(path) {
                continue;
            }
            let in_tests = crates.iter().any(|c| path.starts_with(&format!("{c}/tests/")));
            let mod_rs = path.ends_with("/mod.rs");
            walk.visit(path, if in_tests { Role::Test } else { Role::Source }, mod_rs);
        }
        Layout { roles: walk.roles, library, failures: walk.failures }
    }

    /// The role of `path`.
    pub(crate) fn role(&self, path: &str) -> Role {
        self.roles.get(path).copied().unwrap_or(Role::Source)
    }

    /// Whether `path` is in the deterministic core.
    pub(crate) fn is_core(&self, path: &str) -> bool {
        let in_library =
            path.starts_with("crates/lumen/src/") && !path.starts_with("crates/lumen/src/bin/");
        self.library.contains(path) || (in_library && self.role(path) == Role::Source)
    }
}

/// The role of `path` if it is a crate root of the crate in directory `krate`: the library, a
/// binary, an example, a bench or the build script is source; an integration test is a test.
fn root_role(krate: &str, path: &str) -> Option<Role> {
    let rest = path.strip_prefix(krate)?.strip_prefix('/')?;
    let parts: Vec<&str> = rest.split('/').collect();
    let target = |dir: &str| match parts.as_slice() {
        [d, _] | [d, _, "main.rs"] => *d == dir,
        _ => false,
    };
    let binary =
        rest.starts_with("src/bin/") && (parts.len() == 3 || parts.get(3) == Some(&"main.rs"));
    if matches!(rest, "src/lib.rs" | "src/main.rs" | "build.rs")
        || binary
        || ["examples", "benches"].iter().any(|d| target(d))
    {
        Some(Role::Source)
    } else {
        target("tests").then_some(Role::Test)
    }
}

/// The module walk: the files' texts, the roles found so far and the failures.
struct Walk<'a> {
    text: &'a BTreeMap<&'a str, &'a str>,
    roles: BTreeMap<String, Role>,
    /// Each file and directory ownership visited, with the highest role it was visited as: a
    /// file reached both as `mod.rs`-like and not resolves its children both ways.
    seen: BTreeMap<(String, bool), Role>,
    failures: BTreeSet<Failure>,
}

impl Walk<'_> {
    /// Visits `path` as `role` (a `mod.rs`-like file when `mod_rs`) and the modules it declares.
    fn visit(&mut self, path: &str, role: Role, mod_rs: bool) {
        let Some(text) = self.text.get(path) else { return };
        let parsed = parse(text);
        let role = if parsed.test { Role::Test } else { role };
        let key = (path.to_owned(), mod_rs);
        if self.seen.get(&key).is_some_and(|known| *known >= role) {
            return;
        }
        self.seen.insert(key, role);
        let known = self.roles.get(path).copied();
        self.roles.insert(path.to_owned(), known.map_or(role, |k| k.max(role)));
        for (line, what) in parsed.failures {
            self.failures.insert(Failure { path: path.to_owned(), line, what });
        }
        for decl in parsed.decls {
            let child_role = if decl.test { Role::Test } else { role };
            match self.resolve(path, mod_rs, &decl) {
                Ok(Some((child, child_mod_rs))) => self.visit(&child, child_role, child_mod_rs),
                Ok(None) => {}
                Err(what) => {
                    self.failures.insert(Failure { path: path.to_owned(), line: decl.line, what });
                }
            }
        }
    }

    /// The file `decl` in `path` loads and whether it is `mod.rs`-like, as the Rust reference's
    /// "Module source filenames" and "The path attribute" give it; `None` for a module whose
    /// file is missing (the compiler refuses it unless its `cfg` is off).
    fn resolve(
        &self,
        path: &str,
        mod_rs: bool,
        decl: &Decl,
    ) -> Result<Option<(String, bool)>, String> {
        let parent = path.rsplit_once('/').map_or("", |(dir, _)| dir);
        let stem = path.rsplit('/').next().unwrap_or("").trim_end_matches(".rs");
        // rustc_expand's module.rs: a non-`mod.rs` file's own directory (`n/` for `n.rs`) is
        // pending until a named inline module or an out-of-line `mod` needs it; an inline
        // module's `#[path]` replaces the directory and drops the pending part.
        let (mut dir, mut pending) = (parent.to_owned(), (!mod_rs).then_some(stem));
        for inline in &decl.inline {
            match inline {
                InlineDir::Name(name) => {
                    if let Some(own) = pending.take() {
                        dir = format!("{dir}/{own}");
                    }
                    dir = format!("{dir}/{name}");
                }
                InlineDir::Path(to) => {
                    dir = normalize(&format!("{dir}/{to}")).unwrap_or_default();
                    pending = None;
                }
            }
        }
        if let Some(target) = &decl.path {
            let file = normalize(&format!("{dir}/{target}"));
            return match file {
                Some(f) if self.text.contains_key(f.as_str()) => Ok(Some((f, true))),
                _ => Err(format!("#[path = \"{target}\"] loads a file the gates do not read: make it a .rs module under crates/ or xtask/")),
            };
        }
        let base = pending.map_or_else(|| dir.clone(), |own| format!("{dir}/{own}"));
        let flat = format!("{base}/{}.rs", decl.name);
        let nested = format!("{base}/{}/mod.rs", decl.name);
        Ok([(flat, false), (nested, true)]
            .into_iter()
            .find(|(f, _)| self.text.contains_key(f.as_str())))
    }
}

/// `path` with `.` and `..` segments folded, or `None` if it leaves the repository.
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
}
