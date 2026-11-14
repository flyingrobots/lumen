//! Repository checks that cargo does not provide.
//!
//! `cargo run --locked -p xtask -- file-length` measures every `.rs` file under `crates/` and
//! `xtask/` against the Rust standard's file limits ([`file_length`]) and exits 1 if any fails.
//! Files held by an exception are printed as GitHub warning annotations, so they stay visible.

#![expect(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    reason = "std-env, std-fs: repository tooling reads its arguments and the source tree"
)]

mod file_length;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{fs, io};

use file_length::{code_lines, judge, Measured, EXCEPTIONS};

/// The directories measured, relative to the repository root.
const ROOTS: [&str; 2] = ["crates", "xtask"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args != ["file-length"] {
        eprintln!("usage: cargo run --locked -p xtask -- file-length");
        return ExitCode::from(2);
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let files = match measure(&root) {
        Ok(files) => files,
        Err(error) => {
            eprintln!("file-length: {error}");
            return ExitCode::from(2);
        }
    };
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
    if verdict.failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Every `.rs` file under [`ROOTS`] and its lines of code, sorted by path.
fn measure(root: &Path) -> io::Result<Vec<Measured>> {
    let mut paths = Vec::new();
    for dir in ROOTS {
        collect(&root.join(dir), &mut paths)?;
    }
    let mut files = Vec::new();
    for path in paths {
        let relative = path.strip_prefix(root).map_err(io::Error::other)?;
        let parts: Vec<String> =
            relative.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        files.push(Measured {
            path: parts.join("/"),
            lines: code_lines(&fs::read_to_string(&path)?),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
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
