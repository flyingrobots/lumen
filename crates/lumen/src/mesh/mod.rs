//! Triangle meshes from files: reading with Bunny's ASCII PLY reader and placement on the
//! reference scene's vertex grid.
//!
//! [`loaded`] gathers the mesh that Bunny's `bunny_codec::parse_ascii_ply` reads (profile
//! `bunny-ascii-ply-triangle-mesh/1`, decimals converted by `bunny-decimal-q32-32/1`);
//! [`placement`] puts it on the `2^-16` grid.

pub mod loaded;
pub mod placement;

pub use loaded::{read_ascii_ply, PlyMesh};
pub use placement::{
    place_mesh, Axis, CoordinateOverflow, MeshPlacement, MeshTurn, PlaceError, PlacedMesh,
};
