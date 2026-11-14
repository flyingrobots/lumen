//! Why a heightfield spec was refused: [`HeightfieldError`], and the checks that keep every step
//! of the generator inside `i64`.

use core::fmt;

use super::{HeightfieldSpec, NoiseLayer, MAX_AMPLITUDE, MAX_SPACING, MAX_VERTICES};

/// Why [`generate`](super::generate) refused a spec. Each variant names the parameter and what
/// was given.
///
/// Non-exhaustive: a later generator version may refuse a spec for a new reason, so a `match`
/// outside this crate needs a wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HeightfieldError {
    /// The grid has no vertices, or more than [`MAX_VERTICES`].
    GridSize {
        /// The columns given.
        columns: u32,
        /// The rows given.
        rows: u32,
    },
    /// More noise layers than [`MAX_SPACING`].
    TooManyLayers {
        /// The layers given.
        layers: usize,
    },
    /// A noise layer (`Some(k)`, counting from 0) or the meander (`None`) has a spacing outside
    /// `1..=MAX_SPACING` or an amplitude outside `0..=MAX_AMPLITUDE`.
    Layer {
        /// Which layer.
        layer: Option<usize>,
        /// Its spacing.
        spacing: u32,
        /// Its amplitude.
        amplitude: i64,
    },
    /// The terraces have a width outside `1..=MAX_SPACING`, a riser outside `1..=width`, a
    /// count above `MAX_SPACING` or a height outside `0..=MAX_AMPLITUDE`.
    Terraces,
    /// A noise weight is outside `0..=16`.
    Weight {
        /// The weight given.
        weight: i64,
    },
    /// A shelf (counting from 0) is not inside the grid, has its first column or row after its
    /// last, or has a height outside `-MAX_AMPLITUDE..=MAX_AMPLITUDE`.
    Shelf {
        /// Which shelf.
        shelf: usize,
    },
    /// The heights could not be reserved.
    OutOfMemory {
        /// The vertices asked for.
        vertices: usize,
    },
}

impl fmt::Display for HeightfieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GridSize { columns, rows } => write!(
                f,
                "a grid of {columns} by {rows} vertices is outside 1..={MAX_VERTICES} vertices"
            ),
            Self::TooManyLayers { layers } => {
                write!(f, "{layers} noise layers is more than {MAX_SPACING}")
            }
            Self::Layer { layer, spacing, amplitude } => {
                match layer {
                    Some(k) => write!(f, "noise layer {k}")?,
                    None => f.write_str("the meander")?,
                }
                write!(
                    f,
                    " has spacing {spacing} and amplitude {amplitude}; the limits are \
                     1..={MAX_SPACING} and 0..={MAX_AMPLITUDE}"
                )
            }
            Self::Terraces => f.write_str("the terraces are outside their stated ranges"),
            Self::Weight { weight } => write!(f, "noise weight {weight} is outside 0..=16"),
            Self::Shelf { shelf } => write!(f, "shelf {shelf} is not a rectangle inside the grid"),
            Self::OutOfMemory { vertices } => {
                write!(f, "the heights of {vertices} vertices could not be reserved")
            }
        }
    }
}

impl std::error::Error for HeightfieldError {}

/// Checks every parameter of `spec` against its stated range and returns the vertex count.
pub(super) fn check(spec: &HeightfieldSpec<'_>) -> Result<usize, HeightfieldError> {
    let (columns, rows) = (spec.columns, spec.rows);
    let vertices = u64::from(columns) * u64::from(rows);
    if vertices == 0 || vertices > MAX_VERTICES {
        return Err(HeightfieldError::GridSize { columns, rows });
    }
    // A count that does not fit `u64` is more than the limit too, so it is refused as well.
    if !u64::try_from(spec.layers.len()).is_ok_and(|layers| layers <= u64::from(MAX_SPACING)) {
        return Err(HeightfieldError::TooManyLayers { layers: spec.layers.len() });
    }
    let layers = spec.layers.iter().enumerate().map(|(k, layer)| (Some(k), layer));
    for (layer, &NoiseLayer { spacing, amplitude }) in layers.chain([(None, &spec.meander)]) {
        if !(1..=MAX_SPACING).contains(&spacing) || !(0..=MAX_AMPLITUDE).contains(&amplitude) {
            return Err(HeightfieldError::Layer { layer, spacing, amplitude });
        }
    }
    let terraces = spec.terraces;
    if !(1..=MAX_SPACING).contains(&terraces.width)
        || !(1..=terraces.width).contains(&terraces.riser)
        || terraces.count > MAX_SPACING
        || !(0..=MAX_AMPLITUDE).contains(&terraces.height)
    {
        return Err(HeightfieldError::Terraces);
    }
    let weights = spec.weights;
    for weight in [weights.floor, weights.wall, weights.plateau] {
        if !(0..=16).contains(&weight) {
            return Err(HeightfieldError::Weight { weight });
        }
    }
    for (shelf, s) in spec.shelves.iter().enumerate() {
        let inside = s.columns[0] <= s.columns[1]
            && s.columns[1] < columns
            && s.rows[0] <= s.rows[1]
            && s.rows[1] < rows
            && (-MAX_AMPLITUDE..=MAX_AMPLITUDE).contains(&s.height);
        if !inside {
            return Err(HeightfieldError::Shelf { shelf });
        }
    }
    // At most `MAX_VERTICES`, `2^22`, which fits the `usize` of every target lumen builds for
    // (32 bits or more); a narrower one could not reserve the heights either.
    usize::try_from(vertices).map_err(|_| HeightfieldError::OutOfMemory { vertices: usize::MAX })
}
