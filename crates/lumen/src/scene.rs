//! Scenes: triangles, their bounding volume hierarchy, and a camera placement.
//!
//! A [`Scene`] is a list of triangles in primitive order, one geometric normal per triangle,
//! a BVH built over them with Bunny's `build_bvh`, and the placement of a pinhole camera.
//! [`reference_scene`] builds lumen's built-in reference scene, entirely in code.

use core::fmt;

use bunny_broadphase::{build_bvh, closest_hit, BvhNode, BvhView, TraversalError};
use bunny_geom::FixedAabb3;
use bunny_linalg::FixedVec3;
use bunny_num::fixed_q32_32::ONE_RAW;
use bunny_num::FixedQ32_32;
use bunny_optics::{CameraError, CameraSpec, PinholeCamera};
use bunny_query::{TraceRay, TraceTriangle};

/// The stable name and version of [`reference_scene`].
///
/// Any change to the reference scene's triangles, their order, or its camera placement is a
/// new version, and changes every image hash of it.
pub const REFERENCE_SCENE_ID: &str = "lumen-reference-scene/1";

/// The farthest distance a camera ray searches: no limit, raw `i64::MAX`.
const FAR: FixedQ32_32 = FixedQ32_32::from_raw(i64::MAX);

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
    /// `build_bvh` refused the buffers it was given.
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
    /// The hit distance, a multiple of the ray's direction (`TriangleHit::distance`).
    pub distance: FixedQ32_32,
    /// Whether the ray arrives on the side the triangle's normal points to
    /// (`TriangleHit::front_face`).
    pub front_face: bool,
}

/// Why a ray could not be traced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceError {
    /// The traversal refused the tree (`closest_hit`'s error).
    Traversal(TraversalError),
    /// The traversal named a primitive that the triangle list does not hold, or whose hit
    /// could not be repeated. Neither can happen with a tree built by [`Scene::new`].
    InconsistentHit,
}

impl fmt::Display for TraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Traversal(error) => write!(f, "closest-hit traversal failed: {error:?}"),
            Self::InconsistentHit => f.write_str("closest-hit traversal reported no valid hit"),
        }
    }
}

impl std::error::Error for TraceError {}

/// Triangles in primitive order with their normals, BVH and camera placement.
#[derive(Clone, Debug)]
pub struct Scene {
    triangles: Vec<TraceTriangle>,
    normals: Vec<FixedVec3>,
    nodes: Vec<BvhNode>,
    prim_indices: Vec<u32>,
    camera: CameraPlacement,
}

impl Scene {
    /// Builds a scene from triangles, given as vertex triples in primitive order.
    ///
    /// Each triangle's normal is `normalize(cross(v1 - v0, v2 - v0))` with Bunny's
    /// `FixedVec3` operations: every product is a Q32.32 multiplication rounded to nearest
    /// with ties to even, the length is the floor of the exact square root of the raw squares,
    /// and each component is divided by it with the same rounding. Inside the trace domain an
    /// edge is below 512 units on each axis, so no product or difference saturates, and no
    /// normal component exceeds 1.0 in magnitude (each component's magnitude is at most the
    /// floored length). A zero-area triangle gets the zero normal; Bunny's triangle test never
    /// reports a hit on it.
    ///
    /// # Errors
    ///
    /// [`SceneError::Empty`] for no triangles, [`SceneError::TooManyTriangles`] for more than
    /// `u32::MAX`, [`SceneError::OutsideTraceDomain`] for the first triangle Bunny's
    /// `TraceTriangle::new` refuses, [`SceneError::OutOfMemory`] when a buffer cannot be
    /// reserved, and [`SceneError::BvhBuild`] if `build_bvh` refuses its buffers.
    pub fn new(vertices: &[[FixedVec3; 3]], camera: CameraPlacement) -> Result<Self, SceneError> {
        let count = vertices.len();
        if count == 0 {
            return Err(SceneError::Empty);
        }
        if u32::try_from(count).is_err() {
            return Err(SceneError::TooManyTriangles);
        }
        let mut triangles = reserved(count)?;
        let mut normals = reserved(count)?;
        let mut boxes: Vec<FixedAabb3> = reserved(count)?;
        for (primitive, &[v0, v1, v2]) in vertices.iter().enumerate() {
            let triangle = TraceTriangle::new(v0, v1, v2)
                .ok_or(SceneError::OutsideTraceDomain { primitive })?;
            boxes.push(triangle.bounds());
            triangles.push(triangle);
            normals.push((v1 - v0).cross(v2 - v0).normalize().unwrap_or(ZERO_VEC));
        }
        let node_count = count.checked_mul(2).ok_or(SceneError::TooManyTriangles)?;
        let empty = BvhNode {
            bounds: FixedAabb3::new(ZERO_VEC, ZERO_VEC),
            first_child_or_prim_idx: 0,
            prim_count: 0,
        };
        let mut nodes = reserved(node_count)?;
        nodes.resize(node_count, empty);
        let mut prim_indices = reserved(count)?;
        prim_indices.resize(count, 0_u32);
        let used = build_bvh(&mut nodes, &mut prim_indices, &boxes).ok_or(SceneError::BvhBuild)?;
        nodes.truncate(used);
        Ok(Self { triangles, normals, nodes, prim_indices, camera })
    }

    /// The triangles, in primitive order.
    #[must_use]
    pub fn triangles(&self) -> &[TraceTriangle] {
        &self.triangles
    }

    /// The geometric normal of each triangle, in primitive order (see [`Scene::new`]).
    #[must_use]
    pub fn normals(&self) -> &[FixedVec3] {
        &self.normals
    }

    /// The camera placement.
    #[must_use]
    pub const fn camera(&self) -> CameraPlacement {
        self.camera
    }

    /// The nearest triangle `ray` hits at any distance from zero up, or `None` for a miss.
    ///
    /// This is Bunny's `closest_hit` over the scene's BVH with `TraceRay::hit_triangle` as
    /// the primitive test (`t_min` zero, `t_max` the traversal's limit). Of triangles at the
    /// same distance the lowest primitive index wins, and the answer equals a test of every
    /// triangle. The winner is tested once more to read its facing.
    ///
    /// # Errors
    ///
    /// [`TraceError::Traversal`] if the traversal refuses the tree, and
    /// [`TraceError::InconsistentHit`] if the winner cannot be found again; neither happens
    /// for a scene built by [`Scene::new`].
    pub fn closest(&self, ray: &TraceRay) -> Result<Option<Hit>, TraceError> {
        let bvh = BvhView::new(&self.nodes, &self.prim_indices);
        let found = closest_hit(bvh, ray, FAR, |primitive, limit| {
            let triangle = self.triangles.get(usize::try_from(primitive).ok()?)?;
            ray.hit_triangle(triangle, FixedQ32_32::ZERO, limit).map(|hit| hit.distance)
        })
        .map_err(TraceError::Traversal)?;
        let Some(found) = found else { return Ok(None) };
        let triangle = usize::try_from(found.primitive)
            .ok()
            .and_then(|index| self.triangles.get(index))
            .ok_or(TraceError::InconsistentHit)?;
        let hit = ray
            .hit_triangle(triangle, found.distance, found.distance)
            .ok_or(TraceError::InconsistentHit)?;
        Ok(Some(Hit {
            primitive: found.primitive,
            distance: found.distance,
            front_face: hit.front_face,
        }))
    }
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
const GRID_RAW: i64 = 1 << 16;

/// A value of `units / 2^16` units: every scene constant is a whole number of grid steps.
const fn grid(units_times_2_16: i64) -> FixedQ32_32 {
    FixedQ32_32::from_raw(units_times_2_16 * GRID_RAW)
}

/// A point from three coordinates in grid steps of `2^-16` units.
const fn at(x: i64, y: i64, z: i64) -> FixedVec3 {
    FixedVec3::new(grid(x), grid(y), grid(z))
}

/// Whole units in grid steps.
const U: i64 = 1 << 16;

/// The ground's height: -1.125 units, just below the sphere's lowest point (-1.0).
const GROUND_Y: i64 = -U * 9 / 8;
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
const fn snap(value: FixedQ32_32) -> FixedQ32_32 {
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
///    1.0 to 1.04 around the origin, 27 648 triangles.
///
/// Every triangle winds counter-clockwise seen from outside its object (from above for the
/// ground, from the camera's side for the wall), so its normal points outward.
#[must_use]
pub fn reference_triangles() -> Vec<[FixedVec3; 3]> {
    let mut triangles = Vec::with_capacity(4 + 8 + 6 * 48 * 48 * 2);
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
    triangles
}

/// The reference camera: at `(0, 1, 5)`, looking along `(0, -0.25, -1)` (down toward the
/// sphere), up `+Y`, with a vertical field of view whose half-angle has tangent 0.5 (about
/// 53 degrees in all).
#[must_use]
pub const fn reference_camera() -> CameraPlacement {
    CameraPlacement {
        position: at(0, U, 5 * U),
        look: at(0, -U / 4, -U),
        up: at(0, U, 0),
        tan_half_fov: grid(U / 2),
    }
}

/// The built-in reference scene, [`REFERENCE_SCENE_ID`]: [`reference_triangles`] seen by
/// [`reference_camera`].
///
/// # Errors
///
/// [`SceneError::OutOfMemory`] when its buffers cannot be reserved. The other errors of
/// [`Scene::new`] do not occur for this scene; a test checks it builds.
pub fn reference_scene() -> Result<Scene, SceneError> {
    Scene::new(&reference_triangles(), reference_camera())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reference_scene_has_the_stated_triangles() {
        assert_eq!(reference_triangles().len(), 2 + 2 + 8 + 27_648);
        let scene = reference_scene().expect("the reference scene builds");
        assert_eq!(scene.triangles().len(), 27_660);
        // A tree over n primitives uses at most 2n - 1 nodes.
        assert!(scene.nodes.len() < 2 * 27_660);
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
    fn every_normal_points_out_of_its_object() {
        let scene = reference_scene().expect("the reference scene builds");
        let (cx, cy, cz) = OCTAHEDRON_CENTRE;
        let centre_of = |primitive: usize| match primitive {
            0..=3 => None,
            4..=11 => Some(at(cx, cy, cz)),
            _ => Some(at(0, 0, 0)),
        };
        for (primitive, (triangle, normal)) in
            scene.triangles().iter().zip(scene.normals()).enumerate()
        {
            assert_ne!(*normal, ZERO_VEC, "triangle {primitive} is degenerate");
            let expected_axis = match primitive {
                0 | 1 => Some(at(0, U, 0)),
                2 | 3 => Some(at(0, 0, U)),
                _ => None,
            };
            if let Some(axis) = expected_axis {
                assert_eq!(*normal, axis, "triangle {primitive}");
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
        for normal in &scene.normals()[4..12] {
            for c in [normal.x, normal.y, normal.z] {
                assert!((c.raw().abs() - third).abs() <= 2, "{c:?}");
            }
        }
    }

    #[test]
    fn scene_construction_refuses_bad_input() {
        let camera = reference_camera();
        assert_eq!(Scene::new(&[], camera).err(), Some(SceneError::Empty));
        let far = FixedVec3::new(grid(256 * U), FixedQ32_32::ZERO, FixedQ32_32::ZERO);
        let fine = at(0, 0, 0);
        let triangles = [[fine, at(U, 0, 0), at(0, U, 0)], [fine, far, at(0, U, 0)]];
        assert_eq!(
            Scene::new(&triangles, camera).err(),
            Some(SceneError::OutsideTraceDomain { primitive: 1 })
        );
    }
}
