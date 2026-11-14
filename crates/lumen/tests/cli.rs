//! The `lumen` command, run as a separate process.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use lumen::{render_reference, write_png, RenderSettings};

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
    let (mut hashes, mut rays) = (Vec::new(), Vec::new());
    for path in [&a, &b] {
        let output = lumen(&[
            "render",
            "--width",
            "40",
            "--height",
            "24",
            "--samples",
            "2",
            "--seed",
            "5",
            "--out",
            path_str(path),
        ]);
        assert!(output.status.success(), "{}", text(&output.stderr));
        let stdout = text(&output.stdout);
        assert_eq!(field(&stdout, "scene"), "lumen-reference-scene/3");
        assert_eq!(field(&stdout, "integrator"), "lumen-diffuse-path/1");
        assert_eq!(field(&stdout, "shading"), "bunny-diffuse-shading/1");
        assert_eq!(field(&stdout, "sampler"), "bunny-sobol-owen/1");
        assert_eq!(field(&stdout, "tone"), "bunny-tone-srgb/1");
        assert_eq!(field(&stdout, "encoding"), "bunny-rgba8-opaque/1");
        assert_eq!(field(&stdout, "bunny"), lumen::BUNNY_REV);
        assert_eq!(field(&stdout, "size"), "40x24");
        assert_eq!(field(&stdout, "samples"), "2");
        assert_eq!(field(&stdout, "seed"), "5");
        assert_eq!(field(&stdout, "bounces"), "1");
        assert_eq!(field(&stdout, "pixels"), "960");
        assert_eq!(field(&stdout, "tile"), "32x32");
        assert!(field(&stdout, "threads").parse::<u32>().is_ok_and(|n| (1..=2).contains(&n)));
        for timing in ["scene", "render", "hash", "png", "total"] {
            let line = field(&stdout, &format!("{timing}-seconds"));
            let seconds: f64 = line.split(' ').next().expect("value").parse().expect("number");
            assert!(seconds >= 0.0, "{timing}: {line}");
        }
        rays.push(field(&stdout, "rays").to_owned());
        hashes.push(field(&stdout, "image").to_owned());
    }
    let (file_a, file_b) = (std::fs::read(&a).expect("a"), std::fs::read(&b).expect("b"));
    assert_eq!(file_a, file_b);
    assert_eq!(hashes[0], hashes[1]);
    // The printed hash, ray count and file are those of the library's render of the same size
    // and settings.
    let render =
        render_reference(40, 24, RenderSettings::new(2, 5).expect("valid")).expect("renders");
    let image = render.image;
    assert_eq!(hashes[0], image.hash());
    let r = render.rays;
    assert_eq!(rays[0], format!("{} (closest {}, shadow {})", r.total(), r.closest, r.shadow));
    assert_eq!(rays[0], rays[1]);
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
    assert_refused(
        &lumen(&["render", "--threads", "0"]),
        2,
        "--threads: threads must be at least 1",
    );
    assert_refused(
        &lumen(&["render", "--threads", "257"]),
        2,
        "--threads: threads must be at most 256, not 257",
    );
    assert_refused(&lumen(&["render", "--tile", "0x4"]), 2, "--tile: tile width and height");
    assert_refused(&lumen(&["render", "--tile", "4by4"]), 2, "--tile needs N or WxH");
    assert_refused(&lumen(&["render", "--out"]), 2, "--out needs a value");
    assert_refused(&lumen(&["render", "--out", "x.png", "--spp", "4"]), 2, "unknown option");
    assert_refused(
        &lumen(&["render", "--width", "wide", "--out", "x.png"]),
        2,
        "--width needs a whole number",
    );
    assert_refused(
        &lumen(&["render", "--height", "-3", "--out", "x.png"]),
        2,
        "--height needs a whole number",
    );
    assert_refused(
        &lumen(&["render", "--samples", "3", "--out", "x.png"]),
        2,
        "samples per pixel must be a power of two",
    );
    assert_refused(
        &lumen(&["render", "--samples", "2048", "--out", "x.png"]),
        2,
        "samples per pixel must be at most 1024",
    );
    assert_refused(
        &lumen(&["render", "--seed", "4294967296", "--out", "x.png"]),
        2,
        "--seed needs a whole number",
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

/// A non-Unicode output path (possible on Unix) is written to, with its bytes kept; a
/// non-Unicode number is refused with a usage error. Neither panics.
#[cfg(unix)]
#[test]
fn non_unicode_arguments_never_panic() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let dir = scratch("non-unicode");
    let name = OsStr::from_bytes(b"lit-\xff\xfe.png");
    let out = dir.join(name);
    let output = Command::new(env!("CARGO_BIN_EXE_lumen"))
        .args([OsStr::new("render"), OsStr::new("--width"), OsStr::new("4")])
        .args([OsStr::new("--height"), OsStr::new("3"), OsStr::new("--out"), out.as_os_str()])
        .output()
        .expect("lumen runs");
    let stderr = text(&output.stderr);
    assert!(!stderr.contains("panicked"), "{stderr}");
    // Linux file names are bytes, so the file is written. Other Unix file systems (APFS on
    // macOS) may refuse the name, which must then be a normal I/O error, not a panic.
    if cfg!(target_os = "linux") || output.status.success() {
        assert!(output.status.success(), "{stderr}");
        let written = std::fs::read(&out).expect("the file exists under its non-Unicode name");
        let mut expected = Vec::new();
        let image = render_reference(4, 3, RenderSettings::new(4, 0).expect("valid"))
            .expect("renders")
            .image;
        write_png(&mut expected, &image).expect("writes to memory");
        assert_eq!(written, expected);
    } else {
        assert_refused(&output, 1, "lit-");
    }
    let output = Command::new(env!("CARGO_BIN_EXE_lumen"))
        .args([OsStr::new("render"), OsStr::new("--samples"), name])
        .args([OsStr::new("--out"), OsStr::new("x.png")])
        .output()
        .expect("lumen runs");
    assert_refused(&output, 2, "--samples needs a whole number, not the non-Unicode");
}

/// Without `--out` nothing is written and `png none` is printed; the hash and ray count are the
/// same at every thread count and tile size, and the report names the threads and the tile.
#[test]
fn the_hash_is_the_same_at_every_thread_count_without_a_file() {
    let dir = scratch("threads");
    let mut seen = Vec::new();
    for (threads, tile) in [("1", "32"), ("3", "5x3"), ("8", "1"), ("2", "40x1")] {
        let output = Command::new(env!("CARGO_BIN_EXE_lumen"))
            .current_dir(&dir)
            .args(["render", "--width", "40", "--height", "24", "--samples", "2", "--seed", "5"])
            .args(["--threads", threads, "--tile", tile])
            .output()
            .expect("lumen runs");
        assert!(output.status.success(), "{}", text(&output.stderr));
        let stdout = text(&output.stdout);
        assert_eq!(field(&stdout, "threads-asked"), threads);
        assert_eq!(field(&stdout, "threads"), threads);
        let want_tile = if tile.contains('x') { tile.to_owned() } else { format!("{tile}x{tile}") };
        assert_eq!(field(&stdout, "tile"), want_tile);
        assert_eq!(field(&stdout, "png"), "none");
        seen.push((field(&stdout, "image").to_owned(), field(&stdout, "rays").to_owned()));
    }
    assert!(std::fs::read_dir(&dir).expect("dir").next().is_none(), "a file was written");
    let render =
        render_reference(40, 24, RenderSettings::new(2, 5).expect("valid")).expect("renders");
    assert!(seen.iter().all(|(hash, _)| *hash == render.image.hash()), "{seen:?}");
    assert!(seen.iter().all(|(_, rays)| *rays == seen[0].1), "{seen:?}");
}
