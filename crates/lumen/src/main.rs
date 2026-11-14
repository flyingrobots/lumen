//! The `lumen` command: argument parsing and I/O around the library.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use lumen::cli::{self, Command, USAGE};
use lumen::png::write_png;
use lumen::render::{MAX_BOUNCES, TONE_CURVE};
use lumen::{
    reference_scene, render_scheduled, ImageSize, RenderSettings, Schedule, BUNNY_REV,
    INTEGRATOR_ID, MAX_THREADS, REFERENCE_SCENE_ID,
};

fn main() -> ExitCode {
    match cli::parse(std::env::args_os().skip(1)) {
        Ok(Command::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Command::Version) => {
            println!("lumen {} (bunny {BUNNY_REV})", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Command::Render { width, height, settings, threads, tile, out }) => {
            let threads = threads.unwrap_or_else(available_threads);
            let schedule = match Schedule::new(threads, tile) {
                Ok(schedule) => schedule,
                Err(error) => {
                    eprintln!("lumen: --threads: {error}");
                    return ExitCode::from(2);
                }
            };
            match run(width, height, settings, schedule, out.as_deref()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(message) => {
                    eprintln!("lumen: {message}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(error) => {
            eprintln!("lumen: {error}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// The default thread count: what `std::thread::available_parallelism` reports (inside a
/// container, its CPU limit), within `1..=MAX_THREADS`, or 1 if it reports nothing. This is the
/// only place lumen asks the host how many threads to use; the library takes a count.
fn available_threads() -> u32 {
    std::thread::available_parallelism()
        .map_or(1, |n| u32::try_from(n.get()).unwrap_or(MAX_THREADS))
        .clamp(1, MAX_THREADS)
}

/// Builds the reference scene, renders it, hashes the image, writes the PNG if asked, and prints
/// what was made and how long each part took.
///
/// The times are wall-clock readings of a monotonic clock (`Instant`), taken around each part;
/// they are printed and never reach the image.
fn run(
    width: u32,
    height: u32,
    settings: RenderSettings,
    schedule: Schedule,
    out: Option<&Path>,
) -> Result<(), String> {
    let started = Instant::now();
    let size = ImageSize::new(width, height)
        .map_err(|error| lumen::RenderError::Size(error).to_string())?;
    let scene = reference_scene().map_err(|error| lumen::RenderError::Scene(error).to_string())?;
    let built = started.elapsed();
    let render =
        render_scheduled(&scene, size, settings, schedule).map_err(|error| error.to_string())?;
    let rendered = started.elapsed();
    let image = &render.image;
    let hash = image.hash();
    let hashed = started.elapsed();
    let written = match out {
        Some(path) => {
            Some(write_file(path, image).map_err(|error| format!("{}: {error}", path.display()))?)
        }
        None => None,
    };
    let total = started.elapsed();
    let times = Times {
        scene: built,
        render: rendered.saturating_sub(built),
        hash: hashed.saturating_sub(rendered),
        png: total.saturating_sub(hashed),
        total,
    };
    let report = Report {
        size,
        settings,
        schedule,
        workers: schedule.workers(size),
        rays: render.rays,
        hash,
        png: out.map(Path::to_path_buf).zip(written),
        times,
    };
    let mut stdout = io::stdout().lock();
    report.write(&mut stdout).map_err(|error| format!("stdout: {error}"))
}

/// The wall-clock parts of one run.
struct Times {
    scene: Duration,
    render: Duration,
    hash: Duration,
    png: Duration,
    total: Duration,
}

/// What `lumen render` prints.
struct Report {
    size: ImageSize,
    settings: RenderSettings,
    schedule: Schedule,
    workers: u32,
    rays: lumen::render::RayCount,
    hash: String,
    png: Option<(PathBuf, u64)>,
    times: Times,
}

impl Report {
    fn write(&self, out: &mut impl Write) -> io::Result<()> {
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

/// Writes the PNG file and returns its length.
fn write_file(path: &Path, image: &lumen::Rgba8Image) -> io::Result<u64> {
    let mut file = BufWriter::new(File::create(path)?);
    write_png(&mut file, image)?;
    file.into_inner().map_err(io::IntoInnerError::into_error)?.sync_all()?;
    Ok(lumen::png::png_len(image))
}
