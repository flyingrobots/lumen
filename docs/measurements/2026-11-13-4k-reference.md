# Measurement record: 4K reference scene at 16 samples per pixel, 2026-11-13

This is a record of one session. It describes what was measured on the date above, at the commit
below. It is not a statement about any later commit. To repeat it, see
[Reproduce](../how-to/measure-performance.md#reproduce) in the performance guide (the section
moved there from the README on 2026-11-14).

It answers flyingrobots/lumen#5: does Lumen render the 4K reference scene at 16 samples per pixel
in under 60 seconds, with the same image hash on two runs?

## Conditions

| Item | Value |
| --- | --- |
| Commit | `d8deec49186007776f66f37c17c2f9f544242f9c` (`main`, after the merge of pull request 26) |
| Machine | Apple M5 Pro: 6 "super" and 12 performance cores |
| Where it ran | the `rust-dev:1.96.0` Linux container, through `devrun`, on a Docker VM with 18 CPUs; not native macOS |
| Profile | release (`cargo build --release --locked`) |
| Isolation | every run used `devrun --exclusive`, so no other run shared the machine |
| Limits | `--cpus 18 --memory 12g`, except the run marked "6 CPUs" |
| Scene | `lumen-reference-scene/3`, 31 511 triangles |
| Integrator | `lumen-diffuse-path/1`, 1 bounce |
| Shading, sampler, tone, encoding | `bunny-diffuse-shading/1`, `bunny-sobol-owen/1`, `bunny-tone-srgb/1`, `bunny-rgba8-opaque/1` |
| Bunny | `dcd6bd37613738de93f7084e7c8aa0283de964ea` |
| Image | 3840 by 2160 (8 294 400 pixels), 16 samples, seed 0, 32 by 32 tiles |
| Rays | 355 318 863 (closest 234 376 387, shadow 120 942 476) |

Command, run from a worktree of the repository at that commit (one container invocation per run):

```sh
D=~/git/devcontainers/bin/devrun
python3 $D lumen <worktree> --commit HEAD --fetch --exclusive --cpus 18 --memory 12g -- \
  bash -c 'cargo build --release --locked -q && mkdir -p /scratch/out && \
    /target/release/lumen render --width 3840 --height 2160 --samples 16 --out /scratch/out/4k.png'
```

The README's reproduction command worked as written, with the worktree path in place of
`~/git/lumen`. The single-thread run added `--threads 1` to the `lumen render` command; the
6 CPU run used `--cpus 6` instead of `--cpus 18`. Thread count was the default in all other runs
(`threads-asked 18`; with `--cpus 6` the program reported 6).

## Interval compared with the target

`total-seconds`: the whole `lumen render` command after argument parsing, that is scene build,
render, hashing and writing the PNG. The `cargo build` in the same container command is not in it.

## Runs

All values are seconds, as printed. Five runs at 18 CPUs and default threads, in order:

| Run | scene | render | hash | png | total |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.032 | 5.600 | 0.101 | 0.095 | 5.828 |
| 2 | 0.032 | 5.723 | 0.104 | 0.103 | 5.962 |
| 3 | 0.032 | 5.679 | 0.107 | 0.109 | 5.926 |
| 4 | 0.031 | 5.934 | 0.111 | 0.095 | 6.171 |
| 5 | 0.031 | 5.983 | 0.114 | 0.103 | 6.232 |

`total` over the five runs: minimum 5.828, maximum 6.232, mean 6.024. Rays per second (wall
clock, all threads): 63 444 569, 62 089 945, 62 568 805, 59 881 207, 59 388 809. Nanoseconds
per ray: 15.8, 16.1, 16.0, 16.7, 16.8.

Two further runs, one each:

| Condition | threads used | scene | render | hash | png | total | rays per second | ns per ray |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `--threads 1`, 18 CPUs | 1 | 0.033 | 72.226 | 0.098 | 0.100 | 72.457 | 4 919 511 | 203.3 |
| 6 CPUs | 6 | 0.032 | 13.536 | 0.099 | 0.100 | 13.767 | 26 249 051 | 38.1 |

## Image hash

All seven runs, at every thread count and CPU limit, printed the same hash:

```text
sha256:97d1d3034439186d6891a65cb6c2552945e5c9fc10fa584a83cfa6c28fdbebfe
```

Independent check of run 5's exported PNG (33 188 430 bytes, 8-bit RGBA, filter type 0 on every
row), on the host, without Lumen's code: Python `zlib` decompressed the `IDAT` data, the filter
bytes were removed, and `hashlib.sha256` over the 3840 by 2160 RGBA8 pixels gave the same value.

## Verdicts

- Under 60 seconds: met in all five default runs (maximum `total` 6.232) and in the 6 CPU run
  (13.767). Not met with one thread (72.457).
- Same hash on two runs: met; equal in all seven runs.
- Under 15 seconds (the stretch reading): met in all five default runs and in the 6 CPU run.
  Not met with one thread.

## Limits of the claim

- One machine, one Docker VM and one container image; no native macOS, Linux or Windows number.
- One scene of 31 511 triangles, one directional light and one bounce. Other scenes and
  integrators were not measured.
- One session on one date; five runs at 18 CPUs, one run each at 1 thread and 6 CPUs.
- The 18 CPU runs use the VM's CPUs as the hypervisor schedules them; a run cannot be pinned to
  the fast cores.
- A single thread does not meet 60 seconds on this machine.
