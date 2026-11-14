//! Hand-out orders explored on one thread, with no clock and no real threads: simulated workers
//! advance one step at a time (claim a tile, render one row, store the tile) through the same
//! `claim`, `render_row`, `store`, `add` and `finish` the threads run, in an order chosen by a
//! policy: a seeded generator, or a scripted worst case. Every failure message names the policy,
//! so any order can be replayed. A step budget, not a timeout, ends a simulation that does not
//! finish.

use super::*;
use crate::schedule::TileSize;

/// A pixel's coordinates, also the simulated pixel function's error.
type Coord = (u32, u32);
/// What the simulated pixel function returns.
type PixelResult = Result<([u8; 4], u64), Coord>;
/// What a simulated render returns: the image bytes and the pixel count, or why it stopped.
type Outcome = Result<(Vec<u8>, u64), WorkerError<Coord>>;
/// Failing pixels and the one whose error must be reported.
type Case<'a> = (&'a [Coord], Coord);

/// Which simulated worker moves next.
#[derive(Clone, Copy, Debug)]
enum Policy {
    /// Drawn from a xorshift generator with this seed.
    Seeded(u64),
    /// Always the first worker still running: one worker takes everything it can.
    First,
    /// Idle workers claim first; then the worker holding the highest tile moves, so tiles finish
    /// in reverse order and the earliest tile (and its failure) comes last.
    HighestTileFirst,
}

/// A simulated worker: idle, or partway through a tile.
struct Sim {
    lane: Lane<u64>,
    job: Option<(Job, u32)>,
    done: bool,
}

/// The pixel function under test: a colour from the coordinates, the value 1 per pixel, and a
/// failure (the pixel's own coordinates) at each pixel of `failing`.
fn pixel(failing: &[Coord]) -> impl Fn(u32, u32) -> PixelResult + '_ {
    move |x, y| {
        if failing.contains(&(x, y)) {
            return Err((x, y));
        }
        Ok(([x.to_le_bytes()[0], y.to_le_bytes()[0], 9, 255], 1))
    }
}

/// The next worker to move under `policy`, or `None` when all are done.
fn pick(sims: &[Sim], policy: Policy, rng: &mut u64) -> Option<usize> {
    let live: Vec<usize> = (0..sims.len()).filter(|&n| !sims[n].done).collect();
    let first = *live.first()?;
    Some(match policy {
        Policy::First => first,
        Policy::Seeded(_) => {
            *rng ^= *rng << 13;
            *rng ^= *rng >> 7;
            *rng ^= *rng << 17;
            let count = u64::try_from(live.len()).expect("few workers");
            live[usize::try_from(*rng % count).expect("below the count")]
        }
        Policy::HighestTileFirst => {
            let idle = live.iter().copied().find(|&n| sims[n].job.is_none());
            idle.unwrap_or_else(|| {
                let tile = |n: usize| sims[n].job.map_or(0, |(job, _)| job.index);
                live.iter().copied().max_by_key(|&n| tile(n)).expect("live")
            })
        }
    })
}

/// Moves worker `sim` one step.
fn step(shared: &Shared<'_, u64, Coord>, sim: &mut Sim, pixel: &impl Fn(u32, u32) -> PixelResult) {
    match sim.job {
        None => match shared.claim() {
            None => {
                shared.add(sim.lane.total);
                sim.done = true;
            }
            Some(job) => {
                sim.lane.buffer.clear();
                sim.job = Some((job, job.rect.y0));
            }
        },
        Some((job, y)) if y == job.rect.y1 => {
            assert!(shared.store(job, &sim.lane).is_some(), "tile {} does not fit", job.index);
            sim.job = None;
        }
        Some((job, y)) => {
            let row = shared.render_row(job, y, pixel, &mut sim.lane);
            sim.job = (row == Row::Done).then_some((job, y + 1));
        }
    }
}

/// Runs `workers` simulated workers over a 13 by 9 image in `tile` tiles in the order `policy`
/// chooses, and returns what `render_pixels` would.
fn simulate(tile: TileSize, workers: usize, policy: Policy, failing: &[Coord]) -> Outcome {
    let size = ImageSize::new(13, 9).expect("size");
    let mut bytes = blank(size)?;
    let tiles = Tiles::new(size, tile).expect("tiles");
    let shared = Shared::new(tiles, &mut bytes, 0_u64);
    let pixel = pixel(failing);
    let mut sims: Vec<Sim> = (0..workers)
        .map(|_| Sim { lane: Lane { buffer: Vec::new(), total: 0 }, job: None, done: false })
        .collect();
    let mut rng = match policy {
        Policy::Seeded(seed) => seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
        _ => 1,
    };
    // At most 117 tiles, each claimed once, each of its at most 9 rows rendered once, stored
    // once; each worker also makes one last claim. More steps mean a scheduler that does not end.
    let budget = 117 * (9 + 2) + workers;
    for _ in 0..budget {
        let Some(n) = pick(&sims, policy, &mut rng) else {
            let total = finish(shared)?;
            return Ok((bytes, total));
        };
        step(&shared, &mut sims[n], &pixel);
    }
    panic!("{policy:?}: no end within {budget} steps");
}

/// The image without any scheduling: every pixel in raster order.
fn raster() -> Vec<u8> {
    let pixel = pixel(&[]);
    let mut bytes = Vec::new();
    for y in 0..9 {
        for x in 0..13 {
            bytes.extend_from_slice(&pixel(x, y).expect("no failure").0);
        }
    }
    bytes
}

fn tile(width: u32, height: u32) -> TileSize {
    TileSize::new(width, height).expect("tile")
}

/// Every policy tried: the scripted worst cases and 200 seeds.
fn policies() -> impl Iterator<Item = Policy> {
    [Policy::First, Policy::HighestTileFirst].into_iter().chain((0..200).map(Policy::Seeded))
}

const TILES: [(u32, u32); 5] = [(1, 1), (4, 3), (5, 9), (13, 1), (20, 20)];
const WORKERS: [usize; 5] = [1, 2, 3, 7, 200];

/// Oracle: the raster-order loop above. Under every policy, 1 to 200 workers (200 is more than
/// there are tiles of 4 by 3, so every tile is in flight at once and `HighestTileFirst` finishes
/// them in exact reverse order) and five tile sizes, the image equals it byte for byte and every
/// pixel is counted once.
#[test]
fn every_hand_out_order_gives_the_raster_image() {
    let want = raster();
    for (tw, th) in TILES {
        for workers in WORKERS {
            for policy in policies() {
                let got = simulate(tile(tw, th), workers, policy, &[]);
                assert_eq!(
                    got,
                    Ok((want.clone(), 13 * 9)),
                    "{policy:?}, {workers} workers, {tw}x{th}"
                );
            }
        }
    }
}

/// Oracle: the failing pixel that comes first in raster order, by inspection of each case. It
/// is found under every policy, also when a later tile fails first: `HighestTileFirst` makes
/// the earliest failing tile the last one rendered.
#[test]
fn every_hand_out_order_reports_the_raster_first_failure() {
    let cases: [Case<'_>; 4] = [
        (&[(12, 8)], (12, 8)),
        (&[(2, 5), (9, 0)], (9, 0)),
        (&[(0, 8), (12, 7), (6, 7)], (6, 7)),
        (&[(4, 4), (3, 4), (8, 4)], (3, 4)),
    ];
    for (failing, (x, y)) in cases {
        for (tw, th) in TILES {
            for workers in WORKERS {
                for policy in policies() {
                    let got = simulate(tile(tw, th), workers, policy, failing);
                    let want = Err(WorkerError::Pixel { x, y, error: (x, y) });
                    assert_eq!(got, want, "{policy:?}, {workers} workers, {tw}x{th}, {failing:?}");
                }
            }
        }
    }
}
