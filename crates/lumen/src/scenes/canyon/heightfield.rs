//! The canyon's terrain heights: the generator [`HEIGHTFIELD_ID`], `lumen-canyon-heightfield/1`.
//!
//! This module owns the generator: from a [`HeightfieldSpec`] it computes one height, in whole
//! `2^-16` grid steps, at every vertex of a regular grid of `columns` by `rows`, using integer
//! arithmetic only. Column `i` runs across the canyon, row `j` along it. A height is a pure
//! function of the spec and `(i, j)`, so the same spec gives the same heights, bit for bit, on
//! every platform.
//!
//! # The algorithm, `lumen-canyon-heightfield/1`
//!
//! Every integer below is exact; `/` on a nonnegative value and `div_euclid` both floor.
//!
//! 1. **Lattice hash.** For a seed `s`, a layer tag `k` and a lattice point `(a, b)`, all as
//!    `u64`, `z = s ^ k * 0xD6E8FEB86659FD93 ^ a * 0x9E3779B97F4A7C15 ^ b * 0xC2B2AE3D27D4EB4F`
//!    (products wrap modulo `2^64`), then the `SplitMix64` output function of state `z`:
//!    `z += 0x9E3779B97F4A7C15; z = (z ^ z >> 30) * 0xBF58476D1CE4E5B9;
//!    z = (z ^ z >> 27) * 0x94D049BB133111EB; h = z ^ z >> 31` (all wrapping). With every input
//!    zero, `h` is `SplitMix64`'s first output from state 0, `0xE220A8397B1DCDAF`.
//! 2. **Lattice value.** The top 16 bits of `h` less 32 768, `t` in `-32768..=32767`, scaled to
//!    the layer's amplitude `A`: `v = (t * A).div_euclid(32768)`, so `-A <= v < A`.
//! 3. **Noise.** Each [`NoiseLayer`] `k` (counting from 0) with spacing `S` and amplitude `A`
//!    puts lattice points at every `S`-th column and row. At vertex `(i, j)`, with
//!    `(a, f) = (i / S, i % S)` and `(b, g) = (j / S, j % S)` and the values `v00`, `v10`, `v01`,
//!    `v11` at `(a, b)`, `(a + 1, b)`, `(a, b + 1)`, `(a + 1, b + 1)` (layer tag `k`), the layer
//!    is `(v00 (S - f)(S - g) + v10 f (S - g) + v01 (S - f) g + v11 f g).div_euclid(S * S)`, and
//!    the noise `n` is the sum of the layers.
//! 4. **Centre line.** The meander layer (spacing `M` rows, amplitude `C` columns, layer tag
//!    `u64::MAX`, lattice point `(0, b)`) gives the centre of row `j` in 256ths of a column:
//!    `c = 256 * centre + (256 * (m0 (M - g) + m1 g)).div_euclid(M)`, with
//!    `(b, g) = (j / M, j % M)` and `m0`, `m1` the values at `(0, b)` and `(0, b + 1)`.
//! 5. **Carve.** All in 256ths of a column: `d = |256 i - c|`, and the [`Terraces`] (`count`
//!    tiers, each `width` columns wide, of which the first `riser` rise by `height` steps and
//!    the rest are a flat ledge) with `F = 256 * floor_half_width`, `W = 256 * width` and
//!    `R = 256 * riser`. A floor (`d <= F`) has profile 0; otherwise `u = d - F` and tier
//!    `t = u / W`; a wall (`t < count`) has `t * height + height * min(u % W, R) / R`; beyond
//!    the walls the plateau is `count * height`. The fraction keeps a wall smooth where the
//!    centre line moves by less than a column from row to row.
//! 6. **Height.** `profile + (n * w).div_euclid(4)`, where `w` is the zone's [`NoiseWeights`]
//!    entry in quarters: the floor's, the walls' or the plateau's.
//! 7. **Shelves.** Each [`Shelf`] in order lowers every vertex of its rectangle of columns and
//!    rows that is above its height to that height exactly: a level ledge cut into the rock,
//!    whose edges stay rough where the rock was already lower.
//!
//! [`HeightfieldSpec::bounds`] states the range every height of an accepted spec lies in, and
//! refuses, like [`generate`], a spec outside its ranges. Any change to these steps, their
//! constants or their order is a new version of the generator.

mod error;

pub use error::HeightfieldError;

/// The generator's stable name and version.
pub const HEIGHTFIELD_ID: &str = "lumen-canyon-heightfield/1";

/// The most vertices a heightfield may have: `2^22`, about four million.
pub const MAX_VERTICES: u64 = 1 << 22;

/// The largest amplitude, tier height or shelf height magnitude, in grid steps: `2^24`, 256
/// units, the extent of Bunny's trace domain. With [`MAX_SPACING`] it keeps every product of
/// step 3 below `2^51`.
pub const MAX_AMPLITUDE: i64 = 1 << 24;

/// The largest noise spacing, meander spacing, terrace width or layer count: `2^12`.
pub const MAX_SPACING: u32 = 1 << 12;

/// The multipliers of the lattice hash's inputs (step 1): the layer tag, the column and the row.
const MIX: [u64; 3] = [0xD6E8_FEB8_6659_FD93, 0x9E37_79B9_7F4A_7C15, 0xC2B2_AE3D_27D4_EB4F];

/// The layer tag of the meander.
const MEANDER_TAG: u64 = u64::MAX;

/// One layer of value noise: lattice points every `spacing` vertices, values in
/// `-amplitude..amplitude`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoiseLayer {
    /// Vertices between lattice points, `1..=MAX_SPACING`.
    pub spacing: u32,
    /// The amplitude, `0..=MAX_AMPLITUDE`: grid steps for terrain noise, columns for the
    /// meander.
    pub amplitude: i64,
}

/// The canyon's walls: tiers, each a steep riser and a flat ledge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Terraces {
    /// The number of tiers between the floor and the plateau.
    pub count: u32,
    /// The columns each tier spans, `1..=MAX_SPACING`.
    pub width: u32,
    /// The columns of a tier's rise, `1..=width`; the rest of the tier is level.
    pub riser: u32,
    /// The grid steps each tier rises, `0..=MAX_AMPLITUDE`.
    pub height: i64,
}

/// How much of the noise each zone takes, in quarters: 4 is the whole noise, 0 none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoiseWeights {
    /// The floor's weight, `0..=16`.
    pub floor: i64,
    /// The walls' weight, `0..=16`.
    pub wall: i64,
    /// The plateau's weight, `0..=16`.
    pub plateau: i64,
}

/// A ledge cut level: in columns `columns[0]..=columns[1]` and rows `rows[0]..=rows[1]`, every
/// vertex above `height` grid steps is lowered to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shelf {
    /// The first and last column.
    pub columns: [u32; 2],
    /// The first and last row.
    pub rows: [u32; 2],
    /// The height, `-MAX_AMPLITUDE..=MAX_AMPLITUDE`.
    pub height: i64,
}

/// Everything the generator reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeightfieldSpec<'a> {
    /// The seed of every lattice hash.
    pub seed: u64,
    /// Vertices across the canyon, at least 1.
    pub columns: u32,
    /// Vertices along the canyon, at least 1.
    pub rows: u32,
    /// The noise layers, at most `MAX_SPACING`, in order (the order sets their layer tags).
    pub layers: &'a [NoiseLayer],
    /// The centre column before the meander.
    pub centre: u32,
    /// The meander of the centre line: spacing in rows, amplitude in columns.
    pub meander: NoiseLayer,
    /// The columns either side of the centre line that are floor.
    pub floor_half_width: u32,
    /// The walls.
    pub terraces: Terraces,
    /// The noise per zone.
    pub weights: NoiseWeights,
    /// The level shelves, applied last, in order.
    pub shelves: &'a [Shelf],
}

/// The heights of every vertex, row by row: the height of column `i` of row `j` is entry
/// `j * columns + i`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heightfield {
    columns: u32,
    rows: u32,
    heights: Vec<i64>,
}

impl Heightfield {
    /// Vertices across.
    #[must_use]
    pub const fn columns(&self) -> u32 {
        self.columns
    }

    /// Vertices along.
    #[must_use]
    pub const fn rows(&self) -> u32 {
        self.rows
    }

    /// Every height in grid steps, row by row.
    #[must_use]
    pub fn heights(&self) -> &[i64] {
        &self.heights
    }

    /// The height of column `column` of row `row`, or `None` outside the grid.
    #[must_use]
    pub fn height(&self, column: u32, row: u32) -> Option<i64> {
        if column >= self.columns {
            return None;
        }
        let index = u64::from(row) * u64::from(self.columns) + u64::from(column);
        self.heights.get(usize::try_from(index).ok()?).copied()
    }
}

/// Which part of the profile a vertex is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Zone {
    Floor,
    Wall,
    Plateau,
}

impl HeightfieldSpec<'_> {
    /// The centre of row `row` in 256ths of a column (step 4): where the canyon's floor is
    /// centred.
    ///
    /// # Errors
    ///
    /// The [`HeightfieldError`] [`generate`] returns for a spec outside its stated ranges: the
    /// centre is stated only for a spec the generator accepts.
    pub fn centre_256(&self, row: u32) -> Result<i64, HeightfieldError> {
        error::check(self)?;
        Ok(self.accepted_centre_256(row))
    }

    /// The centre of row `row` of a spec [`error::check`] accepted. Inside its ranges nothing
    /// overflows: a meander value is below `2^24` in magnitude and its spacing at most `2^12`, so
    /// the blend is below `2^8 * 2^24 * 2^12 = 2^44`, and `256 * centre` is below `2^40`.
    fn accepted_centre_256(&self, row: u32) -> i64 {
        let spacing = i64::from(self.meander.spacing);
        let (b, g) = (i64::from(row) / spacing, i64::from(row) % spacing);
        let value = |b: i64| lattice_value(self.seed, MEANDER_TAG, [0, b], self.meander.amplitude);
        let offset = (256 * (value(b) * (spacing - g) + value(b + 1) * g)).div_euclid(spacing);
        256 * i64::from(self.centre) + offset
    }

    /// The range every height lies in, `[lowest, highest]` grid steps: the profile's range
    /// widened by the largest weighted noise (each layer is within its amplitude, step 2), and
    /// every shelf's height.
    ///
    /// # Errors
    ///
    /// The [`HeightfieldError`] [`generate`] returns for a spec outside its stated ranges: the
    /// range is stated only for a spec the generator accepts.
    pub fn bounds(&self) -> Result<[i64; 2], HeightfieldError> {
        error::check(self)?;
        // Inside the checked ranges nothing overflows: at most 2^12 layers of amplitude at most
        // 2^24 sum below 2^36, a weight of at most 16 makes the spread at most 2^40, and the
        // rim, at most 2^12 tiers of at most 2^24 steps, at most 2^36.
        let noise: i64 = self.layers.iter().map(|layer| layer.amplitude).sum();
        let weight = self.weights.floor.max(self.weights.wall).max(self.weights.plateau);
        let spread = noise * weight;
        let rim = i64::from(self.terraces.count) * self.terraces.height;
        let shelves = self.shelves.iter().map(|shelf| shelf.height);
        let lowest = shelves.clone().fold((-spread).div_euclid(4), i64::min);
        let highest = shelves.fold(rim + spread.div_euclid(4), i64::max);
        Ok([lowest, highest])
    }

    /// The carve's profile at `distance` 256ths of a column from the centre line, and its zone
    /// (step 5).
    fn profile(&self, distance: i64) -> (i64, Zone) {
        let floor = 256 * i64::from(self.floor_half_width);
        if distance <= floor {
            return (0, Zone::Floor);
        }
        let Terraces { count, width, riser, height } = self.terraces;
        let (width, riser) = (256 * i64::from(width), 256 * i64::from(riser));
        let (tier, within) = ((distance - floor) / width, (distance - floor) % width);
        if tier >= i64::from(count) {
            return (i64::from(count) * height, Zone::Plateau);
        }
        (tier * height + height * within.min(riser) / riser, Zone::Wall)
    }

    /// The noise at vertex `(column, row)` (step 3).
    fn noise(&self, column: u32, row: u32) -> i64 {
        let mut total = 0;
        for (layer, tag) in self.layers.iter().zip(0_u64..) {
            let spacing = i64::from(layer.spacing);
            let (across, f) = (i64::from(column) / spacing, i64::from(column) % spacing);
            let (along, g) = (i64::from(row) / spacing, i64::from(row) % spacing);
            let value = |da: i64, db: i64| {
                lattice_value(self.seed, tag, [across + da, along + db], layer.amplitude)
            };
            let sum = value(0, 0) * (spacing - f) * (spacing - g)
                + value(1, 0) * f * (spacing - g)
                + value(0, 1) * (spacing - f) * g
                + value(1, 1) * f * g;
            total += sum.div_euclid(spacing * spacing);
        }
        total
    }

    /// The height of vertex `(column, row)` before the shelves (steps 4 to 6).
    fn carved(&self, column: u32, row: u32) -> i64 {
        let distance = (256 * i64::from(column) - self.accepted_centre_256(row)).abs();
        let (profile, zone) = self.profile(distance);
        let weight = match zone {
            Zone::Floor => self.weights.floor,
            Zone::Wall => self.weights.wall,
            Zone::Plateau => self.weights.plateau,
        };
        profile + (self.noise(column, row) * weight).div_euclid(4)
    }
}

/// Generates the heightfield of `spec` by [`HEIGHTFIELD_ID`].
///
/// # Errors
///
/// A [`HeightfieldError`] naming the first parameter outside its stated range, checked before
/// anything is computed or allocated, or [`HeightfieldError::OutOfMemory`] if the heights cannot
/// be reserved.
pub fn generate(spec: &HeightfieldSpec<'_>) -> Result<Heightfield, HeightfieldError> {
    let vertices = error::check(spec)?;
    let mut heights = Vec::new();
    heights.try_reserve_exact(vertices).map_err(|_| HeightfieldError::OutOfMemory { vertices })?;
    for row in 0..spec.rows {
        for column in 0..spec.columns {
            heights.push(spec.carved(column, row));
        }
    }
    for shelf in spec.shelves {
        for row in shelf.rows[0]..=shelf.rows[1] {
            let start = u64::from(row) * u64::from(spec.columns);
            let first = usize::try_from(start + u64::from(shelf.columns[0]));
            let last = usize::try_from(start + u64::from(shelf.columns[1]));
            if let (Ok(first), Ok(last)) = (first, last) {
                heights.get_mut(first..=last).into_iter().flatten().for_each(|h| {
                    *h = (*h).min(shelf.height);
                });
            }
        }
    }
    Ok(Heightfield { columns: spec.columns, rows: spec.rows, heights })
}

/// The lattice value of point `[a, b]` of layer `tag` (steps 1 and 2): `-amplitude..amplitude`.
fn lattice_value(seed: u64, tag: u64, [a, b]: [i64; 2], amplitude: i64) -> i64 {
    // Lattice coordinates are nonnegative: vertices are, and the farthest corner is one more.
    let unsigned = |n: i64| n.unsigned_abs();
    let mut z = seed
        ^ tag.wrapping_mul(MIX[0])
        ^ unsigned(a).wrapping_mul(MIX[1])
        ^ unsigned(b).wrapping_mul(MIX[2]);
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    let [high, low, ..] = (z ^ (z >> 31)).to_be_bytes();
    let top = i64::from(u16::from_be_bytes([high, low])) - 32_768;
    (top * amplitude).div_euclid(32_768)
}
