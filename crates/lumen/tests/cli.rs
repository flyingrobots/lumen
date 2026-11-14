//! Size class: medium (spawns the `lumen` process and writes files under `CARGO_TARGET_TMPDIR`).
//!
//! The `lumen` command, run as a separate process. Every test owns a scratch directory named
//! for the test and the process, runs the command with it as the working directory and gives
//! absolute paths, so a refusal that regressed into writing a file would write there, not into
//! the source tree, and concurrent runs (test and release profiles) cannot meet.

#![expect(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::disallowed_macros,
    reason = "std-fs, std-process, std-env, float-type: runs the command in a scratch directory and \
              reads its files and timing lines; a test outside the deterministic core"
)]

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use lumen::{render_reference, write_png, BuiltinScene, RenderSettings};

mod golden;
use golden::{
    GOLDEN_96X54_4SPP, GOLDEN_FIXED_POINT_96X54_4SPP, GOLDEN_FIXED_POINT_CONTENT,
    GOLDEN_REFERENCE_CONTENT,
};

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("utf-8")
}

/// A fresh directory for one test's files, removed when dropped. Its name holds the process id
/// and the test's name, so no other test or run shares it.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("cli-{}-{name}", std::process::id()));
        assert!(dir.is_absolute(), "{}", dir.display());
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch directory");
        Self(dir)
    }

    /// An absolute path of a file in the directory.
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    /// The command with this directory as its working directory.
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_lumen"));
        command.current_dir(&self.0);
        command
    }

    fn lumen(&self, args: &[&str]) -> Output {
        self.command().args(args).output().expect("lumen runs")
    }

    fn is_empty(&self) -> bool {
        std::fs::read_dir(&self.0).expect("dir").next().is_none()
    }
}

impl Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("utf-8 path")
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

/// The arguments of a small render, the report's fields and the file it writes.
fn small_render<'a>(out: &'a str, extra: &[&'a str]) -> Vec<&'a str> {
    let mut args =
        vec!["render", "--width", "40", "--height", "24", "--samples", "2", "--seed", "5"];
    args.extend_from_slice(extra);
    args.extend_from_slice(&["--out", out]);
    args
}

#[test]
fn the_report_names_the_scene_the_settings_and_the_work() {
    let dir = Scratch::new("report");
    let out = dir.file("a.png");
    let output = dir.lumen(&small_render(path_str(&out), &[]));
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
    assert_eq!(field(&stdout, "band-rows"), "1");
    // As many workers as asked for, but never more than the 24 one-row bands.
    let asked: u32 = field(&stdout, "threads-asked").parse().expect("a count");
    assert_eq!(field(&stdout, "threads").parse::<u32>(), Ok(asked.min(24)));
}

#[test]
fn the_report_times_every_stage() {
    let dir = Scratch::new("timings");
    let out = dir.file("a.png");
    let output = dir.lumen(&[
        "render",
        "--width",
        "8",
        "--height",
        "4",
        "--samples",
        "1",
        "--out",
        path_str(&out),
    ]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    for timing in ["scene", "render", "hash", "png", "total"] {
        let line = field(&stdout, &format!("{timing}-seconds"));
        let seconds: f64 = line.split(' ').next().expect("value").parse().expect("number");
        assert!(seconds >= 0.0, "{timing}: {line}");
    }
}

#[test]
fn two_processes_write_the_same_file_and_print_the_same_hash() {
    let dir = Scratch::new("determinism");
    let (a, b) = (dir.file("a.png"), dir.file("b.png"));
    let outputs: Vec<String> = [&a, &b]
        .map(|path| {
            let output = dir.lumen(&small_render(path_str(path), &[]));
            assert!(output.status.success(), "{}", text(&output.stderr));
            text(&output.stdout)
        })
        .into();
    assert_eq!(std::fs::read(&a).expect("a"), std::fs::read(&b).expect("b"));
    assert_eq!(field(&outputs[0], "image"), field(&outputs[1], "image"));
    assert_eq!(field(&outputs[0], "rays"), field(&outputs[1], "rays"));
}

#[test]
fn the_printed_hash_ray_count_and_file_are_the_librarys() {
    let dir = Scratch::new("library-agrees");
    let out = dir.file("a.png");
    let output = dir.lumen(&small_render(path_str(&out), &[]));
    assert!(output.status.success(), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    let render =
        render_reference(40, 24, RenderSettings::new(2, 5).expect("valid")).expect("renders");
    assert_eq!(field(&stdout, "image"), render.image.hash());
    let r = render.rays;
    assert_eq!(
        field(&stdout, "rays"),
        format!("{} (closest {}, shadow {})", r.total(), r.closest, r.shadow)
    );
    let mut expected = Vec::new();
    write_png(&mut expected, &render.image).expect("writes to memory");
    assert_eq!(std::fs::read(&out).expect("the file"), expected);
}

#[test]
fn help_prints_the_usage() {
    let dir = Scratch::new("help");
    let help = dir.lumen(&["--help"]);
    assert!(help.status.success());
    assert!(text(&help.stdout).starts_with("usage: lumen render"));
}

#[test]
fn version_prints_the_package_version() {
    let dir = Scratch::new("version");
    let version = dir.lumen(&["--version"]);
    assert!(version.status.success());
    assert!(text(&version.stdout).starts_with(&format!("lumen {}", env!("CARGO_PKG_VERSION"))));
}

/// Each refusal exits with code 2 and its message, prints no panic and no report, and leaves
/// the scratch directory empty, whether the output path is relative or absolute.
#[test]
fn bad_arguments_are_refused_with_usage_and_write_nothing() {
    let dir = Scratch::new("refusals");
    let out = dir.file("x.png");
    let out = path_str(&out);
    let cases: [(&[&str], &str); 18] = [
        (&[], "no command given"),
        (&["paint"], "unknown command \"paint\""),
        (&["render", "--threads", "0"], "--threads: threads must be at least 1"),
        (&["render", "--threads", "257"], "--threads: threads must be at most 256, not 257"),
        (&["render", "--band-rows", "0"], "--band-rows: a band must be"),
        (&["render", "--band-rows", "4x4"], "--band-rows needs a whole"),
        (&["render", "--tile", "32"], "unknown option \"--tile\""),
        (&["render", "--out"], "--out needs a value"),
        (&["render", "--out", "x.png", "--spp", "4"], "unknown option"),
        (&["render", "--width", "wide", "--out", out], "--width needs a whole number"),
        (&["render", "--height", "-3", "--out", out], "--height needs a whole number"),
        (&["render", "--samples", "3", "--out", out], "samples per pixel must be a power of two"),
        (&["render", "--samples", "2048", "--out", out], "samples per pixel must be at most 1024"),
        (&["render", "--seed", "4294967296", "--out", out], "--seed needs a whole number"),
        (&["render", "--out", "a", "--out", "b"], "--out given more than once"),
        (&["render", "--out", out, "--out", out], "--out given more than once"),
        (&["render", "--width", "wide", "--out", "x.png"], "--width needs a whole number"),
        (
            &["render", "--samples", "3", "--out", "x.png"],
            "samples per pixel must be a power of two",
        ),
    ];
    for (args, message) in cases {
        assert_refused(&dir.lumen(args), 2, message);
        assert!(dir.is_empty(), "{args:?} wrote a file");
    }
}

/// A refused size is a refused argument value: exit code 2, as for samples, threads and band rows.
#[test]
fn bad_sizes_are_refused_before_anything_is_written() {
    let dir = Scratch::new("sizes");
    let out = dir.file("x.png");
    let cases: [(&str, &str, &str); 4] = [
        ("0", "10", "at least 1"),
        ("10", "0", "at least 1"),
        ("32769", "10", "at most 32768"),
        ("32768", "32768", "at most 134217728 pixels"),
    ];
    for (width, height, message) in cases {
        let output =
            dir.lumen(&["render", "--width", width, "--height", height, "--out", path_str(&out)]);
        assert_refused(&output, 2, message);
        assert!(!out.exists(), "{width}x{height} wrote a file");
    }
}

/// The report's lines that do not depend on the clock.
fn stable_lines(stdout: &str) -> Vec<&str> {
    stdout.lines().filter(|line| !line.contains("information only")).collect()
}

/// A value that reads as an option is refused for every option that takes a value, writes
/// nothing, and exits 2; `--option=value` and negative numbers are values.
#[test]
fn an_option_looking_value_is_refused_unless_written_with_an_equals_sign() {
    let dir = Scratch::new("option-like-value");
    let cases: [(&[&str], &str); 8] = [
        (&["render", "--out", "--width"], "--out needs a value, not the option \"--width\""),
        (&["render", "--out", "--width", "8"], "--out needs a value, not the option \"--width\""),
        (&["render", "--width", "--height", "5"], "--width needs a value, not the option"),
        (&["render", "--samples", "--out", "x.png"], "--samples needs a value, not the option"),
        (&["render", "--seed", "-h"], "--seed needs a value, not the option \"-h\""),
        (&["render", "--threads", "--seed=1"], "--threads needs a value, not the option"),
        (&["render", "--band-rows", "--scene"], "--band-rows needs a value, not the option"),
        (&["render", "--scene", "--out=x.png"], "--scene needs a value, not the option"),
    ];
    for (args, message) in cases {
        assert_refused(&dir.lumen(args), 2, message);
        assert!(dir.is_empty(), "{args:?} wrote a file");
    }
    // The message says how to write the value; written so, it is the value.
    let refused = dir.lumen(&["render", "--out", "--width"]);
    assert!(text(&refused.stderr).contains("write --out=--width"), "{}", text(&refused.stderr));
    let output = dir.lumen(&["render", "--width=8", "--height=4", "--samples=1", "--out=--width"]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(field(&text(&output.stdout), "png").split(' ').next(), Some("--width"));
    assert!(dir.file("--width").is_file());
    // A negative number is a value, so a number option refuses it as a number.
    assert_refused(&dir.lumen(&["render", "--height", "-3"]), 2, "--height needs a whole number");
}

/// Leading zeros are accepted for every numeric option and mean the same decimal number.
#[test]
fn leading_zeros_mean_the_same_number_for_every_numeric_option() {
    let dir = Scratch::new("leading-zeros");
    let plain = dir.lumen(&[
        "render",
        "--width",
        "24",
        "--height",
        "16",
        "--samples",
        "2",
        "--seed",
        "7",
        "--threads",
        "2",
        "--band-rows",
        "3",
    ]);
    let zeros = dir.lumen(&[
        "render",
        "--width",
        "000024",
        "--height",
        "016",
        "--samples",
        "0002",
        "--seed",
        "0007",
        "--threads",
        "02",
        "--band-rows",
        "003",
    ]);
    assert!(plain.status.success() && zeros.status.success());
    let (plain, zeros) = (text(&plain.stdout), text(&zeros.stdout));
    assert_eq!(stable_lines(&plain), stable_lines(&zeros));
    assert_eq!(field(&zeros, "size"), "24x16");
    // The padded zero is still zero, and a padded refused value is refused.
    assert_refused(&dir.lumen(&["render", "--threads", "000"]), 2, "threads must be at least 1");
    assert_refused(&dir.lumen(&["render", "--samples", "0003"]), 2, "must be a power of two");
}

/// The report's byte count is what the file holds.
#[test]
fn the_reported_byte_count_is_the_files_length() {
    let dir = Scratch::new("byte-count");
    let out = dir.file("a.png");
    let output = dir.lumen(&small_render(path_str(&out), &[]));
    assert!(output.status.success(), "{}", text(&output.stderr));
    let png = field(&text(&output.stdout), "png").to_owned();
    let length = std::fs::metadata(&out).expect("the file").len();
    assert_eq!(png, format!("{} ({length} bytes)", out.display()));
}

#[test]
fn a_path_in_a_missing_directory_is_reported_by_name() {
    let dir = Scratch::new("missing-parent");
    let missing_parent = dir.file("no-such-directory").join("x.png");
    let output =
        dir.lumen(&["render", "--width", "4", "--height", "3", "--out", path_str(&missing_parent)]);
    assert_refused(&output, 1, "no-such-directory");
}

#[test]
fn a_directory_is_not_a_file_to_write() {
    let dir = Scratch::new("directory-as-file");
    let output = dir.lumen(&["render", "--width", "4", "--height", "3", "--out", path_str(&dir)]);
    assert_refused(&output, 1, path_str(&dir));
}

/// A render that would run for hours if it started: 2^27 pixels at 1024 samples.
const HOURS_OF_RENDERING: [&str; 7] =
    ["render", "--width", "32768", "--height", "4096", "--samples", "1024"];

/// Runs the command and fails the test if it has not exited within a minute: a refusal takes
/// milliseconds, so a command that is still running has started to render. The child is killed
/// by its own handle.
fn lumen_that_must_refuse_quickly(dir: &Scratch, extra: &[&str]) -> Output {
    let mut child = dir
        .command()
        .args(HOURS_OF_RENDERING)
        .args(extra)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("lumen starts");
    let started = Instant::now();
    while child.try_wait().expect("polls").is_none() {
        if started.elapsed() > Duration::from_mins(1) {
            child.kill().expect("kills its own child");
            child.wait().expect("reaps");
            panic!("{extra:?}: still running after a minute: it started to render");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    child.wait_with_output().expect("output")
}

/// An unwritable destination is refused before the render starts (the render here would take
/// hours), with the same message and exit code as before, and nothing is left behind.
#[test]
fn an_unwritable_destination_is_refused_before_any_pixel_is_computed() {
    let dir = Scratch::new("refused-before-render");
    let readonly = dir.file("readonly.png");
    std::fs::write(&readonly, b"keep me").expect("old file");
    let mut permissions = std::fs::metadata(&readonly).expect("meta").permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&readonly, permissions).expect("read-only");
    let subdirectory = dir.file("sub");
    std::fs::create_dir(&subdirectory).expect("sub");
    let missing = dir.file("no-such-directory").join("x.png");
    let cases = [
        (missing.as_path(), "no-such-directory"),
        (subdirectory.as_path(), "sub: is a directory"),
        (readonly.as_path(), "readonly.png: the file is read-only"),
    ];
    for (destination, message) in cases {
        let output = lumen_that_must_refuse_quickly(&dir, &["--out", path_str(destination)]);
        assert_refused(&output, 1, message);
    }
    assert_eq!(std::fs::read(&readonly).expect("file"), b"keep me");
    let mut names: Vec<_> = std::fs::read_dir(&*dir)
        .expect("dir")
        .map(|entry| entry.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["readonly.png", "sub"]);
}

/// A render that replaces a file leaves the complete PNG and no temporary file next to it.
#[test]
fn a_render_replaces_an_existing_file_and_leaves_no_temporary_file() {
    let dir = Scratch::new("replace");
    let out = dir.file("a.png");
    std::fs::write(&out, b"the old file").expect("old file");
    let output = dir.lumen(&small_render(path_str(&out), &[]));
    assert!(output.status.success(), "{}", text(&output.stderr));
    let render =
        render_reference(40, 24, RenderSettings::new(2, 5).expect("valid")).expect("renders");
    let mut expected = Vec::new();
    write_png(&mut expected, &render.image).expect("writes to memory");
    assert_eq!(std::fs::read(&out).expect("the file"), expected);
    let names: Vec<_> = std::fs::read_dir(&*dir)
        .expect("dir")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(names, [std::ffi::OsString::from("a.png")]);
}

/// Arguments that are not Unicode, possible on Unix only.
#[cfg(unix)]
mod non_unicode {
    use super::*;
    use std::ffi::OsStr;

    fn non_unicode_name() -> &'static OsStr {
        use std::os::unix::ffi::OsStrExt;
        OsStr::from_bytes(b"lit-\xff\xfe.png")
    }

    /// A non-Unicode number is refused with a usage error, not a panic.
    #[test]
    fn a_non_unicode_number_is_refused_with_a_usage_error() {
        let dir = Scratch::new("non-unicode-number");
        let output = dir
            .command()
            .args([OsStr::new("render"), OsStr::new("--samples"), non_unicode_name()])
            .args([OsStr::new("--out"), dir.file("x.png").as_os_str()])
            .output()
            .expect("lumen runs");
        assert_refused(&output, 2, "--samples needs a whole number, not the non-Unicode");
        assert!(dir.is_empty());
    }

    fn render_to(dir: &Scratch, out: &Path) -> Output {
        dir.command()
            .args([OsStr::new("render"), OsStr::new("--width"), OsStr::new("4")])
            .args([OsStr::new("--height"), OsStr::new("3"), OsStr::new("--out"), out.as_os_str()])
            .output()
            .expect("lumen runs")
    }

    /// Linux file names are bytes, so a non-Unicode output path is written with its bytes kept.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_non_unicode_output_path_is_written_with_its_bytes_kept() {
        let dir = Scratch::new("non-unicode-path");
        let out = dir.file("x").with_file_name(non_unicode_name());
        let output = render_to(&dir, &out);
        assert!(output.status.success(), "{}", text(&output.stderr));
        let written = std::fs::read(&out).expect("the file exists under its non-Unicode name");
        let image = render_reference(4, 3, RenderSettings::new(4, 0).expect("valid"))
            .expect("renders")
            .image;
        let mut expected = Vec::new();
        write_png(&mut expected, &image).expect("writes to memory");
        assert_eq!(written, expected);
    }

    /// Other Unix file systems (APFS on macOS) may refuse the name; that must be a normal I/O
    /// error (exit 1), never a panic or a usage error.
    #[cfg(all(unix, not(target_os = "linux")))]
    #[test]
    fn a_non_unicode_output_path_never_panics() {
        let dir = Scratch::new("non-unicode-path");
        let out = dir.file("x").with_file_name(non_unicode_name());
        let output = render_to(&dir, &out);
        let stderr = text(&output.stderr);
        assert!(!stderr.contains("panicked"), "{stderr}");
        assert!(matches!(output.status.code(), Some(0 | 1)), "{stderr}");
    }
}

#[test]
fn without_out_no_file_is_written_and_png_is_none() {
    let dir = Scratch::new("no-out");
    let output = dir.lumen(&["render", "--width", "8", "--height", "4", "--samples", "1"]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(field(&text(&output.stdout), "png"), "none");
    assert!(dir.is_empty(), "a file was written");
}

#[test]
fn the_report_names_the_threads_and_the_band_asked_for() {
    let dir = Scratch::new("threads-report");
    for (threads, rows) in [("1", "32"), ("3", "5"), ("8", "1"), ("2", "7")] {
        let output = dir.lumen(&[
            "render",
            "--width",
            "8",
            "--height",
            "32",
            "--samples",
            "1",
            "--threads",
            threads,
            "--band-rows",
            rows,
        ]);
        assert!(output.status.success(), "{}", text(&output.stderr));
        let stdout = text(&output.stdout);
        assert_eq!(field(&stdout, "threads-asked"), threads);
        assert_eq!(field(&stdout, "threads"), threads);
        assert_eq!(field(&stdout, "band-rows"), rows);
    }
}

/// The hash and ray count are the same at every thread count and band height.
#[test]
fn the_hash_and_ray_count_are_the_same_at_every_thread_count_and_band_height() {
    let dir = Scratch::new("threads-hash");
    let mut seen = Vec::new();
    for (threads, rows) in [("1", "32"), ("3", "5"), ("8", "1"), ("2", "7")] {
        let output = dir.lumen(&[
            "render",
            "--width",
            "40",
            "--height",
            "24",
            "--samples",
            "2",
            "--seed",
            "5",
            "--threads",
            threads,
            "--band-rows",
            rows,
        ]);
        assert!(output.status.success(), "{}", text(&output.stderr));
        let stdout = text(&output.stdout);
        seen.push((field(&stdout, "image").to_owned(), field(&stdout, "rays").to_owned()));
    }
    let render =
        render_reference(40, 24, RenderSettings::new(2, 5).expect("valid")).expect("renders");
    assert!(seen.iter().all(|(hash, _)| *hash == render.image.hash()), "{seen:?}");
    assert!(seen.iter().all(|(_, rays)| *rays == seen[0].1), "{seen:?}");
}

fn render_scene(dir: &Scratch, size: [&str; 2], extra: &[&str]) -> String {
    let mut args = vec!["render", "--width", size[0], "--height", size[1], "--samples", "4"];
    args.extend_from_slice(extra);
    let output = dir.lumen(&args);
    assert!(output.status.success(), "{}", text(&output.stderr));
    text(&output.stdout)
}

/// `--scene` names the scene: the reported identity is that scene's.
#[test]
fn each_scene_reports_its_own_identity() {
    let dir = Scratch::new("scene-identity");
    for scene in BuiltinScene::ALL {
        let stdout = render_scene(&dir, ["8", "4"], &["--scene", scene.name()]);
        assert_eq!(field(&stdout, "scene"), scene.id());
    }
}

/// The `scene-content` line (the last provenance line, after `bounces`) is the scene's golden
/// content identity, and the image is the one the library's golden hash records for it (96 by
/// 54 pixels, 4 samples, seed 0).
#[test]
fn each_scene_renders_its_golden_image() {
    let dir = Scratch::new("scene-golden");
    let golden = |scene: BuiltinScene| match scene {
        BuiltinScene::Reference => (GOLDEN_96X54_4SPP, GOLDEN_REFERENCE_CONTENT),
        BuiltinScene::FixedPoint => (GOLDEN_FIXED_POINT_96X54_4SPP, GOLDEN_FIXED_POINT_CONTENT),
    };
    for scene in BuiltinScene::ALL {
        let stdout = render_scene(&dir, ["96", "54"], &["--scene", scene.name()]);
        let (image, content) = golden(scene);
        assert_eq!(field(&stdout, "image"), image, "{}", scene.name());
        let after_bounces = stdout.lines().skip_while(|line| *line != "bounces 1").nth(1);
        assert_eq!(after_bounces, Some(format!("scene-content {content}").as_str()));
    }
}

#[test]
fn without_scene_the_reference_scene_is_rendered() {
    let dir = Scratch::new("default-scene");
    let stdout = render_scene(&dir, ["96", "54"], &[]);
    assert_eq!(field(&stdout, "scene"), BuiltinScene::Reference.id());
    assert_eq!(field(&stdout, "scene-content"), GOLDEN_REFERENCE_CONTENT);
    assert_eq!(field(&stdout, "image"), GOLDEN_96X54_4SPP);
}

#[test]
fn an_unknown_scene_is_a_usage_error_listing_the_scenes() {
    let dir = Scratch::new("unknown-scene");
    let output = dir.lumen(&["render", "--scene", "bunny"]);
    assert_refused(
        &output,
        2,
        "--scene: unknown scene \"bunny\"; the scenes are reference, fixed-point",
    );
    assert!(text(&output.stderr).contains("usage: lumen render"));
}

#[test]
fn help_lists_the_scenes() {
    let dir = Scratch::new("help-scenes");
    let help = text(&dir.lumen(&["--help"]).stdout);
    for scene in BuiltinScene::ALL {
        assert!(help.contains(scene.name()), "{}", scene.name());
    }
}

/// Oracle: `lumen stats` against the library. The header is the scene's name and golden content
/// identity; every other line is `key value...` with a key used once; the counts are those of
/// `Scene::statistics`; the command writes no file.
#[test]
fn stats_prints_the_scenes_statistics_without_rendering() {
    let dir = Scratch::new("stats");
    for scene in BuiltinScene::ALL {
        let output = dir.lumen(&["stats", "--scene", scene.name()]);
        assert!(output.status.success(), "{}", text(&output.stderr));
        let stdout = text(&output.stdout);
        let content = match scene {
            BuiltinScene::Reference => GOLDEN_REFERENCE_CONTENT,
            BuiltinScene::FixedPoint => GOLDEN_FIXED_POINT_CONTENT,
        };
        let header = format!("scene {}\nscene-content {content}\nintegrator ", scene.id());
        assert!(stdout.starts_with(&header), "{stdout}");
        let keys: Vec<&str> =
            stdout.lines().map(|line| line.split_once(' ').expect("key value").0).collect();
        let unique: std::collections::BTreeSet<&str> = keys.iter().copied().collect();
        assert_eq!(unique.len(), keys.len(), "a key repeats in {stdout}");
        let stats = scene.build().expect("builds").statistics().expect("counts");
        assert_eq!(field(&stdout, "scene.triangles"), stats.triangles.to_string());
        assert_eq!(field(&stdout, "tree.nodes"), stats.tree.nodes.to_string());
        let total = format!("{} (information only: this build's type sizes)", stats.memory.total());
        assert_eq!(field(&stdout, "memory.total-bytes"), total);
        assert!(!stdout.contains("image ") && !stdout.contains("seconds"), "{stdout}");
        assert!(dir.is_empty());
    }
}

/// Oracle: the report without `--stats`, from the same command line, and the library. With
/// `--stats` the output is the plain report (timing values aside), then the scene block that
/// `lumen stats` prints, then the ray lines, whose counts are the library's.
#[test]
fn render_stats_appends_the_statistics_after_the_unchanged_report() {
    let dir = Scratch::new("render-stats");
    let args = ["render", "--width", "12", "--height", "6", "--samples", "2", "--threads", "2"];
    let plain = text(&dir.lumen(&args).stdout);
    let output = dir.lumen(&[&args[..], &["--stats"]].concat());
    assert!(output.status.success(), "{}", text(&output.stderr));
    let with = text(&output.stdout);
    // A timing line keeps its key; its value is a wall-clock reading.
    let untimed = |report: &str| -> Vec<String> {
        let mask = |line: &str| match line.split_once(' ') {
            Some((key, rest))
                if key.ends_with("seconds")
                    || key.contains("per-") && rest.contains("information only") =>
            {
                key.to_owned()
            }
            _ => line.to_owned(),
        };
        report.lines().map(mask).collect()
    };
    let (plain, with) = (untimed(&plain), untimed(&with));
    assert_eq!(with[..plain.len()], plain[..], "the report itself is unchanged");
    let stats = text(&dir.lumen(&["stats"]).stdout);
    let scene_block: Vec<&str> = stats.lines().skip(3).collect();
    let after = &with[plain.len()..];
    assert_eq!(after[..scene_block.len()], scene_block[..]);
    let rays = after[scene_block.len()..].join("\n");
    assert!(rays.starts_with("rays.camera 144\n"), "{rays}");
    let scene = lumen::reference_scene().expect("builds");
    let settings = RenderSettings::new(2, 0).expect("settings");
    let size = lumen::ImageSize::new(12, 6).expect("size");
    let render = lumen::render(&scene, size, settings).expect("renders");
    assert!(rays.contains(&format!("\nrays.total {}\n", render.rays.total())), "{rays}");
    let max = format!("\nrays.per-pixel-max {}\n", render.statistics.max_pixel_rays);
    assert!(rays.contains(&max), "{rays}");
}

/// Oracle: the schedule asked for. `render --stats` ends with the band height, the band count,
/// one record per worker, each labelled information only, whose rows sum to the image height.
#[test]
fn render_stats_ends_with_what_each_worker_did() {
    let dir = Scratch::new("render-stats-run");
    let args = ["render", "--width", "8", "--height", "7", "--samples", "1", "--threads", "3"];
    let output = dir.lumen(&[&args[..], &["--band-rows", "2", "--stats"]].concat());
    assert!(output.status.success(), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    assert_eq!(field(&stdout, "run.band-rows"), "2");
    assert_eq!(field(&stdout, "run.bands"), "4");
    assert_eq!(field(&stdout, "run.workers"), "3");
    let note = " (information only: differs from run to run)";
    let rows: u32 = (0..3)
        .map(|n| field(&stdout, &format!("run.worker.{n}.rows")))
        .map(|value| value.strip_suffix(note).expect("labelled").parse::<u32>().expect("a count"))
        .sum();
    assert_eq!(rows, 7);
    assert!(stdout.lines().last().is_some_and(|line| line.starts_with("run.worker.2.rays ")));
}

/// Oracle: the library's counting render of the same scene, size and settings. `render --stats`
/// prints the traversal lines after the ray lines and before the run lines: per kind, each
/// counter's total, mean and most, and the deepest stack, equal to the library's counts.
#[test]
fn render_stats_prints_the_traversal_work_of_each_kind_of_ray() {
    let dir = Scratch::new("render-stats-traversal");
    let args = ["render", "--width", "12", "--height", "6", "--samples", "2", "--threads", "2"];
    let output = dir.lumen(&[&args[..], &["--stats"]].concat());
    assert!(output.status.success(), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    let scene = lumen::reference_scene().expect("builds");
    let (size, settings) =
        (lumen::ImageSize::new(12, 6).expect("size"), RenderSettings::new(2, 0).expect("settings"));
    let schedule = lumen::Schedule::single();
    let (render, _, work) =
        lumen::render_traced_with_traversal(&scene, &scene, size, settings, schedule)
            .expect("renders");
    let camera = work.camera.total;
    assert_eq!(field(&stdout, "traversal.camera.box-tests"), camera.box_tests.to_string());
    assert_eq!(
        field(&stdout, "traversal.camera.box-tests-max"),
        work.camera.most.box_tests.to_string()
    );
    let shadow = work.shadow.total.primitive_tests;
    assert_eq!(field(&stdout, "traversal.shadow.primitive-tests"), shadow.to_string());
    assert_eq!(
        field(&stdout, "traversal.bounce.peak-stack"),
        work.bounce.total.peak_stack.to_string()
    );
    let rays = render.statistics.outcomes.camera();
    let hundredths =
        (u128::from(camera.box_tests) * 200 + u128::from(rays)) / (2 * u128::from(rays));
    let mean = format!("{}.{:02}", hundredths / 100, hundredths % 100);
    assert_eq!(field(&stdout, "traversal.camera.box-tests-mean"), mean);
    let keys: Vec<&str> = stdout.lines().filter_map(|l| l.split(' ').next()).collect();
    let position = |key: &str| keys.iter().position(|k| *k == key).expect(key);
    assert!(position("rays.per-pixel-mean") < position("traversal.camera.box-tests"));
    assert!(position("traversal.shadow.peak-stack") < position("run.band-rows"));
    assert_eq!(keys.iter().filter(|k| k.starts_with("traversal.")).count(), 3 * 19);
}
