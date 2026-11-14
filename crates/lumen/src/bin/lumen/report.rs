//! What `lumen render` prints: the render's provenance, the run's threads and band height, the ray
//! count, the image hash, the file written and the wall-clock times.

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use lumen::{ProvenanceValue, RayCount, RenderProvenance, Schedule};

/// The wall-clock parts of one run.
pub(crate) struct Times {
    pub(crate) scene: Duration,
    pub(crate) render: Duration,
    pub(crate) hash: Duration,
    pub(crate) png: Duration,
    pub(crate) total: Duration,
}

/// What `lumen render` prints: the provenance (facts about the image), then the facts of this
/// run (threads, band height, file, times), with the render's results (rays, hash) between them.
pub(crate) struct Report {
    pub(crate) provenance: RenderProvenance,
    pub(crate) schedule: Schedule,
    pub(crate) workers: u32,
    pub(crate) rays: RayCount,
    pub(crate) hash: String,
    pub(crate) png: Option<(PathBuf, u64)>,
    pub(crate) times: Times,
}

impl Report {
    pub(crate) fn write(&self, out: &mut impl Write) -> io::Result<()> {
        let Self { provenance, schedule, workers, rays, hash, png, times } = self;
        for (name, value) in provenance.fields() {
            match value {
                ProvenanceValue::Identity(identity) => writeln!(out, "{name} {identity}")?,
                ProvenanceValue::Unnamed => writeln!(out, "{name} (unnamed)")?,
                ProvenanceValue::Content(content) => writeln!(out, "{name} {content}")?,
                ProvenanceValue::Size(size) => {
                    writeln!(out, "{name} {}x{}", size.width(), size.height())?;
                }
                ProvenanceValue::Number(number) => writeln!(out, "{name} {number}")?,
            }
        }
        writeln!(
            out,
            "threads {workers}\nthreads-asked {}\nband-rows {}\n\
             rays {} (closest {}, shadow {})\nimage {hash}",
            schedule.threads(),
            schedule.band().get(),
            rays.total(),
            rays.closest,
            rays.shadow,
        )?;
        match png {
            Some((path, bytes)) => writeln!(out, "png {} ({bytes} bytes)", path.display())?,
            None => writeln!(out, "png none")?,
        }
        write_times(out, times, *rays)
    }
}

/// The wall-clock readings of this run: information only, never part of the image.
fn write_times(out: &mut impl Write, times: &Times, rays: RayCount) -> io::Result<()> {
    let render = times.render.as_secs_f64();
    #[allow(
        clippy::cast_precision_loss,
        reason = "a ray count below 2^53 converts exactly; the rate is information only"
    )]
    let total_rays = rays.total() as f64;
    let per_second = if render > 0.0 { total_rays / render } else { 0.0 };
    let ns_per_ray = render * 1e9 / total_rays.max(1.0);
    writeln!(
        out,
        "scene-seconds {:.3} (information only)\n\
         render-seconds {render:.3} (information only)\n\
         hash-seconds {:.3} (information only)\n\
         png-seconds {:.3} (information only)\n\
         total-seconds {:.3} (information only: scene, render, hash and png)\n\
         rays-per-second {per_second:.0} (information only: wall clock, all threads)\n\
         ns-per-ray {ns_per_ray:.1} (information only: wall clock, all threads)",
        times.scene.as_secs_f64(),
        times.hash.as_secs_f64(),
        times.png.as_secs_f64(),
        times.total.as_secs_f64(),
    )
}
