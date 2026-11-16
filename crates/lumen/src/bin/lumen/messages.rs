//! The messages `lumen` writes to standard output: one JSON object per line under
//! `lumen-message/1` (ADR 0015, `schemas/lumen-message-1.schema.json`). This module holds the
//! messages of a render (`provenance`, `result`, `timings`), of `--help` (`help`) and of
//! `--version` (`version`); `stats.rs` holds the four statistics messages.

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use lumen::{ProvenanceValue, RayCount, RenderProvenance, Schedule};

use crate::json::{Object, Value};

/// The version tag every message carries in its `schema` member.
pub(crate) const MESSAGE_SCHEMA_ID: &str = "lumen-message/1";

/// A message of type `kind`: its `schema` and `type` members, to which the caller adds the rest.
pub(crate) fn message(kind: &'static str) -> Object {
    Object::new().with("schema", MESSAGE_SCHEMA_ID).with("type", kind)
}

/// Writes `message` as one line and flushes it, so that a caller reading the stream sees each
/// message when it is written (ADR 0015, "Messages are written as the work proceeds").
pub(crate) fn emit(out: &mut impl Write, message: &Object) -> io::Result<()> {
    let mut line = message.to_json();
    line.push('\n');
    out.write_all(line.as_bytes())?;
    out.flush()
}

/// `lumen --help`: the usage text, as one string.
pub(crate) fn help(usage: &str) -> Object {
    message("help").with("usage", usage)
}

/// `lumen --version`: the package version and the Bunny commit.
pub(crate) fn version(lumen: &str, bunny: &str) -> Object {
    message("version").with("lumen", lumen).with("bunny", bunny)
}

/// The render's provenance, written when the render starts: `RenderProvenance::fields()` in
/// order, each key with its hyphen written as an underscore, the size as `width` and `height`,
/// and an unnamed scene as `null`.
pub(crate) fn provenance(provenance: &RenderProvenance) -> Object {
    provenance.fields().into_iter().fold(message("provenance"), |object, (name, value)| {
        match value {
            ProvenanceValue::Identity(identity) => object.with(member(name), identity),
            ProvenanceValue::Unnamed => object.with(member(name), Value::Null),
            ProvenanceValue::Content(content) => object.with(member(name), content.to_string()),
            ProvenanceValue::Size(size) => {
                object.with("width", size.width()).with("height", size.height())
            }
            ProvenanceValue::Number(number) => object.with(member(name), number),
        }
    })
}

/// The member name of a provenance field: its name with `-` written as `_`.
fn member(field: &str) -> String {
    field.replace('-', "_")
}

/// What a render produced, written when it finishes.
pub(crate) struct Outcome {
    /// The schedule asked for.
    pub(crate) schedule: Schedule,
    /// The workers that rendered: never more than the bands.
    pub(crate) workers: u32,
    /// The rays traced.
    pub(crate) rays: RayCount,
    /// The image hash.
    pub(crate) hash: String,
    /// The file written and its length in bytes, if `--out` was given.
    pub(crate) png: Option<(PathBuf, u64)>,
}

/// The `result` message: the size, the schedule, the rays, the image hash and the file.
pub(crate) fn result(provenance: &RenderProvenance, outcome: &Outcome) -> Object {
    let Outcome { schedule, workers, rays, hash, png } = outcome;
    let size = provenance.size();
    let rays = Object::new()
        .with("total", rays.total())
        .with("closest", rays.closest)
        .with("shadow", rays.shadow);
    let png = png.as_ref().map(|(path, bytes)| {
        Object::new().with("path", path.display().to_string()).with("bytes", *bytes)
    });
    message("result")
        .with("width", size.width())
        .with("height", size.height())
        .with("threads", *workers)
        .with("threads_asked", schedule.threads())
        .with("band_rows", schedule.band().get())
        .with("rays", rays)
        .with("image", hash.as_str())
        .with("png", png)
}

/// The wall-clock parts of one run.
pub(crate) struct Times {
    pub(crate) scene: Duration,
    pub(crate) render: Duration,
    pub(crate) hash: Duration,
    pub(crate) png: Duration,
    pub(crate) total: Duration,
}

/// The `timings` message: each part's wall-clock time in whole microseconds, information only.
pub(crate) fn timings(times: &Times) -> Object {
    message("timings")
        .with("scene_us", micros(times.scene))
        .with("render_us", micros(times.render))
        .with("hash_us", micros(times.hash))
        .with("png_us", micros(times.png))
        .with("total_us", micros(times.total))
}

/// A duration in whole microseconds, rounded down, and `u64::MAX` for one longer than that
/// (about 584 942 years).
pub(crate) fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests;
