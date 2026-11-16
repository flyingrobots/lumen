//! What a render's rays did: [`RayOutcomes`] per path, [`RayStatistics`] per render, and the
//! checks a render's statistics must pass ([`RayStatistics::check`], [`RayCheckError`]).
//!
//! Every count is an integer add of a fact the path already computed (a query's answer, a
//! bounce Bunny rejected), after the fact; nothing a path does reads one (ADR 0008).

use core::fmt;
use core::ops::Add;

use super::{RayCount, RenderSettings};
use crate::image::ImageSize;

/// What the rays of one path, or of many paths added together, found.
///
/// A path traces one camera ray; at a hit, a shadow ray if the light has one and a bounce ray
/// unless Bunny's smooth shading rejects the bounce; at the bounce's hit, another shadow ray. So
/// a path counts exactly one camera hit or miss, at most one bounce hit, miss or rejection, and at
/// most two shadow results.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct RayOutcomes {
    /// Camera rays that hit a triangle.
    pub camera_hits: u64,
    /// Camera rays that left the scene.
    pub camera_misses: u64,
    /// Bounce rays that hit a triangle.
    pub bounce_hits: u64,
    /// Bounce rays that left the scene.
    pub bounce_misses: u64,
    /// Bounces Bunny's smooth shading rejected as below the surface: no ray was traced.
    pub rejected_bounces: u64,
    /// Shadow rays that met a triangle before `SHADOW_T_MAX`: the light was blocked.
    pub shadow_occluded: u64,
    /// Shadow rays that met nothing: the light was visible.
    pub shadow_unoccluded: u64,
}

impl RayOutcomes {
    /// Camera rays traced: hits and misses.
    #[must_use]
    pub const fn camera(&self) -> u64 {
        self.camera_hits.saturating_add(self.camera_misses)
    }

    /// Bounce rays traced: hits and misses. A rejected bounce traced none.
    #[must_use]
    pub const fn bounce(&self) -> u64 {
        self.bounce_hits.saturating_add(self.bounce_misses)
    }

    /// Shadow rays traced: occluded and unoccluded.
    #[must_use]
    pub const fn shadow(&self) -> u64 {
        self.shadow_occluded.saturating_add(self.shadow_unoccluded)
    }

    /// The same rays as a [`RayCount`]: camera and bounce rays are its closest-hit queries,
    /// shadow rays its any-hit queries.
    #[must_use]
    pub const fn count(&self) -> RayCount {
        RayCount { closest: self.camera().saturating_add(self.bounce()), shadow: self.shadow() }
    }
}

impl Add for RayOutcomes {
    type Output = Self;

    /// Field by field. A render has at most 2^39 rays (see [`RayCount`]), so no sum saturates.
    fn add(self, other: Self) -> Self {
        Self {
            camera_hits: self.camera_hits.saturating_add(other.camera_hits),
            camera_misses: self.camera_misses.saturating_add(other.camera_misses),
            bounce_hits: self.bounce_hits.saturating_add(other.bounce_hits),
            bounce_misses: self.bounce_misses.saturating_add(other.bounce_misses),
            rejected_bounces: self.rejected_bounces.saturating_add(other.rejected_bounces),
            shadow_occluded: self.shadow_occluded.saturating_add(other.shadow_occluded),
            shadow_unoccluded: self.shadow_unoccluded.saturating_add(other.shadow_unoccluded),
        }
    }
}

/// What a render's rays found, in all and per pixel.
///
/// A function of the scene, the size and the settings alone: the same at every thread count and
/// band height, since each pixel's record is combined with the others by sums, a minimum and a
/// maximum, none of which depends on the order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RayStatistics {
    /// Every path's outcomes, added.
    pub outcomes: RayOutcomes,
    /// The pixels rendered.
    pub pixels: u64,
    /// The samples traced, over every pixel: one path each.
    pub samples: u64,
    /// The fewest rays any one pixel traced; `u64::MAX` when no pixel was rendered.
    pub min_pixel_rays: u64,
    /// The most rays any one pixel traced; zero when no pixel was rendered.
    pub max_pixel_rays: u64,
}

impl RayStatistics {
    /// No pixel: the start of a sum.
    pub const NONE: Self = Self {
        outcomes: RayOutcomes {
            camera_hits: 0,
            camera_misses: 0,
            bounce_hits: 0,
            bounce_misses: 0,
            rejected_bounces: 0,
            shadow_occluded: 0,
            shadow_unoccluded: 0,
        },
        pixels: 0,
        samples: 0,
        min_pixel_rays: u64::MAX,
        max_pixel_rays: 0,
    };

    /// One pixel of `samples` paths whose rays found `outcomes`.
    pub(crate) fn pixel(outcomes: RayOutcomes, samples: u32) -> Self {
        let rays = outcomes.count().total();
        Self {
            outcomes,
            pixels: 1,
            samples: u64::from(samples),
            min_pixel_rays: rays,
            max_pixel_rays: rays,
        }
    }

    /// The rays as a [`RayCount`].
    #[must_use]
    pub const fn count(&self) -> RayCount {
        self.outcomes.count()
    }

    /// Checks what every render of `size` with `settings` must satisfy: one pixel per pixel of
    /// the image, `samples_per_pixel` samples per pixel, exactly one camera ray per sample, and at
    /// most four rays per sample (a camera ray, a bounce ray and a shadow ray at each of their
    /// hits).
    ///
    /// # Errors
    ///
    /// The first [`RayCheckError`] in that order.
    pub fn check(&self, size: ImageSize, settings: RenderSettings) -> Result<(), RayCheckError> {
        let pixels = u64::from(size.width()).saturating_mul(u64::from(size.height()));
        if self.pixels != pixels {
            return Err(RayCheckError::Pixels { expected: pixels, observed: self.pixels });
        }
        let samples = pixels.saturating_mul(u64::from(settings.samples_per_pixel()));
        if self.samples != samples {
            return Err(RayCheckError::Samples { expected: samples, observed: self.samples });
        }
        let camera = self.outcomes.camera();
        if camera != samples {
            return Err(RayCheckError::CameraRays { expected: samples, observed: camera });
        }
        let total = self.count().total();
        let limit = samples.saturating_mul(4);
        if total > limit {
            return Err(RayCheckError::TooManyRays { limit, observed: total });
        }
        Ok(())
    }
}

impl Add for RayStatistics {
    type Output = Self;

    /// Sums, and the minimum and maximum per pixel: associative and commutative, with
    /// [`RayStatistics::NONE`] as the zero.
    fn add(self, other: Self) -> Self {
        Self {
            outcomes: self.outcomes + other.outcomes,
            pixels: self.pixels.saturating_add(other.pixels),
            samples: self.samples.saturating_add(other.samples),
            min_pixel_rays: self.min_pixel_rays.min(other.min_pixel_rays),
            max_pixel_rays: self.max_pixel_rays.max(other.max_pixel_rays),
        }
    }
}

/// A render's [`RayStatistics`] contradict its size and settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RayCheckError {
    /// Not one record per pixel.
    Pixels {
        /// Width times height.
        expected: u64,
        /// The pixels counted.
        observed: u64,
    },
    /// Not `samples_per_pixel` samples per pixel.
    Samples {
        /// Pixels times samples per pixel.
        expected: u64,
        /// The samples counted.
        observed: u64,
    },
    /// Not exactly one camera ray per sample.
    CameraRays {
        /// The samples.
        expected: u64,
        /// The camera rays counted.
        observed: u64,
    },
    /// More than four rays per sample.
    TooManyRays {
        /// Four times the samples.
        limit: u64,
        /// The rays counted.
        observed: u64,
    },
}

impl fmt::Display for RayCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pixels { expected, observed } => {
                write!(f, "{observed} pixels counted, expected {expected}")
            }
            Self::Samples { expected, observed } => {
                write!(f, "{observed} samples counted, expected {expected}")
            }
            Self::CameraRays { expected, observed } => {
                write!(f, "{observed} camera rays counted, expected one per sample, {expected}")
            }
            Self::TooManyRays { limit, observed } => {
                write!(f, "{observed} rays counted, more than four per sample, {limit}")
            }
        }
    }
}

impl std::error::Error for RayCheckError {}
