//! What a render is: the scene's content identity, the identity of every contract that decides
//! its pixels, and the size and settings it was made at.
//!
//! This module owns [`RenderProvenance`]. It holds facts about the image only. How the image was
//! computed on this run (the thread count, the band height, the wall-clock times) is not provenance:
//! it never changes a pixel, so a front end reports it separately.

use bunny_optics::{RGBA8_OPAQUE_IDENTITY, SAMPLER_ID};

use crate::image::ImageSize;
use crate::render::{Integrator, RenderSettings, MAX_BOUNCES, TONE_CURVE};
use crate::scene::{Scene, SceneContentId};
use crate::view::View;
use crate::BUNNY_REV;

/// The number of fields of a [`RenderProvenance`], the length of [`RenderProvenance::fields`].
pub const PROVENANCE_FIELDS: usize = 13;

/// The record of what was rendered: the scene's content identity and name, the contracts that
/// decide every pixel, the Bunny commit they come from, the image size and the settings.
///
/// Two scheduled renders with equal provenance have the same RGBA8 bytes, and so the same image
/// hash, at every thread count and band height, on every platform CI checks (x86-64 and aarch64
/// Linux, aarch64 macOS, x86-64 Windows and `wasm32-wasip1`). A scheduled render is
/// [`render`](crate::render), [`render_scheduled`](crate::render_scheduled), or
/// [`render_traced`](crate::render_traced) and
/// [`render_traced_with_run`](crate::render_traced_with_run) given the scene as its own tracer.
/// Provenance does not describe a caller-supplied [`Tracer`](crate::Tracer), so a render through
/// one is outside the promise. It is known before the render runs, so a front end can record it
/// without rendering. It is not the render's result: the image, its hash and the ray count are in
/// [`Render`](crate::Render) and [`Rgba8Image::hash`](crate::Rgba8Image::hash).
///
/// The scene enters as its [`SceneContentId`] ([`Scene::content_id`], ADR 0005), taken from the
/// [`Scene`] itself, so it cannot describe a scene other than the one passed. A name, such as
/// [`REFERENCE_SCENE_ID`](crate::REFERENCE_SCENE_ID), is an optional human-readable label carried
/// beside it ([`RenderProvenance::named`]); lumen does not check it against the content. Every
/// other identity is a constant of this build of lumen.
///
/// # Examples
///
/// ```
/// use lumen::{reference_scene, ImageSize, ProvenanceValue, RenderProvenance, RenderSettings};
/// use lumen::REFERENCE_SCENE_ID;
///
/// let scene = reference_scene()?;
/// let size = ImageSize::new(16, 9)?;
/// let settings = RenderSettings::new(4, 7)?;
/// let provenance = RenderProvenance::named(REFERENCE_SCENE_ID, &scene, size, settings);
/// let fields = provenance.fields();
/// assert_eq!(fields[0], ("scene", ProvenanceValue::Identity("lumen-reference-scene/3")));
/// assert_eq!(fields[7], ("size", ProvenanceValue::Size(size)));
/// assert_eq!(fields[8], ("pixels", ProvenanceValue::Number(144)));
/// assert_eq!(fields[10], ("seed", ProvenanceValue::Number(7)));
/// assert_eq!(fields[12], ("scene-content", ProvenanceValue::Content(scene.content_id())));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderProvenance {
    name: Option<&'static str>,
    content: SceneContentId,
    integrator: Integrator,
    view: View,
    size: ImageSize,
    settings: RenderSettings,
}

/// The value of one field of a [`RenderProvenance`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProvenanceValue {
    /// A stable name and version, such as `lumen-diffuse-path/1`, or a Bunny commit.
    Identity(&'static str),
    /// The `scene` field of a scene given no name.
    Unnamed,
    /// A scene's content identity.
    Content(SceneContentId),
    /// An image size.
    Size(ImageSize),
    /// A whole number: a count or a seed.
    Number(u64),
}

impl RenderProvenance {
    /// The provenance of a render of `scene`, which has no name, at `size` with `settings`.
    ///
    /// This does not allocate, block or render: the scene's content identity was computed when
    /// it was built.
    #[must_use]
    pub const fn new(scene: &Scene, size: ImageSize, settings: RenderSettings) -> Self {
        let integrator = Integrator::for_scene(scene);
        Self {
            name: None,
            content: scene.content_id(),
            integrator,
            view: View::Lit,
            size,
            settings,
        }
    }

    /// The provenance of a render of `scene` under the human-readable `name`, such as
    /// [`BuiltinScene::id`](crate::BuiltinScene::id). The name is a label, not checked against
    /// the scene; the content identity is the scene's own.
    #[must_use]
    pub const fn named(
        name: &'static str,
        scene: &Scene,
        size: ImageSize,
        settings: RenderSettings,
    ) -> Self {
        let integrator = Integrator::for_scene(scene);
        Self {
            name: Some(name),
            content: scene.content_id(),
            integrator,
            view: View::Lit,
            size,
            settings,
        }
    }

    /// The same provenance for a render of `view` instead of the lit image ([`render_view`]).
    /// [`View::Lit`] leaves it unchanged. Any other view replaces the `integrator` field with the
    /// view's identity and the `shading` field with the contracts the view uses, and, for a
    /// single-sample view, records one sample and no bounce (docs/adr/0010-debug-views.md).
    ///
    /// [`render_view`]: crate::render_view
    #[must_use]
    pub const fn with_view(self, view: View) -> Self {
        Self { view, ..self }
    }

    /// The view: [`View::Lit`] unless [`RenderProvenance::with_view`] chose another.
    #[must_use]
    pub const fn view(&self) -> View {
        self.view
    }

    /// The scene's name, if it was given one ([`RenderProvenance::named`]).
    #[must_use]
    pub const fn name(&self) -> Option<&'static str> {
        self.name
    }

    /// The scene's content identity, [`Scene::content_id`].
    #[must_use]
    pub const fn scene_content(&self) -> SceneContentId {
        self.content
    }

    /// The image size.
    #[must_use]
    pub const fn size(&self) -> ImageSize {
        self.size
    }

    /// The samples per pixel and the seed.
    #[must_use]
    pub const fn settings(&self) -> RenderSettings {
        self.settings
    }

    /// Every field, named, in this order, which is stable (a new field is added at the end):
    ///
    /// 1. `scene`: the scene's name, or [`ProvenanceValue::Unnamed`];
    /// 2. `integrator`: for a debug view, its identity ([`View::id`]); otherwise the scene's
    ///    [`Integrator`] ([`Integrator::for_scene`]):
    ///    [`INTEGRATOR_ID`](crate::INTEGRATOR_ID), or
    ///    [`SMOOTH_INTEGRATOR_ID`](crate::SMOOTH_INTEGRATOR_ID) for a scene with vertex normals,
    ///    which also uses Bunny's `bunny-smooth-shading/1` (ADR 0007);
    /// 3. `shading`: for a debug view, the contracts it uses ([`View::shading_id`]); otherwise
    ///    the Bunny shading contracts that integrator uses
    ///    ([`Integrator::shading_id`]): `bunny_optics::SHADING_ID` for a flat scene,
    ///    [`SMOOTH_SHADING_IDS`](crate::SMOOTH_SHADING_IDS) for a smooth one;
    /// 4. `sampler`: Bunny's sampler, `bunny_optics::SAMPLER_ID`;
    /// 5. `tone`: the tone curve of [`TONE_CURVE`](crate::TONE_CURVE);
    /// 6. `encoding`: the pixel encoding, `bunny_optics::RGBA8_OPAQUE_IDENTITY`;
    /// 7. `bunny`: the Bunny commit, [`BUNNY_REV`](crate::BUNNY_REV);
    /// 8. `size`: the image size;
    /// 9. `pixels`: width times height;
    /// 10. `samples`: the samples per pixel (1 for a single-sample view);
    /// 11. `seed`: the seed;
    /// 12. `bounces`: the most bounces a path takes, [`MAX_BOUNCES`](crate::MAX_BOUNCES) (0 for
    ///     a single-sample view);
    /// 13. `scene-content`: the scene's content identity, [`Scene::content_id`].
    #[must_use]
    pub fn fields(&self) -> [(&'static str, ProvenanceValue); PROVENANCE_FIELDS] {
        use ProvenanceValue::{Content, Identity, Number, Size, Unnamed};
        // Two `u32` factors always fit a `u64` product, so this never saturates.
        let pixels = u64::from(self.size.width()).saturating_mul(u64::from(self.size.height()));
        let single = self.view.is_single_sample();
        let samples = if single { 1 } else { self.settings.samples_per_pixel() };
        let bounces = if single { 0 } else { MAX_BOUNCES };
        [
            ("scene", self.name.map_or(Unnamed, Identity)),
            ("integrator", Identity(self.view.id().unwrap_or(self.integrator.id()))),
            ("shading", Identity(self.view.shading_id().unwrap_or(self.integrator.shading_id()))),
            ("sampler", Identity(SAMPLER_ID)),
            ("tone", Identity(TONE_CURVE.identity())),
            ("encoding", Identity(RGBA8_OPAQUE_IDENTITY)),
            ("bunny", Identity(BUNNY_REV)),
            ("size", Size(self.size)),
            ("pixels", Number(pixels)),
            ("samples", Number(u64::from(samples))),
            ("seed", Number(u64::from(self.settings.seed()))),
            ("bounces", Number(u64::from(bounces))),
            ("scene-content", Content(self.content)),
        ]
    }
}
