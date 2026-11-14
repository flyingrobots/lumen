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

Ray statistics cost nothing measurable: the outcomes `render --stats` reports are always
collected, and the render with them took 7.037 s at one thread and 0.547 s at 18 against 6.994 s
and 0.544 s on `main` (1280 by 720, 16 samples, medians of three interleaved runs), the same gap
as between two binaries running identical render code ([ADR 0008](docs/adr/0008-statistics.md)
has the runs).

Row bands with whole-program release builds (`lto = "fat"`, `codegen-units = 1`) against the
tile scheduler on `main` before flyingrobots/lumen#28 (0de5fe7, default release profile), in one
session, interleaved, six runs each (`lumen-reference-scene/3`, 1280 by 720, 16 samples per
pixel, seed 0; render seconds, median; the same image hash in every run):

| Threads | main, tiles | row bands, LTO | Difference |
| ---: | ---: | ---: | ---: |
| 1 | 8.483 | 7.503 | -11.6 % |
| 4 | 2.270 | 1.918 | -15.5 % |
| 18 | 0.661 | 0.623 | -5.8 % |

The gain is fat LTO (Bunny's kernels inline into the render loop), not the change of work unit:
row bands without LTO measured the same as tiles. That was also the cause of the 8 %
multi-thread slowdown after the scheduler was split out in 6a816ef. The sessions, the band-height
sweep and what was ruled out are in [ADR 0003](docs/adr/0003-row-bands.md). Across sessions the 18
thread gain ranged from 6 to 16 %; runs at 18 threads spread the most. The Fixed Point scene at
3840 by 2160 and 64 samples, also interleaved, took 20.734 s against 23.075 s (median of three,
18 threads, -10.1 %), with the same hash.

An earlier session, with the tile scheduler and the default release profile
(`lumen-reference-scene/3`, 1280 by 720, 16 samples per pixel, seed 0, 39 479 111 rays, 32 by 32
tiles; three runs each, render seconds), shows how the render scales with threads:

| Threads | Runs | Mean | Speedup | Per thread |
| ---: | --- | ---: | ---: | ---: |
| 1 | 8.164, 8.130, 8.170 | 8.155 | 1.00 | 1.00 |
| 2 | 4.118, 4.144, 4.066 | 4.109 | 1.98 | 0.99 |
| 4 | 2.064, 2.085, 2.103 | 2.084 | 3.91 | 0.98 |
| 6 | 1.543, 1.472, 1.476 | 1.497 | 5.45 | 0.91 |
| 8 | 1.168, 1.171, 1.177 | 1.172 | 6.96 | 0.87 |
| 12 | 0.853, 0.857, 0.840 | 0.850 | 9.59 | 0.80 |
| 18 | 0.627, 0.624, 0.627 | 0.626 | 13.03 | 0.72 |

Scaling is near linear to 4 threads and falls off after 6. The likeliest reason, not proven
here, is the two core classes: if the VM's busiest threads land on the 6 super cores first, the
threads beyond 6 run on the slower performance cores. Shared memory bandwidth may add to it; the
scene and its tree are small (31 511 triangles) and stay in cache, and the image is written once.

Band heights at 18 threads, with LTO, same image (render seconds, median of five): 1 row: 0.622;
2: 0.598; 4: 0.646; 8: 0.635. Without LTO, in another session: 1: 0.662; 2: 0.667; 4: 0.698;
8: 0.716; 16: 0.746; 45: 1.067. Up to 8 rows the differences are within the noise. From 16 rows
the last bands leave workers idle at the end of the render: a 16-row band is about 190 ms of
one core, and 45 rows make 16 bands for 18 workers. The default is one row: as fast as any, and
the finest grain (2 160 bands at 4K). In the earlier tile session, tiles from 4 by 4 to
64 by 64 and single rows measured the same (0.637 to 0.661 s at 18 threads), and 128 by 128 and
larger were slower for the same reason (0.832 and 1.761 s at 128 and 256).

