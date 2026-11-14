//! Scenes: triangles with materials, a directional light and a sky, the acceleration structure
//! that traces rays through them, and a camera placement.
//!
//! A [`Scene`] is built once from a [`SceneSpec`]: a list of triangles in primitive order, a
//! material (a Bunny [`Albedo`]) for each, the [`Lighting`] and the placement of a pinhole camera.
//! Building it prepares everything a ray needs, so tracing allocates nothing: Bunny's
//! `TraceTriangle` and `TriangleSurface` per triangle, and the bounding volume hierarchy (see
//! [`Scene::new`]). The built-in reference scene is in [`crate::reference`].

use core::fmt;

use bunny_broadphase::{
    build_bvh_sah, compact_any_hit, compact_bvh, compact_closest_hit, BvhNode, CompactChild,
    CompactNode, CompactRoot, CompactView, TraversalError,
};
use bunny_geom::FixedAabb3;
use bunny_linalg::FixedVec3;
use bunny_num::FixedQ32_32;
use bunny_optics::{
    Albedo, CameraError, CameraSpec, DirectionalLight, PinholeCamera, SkyLight, TriangleSurface,
};
use bunny_query::{TraceRay, TraceTriangle, TriangleHit};

use crate::mesh::PlaceError;

/// The farthest distance a camera or bounce ray searches: no limit, raw `i64::MAX`.
pub const FAR: FixedQ32_32 = FixedQ32_32::from_raw(i64::MAX);

/// Where a pinhole camera stands and where it looks. The image size is chosen per render.
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
    /// The light and the sky.
    pub lighting: Lighting,
    /// The camera placement.
    pub camera: CameraPlacement,
}

/// Why a scene could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneError {
    /// The scene has no triangles.
    Empty,
    /// The scene has more triangles than a `u32` primitive index can name.
    TooManyTriangles,
    /// A vertex coordinate of the triangle at this primitive index is outside the trace domain
    /// (`abs(raw) >= 2^40`, 256 units).
    OutsideTraceDomain {
        /// The primitive index of the triangle.
        primitive: usize,
    },
    /// `material_of` does not have one entry per triangle.
    MaterialCount,
    /// The triangle at this primitive index names a material that does not exist.
    UnknownMaterial {
        /// The primitive index of the triangle.
        primitive: usize,
    },
    /// The bounding volume hierarchy could not be built or converted.
    BvhBuild,
    /// Memory for the scene's buffers could not be reserved.
    OutOfMemory,
    /// A mesh of the scene could not be read or placed.
    Mesh(PlaceError),
}

impl fmt::Display for SceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("scene has no triangles"),
            Self::TooManyTriangles => f.write_str("scene has more than 2^32 - 1 triangles"),
            Self::OutsideTraceDomain { primitive } => {
                write!(f, "triangle {primitive} has a vertex outside the trace domain")
            }
            Self::MaterialCount => f.write_str("scene needs one material index per triangle"),
            Self::UnknownMaterial { primitive } => {
                write!(f, "triangle {primitive} names a material that does not exist")
            }
            Self::BvhBuild => f.write_str("the bounding volume hierarchy could not be built"),
            Self::OutOfMemory => f.write_str("not enough memory for the scene"),
            Self::Mesh(error) => write!(f, "a mesh could not be placed: {error}"),
        }
    }
}

impl std::error::Error for SceneError {}

/// The nearest triangle a ray hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    /// The primitive index of the triangle.
    pub primitive: u32,
    /// Bunny's `TriangleHit` for that triangle: the distance (a multiple of the ray's
    /// direction, floored) and the facing.
    pub hit: TriangleHit,
}

/// Why a ray could not be traced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceError {
    /// The traversal refused the tree.
    Traversal(TraversalError),
    /// The traversal named a primitive that the triangle list does not hold, or whose hit
    /// could not be repeated. Neither can happen with a tree built by [`Scene::new`].
    InconsistentHit,
}

impl fmt::Display for TraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Traversal(error) => write!(f, "traversal failed: {error:?}"),
            Self::InconsistentHit => f.write_str("traversal reported no valid hit"),
        }
    }
}

impl std::error::Error for TraceError {}

/// The two ray queries a path needs. [`Scene`] implements it with Bunny's traversals; a test can
/// wrap a scene to check every query against a test of every triangle.
pub trait Tracer {
    /// The nearest triangle `ray` hits at a distance in `0..=FAR`, or `None` for a miss. Of
    /// triangles at the same distance the lowest primitive index wins.
    ///
    /// # Errors
    ///
    /// A [`TraceError`] if the traversal fails.
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError>;

    /// Whether `ray` hits any triangle at a distance in `0..=t_max`.
    ///
    /// # Errors
    ///
    /// A [`TraceError`] if the traversal fails.
    fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError>;
}

/// Triangles in primitive order with their surfaces, materials, lighting, acceleration structure
/// and camera placement.
#[derive(Clone, Debug)]
pub struct Scene {
    triangles: Vec<TraceTriangle>,
    surfaces: Vec<Option<TriangleSurface>>,
    material_of: Vec<u32>,
    materials: Vec<Albedo>,
    lighting: Lighting,
    camera: CameraPlacement,
    tree: Tree,
}

/// A compact BVH: its root, its nodes and the primitive-index buffer its leaves refer to.
#[derive(Clone, Debug)]
struct Tree {
    root: Option<CompactRoot>,
    nodes: Vec<CompactNode>,
    prim_indices: Vec<u32>,
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
    /// # Errors
    ///
    /// [`SceneError::Empty`] for no triangles, [`SceneError::TooManyTriangles`] for more than
    /// `u32::MAX`, [`SceneError::MaterialCount`] when `material_of` has the wrong length,
    /// [`SceneError::OutsideTraceDomain`] for the first triangle Bunny's `TraceTriangle::new`
    /// refuses, [`SceneError::UnknownMaterial`] for the first triangle whose material index is
    /// out of range, [`SceneError::OutOfMemory`] when a buffer cannot be reserved, and
    /// [`SceneError::BvhBuild`] if Bunny refuses to build or convert the tree.
    pub fn new(spec: &SceneSpec<'_>) -> Result<Self, SceneError> {
        let count = spec.triangles.len();
        if count == 0 {
            return Err(SceneError::Empty);
        }
        if u32::try_from(count).is_err() {
            return Err(SceneError::TooManyTriangles);
        }
        if spec.material_of.len() != count {
            return Err(SceneError::MaterialCount);
        }
        let mut triangles = reserved(count)?;
        let mut surfaces = reserved(count)?;
        let mut boxes: Vec<FixedAabb3> = reserved(count)?;
        for (primitive, (&[v0, v1, v2], &material)) in
            spec.triangles.iter().zip(spec.material_of).enumerate()
        {
            let triangle = TraceTriangle::new(v0, v1, v2)
                .ok_or(SceneError::OutsideTraceDomain { primitive })?;
            let known = usize::try_from(material).is_ok_and(|index| index < spec.materials.len());
            if !known {
                return Err(SceneError::UnknownMaterial { primitive });
            }
            boxes.push(triangle.bounds());
            surfaces.push(TriangleSurface::new(&triangle));
            triangles.push(triangle);
        }
        let tree = build_tree(&boxes)?;
        let mut material_of = reserved(count)?;
        material_of.extend_from_slice(spec.material_of);
        let mut materials = reserved(spec.materials.len())?;
        materials.extend_from_slice(spec.materials);
        Ok(Self {
            triangles,
            surfaces,
            material_of,
            materials,
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
    pub fn surface(&self, primitive: u32) -> Option<&TriangleSurface> {
        self.surfaces.get(usize::try_from(primitive).ok()?)?.as_ref()
    }

    /// The albedo of the triangle at `primitive`, or `None` for an index out of range.
    #[must_use]
    pub fn albedo(&self, primitive: u32) -> Option<&Albedo> {
        let material = *self.material_of.get(usize::try_from(primitive).ok()?)?;
        self.materials.get(usize::try_from(material).ok()?)
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

    fn view(&self) -> CompactView<'_> {
        CompactView {
            root: self.tree.root,
            nodes: &self.tree.nodes,
            prim_indices: &self.tree.prim_indices,
        }
    }

    fn triangle(&self, primitive: u32) -> Option<&TraceTriangle> {
        self.triangles.get(usize::try_from(primitive).ok()?)
    }
}

impl Tracer for Scene {
    /// Bunny's `compact_closest_hit` over the scene's tree with `TraceRay::hit_triangle` as the
    /// primitive test (`t_min` zero, `t_max` the traversal's limit, at first [`FAR`]). The
    /// winner is tested once more, at exactly its distance, to read its full `TriangleHit`.
    fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
        let found = compact_closest_hit(self.view(), ray, FAR, |primitive, limit| {
            let triangle = self.triangle(primitive)?;
            ray.hit_triangle(triangle, FixedQ32_32::ZERO, limit).map(|hit| hit.distance)
        })
        .map_err(TraceError::Traversal)?;
        let Some(found) = found else { return Ok(None) };
        let triangle = self.triangle(found.primitive).ok_or(TraceError::InconsistentHit)?;
        let hit = ray
            .hit_triangle(triangle, found.distance, found.distance)
            .ok_or(TraceError::InconsistentHit)?;
        Ok(Some(Hit { primitive: found.primitive, hit }))
    }

    /// Bunny's `compact_any_hit` over the scene's tree with `TraceRay::hit_triangle` (`t_min`
    /// zero, `t_max` as given) as the primitive test.
    fn occluded(&self, ray: &TraceRay, t_max: FixedQ32_32) -> Result<bool, TraceError> {
        compact_any_hit(self.view(), ray, t_max, |primitive| {
            self.triangle(primitive).is_some_and(|triangle| {
                ray.hit_triangle(triangle, FixedQ32_32::ZERO, t_max).is_some()
            })
        })
        .map_err(TraceError::Traversal)
    }
}

/// A tree over `boxes` from `build_bvh_sah`, converted by `compact_bvh`: the compact root, the
/// compact nodes and the primitive-index buffer.
fn build_tree(boxes: &[FixedAabb3]) -> Result<Tree, SceneError> {
    let count = boxes.len();
    let node_count = count.checked_mul(2).ok_or(SceneError::TooManyTriangles)?;
    let empty_box = FixedAabb3::new(ZERO_VEC, ZERO_VEC);
    let empty = BvhNode { bounds: empty_box, first_child_or_prim_idx: 0, prim_count: 0 };
    let mut nodes = reserved(node_count)?;
    nodes.resize(node_count, empty);
    let mut prim_indices = reserved(count)?;
    prim_indices.resize(count, 0_u32);
    let used = build_bvh_sah(&mut nodes, &mut prim_indices, boxes).ok_or(SceneError::BvhBuild)?;
    nodes.truncate(used);
    // A tree of n nodes has (n - 1) / 2 interior nodes, one compact node each.
    let interior = used.saturating_sub(1) / 2;
    let blank = CompactNode {
        bounds: [empty_box, empty_box],
        children: [CompactChild::default(), CompactChild::default()],
    };
    let mut compact = reserved(interior)?;
    compact.resize(interior, blank);
    let converted = compact_bvh(&nodes, &mut compact).map_err(|_| SceneError::BvhBuild)?;
    let root = converted.map(|(root, written)| {
        compact.truncate(written);
        root
    });
    Ok(Tree { root, nodes: compact, prim_indices })
}

const ZERO_VEC: FixedVec3 = FixedVec3::new(FixedQ32_32::ZERO, FixedQ32_32::ZERO, FixedQ32_32::ZERO);

/// An empty vector with room for `count` items, or [`SceneError::OutOfMemory`].
fn reserved<T>(count: usize) -> Result<Vec<T>, SceneError> {
    let mut items = Vec::new();
    items.try_reserve_exact(count).map_err(|_| SceneError::OutOfMemory)?;
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::{per_256, reference_camera, reference_scene, REFERENCE_TRIANGLE_COUNT};
    use crate::vertex_grid::{at, grid, U};

    fn grey() -> Albedo {
        Albedo::new([per_256(128); 3]).expect("valid")
    }

    fn dark() -> Lighting {
        Lighting { sun: None, sky: SkyLight::new([FixedQ32_32::ZERO; 3]).expect("valid") }
    }

    /// A tree over n primitives has at most n - 1 interior nodes.
    #[test]
    fn the_reference_tree_has_fewer_interior_nodes_than_triangles() {
        let scene = reference_scene().expect("the reference scene builds");
        assert!(scene.tree.nodes.len() < REFERENCE_TRIANGLE_COUNT);
    }

    #[test]
    fn scene_construction_refuses_bad_input() {
        let camera = reference_camera();
        let materials = [grey()];
        let spec = |triangles: &[[FixedVec3; 3]], material_of: &[u32]| {
            let spec = SceneSpec {
                triangles,
                material_of,
                materials: &materials,
                lighting: dark(),
                camera,
            };
            Scene::new(&spec).err()
        };
        assert_eq!(spec(&[], &[]), Some(SceneError::Empty));
        let far = FixedVec3::new(grid(256 * U), FixedQ32_32::ZERO, FixedQ32_32::ZERO);
        let fine = at(0, 0, 0);
        let good = [fine, at(U, 0, 0), at(0, U, 0)];
        let triangles = [good, [fine, far, at(0, U, 0)]];
        assert_eq!(
            spec(&triangles, &[0, 0]),
            Some(SceneError::OutsideTraceDomain { primitive: 1 })
        );
        assert_eq!(spec(&[good], &[]), Some(SceneError::MaterialCount));
        assert_eq!(
            spec(&[good, good], &[0, 1]),
            Some(SceneError::UnknownMaterial { primitive: 1 })
        );
        // A zero-area triangle is accepted and has no surface.
        let flat = [fine, fine, at(U, 0, 0)];
        let built = Scene::new(&SceneSpec {
            triangles: &[good, flat],
            material_of: &[0, 0],
            materials: &materials,
            lighting: dark(),
            camera,
        })
        .expect("builds");
        assert!(built.surface(0).is_some());
        assert!(built.surface(1).is_none());
        assert!(built.surface(2).is_none());
    }

    /// A one-triangle scene is a one-leaf tree, which the compact layout stores in its root
    /// with no compact node; tracing still works.
    #[test]
    fn a_one_triangle_scene_traces() {
        let triangle = [at(-U, -U, 0), at(U, -U, 0), at(0, U, 0)];
        let scene = Scene::new(&SceneSpec {
            triangles: &[triangle],
            material_of: &[0],
            materials: &[grey()],
            lighting: dark(),
            camera: reference_camera(),
        })
        .expect("builds");
        assert!(scene.tree.nodes.is_empty());
        let toward = TraceRay::new(at(0, 0, 4 * U), at(0, 0, -U)).expect("ray");
        let hit = scene.closest(&toward).expect("traces").expect("hits");
        assert_eq!((hit.primitive, hit.hit.distance, hit.hit.front_face), (0, grid(4 * U), true));
        assert!(scene.occluded(&toward, FAR).expect("traces"));
        assert!(!scene.occluded(&toward, grid(4 * U - 1)).expect("traces"));
        let away = TraceRay::new(at(0, 0, 4 * U), at(0, 0, U)).expect("ray");
        assert_eq!(scene.closest(&away).expect("traces"), None);
        assert!(!scene.occluded(&away, FAR).expect("traces"));
    }
}
