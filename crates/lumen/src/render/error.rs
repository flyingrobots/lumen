//! Why a render failed: [`RenderError`].

use core::fmt;

use bunny_optics::{CameraError, PixelError};

use crate::image::SizeError;
use crate::scene::{SceneError, TraceError};

/// Why a render failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderError {
    /// The image size was refused.
    Size(SizeError),
    /// The scene could not be built.
    Scene(SceneError),
    /// Bunny refused the camera at the image size. [`Scene::new`] has already checked the
    /// placement, so for a built scene this is a size Bunny's camera does not accept.
    Camera(CameraError),
    /// The camera gave no ray for a pixel sample inside the image.
    NoRay {
        /// The pixel column.
        x: u32,
        /// The pixel row.
        y: u32,
    },
    /// A ray could not be traced.
    Trace(TraceError),
    /// Bunny's pixel path refused a value.
    Pixel(PixelError),
    /// A shading step refused its input: a hit on a triangle without a surface, a start point
    /// outside the trace domain, a sampler point outside `[0, 1)` or a radiance sum that is not
    /// representable. None of these happens for a scene built by `Scene::new` inside the trace
    /// domain.
    Shading,
    /// Memory for the image could not be reserved.
    OutOfMemory,
    /// The operating system refused to start a worker thread.
    Spawn,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Size(error) => write!(f, "invalid image size: {error}"),
            Self::Scene(error) => write!(f, "invalid scene: {error}"),
            Self::Camera(error) => write!(f, "invalid camera: {error}"),
            Self::NoRay { x, y } => write!(f, "the camera gave no ray for pixel ({x}, {y})"),
            Self::Trace(error) => write!(f, "{error}"),
            Self::Pixel(error) => write!(f, "pixel value refused: {error}"),
            Self::Shading => f.write_str("a shading step refused its input"),
            Self::OutOfMemory => f.write_str("not enough memory for the image"),
            Self::Spawn => f.write_str("a worker thread could not be started"),
        }
    }
}

impl std::error::Error for RenderError {}

impl From<SizeError> for RenderError {
    fn from(error: SizeError) -> Self {
        Self::Size(error)
    }
}

impl From<SceneError> for RenderError {
    fn from(error: SceneError) -> Self {
        Self::Scene(error)
    }
}

impl From<CameraError> for RenderError {
    fn from(error: CameraError) -> Self {
        Self::Camera(error)
    }
}

impl From<TraceError> for RenderError {
    fn from(error: TraceError) -> Self {
        Self::Trace(error)
    }
}

impl From<PixelError> for RenderError {
    fn from(error: PixelError) -> Self {
        Self::Pixel(error)
    }
}
