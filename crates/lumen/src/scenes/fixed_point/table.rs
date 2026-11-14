//! The craps table: the felt bed, the padded rail around it, the white layout lines and the
//! row of number boxes, each with its block numeral.

use super::content::{FixedPointContent, PartKind};
use super::numerals::push_numeral;
use super::solids::{flat_rect, quad, StepTriangle, UNIT};

/// A rectangle on the table, in grid steps: `min_x..=max_x` by `min_z..=max_z`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Footprint {
    /// The smallest X.
    pub min_x: i64,
    /// The smallest Z (the far side, away from the camera).
    pub min_z: i64,
    /// The largest X.
    pub max_x: i64,
    /// The largest Z (the near side).
    pub max_z: i64,
}

impl Footprint {
    /// Whether the point `(x, z)` is inside the rectangle, edges included.
    #[must_use]
    pub const fn contains(&self, x: i64, z: i64) -> bool {
        self.min_x <= x && x <= self.max_x && self.min_z <= z && z <= self.max_z
    }

    const fn corners(&self) -> [i64; 4] {
        [self.min_x, self.min_z, self.max_x, self.max_z]
    }
}

/// The felt: `x = -7.5..=7.5`, `z = -12..=3.25` units, at `y = 0`.
pub const FELT: Footprint = Footprint {
    min_x: -UNIT * 15 / 2,
    min_z: -12 * UNIT,
    max_x: UNIT * 15 / 2,
    max_z: UNIT * 13 / 4,
};

/// The height of the felt's surface, which the dice, the puck and the bunny rest on.
pub const FELT_Y: i64 = 0;

/// The height of the layout lines and numerals above the felt: `2^-10` units, painted on.
pub const PAINT_Y: i64 = 64;

/// Half the width of a layout line: lines are `1/16` unit wide.
pub const LINE_HALF: i64 = UNIT / 32;

/// The rail's cross-section as `(outward offset, height)` in steps, from the felt's edge up
/// over the padded top and down the outside: 0.375 high at the felt, 0.75 at its crown,
/// 1.125 wide.
const RAIL_PROFILE: [(i64, i64); 8] = [
    (0, 0),
    (0, UNIT * 3 / 8),
    (UNIT / 8, UNIT * 5 / 8),
    (UNIT * 3 / 8, UNIT * 3 / 4),
    (UNIT * 3 / 4, UNIT * 3 / 4),
    (UNIT, UNIT * 5 / 8),
    (UNIT * 9 / 8, UNIT * 3 / 8),
    (UNIT * 9 / 8, -UNIT * 3 / 2),
];

/// The numbers of the boxes, left to right.
pub const BOX_NUMBERS: [u8; 6] = [4, 5, 6, 8, 9, 10];

/// The X of the left edge (line centre) of the first box; the boxes are 1.75 units wide.
const BOX_ROW_LEFT: i64 = -UNIT * 21 / 4;
const BOX_WIDTH: i64 = UNIT * 7 / 4;
/// The Z of the far and near box lines' centres.
const BOX_FAR: i64 = -UNIT * 9 / 2;
const BOX_NEAR: i64 = -UNIT * 3 / 2;
/// The numeral's far and near edges: in the near half of the box, so the puck fits behind.
const NUMERAL_FAR: i64 = -UNIT * 25 / 8;
const NUMERAL_NEAR: i64 = -UNIT * 7 / 4;

/// The width of one digit cell, the gap between the two digits of 10, and the stroke.
pub const DIGIT_WIDTH: i64 = UNIT * 11 / 16;
/// The gap between the two digits of 10.
pub const DIGIT_GAP: i64 = UNIT / 8;
/// The width of a numeral's stroke.
pub const STROKE: i64 = UNIT * 3 / 16;

/// A number box: its number, the felt inside its lines, and the rectangle of its numeral.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumberBox {
    /// The box's number.
    pub number: u8,
    /// The felt inside the box's lines.
    pub interior: Footprint,
    /// The numeral's rectangle: one digit cell of [`DIGIT_WIDTH`], or for 10 two cells and
    /// [`DIGIT_GAP`] between them, centred in the box.
    pub numeral: Footprint,
}

/// The six number boxes, left to right.
#[must_use]
pub fn number_boxes() -> [NumberBox; 6] {
    let mut column = 0;
    BOX_NUMBERS.map(|number| {
        let left = BOX_ROW_LEFT + column * BOX_WIDTH;
        column += 1;
        let interior = Footprint {
            min_x: left + LINE_HALF,
            min_z: BOX_FAR + LINE_HALF,
            max_x: left + BOX_WIDTH - LINE_HALF,
            max_z: BOX_NEAR - LINE_HALF,
        };
        let width = if number >= 10 { 2 * DIGIT_WIDTH + DIGIT_GAP } else { DIGIT_WIDTH };
        let centre = left + BOX_WIDTH / 2;
        let numeral = Footprint {
            min_x: centre - width / 2,
            min_z: NUMERAL_FAR,
            max_x: centre + width / 2,
            max_z: NUMERAL_NEAR,
        };
        NumberBox { number, interior, numeral }
    })
}

/// Appends the felt, the rail, the layout lines and the numerals, in that order.
pub(crate) fn push_table(content: &mut FixedPointContent) {
    let mut felt = Vec::new();
    flat_rect(&mut felt, FELT.corners(), FELT_Y);
    content.push_steps(PartKind::Felt, &felt);
    content.push_steps(PartKind::Rail, &rail());
    content.push_steps(PartKind::LayoutLines, &layout_lines());
    for number_box in number_boxes() {
        push_numeral(content, number_box.number, number_box.numeral);
    }
}

/// The rail: the profile swept around the felt's rectangle with mitred corners, four sides
/// of seven strips.
fn rail() -> Vec<StepTriangle> {
    let Footprint { min_x, min_z, max_x, max_z } = FELT;
    let mut out = Vec::new();
    // Each side as a corner and the outward directions along X and Z, going round.
    let sides = [
        ((min_x, max_z), (max_x, max_z), [0, 0, 1]),
        ((max_x, max_z), (max_x, min_z), [1, 0, 0]),
        ((max_x, min_z), (min_x, min_z), [0, 0, -1]),
        ((min_x, min_z), (min_x, max_z), [-1, 0, 0]),
    ];
    for ((ax, az), (bx, bz), outward) in sides {
        let corner = |x: i64, z: i64, (offset, y): (i64, i64)| {
            [x + x.signum() * offset, y, z + z.signum() * offset]
        };
        for pair in RAIL_PROFILE.windows(2) {
            if let [inner, outer] = *pair {
                let toward = [outward[0], 2, outward[2]];
                let strip = [
                    corner(ax, az, inner),
                    corner(bx, bz, inner),
                    corner(bx, bz, outer),
                    corner(ax, az, outer),
                ];
                quad(&mut out, strip, toward);
            }
        }
    }
    out
}

/// The layout lines: the far and near lines of the box row, the dividers between the boxes,
/// and two long lines in front of the row.
fn layout_lines() -> Vec<StepTriangle> {
    let mut out = Vec::new();
    let row_right = BOX_ROW_LEFT + 6 * BOX_WIDTH;
    for z in [BOX_FAR, BOX_NEAR] {
        let rect = [BOX_ROW_LEFT - LINE_HALF, z - LINE_HALF, row_right + LINE_HALF, z + LINE_HALF];
        flat_rect(&mut out, rect, PAINT_Y);
    }
    for column in 0..=6 {
        let x = BOX_ROW_LEFT + column * BOX_WIDTH;
        let rect = [x - LINE_HALF, BOX_FAR + LINE_HALF, x + LINE_HALF, BOX_NEAR - LINE_HALF];
        flat_rect(&mut out, rect, PAINT_Y);
    }
    for z in [-UNIT / 2, 2 * UNIT] {
        let rect = [-UNIT * 27 / 4, z - LINE_HALF, UNIT * 27 / 4, z + LINE_HALF];
        flat_rect(&mut out, rect, PAINT_Y);
    }
    out
}
