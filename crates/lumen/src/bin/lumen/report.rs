//! What `lumen render` prints: the render's identities and settings, the run's threads and
//! tile, the ray count, the image hash, the file written and the wall-clock times.

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use lumen::render::{RayCount, MAX_BOUNCES, TONE_CURVE};
use lumen::{ImageSize, RenderSettings, Schedule, BUNNY_REV, INTEGRATOR_ID, REFERENCE_SCENE_ID};

/// The wall-clock parts of one run.
pub(crate) struct Times {
    pub(crate) scene: Duration,
    pub(crate) render: Duration,
    pub(crate) hash: Duration,
    pub(crate) png: Duration,
    pub(crate) total: Duration,
}

/// What `lumen render` prints.
pub(crate) struct Report {
    pub(crate) size: ImageSize,
    pub(crate) settings: RenderSettings,
    pub(crate) schedule: Schedule,
    pub(crate) workers: u32,
    pub(crate) rays: RayCount,
    pub(crate) hash: String,
    pub(crate) png: Option<(PathBuf, u64)>,
    pub(crate) times: Times,
}

impl Report {
    pub(crate) fn write(&self, out: &mut impl Write) -> io::Result<()> {
        let Self { size, settings, schedule, workers, rays, hash, png, times } = self;
        writeln!(
            out,
            "scene {REFERENCE_SCENE_ID}\nintegrator {INTEGRATOR_ID}\nshading {}\nsampler {}\n\
             tone {}\nencoding {}\nbunny {BUNNY_REV}\n\
             size {}x{}\npixels {}\nsamples {}\nseed {}\nbounces {MAX_BOUNCES}\n\
             threads {workers}\nthreads-asked {}\ntile {}x{}\n\
             rays {} (closest {}, shadow {})\nimage {hash}",
            bunny_optics::SHADING_ID,
            bunny_optics::SAMPLER_ID,
            TONE_CURVE.identity(),
            bunny_optics::RGBA8_OPAQUE_IDENTITY,
            size.width(),
            size.height(),
            u64::from(size.width()) * u64::from(size.height()),
            settings.samples_per_pixel(),
            settings.seed(),
            schedule.threads(),
            schedule.tile().width(),
            schedule.tile().height(),
            rays.total(),
            rays.closest,
            rays.shadow,
        )?;
        match png {
            Some((path, bytes)) => writeln!(out, "png {} ({bytes} bytes)", path.display())?,
            None => writeln!(out, "png none")?,
        }
        // Information only: wall-clock readings of this run, never part of the image.
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
}
