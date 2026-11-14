//! Command-line arguments.

use core::fmt;
use std::path::PathBuf;

/// The default image width: small enough to render in about a second in a debug build.
pub const DEFAULT_WIDTH: u32 = 320;
/// The default image height.
pub const DEFAULT_HEIGHT: u32 = 180;

/// The usage text.
pub const USAGE: &str = "\
usage: lumen render --out FILE [--width W] [--height H]
       lumen --help | --version

Renders lumen's built-in reference scene to a PNG file on one thread and prints the image
hash (SHA-256 of the raw RGBA8 bytes). Defaults: --width 320 --height 180.
Width and height are 1..=32768 each, with at most 134217728 (2^27) pixels in all.";

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
        /// The PNG file to write.
        out: PathBuf,
    },
}

/// Why a command line was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// No command was given.
    MissingCommand,
    /// The command is not known.
    UnknownCommand(String),
    /// An option is not known.
    UnknownOption(String),
    /// An option was given twice.
    Repeated(&'static str),
    /// An option has no value after it.
    MissingValue(&'static str),
    /// A width or height is not a decimal integer in `0..=u32::MAX`.
    InvalidNumber {
        /// The option.
        option: &'static str,
        /// The value given.
        value: String,
    },
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
            Self::InvalidNumber { option, value } => {
                write!(f, "{option} needs a whole number of pixels, not {value:?}")
            }
            Self::MissingOut => f.write_str("render needs --out FILE"),
        }
    }
}

impl std::error::Error for UsageError {}

/// Parses the arguments after the program name.
///
/// # Errors
///
/// A [`UsageError`] for anything but `render` with known options, `--help`/`-h`/`help`, or
/// `--version`/`-V`.
pub fn parse<I, S>(args: I) -> Result<Command, UsageError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut args = args.into_iter().map(Into::into);
    let command = args.next().ok_or(UsageError::MissingCommand)?;
    match command.as_str() {
        "--help" | "-h" | "help" => return Ok(Command::Help),
        "--version" | "-V" => return Ok(Command::Version),
        "render" => {}
        _ if command.starts_with('-') => return Err(UsageError::UnknownOption(command)),
        _ => return Err(UsageError::UnknownCommand(command)),
    }
    let (mut width, mut height, mut out) = (None, None, None);
    while let Some(option) = args.next() {
        let name = match option.as_str() {
            "--width" => "--width",
            "--height" => "--height",
            "--out" => "--out",
            "--help" | "-h" => return Ok(Command::Help),
            _ => return Err(UsageError::UnknownOption(option)),
        };
        let value = args.next().ok_or(UsageError::MissingValue(name))?;
        let number = || {
            // Digits only: no sign, no spaces, no underscores.
            let digits = !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit());
            digits
                .then(|| value.parse::<u32>().ok())
                .flatten()
                .ok_or_else(|| UsageError::InvalidNumber { option: name, value: value.clone() })
        };
        let repeated = match name {
            "--width" => width.replace(number()?).is_some(),
            "--height" => height.replace(number()?).is_some(),
            _ => out.replace(PathBuf::from(&value)).is_some(),
        };
        if repeated {
            return Err(UsageError::Repeated(name));
        }
    }
    Ok(Command::Render {
        width: width.unwrap_or(DEFAULT_WIDTH),
        height: height.unwrap_or(DEFAULT_HEIGHT),
        out: out.ok_or(UsageError::MissingOut)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(line: &str) -> Result<Command, UsageError> {
        parse(line.split_whitespace())
    }

    #[test]
    fn render_with_defaults_and_options() {
        assert_eq!(
            parse_str("render --out a.png"),
            Ok(Command::Render { width: 320, height: 180, out: "a.png".into() })
        );
        assert_eq!(
            parse_str("render --height 9 --out b.png --width 16"),
            Ok(Command::Render { width: 16, height: 9, out: "b.png".into() })
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
        for value in ["-1", "+5", "1.5", "abc", "4294967296", "0x10", ""] {
            let args = ["render", "--width", value, "--out", "a"];
            assert_eq!(
                parse(args),
                Err(UsageError::InvalidNumber { option: "--width", value: value.into() }),
                "{value:?}"
            );
        }
    }
}
