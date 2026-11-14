//! Tests of the embedded full-resolution bunny: its bytes, its counts, its bounds and its
//! topology as read by Bunny's reader.
//!
//! Oracle for the counts and the topology: Stanford's description of the file (35 947 vertices,
//! 69 451 faces) and an independent count of the same file by a separate program (Python, on
//! the file's text, with exact decimals), recorded in `assets/stanford-bunny/README.md`.

use std::collections::{BTreeMap, BTreeSet};

use bunny_linalg::FixedVec3;

use super::*;
use crate::mesh::{read_ascii_ply, PlyMesh};

fn mesh() -> PlyMesh {
    read_ascii_ply(FULL_BUNNY_PLY.as_bytes()).expect("Bunny's reader reads the full bunny")
}

#[test]
fn the_committed_asset_has_the_archives_hash_and_size() {
    let digest = crate::sha256::digest(FULL_BUNNY_PLY.as_bytes());
    assert_eq!(crate::sha256::hex(&digest), FULL_BUNNY_SHA256);
    assert_eq!(
        FULL_BUNNY_SHA256,
        "b1acc63bece78444aa2e15bdcc72371a201279b98c6f5d4b74c993d02f0566fe"
    );
    assert_eq!(FULL_BUNNY_PLY.len(), 3_033_195);
}

#[test]
fn the_full_bunny_reads_with_stanfords_counts() {
    let mesh = mesh();
    assert_eq!(mesh.vertices.len(), FULL_BUNNY_VERTEX_COUNT);
    assert_eq!(mesh.faces.len(), FULL_BUNNY_FACE_COUNT);
    assert_eq!((FULL_BUNNY_VERTEX_COUNT, FULL_BUNNY_FACE_COUNT), (35_947, 69_451));
}

/// The bounds are the file's own extreme decimal strings (found by the separate count),
/// converted by Bunny's decimal routine: the reader's conversion of every vertex agrees.
#[test]
fn the_full_bunny_bounds_are_its_extreme_decimals() {
    let decimal = |values: [&str; 3]| {
        let [x, y, z] =
            values.map(|v| bunny_codec::parse_decimal_q32_32(v.as_bytes()).expect("a decimal"));
        FixedVec3::new(x, y, z)
    };
    let (lo, hi) = mesh().bounds().expect("vertices");
    assert_eq!(lo, decimal(["-0.0946899", "0.0329874", "-0.0618736"]));
    assert_eq!(hi, decimal(["0.0610091", "0.187321", "0.0587997"]));
}

/// No two vertices share a position, 1 113 vertices are named by no face, and no face repeats
/// an index or another face's vertices.
#[test]
fn the_full_bunny_has_no_duplicate_vertex_or_face() {
    let mesh = mesh();
    let positions: BTreeSet<[i64; 3]> =
        mesh.vertices.iter().map(|v| [v.x.raw(), v.y.raw(), v.z.raw()]).collect();
    assert_eq!(positions.len(), mesh.vertices.len());
    let used: BTreeSet<u32> = mesh.faces.iter().flatten().copied().collect();
    assert_eq!(mesh.vertices.len() - used.len(), 1_113);
    assert!(mesh.faces.iter().all(|&[a, b, c]| a != b && b != c && c != a));
    let sets: BTreeSet<[u32; 3]> = mesh
        .faces
        .iter()
        .map(|&face| {
            let mut key = face;
            key.sort_unstable();
            key
        })
        .collect();
    assert_eq!(sets.len(), mesh.faces.len());
}

/// The surface is manifold and consistently wound: every edge belongs to one or two faces, and
/// two faces that share an edge traverse it in opposite directions. Its 223 boundary edges are
/// the holes in its base (five loops, of 80, 42, 40, 39 and 22 edges).
#[test]
fn the_full_bunny_is_manifold_consistently_wound_and_has_five_holes() {
    let mesh = mesh();
    let mut directed: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for &[a, b, c] in &mesh.faces {
        for edge in [(a, b), (b, c), (c, a)] {
            *directed.entry(edge).or_default() += 1;
        }
    }
    assert!(directed.values().all(|&n| n == 1), "a directed edge is used twice");
    let boundary: BTreeMap<u32, u32> = directed
        .keys()
        .filter(|&&(a, b)| !directed.contains_key(&(b, a)))
        .map(|&(a, b)| (a, b))
        .collect();
    assert_eq!(boundary.len(), 223);
    assert_eq!(directed.len() - boundary.len(), 2 * (104_288 - 223));
    // Each boundary vertex starts exactly one boundary edge, so the edges form loops.
    let mut seen = BTreeSet::new();
    let mut loops = Vec::new();
    for &start in boundary.keys() {
        let mut length = 0;
        let mut at = start;
        while seen.insert(at) {
            length += 1;
            at = *boundary.get(&at).expect("a boundary edge continues the loop");
        }
        if length > 0 {
            loops.push(length);
        }
    }
    loops.sort_unstable();
    assert_eq!(loops, vec![22, 39, 40, 42, 80]);
}
