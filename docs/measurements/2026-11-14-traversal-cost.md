# Measurement record: traversal work per ray in the three built-in scenes, 2026-11-14

This is a record of one session. It describes what was measured on the date above, at the
commit below. It is not a statement about any later commit.

It answers the question the canyon record (`2026-11-14-canyon.md`) left open. In that record a
canyon ray cost about 2.7 times a reference-scene ray, while the canyon's tree is only a few
levels deeper. Bunny's counted traversals (`render --stats`, ADR 0008) now say how much work
each ray does.

## Conditions

| Item | Value |
| --- | --- |
| Commit | `5f31b49` (`feat/traversal-stats-output`: the `render --stats` traversal lines, on main with the canyon scene) |
| Bunny | `2cd551e248d55407a8f27e84dcb26d1b1d8768bd` |
| Machine | Apple M5 Pro, the `rust-dev:1.96.0` Linux container through `devrun --exclusive --cpus 18 --memory 12g` |
| Profile | release |
| Setting | 640 by 360, 16 samples, seed 0, bands of 1 row; `--stats` at 18 threads, then the plain render at 1 thread |

Command:

```sh
D=~/git/devcontainers/bin/devrun
python3 $D lumen <worktree> --worktree --fetch --exclusive --cpus 18 --memory 12g -- \
  bash -c 'cargo build --locked --release -q -p lumen && for s in reference fixed-point canyon; do
    /target/release/lumen render --scene $s --width 640 --height 360 --samples 16 --threads 18 --stats
    /target/release/lumen render --scene $s --width 640 --height 360 --samples 16 --threads 1; done'
```

## The scenes and their trees

From `render --stats` (`scene.` and `tree.` lines):

| Scene | Triangles | Leaves | Deepest leaf | Mean leaf depth | Leaves of 1, 2, 3, 4 triangles |
| --- | ---: | ---: | ---: | ---: | --- |
| reference (`lumen-reference-scene/3`) | 31 511 | 16 548 | 20 | 16.36 | 2 778, 12 708, 931, 131 |
| Fixed Point (`lumen-fixed-point-scene/2`) | 71 575 | 37 946 | 20 | 16.05 | 5 429, 31 422, 1 078, 17 |
| canyon (`lumen-canyon-scene/1`) | 566 005 | 287 844 | 27 | 19.13 | 14 724, 268 436, 4 327, 357 |

## Work per ray

Each cell gives the mean per ray (`traversal.KIND.COUNTER-mean`), and in parentheses the most any
one ray took (`-max`). Box tests count the root's box and both children's boxes at every
interior visit. Primitive tests are triangle tests.

| Scene | Kind | Rays | Box tests | Interior visits | Leaf visits | Primitive tests | Popped, skipped | Peak stack |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| reference | camera | 3 686 400 | 14.15 (203) | 6.57 (101) | 1.04 (21) | 2.08 (42) | 0.52 (11) | 14 |
| reference | bounce | 2 824 071 | 18.78 (237) | 8.89 (118) | 0.77 (27) | 1.55 (52) | 0.38 (12) | 14 |
| reference | shadow | 3 359 387 | 18.27 (209) | 8.64 (104) | 0.64 (22) | 1.25 (44) | 0.00 (0) | 15 |
| Fixed Point | camera | 3 686 400 | 17.48 (225) | 8.24 (112) | 0.77 (27) | 1.53 (54) | 0.47 (8) | 10 |
| Fixed Point | bounce | 2 200 013 | 28.08 (325) | 13.54 (162) | 0.60 (47) | 1.19 (78) | 0.22 (11) | 14 |
| Fixed Point | shadow | 2 447 393 | 25.58 (243) | 12.29 (121) | 0.45 (25) | 0.88 (51) | 0.00 (0) | 11 |
| canyon | camera | 3 686 400 | 52.35 (419) | 25.68 (209) | 1.41 (32) | 2.83 (65) | 2.15 (12) | 15 |
| canyon | bounce | 2 866 103 | 65.32 (445) | 32.16 (222) | 2.36 (67) | 4.79 (162) | 1.03 (13) | 16 |
| canyon | shadow | 3 357 837 | 59.75 (331) | 29.37 (165) | 2.28 (33) | 4.45 (61) | 0.00 (0) | 14 |

A shadow search stops at its first hit and never pops an entry it then drops, so its skipped
count is zero.

Time per ray, wall clock over every ray (`ns-per-ray`, information only):

| Scene | Rays | 18 threads (`--stats`) | 1 thread (plain) |
| --- | ---: | ---: | ---: |
| reference | 9 869 858 | 17.0 ns | 226.6 ns |
| Fixed Point | 8 333 806 | 18.3 ns | 217.0 ns |
| canyon | 9 910 340 | 41.8 ns | 506.3 ns |

Every image hash was the one the same settings give without `--stats` (the counting render is
checked against the plain one in `tests/traversal_statistics.rs`).

## What it shows

These are measured facts, read with the numbers above.

- **Traversal work does not grow with the tree's depth alone.** The canyon's mean leaf is 2.8
  levels deeper than the reference scene's (19.13 against 16.36). A canyon camera ray, though,
  makes 3.7 times as many box tests (52.35 against 14.15) and 3.9 times as many interior visits
  (25.68 against 6.57). A ray visits not one root-to-leaf path but many partial ones: about 26
  interior nodes in a tree whose leaves sit 19 deep on average.
- **The extra work is in the boxes, not the triangles.** Primitive tests per camera ray rise only
  from 2.08 to 2.83. Box tests rise by 38 per camera ray, primitive tests by 0.75.
- **The time follows the box tests.** At one thread a canyon ray costs 2.2 times a
  reference-scene ray (506.3 against 226.6 ns). Its closest-hit searches make 3.4 to 3.7 times as
  many box tests and its shadow searches 3.3 times as many. Each ray also pays the same shading
  and sampling, which do not grow with the scene.
- **More of the canyon's secondary rays hit something.** 53 % of its bounce rays hit (1 520 705 of
  2 866 103) against 24 % in the reference scene, and 22 % of its shadow rays are occluded against
  10 %. A ray that hits terrain is searched until the nearest hit is proven, so it cannot leave the
  tree early through empty space.
- Fixed Point costs about what the reference scene costs per ray (18.3 against 17.0 ns at 18
  threads). It has 2.3 times the triangles, and its bounce and shadow rays make 1.4 to 1.5 times the
  box tests.

## What it does not show

The counts do not say why canyon rays visit so many interior nodes. One explanation fits them:
the canyon's terrain triangles have long, overlapping bounding boxes along the canyon walls, and
grazing rays enter many of them. It is untested here. Bunny's profile reports surface-area cost
terms, which would test it; lumen does not compute them (ADR 0008, "Bunny gaps").
