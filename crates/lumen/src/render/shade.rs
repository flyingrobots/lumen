//! One hit, shaded: Bunny's surface hit, its light sample and its bounce, and the reflection of
//! the triangle's albedo ([`Shade`], [`direct`]).

use bunny_num::FixedQ32_32;
use bunny_optics::{Albedo, SurfaceHit, SHADOW_T_MAX};
use bunny_query::TraceRay;

use super::{PathError, RayCount, ZERO};
use crate::scene::{Hit, PrimitiveIndex, Scene, Tracer};

/// A hit the tracer reported, ready to shade: Bunny's `SurfaceHit` and the triangle's albedo.
///
/// Each step calls Bunny's fast `Option` form, and only after a `None` its `try_` form, for the
/// reason: carrying a reason through every hit costs time (Bunny measured 4 % per hit), and a
/// refusal is never on the path of an image that renders.
pub(super) struct Shade<'a> {
    primitive: PrimitiveIndex,
    pub(super) hit: SurfaceHit,
    albedo: &'a Albedo,
}

impl<'a> Shade<'a> {
    /// The surface hit and albedo of `hit`, the tracer's answer for `ray`.
    ///
    /// `Scene::new` checked every material index, so `Scene::albedo` is `None` only past the
    /// last triangle, and then `Scene::surface` is `None` only for a zero-area triangle.
    pub(super) fn at(scene: &'a Scene, ray: &TraceRay, hit: &Hit) -> Result<Self, PathError> {
        let primitive = hit.primitive;
        let Some(albedo) = scene.albedo(primitive) else {
            let triangles = scene.triangles().len();
            return Err(PathError::UnknownPrimitive { primitive, triangles });
        };
        let surface = scene.surface(primitive).ok_or(PathError::NoSurface { primitive })?;
        let hit = match SurfaceHit::new(surface, ray, &hit.hit) {
            Some(hit) => hit,
            None => SurfaceHit::try_new(surface, ray, &hit.hit)
                .map_err(|error| PathError::SurfaceHit { primitive, error })?,
        };
        Ok(Self { primitive, hit, albedo })
    }

    /// Bunny's bounce ray for the unit-square point `sample`.
    pub(super) fn bounce(&self, sample: [FixedQ32_32; 2]) -> Result<TraceRay, PathError> {
        match self.hit.bounce_ray(sample) {
            Some(ray) => Ok(ray),
            None => self
                .hit
                .try_bounce_ray(sample)
                .map_err(|error| PathError::BounceRay { primitive: self.primitive, error }),
        }
    }

    /// Bunny's `Albedo::reflect` of `direct` and `indirect`.
    pub(super) fn reflect(
        &self,
        direct: [FixedQ32_32; 3],
        indirect: [FixedQ32_32; 3],
    ) -> Result<[FixedQ32_32; 3], PathError> {
        match self.albedo.reflect(direct, indirect) {
            Some(radiance) => Ok(radiance),
            None => {
                Err(PathError::RadianceOverflow { primitive: self.primitive, direct, indirect })
            }
        }
    }
}

/// The direct term at a surface hit: Bunny's light sample, made visible by an any-hit query of
/// its shadow ray from the start point with `SHADOW_T_MAX`. Zero without a sun.
pub(super) fn direct<T: Tracer + ?Sized>(
    scene: &Scene,
    tracer: &T,
    hit: &SurfaceHit,
    rays: &mut RayCount,
) -> Result<[FixedQ32_32; 3], PathError> {
    let Some(sun) = scene.lighting().sun else { return Ok(ZERO) };
    let light = hit.light_sample(&sun);
    let visible = match light.shadow_ray() {
        Some(shadow) => {
            rays.shadow = rays.shadow.saturating_add(1);
            !tracer.occluded(shadow, SHADOW_T_MAX)?
        }
        None => false,
    };
    Ok(light.radiance(visible))
}
