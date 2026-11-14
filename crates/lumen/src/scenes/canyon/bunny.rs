//! The full-resolution Stanford bunny on its ledge, with its smooth-shading normals.

use bunny_linalg::{FixedUnitVec3, FixedVec3};
use bunny_optics::{TriangleSurface, VertexNormals};
use bunny_query::TraceTriangle;

use super::error::CanyonError;
use super::terrain::{grid_point, BUNNY_SPOT, LEDGE_Y};
use crate::mesh::{read_ascii_ply, MeshPlacement, MeshTurn, PlaceError};
use crate::scenes::fixed_point::full_bunny::FULL_BUNNY_PLY;
use crate::vertex_grid::GRID_RAW;

/// The bunny's scale: `2^4`, so it stands about 2.4 units tall.
pub const BUNNY_SHIFT: u32 = 4;

/// The bunny as placed: its kept triangles in the file's face order, their vertex normals, and
/// the faces dropped for zero area.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LedgeBunny {
    /// The triangles of the faces kept.
    pub triangles: Vec<[FixedVec3; 3]>,
    /// Each triangle's vertex normals: Bunny's `VertexNormals` over the placed positions.
    pub normals: Vec<[Option<FixedUnitVec3>; 3]>,
    /// The faces dropped because they have zero area once placed.
    pub dropped: Vec<usize>,
}

/// The full bunny, turned half a turn about `+Y` (the file's bunny faces `-X`; turned, it faces
/// `+X`, across the canyon), scaled by
/// `2^`[`BUNNY_SHIFT`], floored to the grid and moved by whole grid steps so that its lowest
/// vertex is exactly at [`LEDGE_Y`] and the middle of its footprint (half the sum of its
/// lowest and highest X and Z, floored) is at [`BUNNY_SPOT`] on the ledge.
///
/// # Errors
///
/// [`CanyonError::Bunny`] if the embedded file does not read or a face names a missing vertex,
/// [`CanyonError::Overflow`] if a placed vertex overflows, and [`CanyonError::Normals`] if
/// Bunny refuses the placed mesh; a test shows none of these happens.
pub fn ledge_bunny() -> Result<LedgeBunny, CanyonError> {
    let mesh = read_ascii_ply(FULL_BUNNY_PLY.as_bytes())
        .map_err(|error| CanyonError::Bunny(PlaceError::Ply(error)))?;
    let mut used = vec![false; mesh.vertices.len()];
    for (face, indices) in mesh.faces.iter().enumerate() {
        for &index in indices {
            let slot = usize::try_from(index).ok().and_then(|i| used.get_mut(i));
            *slot.ok_or(CanyonError::Bunny(PlaceError::MissingVertex { face }))? = true;
        }
    }
    let scaled =
        MeshPlacement { turn: MeshTurn::HalfAboutY, shift: BUNNY_SHIFT, offset_steps: [0; 3] };
    let mut placed: Vec<[i64; 3]> = Vec::with_capacity(mesh.vertices.len());
    for &vertex in &mesh.vertices {
        let p = scaled.place(vertex).map_err(|_| CanyonError::Overflow)?;
        placed.push([p.x, p.y, p.z].map(|c| c.raw() / GRID_RAW));
    }
    let named = || placed.iter().zip(&used).filter(|(_, &u)| u).map(|(p, _)| *p);
    let low = |k: usize| named().map(|p| p[k]).min().ok_or(CanyonError::EmptyBunny);
    let high = |k: usize| named().map(|p| p[k]).max().ok_or(CanyonError::EmptyBunny);
    let spot = grid_point(BUNNY_SPOT.0, BUNNY_SPOT.1, LEDGE_Y);
    let [spot_x, spot_z] = [spot.x, spot.z].map(|c| c.raw() / GRID_RAW);
    let offset = [
        spot_x - i64::midpoint(low(0)?, high(0)?),
        LEDGE_Y - low(1)?,
        spot_z - i64::midpoint(low(2)?, high(2)?),
    ];
    let positions: Vec<FixedVec3> = placed
        .iter()
        .map(|p| crate::vertex_grid::at(p[0] + offset[0], p[1] + offset[1], p[2] + offset[2]))
        .collect();
    let mut kept = Vec::with_capacity(mesh.faces.len());
    let mut dropped = Vec::new();
    for (index, &face) in mesh.faces.iter().enumerate() {
        let [a, b, c] = face.map(|i| usize::try_from(i).ok().and_then(|i| positions.get(i)));
        let surface = a
            .zip(b)
            .zip(c)
            .and_then(|((a, b), c)| TraceTriangle::new(*a, *b, *c))
            .and_then(|t| TriangleSurface::new(&t));
        match surface {
            Some(_) => kept.push(face),
            None => dropped.push(index),
        }
    }
    let vertex_normals = VertexNormals::new(&positions, &kept).map_err(CanyonError::Normals)?;
    let mut triangles = Vec::with_capacity(kept.len());
    let mut normals = Vec::with_capacity(kept.len());
    for face in &kept {
        let lookup = |i: u32| usize::try_from(i).ok();
        let corner = |i: u32| lookup(i).and_then(|i| positions.get(i)).copied();
        let normal =
            |i: u32| lookup(i).and_then(|i| vertex_normals.normals().get(i)).copied().flatten();
        let [a, b, c] = face.map(corner);
        triangles.push([
            a.ok_or(CanyonError::EmptyBunny)?,
            b.ok_or(CanyonError::EmptyBunny)?,
            c.ok_or(CanyonError::EmptyBunny)?,
        ]);
        normals.push(face.map(normal));
    }
    Ok(LedgeBunny { triangles, normals, dropped })
}
