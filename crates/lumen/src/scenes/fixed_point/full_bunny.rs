//! The full-resolution Stanford bunny, `assets/stanford-bunny/bun_zipper.ply`, embedded byte for
//! byte, and the facts about the file that its tests pin.
//!
//! This module owns the asset. It is Stanford's data, not lumen's (see
//! `assets/stanford-bunny/README.md` for the source, the attribution and the hashes). It is
//! read with Bunny's ASCII PLY reader, [`read_ascii_ply`](crate::mesh::read_ascii_ply), like the
//! decimated bunny of the reference scene.
//!
//! The bytes are only linked into a program that uses this constant: a binary that never
//! reaches it (one that renders only the reference scene) does not carry them.

#[cfg(test)]
mod tests;

/// The full-resolution Stanford bunny, `assets/stanford-bunny/bun_zipper.ply`: the archive's
/// `bunny/reconstruction/bun_zipper.ply`, 3 033 195 bytes, unchanged (its data lines end in a
/// space, which Bunny's reader accepts).
///
/// Stanford Computer Graphics Laboratory, Stanford 3D Scanning Repository.
pub const FULL_BUNNY_PLY: &str =
    include_str!("../../../../../assets/stanford-bunny/bun_zipper.ply");

/// The SHA-256 of [`FULL_BUNNY_PLY`], the same as the file in Stanford's archive.
pub const FULL_BUNNY_SHA256: &str =
    "b1acc63bece78444aa2e15bdcc72371a201279b98c6f5d4b74c993d02f0566fe";

/// The file's vertex count, as Stanford's archive describes it.
pub const FULL_BUNNY_VERTEX_COUNT: usize = 35_947;

/// The file's face count, as Stanford's archive describes it.
pub const FULL_BUNNY_FACE_COUNT: usize = 69_451;
