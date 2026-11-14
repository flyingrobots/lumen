//! Size class: small (in process, no file, no network).
//!
//! Every error variant lumen reports, produced through the public API where an input reaches
//! it, with the values it carries and its `source()` chain. A variant no input reaches is
//! constructed directly for its text, and `tests/generated.rs` counts that it never occurs.

use std::error::Error;

use bunny_linalg::FixedVec3;
use bunny_num::FixedQ32_32;
use bunny_optics::{point_at, PinholeCamera, PixelError, SecondaryRayError};
use bunny_query::TraceRay;
use lumen::reference::{reference_lighting, reference_materials};
use lumen::{
    pixel_radiance, render_traced, trace_path, CameraPlacement, Hit, ImageSize, PathError,
    PrimitiveIndex, RenderError, RenderSettings, Scene, SceneSpec, Schedule, TraceError, Tracer,
    FAR,
};

fn point(x: i64, y: i64, z: i64) -> FixedVec3 {
    let unit = |n: i64| FixedQ32_32::from_raw(n << 32);
    FixedVec3::new(unit(x), unit(y), unit(z))
}

fn settings() -> RenderSettings {
    RenderSettings::new(1, 0).expect("valid settings")
}

/// A ground triangle at `y = 0` filling the view of a camera 4 units above it, looking down, and
/// after it a triangle of zero area (primitive 1) that no ray can hit.
fn ground_scene() -> Scene {
    let triangles = [
        [point(-64, 0, 64), point(64, 0, 64), point(0, 0, -64)],
        [point(1, 1, 1), point(1, 1, 1), point(1, 1, 1)],
    ];
    let camera = CameraPlacement {
        position: point(0, 4, 0),
        look: point(0, -1, 0),
        up: point(0, 0, -1),
        tan_half_fov: FixedQ32_32::from_raw(1 << 32),
    };
    Scene::new(&SceneSpec {
        triangles: &triangles,
        material_of: &[0, 0],
        materials: &reference_materials().expect("the reference materials")[..1],
        lighting: reference_lighting().expect("the reference lighting"),
        camera,
    })
    .expect("the test scene builds")
}

fn camera(scene: &Scene) -> PinholeCamera {
    scene.camera().camera(4, 4).expect("the camera accepts the size")
}

/// A tracer that answers with the scene, then changes every hit it reports with `change`.
struct Altered<'a> {
    scene: &'a Scene,
    change: fn(Hit) -> Hit,
}

impl Tracer for Altered<'_> {
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
        Ok(self.scene.closest(ray)?.map(self.change))
    }

    fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError> {
        self.scene.occluded(ray, t_max)
    }
}

/// Sample 0 of pixel `(1, 1)`, whose camera ray hits the ground, through `tracer`.
fn first_sample(scene: &Scene, tracer: &Altered<'_>) -> Result<(), PathError> {
    let sampler = settings().sampler(1, 1);
    trace_path(scene, tracer, &camera(scene), sampler, 0).map(|_| ())
}

/// Oracle: the camera's own sample. A pixel outside the camera's image has no ray, and the error
/// carries the sample index and the camera sample that was refused.
#[test]
fn a_pixel_outside_the_image_has_no_ray_and_names_its_sample() {
    let scene = ground_scene();
    let refused = pixel_radiance(&scene, &camera(&scene), settings(), 4, 0);
    let point = settings().sampler(4, 0).camera_sample(0);
    assert_eq!(refused, Err(PathError::NoRay { sample: 0, point }));
    assert_eq!(point.x, 4);
}

/// Oracle: the constructed tracer. A hit on primitive 2 of a two-triangle scene is refused with
/// the index and the count, not treated as a miss.
#[test]
fn a_hit_past_the_last_triangle_names_the_index_and_the_count() {
    let scene = ground_scene();
    let past =
        Altered { scene: &scene, change: |hit| Hit { primitive: PrimitiveIndex::new(2), ..hit } };
    let want = PathError::UnknownPrimitive { primitive: PrimitiveIndex::new(2), triangles: 2 };
    assert_eq!(first_sample(&scene, &past), Err(want));
}

/// Oracle: the constructed scene. Primitive 1 has zero area, so a hit reported on it has no
/// surface.
#[test]
fn a_hit_on_a_zero_area_triangle_is_refused_as_having_no_surface() {
    let scene = ground_scene();
    let flat =
        Altered { scene: &scene, change: |hit| Hit { primitive: PrimitiveIndex::new(1), ..hit } };
    let want = PathError::NoSurface { primitive: PrimitiveIndex::new(1) };
    assert_eq!(first_sample(&scene, &flat), Err(want));
}

/// The tracer reports the ground's hit at the farthest distance, `FAR`: a distance changed
/// after the query, whose point is far outside the trace domain, so Bunny's `SurfaceHit`
/// refuses it.
fn moved(scene: &Scene) -> Altered<'_> {
    Altered {
        scene,
        change: |mut hit| {
            hit.hit.distance = FAR;
            hit
        },
    }
}

/// Bunny's reason for refusing the moved hit of sample 0 of pixel `(x, y)`: the point
/// `point_at(ray, FAR)` of its camera ray, outside the trace domain.
fn refusal(scene: &Scene, x: u32, y: u32) -> SecondaryRayError {
    let sample = settings().sampler(x, y).camera_sample(0);
    let ray = camera(scene).ray(sample).expect("a camera ray");
    let point = point_at(&ray, FAR).expect("representable");
    SecondaryRayError::HitPointOutsideTraceDomain { point }
}

/// Oracle: Bunny's documented refusal, with the point Bunny's `point_at` gives. A hit whose
/// point is outside the trace domain is refused, and Bunny's reason, with the point it saw,
/// reaches the caller with the triangle.
#[test]
fn a_hit_bunny_refuses_carries_bunnys_reason_and_the_triangle() {
    let scene = ground_scene();
    let want =
        PathError::SurfaceHit { primitive: PrimitiveIndex::new(0), error: refusal(&scene, 1, 1) };
    assert_eq!(first_sample(&scene, &moved(&scene)), Err(want));
}

/// Oracle: raster order and the text, by hand. Every camera ray of the 4 by 4 image hits the
/// ground, so the first failing pixel is `(0, 0)` on one thread and on three; the render reports
/// it with its coordinates, and the chain runs render, path, Bunny, each stating its values.
#[test]
fn a_failed_render_names_the_pixel_and_chains_to_the_cause() {
    let scene = ground_scene();
    let size = ImageSize::new(4, 4).expect("valid size");
    let SecondaryRayError::HitPointOutsideTraceDomain { point } = refusal(&scene, 0, 0) else {
        unreachable!("refusal gives this variant")
    };
    let (x, y, z) = (point.x.raw(), point.y.raw(), point.z.raw());
    let bunny = format!("hit point raw ({x}, {y}, {z}) is outside the trace domain");
    let path = format!("the hit on triangle 0 was refused: {bunny}");
    let want = [format!("pixel (0, 0): {path}"), path, bunny];
    for threads in [1, 3] {
        let schedule = Schedule::new(threads, lumen::DEFAULT_BAND_ROWS).expect("valid schedule");
        let refused = render_traced(&scene, &moved(&scene), size, settings(), schedule);
        let Err(error) = refused else { panic!("the render was not refused") };
        assert!(matches!(error, RenderError::Pixel { x: 0, y: 0, .. }), "{error:?}");
        let mut chain = vec![error.to_string()];
        let mut cause = error.source();
        while let Some(next) = cause {
            chain.push(next.to_string());
            cause = next.source();
        }
        assert_eq!(chain, want, "{threads} threads");
    }
}

/// Oracle: the text written out by hand. The variants no input reaches state their values as
/// raw integers, never a `Debug` form.
#[test]
fn unreachable_path_errors_state_their_values() {
    let primitive = PrimitiveIndex::new(7);
    let half = FixedQ32_32::from_raw(1 << 31);
    let sample = [half, FixedQ32_32::from_raw(1 << 32)];
    let error = SecondaryRayError::SampleOutsideUnitInterval { sample };
    let cases = [
        (
            PathError::BounceRay { primitive, error },
            "the bounce from triangle 7 was refused: sample raw (2147483648, 4294967296) is \
             outside [0, 1)",
            true,
        ),
        (
            PathError::RadianceOverflow {
                primitive,
                direct: [FixedQ32_32::from_raw(i64::MAX), half, FixedQ32_32::ZERO],
                indirect: [FixedQ32_32::from_raw(1), FixedQ32_32::ZERO, FixedQ32_32::ZERO],
            },
            "triangle 7 reflects an unrepresentable radiance: direct raw (9223372036854775807, \
             2147483648, 0) plus indirect raw (1, 0, 0)",
            false,
        ),
        (
            PathError::Pixel(PixelError::RadianceTooLarge),
            "pixel value refused: sample radiance exceeds the accepted maximum",
            true,
        ),
    ];
    for (error, text, wraps) in cases {
        assert_eq!(error.to_string(), text);
        assert_eq!(error.source().is_some(), wraps, "{error:?}");
    }
}
