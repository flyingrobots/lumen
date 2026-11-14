//! Size class: small (in process, no file, no network).
//!
//! `lumen-smooth-diffuse-path/1` through the public API: which integrator a scene gets, the
//! bytes of a scene whose normals change nothing, hand-derived radiance at smooth hits, Bunny's
//! rejected bounce, and every path of a mixed scene against a test-side composition of Bunny's
//! documented steps over brute-force ray queries.

use std::cell::Cell;

use bunny_linalg::{FixedUnitVec3, FixedVec3};
use bunny_num::FixedQ32_32;
use bunny_optics::{
    Albedo, DirectionalLight, SkyLight, SmoothBounce, SmoothHit, SurfaceHit, VertexNormals,
    SHADOW_T_MAX,
};
use bunny_query::TraceRay;
use lumen::reference::{
    reference_camera, reference_lighting, reference_material_of, reference_materials,
    reference_triangles,
};
use lumen::{
    render, trace_path, CameraPlacement, Hit, ImageSize, Integrator, Lighting, PathSample,
    PrimitiveIndex, RenderProvenance, RenderSettings, Scene, SceneSpec, TraceError, Tracer, FAR,
};

type Normals = [Option<FixedUnitVec3>; 3];

const ONE: i64 = 1 << 32;

fn raw(n: i64) -> FixedQ32_32 {
    FixedQ32_32::from_raw(n)
}

/// A point in sixteenths of a unit.
fn p(x: i64, y: i64, z: i64) -> FixedVec3 {
    FixedVec3::new(raw(x * ONE / 16), raw(y * ONE / 16), raw(z * ONE / 16))
}

/// A camera 4 units above the origin looking straight down.
fn overhead() -> CameraPlacement {
    CameraPlacement {
        position: p(0, 64, 0),
        look: p(0, -16, 0),
        up: p(0, 0, -16),
        tan_half_fov: raw(ONE / 2),
    }
}

/// One albedo of `half` in every channel: 0.5.
fn half() -> Albedo {
    Albedo::new([raw(ONE / 2); 3]).expect("valid albedo")
}

fn build(triangles: &[[FixedVec3; 3]], normals: &[Normals], lighting: Lighting) -> Scene {
    Scene::new(&SceneSpec {
        triangles,
        material_of: &vec![0; triangles.len()],
        materials: &[half()],
        normals,
        lighting,
        camera: overhead(),
    })
    .expect("the scene builds")
}

/// Every sample of a `width` by `height` image at `samples` per pixel, traced by `tracer`.
fn paths<T: Tracer>(scene: &Scene, tracer: &T, size: (u32, u32), samples: u32) -> Vec<PathSample> {
    let camera = scene.camera().camera(size.0, size.1).expect("camera");
    let settings = RenderSettings::new(samples, 5).expect("settings");
    let mut out = Vec::new();
    for y in 0..size.1 {
        for x in 0..size.0 {
            for sample in 0..samples {
                let path = trace_path(scene, tracer, &camera, settings.sampler(x, y), sample);
                out.push(path.expect("traces"));
            }
        }
    }
    out
}

/// A square pyramid of half-width 1 and height 1/2 standing on the origin: four faces sloping
/// `atan(1/2)`, about 26.6 degrees, from the horizontal.
fn pyramid() -> Vec<[FixedVec3; 3]> {
    let apex = p(0, 8, 0);
    let corners = [p(-16, 0, -16), p(16, 0, -16), p(16, 0, 16), p(-16, 0, 16)];
    (0..4).map(|i| [apex, corners[(i + 1) % 4], corners[i]]).collect()
}

/// A sun straight above at irradiance 1.0, and a black sky.
fn noon() -> Lighting {
    let sun = DirectionalLight::new(p(0, 16, 0), [raw(ONE); 3]).expect("valid sun");
    Lighting { sun: Some(sun), sky: SkyLight::new([FixedQ32_32::ZERO; 3]).expect("valid sky") }
}

#[test]
fn a_scene_gets_the_smooth_integrator_only_when_a_triangle_has_a_vertex_normal() {
    let triangles = pyramid();
    let size = ImageSize::new(4, 4).expect("size");
    let settings = RenderSettings::new(1, 0).expect("settings");
    for (normals, smooth, integrator) in [
        (Vec::new(), 0, "lumen-diffuse-path/1"),
        (vec![[None; 3]; 4], 0, "lumen-diffuse-path/1"),
        (
            vec![[None, Some(FixedUnitVec3::UNIT_Y), None], [None; 3], [None; 3], [None; 3]],
            1,
            "lumen-smooth-diffuse-path/1",
        ),
    ] {
        let scene = build(&triangles, &normals, noon());
        assert_eq!(scene.smooth_triangles(), smooth);
        assert_eq!(Integrator::for_scene(&scene).id(), integrator);
        let fields = RenderProvenance::new(&scene, size, settings).fields();
        assert_eq!(fields[1], ("integrator", lumen::ProvenanceValue::Identity(integrator)));
    }
    assert_eq!(lumen::SMOOTH_INTEGRATOR_ID, "lumen-smooth-diffuse-path/1");
    assert_eq!(bunny_optics::SMOOTH_SHADING_ID, "bunny-smooth-shading/1");
    assert_eq!(bunny_optics::SMOOTH_NORMAL_ID, "bunny-smooth-normal/1");
}

/// The reference scene (31 511 triangles), given each triangle's own unit face normal at all
/// three vertices, renders the same bytes as with no normals, though every hit goes through
/// `SmoothHit`. Bunny's shared-normal rule returns an equal normal unchanged, its frame is the
/// face's, and a cosine-weighted direction around the face normal is never below the face.
/// The image without normals is the reference scene itself, whose golden hash pins it.
#[test]
fn face_normals_at_every_vertex_render_the_flat_bytes() {
    let triangles = reference_triangles().expect("the reference triangles");
    let flat = lumen::reference_scene().expect("builds");
    let normals: Vec<Normals> = (0..triangles.len())
        .map(|i| {
            let primitive = PrimitiveIndex::new(u32::try_from(i).expect("fits"));
            let surface = flat.surface(primitive).expect("no zero-area triangle");
            let unit =
                FixedUnitVec3::try_from_unit(surface.frame().normal()).expect("a unit normal");
            [Some(unit); 3]
        })
        .collect();
    let smooth = Scene::new(&SceneSpec {
        triangles: &triangles,
        material_of: &reference_material_of(),
        materials: &reference_materials().expect("materials"),
        normals: &normals,
        lighting: reference_lighting().expect("lighting"),
        camera: reference_camera(),
    })
    .expect("builds");
    assert_eq!(smooth.smooth_triangles(), triangles.len());
    assert_ne!(smooth.content_id(), flat.content_id());
    let size = ImageSize::new(64, 36).expect("size");
    let settings = RenderSettings::new(4, 0).expect("settings");
    let (a, b) = (render(&flat, size, settings), render(&smooth, size, settings));
    let (a, b) = (a.expect("renders"), b.expect("renders"));
    assert_eq!(a.image.hash(), b.image.hash());
    assert_eq!(a.rays, b.rays);
}

/// Oracle: hand-derived. Every vertex normal is `+Y` and the sun is straight above at 1.0, so
/// at every hit Bunny's shared-normal rule gives exactly `+Y`, the cosine is exactly 1.0, the
/// light's term is `FRAC_1_PI` (raw 1 367 130 551), and an albedo of 0.5 reflects half of it,
/// 683 565 275.5, rounded to the even 683 565 276. The sky is black and the pyramid convex, so
/// the bounce adds nothing. Shaded flat, the faces lean `atan(1/2)` away from the light and
/// reflect less (a cosine of about 0.894).
#[test]
fn a_smooth_pyramid_lit_from_above_reflects_the_pole_value_everywhere() {
    let triangles = pyramid();
    let up = [Some(FixedUnitVec3::UNIT_Y); 3];
    let smooth = build(&triangles, &[up; 4], noon());
    let flat = build(&triangles, &[], noon());
    let (smooth, flat) = (paths(&smooth, &smooth, (16, 16), 2), paths(&flat, &flat, (16, 16), 2));
    let mut hits = 0;
    for (s, f) in smooth.iter().zip(&flat) {
        assert_eq!(s.rays.shadow, f.rays.shadow);
        if s.rays.shadow == 0 {
            assert_eq!(s.radiance, [FixedQ32_32::ZERO; 3], "a miss sees the black sky");
            continue;
        }
        hits += 1;
        assert_eq!(s.radiance, [raw(683_565_276); 3]);
        assert!(f.radiance[0] < s.radiance[0], "flat {:?}", f.radiance);
    }
    // The pyramid fills about a quarter of the view: 128 of 512 samples.
    assert!(hits > 80, "{hits} samples hit the pyramid");
}

/// Oracle: Bunny's rule for a smooth bounce. A floor with every vertex normal 53 degrees from
/// its geometric normal, no sun and a sky of 1.0: a bounce Bunny accepts sees the sky, so the
/// sample is 0.5 (raw 2^31) with two closest-hit queries; a rejected bounce traces no ray and
/// adds zero, so the sample is 0 with one query. Both happen.
#[test]
fn a_rejected_bounce_traces_no_ray_and_adds_zero() {
    // Large enough to fill the view (2 units either side at 4 units' distance): every camera
    // ray hits it, so a single query means a rejected bounce.
    let floor = vec![[p(-256, 0, -256), p(256, 0, -256), p(0, 0, 256)]];
    let tilted = FixedUnitVec3::new(p(64, 48, 0)).expect("a unit vector");
    let sky = Lighting { sun: None, sky: SkyLight::new([raw(ONE); 3]).expect("valid sky") };
    let scene = build(&floor, &[[Some(tilted); 3]], sky);
    let (mut accepted, mut rejected) = (0, 0);
    for path in paths(&scene, &scene, (8, 8), 8) {
        match path.rays.closest {
            2 => {
                accepted += 1;
                assert_eq!(path.radiance, [raw(ONE / 2); 3]);
            }
            1 => {
                rejected += 1;
                assert_eq!(path.radiance, [FixedQ32_32::ZERO; 3]);
            }
            n => panic!("{n} closest-hit queries"),
        }
        assert_eq!(path.rays.shadow, 0);
    }
    assert!(accepted > 50 && rejected > 50, "{accepted} accepted, {rejected} rejected");
}

/// A test of every triangle, counting queries.
struct Brute<'a> {
    scene: &'a Scene,
    queries: Cell<u32>,
}

impl Tracer for Brute<'_> {
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
        self.queries.set(self.queries.get() + 1);
        let mut best: Option<Hit> = None;
        for (index, triangle) in self.scene.triangles().iter().enumerate() {
            if let Some(hit) = ray.hit_triangle(triangle, FixedQ32_32::ZERO, FAR) {
                if best.is_none_or(|b| hit.distance < b.hit.distance) {
                    let primitive = PrimitiveIndex::new(u32::try_from(index).expect("fits"));
                    best = Some(Hit { primitive, hit });
                }
            }
        }
        assert_eq!(self.scene.closest(ray)?, best, "the tree's closest hit of {ray:?}");
        Ok(best)
    }

    fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError> {
        let hit = |t| ray.hit_triangle(t, FixedQ32_32::ZERO, t_max).is_some();
        let brute = self.scene.triangles().iter().any(hit);
        assert_eq!(self.scene.occluded(ray, t_max)?, brute, "the tree's any hit of {ray:?}");
        Ok(brute)
    }
}

/// One hit shaded as Bunny's documentation orders it, written out in the test: the light sample
/// and the bounce of `SmoothHit` for a triangle with a vertex normal, of `SurfaceHit` otherwise.
enum Shaded {
    Flat(SurfaceHit),
    Smooth(SmoothHit),
}

fn shade(scene: &Scene, ray: &TraceRay, hit: &Hit) -> Shaded {
    let surface = scene.surface(hit.primitive).expect("a surface");
    let at = SurfaceHit::new(surface, ray, &hit.hit).expect("a surface hit");
    match scene.vertex_normals(hit.primitive) {
        Some(n) if n.iter().any(Option::is_some) => {
            Shaded::Smooth(SmoothHit::new(at, hit.hit.barycentric_numerators(), *n))
        }
        _ => Shaded::Flat(at),
    }
}

fn direct(scene: &Scene, tracer: &Brute<'_>, shaded: &Shaded) -> [FixedQ32_32; 3] {
    let sun = scene.lighting().sun.expect("a sun");
    let light = match shaded {
        Shaded::Flat(h) => h.light_sample(&sun),
        Shaded::Smooth(h) => h.light_sample(&sun),
    };
    let visible = light.shadow_ray().is_some_and(|r| !tracer.occluded(r, SHADOW_T_MAX).expect("q"));
    light.radiance(visible)
}

/// Bunny's order of operations with smooth shading, for one sample, over brute-force queries.
fn reference_path(
    scene: &Scene,
    tracer: &Brute<'_>,
    ray0: &TraceRay,
    bounce: [FixedQ32_32; 2],
) -> [FixedQ32_32; 3] {
    let albedo = |hit: &Hit| *scene.albedo(hit.primitive).expect("an albedo");
    let Some(hit1) = tracer.closest(ray0).expect("q") else {
        return scene.lighting().sky.radiance();
    };
    let h1 = shade(scene, ray0, &hit1);
    let d1 = direct(scene, tracer, &h1);
    let ray1 = match &h1 {
        Shaded::Flat(h) => Some(h.bounce_ray(bounce).expect("a bounce")),
        Shaded::Smooth(h) => match h.bounce(bounce).expect("a bounce") {
            SmoothBounce::Ray(ray) => Some(ray),
            SmoothBounce::BelowSurface => None,
        },
    };
    let b = match ray1 {
        None => [FixedQ32_32::ZERO; 3],
        Some(ray1) => match tracer.closest(&ray1).expect("q") {
            None => scene.lighting().sky.radiance(),
            Some(hit2) => {
                let d2 = direct(scene, tracer, &shade(scene, &ray1, &hit2));
                albedo(&hit2).reflect(d2, [FixedQ32_32::ZERO; 3]).expect("in range")
            }
        },
    };
    albedo(&hit1).reflect(d1, b).expect("in range")
}

/// Oracle: Bunny's documented order of operations (`bunny-smooth-shading/1` at smooth hits,
/// `bunny-diffuse-shading/1` elsewhere), composed in this test over a test of every triangle,
/// for every sample of a scene that mixes a smooth octahedron (Bunny's area-weighted vertex
/// normals, so they differ from vertex to vertex) and a flat floor under it, with the
/// reference sun and sky. Every query of the scene's own tree is checked against brute force.
#[test]
fn every_path_of_a_mixed_scene_follows_bunnys_steps() {
    let positions =
        [p(0, 40, 0), p(16, 24, 0), p(0, 24, 16), p(-16, 24, 0), p(0, 24, -16), p(0, 8, 0)];
    let mut faces: Vec<[u32; 3]> = Vec::new();
    for i in 0..4_u32 {
        let (a, b) = (1 + i, 1 + (i + 1) % 4);
        faces.push([0, b, a]);
        faces.push([5, a, b]);
    }
    let vertex_normals = VertexNormals::new(&positions, &faces).expect("normals");
    let mut triangles: Vec<[FixedVec3; 3]> =
        faces.iter().map(|f| f.map(|i| positions[usize::try_from(i).expect("fits")])).collect();
    let mut normals: Vec<Normals> = faces
        .iter()
        .map(|f| f.map(|i| vertex_normals.normals()[usize::try_from(i).expect("fits")]))
        .collect();
    triangles.push([p(-64, 0, -64), p(0, 0, 64), p(64, 0, -64)]);
    normals.push([None; 3]);
    let lighting = reference_lighting().expect("lighting");
    let scene = build(&triangles, &normals, lighting);
    assert_eq!(scene.smooth_triangles(), 8);
    let tracer = Brute { scene: &scene, queries: Cell::new(0) };
    let (width, height, count) = (12, 12, 4);
    let camera = scene.camera().camera(width, height).expect("camera");
    let settings = RenderSettings::new(count, 9).expect("settings");
    let mut smooth_hits = 0;
    for y in 0..height {
        for x in 0..width {
            let sampler = settings.sampler(x, y);
            for sample in 0..count {
                let path = trace_path(&scene, &tracer, &camera, sampler, sample).expect("traces");
                let ray0 = camera.ray(sampler.camera_sample(sample)).expect("a ray");
                let expected = reference_path(
                    &scene,
                    &tracer,
                    &ray0,
                    sampler.point(sample, lumen::BOUNCE_PAIR),
                );
                assert_eq!(path.radiance, expected, "pixel ({x}, {y}) sample {sample}");
                let first = scene.closest(&ray0).expect("q").map(|h| h.primitive.to_usize());
                smooth_hits += u32::from(first.is_some_and(|i| i < 8));
            }
        }
    }
    assert!(smooth_hits > 50 && tracer.queries.get() > 1000, "{smooth_hits} smooth first hits");
}
