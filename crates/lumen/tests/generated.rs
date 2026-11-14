//! Size class: small (in process, no file, no network).
//!
//! Generated evidence for "for all" claims about the renderer (testing standard rule 5), with a
//! fixed seed, one derived seed per case, and both in every failure message.

use lumen::reference::{reference_camera, reference_lighting, reference_materials};
use lumen::{
    pixel_radiance, reference_scene, trace_path, BuiltinScene, RenderSettings, Scene, SceneSpec,
    Tracer, FAR,
};

use bunny_linalg::FixedVec3;
use bunny_num::FixedQ32_32;
use bunny_optics::PixelAccumulator;
use bunny_query::TraceRay;

mod generator;
use generator::Gen;

const SEED: u64 = 0x5EED_0000_0DE4;

/// Accumulates `samples` in `order`.
fn accumulate(samples: &[[FixedQ32_32; 3]], order: &[usize]) -> PixelAccumulator {
    let mut acc = PixelAccumulator::new();
    for &i in order {
        acc.add_sample(samples[i]).expect("adds");
    }
    acc
}

/// Merges one accumulator per sample in a random tree: while more than one remains, two random
/// ones become their merge, in random operand order.
fn merge_tree(samples: &[[FixedQ32_32; 3]], rng: &mut Gen) -> PixelAccumulator {
    let mut parts: Vec<PixelAccumulator> =
        (0..samples.len()).map(|i| accumulate(samples, &[i])).collect();
    while parts.len() > 1 {
        let a = parts.swap_remove(usize::try_from(rng.below(parts.len() as u64)).expect("index"));
        let mut b =
            parts.swap_remove(usize::try_from(rng.below(parts.len() as u64)).expect("index"));
        if rng.below(2) == 0 {
            b.merge(&a).expect("merges");
            parts.push(b);
        } else {
            let mut a = a;
            a.merge(&b).expect("merges");
            parts.push(a);
        }
    }
    parts[0]
}

/// For all pixels, seeds and orders: a pixel's result is `pixel_radiance`, however its 16
/// samples are evaluated (in a random order), accumulated (a random permutation) or merged
/// (a random tree). Oracle: `pixel_radiance`, which evaluates in index order. 6 pixels, 60
/// orders and 60 trees each at 16 samples; 0.05 s in the test profile.
#[test]
fn a_pixel_does_not_depend_on_the_order_its_samples_are_evaluated_accumulated_or_merged() {
    let scene = reference_scene().expect("builds");
    let (width, height) = (32, 18);
    let camera = scene.camera().camera(width, height).expect("the camera accepts the size");
    for case in 0..6_u64 {
        let case_seed = Gen::case_seed(SEED, case);
        let mut rng = Gen::new(case_seed);
        let (x, y) = (
            u32::try_from(rng.below(width.into())).expect("a column"),
            u32::try_from(rng.below(height.into())).expect("a row"),
        );
        let settings = RenderSettings::new(16, u32::try_from(rng.below(1 << 32)).expect("a seed"))
            .expect("valid settings");
        let at = format!("pixel ({x}, {y}), seed {SEED:#x}, case {case}, case seed {case_seed:#x}");
        let (want, _) = pixel_radiance(&scene, &camera, settings, x, y).expect("renders");

        let sampler = settings.sampler(x, y);
        let mut evaluation: Vec<usize> = (0..16).collect();
        rng.shuffle(&mut evaluation);
        let mut values = vec![[FixedQ32_32::ZERO; 3]; 16];
        for &i in &evaluation {
            let sample = u32::try_from(i).expect("small");
            values[i] =
                trace_path(&scene, &scene, &camera, sampler, sample).expect("traces").radiance;
        }

        let reference = accumulate(&values, &(0..16).collect::<Vec<_>>());
        assert_eq!(reference.resolve(), Ok(want), "index order, {at}");
        for round in 0..60 {
            let mut order: Vec<usize> = (0..16).collect();
            rng.shuffle(&mut order);
            let permuted = accumulate(&values, &order);
            assert_eq!(
                permuted.sums(),
                reference.sums(),
                "sums, order {order:?}, round {round}, {at}"
            );
            assert_eq!(
                permuted.resolve(),
                Ok(want),
                "resolve, order {order:?}, round {round}, {at}"
            );
            let merged = merge_tree(&values, &mut rng);
            assert_eq!(merged.sums(), reference.sums(), "merged sums, round {round}, {at}");
            assert_eq!(merged.count(), 16, "merged count, round {round}, {at}");
            assert_eq!(merged.resolve(), Ok(want), "merged resolve, round {round}, {at}");
        }
    }
}

/// For all pixels, sizes, seeds and sample indices of both built-in scenes: `trace_path` with
/// the scene as its own tracer returns no error. `PathError::BounceRay`,
/// `PathError::RadianceOverflow` and `PathError::Pixel` are in the API for a refusal no input is
/// known to reach; this counts that none occurs, and that the paths counted reached the shading
/// steps (a camera hit and its bounce). Oracle: the absence of any error, with a witness count.
/// 2 scenes, 40 cases of 8 samples each.
#[test]
fn no_path_of_the_built_in_scenes_fails() {
    for scene_kind in BuiltinScene::ALL {
        let scene = scene_kind.build().expect("builds");
        let mut shaded = 0_u32;
        for case in 0..40_u64 {
            let case_seed = Gen::case_seed(SEED ^ 0xE77, case);
            let mut rng = Gen::new(case_seed);
            let width = u32::try_from(rng.below(64) + 1).expect("a width");
            let height = u32::try_from(rng.below(36) + 1).expect("a height");
            let camera = scene.camera().camera(width, height).expect("the camera accepts it");
            let (x, y) = (
                u32::try_from(rng.below(width.into())).expect("a column"),
                u32::try_from(rng.below(height.into())).expect("a row"),
            );
            let seed = u32::try_from(rng.below(1 << 32)).expect("a seed");
            let sampler = RenderSettings::new(1, seed).expect("valid settings").sampler(x, y);
            for _ in 0..8 {
                let sample = u32::try_from(rng.below(1024)).expect("a sample");
                let at = format!(
                    "{scene_kind:?} {width}x{height} pixel ({x}, {y}) seed {seed} sample \
                     {sample}, case {case}, case seed {case_seed:#x}"
                );
                let path = trace_path(&scene, &scene, &camera, sampler, sample);
                let path = path.unwrap_or_else(|error| panic!("{error}: {at}"));
                if path.rays.closest == 2 {
                    shaded += 1;
                }
            }
        }
        assert!(shaded > 0, "{scene_kind:?}: no path reached the shading steps");
    }
}

/// A coordinate in `[-8, 8)` units, on a grid of `2^-8`.
fn coordinate(rng: &mut Gen) -> FixedQ32_32 {
    let steps = i64::try_from(rng.below(1 << 12)).expect("small") - (1 << 11);
    FixedQ32_32::from_raw(steps << 24)
}

fn random_point(rng: &mut Gen) -> FixedVec3 {
    FixedVec3::new(coordinate(rng), coordinate(rng), coordinate(rng))
}

/// For all scenes of 1 to 64 random triangles in a 16-unit cube and all rays from random points
/// in it: `Scene::new` accepts the scene, so its tree names only its own triangles
/// (`SceneError::TreeNamesUnknownPrimitive` never occurs), and the scene's tracer answers every
/// query, so `TraceError::UnknownPrimitive` and `HitNotRepeatable` never occur either. Oracle:
/// the absence of any error, with a witness count of rays that hit. 40 scenes, 32 rays each.
#[test]
fn random_scenes_build_and_answer_every_query() {
    let materials = reference_materials().expect("the reference materials");
    let (lighting, camera) = (reference_lighting().expect("lighting"), reference_camera());
    let mut hits = 0_u32;
    for case in 0..40_u64 {
        let case_seed = Gen::case_seed(SEED ^ 0x7EEE, case);
        let mut rng = Gen::new(case_seed);
        let count = usize::try_from(rng.below(64) + 1).expect("a count");
        let triangles: Vec<[FixedVec3; 3]> = (0..count)
            .map(|_| [random_point(&mut rng), random_point(&mut rng), random_point(&mut rng)])
            .collect();
        let material_of = vec![0_u32; count];
        let spec = SceneSpec {
            triangles: &triangles,
            material_of: &material_of,
            materials: &materials,
            normals: &[],
            lighting,
            camera,
        };
        let at = format!("case {case}, case seed {case_seed:#x}, {count} triangles");
        let scene = Scene::new(&spec).unwrap_or_else(|error| panic!("{error}: {at}"));
        for _ in 0..32 {
            let (origin, toward) = (random_point(&mut rng), random_point(&mut rng));
            let Some(ray) = TraceRay::new(origin, toward) else { continue };
            let found = scene.closest(&ray).unwrap_or_else(|error| panic!("{error}: {at}"));
            scene.occluded(&ray, FAR).unwrap_or_else(|error| panic!("{error}: {at}"));
            hits += u32::from(found.is_some());
        }
    }
    assert!(hits > 0, "no ray hit a triangle");
}
