//! The edge of the workspace boundary: runs `cargo metadata`, reads the members' manifests and
//! looks for Cargo configuration files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::manifest::{config_failure, is_cargo_config, lints_failure, patch_failures};
use super::{boundary, packages, Boundary};

/// The workspace boundary ([`super`]) of the repository at `root`: `cargo metadata --no-deps`,
/// which lists every member and its declared dependencies without resolving them, then each
/// member's `[lints]` ([`lints_failure`]), the root manifest's patches and every Cargo
/// configuration file under `root`.
pub(crate) fn boundary_of(root: &Path) -> io::Result<Boundary> {
    let cargo = std::env::var_os("CARGO").map_or_else(|| PathBuf::from("cargo"), PathBuf::from);
    let output = Command::new(cargo)
        .args(["metadata", "--no-deps", "--format-version", "1", "--locked"])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        let why = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::other(format!("cargo metadata failed: {why}")));
    }
    let packages = packages(&String::from_utf8_lossy(&output.stdout)).map_err(io::Error::other)?;
    let mut found = boundary(&packages);
    for package in &packages {
        let manifest = fs::read_to_string(root.join(&package.dir).join("Cargo.toml"))?;
        if let Some(what) = lints_failure(&manifest) {
            found.failures.push(format!("{}/Cargo.toml {what}", package.dir));
        }
    }
    found.failures.extend(patch_failures(&fs::read_to_string(root.join("Cargo.toml"))?));
    let mut configs = Vec::new();
    cargo_configs(root, root, &mut configs)?;
    found.failures.extend(configs.iter().map(|path| config_failure(path)));
    Ok(found)
}

/// Adds the path, relative to `root`, of every `.cargo/config.toml` and `.cargo/config` under
/// `dir` to `out`, skipping `target` and `.git`.
fn cargo_configs(root: &Path, dir: &Path, out: &mut Vec<String>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if path.is_dir() {
            if name != "target" && name != ".git" {
                cargo_configs(root, &path, out)?;
            }
        } else {
            let relative = path.strip_prefix(root).map_err(io::Error::other)?;
            let relative = relative.to_string_lossy().replace('\\', "/");
            if is_cargo_config(&relative) {
                out.push(relative);
            }
        }
    }
    out.sort();
    Ok(())
}
