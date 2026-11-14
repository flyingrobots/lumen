//! Tests of the embedded bunny: its bytes, its conversion and its placement.

use bunny_linalg::FixedVec3;

use super::*;
use crate::mesh::read_ascii_ply;
use crate::reference::shapes::GROUND_Y;
use crate::reference::{reference_scene, REFERENCE_TRIANGLE_COUNT};
use crate::vertex_grid::{grid, GRID_RAW, U};

fn bunny_raw_bounds(triangles: &[[FixedVec3; 3]]) -> ([i64; 3], [i64; 3]) {
    let mut lo = [i64::MAX; 3];
    let mut hi = [i64::MIN; 3];
    for v in triangles.iter().flatten() {
        for (axis, c) in [v.x, v.y, v.z].into_iter().enumerate() {
            lo[axis] = lo[axis].min(c.raw() / GRID_RAW);
            hi[axis] = hi[axis].max(c.raw() / GRID_RAW);
        }
    }
    (lo, hi)
}

/// The placed bunny's bounds in grid steps, derived by hand from the converted file bounds
/// (`mesh::loaded::tests`): coordinate `c` is placed at `floor(s * c * 8 / 2^16) + offset`, with
/// `s = -1` on X and Z (the half turn).
///
/// - min X from the file's max X, raw `261 712 114`: `-2 093 696 912 / 65 536 = -31 947.28`,
///   floor `-31 948`, plus `-155 648`: `-187 596`;
/// - max X from the file's min X, raw `-405 291 582`: `3 242 332 656 / 65 536 = 49 474.07`,
///   floor `49 474`, plus `-155 648`: `-106 174`;
/// - min Y, raw `143 513 326`: `1 148 106 608 / 65 536 = 17 518.72`, floor `17 518`, plus
///   `-91 246`: `-73 728`, which is `GROUND_Y`: the lowest vertex is on the ground;
/// - max Y, raw `793 765 791`: `6 350 126 328 / 65 536 = 96 895.24`, floor `96 895`, plus
///   `-91 246`: `5 649`;
/// - min Z from the file's max Z, raw `251 105 692`: `-2 008 845 536 / 65 536 = -30 652.55`,
///   floor `-30 653`, plus `32 768`: `2 115`;
/// - max Z from the file's min Z, raw `-264 879 653`: `2 119 037 224 / 65 536 = 32 333.94`,
///   floor `32 333`, plus `32 768`: `65 101`.
#[test]
fn the_bunny_is_placed_where_stated() {
    let bunny = bunny_triangles().expect("the bunny reads and places");
    let (lo, hi) = bunny_raw_bounds(&bunny.triangles);
    assert_eq!(lo, [-187_596, -73_728, 2_115]);
    assert_eq!(hi, [-106_174, 5_649, 65_101]);
    assert_eq!(lo[1], GROUND_Y);
    // Exactly one vertex rests on the ground: the file's vertex 1748.
    let mesh = read_ascii_ply(STANFORD_BUNNY_PLY.as_bytes()).expect("reads");
    let on_ground: Vec<usize> = (0..mesh.vertices.len())
        .filter(|&i| BUNNY_PLACEMENT.place(mesh.vertices[i]).expect("places").y == grid(GROUND_Y))
        .collect();
    assert_eq!(on_ground, vec![1748]);
    // Clear of the sphere (|x| <= 1.04) and of the octahedron (x >= 1.25), inside the
    // ground and in front of the wall.
    assert!(hi[0] < -U * 3 / 2 && lo[0] > -3 * U && lo[2] > 0 && hi[2] < U);
}

#[test]
fn the_bunny_keeps_every_face_and_none_is_degenerate() {
    let bunny = bunny_triangles().expect("the bunny reads and places");
    assert_eq!(bunny.dropped, Vec::<usize>::new());
    assert_eq!(bunny.dropped.len(), BUNNY_DROPPED_FACES);
    assert_eq!(bunny.triangles.len(), BUNNY_TRIANGLE_COUNT);
    assert_eq!(bunny.triangles.len(), 3851);
    // An independent exact test: the cross product of two edges is nonzero.
    let steps = |v: FixedVec3| [v.x, v.y, v.z].map(|c| i128::from(c.raw() / GRID_RAW));
    for (index, &[v0, v1, v2]) in bunny.triangles.iter().enumerate() {
        let (p0, p1, p2) = (steps(v0), steps(v1), steps(v2));
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let normal = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        assert_ne!(normal, [0; 3], "bunny triangle {index} has zero area");
    }
    // Every reference triangle has a surface: no zero-area triangle reaches the scene.
    let scene = reference_scene().expect("builds");
    for primitive in 0..u32::try_from(REFERENCE_TRIANGLE_COUNT).expect("fits") {
        let index = crate::scene::PrimitiveIndex::new(primitive);
        assert!(scene.surface(index).is_some(), "primitive {primitive}");
    }
}

/// The bunny's winding is consistent: an edge shared by exactly two faces is traversed in
/// opposite directions by them. Its holes and its non-manifold edges are counted as stated
/// in [`bunny_triangles`], and its faces wind counter-clockwise seen from outside: the signed
/// volume they enclose is positive (about 0.347 cubic units after placement).
#[test]
fn the_bunny_winding_is_consistent_and_its_holes_are_counted() {
    use std::collections::HashMap;
    let mesh = read_ascii_ply(STANFORD_BUNNY_PLY.as_bytes()).expect("reads");
    let mut directed: HashMap<(u32, u32), u32> = HashMap::new();
    for &[a, b, c] in &mesh.faces {
        for edge in [(a, b), (b, c), (c, a)] {
            *directed.entry(edge).or_default() += 1;
        }
    }
    let mut undirected: HashMap<(u32, u32), u32> = HashMap::new();
    for (&(a, b), &n) in &directed {
        *undirected.entry((a.min(b), a.max(b))).or_default() += n;
    }
    let (mut boundary, mut shared_by_more, mut inconsistent) = (0, 0, 0);
    for (&(a, b), &n) in &undirected {
        match n {
            1 => boundary += 1,
            2 => {
                if directed.get(&(a, b)) != Some(&1) {
                    inconsistent += 1;
                }
            }
            _ => shared_by_more += 1,
        }
    }
    assert_eq!((boundary, shared_by_more, inconsistent), (60, 141, 0));
    let mut faces: HashMap<[u32; 3], u32> = HashMap::new();
    for &face in &mesh.faces {
        let mut key = face;
        key.sort_unstable();
        *faces.entry(key).or_default() += 1;
    }
    let repeats: u32 = faces.values().map(|n| n - 1).sum();
    assert_eq!(repeats, 83);
    let bunny = bunny_triangles().expect("places");
    let steps = |v: FixedVec3| [v.x, v.y, v.z].map(|c| i128::from(c.raw() / GRID_RAW));
    let six_volume: i128 = bunny
        .triangles
        .iter()
        .map(|&[a, b, c]| {
            let (a, b, c) = (steps(a), steps(b), steps(c));
            a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0])
        })
        .sum();
    // In grid steps cubed: 6 * 0.347 * 2^48 is about 5.86e14.
    assert_eq!(six_volume, 585_898_999_030_544);
}

#[test]
fn the_stanford_bunny_reads_with_its_declared_counts() {
    let mesh = read_ascii_ply(STANFORD_BUNNY_PLY.as_bytes()).expect("the bunny reads");
    assert_eq!(mesh.vertices.len(), 1889);
    assert_eq!(mesh.faces.len(), 3851);
    let first = mesh.vertices[0];
    assert_eq!(
        [first.x.raw(), first.y.raw(), first.z.raw()],
        [-158_536_692, 547_659_870, 11_886_623]
    );
}

/// The manifest's bounds (`bunny.mesh.json` in Geordi), the same decimal strings as in the
/// file, converted by the same routine, equal the converted mesh's bounds exactly.
#[test]
fn the_bunny_bounds_equal_the_manifest_bounds_exactly() {
    let manifest = |values: [&str; 3]| {
        let [x, y, z] =
            values.map(|v| bunny_codec::parse_decimal_q32_32(v.as_bytes()).expect("a decimal"));
        FixedVec3::new(x, y, z)
    };
    let mesh = read_ascii_ply(STANFORD_BUNNY_PLY.as_bytes()).expect("the bunny reads");
    let (lo, hi) = mesh.bounds().expect("vertices");
    assert_eq!(lo, manifest(["-0.0943643", "0.0334143", "-0.0616721"]));
    assert_eq!(hi, manifest(["0.0609346", "0.184813", "0.0584651"]));
}

#[test]
fn the_committed_asset_has_geordis_hash() {
    let digest = crate::sha256::digest(STANFORD_BUNNY_PLY.as_bytes());
    assert_eq!(crate::sha256::hex(&digest), STANFORD_BUNNY_SHA256);
    assert_eq!(
        STANFORD_BUNNY_SHA256,
        "975e7f9b160b4ea15b0e225e21b10828ebcf678df020d2f6a46aa408fdcf5cd6"
    );
}
