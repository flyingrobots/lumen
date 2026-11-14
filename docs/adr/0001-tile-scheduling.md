# ADR 0001: Tile scheduling on worker threads

Status: superseded by [ADR 0003](0003-row-bands.md) (flyingrobots/lumen#28). Accepted in
flyingrobots/lumen#4. The text below is the decision as it was taken; the code no longer has
tiles, the tile buffer, the image mutex or the record mutex.

## Context

The 4K reference render (3840 by 2160 at 16 samples per pixel) takes about 64 s on one core of
the Apple M5 Pro, and the goal in flyingrobots/lumen#5 is under 60 s. The machine has 6 fast and
12 slower cores. Lumen's promise is that the image hash depends only on the scene and the
settings, so threading must not change a single byte.

## Decision

The image is split into rectangular tiles (32 by 32 pixels by default), numbered row-major from
the top left (`crates/lumen/src/schedule.rs`). `render_pixels` (`crates/lumen/src/workers.rs`)
runs a pixel function over them:

- **Hand-out**: an atomic counter gives the next tile index to whichever worker asks. Indices
  are handed out in increasing order; which worker gets which, and when each finishes, is left to
  the operating system. The calling thread is one of the workers, so one thread is the same code
  path with no thread started.
- **What is shared**: the scene and the pixel function by shared reference (`Scene` is `Sync` as
  it is, checked at compile time); the counter; an atomic "last tile still wanted"; and two
  mutexes, never held together: the image (held only to copy one finished tile) and a record of
  the first failure, a halt reason and the summed ray counts. No I/O and no pixel work happens
  under a lock.
- **What is not shared**: each worker renders a tile into its own buffer and sums its own ray
  count. Every pixel's samples go into that pixel's own `PixelAccumulator`.
- **Threads**: `std::thread::scope` and the standard library only. The library takes an explicit
  thread count; only the `lumen` binary asks the host (`available_parallelism`).

### Why the image cannot depend on scheduling

A pixel's bytes are computed by exactly one worker from its coordinates alone, with no state
carried between pixels, and are copied to an offset fixed by those coordinates. Ray counts are
integer sums, which do not depend on order. So thread count, tile size, hand-out order and timing
choose only who computes a pixel and when.

### How the first error is chosen

The error returned is that of the first failing pixel in raster order (top row first), which is
what a one-pixel-at-a-time raster loop would return, at every thread count and tile size. When a
pixel in tile `k` fails, every pixel before it in raster order lies in the same row of tiles or an
earlier one, so the "last wanted" tile drops to the end of that row of tiles: workers take no tile
after it and abandon one they hold at its next row. Tiles up to it still run, since an earlier
failure may hide in them, and the record keeps the failure with the smallest `(y, x)`.

## Evidence

Unit tests drive simulated workers on one thread through the same claim, render-row, store and
finish steps, in orders chosen by a seeded generator and by scripted worst cases (one worker
takes everything; tiles finish in reverse order, so the earliest failing tile finishes last), and
compare the image and the error with a raster-order loop. Integration tests run real threads at
fixed counts. Six mutations of the scheduler (wrong tile offset, a dropped tile, a dropped partial
row of tiles, an accumulator shared between pixels, stopping at the failing tile instead of its
row's end, column-major choice of the first failure) each fail tests.

## Rejected

- **A thread-pool crate (rayon)**: a build and supply-chain cost for a counter and a scope.
- **A static split** (one band of rows per thread): the fast and slow cores and the uneven cost of
  regions would leave cores idle.
- **Workers writing straight into the image** without a lock: needs `unsafe` for disjoint
  rectangles, which the workspace forbids. Row bands from `chunks_mut` would avoid it but fix the
  tile shape to whole rows; measured, whole-row tiles were no faster than 32 by 32.
- **The error of the lowest-numbered failing tile**: deterministic for a fixed tile size but not
  across tile sizes, unlike the raster-first pixel.
- **Accumulating samples of one pixel on several threads**: would need a merge order; the pixel is
  the unit of work instead.
