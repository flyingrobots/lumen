# ADR 0008: Statistics: what is counted, where, and why it cannot change the image

Status: accepted (flyingrobots/lumen#29). The scene, ray and run statistics, `lumen stats` and
`render --stats` are built. The traversal statistics are decided here and wait for
flyingrobots/bunny#335.

## Context

The `lumen render` report says what was rendered (the provenance) and how long it took, with one
line of ray totals (`rays N (closest C, shadow S)`). It cannot say how complex the scene is, what
lights it, where the rays went, how much work the tree saved or what the threads did. The
maintainer asked for a statistics report that answers those questions without giving up any of
lumen's promises: the same bytes at every thread count, band height and platform (ADR 0003,
ADR 0004), no clock, host probe, hash map or float in the library (ADR 0006), and a library that
prints nothing (ADR 0002).

Three facts about the code shaped the decision:

- The scene holds Bunny's compact tree (`CompactRoot`, `CompactNode`, `CompactChild`), whose
  fields are public. Its shape can be read without calling any Bunny arithmetic.
- Bunny's traversals (`compact_closest_hit`, `compact_any_hit`) return the hit only. Bunny's own
  profile of `closest_hit` (Bunny's `docs/topics/performance/README.md`, "Profile of
  `closest_hit`") counted work with a replica of the search loop in its benchmark harness, checked
  against the shipped function.
- Ray counts already flow the right way: each path returns its `RayCount`, each worker adds its
  pixels' counts, and the calling thread adds the workers' sums after `join` (ADR 0003). No
  shared counter exists.

## Decision

### Principles

- **Statistics are core values returned beside the result.** The library computes them as typed
  integer records; the command formats them. The library never prints.
- **Each worker counts its own and returns it at `join`**, as ray counts are today: no shared
  mutable counter, no lock, no atomic beyond the scheduler's one (ADR 0003).
- **Every count is an exact integer and a function of the scene and the settings alone**, with
  two kinds of exception that are facts about the run and are labelled `(information only)`
  wherever they are printed: the split of work between workers, and anything timed. The byte
  counts of memory are a third kind, facts about this build: a function of the scene and of
  this build's type sizes, which depend on the target's pointer width. They are labelled
  `(information only: this build's type sizes)`, are never hashed or pinned across platforms, and
  no test on the WebAssembly leg asserts a 64-bit size (the 64-bit size tests are
  `#[cfg(target_pointer_width = "64")]` and outside that leg's test list).
- **Nothing is collected on a ray's path unless it is asked for, or its cost is measured and
  found to be within the noise.** The cost of asking is measured and stated.
- **No count is a float, and no rounding happens in the library.** A mean is reported as its two
  integers (a total and a count). The command divides for display, with integer arithmetic and a
  stated rounding.
- **Nothing reaches around Bunny's public API.** What lumen cannot read from it is listed as a
  Bunny gap below, not rebuilt in lumen.

### Why statistics cannot change the image

The image is a function of the scene and the settings, computed by Bunny calls whose arguments do
not include any count. Each statistic is one of:

- read from the scene after it is built (scene statistics: no render involved);
- an integer add of a fact a path already computed (whether a query hit, whether a bounce was
  rejected), after the fact, which no later step reads;
- a per-worker tally returned at `join`, combined by integer sums, minima and maxima, which do
  not depend on order.

No branch that decides which ray is traced reads a counter. Tests hold the golden image hashes
and content digests with and without statistics, and the totals at every thread count and band
height.

### Scene statistics (built)

`Scene::statistics()` returns a `SceneStatistics`, computed on each call, in one pass over the
material indices and one walk of the tree. Nothing is stored in the scene or collected when it is
built or traced.

- **Triangles**: in all, by material (one entry per albedo, in table order, including unused
  ones; the entries sum to the total), with at least one vertex normal (the triangles the smooth
  integrator shades smooth), and of zero area (kept in the tree and the content identity, never
  hit). Vertices are not shared in lumen's model: each triangle holds its own three, so the
  command's vertex count is three times the triangles.
- **Bounds**: the tree's root box, which Bunny's builder makes the exact extent of every
  triangle's box.
- **The tree**: in the terms of Bunny's `BvhNode` tree before conversion (every node counts once;
  the compact layout stores only interior nodes and keeps leaves in their parent's slot):
  nodes, interior nodes, leaves, the deepest leaf, the sum of leaf depths (the mean is that sum
  over the leaves), the triangles in all leaves (each triangle is in exactly one), the fewest and
  most in a leaf, and how many leaves hold 1, 2, 3, 4 and more triangles (`LEAF_SIZE_BUCKETS`:
  Bunny's builder keeps leaves of at most `SAH_MAX_LEAF_SIZE`, 4, except at its depth cap). The
  root is at depth 0. The walk is a depth-first loop over the public compact nodes with a stack
  of 64 entries, the depth `compact_bvh` accepts, and a bound of one visit per node, so a
  malformed tree is reported (`StatisticsError::MalformedTree`), never looped on. The walk reads
  child references; it makes no box test and calls no Bunny arithmetic, so it is not a second
  traversal.
- **The builder and the layout**: `TREE_BUILDER` (`bunny_broadphase::build_bvh_sah`) and
  `TREE_LAYOUT` (`bunny_broadphase::compact_bvh`), read with the Bunny commit the provenance
  records.
- **Memory**: each buffer's `size_of_val` (its length times this build's element size): the
  triangles, surfaces, vertex normals, material indices, materials, compact nodes and
  primitive indices, and their sum. Not counted: the `Scene` value and its vectors' headers,
  spare capacity, allocator overhead, the buffers `Scene::new` drops after the build (the boxes
  and the `2 * triangles` `BvhNode`s before conversion) and what a render allocates.
- **Lights, sky and camera** need no new type: `Scene::lighting`, `Scene::camera` and
  `Scene::materials` already give them, and the command prints them.

The tree's shape is checked in the tests against a recursive walk of a tree rebuilt with Bunny's
builder from the same boxes (a different representation and a different algorithm) and against
Bunny's documented bounds: at most `2n - 1` nodes, one more leaf than interior nodes, every
triangle in one leaf, no leaf deeper than `SAH_MAX_DEPTH`.

### Ray statistics (built)

Per ray kind (`RayOutcomes`, in `PathSample::outcomes`): camera hits and misses, bounce hits and
misses, bounces Bunny's smooth shading rejected (no ray traced), shadow rays occluded and
unoccluded. Per render (`RayStatistics`, in `Render::statistics`): those summed, the pixels, the
samples traced, and the fewest and most rays of any pixel; the mean is the command's division.
`RayStatistics::check` states, and `render --stats` enforces, that there is one record per pixel,
`samples_per_pixel` samples per pixel, exactly one camera ray per sample and at most four rays per
sample. Hits plus misses equal the rays of each kind by construction (the rays of a kind are
reported as that sum); the tests check each outcome against a tracer that records the answers it
gave.

`RayCount` keeps its two fields and the report's `rays` line is byte-identical; `PathSample::rays`
and `Render::rays` are now `outcomes.count()`. The fewest and most rays per pixel combine as
minima and maxima, which, like sums, do not depend on the order workers finish, so every value
is the same at every thread count and band height.

**Always collected, by measurement.** The outcomes are a few integer stores per path, against
hundreds of nanoseconds of traversal. Measured (Apple M5 Pro, `rust-dev:1.96.0` container,
`devrun --exclusive --cpus 18 --memory 12g`, release profile; `lumen-reference-scene/3`, 1280 by
720, 16 samples, seed 0, 39 479 111 rays; render seconds; base `main` at f6e225c, head 3ae3ea6;
the three binaries interleaved, three rounds; every run gave the same image hash and ray count):

| Threads | `main` | head | head `--stats` |
| ---: | --- | --- | --- |
| 1 | 7.018, 6.994, 6.986 (median 6.994) | 7.008, 7.054, 7.037 (7.037) | 7.029, 7.005, 7.010 (7.010) |
| 18 | 0.544, 0.543, 0.544 (0.544) | 0.548, 0.542, 0.547 (0.547) | 0.567, 0.543, 0.539 (0.543) |

Head and head `--stats` run the same render code (`--stats` only formats afterwards), and they
differ by as much as head and `main` (0.4 % at one thread, 0.7 % at 18): the difference is within
the noise of these runs. So there is no counter type parameter and no second path; the uncounted
render is gone, and `--stats` costs only the scene walk and the formatting after the clock stops.

### Run statistics (built)

Per worker: bands and rows taken, pixels and rays. In all: the band count and the band height.
The split between workers differs from run to run, so every per-worker line is
`(information only)`; the totals do not, and the tests check that the per-worker figures sum to
them at 1, 2, 4 and 18 workers. Each worker's bands and rows ride in the `Outcome` it already
returns at `join` (`workers/outcome.rs`); `render_pixels_by_worker` returns them with each
worker's sum (`Rendered`, `WorkerRun`), in a list reserved before any thread starts, and
`render_traced_with_run` turns them into `RunStatistics` (`WorkerStatistics` per worker: bands,
rows, pixels, rays). `Render` does not hold them, so two renders of the same scene at different
schedules still compare equal. `render --stats` prints them last, as `run.` lines.

A worker counts bands, once per band taken, and nothing per row: its rows are its pixels (counted
per pixel already, in its `RayStatistics`) over the width, exact because in a render that
succeeded every worker renders whole rows. A first version counted rows in each worker's step and
measured 2.3 % slower at 18 threads. Measured against the ray-statistics branch (ea89a9d), same
machine and settings as above, base and head alternated, 14 runs each at 18 threads:

| Version | Render seconds, 18 threads | Median |
| --- | --- | ---: |
| base (ea89a9d) | 0.546 to 0.582 | 0.554 |
| rows counted per row | 0.554 to 0.595 | 0.567 (+2.3 %) |
| base (ea89a9d), a second session | 0.539 to 0.574 | 0.561 |
| bands only, rows derived (built) | 0.550 to 0.587 | 0.568 (+1.3 %) |

At one thread the per-row version measured the same as base (medians 7.169 against 7.162 s,
three runs each). What is left at 18 threads, about 7 ms in 0.56 s, is within the spread of
either set of runs but leans one way; if it is to go, the next step is a zero-sized tally type for
the scheduler when run statistics are not asked for, which this change does not make.

There is no per-worker busy time. The library reads no clock (ADR 0006), and the honest simple
measure is the one the command already takes: the wall time of the whole render. An injected
tick source, a seam the command would provide and tests replace, was weighed and rejected: it
puts a clock-shaped interface into the deterministic core for one information-only number per
worker, and the token scan and Clippy bans would need a waiver in the scheduler for it. The issue
also asked for time spent holding and waiting for the image lock; there is no image lock since
ADR 0003 (each band is a disjoint slice), and the one lock left is held for one iterator step.

### Traversal statistics (decided, not yet built; waits for flyingrobots/bunny#335)

Nodes visited, box tests, leaf visits and triangle tests per ray kind, with totals and maxima,
and the same counters for flyingrobots/lumen#16's BVH-cost view, which needs them per camera ray:
one set of counters serves both.

They come from Bunny: flyingrobots/bunny#335 asks for counting variants of the compact
traversals that return the same bits as the uncounted ones (a differential test) with a record of
counts, while the uncounted functions keep their cost (measured). Lumen will call the counted
functions only when traversal statistics are asked for, through a counting tracer beside the
scene's own, so the uncounted render does not change.

Rejected: a replica of Bunny's search loop in lumen, built from `TraceRay::box_entry`, the node
view and `hit_triangle` and asserted identical to Bunny's on every ray. It is what Bunny's own
profile does inside Bunny's harness, but in lumen it would duplicate numerical traversal code
outside the tests, which lumen does not do: every number that decides a pixel comes from Bunny,
and a copy drifts when Bunny's search changes.

### Output (built)

- **`lumen stats [--scene NAME]`** builds the scene, renders nothing, and prints its name,
  content identity and integrator, then the scene block. It needs no render, so it is its own
  command rather than only a part of `render`.
- **`lumen render --stats`** prints the scene block, then the ray block, after the existing
  report. With `--stats` off the output is byte-identical to the output before statistics existed:
  checked against `main` for a fixed set of invocations with the timing values masked (the pull
  request records it), and in `tests/cli.rs` the plain report is exactly the start of the
  `--stats` output.
- **Format: `key value` lines.** A key has no spaces and is dotted by group (`scene.`, `light.`,
  `camera.`, `tree.`, `memory.`, `rays.`, `run.`); the value is one token or a vector of
  tokens separated by single spaces; an information-only line ends in ` (information only)`, as
  the timing lines already do. Keys are stable; new keys are added, never renamed. Fixed-point
  values print in decimal with six places from their exact raw integers (rounded to nearest, ties
  away from zero); a mean prints with two places (rounded to nearest, ties up).
- **No JSON.** The lines are already machine-readable (`awk '$1 == "tree.nodes" { print $2 }'`),
  every key and value is a fixed token with nothing to escape, and lumen takes no dependency and
  would otherwise maintain a serializer for one fixed shape. A JSON form can be added later as a
  formatter over the same records if a consumer needs one.

### Bunny gaps

- **No traversal counters**: flyingrobots/bunny#335.
- **No versioned identities for the tree builder and the layout**: lumen names them by function
  path (`tree.builder`, `tree.layout`), qualified by the Bunny commit. A change of Bunny's builder
  output at the same name would show only as a new commit. Typed constructors and identities are
  being collected in flyingrobots/bunny#295.
- **Not reported**: the surface-area cost terms of the tree (Bunny's profile reports them). They
  need area arithmetic, which would be numerical code in lumen; a Bunny function over a built
  tree would be their home.

## Consequences

- `Scene` gains `statistics()` and `materials()`; the crate root gains `SceneStatistics`,
  `TreeStatistics`, `SceneMemory`, `StatisticsError`, `LEAF_SIZE_BUCKETS`, `TREE_BUILDER` and
  `TREE_LAYOUT`. The records are `#[non_exhaustive]`, so counts can be added without breaking a
  caller; a caller reads their fields and cannot construct them.
- The command gains `stats` and a `Failure::Statistics` (exit code 1), which no built-in scene
  reaches.
- `PathSample` gains `outcomes` and `Render` gains `statistics`, which breaks a caller that builds
  either by its fields; the crate root gains `RayOutcomes`, `RayStatistics` and `RayCheckError`.
  The command gains `render --stats` and `Failure::RayCheck` (exit code 1), which no render
  reaches.
- The crate root gains `render_traced_with_run`, `RunStatistics`, `WorkerStatistics`,
  `render_pixels_by_worker`, `Rendered` and `WorkerRun`; `render_pixels` is unchanged.
- Memory byte counts can differ between targets with different type sizes; nothing else in the
  scene statistics can.
