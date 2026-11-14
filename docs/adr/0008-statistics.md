# ADR 0008: Statistics: what is counted, where, and why it cannot change the image

Status: accepted (flyingrobots/lumen#29). The scene statistics and `lumen stats` are built. The
ray, run and traversal statistics and `render --stats` are decided here and not yet built; each
section says which.

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
  counts of memory are a third, milder kind: a function of the scene and of this build's type
  sizes.
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

### Ray statistics (decided, not yet built)

Per ray kind: camera, bounce and shadow rays traced; for camera and bounce rays, hits and misses;
for shadow rays, occluded and unoccluded; bounces Bunny's smooth shading rejected (no ray
traced); samples per pixel requested and delivered; rays per pixel (fewest, most and total; the
mean is the command's division). The render checks, and the tests check, that camera rays are
exactly `width * height * samples`, that the total lies between that and four times that, and
that hits plus misses equal the rays of each kind.

`RayCount` keeps its two fields and the report's `rays` line stays byte-identical. The richer
record is a new type beside it, of which `RayCount` is a projection. The fewest and most rays per
pixel combine as minima and maxima, which, like sums, do not depend on the order workers finish.

Whether the record is always collected or only on request is decided by measurement, not
assumed: it is a few integer adds per path against hundreds of nanoseconds of traversal. If the
render time with it always on is within the noise of `main` (interleaved, `--exclusive`, three
runs each at 1 and 18 threads on a mid-size image), it is always collected and `RayCount` is
derived from it. Otherwise the path is generic over a counter, a zero-sized one that does nothing
for the uncounted render and the record for `--stats`, so the uncounted render is today's code.
The numbers go in the pull request and the README either way.

### Run statistics (decided, not yet built)

Per worker: bands and rows taken, pixels and rays. In all: the band count and the band height.
The split between workers differs from run to run, so every per-worker line is
`(information only)`; the totals do not, and the tests check that the per-worker figures sum to
them at 1, 2, 4 and 18 workers. Each worker's tally rides in the `Outcome` it already returns
at `join` (`workers/outcome.rs`).

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

### Output (`lumen stats` and the format built; `render --stats` not yet)

- **`lumen stats [--scene NAME]`** builds the scene, renders nothing, and prints its name,
  content identity and integrator, then the scene block. It needs no render, so it is its own
  command rather than only a part of `render`.
- **`lumen render --stats`** (with the ray statistics) will print the scene block, then the ray
  block, after the existing report. With `--stats` off the output is byte-identical to the
  output before statistics existed, which a test checks for a fixed set of invocations with the
  timing values masked.
- **Format: `key value` lines.** A key has no spaces and is dotted by group (`scene.`, `light.`,
  `camera.`, `tree.`, `memory.`; later `rays.` and `run.`); the value is one token or a vector of
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
  path, qualified by the Bunny commit. A change of Bunny's builder output at the same name would
  show only as a new commit.
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
- Memory byte counts can differ between targets with different type sizes; nothing else in the
  scene statistics can.
