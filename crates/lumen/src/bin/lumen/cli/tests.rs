//! The parser's tests: every option, its defaults and every refusal.

use lumen::{ScheduleError, SettingsError};

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
            threads: None,
            tile: DEFAULT_TILE,
            out: Some("a.png".into())
        })
    );
    assert_eq!(
        parse_str("render --height 9 --seed 7 --out b.png --samples 16 --width 16"),
        Ok(Command::Render {
            width: 16,
            height: 9,
            settings: settings(16, 7),
            threads: None,
            tile: DEFAULT_TILE,
            out: Some("b.png".into())
        })
    );
    assert_eq!(
        parse_str("render --samples 1024 --seed 4294967295 --out c"),
        Ok(Command::Render {
            width: 320,
            height: 180,
            settings: settings(1024, u32::MAX),
            threads: None,
            tile: DEFAULT_TILE,
            out: Some("c".into())
        })
    );
    assert_eq!(
        parse_str("render --threads 7 --tile 16x8"),
        Ok(Command::Render {
            width: 320,
            height: 180,
            settings: settings(4, 0),
            threads: Some(7),
            tile: TileSize::new(16, 8).expect("valid"),
            out: None
        })
    );
    assert_eq!(
        parse_str("render --tile 64 --threads 256"),
        Ok(Command::Render {
            width: 320,
            height: 180,
            settings: settings(4, 0),
            threads: Some(256),
            tile: TileSize::new(64, 64).expect("valid"),
            out: None
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
    assert_eq!(parse_str("render --seed 2 --seed 2 --out a"), Err(UsageError::Repeated("--seed")));
    for option in ["--width", "--height", "--samples", "--seed", "--threads"] {
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
fn threads_and_tiles_are_checked() {
    assert_eq!(
        parse_str("render --threads 0"),
        Err(UsageError::Schedule(ScheduleError::NoThreads))
    );
    assert_eq!(
        parse_str("render --threads 257"),
        Err(UsageError::Schedule(ScheduleError::TooManyThreads(257)))
    );
    assert_eq!(parse_str("render --threads 2 --threads 3"), Err(UsageError::Repeated("--threads")));
    for zero in ["0", "0x4", "4x0"] {
        let line = format!("render --tile {zero}");
        assert_eq!(parse_str(&line), Err(UsageError::Schedule(ScheduleError::EmptyTile)));
    }
    for bad in ["x", "4x", "x4", "4x4x4", "4X4", "-4", "4.5", "4 ", "4294967296", "4x+4"] {
        let args = ["render", "--tile", bad];
        assert_eq!(parse(args), Err(UsageError::InvalidTile(bad.into())), "{bad:?}");
    }
    assert_eq!(parse_str("render --tile 4 --tile 4"), Err(UsageError::Repeated("--tile")));
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
    let Ok(Command::Render { out: Some(out), .. }) = parsed else { panic!("{parsed:?}") };
    assert_eq!(out.as_os_str().as_bytes(), bytes.as_bytes());
    // A non-Unicode number is a typed error.
    for option in ["--width", "--height", "--samples", "--seed", "--threads"] {
        let parsed = args(&[OsStr::new(option), bytes, OsStr::new("--out"), OsStr::new("a")]);
        assert_eq!(
            parsed,
            Err(UsageError::NotUnicode { option, value: "out-\u{fffd}\u{fffd}.png".into() })
        );
    }
    // A non-Unicode tile is a typed error.
    assert_eq!(
        args(&[OsStr::new("--tile"), bytes]),
        Err(UsageError::InvalidTile("out-\u{fffd}\u{fffd}.png".into()))
    );
    // A non-Unicode option or command is unknown.
    assert_eq!(args(&[bytes]), Err(UsageError::UnknownOption("out-\u{fffd}\u{fffd}.png".into())));
    assert_eq!(parse([bytes]), Err(UsageError::UnknownCommand("out-\u{fffd}\u{fffd}.png".into())));
}
