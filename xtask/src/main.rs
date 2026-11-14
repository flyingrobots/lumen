//! Repository checks that cargo does not provide.
//!
//! `cargo run --locked -p xtask -- file-length` measures every `.rs` file under `crates/` and
//! `xtask/` against the Rust standard's file limits ([`file_length`]) and exits 1 if any fails.
//! Files held by an exception are printed as GitHub warning annotations, so they stay visible.
//!
//! `cargo run --locked -p xtask -- nondeterminism` scans the deterministic core ([`is_core`]) for
//! what Clippy's bans cannot see, and every file for the form of their waivers
//! ([`nondeterminism`]), and exits 1 on any finding.

#![expect(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    reason = "std-env, std-fs: repository tooling reads its arguments and the source tree"
)]

mod file_length;
mod nondeterminism;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{fs, io};

use file_length::{code_lines, judge, Measured, EXCEPTIONS};

/// The directories read, relative to the repository root.
const ROOTS: [&str; 2] = ["crates", "xtask"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let gate = match args.as_slice() {
        [command] if command == "file-length" => file_length_gate,
        [command] if command == "nondeterminism" => nondeterminism_gate,
        _ => {
            eprintln!("usage: cargo run --locked -p xtask -- file-length|nondeterminism");
            return ExitCode::from(2);
        }
    };
    match sources(&root) {
        Ok(files) if gate(&files) => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::from(2)
        }
    }
}

/// The file-length gate over `files`; true when it passes.
fn file_length_gate(files: &[(String, String)]) -> bool {
    let files: Vec<Measured> = files
        .iter()
        .map(|(path, text)| Measured { path: path.clone(), lines: code_lines(text) })
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
    verdict.failures.is_empty()
}

/// Whether `path` is in the deterministic core: the `lumen` library's sources, without the
/// command (`src/bin/`) and without test files (under a `tests` directory or named `tests.rs`).
fn is_core(path: &str) -> bool {
    path.starts_with("crates/lumen/src/")
        && !path.starts_with("crates/lumen/src/bin/")
        && !file_length::is_test(path)
}

/// The nondeterminism scan over `files`; true when nothing is found.
fn nondeterminism_gate(files: &[(String, String)]) -> bool {
    let (mut core, mut failures) = (0, 0);
    for (path, text) in files {
        let mut found = nondeterminism::waiver_forms(text);
        if is_core(path) {
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
    failures == 0
}

/// Every `.rs` file under [`ROOTS`], relative to `root` with `/` separators, and its text,
/// sorted by path.
fn sources(root: &Path) -> io::Result<Vec<(String, String)>> {
    let mut paths = Vec::new();
    for dir in ROOTS {
        collect(&root.join(dir), &mut paths)?;
    }
    let mut files = Vec::new();
    for path in paths {
        let relative = path.strip_prefix(root).map_err(io::Error::other)?;
        let parts: Vec<String> =
            relative.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        files.push((parts.join("/"), fs::read_to_string(&path)?));
    }
    files.sort();
    Ok(files)
}

/// Adds every `.rs` file under `dir` to `out`, skipping `target` directories.
fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "target") {
                collect(&path, out)?;
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}
