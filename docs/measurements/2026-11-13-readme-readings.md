# Measurement record: the readings the README held, 2026-11-13

This is a record of readings taken on 2026-11-13 and kept in the README until 2026-11-14, when
they moved here unchanged so that every timing lives in one dated place. They were not
re-measured. Each describes the commit it was taken at, not any later one. The README did not
name the commit measured; each section names the commit that added the reading, and the code it
measured is that commit's parent or the commit itself. The conditions are the README's: the
Apple M5 Pro, the `rust-dev:1.96.0` Linux container through `devrun --exclusive`, release
profile, on a Docker VM with 18 CPUs.

Readings taken later are in [ADR 0003](../adr/0003-row-bands.md) (row bands, fat LTO, band
heights) and [ADR 0008](../adr/0008-statistics.md) (the cost of statistics). How to take new
ones is in [measure performance](../how-to/measure-performance.md).

## Tree builders and layouts

Added in `cebcf7f` (parent `3a81d82`), on `lumen-reference-scene/2`, by
`crates/lumen/examples/traversal.rs` (`160 90 4 7`).

One reading of that command (Apple M5 Pro, the `rust-dev:1.96.0` container, run alone with
`--exclusive`, release profile; the median of 7 rounds over 150 726 rays, every ray including its
share of sampling and shading), for information:

| Tree and layout | ns per ray | Build |
| --- | ---: | ---: |
| `build_bvh`, `closest_hit` / `any_hit` | 243.7 | 99.7 ms |
| `build_bvh_sah`, `closest_hit` / `any_hit` | 225.9 | 17.2 ms |
| `build_bvh`, compact layout | 205.5 | 99.7 ms |
| `build_bvh_sah`, compact layout (lumen) | 192.5 | 17.2 ms |

The table was measured on `lumen-reference-scene/2`.

## The reference scene's versions 2 and 3, per ray

Added in `472f8ef` (parent `1e6c6bb`), when the Stanford bunny joined the scene.

The 640 by 360 render at 64 samples per
pixel, in the same setting (the median of three runs of `lumen render`, one thread, all work
included):

| Scene | Triangles | Rays | Time | ns per ray |
| --- | ---: | ---: | ---: | ---: |
| `lumen-reference-scene/2` | 27 660 | 38 588 953 | 7.18 s | 186.1 |
| `lumen-reference-scene/3` | 31 511 | 39 478 487 | 7.90 s | 200.1 |

Version 3 traces 2.3 % more rays (the moved camera sees more ground and less sky) and each ray
costs about 7.5 % more, about 10 % more time in all. The scene has 14 % more triangles, and the
bunny's are small and dense; this measurement does not separate the bunny's cost from the
camera's.

## Thread scaling with the tile scheduler

Added in `2d4c048` (parent `456e32c`), before row bands (ADR 0003) and before fat LTO.

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

## Tile sizes

Added in the same commit, at 18 threads with the tile scheduler and the default release profile
(1280 by 720, 16 samples): tiles from 4 by 4 to
64 by 64 and single rows measured the same (0.637 to 0.661 s at 18 threads), and 128 by 128 and
larger were slower, because the last tiles left workers idle at the end of the render (0.832 and
1.761 s at 128 and 256).
Row bands later replaced tiles; ADR 0003's measurement 5 sweeps band heights.
