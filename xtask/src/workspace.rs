//! This module owns the workspace boundary of the deterministic core: which crates' code can run
//! in, or decide the build of, the `lumen` library, read from `cargo metadata`
//! (docs/adr/0006-nondeterminism-gate.md, amendment of 2026-11-14).
//!
//! The deterministic crates are `lumen` and every workspace member its library reaches through
//! normal and build dependencies, on every target and with every optional dependency (the
//! metadata lists them all). Every other member must be classified as tooling here
//! ([`TOOLING`]); an unclassified member fails, so a new crate is a decision, not a default.
//! The boundary also fails on what would let code the gates do not read decide the library:
//! a path dependency outside the workspace, a dependency from anywhere but Bunny's repository at
//! a pinned commit, a build script or a procedural macro in a deterministic crate, a
//! deterministic crate outside the directories the gates read, a member that sets its own lint
//! levels, a `[patch]` or `[replace]`, and any Cargo configuration file ([`manifest`]).

pub(crate) mod json;
mod manifest;
pub(crate) mod run;
#[cfg(test)]
mod tests;

use json::Json;

/// The package whose library is the deterministic core.
pub(crate) const LIBRARY: &str = "lumen";

/// The workspace members outside the deterministic core, and why.
pub(crate) const TOOLING: [(&str, &str); 2] = [
    ("xtask", "repository tooling: the gates themselves"),
    ("lumen-clippy-fixture", "the compiler layer's seeded fixture, compiled only by clippy-bans"),
];

/// The only source a dependency may come from other than the workspace: Bunny's repository at a
/// full commit (`deny.toml` holds the same rule for `cargo-deny`).
const BUNNY: &str = "git+https://github.com/flyingrobots/bunny?rev=";

/// A workspace member, as `cargo metadata` describes it, paths relative to the workspace root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Package {
    /// Its name.
    pub(crate) name: String,
    /// The directory holding its `Cargo.toml`.
    pub(crate) dir: String,
    /// Its targets: each one's kinds (`lib`, `bin`, `custom-build`, `proc-macro`, ...) and source.
    pub(crate) targets: Vec<(Vec<String>, String)>,
    /// Its declared dependencies.
    pub(crate) deps: Vec<Dep>,
}

/// A declared dependency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Dep {
    /// The package it names.
    pub(crate) name: String,
    /// `normal`, `build` or `dev`.
    pub(crate) kind: String,
    /// Its directory, for a path dependency (relative, or absolute when outside the workspace).
    pub(crate) path: Option<String>,
    /// Its source (`git+...`, `registry+...`), for any other.
    pub(crate) source: Option<String>,
}

/// A deterministic crate: its name, its directory and its library root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Library {
    /// The package name.
    pub(crate) name: String,
    /// Its directory, relative to the workspace root.
    pub(crate) dir: String,
    /// Its library's root file, relative to the workspace root.
    pub(crate) root: String,
}

/// The deterministic crates and every reason the boundary fails.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Boundary {
    /// The deterministic crates, `lumen` first.
    pub(crate) libraries: Vec<Library>,
    /// Each a reason the gate fails.
    pub(crate) failures: Vec<String>,
}

/// The workspace members in `cargo metadata --no-deps --format-version 1` output, or why it
/// cannot be read.
pub(crate) fn packages(metadata: &str) -> Result<Vec<Package>, String> {
    let tree = json::parse(metadata)?;
    let root = tree.get("workspace_root").and_then(Json::str).ok_or("no workspace_root")?;
    let root = format!("{}/", slashes(root).trim_end_matches('/'));
    let relative = |path: &str| {
        let path = slashes(path);
        path.strip_prefix(&root).map_or(path.clone(), str::to_owned)
    };
    let members: Vec<&str> = tree
        .get("workspace_members")
        .map_or(&[][..], Json::items)
        .iter()
        .filter_map(Json::str)
        .collect();
    let mut out = Vec::new();
    for package in tree.get("packages").map_or(&[][..], Json::items) {
        let field = |key: &str| package.get(key).and_then(Json::str);
        if !field("id").is_some_and(|id| members.contains(&id)) {
            continue;
        }
        let name = field("name").ok_or("a package without a name")?.to_owned();
        let manifest = relative(field("manifest_path").ok_or("a package without a manifest")?);
        let dir = manifest.strip_suffix("Cargo.toml").unwrap_or(&manifest).trim_end_matches('/');
        let targets = package.get("targets").map_or(&[][..], Json::items).iter().map(|t| {
            let kinds = t.get("kind").map_or(&[][..], Json::items);
            let kinds = kinds.iter().filter_map(Json::str).map(str::to_owned).collect();
            (kinds, t.get("src_path").and_then(Json::str).map(&relative).unwrap_or_default())
        });
        let deps = package.get("dependencies").map_or(&[][..], Json::items).iter().map(|d| {
            let text = |key: &str| d.get(key).and_then(Json::str).map(str::to_owned);
            Dep {
                name: text("name").unwrap_or_default(),
                kind: text("kind").unwrap_or_else(|| "normal".to_owned()),
                path: text("path").map(|p| relative(&p)),
                source: text("source"),
            }
        });
        let (targets, deps) = (targets.collect(), deps.collect());
        out.push(Package { name, dir: dir.to_owned(), targets, deps });
    }
    Ok(out)
}

/// `path` with `/` separators.
fn slashes(path: &str) -> String {
    path.replace('\\', "/")
}

/// The boundary of `packages`, the workspace's members.
pub(crate) fn boundary(packages: &[Package]) -> Boundary {
    let mut out = Boundary::default();
    let member = |dir: &str| packages.iter().find(|p| p.dir == dir);
    let mut reached: Vec<&Package> = packages.iter().filter(|p| p.name == LIBRARY).collect();
    if reached.is_empty() {
        out.failures.push(format!("the workspace has no package `{LIBRARY}`"));
    }
    let mut next = 0;
    while let Some(package) = reached.get(next).copied() {
        next += 1;
        for dep in package.deps.iter().filter(|d| d.kind != "dev") {
            let found = dep.path.as_deref().and_then(member);
            if let Some(found) = found.filter(|f| !reached.iter().any(|r| r.dir == f.dir)) {
                reached.push(found);
            }
        }
    }
    for package in packages {
        let deterministic = reached.iter().any(|r| r.dir == package.dir);
        let tooling = TOOLING.iter().any(|(name, _)| *name == package.name);
        match (deterministic, tooling) {
            (true, true) => out.failures.push(format!(
                "`{}` is classified as tooling, but the library depends on it",
                package.name
            )),
            (false, false) => out.failures.push(format!(
                "workspace member `{}` ({}) is not classified: make the library depend on it, or \
                 add it to TOOLING in xtask/src/workspace.rs with its reason",
                package.name, package.dir
            )),
            _ => {}
        }
        out.failures.extend(foreign_dependencies(package, packages));
        if deterministic {
            out.failures.extend(code_from_outside(package));
            let under = |r: &&str| package.dir == *r || package.dir.starts_with(&format!("{r}/"));
            if !crate::ROOTS.iter().any(under) {
                out.failures.push(format!(
                    "`{}` is a deterministic crate in {}, outside crates/ and xtask/, which are all \
                     the gates read",
                    package.name, package.dir
                ));
            }
            if let Some((_, root)) =
                package.targets.iter().find(|(k, _)| k.iter().any(|k| k == "lib"))
            {
                let (name, dir, root) = (package.name.clone(), package.dir.clone(), root.clone());
                out.libraries.push(Library { name, dir, root });
            }
        }
    }
    out
}

/// A failure for each dependency of `package` that comes from outside the workspace and is not
/// Bunny at a pinned commit.
fn foreign_dependencies(package: &Package, packages: &[Package]) -> Vec<String> {
    let member = |path: &str| packages.iter().any(|p| p.dir == path);
    let found = package.deps.iter().filter_map(|dep| match (&dep.path, &dep.source) {
        (Some(path), _) if !member(path) => Some(format!(
            "`{}` depends on `{}` at `{path}`, a path outside the workspace's members",
            package.name, dep.name
        )),
        (None, Some(source)) if !pinned_bunny(source) => Some(format!(
            "`{}` depends on `{}` from `{source}`: only Bunny's repository at a full commit is allowed",
            package.name, dep.name
        )),
        (None, None) => Some(format!(
            "`{}` depends on `{}` with neither a path nor a source",
            package.name, dep.name
        )),
        _ => None,
    });
    found.collect()
}

/// Whether `source` is Bunny's repository at a full 40-digit commit.
fn pinned_bunny(source: &str) -> bool {
    source.strip_prefix(BUNNY).is_some_and(|rev| {
        rev.len() == 40 && rev.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    })
}

/// A failure for each target of a deterministic `package` whose code the gates cannot read as
/// the library's: a build script, which can set a `cfg` or an environment variable the library
/// reads, and a procedural macro, whose output is generated code.
fn code_from_outside(package: &Package) -> impl Iterator<Item = String> + '_ {
    package.targets.iter().filter_map(move |(kinds, src)| {
        let what = if kinds.iter().any(|k| k == "custom-build") {
            "a build script"
        } else if kinds.iter().any(|k| k == "proc-macro") {
            "a procedural macro"
        } else {
            return None;
        };
        Some(format!(
            "`{}` is a deterministic crate and has {what} ({src}): its output reaches the library \
             unread by the gates",
            package.name
        ))
    })
}
