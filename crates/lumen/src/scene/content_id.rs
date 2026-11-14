//! A scene's content identity: [`SceneContentId`], the SHA-256 of the canonical encoding
//! [`SCENE_CONTENT_ENCODING`] of its [`SceneSpec`] (ADR 0005).

use core::fmt;

use bunny_linalg::FixedVec3;
use bunny_num::FixedQ32_32;

use super::SceneSpec;
use crate::sha256::{hex, Sha256};

#[cfg(test)]
mod tests;

/// The name and version of the byte encoding a [`SceneContentId`] is the digest of.
///
/// `docs/adr/0005-scene-content-identity.md` defines it field by field: the version string, the
/// triangles in primitive order each with its material index, the material table, the light
/// (tagged present or absent), the sky, the camera placement and the vertex normals (a count,
/// zero or one entry per triangle, each vertex tagged present or absent), every integer
/// little-endian and every count a `u64`. A change to the encoding is a new version and changes
/// every digest: version 2 added the vertex normals.
pub const SCENE_CONTENT_ENCODING: &str = "lumen-scene-content/2";

/// What a scene is, by content: the SHA-256 of the [`SCENE_CONTENT_ENCODING`] of the
/// [`SceneSpec`] it was built from. [`Scene::content_id`](super::Scene::content_id) gives it.
///
/// It covers everything a pixel or a ray count depends on (the triangles in primitive order, each
/// one's material index, the albedos, the light, the sky, the camera and the vertex normals) and
/// nothing that is an
/// execution choice (the tree, the thread count, the band height) or a separate provenance field
/// (the image size, the samples, the seed). Two scenes with equal identities render the same
/// bytes under the same settings; two with different identities may still render the same bytes,
/// for example when they differ only in a triangle no ray reaches.
///
/// Its `Display` is `sha256:` and 64 lowercase hexadecimal digits, the form of
/// [`Rgba8Image::hash`](crate::Rgba8Image::hash).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SceneContentId([u8; 32]);

impl SceneContentId {
    /// The 32 bytes of the SHA-256 digest.
    #[must_use]
    pub const fn to_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Display for SceneContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sha256:{}", hex(&self.0))
    }
}

impl fmt::Debug for SceneContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SceneContentId({self})")
    }
}

/// The bytes of one triangle record: nine `i64` coordinates and a `u32` material index.
const TRIANGLE_RECORD: usize = 9 * 8 + 4;

/// The content identity of `spec`, which [`Scene::new`](super::Scene::new) has admitted. It
/// reads the spec and nothing else, so no tree or schedule can enter it. The encoding is
/// streamed into the hasher a field (or one triangle record) at a time; it never exists whole.
pub(super) fn content_id(spec: &SceneSpec<'_>) -> SceneContentId {
    let mut hasher = Sha256::new();
    put_count(&mut hasher, SCENE_CONTENT_ENCODING.len());
    hasher.update(SCENE_CONTENT_ENCODING.as_bytes());
    put_count(&mut hasher, spec.triangles.len());
    for (vertices, material) in spec.triangles.iter().zip(spec.material_of) {
        let mut record = [0_u8; TRIANGLE_RECORD];
        let (coordinate_bytes, material_bytes) = record.split_at_mut(9 * 8);
        let coordinates = vertices.iter().flat_map(|v| components(*v));
        for (slot, coordinate) in coordinate_bytes.chunks_exact_mut(8).zip(coordinates) {
            slot.copy_from_slice(&coordinate.raw().to_le_bytes());
        }
        material_bytes.copy_from_slice(&material.to_le_bytes());
        hasher.update(&record);
    }
    put_count(&mut hasher, spec.materials.len());
    for albedo in spec.materials {
        put_values(&mut hasher, albedo.channels());
    }
    match spec.lighting.sun {
        None => hasher.update(&[0]),
        Some(sun) => {
            hasher.update(&[1]);
            put_values(&mut hasher, components(sun.toward_light()));
            put_values(&mut hasher, sun.irradiance());
        }
    }
    put_values(&mut hasher, spec.lighting.sky.radiance());
    let camera = spec.camera;
    for vector in [camera.position, camera.look, camera.up] {
        put_values(&mut hasher, components(vector));
    }
    put_values(&mut hasher, [camera.tan_half_fov]);
    put_count(&mut hasher, spec.normals.len());
    for normal in spec.normals.iter().flatten() {
        match normal {
            None => hasher.update(&[0]),
            Some(normal) => {
                hasher.update(&[1]);
                put_values(&mut hasher, components(normal.into_inner()));
            }
        }
    }
    SceneContentId(hasher.finish())
}

/// A count or length as a little-endian `u64`.
fn put_count(hasher: &mut Sha256, count: usize) {
    // `usize` is at most 64 bits wide (asserted below), so the conversion never fails and the
    // fallback is never taken.
    hasher.update(&u64::try_from(count).unwrap_or(u64::MAX).to_le_bytes());
}

const _: () = assert!(usize::BITS <= u64::BITS);

/// Q32.32 values, each as the little-endian `i64` of its raw value.
fn put_values<const N: usize>(hasher: &mut Sha256, values: [FixedQ32_32; N]) {
    for value in values {
        hasher.update(&value.raw().to_le_bytes());
    }
}

/// A vector's components in the order `x`, `y`, `z`.
const fn components(vector: FixedVec3) -> [FixedQ32_32; 3] {
    [vector.x, vector.y, vector.z]
}
