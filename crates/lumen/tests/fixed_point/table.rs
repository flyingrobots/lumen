//! The table: the numerals against a seven-segment truth table, and the puck on the 8.

use lumen::scenes::fixed_point::puck::PUCK_RADIUS;
use lumen::scenes::fixed_point::table::{
    number_boxes, Footprint, NumberBox, BOX_NUMBERS, DIGIT_GAP, DIGIT_WIDTH, FELT_Y, STROKE,
};
use lumen::scenes::fixed_point::PartKind;

use crate::support::{content, part, Steps};

/// Oracle: the seven-segment display's standard digits, written out by segment letter (`a`
/// top, `b` top right, `c` bottom right, `d` bottom, `e` bottom left, `f` top left, `g`
/// middle), independently of the scene's bit table.
fn truth(digit: u8) -> &'static str {
    match digit {
        0 => "abcdef",
        1 => "bc",
        4 => "bcfg",
        5 => "acdfg",
        6 => "acdefg",
        8 => "abcdefg",
        9 => "abcdfg",
        _ => panic!("no box shows {digit}"),
    }
}

/// The centre of each segment's slot in a digit cell `(x, z)` (far-left corner), as `(x, z)`.
fn slots((x, z): (i64, i64), depth: i64) -> [(char, i64, i64); 7] {
    let (w, t) = (DIGIT_WIDTH, STROKE);
    [
        ('a', x + w / 2, z + t / 2),
        ('b', x + w - t / 2, z + depth / 4),
        ('c', x + w - t / 2, z + depth * 3 / 4),
        ('d', x + w / 2, z + depth - t / 2),
        ('e', x + t / 2, z + depth * 3 / 4),
        ('f', x + t / 2, z + depth / 4),
        ('g', x + w / 2, z + depth / 2),
    ]
}

/// Whether `(x, z)` is inside the triangle's footprint, edges included (exact).
fn covers(t: &[Steps; 3], x: i64, z: i64) -> bool {
    let side = |a: Steps, b: Steps| (b[0] - a[0]) * (z - a[2]) - (b[2] - a[2]) * (x - a[0]);
    let s = [side(t[0], t[1]), side(t[1], t[2]), side(t[2], t[0])];
    s.iter().all(|&v| v >= 0) || s.iter().all(|&v| v <= 0)
}

fn the_box(number: u8) -> NumberBox {
    number_boxes().into_iter().find(|b| b.number == number).expect("the box exists")
}

#[test]
fn each_numeral_lights_exactly_its_digits_segments() {
    let content = content();
    let mut checked = 0;
    for NumberBox { number, numeral, interior } in number_boxes() {
        let triangles = part(&content, PartKind::Numeral(number));
        let digits: Vec<u8> =
            if number >= 10 { vec![number / 10, number % 10] } else { vec![number] };
        for (i, &digit) in digits.iter().enumerate() {
            let left = numeral.min_x + i64::try_from(i).expect("small") * (DIGIT_WIDTH + DIGIT_GAP);
            let lit: String = slots((left, numeral.min_z), numeral.max_z - numeral.min_z)
                .iter()
                .filter(|&&(_, x, z)| triangles.iter().any(|t| covers(t, x, z)))
                .map(|&(name, _, _)| name)
                .collect();
            assert_eq!(lit, truth(digit), "box {number}, digit {digit}");
            checked += 1;
        }
        // Painted inside its own rectangle, which is inside the box.
        for p in triangles.iter().flatten() {
            assert!(numeral.contains(p[0], p[2]), "box {number}: {p:?} outside its numeral");
        }
        assert!(interior.contains(numeral.min_x, numeral.min_z));
        assert!(interior.contains(numeral.max_x, numeral.max_z));
    }
    assert_eq!(checked, 7, "seven digits in the boxes 4, 5, 6, 8, 9 and 10");
    assert_eq!(BOX_NUMBERS, [4, 5, 6, 8, 9, 10]);
}

/// Oracle: the craps rule the scene shows: the ON puck marks the point, so it lies in the 8
/// box, clear of the box's lines and of its numeral (the 8 stays readable), on the felt.
#[test]
fn the_puck_lies_inside_the_eight_box_beside_its_numeral() {
    let content = content();
    let eight = the_box(8);
    let mut vertices = Vec::new();
    for kind in [PartKind::PuckFace, PartKind::PuckRim, PartKind::PuckLetters] {
        vertices.extend(part(&content, kind).into_iter().flatten());
    }
    assert_eq!(vertices.len(), (24 + 96 + 14) * 3);
    let Footprint { min_x, min_z, max_x, max_z } = eight.numeral;
    for p in &vertices {
        assert!(eight.interior.contains(p[0], p[2]), "{p:?} is outside the 8 box");
        let beside = p[0] < min_x || p[0] > max_x || p[2] < min_z || p[2] > max_z;
        assert!(beside, "{p:?} covers the 8");
    }
    // Its footprint is the full disc: the widest vertices are a radius apart from the centre.
    let span = vertices.iter().map(|p| p[0]).max().zip(vertices.iter().map(|p| p[0]).min());
    let (right, left) = span.expect("vertices");
    assert_eq!(right - left, 2 * PUCK_RADIUS);
    assert_eq!(vertices.iter().map(|p| p[1]).min(), Some(FELT_Y), "the puck rests on the felt");
}
