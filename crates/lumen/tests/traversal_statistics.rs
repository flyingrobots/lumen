//! Size class: medium (real threads at fixed counts; in process, no file, no network).
//!
//! A render's traversal statistics (`render_traced_with_traversal`, ADR 0008) against something
//! other than the code that attributes and adds them: a tracer that records every counted query
//! itself and tells camera rays from bounce rays by their origin, a scene whose every ray's work
//! is known by hand, and the plain render and the golden hashes, which counting must not change.

use std::sync::Mutex;

use bunny_broadphase::TraversalCounts;
use bunny_linalg::FixedVec3;
use bunny_num::FixedQ32_32;
use bunny_query::TraceRay;
use lumen::reference::{reference_lighting, reference_materials};
use lumen::{
    render_scheduled, render_traced, render_traced_with_run, render_traced_with_traversal,
    BandRows, BuiltinScene, CameraPlacement, CountingTracer, Hit, ImageSize, PrimitiveIndex,
    RenderSettings, Scene, SceneSpec, Schedule, TraceError, Tracer, TraversalStatistics,
    TraversalWork,
};

mod golden;
use golden::{GOLDEN_96X54_4SPP, GOLDEN_FIXED_POINT_96X54_4SPP};

const ONE: i64 = 1 << 32;

fn at(x: i64, y: i64, z: i64) -> FixedVec3 {
    let unit = |n: i64| FixedQ32_32::from_raw(n * ONE);
    FixedVec3::new(unit(x), unit(y), unit(z))
}

fn schedule(threads: u32, rows: u32) -> Schedule {
    Schedule::new(threads, BandRows::new(rows).expect("rows")).expect("schedule")
}

fn fields(c: TraversalCounts) -> [u64; 7] {
    [
        c.box_tests,
        c.box_entries,
        c.popped_skipped,
        c.interior_visits,
        c.leaf_visits,
        c.primitive_tests,
        c.peak_stack,
    ]
}

/// Oracle: the plain render (`render_scheduled`, no counting) and the golden image hashes. For
/// both built-in scenes at 96 by 54, 4 samples, at 1, 2, 4 and 18 threads and band heights 1 and
/// 3, the counting render gives the golden image and the same render (image, ray counts and ray
/// statistics) as the plain one, and the same traversal statistics at every schedule: they are
/// sums and maxima over rays. Witness: every kind did work in both scenes.
#[test]
fn counting_changes_no_pixel_and_counts_the_same_at_every_schedule() {
    let (size, settings) =
        (ImageSize::new(96, 54).expect("size"), RenderSettings::new(4, 0).expect("ok"));
    for (builtin, golden) in [
        (BuiltinScene::Reference, GOLDEN_96X54_4SPP),
        (BuiltinScene::FixedPoint, GOLDEN_FIXED_POINT_96X54_4SPP),
    ] {
        let scene = builtin.build().expect("builds");
        let mut first: Option<TraversalStatistics> = None;
        for threads in [1, 2, 4, 18] {
            for rows in [1, 3] {
                let at = format!("{}, {threads} threads, bands of {rows}", builtin.name());
                let schedule = schedule(threads, rows);
                let plain = render_scheduled(&scene, size, settings, schedule).expect(&at);
                let (counted, _, traversal) =
                    render_traced_with_traversal(&scene, &scene, size, settings, schedule)
                        .expect(&at);
                assert_eq!(counted.image.hash(), golden, "{at}: the golden image");
                assert_eq!(counted, plain, "{at}: the same render");
                assert_eq!(*first.get_or_insert(traversal), traversal, "{at}: the same counts");
            }
        }
        let work = first.expect("rendered");
        for (kind, w) in [("camera", work.camera), ("bounce", work.bounce), ("shadow", work.shadow)]
        {
            assert!(w.total.box_tests > 0 && w.total.primitive_tests > 0, "{builtin:?} {kind}");
        }
    }
}

/// What the recording tracer saw, per kind.
#[derive(Default)]
struct Seen {
    camera: TraversalWork,
    bounce: TraversalWork,
    shadow: TraversalWork,
}

/// Answers every counted query with the scene and records its counts itself: closest-hit
/// queries from the camera's eye point are camera rays (Bunny's pinhole camera starts every ray
/// there), other closest-hit queries bounce rays, any-hit queries shadow rays. Its own sum and
/// counter-by-counter maximum, under a lock a render on several threads shares.
struct Recording<'a> {
    scene: &'a Scene,
    eye: FixedVec3,
    seen: Mutex<Seen>,
}

/// `work` with one more ray's `counts`: the sums by hand, the maxima by hand.
fn record(work: &mut TraversalWork, counts: TraversalCounts) {
    let (total, most) = (&mut work.total, &mut work.most);
    total.box_tests += counts.box_tests;
    total.box_entries += counts.box_entries;
    total.popped_skipped += counts.popped_skipped;
    total.interior_visits += counts.interior_visits;
    total.leaf_visits += counts.leaf_visits;
    total.primitive_tests += counts.primitive_tests;
    total.peak_stack = total.peak_stack.max(counts.peak_stack);
    for (m, c) in [
        (&mut most.box_tests, counts.box_tests),
        (&mut most.box_entries, counts.box_entries),
        (&mut most.popped_skipped, counts.popped_skipped),
        (&mut most.interior_visits, counts.interior_visits),
        (&mut most.leaf_visits, counts.leaf_visits),
        (&mut most.primitive_tests, counts.primitive_tests),
        (&mut most.peak_stack, counts.peak_stack),
    ] {
        *m = (*m).max(c);
    }
}

impl Tracer for Recording<'_> {
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
        self.scene.closest(ray)
    }

    fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError> {
        self.scene.occluded(ray, t_max)
    }
}

impl CountingTracer for Recording<'_> {
    fn closest_counted(
        &self,
        ray: &TraceRay,
    ) -> Result<(Option<Hit>, TraversalCounts), TraceError> {
        let (hit, counts) = self.scene.closest_counted(ray)?;
        let mut seen = self.seen.lock().expect("unpoisoned");
        let kind = if ray.origin() == self.eye { &mut seen.camera } else { &mut seen.bounce };
        record(kind, counts);
        Ok((hit, counts))
    }

    fn occluded_counted(
        &self,
        ray: &TraceRay,
        t_max: FixedQ32_32,
    ) -> Result<(bool, TraversalCounts), TraceError> {
        let (blocked, counts) = self.scene.occluded_counted(ray, t_max)?;
        record(&mut self.seen.lock().expect("unpoisoned").shadow, counts);
        Ok((blocked, counts))
    }
}

/// Oracle: a tracer that records every query's counts by itself, telling the kinds apart by the
/// ray's origin rather than by the path's step. At 1 and 4 threads (so several workers' sums are
/// joined), on both built-in scenes, the render's statistics equal what it recorded, kind by kind,
/// totals and maxima.
#[test]
fn the_render_reports_what_a_recording_tracer_saw() {
    let (size, settings) =
        (ImageSize::new(40, 24).expect("size"), RenderSettings::new(2, 5).expect("ok"));
    for builtin in BuiltinScene::ALL {
        let scene = builtin.build().expect("builds");
        let eye = scene.camera().camera(40, 24).expect("camera").position();
        for threads in [1, 4] {
            let tracer = Recording { scene: &scene, eye, seen: Mutex::new(Seen::default()) };
            let (_, _, got) =
                render_traced_with_traversal(&scene, &tracer, size, settings, schedule(threads, 2))
                    .expect("renders");
            let seen = tracer.seen.into_inner().expect("unpoisoned");
            let at = format!("{} at {threads} threads", builtin.name());
            assert_eq!(got.camera, seen.camera, "{at}: camera rays");
            assert_eq!(got.bounce, seen.bounce, "{at}: bounce rays");
            assert_eq!(got.shadow, seen.shadow, "{at}: shadow rays");
        }
    }
}

/// Oracle: Bunny's search by hand. One triangle behind a camera that looks the other way: every
/// camera ray tests the root's box and misses it (one box test, nothing else), so no ray hits and
/// there is no bounce or shadow ray. On a 6 by 5 image at 4 samples that is 120 camera rays of
/// one box test each: a total of 120, a most of 1, and zero elsewhere.
#[test]
fn a_camera_that_sees_only_sky_tests_one_box_per_ray() {
    let materials = reference_materials().expect("materials");
    let triangle = [at(-4, -1, 8), at(4, -1, 8), at(0, 3, 8)];
    let scene = Scene::new(&SceneSpec {
        triangles: &[triangle],
        material_of: &[0],
        materials: &materials,
        normals: &[],
        lighting: reference_lighting().expect("lighting"),
        camera: CameraPlacement {
            position: at(0, 0, 4),
            look: at(0, 0, -1),
            up: at(0, 1, 0),
            tan_half_fov: FixedQ32_32::from_raw(ONE),
        },
    })
    .expect("builds");
    let (size, settings) =
        (ImageSize::new(6, 5).expect("size"), RenderSettings::new(4, 0).expect("ok"));
    let (render, _, got) =
        render_traced_with_traversal(&scene, &scene, size, settings, schedule(2, 1))
            .expect("renders");
    assert_eq!(render.statistics.outcomes.camera_misses, 120);
    assert_eq!(fields(got.camera.total), [120, 0, 0, 0, 0, 0, 0], "camera total");
    assert_eq!(fields(got.camera.most), [1, 0, 0, 0, 0, 0, 0], "camera most");
    assert_eq!(got.bounce, TraversalWork::default(), "no bounce ray");
    assert_eq!(got.shadow, TraversalWork::default(), "no shadow ray");
}

/// Fails every query whose ray hits triangle 28 000 or above (in the reference scene's bunny), in
/// both forms, as a tracer that contradicts its tree would.
struct FailsOnTheBunny<'a>(&'a Scene);

fn refuse(hit: Option<Hit>) -> Result<Option<Hit>, TraceError> {
    match hit {
        Some(hit) if hit.primitive >= PrimitiveIndex::new(28_000) => {
            Err(TraceError::UnknownPrimitive { primitive: hit.primitive, triangles: 0 })
        }
        other => Ok(other),
    }
}

impl Tracer for FailsOnTheBunny<'_> {
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
        refuse(self.0.closest(ray)?)
    }

    fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError> {
        self.0.occluded(ray, t_max)
    }
}

impl CountingTracer for FailsOnTheBunny<'_> {
    fn closest_counted(
        &self,
        ray: &TraceRay,
    ) -> Result<(Option<Hit>, TraversalCounts), TraceError> {
        let (hit, counts) = self.0.closest_counted(ray)?;
        Ok((refuse(hit)?, counts))
    }

    fn occluded_counted(
        &self,
        ray: &TraceRay,
        t_max: FixedQ32_32,
    ) -> Result<(bool, TraversalCounts), TraceError> {
        self.0.occluded_counted(ray, t_max)
    }
}

/// Oracle: the other path. The counting render reports the same raster-first failure as the
/// plain and the band-counting renders, at 1, 2, 4 and 18 threads and band heights 1 and 3.
#[test]
fn counting_reports_the_same_failure() {
    let scene = BuiltinScene::Reference.build().expect("builds");
    let tracer = FailsOnTheBunny(&scene);
    let (size, settings) =
        (ImageSize::new(32, 18).expect("size"), RenderSettings::new(1, 0).expect("ok"));
    let want = render_traced(&scene, &tracer, size, settings, schedule(1, 1)).expect_err("fails");
    for threads in [1, 2, 4, 18] {
        for rows in [1, 3] {
            let at = format!("{threads} threads, bands of {rows}");
            let s = schedule(threads, rows);
            let run = render_traced_with_run(&scene, &tracer, size, settings, s);
            let counted = render_traced_with_traversal(&scene, &tracer, size, settings, s);
            assert_eq!(run.expect_err(&at), want, "{at}");
            assert_eq!(counted.expect_err(&at), want, "{at}");
        }
    }
}
