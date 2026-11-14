//! The `lumen` command: argument parsing and I/O around the library.

mod cli;
mod report;

use std::fs::File;
use std::io::{self, BufWriter};
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use cli::{Command, USAGE};
use lumen::{
    png_len, render_scheduled, write_png, BuiltinScene, ImageSize, PngError, RenderError,
    RenderProvenance, RenderSettings, Rgba8Image, Schedule, BUNNY_REV, MAX_THREADS,
};
use report::{Report, Times};

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
        Ok(Command::Render { scene, width, height, settings, threads, band, out }) => {
            let threads = threads.unwrap_or_else(available_threads);
            let schedule = match Schedule::new(threads, band) {
                Ok(schedule) => schedule,
                Err(error) => {
                    eprintln!("lumen: --threads: {error}");
                    return ExitCode::from(2);
                }
            };
            match run(scene, width, height, settings, schedule, out.as_deref()) {
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

/// Builds the chosen scene, renders it, hashes the image, writes the PNG if asked, and prints
/// what was made and how long each part took.
///
/// The times are wall-clock readings of a monotonic clock (`Instant`), taken around each part;
/// they are printed and never reach the image.
fn run(
    scene: BuiltinScene,
    width: u32,
    height: u32,
    settings: RenderSettings,
    schedule: Schedule,
    out: Option<&Path>,
) -> Result<(), String> {
    let started = Instant::now();
    let size =
        ImageSize::new(width, height).map_err(|error| RenderError::Size(error).to_string())?;
    let built_scene = scene.build().map_err(|error| error.to_string())?;
    let built = started.elapsed();
    let render = render_scheduled(&built_scene, size, settings, schedule)
        .map_err(|error| error.to_string())?;
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
        provenance: RenderProvenance::named(scene.id(), &built_scene, size, settings),
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

/// Writes the PNG file and returns its length.
fn write_file(path: &Path, image: &Rgba8Image) -> Result<u64, PngError> {
    let mut file = BufWriter::new(File::create(path)?);
    write_png(&mut file, image)?;
    file.into_inner().map_err(io::IntoInnerError::into_error)?.sync_all()?;
    Ok(png_len(image))
}
