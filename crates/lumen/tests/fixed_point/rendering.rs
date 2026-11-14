//! Rendering the scene: every traced ray against a test of every triangle, and the golden
//! image hash.

use std::cell::Cell;

use bunny_num::FixedQ32_32;
use bunny_optics::SHADOW_T_MAX;
use bunny_query::TraceRay;
use lumen::scenes::fixed_point::PartKind;
use lumen::{fixed_point_scene, render, ImageSize, RenderSettings, Scene, BUNNY_REV};
use lumen::{trace_path, Hit, PrimitiveIndex, TraceError, Tracer, FAR, TONE_CURVE};

use crate::golden::GOLDEN_FIXED_POINT_96X54_4SPP;
use crate::support::content;

fn settings(samples: u32, seed: u32) -> RenderSettings {
    RenderSettings::new(samples, seed).expect("valid settings")
}

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

/// Answers with the scene after checking each answer against brute force, counting hits on
/// the parts that matter.
struct Checked<'a> {
    scene: &'a Scene,
    kinds: Vec<(std::ops::Range<usize>, PartKind)>,
    hits: Cell<u32>,
    misses: Cell<u32>,
    shadowed: Cell<u32>,
    lit: Cell<u32>,
    on: [Cell<u32>; 3],
}

impl Checked<'_> {
    fn count(&self, primitive: PrimitiveIndex) {
        let index = primitive.to_usize().expect("fits");
        let kind = self.kinds.iter().find(|(range, _)| range.contains(&index)).map(|(_, k)| *k);
        let slot = match kind {
            Some(PartKind::Bunny) => 0,
            Some(PartKind::DieBody(_) | PartKind::Pips(_)) => 1,
            Some(PartKind::Numeral(_) | PartKind::PuckFace | PartKind::PuckRim) => 2,
            _ => return,
        };
        self.on[slot].set(self.on[slot].get() + 1);
    }
}

impl Tracer for Checked<'_> {
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
        let fast = self.scene.closest(ray)?;
        assert_eq!(fast, brute_closest(self.scene, ray), "closest hit of {ray:?}");
        match fast {
            Some(hit) => {
                self.hits.set(self.hits.get() + 1);
                self.count(hit.primitive);
            }
            None => self.misses.set(self.misses.get() + 1),
        }
        Ok(fast)
    }

    fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError> {
        assert_eq!(t_max, SHADOW_T_MAX, "shadow rays use SHADOW_T_MAX");
        let fast = self.scene.occluded(ray, t_max)?;
        let brute = self
            .scene
            .triangles()
            .iter()
            .any(|t| ray.hit_triangle(t, FixedQ32_32::ZERO, t_max).is_some());
        assert_eq!(fast, brute, "any hit of {ray:?}");
        let counter = if fast { &self.shadowed } else { &self.lit };
        counter.set(counter.get() + 1);
        Ok(fast)
    }
}

/// Oracle: a test of every triangle (the definition of the nearest hit and of any hit), for
/// every camera, shadow and bounce ray of every sample of a 32 by 18 image at 2 samples.
#[test]
fn every_traced_ray_of_the_scene_equals_a_test_of_every_triangle() {
    let scene = fixed_point_scene().expect("builds");
    let kinds = content().parts.into_iter().map(|p| (p.primitives, p.kind)).collect();
    let (width, height) = (32, 18);
    let camera = scene.camera().camera(width, height).expect("camera");
    let checked = Checked {
        scene: &scene,
        kinds,
        hits: Cell::new(0),
        misses: Cell::new(0),
        shadowed: Cell::new(0),
        lit: Cell::new(0),
        on: [Cell::new(0), Cell::new(0), Cell::new(0)],
    };
    for y in 0..height {
        for x in 0..width {
            let sampler = settings(2, 0).sampler(x, y);
            for sample in 0..2 {
                let path = trace_path(&scene, &checked, &camera, sampler, sample).expect("traces");
                let direct = trace_path(&scene, &scene, &camera, sampler, sample).expect("traces");
                assert_eq!(path, direct);
            }
        }
    }
    let (hits, misses) = (checked.hits.get(), checked.misses.get());
    let (shadowed, lit) = (checked.shadowed.get(), checked.lit.get());
    let [bunny, dice, layout] = checked.on.each_ref().map(Cell::get);
    // Meaningful only if every kind of answer occurred, on the bunny, the dice and the layout.
    // (At 806 hits, 1 030 misses, 26 shadowed and 720 lit points when recorded.)
    assert!(
        hits > 600 && misses > 500 && shadowed > 15 && lit > 500,
        "{hits} hits, {misses} misses, {shadowed} shadowed, {lit} lit"
    );
    assert!(bunny > 50 && dice > 25 && layout > 10, "{bunny} bunny, {dice} dice, {layout} layout");
}

#[test]
fn the_fixed_point_image_has_its_golden_hash() {
    assert_eq!(lumen::FIXED_POINT_SCENE_ID, "lumen-fixed-point-scene/1");
    assert_eq!(lumen::INTEGRATOR_ID, "lumen-diffuse-path/1");
    assert_eq!(bunny_optics::SAMPLER_ID, "bunny-sobol-owen/1");
    assert_eq!(bunny_optics::SHADING_ID, "bunny-diffuse-shading/1");
    assert_eq!(TONE_CURVE.identity(), "bunny-tone-srgb/1");
    assert_eq!(bunny_optics::RGBA8_OPAQUE_IDENTITY, "bunny-rgba8-opaque/1");
    assert_eq!(BUNNY_REV, "d4fe51d38fa10f8d6496bd110955314edac30bbf");
    let scene = fixed_point_scene().expect("builds");
    let size = ImageSize::new(96, 54).expect("size");
    let rendered = render(&scene, size, settings(4, 0)).expect("renders");
    assert_eq!(rendered.image.hash(), GOLDEN_FIXED_POINT_96X54_4SPP);
}
