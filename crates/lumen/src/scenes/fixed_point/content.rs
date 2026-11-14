//! The Fixed Point scene's triangle list: every triangle in primitive order, the named part
//! it belongs to, and the material that part is made of.

use core::ops::Range;

use bunny_linalg::FixedVec3;

use super::solids::StepTriangle;
use crate::vertex_grid::at;

/// Which side of the bunny's face, as the camera sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    /// The camera's left (`-X`).
    Left,
    /// The camera's right (`+X`).
    Right,
}

/// One of the two dice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Die {
    /// The die on the camera's left.
    Left,
    /// The die on the camera's right.
    Right,
}

/// A named part of the scene. Every part is one material ([`PartKind::material`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PartKind {
    /// The green felt bed of the table.
    Felt,
    /// The padded rail around the felt.
    Rail,
    /// The white layout lines: the number boxes' outlines and the long lines.
    LayoutLines,
    /// The block numeral of the number box for this number (4, 5, 6, 8, 9 or 10).
    Numeral(u8),
    /// The puck's white top.
    PuckFace,
    /// The puck's black rim and side.
    PuckRim,
    /// The black letters "ON" on the puck.
    PuckLetters,
    /// A die's red body.
    DieBody(Die),
    /// A die's white pips.
    Pips(Die),
    /// The Stanford bunny.
    Bunny,
    /// A lens of the sunglasses.
    Lens(Side),
    /// The bridge of the sunglasses, between the lenses.
    Bridge,
    /// An arm of the sunglasses.
    Arm(Side),
}

/// The scene's materials, in the order of `FIXED_POINT_ALBEDO_256`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Material {
    /// Green felt.
    Felt,
    /// Dark brown leather.
    Leather,
    /// White layout paint.
    Chalk,
    /// Red die.
    DieRed,
    /// White pip and puck face.
    White,
    /// Black puck rim and letters.
    Black,
    /// The bunny's light grey.
    Fur,
    /// The sunglasses' near black.
    Shades,
}

impl Material {
    /// The material's index into `FIXED_POINT_ALBEDO_256`.
    #[must_use]
    pub const fn index(self) -> u32 {
        match self {
            Self::Felt => 0,
            Self::Leather => 1,
            Self::Chalk => 2,
            Self::DieRed => 3,
            Self::White => 4,
            Self::Black => 5,
            Self::Fur => 6,
            Self::Shades => 7,
        }
    }
}

impl PartKind {
    /// The material the part is made of.
    #[must_use]
    pub const fn material(self) -> Material {
        match self {
            Self::Felt => Material::Felt,
            Self::Rail => Material::Leather,
            Self::LayoutLines | Self::Numeral(_) => Material::Chalk,
            Self::PuckFace | Self::Pips(_) => Material::White,
            Self::PuckRim | Self::PuckLetters => Material::Black,
            Self::DieBody(_) => Material::DieRed,
            Self::Bunny => Material::Fur,
            Self::Lens(_) | Self::Bridge | Self::Arm(_) => Material::Shades,
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

/// The scene's triangles in primitive order, the material index of each, and the parts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FixedPointContent {
    /// The triangles, in primitive order.
    pub triangles: Vec<[FixedVec3; 3]>,
    /// The material index of each triangle.
    pub material_of: Vec<u32>,
    /// The parts, in primitive order, covering every triangle once.
    pub parts: Vec<Part>,
}

impl FixedPointContent {
    /// The triangles of the first part of `kind`, or an empty slice if there is none.
    #[must_use]
    pub fn part(&self, kind: PartKind) -> &[[FixedVec3; 3]] {
        self.parts
            .iter()
            .find(|part| part.kind == kind)
            .and_then(|part| self.triangles.get(part.primitives.clone()))
            .unwrap_or(&[])
    }

    /// Appends a part made of triangles in grid steps.
    pub(crate) fn push_steps(&mut self, kind: PartKind, triangles: &[StepTriangle]) {
        let placed: Vec<[FixedVec3; 3]> =
            triangles.iter().map(|t| t.map(|[x, y, z]| at(x, y, z))).collect();
        self.push(kind, &placed);
    }

    /// Appends a part made of placed triangles.
    pub(crate) fn push(&mut self, kind: PartKind, triangles: &[[FixedVec3; 3]]) {
        let first = self.triangles.len();
        self.triangles.extend_from_slice(triangles);
        let material = kind.material().index();
        self.material_of.resize(self.triangles.len(), material);
        self.parts.push(Part { kind, primitives: first..self.triangles.len() });
    }
}
