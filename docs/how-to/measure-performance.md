# Measure performance

## Performance

Timings are information, never part of an image, and never taken from a run that shared the
machine: every timing below ran in the `rust-dev:1.96.0` container through `devrun` with
`--exclusive`, release profile. The Docker VM on the Apple M5 Pro used here has 18 CPUs (the
host has 6 "super" and 12 "performance" cores); `devrun` limits a run to 4 CPUs unless told
otherwise, so a threaded timing must raise the limit. `--cpus` is a CPU-time quota over all 18 VM
CPUs, not a choice of cores: a run cannot be pinned to the fast cores.

### Reproduce

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

### Records

Dated measurement records are kept under [`docs/measurements/`](docs/measurements/). The record for
flyingrobots/lumen#5 is [the 4K reference scene at 16 samples per pixel, 2026-11-13](docs/measurements/2026-11-13-4k-reference.md),
taken with the tile scheduler. [The 4K reference scene with row bands and whole-program release
builds](docs/measurements/2026-11-13-4k-row-bands.md) repeats it for flyingrobots/lumen#28.
[Fixed Point version 2 at 4K and 64 samples per pixel, 2026-11-14](docs/measurements/2026-11-14-4k-fixed-point-v2.md)
records the showpiece for flyingrobots/lumen#39: the same image hash,
`sha256:d276602407edb72109ebfbc8cec9121230171cf156144f139fb2bc3ca1c13bef`, on every run, a mean
of 21.2 seconds with 18 CPUs, about 10 % more than version 1 (19.2 seconds).
Each states its commit and conditions and applies to that commit only.

### Readings

Every timing lives in one dated place, with its conditions; none is repeated here:

- the cost of ray and run statistics: [ADR 0008](../adr/0008-statistics.md), "Ray statistics"
  and "Run statistics";
- row bands against tiles, fat LTO, the band-height sweep and the Fixed Point 4K comparison at
  64 samples: [ADR 0003](../adr/0003-row-bands.md), "Measurements";
- the four tree and layout combinations, the reference scene's versions 2 and 3 per ray, thread
  scaling with the tile scheduler and the tile-size sweep, which the README used to hold:
  [the README readings record](../measurements/2026-11-13-readme-readings.md);
- the 4K renders: the records under [`docs/measurements/`](../measurements/), above.
