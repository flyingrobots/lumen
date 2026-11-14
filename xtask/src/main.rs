//! Repository checks that cargo does not provide.
//!
//! `cargo run --locked -p xtask -- file-length` measures every `.rs` file under `crates/` and
//! `xtask/` against the Rust standard's file limits ([`file_length`]) and exits 1 if any fails.
//! Files held by an exception are printed as GitHub warning annotations, so they stay visible.
//!
//! `cargo run --locked -p xtask -- nondeterminism` scans the deterministic core ([`is_core`]) for
//! what Clippy's bans cannot see, and every file for the form of their waivers
//! ([`nondeterminism`]), and exits 1 on any finding.
//!
//! `cargo run --locked -p xtask -- doc-links` checks every relative link of the repository's
//! Markdown: the file or directory it names exists, and its `#anchor` is a heading of the page
//! ([`doc_links`]). It exits 1 on any broken link. Links with a scheme are not followed.

#![expect(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    reason = "std-env, std-fs: repository tooling reads its arguments and the source tree"
)]

mod doc_links;
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
    let passed = match args.as_slice() {
        [command] if command == "file-length" => sources(&root).map(|f| file_length_gate(&f)),
        [command] if command == "nondeterminism" => sources(&root).map(|f| nondeterminism_gate(&f)),
        [command] if command == "doc-links" => markdown(&root).map(|f| doc_links_gate(&root, &f)),
        _ => {
            eprintln!("usage: cargo run --locked -p xtask -- file-length|nondeterminism|doc-links");
            return ExitCode::from(2);
        }
    };
    match passed {
        Ok(true) => ExitCode::SUCCESS,
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

/// The link check over the Markdown `pages`; true when no link is broken.
fn doc_links_gate(root: &Path, pages: &[(String, String)]) -> bool {
    let broken = doc_links::check(pages, |path| root.join(path).exists());
    for link in &broken {
        println!("::error file={},line={}::{}: {}", link.page, link.line, link.link, link.why);
    }
    println!("doc-links: {} pages, {} broken links", pages.len(), broken.len());
    broken.is_empty()
}

/// Every `.rs` file under [`ROOTS`], relative to `root` with `/` separators, and its text,
/// sorted by path.
fn sources(root: &Path) -> io::Result<Vec<(String, String)>> {
    let mut paths = Vec::new();
    for dir in ROOTS {
        collect(&root.join(dir), "rs", &mut paths)?;
    }
    read_relative(root, paths)
}

/// Every `.md` file of the repository, relative to `root` with `/` separators, and its text,
/// sorted by path: hidden directories, `target` and `out` (where `make` puts images) skipped.
fn markdown(root: &Path) -> io::Result<Vec<(String, String)>> {
    let mut paths = Vec::new();
    collect(root, "md", &mut paths)?;
    read_relative(root, paths)
}

/// `paths` read, each relative to `root` with `/` separators, sorted by path.
fn read_relative(root: &Path, paths: Vec<PathBuf>) -> io::Result<Vec<(String, String)>> {
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

/// Adds every file with `extension` under `dir` to `out`, skipping `target`, `out` and hidden
/// directories.
fn collect(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            let name = path.file_name().map(|name| name.to_string_lossy().into_owned());
            if name.is_some_and(|name| !name.starts_with('.') && name != "target" && name != "out")
            {
                collect(&path, extension, out)?;
            }
        } else if path.extension().is_some_and(|ext| ext == extension) {
            out.push(path);
        }
    }
    Ok(())
}
