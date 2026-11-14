# The `lumen` command

Reference for every command and option of `lumen`, what it prints and how it exits. To render a
first image step by step, see the [tutorial](../../README.md#render-an-image); for how the
renderer works, see [how Lumen renders](../explanation.md).

`lumen` is the crate's binary. From a checkout, run it through Cargo, for example
`cargo run --release -p lumen -- render --out image.png`, or build it once with
`cargo build --release` and run `target/release/lumen`. The examples below write `lumen` for
either.

## Synopsis

`lumen --help` prints this text, which is the authority for the option list; a test
(`the_command_reference_holds_the_usage_text`) fails if this page and the program disagree:

```text
usage: lumen render [--scene NAME] [--out FILE] [--width W] [--height H] [--samples N] [--seed S]
                    [--threads T] [--band-rows R] [--stats]
       lumen stats [--scene NAME]
       lumen --help | --version

Renders one of lumen's built-in scenes (reference, the default, fixed-point or canyon) on T threads,
each taking bands of R whole rows, with N samples per pixel of a diffuse path (a shadow-casting
sun, a uniform sky and one bounce), writes it to a PNG file if --out is given, and prints the
image hash (SHA-256 of the raw RGBA8 bytes) and the timing. The image is the same for every
thread count and band height. --stats adds the scene's statistics and what the rays found
after the report (see docs/reference/cli.md). `lumen stats` builds the scene, renders nothing
and prints what it holds: triangles by material, bounds, lights, camera, the shape of its tree
and its memory.
Scenes (--scene NAME): reference, fixed-point (the Stanford bunny in sunglasses at a craps table),
canyon (craft flying along a fractal canyon under a banded planet).
Defaults: --scene reference --width 320 --height 180 --samples 4 --seed 0 --band-rows 1, and as
many threads as the system reports available.
Width and height are 1..=32768 each, with at most 134217728 (2^27) pixels in all.
Samples per pixel are a power of two from 1 to 1024; the seed is 0..=4294967295.
Threads are 1..=256; band rows are at least 1.
```

## Commands

### `lumen render`

Builds a built-in scene, renders it, prints the [report](#the-report) and, with `--out`, writes
the image as a PNG file. Without `--out` nothing is written and the report says `png none`: the
hash and the timing are the whole result. With `--stats` the report is followed by the
[statistics](#statistics).

### `lumen stats`

Builds a built-in scene and prints its [statistics](#statistics) without rendering. It takes
only `--scene` (or `--scene=NAME`) and `--help`.

### `lumen --help`, `-h`, `help`

Prints the usage text above to standard output and exits 0. `--help` or `-h` after `render` or
`stats` does the same.

### `lumen --version`, `-V`

Prints `lumen 0.1.0 (bunny <commit>)`, the package version and the Bunny commit the binary was
built against (`BUNNY_REV`), and exits 0.

## Options of `lumen render`

| Option | Value | Default | Refused |
| --- | --- | --- | --- |
| `--scene NAME` | `reference` (`lumen-reference-scene/3`), `fixed-point` (`lumen-fixed-point-scene/2`) or `canyon` (`lumen-canyon-scene/1`); see [the scenes](scenes.md) | `reference` | any other name: exit 2, the message lists the scenes |
| `--out FILE` | the PNG file to write; any path, kept byte for byte if it is not Unicode | none: nothing is written | a destination that cannot be written: exit 1, before the render ([below](#the-output-file)) |
| `--width W` | the width in pixels, `1..=32768` (the limit of Bunny's camera), with at most `2^27` pixels in all (a 512 MiB RGBA8 buffer, enough for 16K at 15360 by 8640) | 320 | 0, more than 32768, or too many pixels with the height: exit 2 |
| `--height H` | the height in pixels, on the same terms | 180 | as for the width |
| `--samples N` | samples per pixel, a power of two from 1 to 1024: Bunny's sampler states its stratification for aligned power-of-two blocks of samples, and its accumulator takes at most 1024 | 4 | 0, not a power of two, above 1024: exit 2 |
| `--seed S` | the sampler's seed, `0..=4294967295`, the domain of Geordi's render settings | 0 | anything else: exit 2 |
| `--threads T` | worker threads, `1..=256`; the image does not depend on it | what `std::thread::available_parallelism` reports (inside a container, its CPU limit), within `1..=256` | 0 or more than 256: exit 2 |
| `--band-rows R` | the whole image rows a worker takes at a time, at least 1; the image does not depend on it | 1 | 0: exit 2 |
| `--stats` | a flag: add the statistics after the report | off | given twice: exit 2 |

There is no `--tile` option any more; it is refused as unknown. Each option may be given once;
a repeated option is refused with exit status 2.

### Option values

Each value is the next argument or, for any option, part of the same argument
after an equals sign (`--out=FILE`, `--width=96`); only the first `=` splits, so `--out=a=b` names
`a=b`. A separate value that starts with `-`, has more after it and is not a negative number is
read as an option and refused with status 2 (`--out --width` is an error, not a file called
`--width`), the rule of `clap` and `argparse`; to use such a value, write `--out=--width`. A lone
`-` and a negative number such as `-3` are values (the latter is then refused as a number where a
number is needed). `--` is not an option: lumen has no positional arguments for it to separate.
The `=` form needs an argument that is valid Unicode; a non-Unicode path is passed as a separate
argument. Numbers are decimal digits with no sign, space or underscore; leading zeros are
accepted for every numeric option and change nothing (`--width 000096` is 96, never octal, and
the report prints `96`), because a refusal would only reject a value whose meaning is clear.
Arguments are read as OS strings, so an output path that is not valid Unicode is
used as given; a non-Unicode number or option is refused like any other bad argument.

## The report

`lumen render` prints one `key value` line per fact to standard output. The first thirteen are
the render's provenance (facts about the image), then the run's (threads, band height), the
results (rays, image hash), the file and the wall-clock times. For example, with the defaults
and `--out first.png`; the lines in angle brackets vary from machine to machine and run to run,
every other line is exact:

```text
scene lumen-reference-scene/3
integrator lumen-diffuse-path/1
shading bunny-diffuse-shading/1
sampler bunny-sobol-owen/1
tone bunny-tone-srgb/1
encoding bunny-rgba8-opaque/1
bunny d4fe51d38fa10f8d6496bd110955314edac30bbf
size 320x180
pixels 57600
samples 4
seed 0
bounces 1
scene-content sha256:c1e109477c09328421d76efefd923bd625d842fe3afb93dfadb21ae905bd0f66
threads <workers used>
threads-asked <--threads, or what the system reports available>
band-rows 1
rays 616892 (closest 406903, shadow 209989)
image sha256:77ab18fafd84f6598fa638281e205f2298df6ba6da3471546a3724718d3cef23
png first.png (230699 bytes)
scene-seconds <time> (information only)
render-seconds <time> (information only)
hash-seconds <time> (information only)
png-seconds <time> (information only)
total-seconds <time> (information only: scene, render, hash and png)
rays-per-second <rate> (information only: wall clock, all threads)
ns-per-ray <time> (information only: wall clock, all threads)
```

- `scene` to `scene-content`: the render's provenance, `RenderProvenance::fields()` in its
  order. `bunny` is `BUNNY_REV`; `scene-content` is the scene's content identity
  (ADR 0005).
- `threads` is the workers used: never more than there are bands. `threads-asked` is
  `--threads` or the default.
- `rays`: every ray traced, then the closest-hit queries (camera and bounce rays) and the
  any-hit queries (shadow rays). It does not depend on the threads or the band height.
- `image`: `sha256:` and the lowercase hex SHA-256 of the raw RGBA8 bytes, the image hash.
- `png`: the path as given and the number of bytes written to the file, counted as the file
  accepted them; the command checks that it equals the PNG's length and fails if it does not.
  `png none` without `--out`.
- The `-seconds` lines time each part with a monotonic clock; `total-seconds` is the whole
  command after argument parsing. `rays-per-second` and `ns-per-ray` divide the render's wall
  time by the rays traced on all threads, so they are not CPU time per ray. None of these
  reaches the image.

The PNG is uncompressed, so it is about four bytes per pixel: 230 699 bytes at the default
320 by 180, 922 266 bytes at 640 by 360, and 33 188 430 bytes (about 33 MB) at 3840 by 2160.

## The output file

`--out FILE` is checked before the render starts: a destination that cannot be written (its
directory is missing or not writable, it is a directory or a link to one, it is a read-only file,
it has no file name) is refused with status 1 and a message that names the path, without computing a
pixel and without leaving anything behind.

The PNG is then written to a temporary file in the destination's directory, `.NAME.lumen-PID-N.tmp`
(created exclusively, so it never replaces another file and two runs never share one), flushed,
`sync_all`ed and renamed over the destination. If any step fails the temporary file is removed and
the destination is exactly as it was: a failed write (a full disk, a size limit) no longer leaves a
truncated file or destroys the one that was there. A temporary file that cannot be removed is
named in the error. So `--out` names either the complete new PNG or what it was before, whether
the render or the write fails.

> Caution: `--out` replaces an existing file at that path without asking.

- Durability: on Unix the directory is synced after the rename, so the new name survives a crash;
  if that sync fails the error says the file was written but its directory could not be synced (the
  destination holds the new image). On Windows the standard library cannot sync a directory and
  none is attempted; the replacement itself is a single rename.
- An existing regular file is replaced by a new one: it gets the default mode for new files, not
  the old file's mode, owner or hard links. A read-only file is refused rather than replaced.
- A symbolic link is replaced itself; the file it pointed to is not written (this differs from
  writing through the link). A link to a directory is refused like a directory; a dangling link is
  replaced.
- A device, pipe or socket (`--out /dev/null`) is written in place, without a temporary file or
  a sync, since replacing it would destroy the node.
- The rename needs write permission on the directory, not on the old file.

## Exit status and errors

| Status | When |
| --- | --- |
| 0 | the command did what it was asked |
| 2 | the command line was refused: an unknown command or option, a missing, repeated or option-looking value, a number that is not a decimal `u32`, and every refused value (a size, samples, threads, band rows, a scene). The message is followed by the usage text, except for a refused size |
| 1 | anything that fails once the arguments are accepted: the scene could not be built, the destination was refused or could not be written, the render failed, the statistics could not be read or contradict the render, standard output could not be written |

Every failure prints one message to standard error, after `lumen: `. The message keeps the
library's or the system's error text, for example
`lumen: /no/such/dir/x.png: No such file or directory (os error 2)` for an output file in a
missing directory (the operating system's text varies by platform).

## Statistics

### `lumen stats`

`lumen stats [--scene NAME]` builds a scene and prints what it holds, without rendering:

```sh
lumen stats --scene reference
```

The library computes the counts (`Scene::statistics()`, a `SceneStatistics`) and the command
formats them; nothing is counted while a ray is traced, and nothing in the scene changes.
`Scene::statistics` is described in the [library reference](library.md#a-scene-can-be-counted);
[ADR 0008](../adr/0008-statistics.md) records what is counted and why it cannot change a pixel.
Traversal statistics (nodes visited and triangles tested per ray) are decided there and wait for
flyingrobots/bunny#335.

### `lumen render --stats`

`lumen render ... --stats` prints the report unchanged, then the same scene block (every line of
`lumen stats` after its first three, which the report already gives), then what the rays found,
then how the work was spread over the workers.
Without `--stats` the output is the report alone, byte for byte as before statistics existed.
The command first checks the ray statistics against the size and settings (one record per pixel,
`--samples` samples per pixel, exactly one camera ray per sample, at most four rays per sample)
and fails with exit code 1 if they disagree, which no render the library produced does.

### Format

Each line is a key with no spaces, one space and a value: one token, or a vector of tokens
separated by single spaces. Keys are stable; new ones are added, never renamed. Fixed-point values
are printed in decimal with six places from their exact raw integers, rounded to nearest with ties
away from zero. Every value is a function of the scene alone, except the `memory.` byte counts,
which also depend on this build's type sizes (the values below are those of 64-bit targets).

### Scene keys

| Key | Value |
| --- | --- |
| `scene`, `scene-content`, `integrator` | the scene's name, its content identity and its integrator, as in the render report |
| `scene.triangles` | every triangle, zero-area ones included |
| `scene.vertices` | three per triangle: lumen's triangles hold their own vertices and share none |
| `scene.triangles-smooth` | triangles with at least one vertex normal, which `lumen-smooth-diffuse-path/1` shades smooth |
| `scene.triangles-zero-area` | triangles of zero area: kept, never hit |
| `scene.materials` | the albedos in the material table, used or not |
| `scene.material.N.triangles` | the triangles naming material `N`; they sum to `scene.triangles` |
| `scene.material.N.albedo` | material `N`'s red, green and blue reflectance |
| `scene.bounds-min`, `scene.bounds-max` | the corners of the box around every triangle (the tree's root box) |
| `light.sun` | `present` or `none`; when present, `light.sun-toward` (the direction to the sun) and `light.sun-irradiance` (per channel) follow |
| `light.sky` | the sky's radiance per channel |
| `camera.position`, `camera.look`, `camera.up`, `camera.tan-half-fov` | the camera placement as given |
| `tree.builder`, `tree.layout` | `bunny_broadphase::build_bvh_sah` and `bunny_broadphase::compact_bvh`, at the Bunny commit of `lumen --version` |
| `tree.nodes` | every node of Bunny's tree, interior and leaf: `2 * leaves - 1`, at most `2 * triangles - 1` |
| `tree.interior-nodes`, `tree.leaves` | the two kinds; the compact layout stores one node per interior node and keeps leaves in their parent |
| `tree.leaf-depth-max` | the deepest leaf, the root being at depth 0; at most 60 (Bunny's `SAH_MAX_DEPTH`) |
| `tree.leaf-depth-total` | the sum of every leaf's depth |
| `tree.leaf-depth-mean` | `leaf-depth-total / leaves`, with two places, rounded to nearest, ties up |
| `tree.leaf-triangles` | the triangles of every leaf, summed: each triangle is in one leaf, so this is `scene.triangles` |
| `tree.leaf-triangles-min`, `tree.leaf-triangles-max` | the fewest and most triangles in a leaf |
| `tree.leaf-sizes` | five counts: leaves of 1, 2, 3 and 4 triangles, and of more (only at the depth cap) |
| `memory.NAME-bytes` | information only, a fact about this build (the line ends in `(information only: this build's type sizes)`): each buffer's length times its element size, which depends on the target's pointer width: `triangles` (Bunny's `TraceTriangle`), `surfaces` (`Option<TriangleSurface>`), `vertex-normals`, `material-indices`, `materials`, `tree-nodes` (compact nodes), `tree-primitive-indices`, and their `total` |

### Ray keys

The ray lines (`render --stats`), each a function of the scene, the size and the settings, the
same at every thread count and band height:

| Key | Value |
| --- | --- |
| `rays.camera`, `rays.camera-hits`, `rays.camera-misses` | camera rays traced, and those that hit a triangle or left the scene: exactly one per sample |
| `rays.bounce`, `rays.bounce-hits`, `rays.bounce-misses` | bounce rays traced, one at each camera hit unless rejected, and their hits and misses |
| `rays.bounce-rejected` | bounces Bunny's smooth shading rejected as below the surface: no ray traced, so not in `rays.bounce` |
| `rays.shadow`, `rays.shadow-occluded`, `rays.shadow-unoccluded` | shadow rays traced at camera and bounce hits, and those that met a triangle (the sun blocked) or nothing |
| `rays.total` | every ray: the `rays` line's total |
| `rays.pixels`, `rays.samples-per-pixel`, `rays.samples` | pixels rendered, samples asked for per pixel, and samples traced in all |
| `rays.per-pixel-min`, `rays.per-pixel-max` | the fewest and most rays any one pixel traced |
| `rays.per-pixel-mean` | `rays.total / rays.pixels`, with two places, rounded to nearest, ties up |

### Run keys

The run lines (`render --stats`), last. The band height, the band count and the worker count follow
from the size and `--threads` and `--band-rows`; which worker took which band depends on timing, so
every per-worker line ends in `(information only: differs from run to run)`. The per-worker
figures add up to the totals, which do not differ. Workers report counts only: there is no
per-worker busy time, since the library reads no clock (ADR 0008); `render-seconds` is the whole
render's.

| Key | Value |
| --- | --- |
| `run.band-rows`, `run.bands` | the rows in a band and the bands the image is cut into (the height over the band height, rounded up) |
| `run.workers` | the workers that rendered, the `threads` line's count |
| `run.worker.N.bands`, `.rows`, `.pixels`, `.rays` | what worker `N` (0 is the calling thread) took and traced: information only |

### What is not counted, and an example

Not counted in `memory.`: the `Scene` value and its vectors' headers, spare capacity, the
allocator's overhead, the buffers dropped after the build (the boxes and the uncompacted tree of
`2 * triangles` nodes) and what a render allocates. The output is machine-readable as it is
(`awk '$1 == "tree.nodes" { print $2 }'`); there is no JSON form. For the reference scene:

```text
scene.triangles 31511
scene.material.3.triangles 27648
scene.material.4.triangles 3851
scene.bounds-min -16.000000 -1.125000 -16.000000
scene.bounds-max 16.000000 2.000000 16.000000
tree.nodes 33095
tree.leaves 16548
tree.leaf-depth-max 20
tree.leaf-depth-mean 16.36
tree.leaf-sizes 2778 12708 931 131 0
memory.total-bytes 11432728 (information only: this build's type sizes)
```

