//! Size class: small (in process, no file, no network).
//!
//! Generated evidence for "for all" claims about the renderer (testing standard rule 5), with a
//! fixed seed, one derived seed per case, and both in every failure message.

use lumen::{pixel_radiance, reference_scene, trace_path, RenderSettings};

use bunny_num::FixedQ32_32;
use bunny_optics::PixelAccumulator;

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
