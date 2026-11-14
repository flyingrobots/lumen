//! Size class: medium (real threads at fixed counts; in process, no file, no network).
//!
//! The debug views (`render_view`, ADR 0009): `lit` is the lit render and provenance exactly;
//! every other view is checked against pixels derived by hand on constructed scenes, against its
//! golden hash for each built-in scene, and against itself at every schedule.

#[path = "../golden/mod.rs"]
mod golden;
mod goldens;
mod normals;

use bunny_linalg::FixedVec3;
use bunny_num::FixedQ32_32;
use lumen::{BandRows, CameraPlacement, ImageSize, RenderSettings, Rgba8Image, Schedule};

const ONE: i64 = 1 << 32;

fn at(x: i64, y: i64, z: i64) -> FixedVec3 {
    let unit = |n: i64| FixedQ32_32::from_raw(n * ONE);
    FixedVec3::new(unit(x), unit(y), unit(z))
}

fn schedule(threads: u32, rows: u32) -> Schedule {
    Schedule::new(threads, BandRows::new(rows).expect("rows")).expect("schedule")
}

fn size(width: u32, height: u32) -> ImageSize {
    ImageSize::new(width, height).expect("size")
}

fn one_sample() -> RenderSettings {
    RenderSettings::new(1, 0).expect("settings")
}

/// A camera at `z` on the axis looking along `look_z` (1 or -1), 90 degrees vertically.
fn camera_on_axis(z: i64, look_z: i64) -> CameraPlacement {
    CameraPlacement {
        position: at(0, 0, z),
        look: at(0, 0, look_z),
        up: at(0, 1, 0),
        tan_half_fov: FixedQ32_32::from_raw(ONE),
    }
}

/// The RGBA bytes of pixel `(x, y)`.
fn rgba(image: &Rgba8Image, x: u32, y: u32) -> [u8; 4] {
    image.pixel(x, y).expect("inside the image")
}
