//! What a render is: the identity of every contract that decides its pixels, and the size and
//! settings it was made at.
//!
//! This module owns [`RenderProvenance`]. It holds facts about the image only. How the image was
//! computed on this run (the thread count, the band height, the wall-clock times) is not provenance:
//! it never changes a pixel, so a front end reports it separately.

use bunny_optics::{RGBA8_OPAQUE_IDENTITY, SAMPLER_ID, SHADING_ID};

use crate::image::ImageSize;
use crate::render::{RenderSettings, INTEGRATOR_ID, MAX_BOUNCES, TONE_CURVE};
use crate::BUNNY_REV;

/// The number of fields of a [`RenderProvenance`], the length of [`RenderProvenance::fields`].
pub const PROVENANCE_FIELDS: usize = 12;

/// The record of what was rendered: the scene, the contracts that decide every pixel, the Bunny
/// commit they come from, the image size and the settings.
///
/// Two renders with equal provenance have the same RGBA8 bytes, and so the same image hash, on
/// every machine, at every thread count and band height. It is known before the render runs, so a
/// front end can record it without rendering. It is not the render's result: the image, its
/// hash and the ray count are in [`Render`](crate::Render) and
/// [`Rgba8Image::hash`](crate::Rgba8Image::hash).
///
/// Every identity except the scene's is a constant of this build of lumen. The scene's is given
/// by the caller, who names the scene it renders (lumen's scenes do not carry their identity
/// yet), for example [`REFERENCE_SCENE_ID`](crate::REFERENCE_SCENE_ID) for
/// [`reference_scene`](crate::reference_scene).
///
/// # Examples
///
/// ```
/// use lumen::{ImageSize, ProvenanceValue, RenderProvenance, RenderSettings, REFERENCE_SCENE_ID};
///
/// let size = ImageSize::new(16, 9)?;
/// let settings = RenderSettings::new(4, 7)?;
/// let provenance = RenderProvenance::new(REFERENCE_SCENE_ID, size, settings);
/// let fields = provenance.fields();
/// assert_eq!(fields[0], ("scene", ProvenanceValue::Identity("lumen-reference-scene/3")));
/// assert_eq!(fields[7], ("size", ProvenanceValue::Size(size)));
/// assert_eq!(fields[8], ("pixels", ProvenanceValue::Number(144)));
/// assert_eq!(fields[10], ("seed", ProvenanceValue::Number(7)));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderProvenance {
    scene: &'static str,
    size: ImageSize,
    settings: RenderSettings,
}

/// The value of one field of a [`RenderProvenance`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProvenanceValue {
    /// A stable name and version, such as `lumen-diffuse-path/1`, or a Bunny commit.
    Identity(&'static str),
    /// An image size.
    Size(ImageSize),
    /// A whole number: a count or a seed.
    Number(u64),
}

impl RenderProvenance {
    /// The provenance of a render of the scene named `scene` at `size` with `settings`.
    ///
    /// This does not allocate, block or render.
    #[must_use]
    pub const fn new(scene: &'static str, size: ImageSize, settings: RenderSettings) -> Self {
        Self { scene, size, settings }
    }

    /// The scene's identity, as given to [`RenderProvenance::new`].
    #[must_use]
    pub const fn scene(&self) -> &'static str {
        self.scene
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
    /// 1. `scene`: the scene's identity;
    /// 2. `integrator`: [`INTEGRATOR_ID`](crate::INTEGRATOR_ID);
    /// 3. `shading`: Bunny's shading contract, `bunny_optics::SHADING_ID`;
    /// 4. `sampler`: Bunny's sampler, `bunny_optics::SAMPLER_ID`;
    /// 5. `tone`: the tone curve of [`TONE_CURVE`](crate::TONE_CURVE);
    /// 6. `encoding`: the pixel encoding, `bunny_optics::RGBA8_OPAQUE_IDENTITY`;
    /// 7. `bunny`: the Bunny commit, [`BUNNY_REV`](crate::BUNNY_REV);
    /// 8. `size`: the image size;
    /// 9. `pixels`: width times height;
    /// 10. `samples`: the samples per pixel;
    /// 11. `seed`: the seed;
    /// 12. `bounces`: the most bounces a path takes, [`MAX_BOUNCES`](crate::MAX_BOUNCES).
    #[must_use]
    pub fn fields(&self) -> [(&'static str, ProvenanceValue); PROVENANCE_FIELDS] {
        use ProvenanceValue::{Identity, Number, Size};
        let pixels = u64::from(self.size.width()) * u64::from(self.size.height());
        [
            ("scene", Identity(self.scene)),
            ("integrator", Identity(INTEGRATOR_ID)),
            ("shading", Identity(SHADING_ID)),
            ("sampler", Identity(SAMPLER_ID)),
            ("tone", Identity(TONE_CURVE.identity())),
            ("encoding", Identity(RGBA8_OPAQUE_IDENTITY)),
            ("bunny", Identity(BUNNY_REV)),
            ("size", Size(self.size)),
            ("pixels", Number(pixels)),
            ("samples", Number(u64::from(self.settings.samples_per_pixel()))),
            ("seed", Number(u64::from(self.settings.seed()))),
            ("bounces", Number(u64::from(MAX_BOUNCES))),
        ]
    }
}
