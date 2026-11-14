# Measure render time

Use this procedure to time a render in a way that can be compared with a recorded reading or a
target such as flyingrobots/lumen#5's 60 seconds at 4K. A timing is information about one run on
one machine; it never enters an image, a hash or a test.

## Prerequisites

- The container runner, `devrun` (`~/git/devcontainers/bin/devrun`), as for every build in this
  repository ([contributing](../../README.md#build-and-test)). Timings taken outside it are not
  comparable with the records.
- No other run on the machine: pass `--exclusive`, which waits until no other `devrun` run is
  active and keeps the others out. A timing taken without it is not evidence.

## How the machine shapes a timing

Timings are information, never part of an image, and never taken from a run that shared the
machine: every timing below ran in the `rust-dev:1.96.0` container through `devrun` with
`--exclusive`, release profile. The Docker VM on the Apple M5 Pro used here has 18 CPUs (the
host has 6 "super" and 12 "performance" cores); `devrun` limits a run to 4 CPUs unless told
otherwise, so a threaded timing must raise the limit. `--cpus` is a CPU-time quota over all 18 VM
CPUs, not a choice of cores: a run cannot be pinned to the fast cores.

## Reproduce

```sh
D=~/git/devcontainers/bin/devrun
# the 4K reference render on all 18 VM CPUs; prints every part of the time and the hash
python3 $D lumen ~/git/lumen --commit HEAD --fetch --exclusive --cpus 18 --memory 12g -- \
  bash -c 'cargo build --release --locked -q && mkdir -p /scratch/out && \
    /target/release/lumen render --width 3840 --height 2160 --samples 16 --out /scratch/out/4k.png'
# thread scaling on a mid-size image
python3 $D lumen ~/git/lumen --commit HEAD --exclusive --cpus 18 --memory 12g -- \
  bash -c 'cargo build --release --locked -q && for t in 1 2 4 6 8 12 18; do \
    /target/release/lumen render --width 1280 --height 720 --samples 16 --threads $t | grep -E "^(threads|render-seconds)"; done'
```

Add `--export 4k.png --out "$(mktemp -d)"` to take the PNG out of the container. The interval to
compare with a wall-clock target such as flyingrobots/lumen#5's 60 seconds is `total-seconds`:
the whole command after argument parsing, that is scene build, render, hashing and writing the
PNG; the report shows each part. `rays-per-second` and `ns-per-ray` divide the render's wall time
by the rays traced on all threads, so they are not CPU time per ray; the single-thread figure is a
run with `--threads 1`.

## Verify the run

- Every run of the same command at the same commit prints the same `image` line; a different hash
  means a determinism bug, not noise ([how Lumen renders](../explanation.md#determinism)).
- Compare `render-seconds` or `total-seconds` only with readings taken on the same machine, in
  the same container image, with the same `--cpus`. Interleave the revisions you compare and
  take several runs each; one run is a reading, not a result.

## Record it

A result worth keeping becomes a dated record under [`docs/measurements/`](../measurements/):
the commit, the machine, the container, the limits, the command, every run's figures and the
image hash, as the existing records do. A record states its conditions and applies to its commit
only; it is never edited to describe a later one.

- [The 4K reference scene at 16 samples per pixel, 2026-11-13](../measurements/2026-11-13-4k-reference.md)
  (flyingrobots/lumen#5), taken with the tile scheduler.
- [The 4K reference scene with row bands and whole-program release builds, 2026-11-13](../measurements/2026-11-13-4k-row-bands.md)
  (flyingrobots/lumen#28).
- [Fixed Point version 2 at 4K and 64 samples per pixel, 2026-11-14](../measurements/2026-11-14-4k-fixed-point-v2.md)
  (flyingrobots/lumen#39).
- [The canyon scene at 4K and 16 samples per pixel, 2026-11-14](../measurements/2026-11-14-canyon.md)
  (flyingrobots/lumen#40).
- [The readings the README held, 2026-11-13](../measurements/2026-11-13-readme-readings.md):
  tree builders and layouts, scene versions per ray, thread scaling and tile sizes.

## Other readings

Every other timing lives in one dated place, with its conditions; none is repeated here:

- the cost of ray and run statistics: [ADR 0008](../adr/0008-statistics.md), "Ray statistics"
  and "Run statistics";
- row bands against tiles, fat LTO, the band-height sweep and the Fixed Point 4K comparison at
  64 samples: [ADR 0003](../adr/0003-row-bands.md), "Measurements".
