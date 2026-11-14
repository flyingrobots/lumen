//! Command-line arguments.
//!
//! Arguments are read as `OsString`s (`std::env::args_os`), so an argument that is not valid
//! Unicode never panics: an output path keeps its bytes, and a non-Unicode option name or number
//! is a [`UsageError`].

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use lumen::{BandRows, BuiltinScene, RenderSettings, Schedule, View, DEFAULT_BAND_ROWS};

mod usage_error;

pub(crate) use usage_error::UsageError;

/// The default image width: small enough to render in a few seconds in a debug build.
pub(crate) const DEFAULT_WIDTH: u32 = 320;
/// The default image height.
pub(crate) const DEFAULT_HEIGHT: u32 = 180;
/// The default samples per pixel.
pub(crate) const DEFAULT_SAMPLES: u32 = 4;
/// The default seed.
pub(crate) const DEFAULT_SEED: u32 = 0;

/// The usage text.
pub(crate) const USAGE: &str = "\
usage: lumen render [--scene NAME] [--out FILE] [--width W] [--height H] [--samples N] [--seed S]
                    [--threads T] [--band-rows R] [--view NAME] [--stats]
       lumen stats [--scene NAME]
       lumen --help | --version

Renders one of lumen's built-in scenes (reference, the default, fixed-point or canyon) on T threads,
each taking bands of R whole rows, with N samples per pixel of a diffuse path (a shadow-casting
sun, a uniform sky and one bounce), writes it to a PNG file if --out is given, and prints the
image hash (SHA-256 of the raw RGBA8 bytes) and the timing. The image is the same for every
thread count and band height. --stats adds the scene's statistics and what the rays found
after the report (see the README, Statistics). `lumen stats` builds the scene, renders nothing
and prints what it holds: triangles by material, bounds, lights, camera, the shape of its tree
and its memory.
--view NAME renders a debug view instead of the lit image: lit (the default), normals (the
first hit's geometric normal as colour, back faces dimmer) or shading-normals (the interpolated
vertex normal where a triangle has them); a debug view is one ray through each pixel's centre, so
it takes --samples 1 only (the default for it), and --stats is for the lit image.
Scenes (--scene NAME): reference, fixed-point (the Stanford bunny in sunglasses at a craps table),
canyon (craft flying along a fractal canyon under a banded planet).
Defaults: --scene reference --width 320 --height 180 --samples 4 --seed 0 --band-rows 1, and as
many threads as the system reports available.
Width and height are 1..=32768 each, with at most 134217728 (2^27) pixels in all.
Samples per pixel are a power of two from 1 to 1024; the seed is 0..=4294967295.
Threads are 1..=256; band rows are at least 1.";

/// A parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// Print the usage text.
    Help,
    /// Print the version.
    Version,
    /// Print a built-in scene's statistics, without rendering.
    Stats {
        /// The scene to describe.
        scene: BuiltinScene,
    },
    /// Render a built-in scene.
    Render {
        /// The scene to render.
        scene: BuiltinScene,
        /// Image width in pixels (unchecked here; the renderer checks the size).
        width: u32,
        /// Image height in pixels.
        height: u32,
        /// The samples per pixel and the seed.
        settings: RenderSettings,
        /// The worker threads asked for (checked against `1..=MAX_THREADS`), or `None` for as
        /// many as the system reports available.
        threads: Option<u32>,
        /// The rows in a band, the unit of work a thread takes.
        band: BandRows,
        /// The PNG file to write, with the bytes it was given, if any.
        out: Option<PathBuf>,
        /// Whether the report ends with the statistics.
        detail: Detail,
        /// What to draw: the lit image or a debug view.
        view: View,
    },
}

/// How much `render` reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Detail {
    /// The report alone, as before statistics existed.
    Report,
    /// The report, then the scene and ray statistics (`--stats`).
    Statistics,
}

/// The flag that asks for [`Detail::Statistics`].
const STATS: &str = "--stats";

/// The options `render` takes.
const OPTIONS: [&str; 9] = [
    "--width",
    "--height",
    "--samples",
    "--seed",
    "--threads",
    "--band-rows",
    "--out",
    "--scene",
    "--view",
];

/// Parses the arguments after the program name.
///
/// # Errors
///
/// A [`UsageError`] for anything but `render` with known options, `stats` with `--scene`,
/// `--help`/`-h`/`help`, or `--version`/`-V`.
pub(crate) fn parse<I, S>(args: I) -> Result<Command, UsageError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut args = args.into_iter().map(Into::into);
    let command = args.next().ok_or(UsageError::MissingCommand)?;
    match command.to_str() {
        Some("--help" | "-h" | "help") => return Ok(Command::Help),
        Some("--version" | "-V") => return Ok(Command::Version),
        Some("render") => {}
        Some("stats") => return parse_stats(args),
        _ => {
            let shown = command.to_string_lossy().into_owned();
            return Err(if shown.starts_with('-') {
                UsageError::UnknownOption(shown)
            } else {
                UsageError::UnknownCommand(shown)
            });
        }
    }
    let mut numbers: [Option<u32>; 6] = [None; 6];
    let mut out: Option<PathBuf> = None;
    let mut scene: Option<BuiltinScene> = None;
    let mut view: Option<View> = None;
    let mut detail = Detail::Report;
    while let Some(option) = args.next() {
        let (name, inline) = match option.to_str() {
            Some("--help" | "-h") => return Ok(Command::Help),
            Some(STATS) if detail == Detail::Report => {
                detail = Detail::Statistics;
                continue;
            }
            Some(STATS) => return Err(UsageError::Repeated(STATS)),
            Some(text) => {
                // `--name=value` carries its value in the same argument, whatever it looks like.
                let (name, inline) = text.split_once('=').unzip();
                (OPTIONS.iter().copied().find(|known| *known == name.unwrap_or(text)), inline)
            }
            None => (None, None),
        };
        let Some(name) = name else {
            return Err(UsageError::UnknownOption(option.to_string_lossy().into_owned()));
        };
        let value = if let Some(inline) = inline {
            OsString::from(inline)
        } else {
            separate_value(&mut args, name)?
        };
        let repeated = match OPTIONS.iter().position(|known| *known == name) {
            Some(slot @ 0..=5) => numbers[slot].replace(number(name, &value)?).is_some(),
            Some(7) => scene.replace(scene_name(&value)?).is_some(),
            Some(8) => view.replace(view_name(&value)?).is_some(),
            _ => out.replace(PathBuf::from(value)).is_some(),
        };
        if repeated {
            return Err(UsageError::Repeated(name));
        }
    }
    let [width, height, samples, seed, threads, band_rows] = numbers;
    let view = view.unwrap_or(View::Lit);
    let samples = debug_view_samples(view, samples, detail)?;
    let settings =
        RenderSettings::new(samples.unwrap_or(DEFAULT_SAMPLES), seed.unwrap_or(DEFAULT_SEED))
            .map_err(UsageError::Samples)?;
    let band =
        band_rows.map_or(Ok(DEFAULT_BAND_ROWS), BandRows::new).map_err(UsageError::Schedule)?;
    if let Some(threads) = threads {
        Schedule::new(threads, band).map_err(UsageError::Schedule)?;
    }
    Ok(Command::Render {
        scene: scene.unwrap_or(BuiltinScene::Reference),
        width: width.unwrap_or(DEFAULT_WIDTH),
        height: height.unwrap_or(DEFAULT_HEIGHT),
        settings,
        threads,
        band,
        out,
        detail,
        view,
    })
}

/// The samples of a debug view: one ray per pixel, so `--samples` may only be 1, its default.
/// `--stats` reports on the lit image's paths and is refused with a debug view.
fn debug_view_samples(
    view: View,
    samples: Option<u32>,
    detail: Detail,
) -> Result<Option<u32>, UsageError> {
    if !view.is_single_sample() {
        return Ok(samples);
    }
    if detail == Detail::Statistics {
        return Err(UsageError::ViewStatistics(view));
    }
    match samples {
        None | Some(1) => Ok(Some(1)),
        Some(n) => Err(UsageError::ViewSamples { view, samples: n }),
    }
}

/// The arguments of `stats`: at most one `--scene NAME` (or `--scene=NAME`), or `--help`.
fn parse_stats(mut args: impl Iterator<Item = OsString>) -> Result<Command, UsageError> {
    const SCENE: &str = "--scene";
    let mut scene: Option<BuiltinScene> = None;
    while let Some(option) = args.next() {
        let text = option.to_str();
        let value = match (text, text.and_then(|t| t.strip_prefix("--scene="))) {
            (Some("--help" | "-h"), _) => return Ok(Command::Help),
            (Some(SCENE), _) => separate_value(&mut args, SCENE)?,
            (_, Some(inline)) => OsString::from(inline),
            _ => return Err(UsageError::UnknownOption(option.to_string_lossy().into_owned())),
        };
        if scene.replace(scene_name(&value)?).is_some() {
            return Err(UsageError::Repeated(SCENE));
        }
    }
    Ok(Command::Stats { scene: scene.unwrap_or(BuiltinScene::Reference) })
}

/// The next argument as the value of `option`: missing, or reading as an option, is refused.
fn separate_value(
    args: &mut impl Iterator<Item = OsString>,
    option: &'static str,
) -> Result<OsString, UsageError> {
    let value = args.next().ok_or(UsageError::MissingValue(option))?;
    if looks_like_option(&value) {
        return Err(UsageError::OptionLikeValue {
            option,
            value: value.to_string_lossy().into_owned(),
        });
    }
    Ok(value)
}

/// Whether a value given as a separate argument reads as an option: it starts with `-`, has more
/// after it, and is not a negative number (`-3` is a value, refused as a number where a number
/// is needed). A lone `-` is a value. This is the rule of `clap` and `argparse`: to use such a
/// value, write `--option=value`. A value that is not Unicode is never an option.
fn looks_like_option(value: &OsStr) -> bool {
    let Some(text) = value.to_str() else {
        return false;
    };
    let mut chars = text.chars();
    chars.next() == Some('-') && chars.next().is_some_and(|c| !c.is_ascii_digit())
}

/// A scene name: one of `BuiltinScene::ALL`, exactly as `BuiltinScene::name` gives it.
fn scene_name(value: &OsStr) -> Result<BuiltinScene, UsageError> {
    // A name that is not Unicode matches no scene, so the lossy text is only for the message.
    value.to_string_lossy().parse().map_err(UsageError::Scene)
}

/// A view name: one of `View::ALL`, exactly as `View::name` gives it.
fn view_name(value: &OsStr) -> Result<View, UsageError> {
    value.to_string_lossy().parse().map_err(UsageError::View)
}

/// A decimal `u32`: digits only, no sign, no spaces, no underscores.
fn number(option: &'static str, value: &OsStr) -> Result<u32, UsageError> {
    let Some(text) = value.to_str() else {
        return Err(UsageError::NotUnicode { option, value: value.to_string_lossy().into_owned() });
    };
    let digits = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    digits
        .then(|| text.parse::<u32>().ok())
        .flatten()
        .ok_or_else(|| UsageError::InvalidNumber { option, value: text.to_owned() })
}

#[cfg(test)]
mod tests;
