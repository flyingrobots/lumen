//! The render, help and version messages, against lines written by hand.

use lumen::{reference_scene, ImageSize, RenderSettings, View, DEFAULT_BAND_ROWS};

use super::*;

/// Oracle: the line by hand, from the library's provenance of a 40 by 24 render at 2 samples,
/// seed 5 (the values `tests/library.rs` pins), in the order of `RenderProvenance::fields()`.
#[test]
fn the_provenance_line_of_a_named_render() {
    let scene = reference_scene().expect("builds");
    let size = ImageSize::new(40, 24).expect("size");
    let settings = RenderSettings::new(2, 5).expect("settings");
    let provenance = RenderProvenance::named("lumen-reference-scene/4", &scene, size, settings);
    let want = format!(
        concat!(
            r#"{{"schema":"lumen-message/1","type":"provenance","scene":"lumen-reference-scene/4","#,
            r#""integrator":"lumen-diffuse-path/1","shading":"bunny-diffuse-shading/1","#,
            r#""sampler":"bunny-sobol-owen/1","tone":"bunny-tone-srgb/1","#,
            r#""encoding":"bunny-rgba8-opaque/1","bunny":"{}","width":40,"height":24,"#,
            r#""pixels":960,"samples":2,"seed":5,"bounces":1,"scene_content":"{}"}}"#
        ),
        lumen::BUNNY_REV,
        scene.content_id()
    );
    assert_eq!(self::provenance(&provenance).to_json(), want);
}

/// Oracle: an unnamed scene's `scene` is `null`; a debug view's provenance names the view, one
/// sample and no bounce.
#[test]
fn an_unnamed_scene_is_null_and_a_view_names_itself() {
    let scene = reference_scene().expect("builds");
    let size = ImageSize::new(4, 3).expect("size");
    let settings = RenderSettings::new(1, 0).expect("settings");
    let unnamed = self::provenance(&RenderProvenance::new(&scene, size, settings)).to_json();
    assert!(unnamed.contains(r#""type":"provenance","scene":null,"integrator":"#), "{unnamed}");
    let view = RenderProvenance::named("x/1", &scene, size, settings).with_view(View::Depth);
    let line = self::provenance(&view).to_json();
    assert!(line.contains(r#""integrator":"lumen-view-depth/1""#), "{line}");
    assert!(line.contains(r#""samples":1,"seed":0,"bounces":0,"#), "{line}");
}

fn outcome(png: Option<(PathBuf, u64)>) -> (RenderProvenance, Outcome) {
    let scene = reference_scene().expect("builds");
    let size = ImageSize::new(8, 4).expect("size");
    let settings = RenderSettings::new(1, 0).expect("settings");
    let provenance = RenderProvenance::named("x/1", &scene, size, settings);
    let schedule = Schedule::new(6, DEFAULT_BAND_ROWS).expect("schedule");
    let rays = RayCount { closest: 40, shadow: 30 };
    (provenance, Outcome { schedule, workers: 4, rays, hash: "sha256:ab".to_owned(), png })
}

/// Oracle: the lines by hand, with distinct numbers so that no member is written under another's
/// name; without `--out`, `png` is `null`.
#[test]
fn the_result_line_with_and_without_a_file() {
    let (provenance, without) = outcome(None);
    assert_eq!(
        result(&provenance, &without).to_json(),
        concat!(
            r#"{"schema":"lumen-message/1","type":"result","width":8,"height":4,"threads":4,"#,
            r#""threads_asked":6,"band_rows":1,"rays":{"total":70,"closest":40,"shadow":30},"#,
            r#""image":"sha256:ab","png":null}"#
        )
    );
    let (provenance, with) = outcome(Some((PathBuf::from("dir/a \"b\".png"), 123)));
    let line = result(&provenance, &with).to_json();
    assert!(line.ends_with(r#""png":{"path":"dir/a \"b\".png","bytes":123}}"#), "{line}");
}

/// Oracle: whole microseconds by hand, rounded down.
#[test]
fn the_timings_line_is_whole_microseconds() {
    let times = Times {
        scene: Duration::from_nanos(1_999),
        render: Duration::from_secs(2),
        hash: Duration::ZERO,
        png: Duration::from_nanos(999),
        total: Duration::from_millis(3),
    };
    assert_eq!(
        timings(&times).to_json(),
        concat!(
            r#"{"schema":"lumen-message/1","type":"timings","scene_us":1,"render_us":2000000,"#,
            r#""hash_us":0,"png_us":0,"total_us":3000}"#
        )
    );
}

/// Range limit: `u64::MAX` microseconds is written as itself; one microsecond more, which a
/// `Duration` holds, saturates to it instead of wrapping.
#[test]
fn a_time_saturates_exactly_past_u64_max_microseconds() {
    let at = Duration::from_micros(u64::MAX);
    assert_eq!(micros(at), u64::MAX);
    assert_eq!(micros(at - Duration::from_micros(1)), u64::MAX - 1);
    assert_eq!(micros(at + Duration::from_micros(1)), u64::MAX);
    assert_eq!(micros(Duration::MAX), u64::MAX);
}

/// Oracle: the lines by hand; the usage text keeps its line ends, escaped.
#[test]
fn the_help_and_version_lines() {
    assert_eq!(
        help("usage: a\n  b").to_json(),
        r#"{"schema":"lumen-message/1","type":"help","usage":"usage: a\n  b"}"#
    );
    assert_eq!(
        version("0.1.0", "abc").to_json(),
        r#"{"schema":"lumen-message/1","type":"version","lumen":"0.1.0","bunny":"abc"}"#
    );
}

/// Every message ends in exactly one line end and holds no other.
#[test]
fn a_message_is_written_as_one_line() {
    let mut out = Vec::new();
    emit(&mut out, &help("a\nb\r\nc")).expect("writes");
    let text = String::from_utf8(out).expect("utf-8");
    assert_eq!(text.matches('\n').count(), 1, "{text}");
    assert!(text.ends_with("}\n") && !text.contains('\r'), "{text}");
}
