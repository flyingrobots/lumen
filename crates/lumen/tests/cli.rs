//! The `lumen` command, run as a separate process.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use lumen::png::write_png;
use lumen::render_reference;

fn lumen(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lumen")).args(args).output().expect("lumen runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("utf-8")
}

/// A fresh directory for one test's files.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("cli-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

/// The value of the line `key value...` in the command's output.
fn field<'a>(stdout: &'a str, key: &str) -> &'a str {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(key).and_then(|rest| rest.strip_prefix(' ')))
        .unwrap_or_else(|| panic!("no {key} line in {stdout:?}"))
}

fn assert_refused(output: &Output, code: i32, message: &str) {
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(code), "stderr: {stderr}");
    assert!(stderr.contains(message), "{message:?} not in {stderr:?}");
    assert!(!stderr.contains("panicked"), "{stderr}");
    assert!(output.stdout.is_empty());
}

#[test]
fn two_processes_write_the_same_file_and_print_the_same_hash() {
    let dir = scratch("determinism");
    let (a, b) = (dir.join("a.png"), dir.join("b.png"));
    let mut hashes = Vec::new();
    for path in [&a, &b] {
        let output = lumen(&["render", "--width", "40", "--height", "24", "--out", path_str(path)]);
        assert!(output.status.success(), "{}", text(&output.stderr));
        let stdout = text(&output.stdout);
        assert_eq!(field(&stdout, "scene"), "lumen-reference-scene/1");
        assert_eq!(field(&stdout, "shading"), "lumen-normal-shading/1");
        assert_eq!(field(&stdout, "tone"), "bunny-tone-srgb/1");
        assert_eq!(field(&stdout, "bunny"), lumen::BUNNY_REV);
        assert_eq!(field(&stdout, "size"), "40x24");
        hashes.push(field(&stdout, "image").to_owned());
    }
    let (file_a, file_b) = (std::fs::read(&a).expect("a"), std::fs::read(&b).expect("b"));
    assert_eq!(file_a, file_b);
    assert_eq!(hashes[0], hashes[1]);
    // The printed hash and the file are those of the library's render of the same size.
    let image = render_reference(40, 24).expect("renders");
    assert_eq!(hashes[0], image.hash());
    let mut expected = Vec::new();
    write_png(&mut expected, &image).expect("writes to memory");
    assert_eq!(file_a, expected);
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("utf-8 path")
}

#[test]
fn help_and_version() {
    let help = lumen(&["--help"]);
    assert!(help.status.success());
    assert!(text(&help.stdout).starts_with("usage: lumen render"));
    let version = lumen(&["--version"]);
    assert!(version.status.success());
    assert!(text(&version.stdout).starts_with(&format!("lumen {}", env!("CARGO_PKG_VERSION"))));
}

#[test]
fn bad_arguments_are_refused_with_usage() {
    assert_refused(&lumen(&[]), 2, "no command given");
    assert_refused(&lumen(&["paint"]), 2, "unknown command \"paint\"");
    assert_refused(&lumen(&["render"]), 2, "render needs --out FILE");
    assert_refused(&lumen(&["render", "--out"]), 2, "--out needs a value");
    assert_refused(&lumen(&["render", "--out", "x.png", "--spp", "4"]), 2, "unknown option");
    assert_refused(
        &lumen(&["render", "--width", "wide", "--out", "x.png"]),
        2,
        "--width needs a whole number of pixels",
    );
    assert_refused(
        &lumen(&["render", "--height", "-3", "--out", "x.png"]),
        2,
        "--height needs a whole number of pixels",
    );
    assert_refused(
        &lumen(&["render", "--out", "a", "--out", "b"]),
        2,
        "--out given more than once",
    );
}

#[test]
fn bad_sizes_are_refused_before_anything_is_written() {
    let dir = scratch("sizes");
    let out = dir.join("x.png");
    let cases: [(&str, &str, &str); 4] = [
        ("0", "10", "at least 1"),
        ("10", "0", "at least 1"),
        ("32769", "10", "at most 32768"),
        ("32768", "32768", "at most 134217728 pixels"),
    ];
    for (width, height, message) in cases {
        let output =
            lumen(&["render", "--width", width, "--height", height, "--out", path_str(&out)]);
        assert_refused(&output, 1, message);
        assert!(!out.exists(), "{width}x{height} wrote a file");
    }
}

#[test]
fn an_unwritable_path_is_reported() {
    let dir = scratch("unwritable");
    let missing_parent = dir.join("no-such-directory").join("x.png");
    let output =
        lumen(&["render", "--width", "4", "--height", "3", "--out", path_str(&missing_parent)]);
    assert_refused(&output, 1, "no-such-directory");
    // A directory is not a file.
    let output = lumen(&["render", "--width", "4", "--height", "3", "--out", path_str(&dir)]);
    assert_refused(&output, 1, path_str(&dir));
}
