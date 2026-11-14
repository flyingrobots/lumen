//! Renders the Fixed Point scene (`lumen-fixed-point-scene/1`) to a PNG file through the library
//! alone, without the CLI (`lumen render --scene fixed-point` is the command's way). The scene and
//! its identity come from `BuiltinScene::FixedPoint`, the same registry entry the command uses.
//!
//! ```sh
//! cargo run --release --locked -p lumen --example fixed_point -- OUT [WIDTH HEIGHT [SAMPLES [SEED [THREADS]]]]
//! ```
//!
//! The defaults are 480 by 270 pixels, 16 samples per pixel, seed 0 and as many threads as the
//! host reports (`std::thread::available_parallelism`; the library itself asks the host
//! nothing, so this program passes the count). The image does not depend on the thread count.
//! It prints the scene identity, the settings, the rays traced, the image hash and the render
//! time.

#![expect(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "time-instant, host-parallelism, std-env, std-fs: a front end like the command, \
              outside the deterministic core"
)]

use std::fs::File;
use std::io::{BufWriter, Write};
use std::process::ExitCode;
use std::time::Instant;

use lumen::DEFAULT_BAND_ROWS;
use lumen::{render_scheduled, write_png, BuiltinScene, ImageSize, RenderSettings, Schedule};

const USAGE: &str = "usage: fixed_point OUT [WIDTH HEIGHT [SAMPLES [SEED [THREADS]]]]";

/// The output path, the size, the samples per pixel, the seed and the thread count.
struct Args {
    out: String,
    size: (u32, u32),
    samples: u32,
    seed: u32,
    threads: u32,
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let out = args.next().ok_or_else(|| USAGE.to_owned())?;
    let mut number = |default: u32| -> Result<u32, String> {
        args.next().map_or(Ok(default), |text| text.parse().map_err(|_| format!("{USAGE}: {text}")))
    };
    let size = (number(480)?, number(270)?);
    let host =
        std::thread::available_parallelism().map_or(1, |n| u32::try_from(n.get()).unwrap_or(1));
    Ok(Args { out, size, samples: number(16)?, seed: number(0)?, threads: number(host)? })
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let size = ImageSize::new(args.size.0, args.size.1).map_err(|e| e.to_string())?;
    let settings = RenderSettings::new(args.samples, args.seed).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let scene = BuiltinScene::FixedPoint.build().map_err(|e| e.to_string())?;
    let schedule = Schedule::new(args.threads, DEFAULT_BAND_ROWS).map_err(|e| e.to_string())?;
    let rendered = render_scheduled(&scene, size, settings, schedule).map_err(|e| e.to_string())?;
    let seconds = started.elapsed().as_secs_f64();
    let file = File::create(&args.out).map_err(|e| format!("{}: {e}", args.out))?;
    let mut writer = BufWriter::new(file);
    write_png(&mut writer, &rendered.image).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    println!("scene {}", BuiltinScene::FixedPoint.id());
    println!("triangles {}", scene.triangles().len());
    println!("size {}x{}", size.width(), size.height());
    println!("samples {}", settings.samples_per_pixel());
    println!("seed {}", settings.seed());
    println!("threads {}", args.threads);
    println!("rays {}", rendered.rays.total());
    println!("image {}", rendered.image.hash());
    println!("png {}", args.out);
    println!("render-seconds {seconds:.2} (information only)");
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("fixed_point: {message}");
            ExitCode::FAILURE
        }
    }
}
