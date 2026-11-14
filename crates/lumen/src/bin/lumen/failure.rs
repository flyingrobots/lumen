//! Why the command failed ([`Failure`]), and the one place that maps a failure to its message
//! and exit code ([`Failure::exit_code`] and its `Display`).

use core::fmt;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use lumen::{BuiltinSceneError, PngError, RenderError, ScheduleError, SizeError};

use crate::cli::{UsageError, USAGE};

/// Why the command failed. Each keeps the library's or the system's error, which `source()`
/// returns.
#[derive(Debug)]
pub(crate) enum Failure {
    /// The command line was refused: exit code 2, the message and the usage text.
    Usage(UsageError),
    /// The thread count the system reported was refused: exit code 2. `available_threads`
    /// clamps it to the accepted range, so this happens for no host.
    Threads(ScheduleError),
    /// The image size was refused: exit code 1.
    Size(SizeError),
    /// The chosen scene could not be built: exit code 1.
    Scene(BuiltinSceneError),
    /// The render failed: exit code 1.
    Render(RenderError),
    /// The PNG file could not be written: exit code 1, with the path.
    Png {
        /// The output path, as given.
        path: PathBuf,
        /// Why it could not be written.
        error: PngError,
    },
    /// The report could not be written to standard output: exit code 1.
    Stdout(io::Error),
}

impl Failure {
    /// The process exit code: 2 for a refused command line (and thread count), 1 for anything
    /// that fails once the command line is accepted.
    pub(crate) fn exit_code(&self) -> ExitCode {
        match self {
            Self::Usage(_) | Self::Threads(_) => ExitCode::from(2),
            Self::Size(_)
            | Self::Scene(_)
            | Self::Render(_)
            | Self::Png { .. }
            | Self::Stdout(_) => ExitCode::FAILURE,
        }
    }
}

/// The message after `lumen: `, as the command prints it to standard error.
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(error) => write!(f, "{error}\n\n{USAGE}"),
            Self::Threads(error) => write!(f, "--threads: {error}"),
            Self::Size(error) => write!(f, "{}", RenderError::Size(*error)),
            Self::Scene(error) => write!(f, "{error}"),
            Self::Render(error) => write!(f, "{error}"),
            Self::Png { path, error } => write!(f, "{}: {error}", path.display()),
            Self::Stdout(error) => write!(f, "stdout: {error}"),
        }
    }
}

impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Usage(error) => Some(error),
            Self::Threads(error) => Some(error),
            Self::Size(error) => Some(error),
            Self::Scene(error) => Some(error),
            Self::Render(error) => Some(error),
            Self::Png { error, .. } => Some(error),
            Self::Stdout(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use lumen::ImageSize;

    use super::*;

    /// Oracle: the messages and codes `tests/cli.rs` pins for the command, written out here
    /// for each variant, including those no command line reaches.
    #[test]
    fn each_failure_has_its_message_and_exit_code() {
        let size = ImageSize::new(0, 3).expect_err("empty");
        let png = PngError::Write(io::Error::other("disk full"));
        let cases = [
            (
                Failure::Usage(UsageError::MissingCommand),
                format!("no command given\n\n{USAGE}"),
                ExitCode::from(2),
            ),
            (
                Failure::Threads(ScheduleError::NoThreads),
                "--threads: threads must be at least 1".to_owned(),
                ExitCode::from(2),
            ),
            (
                Failure::Size(size),
                "invalid image size: image width and height must be at least 1".to_owned(),
                ExitCode::FAILURE,
            ),
            (
                Failure::Png { path: PathBuf::from("out/x.png"), error: png },
                "out/x.png: disk full".to_owned(),
                ExitCode::FAILURE,
            ),
            (
                Failure::Stdout(io::Error::other("closed")),
                "stdout: closed".to_owned(),
                ExitCode::FAILURE,
            ),
        ];
        for (failure, text, code) in cases {
            assert_eq!(failure.to_string(), text);
            assert_eq!(failure.exit_code(), code, "{text}");
            assert!(failure.source().is_some(), "{text}");
        }
    }
}
