//! The canyon scene's triangle list: every triangle in primitive order, the named part it
//! belongs to, its material and its vertex normals.

use core::ops::Range;

use bunny_linalg::{FixedUnitVec3, FixedVec3};

/// One of the three craft.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Craft {
    /// The skiff: a long hexagonal hull, straight wings and a fin, in orange.
    Skiff,
    /// The kite: a flat diamond wing with a raised spine and twin engines, in teal.
    Kite,
    /// The tug: a wedge hull between two pontoons joined by a short wing, in pale cream.
    Tug,
}

impl Craft {
    /// The three craft: the two followers, then the leader.
    pub const ALL: [Self; 3] = [Self::Skiff, Self::Kite, Self::Tug];
}

/// A named part of the scene. Every part is one material ([`PartKind::material`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PartKind {
    /// The heightfield: two triangles per grid cell, row by row, each coloured by its rock
    /// stratum (`terrain::stratum_material`): the one part made of more than one material.
    Terrain,
    /// A craft's painted hull, wings and fins.
    Hull(Craft),
    /// A craft's dark engine blocks.
    Engine(Craft),
    /// The full-resolution Stanford bunny, on its ledge.
    Bunny,
    /// Latitude band `n` of the planet, from its south pole (band 0) north.
    Band(u8),
}

/// The number of latitude bands on the planet, each its own material.
pub const PLANET_BANDS: u8 = 8;

impl PartKind {
    /// The index of the part's material in the scene's material table
    /// (`look::CANYON_ALBEDO_256`): for the terrain, the first of its three rock strata.
    #[must_use]
    pub fn material(self) -> u32 {
        match self {
            Self::Terrain => 0,
            Self::Hull(Craft::Skiff) => 3,
            Self::Hull(Craft::Kite) => 4,
            Self::Hull(Craft::Tug) => 5,
            Self::Engine(_) => 6,
            Self::Bunny => 7,
            // Bands are 0..PLANET_BANDS, so this is 8..=15.
            Self::Band(n) => 8 + u32::from(n),
        }
    }
}

/// A part and the primitive indices of its triangles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    /// What the part is.
    pub kind: PartKind,
    /// Its triangles' primitive indices, consecutive.
    pub primitives: Range<usize>,
}

/// The scene's triangles in primitive order, the material index and vertex normals of each,
/// and the parts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CanyonContent {
    /// The triangles, in primitive order.
    pub triangles: Vec<[FixedVec3; 3]>,
    /// The material index of each triangle.
    pub material_of: Vec<u32>,
    /// The vertex normals of each triangle: the bunny's, from Bunny's `VertexNormals`; none for
    /// every other part, which is shaded flat.
    pub normals: Vec<[Option<FixedUnitVec3>; 3]>,
    /// The parts, in primitive order, covering every triangle once.
    pub parts: Vec<Part>,
}

impl CanyonContent {
    /// The triangles of the first part of `kind`, or an empty slice if there is none.
    #[must_use]
    pub fn part(&self, kind: PartKind) -> &[[FixedVec3; 3]] {
        self.parts
            .iter()
            .find(|part| part.kind == kind)
            .and_then(|part| self.triangles.get(part.primitives.clone()))
            .unwrap_or(&[])
    }

    /// Appends a part shaded flat.
    pub(crate) fn push(&mut self, kind: PartKind, triangles: &[[FixedVec3; 3]]) {
        self.push_smooth(kind, triangles, &[]);
    }

    /// Appends a part with its vertex normals (one entry per triangle, or none).
    pub(crate) fn push_smooth(
        &mut self,
        kind: PartKind,
        triangles: &[[FixedVec3; 3]],
        normals: &[[Option<FixedUnitVec3>; 3]],
    ) {
        let first = self.triangles.len();
        self.triangles.extend_from_slice(triangles);
        self.normals.extend_from_slice(normals);
        self.normals.resize(self.triangles.len(), [None; 3]);
        self.material_of.resize(self.triangles.len(), kind.material());
        self.parts.push(Part { kind, primitives: first..self.triangles.len() });
    }
}
