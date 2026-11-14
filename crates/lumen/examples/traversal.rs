//! Times lumen's integrator over the reference scene with each of Bunny's tree builders and
//! traversal layouts, to choose the one `Scene` uses. Information only, not a test.
//!
//! ```sh
//! cargo run --release --locked -p lumen --example traversal -- [WIDTH HEIGHT SAMPLES ROUNDS]
//! ```
//!
//! Each round renders every pixel sample of a small image once per structure, in an order that
//! rotates between rounds; the report is the median per structure. Every structure must give the
//! same radiance bits for every sample (Bunny guarantees the answers do not depend on the tree),
//! and the example stops if one does not.

use std::time::{Duration, Instant};

use bunny_broadphase::{
    any_hit, build_bvh, build_bvh_sah, closest_hit, compact_any_hit, compact_bvh,
    compact_closest_hit, BvhNode, BvhView, ClosestHit, CompactChild, CompactNode, CompactRoot,
    CompactView, TraversalError,
};
use bunny_geom::FixedAabb3;
use bunny_linalg::FixedVec3;
use bunny_num::FixedQ32_32;
use bunny_query::{TraceRay, TraceTriangle};
use lumen::{reference_scene, trace_path, Hit, RenderSettings, Scene, TraceError, Tracer, FAR};

type Builder = fn(&mut [BvhNode], &mut [u32], &[FixedAabb3]) -> Option<usize>;

struct Plain<'a> {
    triangles: &'a [TraceTriangle],
    nodes: Vec<BvhNode>,
    prims: Vec<u32>,
}

struct Compact<'a> {
    triangles: &'a [TraceTriangle],
    root: Option<CompactRoot>,
    nodes: Vec<CompactNode>,
    prims: Vec<u32>,
}

fn zero_box() -> FixedAabb3 {
    let zero = FixedVec3::new(FixedQ32_32::ZERO, FixedQ32_32::ZERO, FixedQ32_32::ZERO);
    FixedAabb3::new(zero, zero)
}

fn plain(triangles: &[TraceTriangle], build: Builder) -> Plain<'_> {
    let boxes: Vec<FixedAabb3> = triangles.iter().map(TraceTriangle::bounds).collect();
    let empty = BvhNode { bounds: zero_box(), first_child_or_prim_idx: 0, prim_count: 0 };
    let mut nodes = vec![empty; 2 * boxes.len()];
    let mut prims = vec![0; boxes.len()];
    let used = build(&mut nodes, &mut prims, &boxes).expect("builds");
    nodes.truncate(used);
    Plain { triangles, nodes, prims }
}

fn compact<'a>(tree: &Plain<'a>) -> Compact<'a> {
    let blank = CompactNode {
        bounds: [zero_box(); 2],
        children: [CompactChild::default(), CompactChild::default()],
    };
    let mut nodes = vec![blank; tree.nodes.len() / 2];
    let (root, written) = compact_bvh(&tree.nodes, &mut nodes).expect("converts").expect("tree");
    nodes.truncate(written);
    Compact { triangles: tree.triangles, root: Some(root), nodes, prims: tree.prims.clone() }
}

fn closest_from(
    triangles: &[TraceTriangle],
    ray: &TraceRay,
    found: Result<Option<ClosestHit>, TraversalError>,
) -> Result<Option<Hit>, TraceError> {
    let Some(found) = found.map_err(TraceError::Traversal)? else { return Ok(None) };
    let triangle = &triangles[found.primitive as usize];
    let hit = ray.hit_triangle(triangle, found.distance, found.distance);
    Ok(Some(Hit { primitive: found.primitive, hit: hit.ok_or(TraceError::InconsistentHit)? }))
}

impl Tracer for Plain<'_> {
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
        let view = BvhView::new(&self.nodes, &self.prims);
        let found = closest_hit(view, ray, FAR, |p, limit| {
            ray.hit_triangle(&self.triangles[p as usize], FixedQ32_32::ZERO, limit)
                .map(|hit| hit.distance)
        });
        closest_from(self.triangles, ray, found)
    }

    fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError> {
        let view = BvhView::new(&self.nodes, &self.prims);
        any_hit(view, ray, t_max, |p| {
            ray.hit_triangle(&self.triangles[p as usize], FixedQ32_32::ZERO, t_max).is_some()
        })
        .map_err(TraceError::Traversal)
    }
}

impl Tracer for Compact<'_> {
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
        let view = CompactView { root: self.root, nodes: &self.nodes, prim_indices: &self.prims };
        let found = compact_closest_hit(view, ray, FAR, |p, limit| {
            ray.hit_triangle(&self.triangles[p as usize], FixedQ32_32::ZERO, limit)
                .map(|hit| hit.distance)
        });
        closest_from(self.triangles, ray, found)
    }

    fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError> {
        let view = CompactView { root: self.root, nodes: &self.nodes, prim_indices: &self.prims };
        compact_any_hit(view, ray, t_max, |p| {
            ray.hit_triangle(&self.triangles[p as usize], FixedQ32_32::ZERO, t_max).is_some()
        })
        .map_err(TraceError::Traversal)
    }
}

/// Renders every sample with `tracer`; returns the time, the rays and a digest of the bits.
fn run(
    scene: &Scene,
    tracer: &dyn Tracer,
    size: (u32, u32),
    settings: RenderSettings,
) -> (Duration, u64, u64) {
    let camera = scene.camera().camera(size.0, size.1).expect("camera");
    let started = Instant::now();
    let (mut rays, mut digest) = (0_u64, 0_u64);
    for y in 0..size.1 {
        for x in 0..size.0 {
            let sampler = settings.sampler(x, y);
            for sample in 0..settings.samples_per_pixel() {
                let path = trace_path(scene, tracer, &camera, sampler, sample).expect("traces");
                rays += path.rays.total();
                for c in path.radiance {
                    digest = digest.rotate_left(7) ^ c.raw().cast_unsigned();
                }
            }
        }
    }
    (started.elapsed(), rays, digest)
}

fn main() {
    let args: Vec<u32> =
        std::env::args().skip(1).map(|a| a.parse().expect("a whole number")).collect();
    let [width, height, samples, rounds] = match args.as_slice() {
        [] => [160, 90, 4, 7],
        [w, h, s, r] => [*w, *h, *s, *r],
        _ => panic!("usage: traversal [WIDTH HEIGHT SAMPLES ROUNDS]"),
    };
    let settings = RenderSettings::new(samples, 0).expect("power of two");
    let scene = reference_scene().expect("builds");
    let triangles = scene.triangles();
    let boxes: Vec<FixedAabb3> = triangles.iter().map(TraceTriangle::bounds).collect();
    for (name, build) in [("build_bvh", build_bvh as Builder), ("build_bvh_sah", build_bvh_sah)] {
        let mut times = Vec::new();
        for _ in 0..5 {
            let empty = BvhNode { bounds: zero_box(), first_child_or_prim_idx: 0, prim_count: 0 };
            let mut nodes = vec![empty; 2 * boxes.len()];
            let mut prims = vec![0; boxes.len()];
            let started = Instant::now();
            let _ = build(&mut nodes, &mut prims, &boxes).expect("builds");
            times.push(started.elapsed());
        }
        times.sort();
        println!("build {name}: median {:.2} ms over 5", times[2].as_secs_f64() * 1e3);
    }
    let quartile = plain(triangles, build_bvh);
    let sah = plain(triangles, build_bvh_sah);
    let quartile_compact = compact(&quartile);
    let sah_compact = compact(&sah);
    let tracers: [(&str, &dyn Tracer); 5] = [
        ("build_bvh + closest_hit", &quartile),
        ("build_bvh_sah + closest_hit", &sah),
        ("build_bvh + compact", &quartile_compact),
        ("build_bvh_sah + compact", &sah_compact),
        ("Scene (lumen's choice)", &scene),
    ];
    let mut times: Vec<Vec<Duration>> = vec![Vec::new(); tracers.len()];
    let mut reference: Option<(u64, u64)> = None;
    for round in 0..rounds as usize {
        for offset in 0..tracers.len() {
            let index = (round + offset) % tracers.len();
            let (time, rays, digest) = run(&scene, tracers[index].1, (width, height), settings);
            let expected = *reference.get_or_insert((rays, digest));
            assert_eq!((rays, digest), expected, "{} gives different bits", tracers[index].0);
            times[index].push(time);
        }
    }
    let rays = reference.map_or(0, |(rays, _)| rays);
    println!("{width}x{height}, {samples} samples per pixel, {rays} rays, {rounds} rounds");
    for ((name, _), mut runs) in tracers.iter().zip(times) {
        runs.sort();
        let median = runs[runs.len() / 2].as_secs_f64();
        #[allow(clippy::cast_precision_loss)]
        let ns = median * 1e9 / rays as f64;
        println!("{name:32} median {:8.1} ms  {ns:6.1} ns/ray", median * 1e3);
    }
}
