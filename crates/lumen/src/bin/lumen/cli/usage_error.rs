//! Why a command line was refused.

use core::fmt;

use lumen::{ParseSceneError, ScheduleError, SettingsError};

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
    /// The thread count or the tile size was refused.
    Schedule(ScheduleError),
    /// The scene name is not a built-in scene.
    Scene(ParseSceneError),
    /// A tile is not `N` or `WxH` with decimal `u32`s.
    InvalidTile(String),
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
            Self::Schedule(error @ ScheduleError::EmptyTile) => write!(f, "--tile: {error}"),
            Self::Schedule(error) => write!(f, "--threads: {error}"),
            Self::Scene(error) => write!(f, "--scene: {error}"),
            Self::InvalidTile(value) => {
                write!(f, "--tile needs N or WxH with whole numbers, not {value:?}")
            }
        }
    }
}

impl std::error::Error for UsageError {}
