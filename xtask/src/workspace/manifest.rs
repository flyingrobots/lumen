//! This module owns what the manifests and Cargo's configuration files may say outside the
//! dependency graph: a member's lint levels come from the workspace, the workspace patches and
//! replaces no dependency, and the repository holds no Cargo configuration file, which could
//! set `rustflags` (a `--cfg`, or `-A` of a ban lint) that no source file shows.
//!
//! Manifests are read line by line, as TOML table headers and keys: comments and blank lines
//! are skipped, and anything this reading does not recognise in a `[lints]` table fails.

/// The failure for a member's manifest whose `[lints]` table is not exactly
/// `workspace = true`, or that has a `[lints.*]` table or a `lints` key of its own: such a
/// table sets lint levels per crate, and a ban lowered there (`disallowed_methods = "allow"`)
/// is in no source file the scan reads.
pub(crate) fn lints_failure(manifest: &str) -> Option<String> {
    let (mut table, mut inherited, mut own) = (String::new(), false, Vec::new());
    for line in manifest.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[') {
            header.split(']').next().unwrap_or_default().trim().clone_into(&mut table);
            if table.starts_with("lints.") {
                own.push(line.to_owned());
            }
            continue;
        }
        let key = line.split('=').next().unwrap_or_default().trim();
        if table == "lints" {
            if key == "workspace" && value(line) == "true" {
                inherited = true;
            } else {
                own.push(line.to_owned());
            }
        } else if key == "lints" || key.starts_with("lints.") {
            own.push(line.to_owned());
        }
    }
    if !own.is_empty() {
        Some(format!(
            "sets its own lint levels ({}): use `[lints]` `workspace = true` alone",
            own.join("; ")
        ))
    } else if !inherited {
        Some("has no `[lints]` table with `workspace = true`: the workspace's lint levels must apply".to_owned())
    } else {
        None
    }
}

/// The value of a `key = value` line, a trailing comment removed.
fn value(line: &str) -> &str {
    let value = line.split_once('=').map_or("", |(_, v)| v);
    value.split('#').next().unwrap_or_default().trim()
}

/// The failure for each `[patch]` or `[replace]` table of the root manifest: either swaps a
/// dependency for other code, a local path included, that the gates do not read.
pub(crate) fn patch_failures(manifest: &str) -> Vec<String> {
    manifest
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("[patch") || line.starts_with("[replace"))
        .map(|line| format!("Cargo.toml has `{line}`: a patched or replaced dependency is code the gates do not read"))
        .collect()
}

/// Whether `path` (relative, `/` separators) is a Cargo configuration file: `config.toml` or
/// `config` in a `.cargo` directory, which Cargo reads when run in that directory's parent or
/// below it.
pub(crate) fn is_cargo_config(path: &str) -> bool {
    let mut parts = path.rsplit('/');
    let (name, dir) = (parts.next(), parts.next());
    dir == Some(".cargo") && matches!(name, Some("config.toml" | "config"))
}

/// The failure for a Cargo configuration file at `path` (a `.cargo/config.toml` or
/// `.cargo/config` in the repository).
pub(crate) fn config_failure(path: &str) -> String {
    format!(
        "{path}: a Cargo configuration file can set rustflags (a --cfg, or -A of a ban lint) that \
         no source file shows; the repository holds none"
    )
}
