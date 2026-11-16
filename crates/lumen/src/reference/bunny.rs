//! The Stanford bunny in the reference scene: the embedded asset and its placement.

#[cfg(test)]
mod tests;

use crate::mesh::{place_mesh, read_ascii_ply, MeshPlacement, MeshTurn, PlaceError, PlacedMesh};

/// The Stanford bunny, `assets/stanford-bunny/bun_zipper_res3.ply`, embedded byte for byte.
///
/// Stanford Computer Graphics Laboratory, Stanford 3D Scanning Repository; lumen's copy is
/// Geordi's checked-in asset (see `assets/stanford-bunny/README.md`).
pub const STANFORD_BUNNY_PLY: &str =
    include_str!("../../../../assets/stanford-bunny/bun_zipper_res3.ply");

/// The SHA-256 of [`STANFORD_BUNNY_PLY`], the same as Geordi's asset hash.
pub const STANFORD_BUNNY_SHA256: &str =
    "975e7f9b160b4ea15b0e225e21b10828ebcf678df020d2f6a46aa408fdcf5cd6";

/// The bunny file's face count.
pub const BUNNY_FACE_COUNT: usize = 3851;

/// The bunny's faces that are zero-area after placement and are dropped: none. Every face of
/// `bun_zipper_res3.ply` keeps a nonzero area after the conversion, the scale and the floor to
/// the `2^-16` grid (the smallest has about `2.2e-5` square units); a test checks the count.
pub const BUNNY_DROPPED_FACES: usize = 0;

/// The bunny's triangles in the reference scene.
pub const BUNNY_TRIANGLE_COUNT: usize = BUNNY_FACE_COUNT - BUNNY_DROPPED_FACES;

/// The bunny's placement in the reference scene ([`crate::reference_scene`]): a quarter turn
/// about `+Y` (the file's bunny faces `-X`; turned, it faces `+Z`, toward the camera), a scale of
/// 8 (about 0.96 units wide, 1.21 tall and 1.24 deep), and a translation that puts it left of the
/// sphere, its lowest vertex exactly on the ground plane, the centre of its box where version 3's
/// was: the bunny turns where it stands.
///
/// The offsets in grid steps of `2^-16` units:
///
/// - X: `-146 044`, `-2.228` units, so it spans `x = -2.7218..=-1.7607`, centred on `-2.2412`;
/// - Y: `-91 246`. The lowest vertex (index 1748, `y = 0.0334143`, raw `143 513 326`) scales to
///   raw `1 148 106 608`, which floors to `17 518` grid steps; `GROUND_Y` is `-73 728` steps, and
///   `-73 728 - 17 518 = -91 246`. So that vertex lies exactly on the ground (`y = -1.125`): the
///   bunny stands on the plane rather than floating a hair above it, and Bunny's start-point
///   rule handles the touching surfaces;
/// - Z: `24 845`, `0.379` units, so it spans `z = -0.1084..=1.1340`, centred on `0.5128`, about
///   the octahedron's depth.
///
/// Version 3 of the scene turned it half a turn, with offsets `(-155 648, -91 246, 32 768)`, so
/// the bunny faced `+X`, toward the sphere, and the camera saw it in profile; the X and Z offsets
/// above keep the centre of its box within a grid step of that version's.
pub const BUNNY_PLACEMENT: MeshPlacement = MeshPlacement {
    turn: MeshTurn::QuarterAboutY,
    shift: 3,
    offset_steps: [-146_044, -91_246, 24_845],
};

/// The Stanford bunny (`assets/stanford-bunny/bun_zipper_res3.ply`, read by
/// [`read_ascii_ply`]) placed by [`BUNNY_PLACEMENT`]: 3 851 triangles, none dropped.
///
/// The file's winding is consistent: every edge shared by exactly two faces is traversed in
/// opposite directions by them (a test checks it), and the faces wind counter-clockwise seen from
/// outside. It is not a closed surface: Stanford's decimation does not preserve topology, so 60
/// edges belong to one face only (four holes, three in the base and a small one low on the
/// chest), 141 edges are shared by three or more faces, and 83 faces repeat another face's
/// vertices (14 in the same order, 69 reversed). None of this matters to two-sided shading: a
/// duplicate face is a coplanar tie that the lower primitive index wins.
///
/// # Errors
///
/// A [`PlaceError`] if the embedded file is refused or does not place; neither happens (a test
/// checks it).
pub fn bunny_triangles() -> Result<PlacedMesh, PlaceError> {
    let mesh = read_ascii_ply(STANFORD_BUNNY_PLY.as_bytes()).map_err(PlaceError::Ply)?;
    place_mesh(&mesh, &BUNNY_PLACEMENT)
}
