//! Size class: medium (real threads at fixed counts; in process, no file, no network).
//!
//! What a render says its rays found (`Render::statistics`, ADR 0008), checked against a tracer
//! that wraps the scene and records every answer it gives, against each pixel's ray count from
//! `pixel_radiance`, and on scenes built so the counts are known: only sky, a shadow over every
//! visible point, and bounces Bunny's smooth shading rejects.

use std::sync::atomic::{AtomicU64, Ordering};

use bunny_linalg::{FixedUnitVec3, FixedVec3};
use bunny_num::FixedQ32_32;
use bunny_optics::{Albedo, DirectionalLight, SkyLight};
use bunny_query::TraceRay;
use lumen::{
    pixel_radiance, render, render_traced, BandRows, BuiltinScene, CameraPlacement, Hit, ImageSize,
    Lighting, RayCheckError, RayStatistics, RenderSettings, Scene, SceneSpec, Schedule, TraceError,
    Tracer,
};

const ONE: i64 = 1 << 32;

fn raw(n: i64) -> FixedQ32_32 {
    FixedQ32_32::from_raw(n)
}

/// A point in sixteenths of a unit.
fn p(x: i64, y: i64, z: i64) -> FixedVec3 {
    FixedVec3::new(raw(x * ONE / 16), raw(y * ONE / 16), raw(z * ONE / 16))
}

/// Answers every query with the scene and records the answer by kind: closest-hit queries from
/// the camera's eye point (camera rays), other closest-hit queries (bounce rays) and any-hit
/// queries (shadow rays). Atomic, so threads can share it; read after the render's `join`.
struct Observing<'a> {
    scene: &'a Scene,
    eye: FixedVec3,
    /// Camera hits, camera misses, bounce hits, bounce misses, occluded, unoccluded.
    seen: [AtomicU64; 6],
}

impl<'a> Observing<'a> {
    fn new(scene: &'a Scene, (width, height): (u32, u32)) -> Self {
        let eye = scene.camera().camera(width, height).expect("camera").position();
        Self { scene, eye, seen: Default::default() }
    }

    fn get(&self, slot: usize) -> u64 {
        self.seen[slot].load(Ordering::Relaxed)
    }

    fn bump(&self, slot: usize) {
        self.seen[slot].fetch_add(1, Ordering::Relaxed);
    }
}

impl Tracer for Observing<'_> {
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
        let found = self.scene.closest(ray)?;
        let camera = if ray.origin() == self.eye { 0 } else { 2 };
        self.bump(camera + usize::from(found.is_none()));
        Ok(found)
    }

    fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError> {
        let occluded = self.scene.occluded(ray, t_max)?;
        self.bump(4 + usize::from(!occluded));
        Ok(occluded)
    }
}

/// Renders `scene` through the observing tracer at 1, 2, 4 and 18 threads and band heights 1 and
/// 3, and checks at each: the image is the scene's own; every outcome equals the answers
/// observed; the rejected bounces are the camera hits that traced no bounce; the statistics pass
/// their own checks and are the same at every schedule. Returns them. A schedule uses at most
/// one worker per band, so on these small images 18 threads are as many workers as the image has
/// bands of that height (9 at most), not 18.
fn observed_render(scene: &Scene, (width, height): (u32, u32), samples: u32) -> RayStatistics {
    let (image, settings) = (size(width, height), settings(samples));
    let own = render(scene, image, settings).expect("renders");
    for (threads, band) in [(1, 1), (2, 1), (4, 3), (18, 1), (18, 3)] {
        let at = format!("{width}x{height}, {samples} samples, {threads} threads, band {band}");
        let schedule = Schedule::new(threads, BandRows::new(band).expect("rows")).expect("ok");
        let observing = Observing::new(scene, (width, height));
        let got = render_traced(scene, &observing, image, settings, schedule).expect(&at);
        assert_eq!(got.image, own.image, "{at}: the image");
        let found = got.statistics.outcomes;
        let seen = |slot| observing.get(slot);
        assert_eq!(found.camera_hits, seen(0), "{at}: camera hits");
        assert_eq!(found.camera_misses, seen(1), "{at}: camera misses");
        assert_eq!(found.bounce_hits, seen(2), "{at}: bounce hits");
        assert_eq!(found.bounce_misses, seen(3), "{at}: bounce misses");
        assert_eq!(found.shadow_occluded, seen(4), "{at}: occluded shadow rays");
        assert_eq!(found.shadow_unoccluded, seen(5), "{at}: unoccluded shadow rays");
        assert_eq!(found.rejected_bounces, seen(0) - seen(2) - seen(3), "{at}: rejected");
        assert_eq!(got.rays, got.statistics.count(), "{at}: the ray count");
        assert_eq!(got.statistics.check(image, settings), Ok(()), "{at}");
        assert_eq!(got.statistics, own.statistics, "{at}: the same at every schedule");
    }
    own.statistics
}

fn settings(samples: u32) -> RenderSettings {
    RenderSettings::new(samples, 11).expect("valid settings")
}

fn size(width: u32, height: u32) -> ImageSize {
    ImageSize::new(width, height).expect("valid size")
}

/// Oracle: the observing tracer, and each pixel's rays from `pixel_radiance`, for both built-in
/// scenes: the fewest and most rays of any pixel, the pixel count and the samples. Witness: every
/// outcome but a rejected bounce occurs (both scenes are flat).
#[test]
fn a_render_reports_what_its_tracer_answered() {
    for builtin in BuiltinScene::ALL {
        let scene = builtin.build().expect("builds");
        let (width, height, samples) = (16, 9, 4);
        let stats = observed_render(&scene, (width, height), samples);
        let camera = scene.camera().camera(width, height).expect("camera");
        let per_pixel: Vec<u64> = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| pixel_radiance(&scene, &camera, settings(samples), x, y))
            .map(|result| result.expect("renders").1.total())
            .collect();
        let name = builtin.name();
        assert_eq!(stats.min_pixel_rays, *per_pixel.iter().min().expect("pixels"), "{name}");
        assert_eq!(stats.max_pixel_rays, *per_pixel.iter().max().expect("pixels"), "{name}");
        assert_eq!(stats.pixels, 144, "{name}");
        assert_eq!(stats.samples, 144 * 4, "{name}");
        assert!(stats.min_pixel_rays < stats.max_pixel_rays, "{name}: {stats:?}");
        let found = stats.outcomes;
        let each = [found.camera_hits, found.camera_misses, found.bounce_hits, found.bounce_misses];
        assert!(each.iter().all(|&n| n > 0), "{name}: {found:?}");
        assert!(found.shadow_occluded > 0 && found.shadow_unoccluded > 0, "{name}: {found:?}");
    }
}

/// A one-material scene with a sun straight above and the camera at `position` looking `look`.
fn scene(
    triangles: &[[FixedVec3; 3]],
    normals: &[[Option<FixedUnitVec3>; 3]],
    look: (FixedVec3, FixedVec3),
) -> Scene {
    let sun = DirectionalLight::new(p(0, 16, 0), [raw(ONE); 3]).expect("valid sun");
    let sky = SkyLight::new([raw(ONE / 4); 3]).expect("valid sky");
    Scene::new(&SceneSpec {
        triangles,
        material_of: &vec![0; triangles.len()],
        materials: &[Albedo::new([raw(ONE / 2); 3]).expect("valid albedo")],
        normals,
        lighting: Lighting { sun: Some(sun), sky },
        camera: CameraPlacement {
            position: look.0,
            look: look.1,
            up: p(0, 0, -16),
            tan_half_fov: raw(ONE / 2),
        },
    })
    .expect("the scene builds")
}

/// Oracle: construction. The camera looks straight up from above the only triangle, so every
/// camera ray misses: no hit, no bounce, no shadow ray.
#[test]
fn a_camera_that_sees_only_sky_reports_only_camera_misses() {
    let ground = [p(-256, 0, 256), p(256, 0, 256), p(0, 0, -256)];
    let stats = observed_render(&scene(&[ground], &[], (p(0, 64, 0), p(0, 16, 0))), (8, 8), 4);
    let found = stats.outcomes;
    assert_eq!((found.camera_misses, found.camera_hits), (256, 0));
    assert_eq!((found.bounce(), found.rejected_bounces, found.shadow()), (0, 0, 0));
    assert_eq!((stats.min_pixel_rays, stats.max_pixel_rays), (4, 4));
}

/// Oracle: construction. The camera, between a floor and a ceiling that each reach far past
/// its view, looks down at the floor under a sun straight above: every camera ray hits the
/// floor, and every shadow ray from the floor meets the ceiling. A bounce that hits the ceiling
/// meets its underside, which faces away from the sun, so it has no shadow ray. So the shadow
/// rays are exactly the camera rays, and every one is occluded.
#[test]
fn a_ceiling_over_every_visible_point_occludes_every_shadow_ray() {
    let floor = [p(-4000, 0, 4000), p(4000, 0, 4000), p(0, 0, -4000)];
    let ceiling = [p(-4000, 64, 4000), p(0, 64, -4000), p(4000, 64, 4000)];
    let view = (p(0, 32, 0), p(0, -16, 0));
    let stats = observed_render(&scene(&[floor, ceiling], &[], view), (8, 8), 4);
    let found = stats.outcomes;
    assert_eq!((found.camera_hits, found.camera_misses), (256, 0));
    assert_eq!((found.shadow_occluded, found.shadow_unoccluded), (256, 0));
    assert_eq!(found.bounce(), 256, "a flat hit always bounces");
    // Witness: some bounces hit the ceiling and traced no shadow ray of their own.
    assert!(found.bounce_hits > 0, "{found:?}");
}

/// Oracle: the observing tracer, on a floor whose vertex normals lean 53 degrees from its
/// geometric normal (ADR 0007): Bunny rejects some bounces as below the surface. Witness: some
/// bounces are rejected and some are traced.
#[test]
fn a_rejected_smooth_bounce_is_counted_as_rejected_not_as_a_ray() {
    let floor = [p(-256, 0, -256), p(256, 0, -256), p(0, 0, 256)];
    let tilted = FixedUnitVec3::new(p(64, 48, 0)).expect("a unit vector");
    let floor = scene(&[floor], &[[Some(tilted); 3]], (p(0, 64, 0), p(0, -16, 0)));
    let found = observed_render(&floor, (8, 8), 8).outcomes;
    assert_eq!(found.camera_hits, 512);
    assert!(found.rejected_bounces > 0 && found.bounce() > 0, "{found:?}");
    assert_eq!(found.rejected_bounces + found.bounce(), 512);
}

/// Oracle: each check, written out, against statistics changed by one count.
#[test]
fn the_checks_name_the_count_that_is_wrong() {
    let scene = lumen::reference_scene().expect("builds");
    let (image, settings) = (size(4, 3), settings(2));
    let good = render(&scene, image, settings).expect("renders").statistics;
    assert_eq!(good.check(image, settings), Ok(()));
    let mut wrong = good;
    wrong.pixels -= 1;
    assert_eq!(
        wrong.check(image, settings),
        Err(RayCheckError::Pixels { expected: 12, observed: 11 })
    );
    let mut wrong = good;
    wrong.samples += 1;
    assert_eq!(
        wrong.check(image, settings),
        Err(RayCheckError::Samples { expected: 24, observed: 25 })
    );
    let mut wrong = good;
    wrong.outcomes.camera_misses += 1;
    let observed = good.outcomes.camera() + 1;
    assert_eq!(
        wrong.check(image, settings),
        Err(RayCheckError::CameraRays { expected: 24, observed })
    );
    let mut wrong = good;
    wrong.outcomes.shadow_occluded += 100;
    let observed = good.count().total() + 100;
    assert_eq!(
        wrong.check(image, settings),
        Err(RayCheckError::TooManyRays { limit: 96, observed })
    );
}
