//! Scenes: triangles with materials, a directional light and a sky, the acceleration structure
//! that traces rays through them, a camera placement, and the scene's content identity.
//!
//! A [`Scene`] is built once from a [`SceneSpec`]: a list of triangles in primitive order, a
//! material (a Bunny [`Albedo`]) for each, the [`Lighting`] and the placement of a pinhole camera.
//! Building it prepares everything a ray needs, so tracing allocates nothing: Bunny's
//! `TraceTriangle` and `TriangleSurface` per triangle, and the bounding volume hierarchy (see
//! [`Scene::new`]), and the scene's content identity ([`SceneContentId`], ADR 0005). The built-in
//! reference scene is in [`crate::reference`].

use bunny_geom::FixedAabb3;
use bunny_linalg::{FixedUnitVec3, FixedVec3};
use bunny_num::FixedQ32_32;
use bunny_optics::{
    Albedo, CameraError, CameraSpec, DirectionalLight, PinholeCamera, SkyLight, TriangleSurface,
};
use bunny_query::{TraceTriangle, TRACE_COORD_LIMIT_RAW};

use crate::schedule::widen;

mod content_id;
mod error;
mod primitive_index;
mod tracer;
mod tree;

#[cfg(test)]
mod tests;

pub use content_id::{SceneContentId, SCENE_CONTENT_ENCODING};
pub use error::{SceneBuffer, SceneError};
pub use primitive_index::PrimitiveIndex;
pub use tracer::{Hit, TraceError, Tracer, FAR};
use tree::{build_tree, Tree};

/// Where a pinhole camera stands and where it looks. The image size is chosen per render.
///
/// The fields are public, like those of [`SceneSpec`]: a placement is raw input, and
/// [`Scene::new`] is where it is checked. A scene holds only a placement Bunny's camera accepts
/// ([`SceneError::Camera`]), so at render time only the image size can still be refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraPlacement {
    /// The eye point, inside the trace domain.
    pub position: FixedVec3,
    /// The viewing direction, any nonzero length.
    pub look: FixedVec3,
    /// A hint that fixes the roll, not parallel to `look`.
    pub up: FixedVec3,
    /// The tangent of half the vertical field of view.
    pub tan_half_fov: FixedQ32_32,
}

impl CameraPlacement {
    /// The Bunny camera for this placement at an image size.
    ///
    /// # Errors
    ///
    /// Any [`CameraError`] from [`PinholeCamera::new`], for example for an image size outside
    /// `1..=32768` on either axis.
    pub fn camera(&self, width: u32, height: u32) -> Result<PinholeCamera, CameraError> {
        PinholeCamera::new(&CameraSpec {
            position: self.position,
            look: self.look,
            up: self.up,
            tan_half_fov: self.tan_half_fov,
            width,
            height,
        })
    }
}

/// The lights of a scene: at most one directional light and exactly one uniform sky, as in
/// Geordi's ray-traced scene profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lighting {
    /// The directional light, if any. Bunny's `DirectionalLight::new` has checked that its
    /// direction's largest component is exactly 1.0 and its irradiance is in range.
    pub sun: Option<DirectionalLight>,
    /// The radiance of every ray that leaves the scene.
    pub sky: SkyLight,
}

/// What a scene is built from.
#[derive(Clone, Copy, Debug)]
pub struct SceneSpec<'a> {
    /// The triangles' vertices, in primitive order. Each triangle is two-sided; its front face
    /// is the side from which the vertices appear counter-clockwise.
    pub triangles: &'a [[FixedVec3; 3]],
    /// The index into `materials` of each triangle, in primitive order.
    pub material_of: &'a [u32],
    /// The diffuse materials.
    pub materials: &'a [Albedo],
    /// The vertex normals of each triangle, in primitive order and in the order of its
    /// vertices: either empty, for a scene with no vertex normals, or one entry per triangle. A
    /// vertex with `None` has no normal; Bunny's smooth shading falls back to the triangle's
    /// geometric frame for a triangle with any vertex missing a normal.
    pub normals: &'a [[Option<FixedUnitVec3>; 3]],
    /// The light and the sky.
    pub lighting: Lighting,
    /// The camera placement.
    pub camera: CameraPlacement,
}

/// The image size [`Scene::new`] builds a camera at to check a placement: one pixel, inside
/// Bunny's limits, so any refusal is the placement's. Bunny checks the position, the tangent,
/// the look direction and the up hint independently of the size.
const PROBE_SIZE: u32 = 1;

/// Triangles in primitive order with their surfaces, materials, lighting, acceleration structure,
/// camera placement and content identity.
#[derive(Clone, Debug)]
pub struct Scene {
    content: SceneContentId,
    triangles: Vec<TraceTriangle>,
    surfaces: Vec<Option<TriangleSurface>>,
    material_of: Vec<u32>,
    materials: Vec<Albedo>,
    normals: Vec<[Option<FixedUnitVec3>; 3]>,
    lighting: Lighting,
    camera: CameraPlacement,
    tree: Tree,
}
impl Scene {
    /// Builds a scene.
    ///
    /// Each triangle becomes a Bunny `TraceTriangle` and, unless it has zero area, a
    /// `TriangleSurface` (its exact plane, unit normal and shading frame), built once here and
    /// never per hit. Bunny's exact triangle test never reports a hit on a zero-area triangle,
    /// so such a triangle needs no surface.
    ///
    /// The acceleration structure is a tree from Bunny's binned SAH builder `build_bvh_sah`,
    /// converted to Bunny's compact layout by `compact_bvh` and searched with
    /// `compact_closest_hit` and `compact_any_hit`. Bunny guarantees that their answers equal
    /// those of `closest_hit` and `any_hit` on any tree over the same triangles, and so a test
    /// of every triangle; the tree changes only the speed.
    ///
    /// The content identity ([`Scene::content_id`]) is computed here, once, from `spec` alone,
    /// after it is validated and before the tree is built: one SHA-256 pass over the input, with
    /// no second copy of it.
    ///
    /// # Errors
    ///
    /// [`SceneError::Empty`] for no triangles, [`SceneError::TooManyTriangles`] for more than
    /// `u32::MAX`, [`SceneError::MaterialCount`] when `material_of` has the wrong length,
    /// [`SceneError::NormalCount`] when `normals` is neither empty nor one entry per triangle,
    /// [`SceneError::Camera`] when Bunny refuses the camera placement (checked at a one-pixel
    /// image, before anything is allocated),
    /// [`SceneError::OutsideTraceDomain`] for the first triangle Bunny's `TraceTriangle::new`
    /// refuses, [`SceneError::UnknownMaterial`] for the first triangle whose material index is
    /// out of range, [`SceneError::OutOfMemory`] when a buffer cannot be reserved, and
    /// [`SceneError::BvhBuild`] if Bunny refuses to build or convert the tree.
    ///
    /// # Examples
    ///
    /// One triangle in front of the reference camera, with the reference scene's first material,
    /// sun and sky:
    ///
    /// ```
    /// use bunny_linalg::FixedVec3;
    /// use bunny_num::FixedQ32_32;
    /// use lumen::reference::{reference_camera, reference_lighting, reference_materials};
    /// use lumen::{Scene, SceneSpec};
    ///
    /// let at = |x: i64, y: i64, z: i64| {
    ///     let unit = |n: i64| FixedQ32_32::from_raw(n << 32);
    ///     FixedVec3::new(unit(x), unit(y), unit(z))
    /// };
    /// let triangles = [[at(-4, -1, -4), at(4, -1, -4), at(0, 3, -4)]];
    /// let scene = Scene::new(&SceneSpec {
    ///     triangles: &triangles,
    ///     material_of: &[0],
    ///     materials: &reference_materials()?,
    ///     normals: &[],
    ///     lighting: reference_lighting()?,
    ///     camera: reference_camera(),
    /// })?;
    /// assert_eq!(scene.triangles().len(), 1);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn new(spec: &SceneSpec<'_>) -> Result<Self, SceneError> {
        let count = spec.triangles.len();
        if count == 0 {
            return Err(SceneError::Empty);
        }
        if u32::try_from(count).is_err() {
            return Err(SceneError::TooManyTriangles { triangles: count, limit: u32::MAX });
        }
        let material_indices = spec.material_of.len();
        if material_indices != count {
            return Err(SceneError::MaterialCount { triangles: count, material_indices });
        }
        if !spec.normals.is_empty() && spec.normals.len() != count {
            return Err(SceneError::NormalCount { triangles: count, normals: spec.normals.len() });
        }
        spec.camera.camera(PROBE_SIZE, PROBE_SIZE).map_err(SceneError::Camera)?;
        let mut triangles = reserved(SceneBuffer::Triangles, count)?;
        let mut surfaces = reserved(SceneBuffer::Surfaces, count)?;
        let mut boxes: Vec<FixedAabb3> = reserved(SceneBuffer::Boxes, count)?;
        // `count <= u32::MAX`, so every triangle gets its index; the inclusive range cannot
        // overflow while it is zipped.
        let indices = (0..=u32::MAX).map(PrimitiveIndex::new);
        for ((&[v0, v1, v2], &material), primitive) in
            spec.triangles.iter().zip(spec.material_of).zip(indices)
        {
            let Some(triangle) = TraceTriangle::new(v0, v1, v2) else {
                return Err(outside_trace_domain(primitive, [v0, v1, v2]));
            };
            let materials = spec.materials.len();
            if widen(material) >= materials {
                return Err(SceneError::UnknownMaterial { primitive, material, materials });
            }
            boxes.push(triangle.bounds());
            surfaces.push(TriangleSurface::new(&triangle));
            triangles.push(triangle);
        }
        // The identity reads only the admitted spec; the tree is built after it and is no input.
        let content = content_id::content_id(spec);
        let tree = build_tree(&boxes)?;
        let mut material_of = reserved(SceneBuffer::MaterialIndices, count)?;
        material_of.extend_from_slice(spec.material_of);
        let mut materials = reserved(SceneBuffer::Materials, spec.materials.len())?;
        materials.extend_from_slice(spec.materials);
        let mut normals = reserved(SceneBuffer::VertexNormals, spec.normals.len())?;
        normals.extend_from_slice(spec.normals);
        Ok(Self {
            content,
            triangles,
            surfaces,
            material_of,
            materials,
            normals,
            lighting: spec.lighting,
            camera: spec.camera,
            tree,
        })
    }

    /// The triangles, in primitive order.
    #[must_use]
    pub fn triangles(&self) -> &[TraceTriangle] {
        &self.triangles
    }

    /// The prepared surface of the triangle at `primitive`, or `None` for an index out of range
    /// or a zero-area triangle.
    #[must_use]
    pub fn surface(&self, primitive: PrimitiveIndex) -> Option<&TriangleSurface> {
        self.surfaces.get(primitive.to_usize())?.as_ref()
    }

    /// The albedo of the triangle at `primitive`, or `None` for an index out of range.
    #[must_use]
    pub fn albedo(&self, primitive: PrimitiveIndex) -> Option<&Albedo> {
        let material = *self.material_of.get(primitive.to_usize())?;
        self.materials.get(widen(material))
    }

    /// The vertex normals of the triangle at `primitive`, in the order of its vertices, or
    /// `None` for an index out of range or a scene given no vertex normals.
    #[must_use]
    pub fn vertex_normals(&self, primitive: PrimitiveIndex) -> Option<&[Option<FixedUnitVec3>; 3]> {
        self.normals.get(primitive.to_usize())
    }

    /// The light and the sky.
    #[must_use]
    pub const fn lighting(&self) -> Lighting {
        self.lighting
    }

    /// The camera placement.
    #[must_use]
    pub const fn camera(&self) -> CameraPlacement {
        self.camera
    }

    /// The scene's content identity: the SHA-256 of the canonical encoding of the [`SceneSpec`]
    /// it was built from ([`SCENE_CONTENT_ENCODING`], ADR 0005), computed once by
    /// [`Scene::new`]. It does not depend on the tree, and is the same on every platform.
    ///
    /// # Examples
    ///
    /// ```
    /// let scene = lumen::reference_scene()?;
    /// let id = scene.content_id();
    /// assert_eq!(id, lumen::reference_scene()?.content_id());
    /// assert!(id.to_string().starts_with("sha256:") && id.to_string().len() == 71);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub const fn content_id(&self) -> SceneContentId {
        self.content
    }
}

/// An empty vector with room for `items` items of `buffer`, or [`SceneError::OutOfMemory`] with
/// both and the allocator's error.
fn reserved<T>(buffer: SceneBuffer, items: usize) -> Result<Vec<T>, SceneError> {
    let mut vector = Vec::new();
    vector.try_reserve_exact(items).map_err(|error| SceneError::OutOfMemory {
        buffer,
        items,
        error,
    })?;
    Ok(vector)
}

/// Why Bunny's `TraceTriangle::new` refused `vertices`: its one documented check repeated, on
/// the failure path only, to name the first vertex with a coordinate outside the trace domain.
fn outside_trace_domain(primitive: PrimitiveIndex, vertices: [FixedVec3; 3]) -> SceneError {
    let limit = TRACE_COORD_LIMIT_RAW.unsigned_abs();
    let outside = |v: &FixedVec3| [v.x, v.y, v.z].iter().any(|c| c.raw().unsigned_abs() >= limit);
    let first = vertices.iter().zip(0_u8..).find(|(v, _)| outside(v));
    match first {
        Some((&point, vertex)) => SceneError::OutsideTraceDomain { primitive, vertex, point },
        None => SceneError::TriangleRefused { primitive },
    }
}
