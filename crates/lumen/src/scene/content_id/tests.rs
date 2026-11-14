//! The content identity's tests. Oracles: the encoding of ADR 0005 (`lumen-scene-content/2`) built
//! by hand and hashed with
//! the crate's SHA-256 (itself checked against FIPS 180-4 vectors); inequality of digests for
//! inputs that differ; and, for a triangle no ray reaches, a test of every triangle on every ray.

use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

use bunny_linalg::FixedUnitVec3;
use bunny_optics::{Albedo, DirectionalLight, SkyLight};
use bunny_query::TraceRay;

use super::*;
use crate::reference::{per_256, reference_camera, reference_lighting};
use crate::scene::{
    CameraPlacement, Hit, Lighting, PrimitiveIndex, Scene, TraceError, Tracer, FAR,
};
use crate::vertex_grid::{at, grid, U};
use crate::{render_traced, ImageSize, RenderSettings, Schedule};

/// A scene's input, owned, so a test can change one field.
#[derive(Clone)]
struct Input {
    triangles: Vec<[FixedVec3; 3]>,
    material_of: Vec<u32>,
    materials: Vec<Albedo>,
    normals: Vec<[Option<FixedUnitVec3>; 3]>,
    lighting: Lighting,
    camera: CameraPlacement,
}

impl Input {
    fn spec(&self) -> SceneSpec<'_> {
        SceneSpec {
            triangles: &self.triangles,
            material_of: &self.material_of,
            materials: &self.materials,
            normals: &self.normals,
            lighting: self.lighting,
            camera: self.camera,
        }
    }

    fn build(&self) -> Scene {
        Scene::new(&self.spec()).expect("the scene builds")
    }

    fn id(&self) -> SceneContentId {
        self.build().content_id()
    }
}

fn albedo(r: i64, g: i64, b: i64) -> Albedo {
    Albedo::new([per_256(r), per_256(g), per_256(b)]).expect("valid albedo")
}

/// Two triangles, two materials, the reference sun and sky and the reference camera.
fn small() -> Input {
    Input {
        triangles: vec![
            [at(-U, -U, 0), at(U, -U, 0), at(0, U, 0)],
            [at(-U, 0, -U), at(U, 0, -U), at(0, 0, U)],
        ],
        material_of: vec![1, 0],
        materials: vec![albedo(200, 100, 50), albedo(10, 20, 30)],
        normals: Vec::new(),
        lighting: reference_lighting().expect("valid lighting"),
        camera: reference_camera(),
    }
}

/// The encoding of ADR 0005, written out from its table, independently of `content_id`.
fn encoded_by_hand(input: &Input) -> Vec<u8> {
    fn values(bytes: &mut Vec<u8>, values: &[FixedQ32_32]) {
        for value in values {
            bytes.extend(value.raw().to_le_bytes());
        }
    }
    fn vectors(bytes: &mut Vec<u8>, vectors: &[FixedVec3]) {
        for v in vectors {
            values(bytes, &[v.x, v.y, v.z]);
        }
    }
    let count = |n: usize| u64::try_from(n).expect("fits").to_le_bytes();
    let mut bytes = Vec::new();
    bytes.extend(21_u64.to_le_bytes());
    bytes.extend(b"lumen-scene-content/2");
    bytes.extend(count(input.triangles.len()));
    for (triangle, material) in input.triangles.iter().zip(&input.material_of) {
        vectors(&mut bytes, triangle);
        bytes.extend(material.to_le_bytes());
    }
    bytes.extend(count(input.materials.len()));
    for material in &input.materials {
        values(&mut bytes, &material.channels());
    }
    if let Some(sun) = input.lighting.sun {
        bytes.push(1);
        vectors(&mut bytes, &[sun.toward_light()]);
        values(&mut bytes, &sun.irradiance());
    } else {
        bytes.push(0);
    }
    values(&mut bytes, &input.lighting.sky.radiance());
    let camera = input.camera;
    vectors(&mut bytes, &[camera.position, camera.look, camera.up]);
    values(&mut bytes, &[camera.tan_half_fov]);
    bytes.extend(count(input.normals.len()));
    for normal in input.normals.iter().flatten() {
        if let Some(normal) = normal {
            bytes.push(1);
            vectors(&mut bytes, &[normal.into_inner()]);
        } else {
            bytes.push(0);
        }
    }
    bytes
}

/// The unit normal `(0, 3, 4) / 5`, exact in Q32.32 only to a raw unit, as Bunny normalizes it.
fn tilted() -> FixedUnitVec3 {
    FixedUnitVec3::new(at(0, 3 * U, 4 * U)).expect("a unit vector")
}

/// [`small`] with vertex normals: up, none and tilted on the first triangle, none on the second.
fn with_normals() -> Input {
    Input {
        normals: vec![[Some(FixedUnitVec3::UNIT_Y), None, Some(tilted())], [None; 3]],
        ..small()
    }
}

#[test]
fn the_digest_is_the_sha256_of_the_adr_encoding_built_by_hand() {
    let lit = small();
    let unlit = Input { lighting: Lighting { sun: None, ..lit.lighting }, ..lit.clone() };
    let smooth = with_normals();
    // Two of the six vertices have a normal: six tags and two vectors of three `i64`.
    let normal_bytes = 6 + 2 * 24;
    for (input, light_bytes, normals) in
        [(&lit, 1 + 48, 0), (&unlit, 1, 0), (&smooth, 1 + 48, normal_bytes)]
    {
        let bytes = encoded_by_hand(input);
        // Version, triangle count and records, material count and records, light, sky, camera,
        // the normals' count and records.
        let length = 8 + 21 + 8 + 2 * 76 + 8 + 2 * 24 + light_bytes + 24 + 80 + 8 + normals;
        assert_eq!(bytes.len(), length);
        let id = input.id();
        assert_eq!(id.to_bytes(), crate::sha256::digest(&bytes));
        assert_eq!(id.to_string(), format!("sha256:{}", hex(&id.to_bytes())));
    }
}

/// Changes `input` with `change`.
fn with(change: impl FnOnce(&mut Input)) -> Input {
    let mut input = small();
    change(&mut input);
    input
}

#[test]
fn every_single_change_to_the_content_changes_the_digest() {
    let one = |value: &mut FixedQ32_32| *value = FixedQ32_32::from_raw(value.raw() + 1);
    let sun = |[x, y, z]: [i64; 3], irradiance: [i64; 3]| {
        let toward = at(x * U / 16, y * U / 16, z * U / 16);
        Some(DirectionalLight::new(toward, irradiance.map(per_256)).expect("valid light"))
    };
    let sky = SkyLight::new([per_256(64), per_256(85), per_256(121)]).expect("valid sky");
    let mut cases: Vec<(String, Input)> = vec![
        (
            "both triangles swapped with their materials".into(),
            with(|s| {
                s.triangles.swap(0, 1);
                s.material_of.swap(0, 1);
            }),
        ),
        ("the triangles swapped".into(), with(|s| s.triangles.swap(0, 1))),
        ("a material index".into(), with(|s| s.material_of[0] = 0)),
        ("an unused material appended".into(), with(|s| s.materials.push(albedo(1, 2, 3)))),
        (
            "the light's direction".into(),
            with(|s| s.lighting.sun = sun([8, 6, 16], [1024, 970, 900])),
        ),
        (
            "the light's irradiance".into(),
            with(|s| s.lighting.sun = sun([8, 5, 16], [1024, 970, 901])),
        ),
        ("the light removed".into(), with(|s| s.lighting.sun = None)),
        ("the sky".into(), with(|s| s.lighting.sky = sky)),
        ("the camera position".into(), with(|s| one(&mut s.camera.position.x))),
        ("the camera look".into(), with(|s| one(&mut s.camera.look.y))),
        ("the camera up".into(), with(|s| one(&mut s.camera.up.z))),
        ("the camera tangent".into(), with(|s| one(&mut s.camera.tan_half_fov))),
        ("vertex normals, all absent".into(), with(|s| s.normals = vec![[None; 3]; 2])),
        ("vertex normals".into(), with_normals()),
        (
            "a vertex normal moved to another vertex".into(),
            Input {
                normals: vec![[None, Some(FixedUnitVec3::UNIT_Y), Some(tilted())], [None; 3]],
                ..small()
            },
        ),
        (
            "a vertex normal changed".into(),
            Input {
                normals: vec![[Some(FixedUnitVec3::UNIT_Z), None, Some(tilted())], [None; 3]],
                ..small()
            },
        ),
    ];
    for channel in 0..3 {
        let changed = with(|s| {
            let mut channels = s.materials[0].channels();
            one(&mut channels[channel]);
            s.materials[0] = Albedo::new(channels).expect("valid");
        });
        cases.push((format!("albedo channel {channel}"), changed));
    }
    for slot in 0..9 {
        let changed = with(|s| {
            let v = &mut s.triangles[1][slot / 3];
            one(match slot % 3 {
                0 => &mut v.x,
                1 => &mut v.y,
                _ => &mut v.z,
            });
        });
        cases.push((format!("coordinate {slot} of triangle 1"), changed));
    }
    let mut seen = std::collections::BTreeMap::new();
    seen.insert(small().id(), "the unchanged scene".to_owned());
    for (name, input) in cases {
        if let Some(earlier) = seen.insert(input.id(), name.clone()) {
            panic!("{name} has the digest of {earlier}");
        }
    }
    assert_eq!(seen.len(), 1 + 16 + 3 + 9);
}

/// Without the counts, seven all-zero triangles with one all-zero albedo and one all-zero
/// triangle with twenty all-zero albedos are the same 556 bytes (ADR 0005). With them, the two
/// scenes have different digests.
#[test]
fn the_counts_keep_lists_of_different_lengths_apart() {
    let zeros = |triangles: usize, materials: usize| Input {
        triangles: vec![[at(0, 0, 0); 3]; triangles],
        material_of: vec![0; triangles],
        materials: vec![albedo(0, 0, 0); materials],
        ..small()
    };
    let (seven, one) = (zeros(7, 1), zeros(1, 20));
    let lists = |input: &Input| {
        let bytes = encoded_by_hand(input);
        // The triangle and material records without the version, the counts and the tail (the
        // light, the sky, the camera and an empty normals count).
        let tail = 1 + 48 + 24 + 80 + 8;
        let records = 8 + 21 + 8..bytes.len() - tail;
        let mut naive = bytes[records].to_vec();
        naive.drain(76 * input.triangles.len()..76 * input.triangles.len() + 8);
        naive
    };
    assert_eq!(lists(&seven), vec![0; 556]);
    assert_eq!(lists(&seven), lists(&one));
    assert_ne!(seven.id(), one.id());
}

#[test]
fn the_same_spec_gives_the_same_digest_and_the_tree_is_no_input() {
    let input = small();
    let (first, second) = (input.build(), input.build());
    assert_eq!(first.content_id(), second.content_id());
    // The scene's identity is the function of the spec alone that `Scene::new` calls before it
    // builds the tree.
    assert_eq!(first.content_id(), content_id(&input.spec()));
}

/// A closed cube of half-width one around the origin, two triangles per face.
fn cube() -> Vec<[FixedVec3; 3]> {
    let corner = |i: usize| {
        let side = |bit: usize| if i & bit == 0 { -U } else { U };
        at(side(1), side(2), side(4))
    };
    let faces =
        [[0, 2, 6, 4], [1, 3, 7, 5], [0, 1, 5, 4], [2, 3, 7, 6], [0, 1, 3, 2], [4, 5, 7, 6]];
    faces
        .iter()
        .flat_map(|&[a, b, c, d]| [[a, b, c], [a, c, d]])
        .map(|corners| corners.map(corner))
        .collect()
}

/// Answers with the scene after checking that a test of every triangle except `hidden` gives the
/// same answer: then `hidden` decides no query.
struct WithoutHidden<'a> {
    scene: &'a Scene,
    hidden: usize,
    hits: AtomicU32,
    shadow_queries: AtomicU32,
}

impl WithoutHidden<'_> {
    fn others(&self) -> impl Iterator<Item = (usize, &bunny_query::TraceTriangle)> {
        self.scene.triangles().iter().enumerate().filter(|(i, _)| *i != self.hidden)
    }
}

impl Tracer for WithoutHidden<'_> {
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
        let found = self.scene.closest(ray)?;
        let mut best: Option<Hit> = None;
        for (index, triangle) in self.others() {
            if let Some(hit) = ray.hit_triangle(triangle, FixedQ32_32::ZERO, FAR) {
                if best.is_none_or(|b| hit.distance < b.hit.distance) {
                    let primitive = PrimitiveIndex::new(u32::try_from(index).expect("fits"));
                    best = Some(Hit { primitive, hit });
                }
            }
        }
        assert_eq!(found, best, "closest hit of {ray:?}");
        self.hits.fetch_add(u32::from(found.is_some()), Relaxed);
        Ok(found)
    }

    fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError> {
        let found = self.scene.occluded(ray, t_max)?;
        let brute =
            self.others().any(|(_, t)| ray.hit_triangle(t, FixedQ32_32::ZERO, t_max).is_some());
        assert_eq!(found, brute, "any hit of {ray:?}");
        self.shadow_queries.fetch_add(1, Relaxed);
        Ok(found)
    }
}

/// A triangle inside a closed cube, moved: a different scene, the same image. Every ray of both
/// renders is checked against a test of every other triangle, so the hidden one decides nothing.
#[test]
fn a_hidden_triangle_moved_changes_the_digest_and_no_pixel() {
    let inside = [at(-U / 4, -U / 4, 0), at(U / 4, -U / 4, 0), at(0, U / 4, 0)];
    let moved = inside.map(|v| FixedVec3::new(v.x + grid(U / 8), v.y + grid(U / 8), v.z));
    let scene_with = |hidden: [FixedVec3; 3]| {
        let mut triangles = cube();
        triangles.push(hidden);
        Input { material_of: vec![0; triangles.len()], triangles, ..small() }.build()
    };
    let size = ImageSize::new(16, 9).expect("valid size");
    let settings = RenderSettings::new(2, 3).expect("valid settings");
    let render = |scene: &Scene| {
        let tracer = WithoutHidden {
            scene,
            hidden: 12,
            hits: AtomicU32::new(0),
            shadow_queries: AtomicU32::new(0),
        };
        let image = render_traced(scene, &tracer, size, settings, Schedule::single())
            .expect("renders")
            .image;
        assert!(tracer.hits.load(Relaxed) > 0 && tracer.shadow_queries.load(Relaxed) > 0);
        image.hash()
    };
    let (before, after) = (scene_with(inside), scene_with(moved));
    assert_ne!(before.content_id(), after.content_id());
    assert_eq!(render(&before), render(&after));
}
