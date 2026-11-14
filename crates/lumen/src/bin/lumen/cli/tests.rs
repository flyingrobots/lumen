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
            scene: BuiltinScene::Reference,
            width: 320,
            height: 180,
            settings: settings(4, 0),
            threads: None,
            band: DEFAULT_BAND_ROWS,
            out: Some("a.png".into()),
            detail: Detail::Report
        })
    );
    assert_eq!(
        parse_str("render --height 9 --seed 7 --out b.png --samples 16 --width 16"),
        Ok(Command::Render {
            scene: BuiltinScene::Reference,
            width: 16,
            height: 9,
            settings: settings(16, 7),
            threads: None,
            band: DEFAULT_BAND_ROWS,
            out: Some("b.png".into()),
            detail: Detail::Report
        })
    );
    assert_eq!(
        parse_str("render --samples 1024 --seed 4294967295 --out c"),
        Ok(Command::Render {
            scene: BuiltinScene::Reference,
            width: 320,
            height: 180,
            settings: settings(1024, u32::MAX),
            threads: None,
            band: DEFAULT_BAND_ROWS,
            out: Some("c".into()),
            detail: Detail::Report
        })
    );
    assert_eq!(
        parse_str("render --threads 7 --band-rows 8"),
        Ok(Command::Render {
            scene: BuiltinScene::Reference,
            width: 320,
            height: 180,
            settings: settings(4, 0),
            threads: Some(7),
            band: BandRows::new(8).expect("valid"),
            out: None,
            detail: Detail::Report
        })
    );
    assert_eq!(
        parse_str("render --band-rows 64 --threads 256"),
        Ok(Command::Render {
            scene: BuiltinScene::Reference,
            width: 320,
            height: 180,
            settings: settings(4, 0),
            threads: Some(256),
            band: BandRows::new(64).expect("valid"),
            out: None,
            detail: Detail::Report
        })
    );
    assert_eq!(parse_str("--help"), Ok(Command::Help));
    assert_eq!(parse_str("render --help"), Ok(Command::Help));
    assert_eq!(parse_str("--version"), Ok(Command::Version));
}

#[test]
fn the_scene_is_chosen_by_name() {
    let scene_of = |line: &str| match parse_str(line) {
        Ok(Command::Render { scene, .. }) => scene,
        other => panic!("{line}: {other:?}"),
    };
    assert_eq!(scene_of("render"), BuiltinScene::Reference);
    for scene in BuiltinScene::ALL {
        assert_eq!(scene_of(&format!("render --scene {}", scene.name())), scene);
    }
    assert_eq!(scene_of("render --out a --scene fixed-point"), BuiltinScene::FixedPoint);
}

#[test]
fn a_bad_scene_option_is_refused() {
    let unknown =
        |name: &str| UsageError::Scene(name.parse::<BuiltinScene>().expect_err("unknown"));
    for bad in ["Reference", "fixed_point", "fixed point", "", "bunny"] {
        assert_eq!(parse(["render", "--scene", bad]), Err(unknown(bad)), "{bad:?}");
    }
    assert_eq!(parse_str("render --scene"), Err(UsageError::MissingValue("--scene")));
    assert_eq!(
        parse_str("render --scene reference --scene reference"),
        Err(UsageError::Repeated("--scene"))
    );
    let message = unknown("bunny").to_string();
    assert_eq!(message, "--scene: unknown scene \"bunny\"; the scenes are reference, fixed-point");
}

#[test]
fn the_usage_text_lists_every_scene() {
    for scene in BuiltinScene::ALL {
        assert!(USAGE.contains(scene.name()), "{}", scene.name());
    }
    assert!(USAGE.contains("--scene NAME"));
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
    for option in ["--width", "--height", "--samples", "--seed", "--threads", "--band-rows"] {
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
fn threads_and_bands_are_checked() {
    assert_eq!(
        parse_str("render --threads 0"),
        Err(UsageError::Schedule(ScheduleError::NoThreads))
    );
    assert_eq!(
        parse_str("render --threads 257"),
        Err(UsageError::Schedule(ScheduleError::TooManyThreads(257)))
    );
    assert_eq!(parse_str("render --threads 2 --threads 3"), Err(UsageError::Repeated("--threads")));
    assert_eq!(
        parse_str("render --band-rows 0"),
        Err(UsageError::Schedule(ScheduleError::EmptyBand))
    );
    assert_eq!(
        parse_str("render --band-rows 4 --band-rows 4"),
        Err(UsageError::Repeated("--band-rows"))
    );
    // The tile option is gone, not quietly reinterpreted.
    assert_eq!(parse_str("render --tile 32"), Err(UsageError::UnknownOption("--tile".into())));
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
    for option in ["--width", "--height", "--samples", "--seed", "--threads", "--band-rows"] {
        let parsed = args(&[OsStr::new(option), bytes, OsStr::new("--out"), OsStr::new("a")]);
        assert_eq!(
            parsed,
            Err(UsageError::NotUnicode { option, value: "out-\u{fffd}\u{fffd}.png".into() })
        );
    }
    // A non-Unicode scene name is an unknown scene.
    assert!(matches!(args(&[OsStr::new("--scene"), bytes]), Err(UsageError::Scene(_))));
    // A non-Unicode option or command is unknown.
    assert_eq!(args(&[bytes]), Err(UsageError::UnknownOption("out-\u{fffd}\u{fffd}.png".into())));
    assert_eq!(parse([bytes]), Err(UsageError::UnknownCommand("out-\u{fffd}\u{fffd}.png".into())));
}

#[test]
fn an_option_value_may_be_written_after_an_equals_sign_and_may_then_look_like_an_option() {
    let command = |line: &str| parse_str(line).expect("parses");
    let Command::Render { width, out, .. } = command("render --width=96 --out=--width") else {
        unreachable!()
    };
    assert_eq!((width, out), (96, Some("--width".into())));
    // Only the first `=` splits: the value may hold more.
    let Command::Render { out, .. } = command("render --out=a=b") else { unreachable!() };
    assert_eq!(out, Some("a=b".into()));
}

#[test]
fn a_separate_value_that_looks_like_an_option_is_refused_but_not_a_negative_number() {
    for option in OPTIONS {
        assert_eq!(
            parse_str(&format!("render {option} --width")),
            Err(UsageError::OptionLikeValue { option, value: "--width".into() }),
            "{option}"
        );
    }
    assert!(matches!(parse_str("render --out -h"), Err(UsageError::OptionLikeValue { .. })));
    assert!(matches!(parse_str("render --width -3"), Err(UsageError::InvalidNumber { .. })));
    // A lone dash is a file name.
    let Ok(Command::Render { out, .. }) = parse_str("render --out -") else { panic!("refused") };
    assert_eq!(out, Some("-".into()));
}

#[test]
fn leading_zeros_are_decimal_for_every_numeric_option() {
    assert_eq!(
        parse_str(
            "render --width 0096 --height 054 --samples 004 --seed 0000 --threads 02 --band-rows 007"
        ),
        parse_str("render --width 96 --height 54 --samples 4 --seed 0 --threads 2 --band-rows 7")
    );
}

/// Oracle: the grammar of `stats`, written out: an optional `--scene NAME` or `--scene=NAME`,
/// the reference scene by default, and no render option.
#[test]
fn stats_takes_a_scene_and_nothing_else() {
    let stats = |scene| Ok(Command::Stats { scene });
    assert_eq!(parse_str("stats"), stats(BuiltinScene::Reference));
    assert_eq!(parse_str("stats --scene fixed-point"), stats(BuiltinScene::FixedPoint));
    assert_eq!(parse_str("stats --scene=fixed-point"), stats(BuiltinScene::FixedPoint));
    assert_eq!(parse_str("stats --help"), Ok(Command::Help));
    assert_eq!(
        parse_str("stats --scene reference --scene=reference"),
        Err(UsageError::Repeated("--scene"))
    );
    assert_eq!(parse_str("stats --width 9"), Err(UsageError::UnknownOption("--width".into())));
    assert_eq!(parse_str("stats --scenes"), Err(UsageError::UnknownOption("--scenes".into())));
    assert_eq!(parse_str("stats --scene"), Err(UsageError::MissingValue("--scene")));
    assert!(matches!(parse_str("stats --scene nope"), Err(UsageError::Scene(_))));
}

/// Oracle: the grammar of `--stats`, written out: a flag with no value, anywhere among the
/// render options, at most once.
#[test]
fn stats_is_a_flag_of_render() {
    let detail = |line: &str| match parse_str(line) {
        Ok(Command::Render { detail, .. }) => Ok(detail),
        other => Err(other),
    };
    assert_eq!(detail("render"), Ok(Detail::Report));
    assert_eq!(detail("render --stats"), Ok(Detail::Statistics));
    assert_eq!(detail("render --width 8 --stats --height 4"), Ok(Detail::Statistics));
    assert_eq!(parse_str("render --stats --stats"), Err(UsageError::Repeated("--stats")));
    assert_eq!(
        parse_str("render --stats=json"),
        Err(UsageError::UnknownOption("--stats=json".into()))
    );
}
