//! Size class: medium (spawns the `lumen` process and writes files under `CARGO_TARGET_TMPDIR`).
//!
//! The tutorial, `docs/tutorial.md`, run as written: each of its `lumen render` commands, in a
//! scratch directory of its own. Every exact line the tutorial shows for a command must be what
//! the command prints, so the page cannot promise a hash the program does not give.

#![expect(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::disallowed_macros,
    reason = "std-fs, std-process, std-env: runs the command in a scratch directory; a test outside \
              the deterministic core"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// The tutorial, as it is in the repository.
const TUTORIAL: &str = include_str!("../../../docs/tutorial.md");

/// A fresh directory for one test's files, removed when dropped, named for the test and the
/// process so no other test or run shares it.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("tutorial-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch directory");
        Self(dir)
    }

    /// The report of `lumen` run with `args` in this directory.
    fn lumen(&self, args: &[&str]) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_lumen"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .expect("lumen runs");
        assert!(
            output.status.success(),
            "lumen {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("utf-8")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The lines of the `text` block that follows the tutorial's `sh` block holding `command`, before
/// any other command's block. The tutorial is read with `\n` line ends, however the checkout
/// wrote them (Git on Windows checks text out with `\r\n`).
fn shown_after(command: &str) -> Vec<String> {
    let tutorial = TUTORIAL.replace("\r\n", "\n");
    let at = tutorial.find(&format!("```sh\n{command}\n```")).expect("the tutorial runs it");
    let rest = &tutorial[at..];
    let start = rest.find("```text\n").expect("an output block follows") + "```text\n".len();
    // The output block must be this command's: no other command's block comes between them.
    let next_command = rest[1..].find("```sh\n").map_or(usize::MAX, |at| at + 1);
    assert!(start < next_command, "the tutorial shows no output for `{command}`");
    let end = rest[start..].find("```").expect("the block ends");
    rest[start..start + end].lines().map(str::to_owned).collect()
}

/// Oracle: the program's own report. Every line the tutorial shows after `command` is a line
/// `lumen` prints for it (the timing lines, which vary, are not shown). Witness count: the lines
/// compared.
fn the_tutorial_shows_what(command: &str, name: &str) -> usize {
    let args: Vec<&str> = command
        .strip_prefix("cargo run --release -p lumen -- ")
        .expect("a lumen command")
        .split(' ')
        .collect();
    let report = Scratch::new(name).lumen(&args);
    let printed: Vec<&str> = report.lines().collect();
    let shown = shown_after(command);
    let missing: Vec<&String> =
        shown.iter().filter(|line| !printed.contains(&line.as_str())).collect();
    assert!(
        missing.is_empty(),
        "{command}: the tutorial shows {missing:?}; lumen printed:\n{report}"
    );
    shown.len()
}

#[test]
fn the_first_render_prints_what_the_tutorial_shows() {
    let compared =
        the_tutorial_shows_what("cargo run --release -p lumen -- render --out first.png", "first");
    assert!(compared >= 7, "{compared} lines");
}

#[test]
fn the_one_thread_render_prints_what_the_tutorial_shows() {
    let compared =
        the_tutorial_shows_what("cargo run --release -p lumen -- render --threads 1", "one-thread");
    assert!(compared >= 2, "{compared} lines");
}

#[test]
fn the_fixed_point_render_prints_what_the_tutorial_shows() {
    let command =
        "cargo run --release -p lumen -- render --scene fixed-point --out fixed-point.png";
    let compared = the_tutorial_shows_what(command, "fixed-point");
    assert!(compared >= 3, "{compared} lines");
}
