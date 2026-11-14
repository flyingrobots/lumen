//! The `lumen` command: argument parsing and I/O around the library.

// The command is the edge outside the deterministic core (docs/adr/0006-nondeterminism-gate.md):
// everything the library may not touch happens here, and none of it reaches a pixel or a hash.
#![expect(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::disallowed_macros,
    reason = "time-instant, host-parallelism, std-env, std-fs, float-type: the command's edge reads \
              its arguments, writes the PNG, times the phases in seconds and asks the host for the \
              default thread count; the library gets only values"
)]

mod cli;
mod failure;
mod report;

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use cli::{Command, USAGE};
use failure::Failure;
use lumen::{
    png_len, render_scheduled, write_png, BuiltinScene, ImageSize, PngError, RenderProvenance,
    RenderSettings, Rgba8Image, Schedule, BUNNY_REV, MAX_THREADS,
};
use report::{Report, Times};

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
        Command::Help => println!("{USAGE}"),
        Command::Version => println!("lumen {} (bunny {BUNNY_REV})", env!("CARGO_PKG_VERSION")),
        Command::Render { scene, width, height, settings, threads, band, out } => {
            let threads = threads.unwrap_or_else(available_threads);
            let schedule = Schedule::new(threads, band).map_err(Failure::Threads)?;
            run(scene, width, height, settings, schedule, out.as_deref())?;
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
) -> Result<(), Failure> {
    let started = Instant::now();
    let size = ImageSize::new(width, height).map_err(Failure::Size)?;
    let built_scene = scene.build().map_err(Failure::Scene)?;
    let built = started.elapsed();
    let render =
        render_scheduled(&built_scene, size, settings, schedule).map_err(Failure::Render)?;
    let rendered = started.elapsed();
    let image = &render.image;
    let hash = image.hash();
    let hashed = started.elapsed();
    let written = match out {
        Some(path) => Some(
            write_file(path, image)
                .map_err(|error| Failure::Png { path: path.to_path_buf(), error })?,
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
    report.write(&mut stdout).map_err(Failure::Stdout)
}

/// Writes the PNG file and returns the number of bytes written to it.
fn write_file(path: &Path, image: &Rgba8Image) -> Result<u64, PngError> {
    let mut file = BufWriter::new(Counting { inner: File::create(path)?, bytes: 0 });
    write_png(&mut file, image)?;
    let counted = file.into_inner().map_err(io::IntoInnerError::into_error)?;
    counted.inner.sync_all()?;
    counted.checked(png_len(image)).map_err(PngError::from)
}

/// A writer that counts the bytes its inner writer accepted.
struct Counting<W> {
    inner: W,
    bytes: u64,
}

impl<W> Counting<W> {
    /// The count of bytes written, if it is the `expected` length of the PNG.
    ///
    /// # Errors
    ///
    /// An `io::Error` naming both counts, if they differ: the file is not the PNG.
    fn checked(&self, expected: u64) -> io::Result<u64> {
        if self.bytes == expected {
            Ok(self.bytes)
        } else {
            Err(io::Error::other(format!("wrote {} bytes where the PNG is {expected}", self.bytes)))
        }
    }
}

impl<W: Write> Write for Counting<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.bytes += u64::try_from(n).unwrap_or(u64::MAX);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use lumen::ImageSize;

    use super::*;

    fn image() -> Rgba8Image {
        let size = ImageSize::new(9, 7).expect("valid");
        Rgba8Image::from_bytes(size, vec![200; size.byte_len()]).expect("length")
    }

    /// A writer that takes at most `chunk` bytes per call, so the count is the sum of short
    /// writes, not one call's length.
    struct Short(Vec<u8>, usize);

    impl Write for Short {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let n = buf.len().min(self.1);
            self.0.extend_from_slice(&buf[..n]);
            Ok(n)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn the_count_is_what_the_writer_accepted_across_short_writes() {
        let image = image();
        let mut counting = Counting { inner: Short(Vec::new(), 5), bytes: 0 };
        write_png(&mut counting, &image).expect("writes");
        assert_eq!(counting.checked(png_len(&image)).expect("matches"), png_len(&image));
        assert_eq!(u64::try_from(counting.inner.0.len()), Ok(counting.bytes));
    }

    #[test]
    fn a_count_that_differs_from_the_expected_length_is_an_error() {
        let image = image();
        let mut counting = Counting { inner: Short(Vec::new(), 64), bytes: 0 };
        write_png(&mut counting, &image).expect("writes");
        let expected = png_len(&image) + 1;
        let error = counting.checked(expected).expect_err("differs").to_string();
        assert_eq!(error, format!("wrote {} bytes where the PNG is {expected}", expected - 1));
    }
}
