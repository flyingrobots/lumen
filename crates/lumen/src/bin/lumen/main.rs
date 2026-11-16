//! The `lumen` command: argument parsing and I/O around the library.

// The command is the edge outside the deterministic core (docs/adr/0006-nondeterminism-gate.md):
// everything the library may not touch happens here, and none of it reaches a pixel or a hash.
#![expect(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::disallowed_macros,
    reason = "time-instant, host-parallelism, std-env, std-fs, host-io: the command's edge reads \
              its arguments, writes the PNG to a file and its messages to standard output, times \
              the phases in microseconds and asks the host for the default thread count; the \
              library gets only values"
)]

mod cli;
mod failure;
mod json;
mod messages;
mod publish;
mod stats;

use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use cli::{Command, Detail, USAGE};
use failure::Failure;
use lumen::{
    render_traced_with_traversal, render_view, BuiltinScene, ImageSize, RenderProvenance,
    RenderSettings, Schedule, View, BUNNY_REV, MAX_THREADS,
};
use publish::Disk;
use messages::{emit, Outcome, Times};

fn main() -> ExitCode {
    match command(std::env::args_os().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprintln!("lumen: {failure}");
            failure.exit_code()
        }
    }
}

/// Runs the command line `args` (after the program name).
fn command(args: impl Iterator<Item = OsString>) -> Result<(), Failure> {
    match cli::parse(args).map_err(Failure::Usage)? {
        Command::Help => print(&messages::help(USAGE))?,
        Command::Version => print(&messages::version(env!("CARGO_PKG_VERSION"), BUNNY_REV))?,
        Command::Stats { scene } => describe(scene)?,
        Command::Render { scene, width, height, settings, threads, band, out, detail, view } => {
            let threads = threads.unwrap_or_else(available_threads);
            let schedule = Schedule::new(threads, band).map_err(Failure::Threads)?;
            let size = ImageSize::new(width, height).map_err(Failure::Size)?;
            run(Job { scene, view, size, settings, schedule }, out.as_deref(), detail)?;
        }
    }
    Ok(())
}

/// The default thread count: what `std::thread::available_parallelism` reports (inside a
/// container, its CPU limit), within `1..=MAX_THREADS`, or 1 if it reports nothing. This is the
/// only place lumen asks the host how many threads to use; the library takes a count.
fn available_threads() -> u32 {
    std::thread::available_parallelism()
        .map_or(1, |n| u32::try_from(n.get()).unwrap_or(MAX_THREADS))
        .clamp(1, MAX_THREADS)
}

/// Writes one message to standard output.
fn print(message: &json::Object) -> Result<(), Failure> {
    emit(&mut io::stdout().lock(), message).map_err(Failure::Stdout)
}

/// Builds the chosen scene and prints its statistics: no render, no clock.
fn describe(scene: BuiltinScene) -> Result<(), Failure> {
    let built = scene.build().map_err(Failure::Scene)?;
    let statistics = built.statistics().map_err(Failure::Statistics)?;
    print(&stats::scene_statistics(scene.id(), &built, &statistics))
}

/// What to render: the scene, the view, the size, the settings and the schedule.
#[derive(Clone, Copy)]
struct Job {
    scene: BuiltinScene,
    view: View,
    size: ImageSize,
    settings: RenderSettings,
    schedule: Schedule,
}

/// Builds the chosen scene, checks that the PNG can be written if asked, prints the render's
/// `provenance`, renders it, hashes the image, publishes the PNG, and prints the `result` and
/// the `timings` (and with `--stats` the four statistics messages). Each message is flushed when
/// it is written (ADR 0015).
///
/// The times are wall-clock readings of a monotonic clock (`Instant`), taken around each part;
/// they are printed and never reach the image.
fn run(job: Job, out: Option<&Path>, detail: Detail) -> Result<(), Failure> {
    let Job { scene, view, size, settings, schedule } = job;
    let started = Instant::now();
    let built_scene = scene.build().map_err(Failure::Scene)?;
    if let Some(path) = out {
        // Refused before any pixel is computed, with nothing left behind.
        publish::preflight(&Disk, path)
            .map_err(|error| Failure::Output { path: path.to_path_buf(), error })?;
    }
    let built = started.elapsed();
    let provenance =
        RenderProvenance::named(scene.id(), &built_scene, size, settings).with_view(view);
    print(&messages::provenance(&provenance))?;
    // Only `--stats` asks the workers and the traversals to count: without it the render does no
    // counting work for run or traversal statistics (ADR 0008).
    let (render, counted) = match detail {
        Detail::Report => (
            render_view(&built_scene, view, size, settings, schedule).map_err(Failure::Render)?,
            None,
        ),
        Detail::Statistics => {
            let (render, run, traversal) =
                render_traced_with_traversal(&built_scene, &built_scene, size, settings, schedule)
                    .map_err(Failure::Render)?;
            (render, Some((run, traversal)))
        }
    };
    let rendered = started.elapsed();
    let image = &render.image;
    let hash = image.hash();
    let hashed = started.elapsed();
    let written = match out {
        Some(path) => Some(
            publish::publish(&Disk, path, image)
                .map_err(|error| Failure::Output { path: path.to_path_buf(), error })?,
        ),
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
    let outcome = Outcome {
        schedule,
        workers: schedule.workers(size),
        rays: render.rays,
        hash,
        png: out.map(Path::to_path_buf).zip(written),
    };
    // Read after the clock stops, and only when asked: the statistics take no part in the times.
    let statistics = match detail {
        Detail::Report => None,
        Detail::Statistics => {
            render.statistics.check(size, settings).map_err(Failure::RayCheck)?;
            Some(built_scene.statistics().map_err(Failure::Statistics)?)
        }
    };
    print(&messages::result(&provenance, &outcome))?;
    print(&messages::timings(&times))?;
    if let Some(scene_statistics) = statistics {
        print(&stats::scene_statistics(scene.id(), &built_scene, &scene_statistics))?;
        print(&stats::ray_statistics(&render.statistics, settings))?;
        if let Some((run, traversal)) = &counted {
            print(&stats::traversal_statistics(traversal))?;
            print(&stats::run_statistics(run))?;
        }
    }
    Ok(())
}
