//! One hit, shaded: Bunny's surface hit, flat or with the triangle's interpolated vertex normal,
//! its light sample and its bounce, and the reflection of the triangle's albedo ([`Shade`],
//! [`direct`]).

use bunny_num::FixedQ32_32;
use bunny_optics::{
    Albedo, LightSample, SecondaryRayError, SmoothBounce, SmoothHit, SurfaceHit, SHADOW_T_MAX,
};
use bunny_query::TraceRay;

use super::{PathError, RayCount, ZERO};
use crate::scene::{Hit, PrimitiveIndex, Scene, Tracer};

/// How a hit is shaded.
enum Shading {
    /// The steps of `bunny-diffuse-shading/1` on the geometric frame: a triangle with no vertex
    /// normal.
    Flat(SurfaceHit),
    /// The steps of `bunny-smooth-shading/1`: a triangle with at least one vertex normal.
    Smooth(SmoothHit),
}

/// A hit the tracer reported, ready to shade: Bunny's `SurfaceHit`, flat or smooth, and the
/// triangle's albedo.
///
/// Each step calls Bunny's fast `Option` form, and only after a `None` its `try_` form, for the
/// reason: carrying a reason through every hit costs time (Bunny measured 4 % per hit), and a
/// refusal is never on the path of an image that renders.
pub(super) struct Shade<'a> {
    primitive: PrimitiveIndex,
    shading: Shading,
    albedo: &'a Albedo,
}

impl<'a> Shade<'a> {
    /// The surface hit and albedo of `hit`, the tracer's answer for `ray`.
    ///
    /// `Scene::new` checked every material index, so `Scene::albedo` is `None` only past the
    /// last triangle, and then `Scene::surface` is `None` only for a zero-area triangle. A
    /// triangle with at least one vertex normal gets Bunny's `SmoothHit::new` of the surface
    /// hit, at the hit's exact barycentric numerators, with its three normals as given (Bunny
    /// falls back to the geometric frame when one is missing); any other triangle is shaded
    /// flat, exactly as by `lumen-diffuse-path/1`.
    pub(super) fn at(scene: &'a Scene, ray: &TraceRay, hit: &Hit) -> Result<Self, PathError> {
        let primitive = hit.primitive;
        let Some(albedo) = scene.albedo(primitive) else {
            let triangles = scene.triangles().len();
            return Err(PathError::UnknownPrimitive { primitive, triangles });
        };
        let surface = scene.surface(primitive).ok_or(PathError::NoSurface { primitive })?;
        let surface_hit = match SurfaceHit::new(surface, ray, &hit.hit) {
            Some(surface_hit) => surface_hit,
            None => SurfaceHit::try_new(surface, ray, &hit.hit)
                .map_err(|error| PathError::SurfaceHit { primitive, error })?,
        };
        let shading = match scene.vertex_normals(primitive) {
            Some(&normals) if normals.iter().any(Option::is_some) => Shading::Smooth(
                SmoothHit::new(surface_hit, hit.hit.barycentric_numerators(), normals),
            ),
            _ => Shading::Flat(surface_hit),
        };
        Ok(Self { primitive, shading, albedo })
    }

    /// Bunny's light sample of `sun` at this hit: `SurfaceHit::light_sample` or
    /// `SmoothHit::light_sample`.
    fn light_sample(&self, sun: &bunny_optics::DirectionalLight) -> LightSample {
        match &self.shading {
            Shading::Flat(hit) => hit.light_sample(sun),
            Shading::Smooth(hit) => hit.light_sample(sun),
        }
    }

    /// Bunny's bounce for the unit-square point `sample`: the ray, or `None` when Bunny's smooth
    /// shading rejects the direction as at or below the geometric surface
    /// (`SmoothBounce::BelowSurface`: no ray is traced and the bounce contributes zero). A flat
    /// hit always has a ray.
    pub(super) fn bounce(&self, sample: [FixedQ32_32; 2]) -> Result<Option<TraceRay>, PathError> {
        let refused = |error| PathError::BounceRay { primitive: self.primitive, error };
        match &self.shading {
            Shading::Flat(hit) => match hit.bounce_ray(sample) {
                Some(ray) => Ok(Some(ray)),
                None => hit.try_bounce_ray(sample).map(Some).map_err(refused),
            },
            Shading::Smooth(hit) => match hit.bounce(sample) {
                Some(SmoothBounce::Ray(ray)) => Ok(Some(ray)),
                Some(SmoothBounce::BelowSurface) => Ok(None),
                // Bunny's `bounce` has no `try_` form; its two refusals are those of the frame's
                // cosine direction and of `TraceRay::new`, named here by Bunny's own variants.
                None => Err(refused(match hit.frame().try_cosine_direction(sample) {
                    Err(error) => error,
                    Ok(direction) => SecondaryRayError::BounceRayRefused {
                        origin: hit.surface_hit().point(),
                        direction,
                    },
                })),
            },
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

/// The direct term at a shaded hit: Bunny's light sample, made visible by an any-hit query of
/// its shadow ray from the start point with `SHADOW_T_MAX`. Zero without a sun.
pub(super) fn direct<T: Tracer + ?Sized>(
    scene: &Scene,
    tracer: &T,
    hit: &Shade<'_>,
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
