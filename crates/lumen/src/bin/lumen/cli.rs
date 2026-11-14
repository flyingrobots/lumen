//! Command-line arguments.
//!
//! Arguments are read as `OsString`s (`std::env::args_os`), so an argument that is not valid
//! Unicode never panics: an output path keeps its bytes, and a non-Unicode option name or number
//! is a [`UsageError`].

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use lumen::{BandRows, BuiltinScene, RenderSettings, Schedule, DEFAULT_BAND_ROWS};

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
                    [--threads T] [--band-rows R]
       lumen --help | --version

Renders one of lumen's built-in scenes (reference, the default, or fixed-point) on T threads,
each taking bands of R whole rows, with N samples per pixel of a diffuse path (a shadow-casting
sun, a uniform sky and one bounce), writes it to a PNG file if --out is given, and prints the
image hash (SHA-256 of the raw RGBA8 bytes) and the timing. The image is the same for every
thread count and band height.
Scenes (--scene NAME): reference, fixed-point (the Stanford bunny in sunglasses at a craps table).
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
    },
}

/// The options `render` takes.
const OPTIONS: [&str; 8] =
    ["--width", "--height", "--samples", "--seed", "--threads", "--band-rows", "--out", "--scene"];

/// Parses the arguments after the program name.
///
/// # Errors
///
/// A [`UsageError`] for anything but `render` with known options, `--help`/`-h`/`help`, or
/// `--version`/`-V`.
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
    while let Some(option) = args.next() {
        let name = match option.to_str() {
            Some("--help" | "-h") => return Ok(Command::Help),
            Some(name) => OPTIONS.iter().copied().find(|known| *known == name),
            None => None,
        };
        let Some(name) = name else {
            return Err(UsageError::UnknownOption(option.to_string_lossy().into_owned()));
        };
        let value = args.next().ok_or(UsageError::MissingValue(name))?;
        let repeated = match OPTIONS.iter().position(|known| *known == name) {
            Some(slot @ 0..=5) => numbers[slot].replace(number(name, &value)?).is_some(),
            Some(7) => scene.replace(scene_name(&value)?).is_some(),
            _ => out.replace(PathBuf::from(value)).is_some(),
        };
        if repeated {
            return Err(UsageError::Repeated(name));
        }
    }
    let [width, height, samples, seed, threads, band_rows] = numbers;
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
    })
}

/// A scene name: one of `BuiltinScene::ALL`, exactly as `BuiltinScene::name` gives it.
fn scene_name(value: &OsStr) -> Result<BuiltinScene, UsageError> {
    // A name that is not Unicode matches no scene, so the lossy text is only for the message.
    value.to_string_lossy().parse().map_err(UsageError::Scene)
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
