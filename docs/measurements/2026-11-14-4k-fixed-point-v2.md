# Measurement record: 4K Fixed Point, version 2, at 64 samples per pixel, 2026-11-14

This is a record of one session. It describes what was measured on the date above, at the
commits below. It is not a statement about any later commit. To repeat it, see the command
below or `make 4k-fixed-point` (which runs the same render with 8 CPUs by default; pass
`CPUS=18 MEMORY=12g`).

It answers flyingrobots/lumen#39: the showpiece rendered at 4K and 64 samples through the
command, with its image hash recorded, the same on every run; and what the full-resolution,
smooth-shaded bunny costs against version 1.

## Conditions

| Item | Value |
| --- | --- |
| Commit | `6ef26067c15e` (the head of pull request 125, merged into `main` as `26c9093`) |
| Machine | Apple M5 Pro: 6 "super" and 12 performance cores |
| Where it ran | the `rust-dev:1.96.0` Linux container, through `devrun`, on a Docker VM with 18 CPUs; not native macOS |
| Profile | release (`cargo build --release --locked`) |
| Isolation | every run used `devrun --exclusive`, so no other run shared the machine |
| Limits | `--cpus 18 --memory 12g` |
| Scene | `lumen-fixed-point-scene/2`, 71 575 triangles (69 451 of them the smooth bunny) |
| Scene content | `sha256:f80160bbd1ebb6e79f186bf5a86f2ce3419d4ef31168cc4cab0b6fd1f0f64e11` (`lumen-scene-content/2`) |
| Integrator | `lumen-smooth-diffuse-path/1`, 1 bounce |
| Shading | `bunny-diffuse-shading/1+bunny-smooth-shading/1+bunny-smooth-normal/1` |
| Sampler, tone, encoding | `bunny-sobol-owen/1`, `bunny-tone-srgb/1`, `bunny-rgba8-opaque/1` |
| Bunny | `d4fe51d38fa10f8d6496bd110955314edac30bbf` |
| Image | 3840 by 2160 (8 294 400 pixels), 64 samples, seed 0, row bands of 1 row |
| Rays | 1 200 066 195 (closest 847 635 504, shadow 352 430 691) |

Command, one container invocation per run:

```sh
D=~/git/devcontainers/bin/devrun
python3 $D lumen <worktree> --commit 6ef2606 --fetch --exclusive --cpus 18 --memory 12g \
  --export fp4k.png --out "$(mktemp -d)" -- \
  bash -c 'cargo build --release --locked -q && mkdir -p /scratch/out && \
    /target/release/lumen render --scene fixed-point --width 3840 --height 2160 --samples 64 \
    --out /scratch/out/fp4k.png'
```

The single-thread run added `--threads 1` at 960 by 540; every other run used the default
thread count (`threads-asked 18`).

## Runs

All values are seconds, as printed by the command; `total` is the scene build, render, hashing
and writing the PNG, not the `cargo build` before it. Three runs at 3840 by 2160, in order:

| Run | scene | render | hash | png | total | rays per second | ns per ray |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.121 | 20.608 | 0.104 | 0.112 | 20.945 | 58 232 378 | 17.2 |
| 2 | 0.125 | 19.850 | 0.099 | 0.103 | 20.177 | 60 455 890 | 16.5 |
| 3 | 0.136 | 21.978 | 0.129 | 0.119 | 22.363 | 54 602 269 | 18.3 |

`total`: minimum 20.177, maximum 22.363, mean 21.162. A fourth run of the same render at
`26c9093` (`main` after the merge) took 21.250 and printed the same hash.

At 960 by 540 and 64 samples (75 002 401 rays), one run each:

| Threads | scene | render | total | rays per second | ns per ray |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.120 | 15.768 | 15.909 | 4 756 591 | 210.2 |
| 18 | 0.119 | 1.242 | 1.379 | 60 398 500 | 16.6 |

The 1280 by 720 render at 64 samples (133 339 741 rays) took 2.363 in total.

## Image hashes

All three 4K runs, and the fourth at `26c9093`, printed:

```text
sha256:d276602407edb72109ebfbc8cec9121230171cf156144f139fb2bc3ca1c13bef
```

The 960 by 540 render printed the same hash at 1 thread and at 18,
`sha256:0e182df636f3570861a870c6b2688d220643524500b52cdf5137a36877419dc7`. The 1280 by 720
render printed `sha256:86cdfd58d7745297b52f63f443cedd789a773996c66ad57d45b5d8a6f67fb153`.

Independent check of every exported PNG (each 4K file 33 188 430 bytes, 8-bit RGBA, filter type
0 on every row), on the host, without Lumen's code: Python `zlib` decompressed the `IDAT` data,
the filter bytes were removed, and `hashlib.sha256` over the RGBA8 pixels gave the hash the
command printed, for all six files.

## Against version 1

Version 1 (the decimated bunny, 3 851 faces, flat) rendered at `dc3b8b2`, `main` just before
version 2 was merged (same code otherwise, ray statistics included), under the same conditions,
one run: 1 193 851 653 rays, render 18.934, total 19.152, 15.9 ns per ray, image
`sha256:feb17a60c0787f9c7db4af9effa66701496861942a1eed016ac7f489bcd408d2`. An earlier run at
`637e4c7`, before the ray statistics, took 19.183.

Version 2 costs about 2.0 seconds more at 4K, about 10 % (mean total 21.162 against 19.152),
for 0.5 % more rays (the scene's other changes move the light and the puck): about 1.3 ns more
per ray (17.3 on average against 15.9). Both of its changes are in that difference and this
record does not separate them: the bunny has 18 times as many triangles (a deeper tree for every
ray that passes near it; the scene build rises from 0.008 to about 0.12 seconds, mostly reading
the 3 MB file and computing the normals), and a hit on the bunny is shaded by Bunny's smooth
step, which Bunny measures at about 171 ns against 67 ns flat.

## Limits of the claim

- One machine, one Docker VM and one container image; no native macOS, Linux or Windows number.
- One session on one date; three 4K runs of version 2 at 18 CPUs (and one more at the merge
  commit), one run of version 1, one run each at 1 and 18 threads at 960 by 540.
- The 18 CPU runs use the VM's CPUs as the hypervisor schedules them, and the three 4K totals
  spread over 2.2 seconds; the 2.0 seconds against version 1 is of the same size as that spread,
  so it is a reading, not a measured cost.
- The comparison with version 1 is of two scenes, not of the smooth shading alone.
