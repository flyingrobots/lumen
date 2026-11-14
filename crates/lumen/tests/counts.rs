//! Size class: medium (real threads at fixed counts; in process, no file, no network).
//!
//! What a render says it did, checked by something other than the code that did it: the number
//! of samples behind a pixel above 32, the ray counts of paths and renders against the queries a
//! wrapping tracer observed, and the seed's top and lowest bits against a recomputation that
//! builds each pixel's sampler itself.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

use bunny_linalg::{FixedUnitVec3, FixedVec3};
use bunny_num::FixedQ32_32;
use bunny_optics::{
    write_rgba8, Albedo, DirectionalLight, PixelAccumulator, PixelSampler, SkyLight,
};
use bunny_query::TraceRay;
use lumen::{
    pixel_radiance, reference_scene, render, render_traced, trace_path, BandRows, BuiltinScene,
    CameraPlacement, Hit, ImageSize, Lighting, RayCount, RenderSettings, Scene, SceneSpec,
    Schedule, TraceError, Tracer, TONE_CURVE,
};

const ONE: i64 = 1 << 32;

fn raw(n: i64) -> FixedQ32_32 {
    FixedQ32_32::from_raw(n)
}

/// A point in sixteenths of a unit.
fn p(x: i64, y: i64, z: i64) -> FixedVec3 {
    FixedVec3::new(raw(x * ONE / 16), raw(y * ONE / 16), raw(z * ONE / 16))
}

fn settings(samples: u32, seed: u32) -> RenderSettings {
    RenderSettings::new(samples, seed).expect("valid settings")
}

fn size(width: u32, height: u32) -> ImageSize {
    ImageSize::new(width, height).expect("valid size")
}

fn schedule(threads: u32) -> Schedule {
    Schedule::new(threads, BandRows::new(1).expect("rows")).expect("schedule")
}

/// The mean of samples `indices` of the pixel `sampler` belongs to, in a `width` by `height`
/// image: each a direct `trace_path` call, in one Bunny `PixelAccumulator`; and the sum of their
/// ray counts.
fn traced_mean(
    scene: &Scene,
    (width, height): (u32, u32),
    sampler: PixelSampler,
    indices: core::ops::Range<u32>,
) -> ([FixedQ32_32; 3], RayCount) {
    let camera = scene.camera().camera(width, height).expect("camera");
    let mut accumulator = PixelAccumulator::new();
    let mut rays = RayCount::default();
    for sample in indices {
        let path = trace_path(scene, scene, &camera, sampler, sample).expect("traces");
        accumulator.add_sample(path.radiance).expect("in range");
        rays = RayCount {
            closest: rays.closest + path.rays.closest,
            shadow: rays.shadow + path.rays.shadow,
        };
    }
    (accumulator.resolve().expect("resolves"), rays)
}

// ---------------------------------------------------------------------------------------------
// Sample counts above 32.

/// Oracle: the definition of a pixel, `samples` direct `trace_path` calls in one accumulator,
/// with no use of `pixel_radiance`'s loop. At 64 and 1024 samples it equals `pixel_radiance`, and
/// the ray count equals the sum of the paths'. Witness: for each pixel the mean of the first 32
/// samples differs from the full mean, so a loop that stopped at 32 would be seen.
#[test]
fn a_pixel_above_32_samples_is_the_mean_of_that_many_traced_paths() {
    let scene = reference_scene().expect("builds");
    let camera = scene.camera().camera(32, 18).expect("camera");
    // The bunny, a shadowed ground pixel and the sphere.
    for (x, y) in [(8, 12), (10, 9), (24, 9)] {
        for samples in [64, 1024] {
            let settings = settings(samples, 3);
            let sampler = settings.sampler(x, y);
            let want = traced_mean(&scene, (32, 18), sampler, 0..samples);
            let got = pixel_radiance(&scene, &camera, settings, x, y).expect("renders");
            assert_eq!(got, want, "pixel ({x}, {y}) at {samples} samples");
            let first_32 = traced_mean(&scene, (32, 18), sampler, 0..32).0;
            assert_ne!(first_32, want.0, "pixel ({x}, {y}): 32 samples already give the mean");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Ray counts against the queries a tracer observed.

/// Answers every query with the scene and counts it: closest-hit queries from the camera's eye
/// point (camera rays: Bunny's pinhole camera starts every ray there), other closest-hit queries
/// (bounce rays, which start at a surface's start point) and any-hit queries (shadow rays).
/// Atomic, so a render on several threads can share it; the counts are read after the render,
/// whose `join` orders them.
struct Counting<'a> {
    scene: &'a Scene,
    eye: FixedVec3,
    camera: AtomicU64,
    bounce: AtomicU64,
    shadow: AtomicU64,
}

impl<'a> Counting<'a> {
    fn new(scene: &'a Scene, (width, height): (u32, u32)) -> Self {
        let eye = scene.camera().camera(width, height).expect("camera").position();
        let zero = || AtomicU64::new(0);
        Self { scene, eye, camera: zero(), bounce: zero(), shadow: zero() }
    }

    /// The counts as `RayCount` reports them: camera and bounce rays are closest-hit queries.
    fn observed(&self) -> RayCount {
        let get = |n: &AtomicU64| n.load(Ordering::Relaxed);
        RayCount { closest: get(&self.camera) + get(&self.bounce), shadow: get(&self.shadow) }
    }

    fn camera_rays(&self) -> u64 {
        self.camera.load(Ordering::Relaxed)
    }
}

impl Tracer for Counting<'_> {
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
        let kind = if ray.origin() == self.eye { &self.camera } else { &self.bounce };
        kind.fetch_add(1, Ordering::Relaxed);
        self.scene.closest(ray)
    }

    fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError> {
        self.shadow.fetch_add(1, Ordering::Relaxed);
        self.scene.occluded(ray, t_max)
    }
}

/// Renders `scene` through a counting tracer at 1, 2 and 4 threads and checks, at each:
/// the image is the scene's own; `Render::rays` equals the queries observed, per kind; camera
/// rays are exactly `width * height * samples`; and the total lies between that and four times
/// that (a path traces a camera ray, at most one bounce ray and at most one shadow ray at each of
/// its two hits). Returns the counts at one thread.
fn check_render(scene: &Scene, (width, height): (u32, u32), samples: u32) -> Counting<'_> {
    let (image, settings) = (size(width, height), settings(samples, 11));
    let camera_rays = u64::from(width * height * samples);
    let own = render(scene, image, settings).expect("renders");
    let mut first = None;
    for threads in [1, 2, 4] {
        let at = format!("{width}x{height}, {samples} samples, {threads} threads");
        let counting = Counting::new(scene, (width, height));
        let got = render_traced(scene, &counting, image, settings, schedule(threads)).expect(&at);
        assert_eq!(got.image, own.image, "{at}: the counting tracer changes nothing");
        assert_eq!(got.rays, counting.observed(), "{at}: reported against observed");
        assert_eq!(counting.camera_rays(), camera_rays, "{at}: one camera ray per sample");
        let total = got.rays.total();
        assert!((camera_rays..=4 * camera_rays).contains(&total), "{at}: {total} rays");
        first.get_or_insert(counting);
    }
    first.expect("three renders")
}

/// Oracle: the queries a wrapping tracer saw, counted by kind, for each built-in scene. The
/// sizes include 64 and 1024 samples per pixel. Witness: every kind of ray occurred.
#[test]
fn a_render_reports_the_rays_its_tracer_was_asked_for() {
    for builtin in BuiltinScene::ALL {
        let scene = builtin.build().expect("builds");
        for (width, height, samples) in [(16, 9, 4), (4, 2, 64), (2, 1, 1024)] {
            let counting = check_render(&scene, (width, height), samples);
            let name = builtin.name();
            assert!(counting.bounce.load(Ordering::Relaxed) > 0, "{name}: no bounce ray");
            assert!(counting.shadow.load(Ordering::Relaxed) > 0, "{name}: no shadow ray");
        }
    }
}

/// Counts the queries of one path, on one thread.
struct PathCount<'a> {
    scene: &'a Scene,
    closest: Cell<u64>,
    shadow: Cell<u64>,
}

impl Tracer for PathCount<'_> {
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
        self.closest.set(self.closest.get() + 1);
        self.scene.closest(ray)
    }

    fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError> {
        self.shadow.set(self.shadow.get() + 1);
        self.scene.occluded(ray, t_max)
    }
}

/// Oracle: the queries one path made, counted by a fresh tracer for each path. Every sample of a
/// 16 by 9 image of each built-in scene reports exactly them. Witness: paths that end in the
/// sky after one query, paths with two closest-hit queries, and paths with zero, one and two
/// shadow rays all occurred.
#[test]
fn every_path_reports_the_queries_it_made() {
    for builtin in BuiltinScene::ALL {
        let scene = builtin.build().expect("builds");
        let camera = scene.camera().camera(16, 9).expect("camera");
        let settings = settings(2, 5);
        let mut seen = [[0_u32; 3]; 3];
        for y in 0..9 {
            for x in 0..16 {
                for sample in 0..2 {
                    let counting =
                        PathCount { scene: &scene, closest: Cell::new(0), shadow: Cell::new(0) };
                    let sampler = settings.sampler(x, y);
                    let path =
                        trace_path(&scene, &counting, &camera, sampler, sample).expect("traces");
                    let observed =
                        RayCount { closest: counting.closest.get(), shadow: counting.shadow.get() };
                    let at = format!("{}: pixel ({x}, {y}) sample {sample}", builtin.name());
                    assert_eq!(path.rays, observed, "{at}");
                    let closest = usize::try_from(observed.closest).expect("small");
                    let shadow = usize::try_from(observed.shadow).expect("small");
                    seen[closest.min(2)][shadow.min(2)] += 1;
                }
            }
        }
        let name = builtin.name();
        assert!(seen[1][0] > 0 && seen[2][1] > 0 && seen[2][2] > 0, "{name}: {seen:?}");
    }
}

/// Oracle: a camera that sees only sky. A ground faces up, a sun shines on it, and the camera is
/// above it looking straight up, so every camera ray misses: a miss ends the path with no
/// shadow and no bounce ray, and the total is exactly the camera rays.
#[test]
fn a_camera_that_sees_only_sky_traces_only_camera_rays() {
    let ground = [p(-256, 0, 256), p(256, 0, 256), p(0, 0, -256)];
    let sun = DirectionalLight::new(p(0, 16, 0), [raw(ONE); 3]).expect("valid sun");
    let lighting =
        Lighting { sun: Some(sun), sky: SkyLight::new([raw(ONE / 4); 3]).expect("valid sky") };
    let camera = CameraPlacement {
        position: p(0, 64, 0),
        look: p(0, 16, 0),
        up: p(0, 0, -16),
        tan_half_fov: raw(ONE / 2),
    };
    let scene = Scene::new(&SceneSpec {
        triangles: &[ground],
        material_of: &[0],
        materials: &[Albedo::new([raw(ONE / 2); 3]).expect("valid albedo")],
        normals: &[],
        lighting,
        camera,
    })
    .expect("the scene builds");
    let counting = check_render(&scene, (8, 8), 4);
    assert_eq!(counting.observed(), RayCount { closest: 8 * 8 * 4, shadow: 0 });
}

/// Oracle: the queries observed, on a scene shaded by `lumen-smooth-diffuse-path/1`. A floor
/// whose vertex normals lean 53 degrees from its geometric normal, under a sun straight above:
/// Bunny's smooth shading rejects some bounces as below the surface, and a rejected bounce traces
/// no ray (and no shadow ray at a second hit), so it is not counted. The render's counts still
/// equal the queries observed, and every path's count its own. Witness: fewer bounce rays than
/// camera rays (some bounces were rejected), and some bounce rays and shadow rays.
#[test]
fn a_rejected_smooth_bounce_is_not_counted_as_a_ray() {
    let floor = [p(-256, 0, -256), p(256, 0, -256), p(0, 0, 256)];
    let tilted = FixedUnitVec3::new(p(64, 48, 0)).expect("a unit vector");
    let sun = DirectionalLight::new(p(0, 16, 0), [raw(ONE); 3]).expect("valid sun");
    let lighting =
        Lighting { sun: Some(sun), sky: SkyLight::new([raw(ONE); 3]).expect("valid sky") };
    let camera = CameraPlacement {
        position: p(0, 64, 0),
        look: p(0, -16, 0),
        up: p(0, 0, -16),
        tan_half_fov: raw(ONE / 2),
    };
    let scene = Scene::new(&SceneSpec {
        triangles: &[floor],
        material_of: &[0],
        materials: &[Albedo::new([raw(ONE / 2); 3]).expect("valid albedo")],
        normals: &[[Some(tilted); 3]],
        lighting,
        camera,
    })
    .expect("the scene builds");
    assert_eq!(scene.smooth_triangles(), 1);
    let counting = check_render(&scene, (8, 8), 8);
    let (camera_rays, bounce) = (counting.camera_rays(), counting.bounce.load(Ordering::Relaxed));
    assert!(0 < bounce && bounce < camera_rays, "{bounce} bounces of {camera_rays} paths");
    assert!(counting.shadow.load(Ordering::Relaxed) > 0, "no shadow ray");
}

// ---------------------------------------------------------------------------------------------
// The seed's bits.

/// The image of `scene` at `width` by `height`, `samples` samples and `seed`, recomputed without
/// `RenderSettings::sampler`, `pixel_radiance` or the scheduler: each pixel's sampler is Bunny's
/// `PixelSampler::new` of the seed widened to `u64`, its samples are direct `trace_path` calls
/// in one accumulator, and its bytes Bunny's `write_rgba8` through the tone curve, in raster
/// order.
fn recomputed(scene: &Scene, (width, height): (u32, u32), spp: u32, seed: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let sampler = PixelSampler::new(u64::from(seed), x, y);
            let (radiance, _) = traced_mean(scene, (width, height), sampler, 0..spp);
            let mut rgba = [0_u8; 4];
            write_rgba8(&mut rgba, TONE_CURVE, radiance).expect("writes");
            bytes.extend_from_slice(&rgba);
        }
    }
    bytes
}

/// Oracle: the recomputation above, and inequality. For seeds that differ only in the top bit
/// (bit 31), and for seeds that differ only in the lowest bit, each image equals its own
/// recomputation and the two images differ: no bit of the seed is dropped on the way to Bunny's
/// sampler.
#[test]
fn seeds_that_differ_in_one_bit_give_different_images() {
    let scene = reference_scene().expect("builds");
    let (width, height, samples) = (16, 9, 2);
    let pairs =
        [(0x0000_0000, 0x8000_0000), (0x7FFF_FFFF, 0xFFFF_FFFF), (0x1234_5678, 0x1234_5679)];
    for (a, b) in pairs {
        let mut images = Vec::new();
        for seed in [a, b] {
            let got =
                render(&scene, size(width, height), settings(samples, seed)).expect("renders");
            let want = recomputed(&scene, (width, height), samples, seed);
            assert_eq!(got.image.bytes(), want.as_slice(), "seed {seed:#010x}");
            images.push(want);
        }
        assert_ne!(images[0], images[1], "seeds {a:#010x} and {b:#010x} give the same image");
    }
}
