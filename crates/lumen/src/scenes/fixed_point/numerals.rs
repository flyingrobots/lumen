//! Block numerals for the number boxes: seven-segment digits made of painted rectangles.
//!
//! A digit's top is its far side (`-Z`), so it reads upright from the camera. Its segments are
//! named as on a seven-segment display: `a` top, `b` top right, `c` bottom right, `d` bottom,
//! `e` bottom left, `f` top left, `g` middle. The vertical strokes run the full half height and
//! the horizontal ones fit between them, so no two rectangles overlap.

use super::content::{FixedPointContent, PartKind};
use super::solids::{flat_rect, StepTriangle};
use super::table::{Footprint, DIGIT_GAP, DIGIT_WIDTH, PAINT_Y, STROKE};

/// The lit segments of a digit, bit 0 for `a` to bit 6 for `g`.
const fn segments(digit: u8) -> u8 {
    match digit {
        0 => 0b011_1111,
        1 => 0b000_0110,
        2 => 0b101_1011,
        3 => 0b100_1111,
        4 => 0b110_0110,
        5 => 0b110_1101,
        6 => 0b111_1101,
        7 => 0b000_0111,
        8 => 0b111_1111,
        _ => 0b110_1111,
    }
}

/// The rectangle `[min_x, min_z, max_x, max_z]` of segment `index` (0 = `a` to 6 = `g`) in the
/// digit cell whose far-left corner is `(x, z)`, `width` wide and `depth` deep.
fn segment_rect(index: u32, (x, z): (i64, i64), width: i64, depth: i64) -> [i64; 4] {
    let (right, near, middle) = (x + width, z + depth, z + depth / 2);
    let (inner_left, inner_right) = (x + STROKE, right - STROKE);
    match index {
        0 => [inner_left, z, inner_right, z + STROKE],
        1 => [inner_right, z, right, middle],
        2 => [inner_right, middle, right, near],
        3 => [inner_left, near - STROKE, inner_right, near],
        4 => [x, middle, inner_left, near],
        5 => [x, z, inner_left, middle],
        _ => [inner_left, middle - STROKE / 2, inner_right, middle + STROKE / 2],
    }
}

/// Appends the numeral for `number` (one or two digits) laid out in `area` as one part.
pub(crate) fn push_numeral(content: &mut FixedPointContent, number: u8, area: Footprint) {
    let depth = area.max_z - area.min_z;
    let digits: &[u8] = if number >= 10 { &[number / 10, number % 10] } else { &[number] };
    let mut out: Vec<StepTriangle> = Vec::new();
    let mut left = area.min_x;
    for &digit in digits {
        let lit = segments(digit);
        for index in (0..7).filter(|i| (lit >> i) & 1 == 1) {
            let rect = segment_rect(index, (left, area.min_z), DIGIT_WIDTH, depth);
            flat_rect(&mut out, rect, PAINT_Y);
        }
        left += DIGIT_WIDTH + DIGIT_GAP;
    }
    content.push_steps(PartKind::Numeral(number), &out);
}
