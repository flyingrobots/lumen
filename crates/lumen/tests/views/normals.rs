//! The `lit`, `normals` and `shading-normals` views.

use bunny_linalg::FixedUnitVec3;
use lumen::reference::{reference_lighting, reference_materials};
use lumen::{
    render_scheduled, render_view, BuiltinScene, ProvenanceValue, RenderProvenance, Scene,
    SceneSpec, View, BUNNY_REV,
};

use super::golden::{GOLDEN_96X54_4SPP, GOLDEN_BUNNY_REV};
use super::goldens::GOLDEN_VIEWS_96X54;
use super::{at, camera_on_axis, one_sample, rgba, schedule, size};

/// One triangle in the plane z = -4, counter-clockwise seen from +z (its geometric normal is
/// +z), seen by a camera at `camera_z` looking along `look_z`, with `normals` as its vertex
/// normals.
fn one_triangle(camera_z: i64, look_z: i64, normals: &[[Option<FixedUnitVec3>; 3]]) -> Scene {
    let materials = reference_materials().expect("materials");
    Scene::new(&SceneSpec {
        triangles: &[[at(-4, -1, -4), at(4, -1, -4), at(0, 3, -4)]],
        material_of: &[0],
        materials: &materials,
        normals,
        lighting: reference_lighting().expect("lighting"),
        camera: camera_on_axis(camera_z, look_z),
    })
    .expect("builds")
}

/// Oracle: the mapping of ADR 0010 by hand. A 9 by 9 image of the triangle from 8 units in
/// front: the centre pixel's ray hits it at (0, 0, -4), the corner pixel's passes 7 units off
/// the axis at the plane and misses. Front face, normal +z: `(128 + 0, 128 + 0, 128 + 127)`.
/// From behind (a camera at z = -12 looking +z), the same geometric normal on the back face:
/// `(128, 128, 128 + 63)`. A miss is black.
#[test]
fn normals_colour_the_geometric_normal_and_dim_back_faces() {
    let front = render_view(
        &one_triangle(4, -1, &[]),
        View::Normals,
        size(9, 9),
        one_sample(),
        schedule(1, 1),
    )
    .expect("renders");
    assert_eq!(rgba(&front.image, 4, 4), [128, 128, 255, 255], "front face, +z");
    assert_eq!(rgba(&front.image, 0, 0), [0, 0, 0, 255], "a miss");
    let back = render_view(
        &one_triangle(-12, 1, &[]),
        View::Normals,
        size(9, 9),
        one_sample(),
        schedule(1, 1),
    )
    .expect("renders");
    assert_eq!(rgba(&back.image, 4, 4), [128, 128, 191, 255], "back face, +z");
    assert_eq!(front.statistics.outcomes.camera(), 81, "one ray per pixel");
}

/// Oracle: Bunny's `smooth_normal` returns a normal shared by every vertex unchanged, so a
/// triangle whose three vertex normals are +x shows `(128 + 127, 128, 128)` where `normals`
/// shows its geometric +z. With one vertex normal missing, Bunny's smooth shading uses the
/// geometric normal, and so does the view.
#[test]
fn shading_normals_colour_the_interpolated_normal() {
    let x = Some(FixedUnitVec3::UNIT_X);
    let smooth = one_triangle(4, -1, &[[x, x, x]]);
    let view = |scene: &Scene, view| {
        let image = render_view(scene, view, size(9, 9), one_sample(), schedule(2, 1))
            .expect("renders")
            .image;
        rgba(&image, 4, 4)
    };
    assert_eq!(view(&smooth, View::ShadingNormals), [255, 128, 128, 255], "interpolated +x");
    assert_eq!(view(&smooth, View::Normals), [128, 128, 255, 255], "geometric +z");
    let partial = one_triangle(4, -1, &[[x, None, x]]);
    assert_eq!(view(&partial, View::ShadingNormals), [128, 128, 255, 255], "falls back");
}

/// Oracle: the golden hash and the lit render. `lit` through `render_view` is `render_scheduled`,
/// and its provenance is the lit provenance, field for field.
#[test]
fn lit_is_the_lit_render_and_provenance() {
    let scene = BuiltinScene::Reference.build().expect("builds");
    let settings = lumen::RenderSettings::new(4, 0).expect("settings");
    let lit =
        render_view(&scene, View::Lit, size(96, 54), settings, schedule(4, 2)).expect("renders");
    assert_eq!(lit.image.hash(), GOLDEN_96X54_4SPP);
    assert_eq!(
        lit,
        render_scheduled(&scene, size(96, 54), settings, schedule(1, 1)).expect("renders")
    );
    let plain = RenderProvenance::named("lumen-reference-scene/3", &scene, size(96, 54), settings);
    let lit = plain.with_view(View::Lit).fields();
    assert_eq!(lit, plain.fields());
    // The lit values themselves, not only lit against itself: the integrator, its settings.
    assert_eq!(lit[1], ("integrator", ProvenanceValue::Identity("lumen-diffuse-path/1")));
    assert_eq!(lit[9], ("samples", ProvenanceValue::Number(4)));
    assert_eq!(lit[11], ("bounces", ProvenanceValue::Number(1)));
}

/// Oracle: ADR 0010's provenance rule. A debug view replaces `integrator` with its identity and
/// `shading` with the contracts it uses, records one sample and no bounce, and keeps every
/// other field.
#[test]
fn a_view_is_its_own_integrator_in_the_provenance() {
    let scene = BuiltinScene::FixedPoint.build().expect("builds");
    let settings = lumen::RenderSettings::new(1, 9).expect("settings");
    let plain = RenderProvenance::named("lumen-fixed-point-scene/2", &scene, size(8, 4), settings);
    for (view, id, shading) in [
        (View::Normals, "lumen-view-normals/1", "none"),
        (View::ShadingNormals, "lumen-view-shading-normals/1", "bunny-smooth-normal/1"),
    ] {
        let fields = plain.with_view(view).fields();
        assert_eq!(fields[1], ("integrator", ProvenanceValue::Identity(id)));
        assert_eq!(fields[2], ("shading", ProvenanceValue::Identity(shading)));
        assert_eq!(fields[9], ("samples", ProvenanceValue::Number(1)));
        assert_eq!(fields[11], ("bounces", ProvenanceValue::Number(0)));
        let same = [0, 3, 4, 5, 6, 7, 8, 10, 12];
        for i in same {
            assert_eq!(fields[i], plain.fields()[i], "{view}: field {i}");
        }
    }
}

/// Oracle: the golden hashes, and the one-thread render. Each view of each built-in scene at 96
/// by 54 has its golden hash, the same at 1, 2, 4 and 18 threads and band heights 1 and 3.
#[test]
fn every_view_has_its_golden_hash_at_every_schedule() {
    assert_eq!(BUNNY_REV, GOLDEN_BUNNY_REV);
    for (builtin, view, golden) in GOLDEN_VIEWS_96X54 {
        let scene = builtin.build().expect("builds");
        for threads in [1, 2, 4, 18] {
            for rows in [1, 3] {
                let at = format!("{} {view}, {threads} threads, bands of {rows}", builtin.name());
                let render =
                    render_view(&scene, view, size(96, 54), one_sample(), schedule(threads, rows))
                        .expect(&at);
                assert_eq!(render.image.hash(), golden, "{at}");
            }
        }
    }
}
