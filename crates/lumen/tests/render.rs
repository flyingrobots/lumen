//! Size class: small (in process, no file, no network).
//!
//! Render tests: hand-derived radiance, shadows against exact geometry, every traced ray against
//! brute force, an independent composition of the documented order of operations, order
//! independence, convergence, determinism and the golden image hash.

use bunny_broadphase::{any_hit, build_bvh, closest_hit, BvhNode, BvhView};
use bunny_geom::FixedAabb3;
use bunny_linalg::FixedVec3;
use bunny_num::fixed_q32_32::ONE_RAW;
use bunny_num::FixedQ32_32;
use bunny_optics::{
    write_rgba8, Albedo, DirectionalLight, PinholeCamera, PixelAccumulator, PixelSample,
    PixelSampler, SkyLight, SurfaceHit, FRAC_1_PI, SHADOW_T_MAX,
};
use bunny_query::{TraceRay, TraceTriangle};
use lumen::reference::{reference_camera, BUNNY_FIRST_PRIMITIVE, REFERENCE_TRIANGLE_COUNT};
use lumen::{
    pixel_radiance, reference_scene, render, render_reference, trace_path, CameraPlacement, Hit,
    ImageSize, Lighting, PrimitiveIndex, RenderSettings, Rgba8Image, Scene, SceneSpec, TraceError,
    Tracer, BUNNY_REV, FAR, TONE_CURVE,
};
use std::cell::Cell;

mod golden;
use golden::{GOLDEN_96X54_4SPP, GOLDEN_BUNNY_REV};

// ---------------------------------------------------------------------------------------------
// Helpers.

const ZERO3: [FixedQ32_32; 3] = [FixedQ32_32::ZERO; 3];

fn raw(raw: i64) -> FixedQ32_32 {
    FixedQ32_32::from_raw(raw)
}

/// `n / d` units, for `d` a power of two up to `2^32`.
fn frac(n: i64, d: i64) -> FixedQ32_32 {
    raw(n * (ONE_RAW / d))
}

fn units(n: i64) -> FixedQ32_32 {
    raw(n * ONE_RAW)
}

fn point(x: i64, y: i64, z: i64) -> FixedVec3 {
    FixedVec3::new(units(x), units(y), units(z))
}

fn vec3(x: FixedQ32_32, y: FixedQ32_32, z: FixedQ32_32) -> FixedVec3 {
    FixedVec3::new(x, y, z)
}

fn settings(samples: u32, seed: u32) -> RenderSettings {
    RenderSettings::new(samples, seed).expect("valid settings")
}

fn albedo(channels: [FixedQ32_32; 3]) -> Albedo {
    Albedo::new(channels).expect("valid albedo")
}

fn sky(radiance: [FixedQ32_32; 3]) -> SkyLight {
    SkyLight::new(radiance).expect("valid sky")
}

fn sun(toward: FixedVec3, irradiance: [FixedQ32_32; 3]) -> DirectionalLight {
    DirectionalLight::new(toward, irradiance).expect("valid light")
}

/// A scene with one material per triangle.
fn build(
    triangles: &[[FixedVec3; 3]],
    albedos: &[Albedo],
    lighting: Lighting,
    camera: CameraPlacement,
) -> Scene {
    let material_of: Vec<u32> = (0..u32::try_from(triangles.len()).expect("fits")).collect();
    Scene::new(&SceneSpec {
        triangles,
        material_of: &material_of,
        materials: albedos,
        normals: &[],
        lighting,
        camera,
    })
    .expect("the test scene builds")
}

/// A ground triangle at `y = 0`, facing `+Y`, wide enough (128 units) to fill every view here.
fn ground() -> [FixedVec3; 3] {
    // cross((128, 0, 0), (64, 0, -128)) = (0, 16384, 0): +Y.
    [point(-64, 0, 64), point(64, 0, 64), point(0, 0, -64)]
}

/// The same triangle at height `y` (any raw value), facing `+Y`.
fn plate(y: FixedQ32_32) -> [FixedVec3; 3] {
    ground().map(|v| vec3(v.x, y, v.z))
}

/// A camera at `position` looking straight down, `-Z` at the top of the image.
fn looking_down(position: FixedVec3, tan_half_fov: FixedQ32_32) -> CameraPlacement {
    CameraPlacement { position, look: point(0, -1, 0), up: point(0, 0, -1), tan_half_fov }
}

fn camera(scene: &Scene, width: u32, height: u32) -> PinholeCamera {
    scene.camera().camera(width, height).expect("the camera accepts the size")
}

/// Every sample's radiance for every pixel of a `width` by `height` image.
fn each_sample(
    scene: &Scene,
    (width, height): (u32, u32),
    settings: RenderSettings,
    mut check: impl FnMut(u32, u32, u32, [FixedQ32_32; 3]),
) {
    let camera = camera(scene, width, height);
    for y in 0..height {
        for x in 0..width {
            for sample in 0..settings.samples_per_pixel() {
                let path = trace_path(scene, scene, &camera, settings.sampler(x, y), sample)
                    .expect("traces");
                check(x, y, sample, path.radiance);
            }
        }
    }
}

/// The resolved radiance of every pixel at `spp` samples.
fn resolved(
    scene: &Scene,
    (width, height): (u32, u32),
    settings: RenderSettings,
) -> Vec<[FixedQ32_32; 3]> {
    let camera = camera(scene, width, height);
    let mut out = Vec::new();
    for y in 0..height {
        for x in 0..width {
            out.push(pixel_radiance(scene, &camera, settings, x, y).expect("renders").0);
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Brute force: a test of every triangle.

/// The nearest hit by testing every triangle: the smallest distance, then the lowest index.
fn brute_closest(scene: &Scene, ray: &TraceRay) -> Option<Hit> {
    let mut best: Option<Hit> = None;
    for (index, triangle) in scene.triangles().iter().enumerate() {
        if let Some(hit) = ray.hit_triangle(triangle, FixedQ32_32::ZERO, FAR) {
            if best.is_none_or(|b| hit.distance < b.hit.distance) {
                let primitive = PrimitiveIndex::new(u32::try_from(index).expect("fits"));
                best = Some(Hit { primitive, hit });
            }
        }
    }
    best
}

/// Whether a reference primitive belongs to the bunny.
fn is_bunny(primitive: PrimitiveIndex) -> bool {
    (BUNNY_FIRST_PRIMITIVE..REFERENCE_TRIANGLE_COUNT).contains(&primitive.to_usize())
}

/// Whether any triangle is hit within `0..=t_max`.
fn brute_occluded(scene: &Scene, ray: &TraceRay, t_max: FixedQ32_32) -> bool {
    scene.triangles().iter().any(|t| ray.hit_triangle(t, FixedQ32_32::ZERO, t_max).is_some())
}

/// Answers with the scene, after checking every answer against brute force, and counts what it
/// saw.
#[derive(Default)]
struct Counts {
    closest_hits: Cell<u32>,
    bunny_hits: Cell<u32>,
    closest_misses: Cell<u32>,
    occluded: Cell<u32>,
    visible: Cell<u32>,
    shadow_limits_wrong: Cell<u32>,
}

struct Checked<'a> {
    scene: &'a Scene,
    counts: Counts,
}

fn bump(cell: &Cell<u32>) {
    cell.set(cell.get() + 1);
}

impl Tracer for Checked<'_> {
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
        let found = self.scene.closest(ray)?;
        assert_eq!(found, brute_closest(self.scene, ray), "closest hit of {ray:?}");
        bump(if found.is_some() { &self.counts.closest_hits } else { &self.counts.closest_misses });
        if found.is_some_and(|hit| is_bunny(hit.primitive)) {
            bump(&self.counts.bunny_hits);
        }
        Ok(found)
    }

    fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError> {
        let found = self.scene.occluded(ray, t_max)?;
        assert_eq!(found, brute_occluded(self.scene, ray, t_max), "any hit of {ray:?}");
        if t_max != SHADOW_T_MAX {
            bump(&self.counts.shadow_limits_wrong);
        }
        bump(if found { &self.counts.occluded } else { &self.counts.visible });
        Ok(found)
    }
}

// ---------------------------------------------------------------------------------------------
// An independent composition of Bunny's documented order of operations.

/// Bunny's order of operations for one path, written out again from
/// `docs/topics/optics/README.md` ("Order Of Operations") with brute-force queries, to compare
/// with lumen's `trace_path` bit for bit.
fn reference_path(
    scene: &Scene,
    camera: &PinholeCamera,
    sampler: PixelSampler,
    sample: u32,
) -> [FixedQ32_32; 3] {
    let lighting = scene.lighting();
    let direct = |h: &SurfaceHit| match lighting.sun {
        None => ZERO3,
        Some(light) => {
            let s = h.light_sample(&light);
            let visible = s.shadow_ray().is_some_and(|r| !brute_occluded(scene, r, SHADOW_T_MAX));
            s.radiance(visible)
        }
    };
    // 1. camera ray, pair 0.
    let ray0 = camera.ray(sampler.camera_sample(sample)).expect("inside the image");
    // 2. primary.
    let Some(hit1) = brute_closest(scene, &ray0) else { return lighting.sky.radiance() };
    // 3. hit 1.
    let surface1 = scene.surface(hit1.primitive).expect("surface");
    let h1 = SurfaceHit::new(surface1, &ray0, &hit1.hit).expect("start point");
    // 4. direct 1.
    let d1 = direct(&h1);
    // 5. bounce, pair 1.
    let ray1 = h1.bounce_ray(sampler.point(sample, 1)).expect("bounce ray");
    let b = match brute_closest(scene, &ray1) {
        None => lighting.sky.radiance(),
        Some(hit2) => {
            let surface2 = scene.surface(hit2.primitive).expect("surface");
            let h2 = SurfaceHit::new(surface2, &ray1, &hit2.hit).expect("start point");
            let d2 = direct(&h2);
            scene.albedo(hit2.primitive).expect("albedo").reflect(d2, ZERO3).expect("sum")
        }
    };
    // 6. sample.
    scene.albedo(hit1.primitive).expect("albedo").reflect(d1, b).expect("sum")
}

// ---------------------------------------------------------------------------------------------
// Hand-derived radiance.

/// A sun straight above (`(0, 1, 0)`), irradiance 3.0, over the ground triangle of albedo 0.5,
/// seen from 4 units above, sky zero.
fn lit_ground(extra: &[[FixedVec3; 3]], toward: FixedVec3) -> Scene {
    let mut triangles = vec![ground()];
    triangles.extend_from_slice(extra);
    let albedos = vec![albedo([frac(1, 2); 3]); triangles.len()];
    let lighting = Lighting { sun: Some(sun(toward, [units(3); 3])), sky: sky(ZERO3) };
    build(&triangles, &albedos, lighting, looking_down(point(0, 4, 0), frac(1, 2)))
}

/// By hand. The ground's unit normal is exactly `(0, 1, 0)` and so is the sun's unit
/// direction, so the cosine is exactly 1.0 (raw `2^32`). The light's weight is
/// `1.0 * FRAC_1_PI` = 1 367 130 551 raw; the term is `3.0 * weight` = 4 101 391 653 raw (exact:
/// an integer times a raw value). Every bounce leaves upward, where nothing is, so it sees the
/// zero sky: `B = 0`. Then `L = 0.5 * (4 101 391 653 + 0)` = 2 050 695 826.5 raw, a tie, which
/// goes to the even 2 050 695 826.
const LIT_GROUND_RAW: i64 = 2_050_695_826;

#[test]
fn a_lit_triangle_facing_the_light_has_its_hand_derived_radiance() {
    let scene = lit_ground(&[], point(0, 1, 0));
    // The same value from the documented formulas with Bunny's own arithmetic.
    let weight = FixedQ32_32::ONE * FRAC_1_PI;
    let term = units(3) * weight;
    assert_eq!((weight.raw(), term.raw()), (1_367_130_551, 4_101_391_653));
    assert_eq!((frac(1, 2) * (term + FixedQ32_32::ZERO)).raw(), LIT_GROUND_RAW);
    let want = [raw(LIT_GROUND_RAW); 3];
    let mut count = 0;
    each_sample(&scene, (4, 4), settings(16, 0), |x, y, sample, radiance| {
        assert_eq!(radiance, want, "pixel ({x}, {y}) sample {sample}");
        count += 1;
    });
    assert_eq!(count, 4 * 4 * 16);
    for spp in [1, 4, 64] {
        assert!(resolved(&scene, (4, 4), settings(spp, 3)).iter().all(|&r| r == want), "{spp}");
        let image = render(&scene, ImageSize::new(4, 4).expect("size"), settings(spp, 3))
            .expect("renders")
            .image;
        let mut code = [0_u8; 4];
        write_rgba8(&mut code, TONE_CURVE, want).expect("four bytes");
        assert!(image.bytes().chunks_exact(4).all(|pixel| pixel == code), "{spp}");
    }
}

/// The sun at 45 degrees, `(0, 1, 1)`: the cosine is Bunny's `DirectionalLight::cosine` of the
/// normal `(0, 1, 0)`, within a raw unit of `2^32 / sqrt(2)` = 3 037 000 499.98, and the
/// radiance is `0.5 * (3.0 * (cosine * FRAC_1_PI))`. A path that drops the cosine gets the
/// value of the light straight above instead.
#[test]
fn a_lit_triangle_at_an_angle_carries_the_cosine() {
    let toward = point(0, 1, 1);
    let scene = lit_ground(&[], toward);
    let light = sun(toward, [units(3); 3]);
    let cosine = light.cosine(point(0, 1, 0));
    assert!((cosine.raw() - 3_037_000_500).abs() <= 1, "{cosine:?}");
    let value = frac(1, 2) * (units(3) * (cosine * FRAC_1_PI));
    // 0.5 * 3 * (1 / sqrt(2)) / pi = 0.337618613..., 1 450 060 9xx raw.
    assert!((value.raw() - 1_450_060_900).abs() < 2_000, "{value:?}");
    assert_ne!(value.raw(), LIT_GROUND_RAW);
    each_sample(&scene, (4, 4), settings(8, 1), |x, y, sample, radiance| {
        assert_eq!(radiance, [value; 3], "pixel ({x}, {y}) sample {sample}");
    });
}

/// An occluder 2 units above the ground covers it entirely from the camera's point of view at
/// 1 unit: every shadow ray is blocked (`D1 = 0`), every bounce either meets the occluder's
/// underside, which faces away from the sun (`D2 = 0`, so `B = 0`), or leaves past its edge
/// into the zero sky. So every sample is exactly zero, where the same ground without the
/// occluder is lit.
#[test]
fn an_occluded_triangle_has_no_direct_light() {
    let occluder = plate(units(2));
    let mut scene = lit_ground(&[occluder], point(0, 1, 0));
    // The camera between the ground and the occluder.
    scene = Scene::new(&SceneSpec {
        triangles: &[ground(), occluder],
        material_of: &[0, 0],
        materials: &[albedo([frac(1, 2); 3])],
        normals: &[],
        lighting: scene.lighting(),
        camera: looking_down(point(0, 1, 0), frac(1, 2)),
    })
    .expect("builds");
    for spp in [1, 16] {
        each_sample(&scene, (4, 4), settings(spp, 0), |x, y, sample, radiance| {
            assert_eq!(radiance, ZERO3, "pixel ({x}, {y}) sample {sample}");
        });
    }
}

/// A camera looking up from below the ground (which faces down there, its back face) sees
/// nothing at all: every sample is exactly the sky.
#[test]
fn a_pixel_that_sees_only_sky_is_the_sky() {
    let radiance = [frac(1, 4), frac(3, 8), raw(5 * ONE_RAW / 2)];
    let lighting = Lighting { sun: Some(sun(point(0, 1, 0), [units(3); 3])), sky: sky(radiance) };
    let camera = CameraPlacement {
        position: point(0, 1, 0),
        look: point(0, 1, 0),
        up: point(0, 0, -1),
        tan_half_fov: frac(1, 2),
    };
    let scene = build(&[ground()], &[albedo([FixedQ32_32::ONE; 3])], lighting, camera);
    each_sample(&scene, (3, 3), settings(4, 0), |_, _, _, value| assert_eq!(value, radiance));
    let image =
        render(&scene, ImageSize::new(3, 3).expect("size"), settings(4, 0)).expect("renders").image;
    let mut code = [0_u8; 4];
    write_rgba8(&mut code, TONE_CURVE, radiance).expect("four bytes");
    assert!(image.bytes().chunks_exact(4).all(|pixel| pixel == code));
}

/// A ground of albedo 1.0 under a uniform sky, with no sun: `D1 = 0`, every bounce leaves
/// upward into the sky, so `B = sky` and `L = 1.0 * (0 + sky) = sky` exactly (multiplying by
/// 1.0 is exact).
#[test]
fn a_white_surface_under_a_uniform_sky_returns_the_sky_exactly() {
    let radiance = [frac(1, 4), frac(1, 2), units(2)];
    let lighting = Lighting { sun: None, sky: sky(radiance) };
    let scene = build(
        &[ground()],
        &[albedo([FixedQ32_32::ONE; 3])],
        lighting,
        looking_down(point(0, 4, 0), frac(1, 2)),
    );
    let mut count = 0;
    each_sample(&scene, (4, 4), settings(8, 5), |x, y, sample, value| {
        assert_eq!(value, radiance, "pixel ({x}, {y}) sample {sample}");
        count += 1;
    });
    assert_eq!(count, 128);
    for spp in [1, 8, 64] {
        assert!(resolved(&scene, (4, 4), settings(spp, 2)).iter().all(|&r| r == radiance));
    }
}

/// A ceiling `2^-12` units above the ground, the camera between them at `2^-13`, looking
/// along the gap: every shadow ray meets the ceiling at a distance of about `2^-12` and every
/// bounce ray meets its unlit underside (or leaves past its edge into the zero sky), so every
/// sample is exactly zero. A minimum distance above that gap, as an epsilon would put on
/// secondary rays, lets the sun through.
#[test]
fn a_ceiling_closer_than_any_epsilon_still_casts_its_shadow() {
    let gap = raw(1 << 20);
    let camera = CameraPlacement {
        position: vec3(units(-8), raw(1 << 19), FixedQ32_32::ZERO),
        look: vec3(units(8), raw(-(1 << 19)), FixedQ32_32::ZERO),
        up: point(0, 1, 0),
        tan_half_fov: raw(1 << 14),
    };
    let lighting = Lighting { sun: Some(sun(point(0, 1, 0), [units(3); 3])), sky: sky(ZERO3) };
    let materials = [albedo([frac(1, 2); 3])];
    let with_ceiling =
        build(&[ground(), plate(gap)], &[materials[0], materials[0]], lighting, camera);
    let without = build(&[ground()], &materials, lighting, camera);
    let size = (4, 4);
    let cam = self::camera(&with_ceiling, size.0, size.1);
    for y in 0..size.1 {
        for x in 0..size.0 {
            let sampler = settings(4, 0).sampler(x, y);
            for sample in 0..4 {
                // Every camera ray reaches the ground, inside the gap, units away.
                let ray = cam.ray(sampler.camera_sample(sample)).expect("ray");
                let hit = brute_closest(&with_ceiling, &ray).expect("hits the ground");
                assert_eq!(hit.primitive, PrimitiveIndex::new(0));
                assert!(hit.hit.distance > units(1), "{hit:?}");
            }
        }
    }
    each_sample(&with_ceiling, size, settings(4, 0), |x, y, sample, value| {
        assert_eq!(value, ZERO3, "pixel ({x}, {y}) sample {sample}");
    });
    each_sample(&without, size, settings(4, 0), |x, y, sample, value| {
        assert_eq!(value, [raw(LIT_GROUND_RAW); 3], "pixel ({x}, {y}) sample {sample}");
    });
}

// ---------------------------------------------------------------------------------------------
// Shadows against exact geometry.

/// The shadow scene: the ground, and above it at `y = 1` an occluding triangle with albedo 0,
/// the sun straight down, sky zero, and the camera at `(0, 2, 0)` looking straight down with a
/// 90-degree field of view.
///
/// The occluder's vertices in `(x, z)` are `(1/2, -1)`, `(3/2, 0)` and `(1/2, 1)`. Seen from
/// the camera at twice its height, its silhouette on the ground is that triangle scaled by 2
/// about the origin, so the part of its shadow with `x < 1` is visible beside it.
const OCCLUDER_XZ: [(i64, i64); 3] =
    [(ONE_RAW / 2, -ONE_RAW), (3 * ONE_RAW / 2, 0), (ONE_RAW / 2, ONE_RAW)];

fn shadow_scene() -> Scene {
    let occluder = OCCLUDER_XZ.map(|(x, z)| vec3(raw(x), units(1), raw(z)));
    let lighting = Lighting { sun: Some(sun(point(0, 1, 0), [units(3); 3])), sky: sky(ZERO3) };
    let materials = [albedo([frac(1, 2); 3]), albedo(ZERO3)];
    build(
        &[ground(), occluder],
        &materials,
        lighting,
        looking_down(point(0, 2, 0), FixedQ32_32::ONE),
    )
}

/// Where a camera ray from `(0, h, 0)` with direction `d` (raw) crosses the plane `y = h - drop`:
/// the point `(d.x, d.z) * drop / -d.y`, kept exact as numerators and a positive denominator.
/// Returns the signed, scaled edge functions of that point against the triangle `tri`, all
/// positive (counter-clockwise in `(x, z)` seen from below) or all negative when it is inside.
fn edges(d: FixedVec3, drop: i128, tri: [(i64, i64); 3]) -> [i128; 3] {
    let denominator = -i128::from(d.y.raw());
    assert!(denominator > 0, "the ray goes down");
    // Point times the denominator, in raw units: (d.x * drop, d.z * drop), drop in raw units.
    let (px, pz) = (i128::from(d.x.raw()) * drop, i128::from(d.z.raw()) * drop);
    let corner = |i: usize| (i128::from(tri[i % 3].0), i128::from(tri[i % 3].1));
    [0, 1, 2].map(|i| {
        let ((ax, az), (bx, bz)) = (corner(i), corner(i + 1));
        (bx - ax) * (pz - az * denominator) - (bz - az) * (px - ax * denominator)
    })
}

fn inside(edges: [i128; 3]) -> bool {
    edges.iter().all(|&e| e > 0) || edges.iter().all(|&e| e < 0)
}

/// The set of ground pixels whose sample is in shadow equals the set an exact computation of the
/// occluder's vertical projection predicts, and the geometry keeps every sample well away from
/// the shadow's boundary.
#[test]
fn shadows_fall_exactly_inside_the_occluders_projection() {
    let scene = shadow_scene();
    let (width, height) = (32, 32);
    let cam = camera(&scene, width, height);
    let lit = raw(LIT_GROUND_RAW);
    let (mut shadowed, mut lit_ground, mut on_occluder) = (0, 0, 0);
    let mut closest_to_edge = i128::MAX;
    for y in 0..height {
        for x in 0..width {
            let sampler = settings(1, 9).sampler(x, y);
            let ray = cam.ray(sampler.camera_sample(0)).expect("ray");
            let d = ray.direction();
            assert!(-d.y.raw() >= 1 << 30, "{d:?}");
            // The ray meets the occluder's plane after dropping 1 unit and the ground after 2.
            let at_occluder = edges(d, i128::from(ONE_RAW), OCCLUDER_XZ);
            let at_ground = edges(d, 2 * i128::from(ONE_RAW), OCCLUDER_XZ);
            // An edge function is the distance from the edge (in raw units) times the edge's
            // length (at least 1 unit, 2^32 raw) times the denominator (`-d.y`, at least 2^30
            // raw here, since the direction's forward component is `2^-k` with `k <= 2`). So a
            // value above 2^72 is a distance above 2^-22 units, far beyond the few raw units
            // (about 2^-30 units) by which Bunny's start point and floored distances can move.
            for e in at_occluder.iter().chain(&at_ground) {
                closest_to_edge = closest_to_edge.min(e.abs());
            }
            let predicted_hit_occluder = inside(at_occluder);
            let predicted_shadow = inside(at_ground);
            let found = scene.closest(&ray).expect("traces").expect("hits");
            let value = trace_path(&scene, &scene, &cam, sampler, 0).expect("traces").radiance;
            if predicted_hit_occluder {
                assert_eq!(found.primitive, PrimitiveIndex::new(1), "pixel ({x}, {y})");
                on_occluder += 1;
                continue;
            }
            assert_eq!(found.primitive, PrimitiveIndex::new(0), "pixel ({x}, {y})");
            if predicted_shadow {
                assert_eq!(value, ZERO3, "pixel ({x}, {y}) should be in shadow");
                shadowed += 1;
            } else {
                assert_eq!(value, [lit; 3], "pixel ({x}, {y}) should be lit");
                lit_ground += 1;
            }
        }
    }
    assert!(closest_to_edge > 1 << 72, "a sample is too close to an edge: {closest_to_edge}");
    assert!(
        shadowed > 20 && lit_ground > 500 && on_occluder > 50,
        "{shadowed} {lit_ground} {on_occluder}"
    );
}

/// A pixel deep in the visible shadow under the zero sky is exactly zero at every sample count:
/// no light reaches it, directly or by the one bounce.
#[test]
fn a_pixel_in_full_shadow_is_zero_at_every_sample_count() {
    let scene = shadow_scene();
    let (width, height) = (32, 32);
    let cam = camera(&scene, width, height);
    // The ground point (0.75, 0) maps to column 16 + 0.75 * 8 = 22, row 16: the pixel
    // (22, 16) spans x in [0.75, 0.875] and z in [0, 0.125], inside the shadow and outside the
    // silhouette.
    for spp in [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024] {
        let (value, _) = pixel_radiance(&scene, &cam, settings(spp, 0), 22, 16).expect("renders");
        assert_eq!(value, ZERO3, "{spp} samples");
    }
}

// ---------------------------------------------------------------------------------------------
// Every traced ray against brute force; the composition against an independent one.

#[test]
fn every_traced_ray_equals_a_test_of_every_triangle() {
    let scene = reference_scene().expect("builds");
    let (width, height) = (32, 18);
    let cam = camera(&scene, width, height);
    let checked = Checked { scene: &scene, counts: Counts::default() };
    for y in 0..height {
        for x in 0..width {
            let sampler = settings(2, 0).sampler(x, y);
            for sample in 0..2 {
                let path = trace_path(&scene, &checked, &cam, sampler, sample).expect("traces");
                let direct = trace_path(&scene, &scene, &cam, sampler, sample).expect("traces");
                assert_eq!(path, direct);
            }
        }
    }
    let c = &checked.counts;
    let (hits, misses, occluded, visible) =
        (c.closest_hits.get(), c.closest_misses.get(), c.occluded.get(), c.visible.get());
    let bunny = c.bunny_hits.get();
    // The comparison means something only if every kind of answer occurred, including hits on
    // the bunny's triangles.
    assert!(
        hits > 800 && misses > 50 && occluded > 30 && visible > 500 && bunny > 20,
        "{hits} {misses} {occluded} {visible} {bunny}"
    );
    assert_eq!(c.shadow_limits_wrong.get(), 0, "every shadow ray uses SHADOW_T_MAX");
}

#[test]
fn every_sample_equals_an_independent_composition_of_the_documented_order() {
    let scene = reference_scene().expect("builds");
    let (width, height) = (16, 9);
    let cam = camera(&scene, width, height);
    let mut kinds = [0_u32; 3];
    for (seed, spp) in [(0, 2), (77, 1)] {
        let settings = settings(spp, seed);
        each_sample(&scene, (width, height), settings, |x, y, sample, value| {
            let want = reference_path(&scene, &cam, settings.sampler(x, y), sample);
            assert_eq!(value, want, "seed {seed}, pixel ({x}, {y}) sample {sample}");
            kinds[usize::from(value == scene.lighting().sky.radiance())] += 1;
            kinds[2] += 1;
        });
    }
    assert!(kinds[0] > 100 && kinds[1] > 10, "{kinds:?}");
}

/// The camera ray of a sample is `camera.ray(sampler.camera_sample(sample))`, pair 0, and its
/// bounce uses pair 1: checked by tracing the expected rays and comparing what they hit with
/// what the path's first two closest-hit queries were asked.
#[test]
fn the_camera_uses_pair_zero_and_the_bounce_pair_one() {
    struct Recorder<'a> {
        scene: &'a Scene,
        rays: std::cell::RefCell<Vec<TraceRay>>,
    }
    impl Tracer for Recorder<'_> {
        fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
            self.rays.borrow_mut().push(*ray);
            self.scene.closest(ray)
        }
        fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError> {
            self.scene.occluded(ray, t_max)
        }
    }
    let scene = reference_scene().expect("builds");
    let cam = camera(&scene, 16, 9);
    let mut bounces = 0;
    for (x, y) in [(8, 7), (3, 6), (12, 8)] {
        let sampler = settings(4, 11).sampler(x, y);
        for sample in 0..4 {
            let recorder = Recorder { scene: &scene, rays: std::cell::RefCell::new(Vec::new()) };
            trace_path(&scene, &recorder, &cam, sampler, sample).expect("traces");
            let asked = recorder.rays.into_inner();
            let ray0 = cam.ray(sampler.camera_sample(sample)).expect("ray");
            assert_eq!(asked[0], ray0);
            let [u, v] = sampler.point(sample, 0);
            assert_eq!(cam.ray(PixelSample::new(x, y, u, v)), Some(ray0));
            if let Some(hit) = scene.closest(&ray0).expect("traces") {
                let h1 = SurfaceHit::new(
                    scene.surface(hit.primitive).expect("surface"),
                    &ray0,
                    &hit.hit,
                )
                .expect("start");
                assert_eq!(asked[1], h1.bounce_ray(sampler.point(sample, 1)).expect("bounce"));
                // Secondary rays start at Bunny's start point, not at the raw hit point.
                assert_eq!(asked[1].origin(), h1.point());
                bounces += 1;
            }
        }
    }
    assert!(bounces > 4);
}

/// The traversal lumen chose (Bunny's SAH tree in the compact layout) returns, for every camera
/// ray of a small image, the same answer as `closest_hit` and `any_hit` on a plain `build_bvh`
/// tree. Bunny guarantees this; the test shows lumen wired it correctly.
#[test]
fn the_chosen_traversal_matches_closest_hit_on_the_plain_tree() {
    let scene = reference_scene().expect("builds");
    let boxes: Vec<FixedAabb3> = scene.triangles().iter().map(TraceTriangle::bounds).collect();
    let zero = point(0, 0, 0);
    let empty =
        BvhNode { bounds: FixedAabb3::new(zero, zero), first_child_or_prim_idx: 0, prim_count: 0 };
    let mut nodes = vec![empty; 2 * boxes.len()];
    let mut prims = vec![0_u32; boxes.len()];
    let used = build_bvh(&mut nodes, &mut prims, &boxes).expect("builds");
    let view = BvhView::new(&nodes[..used], &prims);
    let triangles = scene.triangles();
    let (width, height) = (48, 27);
    let cam = camera(&scene, width, height);
    let mut hits = 0;
    for y in 0..height {
        for x in 0..width {
            let sampler = settings(2, 0).sampler(x, y);
            for sample in 0..2 {
                let ray = cam.ray(sampler.camera_sample(sample)).expect("ray");
                let plain = closest_hit(view, &ray, FAR, |p, limit| {
                    ray.hit_triangle(&triangles[p as usize], FixedQ32_32::ZERO, limit)
                        .map(|h| h.distance)
                })
                .expect("traverses");
                let chosen = scene.closest(&ray).expect("traces");
                assert_eq!(
                    chosen.map(|h| (h.primitive, h.hit.distance)),
                    plain.map(|h| (PrimitiveIndex::new(h.primitive), h.distance)),
                    "pixel ({x}, {y}) sample {sample}"
                );
                hits += usize::from(chosen.is_some());
                let any = any_hit(view, &ray, FAR, |p| {
                    ray.hit_triangle(&triangles[p as usize], FixedQ32_32::ZERO, FAR).is_some()
                })
                .expect("traverses");
                assert_eq!(scene.occluded(&ray, FAR).expect("traces"), any);
            }
        }
    }
    assert!(hits > 1000 && hits < 48 * 27 * 2, "{hits}");
}

// ---------------------------------------------------------------------------------------------
// Order independence.

#[test]
fn a_pixel_does_not_depend_on_the_order_of_its_samples() {
    let scene = reference_scene().expect("builds");
    let cam = camera(&scene, 32, 18);
    let settings = settings(32, 4);
    for (x, y) in [(16, 12), (9, 13), (25, 10), (2, 2)] {
        let sampler = settings.sampler(x, y);
        let forwards: Vec<[FixedQ32_32; 3]> = (0..32)
            .map(|i| trace_path(&scene, &scene, &cam, sampler, i).expect("traces").radiance)
            .collect();
        // Evaluating the samples in another order gives the same values.
        for i in (0..32).rev() {
            let again = trace_path(&scene, &scene, &cam, sampler, i).expect("traces").radiance;
            assert_eq!(again, forwards[i as usize]);
        }
        let accumulate = |order: &[usize]| {
            let mut acc = PixelAccumulator::new();
            for &i in order {
                acc.add_sample(forwards[i]).expect("adds");
            }
            acc
        };
        let identity: Vec<usize> = (0..32).collect();
        let backwards: Vec<usize> = (0..32).rev().collect();
        let shuffled: Vec<usize> = (0..32).map(|i| (i * 13 + 5) % 32).collect();
        let reference = accumulate(&identity);
        for order in [&backwards, &shuffled] {
            let other = accumulate(order);
            assert_eq!(other.sums(), reference.sums());
            assert_eq!(other.resolve(), reference.resolve());
        }
        // Four partial accumulators of interleaved samples, merged in reverse.
        let mut parts: Vec<PixelAccumulator> = (0..4)
            .map(|part| {
                accumulate(&identity.iter().copied().filter(|i| i % 4 == part).collect::<Vec<_>>())
            })
            .collect();
        parts.reverse();
        let mut merged = PixelAccumulator::new();
        for part in &parts {
            merged.merge(part).expect("merges");
        }
        assert_eq!(merged.sums(), reference.sums());
        assert_eq!(merged.count(), 32);
        let resolved = pixel_radiance(&scene, &cam, settings, x, y).expect("renders").0;
        assert_eq!(Ok(resolved), reference.resolve());
        assert_eq!(merged.resolve(), reference.resolve());
    }
}

#[test]
fn an_image_does_not_depend_on_the_order_of_its_pixels() {
    let scene = reference_scene().expect("builds");
    let (width, height) = (40, 22);
    let settings = settings(4, 2);
    let rendered = render(&scene, ImageSize::new(width, height).expect("size"), settings)
        .expect("renders")
        .image;
    let cam = camera(&scene, width, height);
    let mut bytes = vec![0_u8; (width * height * 4) as usize];
    // Columns right to left, rows bottom to top, in a strided order.
    let count = width * height;
    for k in 0..count {
        let index = (k * 7 + 3) % count; // 7 is coprime with 880
        let (x, y) = (width - 1 - index % width, height - 1 - index / width);
        let (radiance, _) = pixel_radiance(&scene, &cam, settings, x, y).expect("renders");
        let at = ((y * width + x) * 4) as usize;
        write_rgba8(&mut bytes[at..at + 4], TONE_CURVE, radiance).expect("four bytes");
    }
    let shuffled =
        Rgba8Image::from_bytes(ImageSize::new(width, height).expect("size"), bytes).expect("image");
    assert_eq!(shuffled.hash(), rendered.hash());
}

// ---------------------------------------------------------------------------------------------
// Convergence sanity: a regression guard with a stated threshold, not a proof.

/// For six pixels of the reference scene at 32 by 18 (a lit part of the sphere, the ground in
/// the sphere's shadow, which only the bounce and the sky light, the wall, the octahedron, the
/// bunny and the ground in the bunny's shadow), the mean over 1 024 samples differs from the
/// mean over 256 by less than 0.0195 per channel (5 / 256, about 2 % of a fully lit white
/// surface). The largest difference measured when this was written was 0.0116.
#[test]
fn means_over_256_and_1024_samples_agree() {
    let scene = reference_scene().expect("builds");
    let cam = camera(&scene, 32, 18);
    let sun = scene.lighting().sun.expect("a sun");
    let threshold = frac(5, 256).raw();
    // (pixel, the primitives it may see, whether its first sample is in shadow)
    let cases: [((u32, u32), core::ops::RangeInclusive<u32>, bool); 6] = [
        ((17, 10), 12..=27_659, false),
        ((10, 9), 0..=1, true),
        ((12, 4), 2..=3, false),
        ((24, 9), 4..=11, false),
        ((8, 12), 27_660..=31_510, false),
        ((6, 12), 0..=1, true),
    ];
    for ((x, y), primitives, in_shadow) in cases {
        let ray = cam.ray(settings(1, 0).sampler(x, y).camera_sample(0)).expect("ray");
        let hit = scene.closest(&ray).expect("traces").expect("sees a surface");
        assert!(primitives.contains(&hit.primitive.get()), "pixel ({x}, {y}): {hit:?}");
        let h = SurfaceHit::new(scene.surface(hit.primitive).expect("surface"), &ray, &hit.hit)
            .expect("start point");
        let shadow = h.light_sample(&sun);
        let blocked =
            shadow.shadow_ray().is_none_or(|r| scene.occluded(r, SHADOW_T_MAX).expect("traces"));
        assert_eq!(blocked, in_shadow, "pixel ({x}, {y})");
        let (m256, _) = pixel_radiance(&scene, &cam, settings(256, 0), x, y).expect("renders");
        let (m1024, _) = pixel_radiance(&scene, &cam, settings(1024, 0), x, y).expect("renders");
        for c in 0..3 {
            let diff = (m1024[c].raw() - m256[c].raw()).abs();
            assert!(diff < threshold, "pixel ({x}, {y}) channel {c}: {m256:?} vs {m1024:?}");
        }
        // A shadowed pixel still receives light: the bounce and the sky.
        assert!(m1024.iter().all(|c| c.raw() > 0), "pixel ({x}, {y}): {m1024:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// The bunny in the reference scene.

/// The camera ray through the centre of a pixel of a 32 by 18 image.
fn centre_ray(scene: &Scene, x: u32, y: u32) -> TraceRay {
    camera(scene, 32, 18).ray(PixelSample::centre(x, y)).expect("ray")
}

/// The bunny stands left of the sphere, low in the picture: at 32 by 18 the centre of pixel
/// `(8, 12)`, its neighbour right, the pixel below and `(9, 13)` see bunny triangles. Facing the
/// camera, the bunny is about two pixels wide there: the neighbour left sees the ground, in
/// shadow. (A planning projection of the body's centre, about `(-2.24, -0.65, 0.51)`, lands near
/// `(8.4, 12.9)` in that image.) Pixels three to the left and three below see the ground, and
/// the sphere's centre pixel sees the sphere.
#[test]
fn the_camera_sees_the_bunny_where_it_was_placed() {
    let scene = reference_scene().expect("builds");
    for (x, y) in [(8, 12), (9, 12), (8, 13), (9, 13)] {
        let hit = scene.closest(&centre_ray(&scene, x, y)).expect("traces").expect("a hit");
        assert!(is_bunny(hit.primitive), "pixel ({x}, {y}) sees {}", hit.primitive);
    }
    for (x, y) in [(5, 12), (8, 15)] {
        let hit = scene.closest(&centre_ray(&scene, x, y)).expect("traces").expect("a hit");
        assert!(hit.primitive <= PrimitiveIndex::new(1), "pixel ({x}, {y}) sees {}", hit.primitive);
    }
    let hit = scene.closest(&centre_ray(&scene, 15, 9)).expect("traces").expect("a hit");
    assert!(
        (12..27_660).contains(&hit.primitive.get()),
        "the sphere's pixel sees {}",
        hit.primitive
    );
}

/// Whether the reference sun reaches the ground at `(x, z)`, and the primitive its shadow ray
/// meets first, if any. The ground point is reached by a ray straight down from one unit above
/// it, so the shadow ray starts at Bunny's start point as in a render.
fn sun_at_ground(scene: &Scene, x: FixedQ32_32, z: FixedQ32_32) -> (bool, Option<PrimitiveIndex>) {
    let ground_y = raw(-9 * (ONE_RAW / 8));
    let down = TraceRay::new(vec3(x, ground_y + units(1), z), point(0, -1, 0)).expect("ray");
    let hit = scene.closest(&down).expect("traces").expect("the ground");
    assert!(
        hit.primitive <= PrimitiveIndex::new(1),
        "the downward ray at ({x:?}, {z:?}) met {}",
        hit.primitive
    );
    let h = SurfaceHit::new(scene.surface(hit.primitive).expect("surface"), &down, &hit.hit)
        .expect("start point");
    // Bunny's start point is on the ground or up to two raw units above it.
    let above = h.point().y.raw() - ground_y.raw();
    assert!((0..=2).contains(&above), "starts on the ground: {above}");
    let sun = scene.lighting().sun.expect("a sun");
    let shadow = h.light_sample(&sun).shadow_ray().copied().expect("the sun is above the ground");
    let blocked = scene.occluded(&shadow, SHADOW_T_MAX).expect("traces");
    let first = scene.closest(&shadow).expect("traces").map(|h| h.primitive);
    (!blocked, first)
}

/// The bunny casts its shadow where the light direction puts it.
///
/// The point `P = (-2.25, -0.5, 0.5)` is inside the bunny's body: in the file's coordinates it is
/// about `(-0.0151, 0.1115, -0.0027)`, and an exact count of crossings with the placed mesh along
/// ten directions from it (the six axes, the sun's direction both ways, `(1, 1, 1)` and
/// `(-1, 1, -1)`) is one every time, with no edge or vertex hit. The sun is toward
/// `d = (0.5, 0.3125, 1.0)`, so the ground point `G = P - 2 d = (-3.25, -1.125, -1.5)`
/// (`-0.5 - 2 * 0.3125 = -1.125`, the ground's height) sees the sun only through `P`, and the
/// bunny's surface lies between them: `G` is in the bunny's shadow, and the first triangle its
/// shadow ray meets is a bunny triangle.
///
/// The ground point `(-3.25, -1.125, 1.0)`, 2.5 units nearer the camera, is lit: its shadow ray
/// goes toward `+Z` from `z = 1.0`, and the only objects with any point at `z >= 1.0` are the
/// sphere (`z <= 1.04`, `x >= -1.04`, which the ray reaches at `x <= -3.23`), the bunny
/// (`z <= 1.134`, `x >= -2.722`, reached at `x <= -3.183`) and the octahedron (`x >= 1.25`), so
/// it meets nothing.
#[test]
fn the_bunny_shadows_the_ground_behind_it_and_not_beside_it() {
    let scene = reference_scene().expect("builds");
    let (lit, first) = sun_at_ground(&scene, frac(-13, 4), frac(-3, 2));
    assert!(!lit, "the ground behind the bunny is in its shadow");
    assert!(first.is_some_and(is_bunny), "the shadow ray first meets {first:?}");
    let (lit, first) = sun_at_ground(&scene, frac(-13, 4), units(1));
    assert!(lit && first.is_none(), "the ground in front is lit: {first:?}");
}

// ---------------------------------------------------------------------------------------------
// Determinism and the golden hash.

#[test]
fn rendering_twice_in_one_process_gives_the_same_bytes() {
    let first = render_reference(48, 27, settings(2, 0)).expect("renders");
    let second = render_reference(48, 27, settings(2, 0)).expect("renders");
    assert_eq!(first, second);
    // Another seed, sample count or size is another image.
    let seeded = render_reference(48, 27, settings(2, 1)).expect("renders");
    assert_ne!(first.image.hash(), seeded.image.hash());
    let more = render_reference(48, 27, settings(4, 0)).expect("renders");
    assert_ne!(first.image.hash(), more.image.hash());
    let other = render_reference(27, 48, settings(2, 0)).expect("renders");
    assert_ne!(first.image.hash(), other.image.hash());
}

#[test]
fn the_reference_image_has_its_golden_hash() {
    assert_eq!(lumen::REFERENCE_SCENE_ID, "lumen-reference-scene/4");
    assert_eq!(lumen::INTEGRATOR_ID, "lumen-diffuse-path/1");
    assert_eq!(bunny_optics::SAMPLER_ID, "bunny-sobol-owen/1");
    assert_eq!(bunny_optics::SHADING_ID, "bunny-diffuse-shading/1");
    assert_eq!(TONE_CURVE.identity(), "bunny-tone-srgb/1");
    assert_eq!(bunny_optics::RGBA8_OPAQUE_IDENTITY, "bunny-rgba8-opaque/1");
    assert_eq!(BUNNY_REV, GOLDEN_BUNNY_REV);
    let render = render_reference(96, 54, settings(4, 0)).expect("renders");
    assert_eq!(render.image.hash(), GOLDEN_96X54_4SPP);
}

/// Bunny's camera refuses no reference size from 1 by 1 to the largest axis.
#[test]
fn extreme_sizes_have_a_camera() {
    let placement = reference_camera();
    for (width, height) in [(1, 1), (1, 32_768), (32_768, 1), (16_384, 8_192)] {
        assert!(placement.camera(width, height).is_ok(), "{width}x{height}");
    }
}

#[test]
fn the_lockfile_resolves_bunny_to_the_recorded_commit() {
    let lock = include_str!("../../../Cargo.lock");
    let pinned = format!("?rev={BUNNY_REV}#{BUNNY_REV}\"");
    let bunny_sources: Vec<&str> =
        lock.lines().filter(|line| line.contains("flyingrobots/bunny")).collect();
    assert_eq!(bunny_sources.len(), 8, "eight Bunny crates: {bunny_sources:?}");
    assert!(bunny_sources.iter().all(|line| line.ends_with(&pinned)), "{bunny_sources:?}");
    // Lumen's only dependencies are Bunny's: no crate comes from a registry.
    let registry: Vec<&str> = lock.lines().filter(|line| line.contains("registry+")).collect();
    assert!(registry.is_empty(), "registry crates in the lock: {registry:?}");
}
