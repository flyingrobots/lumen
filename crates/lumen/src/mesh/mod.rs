//! Triangle meshes from files: an interim ASCII PLY reader, exact decimal conversion and
//! placement on the reference scene's vertex grid.
//!
//! This module owns turning mesh bytes into placed scene triangles. [`ply`] reads exactly one
//! ASCII PLY profile with coordinates converted by [`decimal`] (`lumen-ascii-ply-q32.32/1`);
//! [`placement`] puts the mesh on the `2^-16` grid. The reader and the conversion are interim,
//! to be replaced by Bunny's ASCII PLY reader (flyingrobots/bunny#247).

pub mod decimal;
pub mod placement;
pub mod ply;

pub use decimal::{decimal_to_q32_32, DecimalError};
pub use placement::{
    place_mesh, Axis, CoordinateOverflow, MeshPlacement, MeshTurn, PlaceError, PlacedMesh,
};
pub use ply::{parse_ascii_ply, Element, PlyError, PlyMesh};

/// The stable name and version of the conversion rule of [`decimal_to_q32_32`] and
/// [`parse_ascii_ply`]. Interim: to be replaced by Bunny's ASCII PLY reader
/// (flyingrobots/bunny#247).
pub const ASCII_PLY_ID: &str = "lumen-ascii-ply-q32.32/1";
