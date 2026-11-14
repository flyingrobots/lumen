# ADR 0003: Row bands, results through `join`, and whole-program release builds

Status: accepted (flyingrobots/lumen#28). Supersedes [ADR 0001](0001-tile-scheduling.md).

## Context

ADR 0001 split the image into rectangular tiles (32 by 32 by default). A rectangle is not a
contiguous part of a row-major buffer, so safe Rust cannot prove two tiles disjoint. The design
therefore had each worker render a tile into its own buffer and copy it into the image under a
mutex, and it kept the first failure, a halt reason and the ray totals in a second mutex. The
workers shared two mutexes and three atomics.

When that code was reworked for the engineering standards (commit 6a816ef: the scheduler split
into `schedule.rs` and `workers.rs`, checked arithmetic and slice access, typed errors),
multi-threaded render time rose by about 8 % while one thread stayed the same. The suspects were
the shared state (the copy, the buffer, the two mutexes, counters on one cache line) and code
generation (inlining across the new module boundary).

## Decision

### Work unit: a band of whole rows

`render_pixels` (`crates/lumen/src/workers.rs`) views the image as `&mut [[u8; 4]]` and cuts it
with `chunks_mut(band_rows * width)`. Each chunk is a band of whole rows, contiguous and disjoint
from every other, and the borrow checker proves it. A worker that takes a band owns that slice
and writes each pixel straight into it. There is no per-worker buffer, no copy and no image lock.
The last band is cut to the image. `--band-rows R` (`BandRows`, default 1) sets the height.

### Hand-out: one small lock around the band iterator

`Mutex<Zip<StepBy<Range<u32>>, ChunksMut<[u8; 4]>>>` yields the next band and the index of its
first row, top to bottom. It is held only for that `next()` call: no pixel work, no I/O and no
other lock under it. Whichever worker asks next gets the next band, so hand-out stays dynamic and
the fast and slow cores still balance. It replaces both the atomic tile counter and the image
mutex.

### Results through `join`

Each worker keeps its own ray count and its own earliest failure and returns them as a plain
value. The calling thread is one of the workers. It joins the others and combines all the
outcomes: it adds the counts and keeps the failure with the smallest `(y, x)`. Integer sums and a
minimum do not depend on order, so the totals and the chosen error are functions of the scene and
settings. A panic in the pixel function halts the other workers (see "A panic halts the other
workers" below) and is passed on with `resume_unwind` after the join. The record mutex is gone.

### The one shared written value: the first row no longer wanted

`end: AtomicU32` starts at the image height. A worker begins row `y` only if `y < end`, and takes
a band only if its first row is below `end`. A failure in row `y` lowers `end` to `y`
(`fetch_min`). A halt sets it to 0: when a worker cannot be started the render returns
`WorkerError::Spawn` with the worker's number and the operating system's error, and when a
worker panics the panic is passed on. Threads are
started through a spawner the unit tests replace, to refuse a chosen worker on real threads. Relaxed ordering is enough. `end` only tells a worker when it may stop
early. Every value it takes is a failing row or 0. The results reach the caller through `join`,
which orders them.

So the shared mutable state went from two mutexes and three atomics to one mutex and one atomic.
No worker writes memory that another worker reads, except `end`.

### Why the reported error is the raster-first failure

Let `F = (xf, yf)` be the first failing pixel in raster order. Every value `end` takes is the row
of a failure, which is at least `yf`, or 0 after a spawn failure (then the error is `Spawn`).

- The band holding row `yf` is handed out. Bands go out in order, and a band is refused only when
  its first row is at least `end`. That first row is at most `yf`, and `end` is at least `yf`
  until `F` itself is found. Until then, `end` is greater than `yf`.
- Row `yf` is begun. Its worker checks `yf < end`, which holds until `F` is found. Rows above
  `yf` in the band have no failure, so the worker reaches row `yf`.
- The worker renders row `yf` from the left, so it meets `F` before any later pixel of that row.
  Only this worker renders row `yf`.
- Every other failure found is later in raster order. Taking the minimum `(y, x)` over all
  workers gives `F`, at every thread count, band height and hand-out order.

### Why work past the failure stops

Once the failure in row `y` is recorded, `end` is at most `y`. No worker begins a row from `y`
down, and none takes another band below it. A row that is already in progress when the failure is
recorded finishes. Rows above `y` still run, because an earlier failure may be there. That is
tighter than ADR 0001's rule, which kept rendering to the end of the failing row of tiles: a row
belongs to one worker, so the failing row needs nothing more.

### A panic halts the other workers

Each worker owns a guard that calls the halt when it is dropped while its thread unwinds
(`std::thread::panicking()`). So a panic in the pixel function sets `end` to 0 as it leaves the
worker, and the guarantee is the halt's: a worker that has seen the halt begins no further row,
and takes no further band; a row it has already begun is finished. This is not a real-time claim.
With relaxed ordering a worker may read the old `end` for a while, and how long depends on the
operating system's scheduling, so on real threads the work done after a panic is not bounded by a
number. The panic reaches the caller through `resume_unwind` once every worker has stopped: the
scope joins every started thread before passing on a panic. No image is returned.

Before this guard, the other workers rendered every row still to be handed out before the panic
reached the caller. On a 64 by 512 image with one-row bands and a panic at `(0, 0)`, 32 705 of
32 768 pixels were computed at 2 and 4 workers, under every hand-out order the simulator tried
(flyingrobots/lumen#56). With the guard, no pixel is computed after the panic in the simulator,
where a row is one step and the halt is seen at the next step.

The halt on a refused spawn and the halt on a panic set the same value, and both are checked in
the simulator through the code the threads run: `start_helpers` (the spawn sequence) driven by a
simulated start that refuses one helper after the others have run, and `Worker::step` driven
under `catch_unwind`, which drops the worker as the panic unwinds.

### Whole-program release builds

`[profile.release]` sets `lto = "fat"` and `codegen-units = 1`. Fat LTO lets Bunny's kernels
inline into lumen's render loop across the crate boundary. That is what the 6a816ef slowdown
turned out to be (see Measurements). The arithmetic is integer-only, so no pixel changes, and the
image hashes are equal in every run below.

### `--tile` is removed

A tile no longer describes the work unit, and keeping `--tile` with another meaning would hide
that. lumen is unreleased, so `--tile N | WxH`, `TileSize`, `DEFAULT_TILE`,
`ScheduleError::EmptyTile`, `UsageError::InvalidTile` and the `TileGeometry` errors are removed.
`--band-rows R`, `BandRows`, `DEFAULT_BAND_ROWS` and `ScheduleError::EmptyBand` replace them. The
report prints `band-rows R` in place of `tile WxH`. `--tile` is now refused as an unknown
option.

## Measurements

All runs used the `rust-dev:1.96.0` container on the Apple M5 Pro, through `devrun --exclusive
--cpus 18 --memory 12g`, in the release profile. Each session built each revision from freshly
checked-out sources, then interleaved the revisions in a rotating order (`lumen render --width
1280 --height 720 --samples 16 --threads T`). Times are render seconds; the tables give the
median and its difference from the first row. The sessions differ in machine state by a few
percent, so compare within a session only. In every session, every revision gave the same image
hash.

**1. Reproduction** (scene version 2, release profile without LTO, six runs each):

| Threads | 0fc51d7 (before the split) | 6a816ef (after) | Difference |
| ---: | ---: | ---: | ---: |
| 1 | 7.190 | 7.185 | -0.1 % |
| 4 | 1.907 | 2.074 | +8.7 % |
| 18 | 0.572 | 0.620 | +8.5 % |

**2. Row bands against the tile design on main** (scene version 3, without LTO, six runs):

| Threads | main (00b04b2, tiles) | bands (2cab5a1) | Difference |
| ---: | ---: | ---: | ---: |
| 1 | 8.216 | 8.402 | +2.3 % |
| 4 | 2.210 | 2.189 | -0.9 % |
| 18 | 0.667 | 0.690 | +3.3 % |

The differences are within the spread of the runs: from 2.145 to 2.251 s for main at 4 threads,
and from 0.637 to 0.746 s at 18. Removing the copy, the buffer and both mutexes together made
no measurable difference. None of them can account for the 8 %, so they were not restored one
at a time.

**3. Code generation, on the reproduction pair** (five runs). `lto = "fat"` with
`codegen-units = 1` was applied to both commits:

| Threads | 0fc51d7 | 6a816ef | 0fc51d7, LTO | 6a816ef, LTO |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 7.894 | 7.692 | 6.434 | 6.416 |
| 4 | 2.039 | 2.200 (+7.9 %) | 1.680 | 1.728 (+2.9 %) |
| 18 | 0.629 | 0.675 (+7.3 %) | 0.547 | 0.528 (-3.5 %) |

With whole-program optimization, both commits are 13 to 19 % faster and the gap between them is
gone: what remains (+2.9 % and -3.5 %) has no consistent sign and is within noise. The rework
did not add work. It changed which Bunny functions the compiler inlined into the default 16
codegen units, which costs more on the threaded runs. Why only those runs pay it was not
established.

**4. Which setting does it** (bands at 79d8641, five runs):

| Threads | main | bands | bands, `codegen-units = 1` | bands, fat LTO | bands, both |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 9.064 | 8.632 | 8.843 | 7.638 | 7.415 |
| 4 | 2.296 | 2.366 | 2.339 | 1.924 | 1.955 |
| 18 | 0.718 | 0.701 | 0.738 | 0.634 | 0.601 |

Fat LTO accounts for the gain, and one codegen unit on its own does nothing. Both together are
kept: at 18 threads they were the fastest of the four.

**5. Band height** (bands at 2cab5a1, without LTO, five runs, medians):

| Rows | 1 | 2 | 4 | 8 | 16 | 45 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 4 threads | 2.240 | 2.236 | 2.287 | 2.301 | 2.289 | 2.416 |
| 18 threads | 0.662 | 0.667 | 0.698 | 0.716 | 0.746 | 1.067 |

One row is the default: it is as fast as any height and has the finest grain. A band is as wide
as the image, so its cost is the sum across the row. That matters only at the end of a render,
where the last bands decide when the slowest worker finishes. One row of 1280 pixels is about
the work of 1.25 of the old 32 by 32 tiles. At one thread a row takes about 12 ms, which is less
than 2 % of an 18-thread render. Taller bands lose for exactly this reason. At 16 rows, a band
takes about 190 ms of one core and the render is 12.7 % slower. At 45 rows there are only 16
bands for 18 workers.

**6. Before and after** (main at 0de5fe7, with tiles and the default profile, against row
bands with fat LTO and one codegen unit at c731bb4; six runs):

| Threads | main | row bands, LTO | Difference |
| ---: | ---: | ---: | ---: |
| 1 | 8.483 | 7.503 | -11.6 % |
| 4 | 2.270 | 1.918 | -15.5 % |
| 18 | 0.661 | 0.623 | -5.8 % |

In the same session, the band-height check with LTO at 18 threads gave these medians: 1 row
0.622, 2 rows 0.598, 4 rows 0.646 and 8 rows 0.635. That is within noise, so the default stays
at one row. The 4K renders are recorded in `docs/measurements/2026-11-13-4k-row-bands.md`. The reference
scene's hash was unchanged, and so was Fixed Point's at 64 samples. Fixed Point was 10.1 %
faster at 18 threads, interleaved against `main` (median 20.734 s against 23.075 s).

## Evidence for correctness

The unit tests in `crates/lumen/src/workers/tests.rs` run simulated workers on one thread. They
run the threads' own `Worker::step` and `combine`, in orders chosen by 200 seeds and by scripted
worst cases: one worker takes everything, and bands finish in reverse. The image is 13 by 9, and
the band heights are 1, 2, 3, 4, 5, 9 and 20 rows. The tests check the image, the pixel count,
the chosen error and the stop rule against a raster-order loop. They also check the halt: after a
halt at any step, after a helper is refused during `start_helpers`, and after a panic at `(0, 0)`
of a 64 by 512 image (the panicking worker moving first, after 100 rows of another, last, and
under 40 seeded orders), no worker computes a pixel. The pixels computed are then exactly the
panicking one plus 64 for each whole row rendered before it.
`crates/lumen/tests/threads.rs` checks real threads at fixed counts, including the golden hash at
1, 2, 3, 4, 7 and 18 threads across ten band heights. Five mutations each fail tests: a band
written at the wrong row offset, the last partial band dropped, the minimum failure chosen by
`(x, y)`, one worker's ray count dropped, and the stop row ignored. So do three more: the guard's
halt removed, the halt on a refused spawn removed, and the guard halting on every drop. Ignoring
`end` when a band is taken is an equivalent mutant: the band's first row is checked against the
same value before any pixel of it, so it costs one step and computes nothing.

## Rejected

- **Keeping tiles.** Tiles needed the buffer, the copy and the image mutex only because a
  rectangle is not a slice. They bought nothing measurable: whole rows were already as fast in
  ADR 0001's tile sweep, and again in measurement 2.
- **Lock-free hand-out.** An atomic counter cannot hand out `&mut` slices in safe Rust. The safe
  versions would index a pre-split `Vec` of per-band `Mutex<Option<&mut [_]>>` slots, which is
  still a lock per band, and `unsafe` is forbidden. With one-row bands, a 1280 by 720 render
  takes the lock 720 times. That cost was not measured separately; measurement 2 shows the
  design as a whole is no slower than the counter.
- **A static split** (one band per thread): the fast and slow cores and the uneven cost of
  regions would leave cores idle (measurement 5, 45 rows).
- **`--tile` kept with a new meaning**: an option whose name says something other than what it
  does.
- **Unchecked indexing in the pixel loop**: not needed. The loop zips column and row ranges with
  slice iterators, so there is nothing to index and no offset arithmetic. Checked indexing was
  never the cause (measurement 3).
- **Padding the shared counters onto separate cache lines**: after this change, the only shared
  values are the hand-out lock, taken once per band, and `end`, read once per row. Measurement 3
  explains the gap without false sharing, so padding was not tried.
- **A thread-pool crate** (rayon): as in ADR 0001, a build and supply-chain cost for a mutex and a
  scope.
