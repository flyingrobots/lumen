//! Scenes: triangles with materials, a directional light and a sky, the acceleration structure
//! that traces rays through them, and a camera placement.
//!
//! A [`Scene`] is built once from a [`SceneSpec`]: a list of triangles in primitive order, a
//! material (a Bunny [`Albedo`]) for each, the [`Lighting`] and the placement of a pinhole camera.
//! Building it prepares everything a ray needs, so tracing allocates nothing: Bunny's
//! `TraceTriangle` and `TriangleSurface` per triangle, and the bounding volume hierarchy (see
//! [`Scene::new`]). [`reference_scene`] builds lumen's built-in reference scene: shapes defined in
//! code and the Stanford bunny, read from an embedded asset by [`crate::ply`].

use core::fmt;

use bunny_broadphase::{
    build_bvh_sah, compact_any_hit, compact_bvh, compact_closest_hit, BvhNode, CompactChild,
    CompactNode, CompactRoot, CompactView, TraversalError,
};
use bunny_geom::FixedAabb3;
use bunny_linalg::FixedVec3;
use bunny_num::fixed_q32_32::ONE_RAW;
use bunny_num::FixedQ32_32;
use bunny_optics::{
    Albedo, CameraError, CameraSpec, DirectionalLight, PinholeCamera, SkyLight, TriangleSurface,
};
use bunny_query::{TraceRay, TraceTriangle, TriangleHit};

use crate::mesh::{bunny_triangles, BUNNY_TRIANGLE_COUNT};

/// The stable name and version of [`reference_scene`].
///
/// Any change to the reference scene's triangles, their order, their materials, its lighting or
/// its camera placement is a new version, and changes every image hash of it. Version 1
/// (`lumen-reference-scene/1`, geometry and camera only, shaded by normal) and version 2
/// (`lumen-reference-scene/2`, version 1 with materials, a light and a sky) are retired.
/// Version 3 keeps every triangle of version 2, their order, its materials and its lighting,
/// appends the Stanford bunny ([`crate::mesh::bunny_triangles`]) with a material of its own,
/// and moves the camera one unit left and one up so the bunny's shadow can be seen
/// ([`reference_camera`]).
pub const REFERENCE_SCENE_ID: &str = "lumen-reference-scene/3";

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

/// Quads along each edge of each cube face of the sphere: 6 * 48 * 48 * 2 = 27 648 triangles.
const SPHERE_GRID: i64 = 48;

/// One raw unit of the vertex grid: `2^-16`. See [`snap`].
pub(crate) const GRID_RAW: i64 = 1 << 16;

/// A value of `units / 2^16` units: every scene constant is a whole number of grid steps.
pub(crate) const fn grid(units_times_2_16: i64) -> FixedQ32_32 {
    FixedQ32_32::from_raw(units_times_2_16 * GRID_RAW)
}

/// A point from three coordinates in grid steps of `2^-16` units.
const fn at(x: i64, y: i64, z: i64) -> FixedVec3 {
    FixedVec3::new(grid(x), grid(y), grid(z))
}

/// Whole units in grid steps.
pub(crate) const U: i64 = 1 << 16;

/// The ground's height: -1.125 units, just below the sphere's lowest point (-1.0).
pub(crate) const GROUND_Y: i64 = -U * 9 / 8;
/// Half the ground's side: it spans `-16..=16` on X and Z.
const GROUND_HALF: i64 = 16 * U;
/// The back wall's plane, `z = -4`, its half width (3 units) and its top (`y = 2`).
const WALL_Z: i64 = -4 * U;
const WALL_HALF: i64 = 3 * U;
const WALL_TOP: i64 = 2 * U;
/// The octahedron's centre (right of the sphere, its lowest vertex on the ground) and
/// its radius (1 unit).
const OCTAHEDRON_CENTRE: (i64, i64, i64) = (U * 9 / 4, GROUND_Y + U, U / 2);

/// Floors a raw coordinate to a multiple of `2^16` raw units (`2^-16` units).
///
/// Every coordinate of the reference scene is below 256 units in magnitude and a multiple
/// of `2^-16`, so it has at most 24 significant bits and is exactly an `f32`. A mesh in the
/// `bunny-ply-binary-le-f32/1` layout (Geordi's scene profile) can therefore carry the
/// scene's vertices without changing a bit.
pub(crate) const fn snap(value: FixedQ32_32) -> FixedQ32_32 {
    FixedQ32_32::from_raw(value.raw() & !(GRID_RAW - 1))
}

/// A point on the bumpy sphere above cube-face coordinates `(a, b)` in `0..=SPHERE_GRID`,
/// adapted from Bunny's scene benchmark (`crates/bunny-perf/src/scene.rs`): a cube point
/// normalized with `FixedVec3::normalize`, pushed out by up to 4 % from a hash of the cube
/// point (so shared corners agree), then snapped to the `2^-16` grid.
fn sphere_point(face: usize, a: i64, b: i64) -> FixedVec3 {
    let along = |i: i64| FixedQ32_32::from_raw(2 * ONE_RAW * i / SPHERE_GRID - ONE_RAW);
    let one = FixedQ32_32::ONE;
    let (a, b) = (along(a), along(b));
    let cube = match face {
        0 => FixedVec3::new(one, a, b),
        1 => FixedVec3::new(-one, b, a),
        2 => FixedVec3::new(b, one, a),
        3 => FixedVec3::new(a, -one, b),
        4 => FixedVec3::new(a, b, one),
        _ => FixedVec3::new(b, a, -one),
    };
    let unit = cube.normalize().unwrap_or(cube);
    let hash = (cube.x.raw() ^ cube.y.raw().rotate_left(21) ^ cube.z.raw().rotate_left(42))
        .wrapping_mul(0x9E37_79B9_7F4A_7C15_u64.cast_signed());
    let radius = FixedQ32_32::from_raw(ONE_RAW + (hash >> 40).rem_euclid(ONE_RAW / 25));
    let point = unit * radius;
    FixedVec3::new(snap(point.x), snap(point.y), snap(point.z))
}

/// The reference scene's triangles, in primitive order:
///
/// 1. the ground, two triangles at `y = -1.125` spanning `-16..=16` on X and Z, normal `+Y`;
/// 2. a back wall, two triangles at `z = -4` spanning `x = -3..=3` and `y = -1.125..=2`,
///    normal `+Z` (toward the camera);
/// 3. an octahedron of radius 1 centred at `(2.25, -0.125, 0.5)`, its lowest vertex on the
///    ground, eight faces with normals `(+-1, +-1, +-1) / sqrt(3)`;
/// 4. the bumpy sphere: a cube subdivided 48 by 48 per face and pushed out to a radius of
///    1.0 to 1.04 around the origin, 27 648 triangles;
/// 5. the Stanford bunny, [`crate::mesh::bunny_triangles`]: 3 851 triangles in the file's face
///    order, after every shape above so that their primitive indices are those of version 2.
///
/// Every triangle of the first four objects winds counter-clockwise seen from outside its object
/// (from above for the ground, from the camera's side for the wall), so its normal points
/// outward. The bunny keeps the file's winding (see [`crate::mesh::bunny_triangles`]); shading
/// is two-sided.
///
/// # Panics
///
/// Never: the embedded bunny reads and places (a test checks it).
#[must_use]
pub fn reference_triangles() -> Vec<[FixedVec3; 3]> {
    let mut triangles = Vec::with_capacity(REFERENCE_TRIANGLE_COUNT);
    let ground = |x, z| at(x, GROUND_Y, z);
    let (g, w) = (GROUND_HALF, WALL_HALF);
    triangles.push([ground(-g, g), ground(g, g), ground(g, -g)]);
    triangles.push([ground(-g, g), ground(g, -g), ground(-g, -g)]);
    let wall = |x, y| at(x, y, WALL_Z);
    triangles.push([wall(-w, GROUND_Y), wall(w, GROUND_Y), wall(w, WALL_TOP)]);
    triangles.push([wall(-w, GROUND_Y), wall(w, WALL_TOP), wall(-w, WALL_TOP)]);
    let (cx, cy, cz) = OCTAHEDRON_CENTRE;
    for sx in [1, -1] {
        for sy in [1, -1] {
            for sz in [1, -1] {
                let (px, py, pz) =
                    (at(cx + sx * U, cy, cz), at(cx, cy + sy * U, cz), at(cx, cy, cz + sz * U));
                // cross(py - px, pz - px) = (sy * sz, sx * sz, sx * sy), which is the outward
                // direction (sx, sy, sz) exactly when sx * sy * sz = 1.
                if sx * sy * sz == 1 {
                    triangles.push([px, py, pz]);
                } else {
                    triangles.push([px, pz, py]);
                }
            }
        }
    }
    for face in 0..6 {
        for a in 0..SPHERE_GRID {
            for b in 0..SPHERE_GRID {
                let corner = |da, db| sphere_point(face, a + da, b + db);
                let (p00, p10, p11, p01) = (corner(0, 0), corner(1, 0), corner(1, 1), corner(0, 1));
                triangles.push([p00, p10, p11]);
                triangles.push([p00, p11, p01]);
            }
        }
    }
    let bunny = bunny_triangles().expect("the embedded bunny reads");
    triangles.extend_from_slice(&bunny.triangles);
    triangles
}

/// The first primitive index of the bunny in the reference scene: every triangle of version 2
/// comes first.
pub const BUNNY_FIRST_PRIMITIVE: usize = 4 + 8 + 6 * 48 * 48 * 2;

/// The reference camera: at `(-1, 2, 5)`, looking along `(0.1875, -0.375, -1)` (down and a
/// little to the right, toward the sphere), up `+Y`, with a vertical field of view whose
/// half-angle has tangent 0.5 (about 53 degrees in all).
///
/// Versions 1 and 2 stood at `(0, 1, 5)` looking along `(0, -0.25, -1)`. From there the line of
/// sight to the bunny runs almost exactly along its shadow (the sun's ground direction is
/// `(-0.5, -1)` in X and Z, and the bunny is at about `(-2.2, 0.5)`), so the bunny hid its own
/// shadow. One unit left and one up, the shadow falls clear of the bunny and every object and
/// shadow of version 2 stays in the picture.
#[must_use]
pub const fn reference_camera() -> CameraPlacement {
    CameraPlacement {
        position: at(-U, 2 * U, 5 * U),
        look: at(U * 3 / 16, -U * 3 / 8, -U),
        up: at(0, U, 0),
        tan_half_fov: grid(U / 2),
    }
}

/// A Q32.32 value of `n / 256`.
const fn per_256(n: i64) -> FixedQ32_32 {
    FixedQ32_32::from_raw(n * (ONE_RAW / 256))
}

/// The reference materials, as `[red, green, blue]` linear albedo in 256ths, indexed by
/// [`REFERENCE_MATERIAL_OF`]: a light warm grey ground, a pale stone wall, a blue octahedron,
/// a red sphere and a green bunny.
pub const REFERENCE_ALBEDO_256: [[i64; 3]; 5] =
    [[150, 145, 135], [175, 165, 145], [50, 95, 200], [205, 60, 45], [70, 165, 85]];

/// The material index of each object of [`reference_triangles`]: ground 0, wall 1, octahedron
/// 2, sphere 3, bunny 4.
pub const REFERENCE_MATERIAL_OF: [u32; 5] = [0, 1, 2, 3, 4];

/// The direction toward the reference sun, in sixteenths: `(0.5, 0.3125, 1.0)`. It comes from
/// the right, from in front of the scene (the camera's side) and from about 15 degrees above
/// the horizon, so the sphere and the octahedron cast long shadows back across the ground and
/// onto the wall. Its largest component is exactly 1.0, as Bunny's `DirectionalLight` requires.
pub const REFERENCE_TOWARD_SUN_16: [i64; 3] = [8, 5, 16];

/// The reference sun's irradiance per channel, in 256ths: about `(4.0, 3.8, 3.5)`, slightly warm.
pub const REFERENCE_SUN_256: [i64; 3] = [1024, 970, 900];

/// The reference sky's radiance per channel, in 256ths: about `(0.25, 0.33, 0.47)`, a pale blue.
pub const REFERENCE_SKY_256: [i64; 3] = [64, 85, 120];

/// The reference lighting: [`REFERENCE_TOWARD_SUN_16`] at [`REFERENCE_SUN_256`] and a
/// [`REFERENCE_SKY_256`] sky.
///
/// # Panics
///
/// Never: the constants are inside Bunny's accepted ranges (a test checks it), so the Bunny
/// constructors accept them.
#[must_use]
pub fn reference_lighting() -> Lighting {
    let [x, y, z] = REFERENCE_TOWARD_SUN_16.map(|c| FixedQ32_32::from_raw(c * (ONE_RAW / 16)));
    let sun = DirectionalLight::new(FixedVec3::new(x, y, z), REFERENCE_SUN_256.map(per_256))
        .expect("the reference sun is a valid Bunny light");
    let sky = SkyLight::new(REFERENCE_SKY_256.map(per_256)).expect("the reference sky is valid");
    Lighting { sun: Some(sun), sky }
}

/// The reference materials, [`REFERENCE_ALBEDO_256`] as Bunny albedos.
///
/// # Panics
///
/// Never: every channel is in `0..=256`, inside Bunny's `0..=1.0`.
#[must_use]
pub fn reference_materials() -> [Albedo; 5] {
    REFERENCE_ALBEDO_256
        .map(|channels| Albedo::new(channels.map(per_256)).expect("albedo channels are in 0..=1"))
}

/// The material index of each reference triangle, in primitive order.
#[must_use]
pub fn reference_material_of() -> Vec<u32> {
    let [ground, wall, octahedron, sphere, bunny] = REFERENCE_MATERIAL_OF;
    let mut material_of = Vec::with_capacity(REFERENCE_TRIANGLE_COUNT);
    material_of.extend([ground; 2]);
    material_of.extend([wall; 2]);
    material_of.extend([octahedron; 8]);
    material_of.resize(BUNNY_FIRST_PRIMITIVE, sphere);
    material_of.resize(REFERENCE_TRIANGLE_COUNT, bunny);
    material_of
}

/// The number of triangles of the reference scene: 2 + 2 + 8 + 27 648 + 3 851 = 31 511.
pub const REFERENCE_TRIANGLE_COUNT: usize = BUNNY_FIRST_PRIMITIVE + BUNNY_TRIANGLE_COUNT;

/// The built-in reference scene, [`REFERENCE_SCENE_ID`]: [`reference_triangles`] with
/// [`reference_materials`] and [`reference_lighting`], seen by [`reference_camera`].
///
/// # Errors
///
/// [`SceneError::OutOfMemory`] when its buffers cannot be reserved. The other errors of
/// [`Scene::new`] do not occur for this scene; a test checks it builds.
pub fn reference_scene() -> Result<Scene, SceneError> {
    let triangles = reference_triangles();
    let material_of = reference_material_of();
    let materials = reference_materials();
    Scene::new(&SceneSpec {
        triangles: &triangles,
        material_of: &material_of,
        materials: &materials,
        lighting: reference_lighting(),
        camera: reference_camera(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grey() -> Albedo {
        Albedo::new([per_256(128); 3]).expect("valid")
    }

    fn dark() -> Lighting {
        Lighting { sun: None, sky: SkyLight::new([FixedQ32_32::ZERO; 3]).expect("valid") }
    }

    #[test]
    fn the_reference_scene_has_the_stated_triangles_and_materials() {
        assert_eq!(reference_triangles().len(), REFERENCE_TRIANGLE_COUNT);
        assert_eq!(BUNNY_FIRST_PRIMITIVE, 27_660);
        assert_eq!(REFERENCE_TRIANGLE_COUNT, 31_511);
        let scene = reference_scene().expect("the reference scene builds");
        assert_eq!(scene.triangles().len(), 31_511);
        // A tree over n primitives has at most n - 1 interior nodes.
        assert!(scene.tree.nodes.len() < 31_511);
        let materials = reference_materials();
        for (primitive, want) in [(0, 0), (1, 0), (2, 1), (3, 1), (4, 2), (11, 2), (12, 3)] {
            assert_eq!(scene.albedo(primitive), Some(&materials[want]), "primitive {primitive}");
        }
        assert_eq!(scene.albedo(27_659), Some(&materials[3]));
        assert_eq!(scene.albedo(27_660), Some(&materials[4]));
        assert_eq!(scene.albedo(31_510), Some(&materials[4]));
        assert_eq!(scene.albedo(31_511), None);
        // Five distinct albedos.
        for (i, a) in REFERENCE_ALBEDO_256.iter().enumerate() {
            assert!(REFERENCE_ALBEDO_256[..i].iter().all(|b| b != a), "albedo {i} repeats");
        }
        let lighting = reference_lighting();
        assert_eq!(scene.lighting(), lighting);
        let sun = lighting.sun.expect("a sun");
        assert_eq!(sun.toward_light().z, FixedQ32_32::ONE);
    }

    #[test]
    fn every_coordinate_is_on_the_f32_exact_grid_inside_the_trace_domain() {
        for triangle in reference_triangles() {
            for vertex in triangle {
                for c in [vertex.x, vertex.y, vertex.z] {
                    assert_eq!(c.raw() % GRID_RAW, 0, "{c:?} is off the 2^-16 grid");
                    assert!(c.raw().abs() < 256 * ONE_RAW);
                    // Exactly an f32 (a boundary conversion only: no float reaches a pixel).
                    let narrowed = FixedQ32_32::try_from_f32(c.to_f32()).expect("finite");
                    assert_eq!(narrowed, c);
                }
            }
        }
    }

    #[test]
    fn every_surface_normal_points_out_of_its_object() {
        let scene = reference_scene().expect("the reference scene builds");
        let (cx, cy, cz) = OCTAHEDRON_CENTRE;
        // The bunny is not convex, so its faces have no centre to point away from; its winding
        // is checked by `mesh::tests::the_bunny_winding_is_consistent_and_its_holes_are_counted`.
        let centre_of = |primitive: usize| match primitive {
            0..=3 => None,
            4..=11 => Some(at(cx, cy, cz)),
            12..BUNNY_FIRST_PRIMITIVE => Some(at(0, 0, 0)),
            _ => None,
        };
        for (primitive, triangle) in scene.triangles().iter().enumerate() {
            let index = u32::try_from(primitive).expect("fits");
            let surface = scene.surface(index).expect("no reference triangle is degenerate");
            let normal = surface.frame().normal();
            let expected_axis = match primitive {
                0 | 1 => Some(at(0, U, 0)),
                2 | 3 => Some(at(0, 0, U)),
                _ => None,
            };
            if let Some(axis) = expected_axis {
                assert_eq!(normal, axis, "triangle {primitive}");
            }
            if let Some(centre) = centre_of(primitive) {
                let [a, b, c] = triangle.vertices();
                let outward = a + b + c - centre - centre - centre;
                assert!(normal.dot(outward) > FixedQ32_32::ZERO, "triangle {primitive} faces in");
            }
        }
    }

    #[test]
    fn octahedron_normals_are_the_diagonals() {
        let scene = reference_scene().expect("the reference scene builds");
        // 1 / sqrt(3) = 0.57735026918962576..., times 2^32 = 2479700524.8...
        let third = 2_479_700_525_i64;
        for primitive in 4..12 {
            let normal = scene.surface(primitive).expect("surface").frame().normal();
            for c in [normal.x, normal.y, normal.z] {
                assert!((c.raw().abs() - third).abs() <= 2, "{c:?}");
            }
        }
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
