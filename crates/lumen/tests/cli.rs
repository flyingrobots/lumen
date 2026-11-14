//! Size class: medium (spawns the `lumen` process and writes files under `CARGO_TARGET_TMPDIR`).
//!
//! The `lumen` command, run as a separate process. Every test owns a scratch directory named
//! for the test and the process, runs the command with it as the working directory and gives
//! absolute paths, so a refusal that regressed into writing a file would write there, not into
//! the source tree, and concurrent runs (test and release profiles) cannot meet.

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use lumen::{render_reference, write_png, BuiltinScene, RenderSettings};

mod golden;
use golden::{GOLDEN_96X54_4SPP, GOLDEN_FIXED_POINT_96X54_4SPP};

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
        assert!(dir.is_absolute(), "{dir:?}");
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
        assert_refused(&output, 1, message);
        assert!(!out.exists(), "{width}x{height} wrote a file");
    }
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

/// The image is the one the library's golden hash records for the scene (96 by 54 pixels, 4
/// samples, seed 0).
#[test]
fn each_scene_renders_its_golden_image() {
    let dir = Scratch::new("scene-golden");
    let golden = |scene: BuiltinScene| match scene {
        BuiltinScene::Reference => GOLDEN_96X54_4SPP,
        BuiltinScene::FixedPoint => GOLDEN_FIXED_POINT_96X54_4SPP,
    };
    for scene in BuiltinScene::ALL {
        let stdout = render_scene(&dir, ["96", "54"], &["--scene", scene.name()]);
        assert_eq!(field(&stdout, "image"), golden(scene), "{}", scene.name());
    }
}

#[test]
fn without_scene_the_reference_scene_is_rendered() {
    let dir = Scratch::new("default-scene");
    let stdout = render_scene(&dir, ["96", "54"], &[]);
    assert_eq!(field(&stdout, "scene"), BuiltinScene::Reference.id());
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
