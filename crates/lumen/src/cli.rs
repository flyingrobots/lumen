//! Command-line arguments.
//!
//! Arguments are read as `OsString`s (`std::env::args_os`), so an argument that is not valid
//! Unicode never panics: an output path keeps its bytes, and a non-Unicode option name or number
//! is a [`UsageError`].

use core::fmt;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use crate::render::{RenderSettings, SettingsError};

/// The default image width: small enough to render in a few seconds in a debug build.
pub const DEFAULT_WIDTH: u32 = 320;
/// The default image height.
pub const DEFAULT_HEIGHT: u32 = 180;
/// The default samples per pixel.
pub const DEFAULT_SAMPLES: u32 = 4;
/// The default seed.
pub const DEFAULT_SEED: u32 = 0;

/// The usage text.
pub const USAGE: &str = "\
usage: lumen render --out FILE [--width W] [--height H] [--samples N] [--seed S]
       lumen --help | --version

Renders lumen's built-in reference scene to a PNG file on one thread, with N samples per
pixel of a diffuse path (a shadow-casting sun, a uniform sky and one bounce), and prints the
image hash (SHA-256 of the raw RGBA8 bytes).
Defaults: --width 320 --height 180 --samples 4 --seed 0.
Width and height are 1..=32768 each, with at most 134217728 (2^27) pixels in all.
Samples per pixel are a power of two from 1 to 1024; the seed is 0..=4294967295.";

/// A parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Print the usage text.
    Help,
    /// Print the version.
    Version,
    /// Render the reference scene.
    Render {
        /// Image width in pixels (unchecked here; the renderer checks the size).
        width: u32,
        /// Image height in pixels.
        height: u32,
        /// The samples per pixel and the seed.
        settings: RenderSettings,
        /// The PNG file to write, with the bytes it was given.
        out: PathBuf,
    },
}

/// Why a command line was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// No command was given.
    MissingCommand,
    /// The command is not known (shown lossily if it is not Unicode).
    UnknownCommand(String),
    /// An option is not known (shown lossily if it is not Unicode).
    UnknownOption(String),
    /// An option was given twice.
    Repeated(&'static str),
    /// An option has no value after it.
    MissingValue(&'static str),
    /// A numeric value is not valid Unicode.
    NotUnicode {
        /// The option.
        option: &'static str,
        /// The value given, shown lossily.
        value: String,
    },
    /// A numeric value is not a decimal integer in `0..=u32::MAX`.
    InvalidNumber {
        /// The option.
        option: &'static str,
        /// The value given.
        value: String,
    },
    /// The sample count was refused.
    Samples(SettingsError),
    /// `--out` was not given.
    MissingOut,
}

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCommand => f.write_str("no command given"),
            Self::UnknownCommand(command) => write!(f, "unknown command {command:?}"),
            Self::UnknownOption(option) => write!(f, "unknown option {option:?}"),
            Self::Repeated(option) => write!(f, "{option} given more than once"),
            Self::MissingValue(option) => write!(f, "{option} needs a value"),
            Self::NotUnicode { option, value } => {
                write!(f, "{option} needs a whole number, not the non-Unicode {value:?}")
            }
            Self::InvalidNumber { option, value } => {
                write!(f, "{option} needs a whole number, not {value:?}")
            }
            Self::Samples(error) => write!(f, "--samples: {error}"),
            Self::MissingOut => f.write_str("render needs --out FILE"),
        }
    }
}

impl std::error::Error for UsageError {}

/// The options `render` takes.
const OPTIONS: [&str; 5] = ["--width", "--height", "--samples", "--seed", "--out"];

/// Parses the arguments after the program name.
///
/// # Errors
///
/// A [`UsageError`] for anything but `render` with known options, `--help`/`-h`/`help`, or
/// `--version`/`-V`.
pub fn parse<I, S>(args: I) -> Result<Command, UsageError>
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
    let mut numbers: [Option<u32>; 4] = [None; 4];
    let mut out: Option<PathBuf> = None;
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
            Some(slot @ 0..=3) => numbers[slot].replace(number(name, &value)?).is_some(),
            _ => out.replace(PathBuf::from(value)).is_some(),
        };
        if repeated {
            return Err(UsageError::Repeated(name));
        }
    }
    let [width, height, samples, seed] = numbers;
    let settings =
        RenderSettings::new(samples.unwrap_or(DEFAULT_SAMPLES), seed.unwrap_or(DEFAULT_SEED))
            .map_err(UsageError::Samples)?;
    Ok(Command::Render {
        width: width.unwrap_or(DEFAULT_WIDTH),
        height: height.unwrap_or(DEFAULT_HEIGHT),
        settings,
        out: out.ok_or(UsageError::MissingOut)?,
    })
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
mod tests {
    use super::*;

    fn parse_str(line: &str) -> Result<Command, UsageError> {
        parse(line.split_whitespace())
    }

    fn settings(samples: u32, seed: u32) -> RenderSettings {
        RenderSettings::new(samples, seed).expect("valid")
    }

    #[test]
    fn render_with_defaults_and_options() {
        assert_eq!(
            parse_str("render --out a.png"),
            Ok(Command::Render {
                width: 320,
                height: 180,
                settings: settings(4, 0),
                out: "a.png".into()
            })
        );
        assert_eq!(
            parse_str("render --height 9 --seed 7 --out b.png --samples 16 --width 16"),
            Ok(Command::Render {
                width: 16,
                height: 9,
                settings: settings(16, 7),
                out: "b.png".into()
            })
        );
        assert_eq!(
            parse_str("render --samples 1024 --seed 4294967295 --out c"),
            Ok(Command::Render {
                width: 320,
                height: 180,
                settings: settings(1024, u32::MAX),
                out: "c".into()
            })
        );
        assert_eq!(parse_str("--help"), Ok(Command::Help));
        assert_eq!(parse_str("render --help"), Ok(Command::Help));
        assert_eq!(parse_str("--version"), Ok(Command::Version));
    }

    #[test]
    fn bad_command_lines_are_refused() {
        assert_eq!(parse_str(""), Err(UsageError::MissingCommand));
        assert_eq!(parse_str("draw"), Err(UsageError::UnknownCommand("draw".into())));
        assert_eq!(parse_str("--fast"), Err(UsageError::UnknownOption("--fast".into())));
        assert_eq!(parse_str("render"), Err(UsageError::MissingOut));
        assert_eq!(parse_str("render --out"), Err(UsageError::MissingValue("--out")));
        assert_eq!(
            parse_str("render --out a --depth 3"),
            Err(UsageError::UnknownOption("--depth".into()))
        );
        assert_eq!(parse_str("render --out a --out b"), Err(UsageError::Repeated("--out")));
        assert_eq!(
            parse_str("render --width 2 --width 3 --out a"),
            Err(UsageError::Repeated("--width"))
        );
        assert_eq!(
            parse_str("render --seed 2 --seed 2 --out a"),
            Err(UsageError::Repeated("--seed"))
        );
        for option in ["--width", "--height", "--samples", "--seed"] {
            for value in ["-1", "+5", "1.5", "abc", "4294967296", "0x10", ""] {
                let args = ["render", option, value, "--out", "a"];
                assert_eq!(
                    parse(args),
                    Err(UsageError::InvalidNumber { option, value: value.into() }),
                    "{option} {value:?}"
                );
            }
        }
    }

    #[test]
    fn sample_counts_must_be_powers_of_two_up_to_1024() {
        for good in [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024] {
            let line = format!("render --samples {good} --out a");
            assert!(parse_str(&line).is_ok(), "{good}");
        }
        for (bad, error) in [
            (0, SettingsError::SamplesNotPowerOfTwo(0)),
            (3, SettingsError::SamplesNotPowerOfTwo(3)),
            (12, SettingsError::SamplesNotPowerOfTwo(12)),
            (1023, SettingsError::SamplesNotPowerOfTwo(1023)),
            (2048, SettingsError::TooManySamples(2048)),
            (1025, SettingsError::TooManySamples(1025)),
        ] {
            let line = format!("render --samples {bad} --out a");
            assert_eq!(parse_str(&line), Err(UsageError::Samples(error)), "{bad}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_arguments_never_panic() {
        use std::os::unix::ffi::OsStrExt;
        let bytes = OsStr::from_bytes(b"out-\xff\xfe.png");
        let args = |middle: &[&OsStr]| {
            let mut all: Vec<OsString> = vec!["render".into()];
            all.extend(middle.iter().map(|arg| arg.to_os_string()));
            parse(all)
        };
        // A non-Unicode output path keeps its bytes.
        let parsed = args(&[OsStr::new("--out"), bytes]);
        let Ok(Command::Render { out, .. }) = parsed else { panic!("{parsed:?}") };
        assert_eq!(out.as_os_str().as_bytes(), bytes.as_bytes());
        // A non-Unicode number is a typed error.
        for option in ["--width", "--height", "--samples", "--seed"] {
            let parsed = args(&[OsStr::new(option), bytes, OsStr::new("--out"), OsStr::new("a")]);
            assert_eq!(
                parsed,
                Err(UsageError::NotUnicode { option, value: "out-\u{fffd}\u{fffd}.png".into() })
            );
        }
        // A non-Unicode option or command is unknown.
        assert_eq!(
            args(&[bytes]),
            Err(UsageError::UnknownOption("out-\u{fffd}\u{fffd}.png".into()))
        );
        assert_eq!(
            parse([bytes]),
            Err(UsageError::UnknownCommand("out-\u{fffd}\u{fffd}.png".into()))
        );
    }
}
