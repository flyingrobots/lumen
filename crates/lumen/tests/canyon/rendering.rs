//! Rendering the scene: what the camera sees, every traced ray of a small image against a test
//! of every triangle, and the golden image hash and content identity.

use std::cell::Cell;

use bunny_num::FixedQ32_32;
use bunny_optics::{PixelSample, SHADOW_T_MAX};
use bunny_query::TraceRay;
use lumen::scenes::canyon::{Craft, PartKind};
use lumen::{render, trace_path, Hit, ImageSize, PrimitiveIndex, RenderSettings, Scene};
use lumen::{TraceError, Tracer, BUNNY_REV, FAR};

use crate::golden::{GOLDEN_BUNNY_REV, GOLDEN_CANYON_96X54_4SPP, GOLDEN_CANYON_CONTENT};
use crate::support::{content, scene};

fn settings(samples: u32, seed: u32) -> RenderSettings {
    RenderSettings::new(samples, seed).expect("valid settings")
}

/// What the camera sees, grouped for the composition: the bunny, a craft, the planet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Subject {
    Bunny,
    Craft(Craft),
    Planet,
}

/// The subject of the part holding `primitive`, if it is one.
fn subject(primitive: PrimitiveIndex) -> Option<Subject> {
    let index = primitive.to_usize();
    let part = content().parts.iter().find(|part| part.primitives.contains(&index))?;
    match part.kind {
        PartKind::Bunny => Some(Subject::Bunny),
        PartKind::Hull(craft) | PartKind::Engine(craft) => Some(Subject::Craft(craft)),
        PartKind::Band(_) => Some(Subject::Planet),
        PartKind::Terrain => None,
    }
}

/// Oracle: the composition stated as pixels. Of a 320 by 180 image, the camera ray through the
/// centre of each pixel first meets the bunny in at least 900 pixels, each of the three craft in
/// at least 40, and the planet in at least 800. (When recorded: 1 209 bunny, 337 skiff, 64
/// kite, 113 tug and 1 081 planet pixels.)
#[test]
fn the_camera_sees_the_bunny_the_craft_and_the_planet() {
    let scene = scene();
    let (width, height) = (320, 180);
    let camera = scene.camera().camera(width, height).expect("camera");
    let half = FixedQ32_32::from_raw(1 << 31);
    let mut seen = [0_u32; 5];
    for y in 0..height {
        for x in 0..width {
            let ray = camera.ray(PixelSample { x, y, sub_x: half, sub_y: half }).expect("a ray");
            let hit = scene.closest(&ray).expect("traces");
            let slot = match hit.and_then(|hit| subject(hit.primitive)) {
                Some(Subject::Bunny) => 0,
                Some(Subject::Craft(Craft::Skiff)) => 1,
                Some(Subject::Craft(Craft::Kite)) => 2,
                Some(Subject::Craft(Craft::Tug)) => 3,
                Some(Subject::Planet) => 4,
                None => continue,
            };
            seen[slot] += 1;
        }
    }
    let [bunny, skiff, kite, tug, planet] = seen;
    assert!(bunny >= 900, "{seen:?}");
    assert!(skiff >= 40 && kite >= 40 && tug >= 40, "{seen:?}");
    assert!(planet >= 800, "{seen:?}");
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

/// Answers with the scene after checking each answer against brute force, counting what it saw.
struct Checked<'a> {
    scene: &'a Scene,
    hits: Cell<u32>,
    misses: Cell<u32>,
    shadowed: Cell<u32>,
    lit: Cell<u32>,
    subjects: Cell<u32>,
}

impl Tracer for Checked<'_> {
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
        let fast = self.scene.closest(ray)?;
        assert_eq!(fast, brute_closest(self.scene, ray), "closest hit of {ray:?}");
        match fast {
            Some(hit) => {
                self.hits.set(self.hits.get() + 1);
                if subject(hit.primitive).is_some() {
                    self.subjects.set(self.subjects.get() + 1);
                }
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

/// Oracle: a test of every one of the 566 005 triangles (the definition of the nearest hit and
/// of any hit), for every camera, shadow and bounce ray of every sample of a 48 by 27 image at
/// 1 sample: 1 296 samples.
///
/// Size class: large. Each brute-force query reads every triangle, so this runs only in the
/// release profile, where it takes about 10 seconds; the test profile skips it (CI runs both).
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "large: a brute-force query reads all 566 005 triangles; runs in the release profile"
)]
fn every_traced_ray_of_the_scene_equals_a_test_of_every_triangle() {
    let scene = scene();
    let (width, height) = (48, 27);
    let camera = scene.camera().camera(width, height).expect("camera");
    let zero = || Cell::new(0);
    let checked = Checked {
        scene,
        hits: zero(),
        misses: zero(),
        shadowed: zero(),
        lit: zero(),
        subjects: zero(),
    };
    for y in 0..height {
        for x in 0..width {
            let sampler = settings(1, 0).sampler(x, y);
            let path = trace_path(scene, &checked, &camera, sampler, 0).expect("traces");
            let direct = trace_path(scene, scene, &camera, sampler, 0).expect("traces");
            assert_eq!(path, direct);
        }
    }
    let counts = [&checked.hits, &checked.misses, &checked.shadowed, &checked.lit];
    let [hits, misses, shadowed, lit] = counts.map(Cell::get);
    // Meaningful only if every kind of answer occurred, and some on the bunny, craft or planet.
    assert!(
        hits > 400 && misses > 100 && shadowed > 50 && lit > 300,
        "{hits} {misses} {shadowed} {lit}"
    );
    assert!(checked.subjects.get() > 10, "{} subject hits", checked.subjects.get());
}

/// The golden image and content identity, under the rules of `tests/golden/mod.rs`.
#[test]
fn the_canyon_image_has_its_golden_hash() {
    assert_eq!(lumen::CANYON_SCENE_ID, "lumen-canyon-scene/1");
    assert_eq!(lumen::SMOOTH_INTEGRATOR_ID, "lumen-smooth-diffuse-path/1");
    assert_eq!(BUNNY_REV, GOLDEN_BUNNY_REV);
    let scene = scene();
    assert_eq!(scene.content_id().to_string(), GOLDEN_CANYON_CONTENT);
    let size = ImageSize::new(96, 54).expect("size");
    let rendered = render(scene, size, settings(4, 0)).expect("renders");
    assert_eq!(rendered.image.hash(), GOLDEN_CANYON_96X54_4SPP);
}
