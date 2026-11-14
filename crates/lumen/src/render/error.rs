//! Why a render failed ([`RenderError`]) and why one path or pixel failed ([`PathError`]).
//!
//! Each `Display` includes the message of the error it wraps, so the text stands alone, and
//! `source()` returns that error, so a program can walk the chain.

use core::fmt;
use std::collections::TryReserveError;

use bunny_num::FixedQ32_32;
use bunny_optics::{CameraError, PixelError, PixelSample, SecondaryRayError};

use crate::image::SizeError;
use crate::scene::{PrimitiveIndex, SceneError, TraceError};
use crate::workers::{Reservation, SpawnError};

/// Why one sample path, or the pixel it belongs to, could not be computed: the error of
/// [`trace_path`](super::trace_path) and [`pixel_radiance`](super::pixel_radiance).
///
/// `NoRay`, `UnknownPrimitive`, `NoSurface` and `SurfaceHit` are reached only through a pixel
/// outside the camera's image or a [`Tracer`](crate::Tracer) that reports a hit the scene did
/// not produce. `BounceRay`, `RadianceOverflow` and `Pixel` are reached by no input: Bunny's
/// sampler gives points in `[0, 1)`, and Bunny's lights and albedos bound every radiance far
/// below the limits. A generated test counts that none of them occurs.
///
/// Non-exhaustive: a later version may report a new failure, so a `match` outside this crate
/// needs a wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PathError {
    /// Bunny's `PinholeCamera::ray` gave no ray for this camera sample: its pixel is outside the
    /// camera's image.
    NoRay {
        /// The sample index within the pixel.
        sample: u32,
        /// The camera sample, with its pixel and sub-pixel offset.
        point: PixelSample,
    },
    /// A ray query failed.
    Trace(TraceError),
    /// The tracer reported a hit on a triangle the scene does not have.
    UnknownPrimitive {
        /// The primitive index reported.
        primitive: PrimitiveIndex,
        /// The scene's triangle count: every valid index is below it.
        triangles: usize,
    },
    /// The tracer reported a hit on a triangle of zero area, which has no surface. Bunny's
    /// triangle test never reports one.
    NoSurface {
        /// The primitive index of the triangle.
        primitive: PrimitiveIndex,
    },
    /// Bunny's `SurfaceHit::try_new` refused the reported hit.
    SurfaceHit {
        /// The primitive index of the triangle hit.
        primitive: PrimitiveIndex,
        /// Bunny's reason.
        error: SecondaryRayError,
    },
    /// Bunny's `SurfaceHit::try_bounce_ray` refused the bounce from a hit.
    BounceRay {
        /// The primitive index of the triangle the bounce leaves.
        primitive: PrimitiveIndex,
        /// Bunny's reason.
        error: SecondaryRayError,
    },
    /// Bunny's `Albedo::reflect` refused a radiance sum, `direct + indirect` in some channel,
    /// as not representable in Q32.32.
    RadianceOverflow {
        /// The primitive index of the reflecting triangle.
        primitive: PrimitiveIndex,
        /// The direct term per channel.
        direct: [FixedQ32_32; 3],
        /// The indirect term per channel.
        indirect: [FixedQ32_32; 3],
    },
    /// Bunny's pixel path (`PixelAccumulator` or `write_rgba8`) refused a value.
    Pixel(PixelError),
}

/// Three Q32.32 values as their raw integers, `(a, b, c)`.
struct Raw3([FixedQ32_32; 3]);

impl fmt::Display for Raw3 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self([a, b, c]) = self;
        write!(f, "({}, {}, {})", a.raw(), b.raw(), c.raw())
    }
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRay { sample, point } => write!(
                f,
                "the camera gave no ray for sample {sample} at raw offset ({}, {}) of pixel \
                 ({}, {})",
                point.sub_x.raw(),
                point.sub_y.raw(),
                point.x,
                point.y,
            ),
            Self::Trace(error) => write!(f, "{error}"),
            Self::UnknownPrimitive { primitive, triangles } => write!(
                f,
                "the tracer reported triangle {primitive}, but the scene has {triangles} \
                 triangles"
            ),
            Self::NoSurface { primitive } => {
                write!(f, "the tracer reported a hit on triangle {primitive}, which has zero area")
            }
            Self::SurfaceHit { primitive, error } => {
                write!(f, "the hit on triangle {primitive} was refused: {error}")
            }
            Self::BounceRay { primitive, error } => {
                write!(f, "the bounce from triangle {primitive} was refused: {error}")
            }
            Self::RadianceOverflow { primitive, direct, indirect } => write!(
                f,
                "triangle {primitive} reflects an unrepresentable radiance: direct raw {} plus \
                 indirect raw {}",
                Raw3(*direct),
                Raw3(*indirect),
            ),
            Self::Pixel(error) => write!(f, "pixel value refused: {error}"),
        }
    }
}

impl std::error::Error for PathError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Trace(error) => Some(error),
            Self::SurfaceHit { error, .. } | Self::BounceRay { error, .. } => Some(error),
            Self::Pixel(error) => Some(error),
            Self::NoRay { .. }
            | Self::UnknownPrimitive { .. }
            | Self::NoSurface { .. }
            | Self::RadianceOverflow { .. } => None,
        }
    }
}

impl From<TraceError> for PathError {
    fn from(error: TraceError) -> Self {
        Self::Trace(error)
    }
}

impl From<PixelError> for PathError {
    fn from(error: PixelError) -> Self {
        Self::Pixel(error)
    }
}

/// Why a render failed.
///
/// Non-exhaustive: a later version may report a new failure, so a `match` outside this crate
/// needs a wildcard arm. Not `Copy`: it can hold the allocator's and the operating system's
/// errors.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RenderError {
    /// The image size was refused.
    Size(SizeError),
    /// The scene could not be built.
    Scene(SceneError),
    /// Bunny refused the camera at the image size. [`Scene::new`](crate::Scene::new) has
    /// already checked the placement, so for a built scene this is a size Bunny's camera does
    /// not accept.
    Camera(CameraError),
    /// The first failing pixel in raster order, and why it failed.
    Pixel {
        /// The pixel column.
        x: u32,
        /// The pixel row.
        y: u32,
        /// Why its path or its value failed.
        error: PathError,
    },
    /// Memory could not be reserved.
    OutOfMemory {
        /// What was being reserved, and how much.
        reserving: Reservation,
        /// The allocator's refusal.
        error: TryReserveError,
    },
    /// The operating system refused to start a worker thread.
    Spawn(SpawnError),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Size(error) => write!(f, "invalid image size: {error}"),
            Self::Scene(error) => write!(f, "invalid scene: {error}"),
            Self::Camera(error) => write!(f, "invalid camera: {error}"),
            Self::Pixel { x, y, error } => write!(f, "pixel ({x}, {y}): {error}"),
            Self::OutOfMemory { reserving, error } => {
                write!(f, "not enough memory for {reserving}: {error}")
            }
            Self::Spawn(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Size(error) => Some(error),
            Self::Scene(error) => Some(error),
            Self::Camera(error) => Some(error),
            Self::Pixel { error, .. } => Some(error),
            Self::OutOfMemory { error, .. } => Some(error),
            Self::Spawn(error) => Some(error),
        }
    }
}

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
