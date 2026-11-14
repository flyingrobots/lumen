//! The `lumen` command: argument parsing and I/O around the library.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use lumen::cli::{self, Command, USAGE};
use lumen::png::write_png;
use lumen::{render_reference, BUNNY_REV, REFERENCE_SCENE_ID, SHADING_ID};

fn main() -> ExitCode {
    match cli::parse(std::env::args().skip(1)) {
        Ok(Command::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Command::Version) => {
            println!("lumen {} (bunny {BUNNY_REV})", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Command::Render { width, height, out }) => match run(width, height, &out) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("lumen: {message}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("lumen: {error}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Renders the reference scene, writes the PNG and prints what was made.
fn run(width: u32, height: u32, out: &Path) -> Result<(), String> {
    let started = Instant::now();
    let image = render_reference(width, height).map_err(|error| error.to_string())?;
    let rendered = started.elapsed();
    let written = write_file(out, &image).map_err(|error| format!("{}: {error}", out.display()))?;
    let mut stdout = io::stdout().lock();
    let report = writeln!(
        stdout,
        "scene {REFERENCE_SCENE_ID}\nshading {SHADING_ID}\ntone {}\nbunny {BUNNY_REV}\n\
         size {width}x{height}\nimage {}\npng {} ({written} bytes)\n\
         render-seconds {:.3} (information only)",
        lumen::render::TONE_CURVE.identity(),
        image.hash(),
        out.display(),
        rendered.as_secs_f64(),
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
