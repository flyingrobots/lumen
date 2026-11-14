//! Size class: small (in process, no file, no network).
//!
//! Every error variant lumen reports, produced through the public API where an input reaches
//! it, with the values it carries and its `source()` chain. A variant no input reaches is
//! constructed directly for its text, and `tests/generated.rs` counts that it never occurs.

use std::error::Error;
use std::io;

use bunny_broadphase::TraversalError;
use bunny_linalg::FixedVec3;
use bunny_num::FixedQ32_32;
use bunny_optics::{point_at, PinholeCamera, PixelError, SecondaryRayError};
use bunny_query::TraceRay;
use lumen::reference::{reference_lighting, reference_materials};
use lumen::{
    pixel_radiance, render_traced, trace_path, CameraPlacement, Hit, ImageSize, PathError,
    PrimitiveIndex, RenderError, RenderSettings, Reservation, Scene, SceneSpec, Schedule,
    SpawnError, TraceError, Tracer, FAR,
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
        normals: &[],
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

/// Oracle: the text written out by hand, and the allocator's and the operating system's own
/// errors as sources. A failed reservation names what was being reserved and how much, and a
/// refused worker its number; neither can be forced on a host with memory and threads to spare,
/// so both are built here from a real allocator refusal and a real `io::Error`. The spawn path
/// is driven end to end through an injected spawner in `src/workers/tests.rs`.
#[test]
fn memory_and_spawn_failures_name_what_they_were_doing_and_chain_to_the_cause() {
    let refusal = Vec::<u8>::new().try_reserve_exact(usize::MAX).expect_err("too large");
    let reserving = Reservation::Image { bytes: 1 << 29 };
    let memory = RenderError::OutOfMemory { reserving, error: refusal.clone() };
    let text = format!("not enough memory for the image's 536870912 bytes: {refusal}");
    assert_eq!(memory.to_string(), text);
    assert_eq!(memory.source().map(ToString::to_string), Some(refusal.to_string()));
    let handles = Reservation::Workers { handles: 17 };
    assert_eq!(handles.to_string(), "17 worker thread handles");

    let spawn = RenderError::Spawn(SpawnError::new(3, io::Error::other("no threads left")));
    assert_eq!(spawn.to_string(), "worker thread 3 could not be started: no threads left");
    let os = spawn.source().and_then(Error::source).map(ToString::to_string);
    assert_eq!(os.as_deref(), Some("no threads left"));
}

/// Oracle: the text written out by hand. Bunny's `TraversalError` has no `Display`, so lumen
/// names each kind; no error prints a `Debug` form. The scene's own tracer reaches none of these
/// (`tests/generated.rs` counts that); a custom `Tracer` may return any of them.
#[test]
fn trace_errors_state_what_the_traversal_did() {
    let primitive = PrimitiveIndex::new(9);
    let cases = [
        (TraceError::Traversal(TraversalError::StackOverflow), "its stack of 64 nodes overflowed"),
        (
            TraceError::Traversal(TraversalError::InvalidNodeIndex),
            "a node names a child outside the tree",
        ),
        (
            TraceError::Traversal(TraversalError::InvalidPrimitiveRange),
            "a leaf names primitives outside the primitive-index list",
        ),
    ];
    for (error, why) in cases {
        assert_eq!(error.to_string(), format!("traversal failed: {why}"));
    }
    let unknown = TraceError::UnknownPrimitive { primitive, triangles: 4 };
    assert_eq!(
        unknown.to_string(),
        "the traversal named triangle 9, but the scene has 4 triangles"
    );
    let distance = FixedQ32_32::from_raw(3 << 31);
    let again = TraceError::HitNotRepeatable { primitive, distance };
    let text = "the traversal's hit on triangle 9 at distance raw 6442450944 could not be repeated";
    assert_eq!(again.to_string(), text);
    let tree = lumen::SceneError::TreeNamesUnknownPrimitive { index: 12, triangles: 5 };
    let text = "the bounding volume hierarchy names triangle 12, but the scene has 5 triangles";
    assert_eq!(tree.to_string(), text);
}

/// Oracle: the text written out by hand. The scene refusals no input reaches (Bunny's tree
/// builders refusing, a count past `u32::MAX`, a node count past `usize`), built directly; the
/// reachable ones are produced by `Scene::new` in `src/scene/tests.rs`. Bunny's `CompactError`
/// has no `Display`, so lumen names each kind.
#[test]
fn unreachable_scene_errors_state_their_values() {
    use bunny_broadphase::CompactError;
    use lumen::{SceneBuffer, SceneError};
    let primitive = PrimitiveIndex::new(4);
    let cases = [
        (
            SceneError::TooManyTriangles { triangles: 1 << 33, limit: u32::MAX },
            "scene has 8589934592 triangles, more than 4294967295",
        ),
        (
            SceneError::TriangleRefused { primitive },
            "triangle 4 was refused, with every coordinate in the domain",
        ),
        (
            SceneError::NodeCountOverflow { triangles: 1 << 31 },
            "a tree over 2147483648 triangles has more nodes than a usize counts",
        ),
        (
            SceneError::BvhBuild { triangles: 3, nodes: 6 },
            "the bounding volume hierarchy over 3 triangles could not be built in 6 nodes",
        ),
        (
            SceneError::BvhCompact(CompactError::InvalidNodeIndex),
            "the bounding volume hierarchy could not be converted: a node names a child outside \
             the tree",
        ),
        (
            SceneError::BvhCompact(CompactError::OutputTooSmall),
            "the bounding volume hierarchy could not be converted: the output holds fewer nodes \
             than the tree has",
        ),
        (
            SceneError::BvhCompact(CompactError::NotATree),
            "the bounding volume hierarchy could not be converted: a node is reached twice",
        ),
        (
            SceneError::BvhCompact(CompactError::TooDeep),
            "the bounding volume hierarchy could not be converted: the tree is deeper than 64 \
             levels",
        ),
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
        assert!(error.source().is_none(), "{error:?}");
    }
    let refusal = Vec::<u8>::new().try_reserve_exact(usize::MAX).expect_err("too large");
    let memory = SceneError::OutOfMemory {
        buffer: SceneBuffer::CompactNodes,
        items: 9,
        error: refusal.clone(),
    };
    assert_eq!(
        memory.to_string(),
        format!("not enough memory for the scene's 9 compact tree nodes: {refusal}")
    );
    assert_eq!(memory.source().map(ToString::to_string), Some(refusal.to_string()));
}

/// Oracle: the camera Bunny refuses, by hand (a zero look direction), and each layer's text.
/// A refused scene inside a built-in scene's error chains down to Bunny's camera error, every
/// layer including the next one's message.
#[test]
fn a_scene_refusal_chains_from_the_built_in_scene_to_bunny() {
    use lumen::scenes::fixed_point::FixedPointError;
    use lumen::{BuiltinSceneError, SceneError};
    let mut camera = ground_scene().camera();
    camera.look = point(0, 0, 0);
    let triangles = [[point(-1, 0, 1), point(1, 0, 1), point(0, 0, -1)]];
    let refused = Scene::new(&SceneSpec {
        triangles: &triangles,
        material_of: &[0],
        materials: &reference_materials().expect("the reference materials")[..1],
        normals: &[],
        lighting: reference_lighting().expect("the reference lighting"),
        camera,
    })
    .expect_err("a zero look direction is refused");
    assert!(matches!(refused, SceneError::Camera(_)), "{refused:?}");
    let error = BuiltinSceneError::FixedPoint(FixedPointError::Scene(refused));
    let mut chain = vec![error.to_string()];
    let mut cause = error.source();
    while let Some(next) = cause {
        chain.push(next.to_string());
        cause = next.source();
    }
    assert_eq!(chain.len(), 4, "{chain:?}");
    for pair in chain.windows(2) {
        assert!(pair[0].ends_with(&pair[1]), "{pair:?}");
    }
    assert!(chain[0].starts_with(
        "the Fixed Point scene: the scene could not be built: invalid camera placement: "
    ));
}

/// A writer that accepts `room` bytes, then refuses every write as a full disk would.
struct Full {
    room: usize,
}

impl std::io::Write for Full {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.room == 0 {
            return Err(io::Error::new(io::ErrorKind::StorageFull, "no space left"));
        }
        let taken = bytes.len().min(self.room);
        self.room -= taken;
        Ok(taken)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Oracle: the constructed writer. A writer that fails part way, in the header or in the pixel
/// data, gives `PngError::Write` with the writer's error, its kind and text kept, as the source.
/// The encoder's own length checks are built directly for their text: no image reaches them,
/// which `src/png/generated.rs` counts over 73 sizes.
#[test]
fn a_failed_png_write_keeps_the_writers_error() {
    use lumen::{write_png, PngError, Rgba8Image};
    let size = ImageSize::new(64, 300).expect("valid size");
    let image = Rgba8Image::from_bytes(size, vec![7; size.byte_len()]).expect("right length");
    for room in [0, 20, 70_000] {
        let refused = write_png(&mut Full { room }, &image).expect_err("the writer is full");
        let PngError::Write(error) = &refused else { panic!("{refused:?}") };
        assert_eq!(error.kind(), io::ErrorKind::StorageFull, "after {room} bytes");
        assert_eq!(refused.to_string(), "no space left");
        assert_eq!(refused.source().map(ToString::to_string).as_deref(), Some("no space left"));
    }
    let chunk = PngError::ChunkTooLong { length: 1 << 33 };
    assert_eq!(
        chunk.to_string(),
        "a PNG chunk of 8589934592 bytes is longer than its length field holds"
    );
    let block = PngError::BlockTooLong { length: 65_536 };
    assert_eq!(block.to_string(), "a stored block of 65536 bytes is longer than 65535");
    assert!(chunk.source().is_none() && block.source().is_none());
}
