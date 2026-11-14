//! The `lumen` command: argument parsing and I/O around the library.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use lumen::cli::{self, Command, USAGE};
use lumen::png::write_png;
use lumen::render::{MAX_BOUNCES, TONE_CURVE};
use lumen::{render_reference, RenderSettings, BUNNY_REV, INTEGRATOR_ID, REFERENCE_SCENE_ID};

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
        Ok(Command::Render { width, height, settings, out }) => {
            match run(width, height, settings, &out) {
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

/// Renders the reference scene, writes the PNG and prints what was made.
fn run(width: u32, height: u32, settings: RenderSettings, out: &Path) -> Result<(), String> {
    let started = Instant::now();
    let render = render_reference(width, height, settings).map_err(|error| error.to_string())?;
    let rendered = started.elapsed();
    let image = &render.image;
    let written = write_file(out, image).map_err(|error| format!("{}: {error}", out.display()))?;
    let rays = render.rays;
    // Information only: a wall-clock reading of this run, never part of the image.
    let seconds = rendered.as_secs_f64();
    #[allow(clippy::cast_precision_loss)]
    let ns_per_ray = seconds * 1e9 / rays.total().max(1) as f64;
    let mut stdout = io::stdout().lock();
    let report = writeln!(
        stdout,
        "scene {REFERENCE_SCENE_ID}\nintegrator {INTEGRATOR_ID}\nshading {}\nsampler {}\n\
         tone {}\nencoding {}\nbunny {BUNNY_REV}\n\
         size {width}x{height}\nsamples {}\nseed {}\nbounces {MAX_BOUNCES}\n\
         rays {} (closest {}, shadow {})\nimage {}\npng {} ({written} bytes)\n\
         render-seconds {seconds:.3} (information only)\n\
         ns-per-ray {ns_per_ray:.1} (information only)",
        bunny_optics::SHADING_ID,
        bunny_optics::SAMPLER_ID,
        TONE_CURVE.identity(),
        bunny_optics::RGBA8_OPAQUE_IDENTITY,
        settings.samples_per_pixel(),
        settings.seed(),
        rays.total(),
        rays.closest,
        rays.shadow,
        image.hash(),
        out.display(),
    );
    report.map_err(|error| format!("stdout: {error}"))
}

/// Writes the PNG file and returns its length.
fn write_file(path: &Path, image: &lumen::Rgba8Image) -> io::Result<u64> {
    let mut file = BufWriter::new(File::create(path)?);
    write_png(&mut file, image)?;
    file.into_inner().map_err(io::IntoInnerError::into_error)?.sync_all()?;
    Ok(lumen::png::png_len(image))
}
