//! The shapes of the reference scene defined in code: the ground, the back wall, the octahedron and
//! the bumpy sphere.

use bunny_linalg::FixedVec3;
use bunny_num::fixed_q32_32::ONE_RAW;
use bunny_num::FixedQ32_32;

use crate::vertex_grid::{at, snap, U};

/// Quads along each edge of each cube face of the sphere: 6 * 48 * 48 * 2 = 27 648 triangles.
const SPHERE_GRID: i64 = 48;

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
pub(crate) const OCTAHEDRON_CENTRE: (i64, i64, i64) = (U * 9 / 4, GROUND_Y + U, U / 2);

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

/// Appends the ground (2 triangles), the wall (2), the octahedron (8) and the sphere (27 648) to
/// `triangles`, in that order. Each winds counter-clockwise seen from outside its object (from
/// above for the ground, from the camera's side for the wall).
pub(crate) fn push_shapes(triangles: &mut Vec<[FixedVec3; 3]>) {
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
}
