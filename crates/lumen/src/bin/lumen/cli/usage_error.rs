//! Why a command line was refused.

use core::fmt;

use lumen::{ParseSceneError, ParseViewError, ScheduleError, SettingsError, View};

/// Why a command line was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UsageError {
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
    /// The value after an option reads as an option itself (`--out --width`); written
    /// `--out=--width` it is a value.
    OptionLikeValue {
        /// The option.
        option: &'static str,
        /// The value given, shown lossily.
        value: String,
    },
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
    /// The thread count or the band height was refused.
    Schedule(ScheduleError),
    /// The scene name is not a built-in scene.
    Scene(ParseSceneError),
    /// The view name is not a view.
    View(ParseViewError),
    /// A debug view traces one ray per pixel, and other samples were asked for.
    ViewSamples {
        /// The view.
        view: View,
        /// The samples asked for.
        samples: u32,
    },
    /// `--stats` was asked for with a debug view.
    ViewStatistics(View),
}

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCommand => f.write_str("no command given"),
            Self::UnknownCommand(command) => write!(f, "unknown command {command:?}"),
            Self::UnknownOption(option) => write!(f, "unknown option {option:?}"),
            Self::Repeated(option) => write!(f, "{option} given more than once"),
            Self::MissingValue(option) => write!(f, "{option} needs a value"),
            Self::OptionLikeValue { option, value } => write!(
                f,
                "{option} needs a value, not the option {value:?} (to use it as the value, write \
                 {option}={value})"
            ),
            Self::NotUnicode { option, value } => {
                write!(f, "{option} needs a whole number, not the non-Unicode {value:?}")
            }
            Self::InvalidNumber { option, value } => {
                write!(f, "{option} needs a whole number, not {value:?}")
            }
            Self::Samples(error) => write!(f, "--samples: {error}"),
            Self::Schedule(error @ ScheduleError::EmptyBand) => write!(f, "--band-rows: {error}"),
            Self::Schedule(error) => write!(f, "--threads: {error}"),
            Self::Scene(error) => write!(f, "--scene: {error}"),
            Self::View(error) => write!(f, "--view: {error}"),
            Self::ViewSamples { view, samples } => write!(
                f,
                "--view {view} traces one ray through each pixel's centre: --samples must be 1, \
                 not {samples}"
            ),
            Self::ViewStatistics(view) => {
                write!(f, "--stats reports on the lit image, not on --view {view}")
            }
        }
    }
}

impl std::error::Error for UsageError {}
