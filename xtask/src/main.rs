//! Repository checks that cargo does not provide.
//!
//! `cargo run --locked -p xtask -- file-length` measures every `.rs` file under `crates/` and
//! `xtask/` against the Rust standard's file limits ([`file_length`]), each by the role the module
//! tree gives it ([`layout`]), and exits 1 if any fails. Files held by an exception are printed as
//! GitHub warning annotations, so they stay visible.
//!
//! `cargo run --locked -p xtask -- nondeterminism` scans the deterministic core
//! ([`layout::Layout::is_core`]) for what Clippy's bans cannot see, and every file for the form of
//! their waivers ([`nondeterminism`]), and exits 1 on any finding.
//!
//! Both fail on a form that makes the compiler read a file they do not: `include!`, or a
//! `#[path]` to anything but a `.rs` file under the roots ([`layout::Layout::failures`]).

#![expect(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    reason = "std-env, std-fs: repository tooling reads its arguments and the source tree"
)]

mod file_length;
mod layout;
mod nondeterminism;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{fs, io};

use file_length::{code_lines, judge, Measured, EXCEPTIONS};
use layout::Layout;

/// The directories read, relative to the repository root.
const ROOTS: [&str; 2] = ["crates", "xtask"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let gate = match args.as_slice() {
        [command] if command == "file-length" => file_length_gate as fn(&_, &_) -> bool,
        [command] if command == "nondeterminism" => nondeterminism_gate,
        _ => {
            eprintln!("usage: cargo run --locked -p xtask -- file-length|nondeterminism");
            return ExitCode::from(2);
        }
    };
    match sources(&root) {
        Ok((files, crates)) if gate(&files, &Layout::new(&files, &crates)) => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::from(2)
        }
    }
}

/// Prints `layout`'s failures as errors; true when there are none.
fn layout_passes(layout: &Layout) -> bool {
    for failure in &layout.failures {
        println!(
            "::error file={},line={}::[source-layout] {}",
            failure.path, failure.line, failure.what
        );
    }
    layout.failures.is_empty()
}

/// The file-length gate over `files`; true when it passes.
fn file_length_gate(files: &[(String, String)], layout: &Layout) -> bool {
    let files: Vec<Measured> = files
        .iter()
        .map(|(path, text)| Measured {
            path: path.clone(),
            role: layout.role(path),
            lines: code_lines(text),
        })
        .collect();
    let verdict = judge(&files, EXCEPTIONS);
    for notice in &verdict.notices {
        println!("::warning::{notice}");
    }
    for failure in &verdict.failures {
        println!("::error::{failure}");
    }
    println!(
        "file-length: {} files, {} under an exception, {} failing",
        files.len(),
        verdict.notices.len(),
        verdict.failures.len()
    );
    layout_passes(layout) && verdict.failures.is_empty()
}

/// The nondeterminism scan over `files`; true when nothing is found.
fn nondeterminism_gate(files: &[(String, String)], layout: &Layout) -> bool {
    let (mut core, mut failures) = (0, 0);
    for (path, text) in files {
        let mut found = nondeterminism::waiver_forms(text);
        if layout.is_core(path) {
            core += 1;
            found.extend(nondeterminism::core_findings(text));
        }
        for finding in &found {
            println!(
                "::error file={path},line={}::[{}] {}",
                finding.line, finding.rule, finding.what
            );
        }
        failures += found.len();
    }
    println!(
        "nondeterminism: {} files, {core} in the deterministic core, {failures} findings",
        files.len()
    );
    layout_passes(layout) && failures == 0
}

/// Every `.rs` file under [`ROOTS`] with its text, and every directory there that holds a
/// `Cargo.toml` (a crate), relative to `root` with `/` separators, sorted.
fn sources(root: &Path) -> io::Result<(Vec<(String, String)>, Vec<String>)> {
    let (mut paths, mut manifests) = (Vec::new(), Vec::new());
    for dir in ROOTS {
        collect(&root.join(dir), &mut paths, &mut manifests)?;
    }
    let relative = |path: &Path| -> io::Result<String> {
        let relative = path.strip_prefix(root).map_err(io::Error::other)?;
        let parts: Vec<String> =
            relative.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        Ok(parts.join("/"))
    };
    let mut files = Vec::new();
    for path in paths {
        files.push((relative(&path)?, fs::read_to_string(&path)?));
    }
    let mut crates = manifests.iter().map(|dir| relative(dir)).collect::<io::Result<Vec<_>>>()?;
    files.sort();
    crates.sort();
    Ok((files, crates))
}

/// Adds every `.rs` file under `dir` to `out` and every directory holding a `Cargo.toml` to
/// `crates`, skipping `target` directories.
fn collect(dir: &Path, out: &mut Vec<PathBuf>, crates: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "target") {
                collect(&path, out, crates)?;
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        } else if path.file_name().is_some_and(|name| name == "Cargo.toml") {
            crates.push(dir.to_path_buf());
        }
    }
    Ok(())
}
