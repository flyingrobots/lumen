//! The edge of the workspace boundary: runs `cargo metadata` and reads its output.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{boundary, packages, Boundary};

/// The workspace boundary ([`super`]) of the repository at `root`, from `cargo metadata
/// --no-deps`, which lists every member and its declared dependencies without resolving them.
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
    Ok(boundary(&packages))
}
