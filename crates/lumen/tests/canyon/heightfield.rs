//! The heightfield generator, `lumen-canyon-heightfield/1`: exact values on tiny grids,
//! determinism, the stated bounds, the carved channel, a smooth wall where the centre line
//! bends, the shelves and the refusals.
//!
//! The values written out below come from an independent Python model of the documented
//! algorithm (its integers are unbounded, so no wrapping or overflow stands in for the
//! arithmetic), and the first from `SplitMix64`'s published first output.

use lumen::scenes::canyon::heightfield::{
    generate, HeightfieldError, HeightfieldSpec, NoiseLayer, NoiseWeights, Shelf, Terraces,
    HEIGHTFIELD_ID, MAX_VERTICES,
};

/// No noise, no meander, no walls: a flat floor. Tests change the fields they need.
const FLAT: HeightfieldSpec<'static> = HeightfieldSpec {
    seed: 0,
    columns: 1,
    rows: 1,
    layers: &[],
    centre: 0,
    meander: NoiseLayer { spacing: 1, amplitude: 0 },
    floor_half_width: 1000,
    terraces: Terraces { count: 1, width: 1, riser: 1, height: 0 },
    weights: NoiseWeights { floor: 4, wall: 4, plateau: 4 },
    shelves: &[],
};

/// The heights of `spec`, row by row; it generates.
fn heights(spec: &HeightfieldSpec<'_>) -> Vec<i64> {
    generate(spec).expect("the spec is valid").heights().to_vec()
}

/// Oracle: `SplitMix64`'s first output from state 0 is `0xE220A8397B1DCDAF` (Vigna's reference
/// `splitmix64.c`). With seed, tag and lattice point all zero the hash is that value, so a
/// layer of spacing 1 and amplitude 32 768 at vertex (0, 0) is its top 16 bits, `0xE220`,
/// less 32 768: 57 888 - 32 768 = 25 120.
#[test]
fn the_lattice_hash_at_zero_is_splitmix64s_first_output() {
    assert_eq!(HEIGHTFIELD_ID, "lumen-canyon-heightfield/1");
    let spec = HeightfieldSpec { layers: &[NoiseLayer { spacing: 1, amplitude: 32_768 }], ..FLAT };
    assert_eq!(heights(&spec), vec![25_120]);
}

/// Oracle: the Python model. The lattice values of seed 7, layer 0, amplitude 1 000 are
/// -221, 847 (row 0, lattice columns 0 and 1) and 205, 861 (row 1); at spacing 2 the corners
/// of a 3 by 3 grid are those values and every other vertex is their bilinear blend, floored:
/// (1, 0) is (2 * -221 + 2 * 847) / 4 = 313.
#[test]
fn a_tiny_grid_has_the_hand_checked_noise() {
    let spec = HeightfieldSpec {
        seed: 7,
        columns: 3,
        rows: 3,
        layers: &[NoiseLayer { spacing: 2, amplitude: 1000 }],
        ..FLAT
    };
    assert_eq!(heights(&spec), vec![-221, 313, 847, -8, 423, 854, 205, 533, 861]);
}

/// Oracle: the profile worked by hand. Centre column 4, floor half-width 1, two tiers 3
/// columns wide with a riser of 2 and a height of 100: distance 0 and 1 are floor (0); 2 is
/// half way up the first riser (50); 3 is its top (100); 4 is the first tier's ledge (100); 5
/// and 6 climb the second riser (150, 200); 7 and beyond are the plateau (200).
#[test]
fn the_carve_has_the_hand_worked_profile() {
    let spec = HeightfieldSpec {
        columns: 12,
        centre: 4,
        floor_half_width: 1,
        terraces: Terraces { count: 2, width: 3, riser: 2, height: 100 },
        ..FLAT
    };
    let want = vec![100, 100, 50, 0, 0, 0, 50, 100, 100, 150, 200, 200];
    assert_eq!(heights(&spec), want);
}

/// Oracle: a pure function gives the same bytes twice; a different seed gives different ones.
#[test]
fn the_same_spec_gives_the_same_heights() {
    let spec = HeightfieldSpec {
        seed: 42,
        columns: 33,
        rows: 17,
        layers: &[
            NoiseLayer { spacing: 4, amplitude: 9000 },
            NoiseLayer { spacing: 1, amplitude: 70 },
        ],
        ..FLAT
    };
    assert_eq!(heights(&spec), heights(&spec));
    assert_ne!(heights(&spec), heights(&HeightfieldSpec { seed: 43, ..spec }));
}

/// A small canyon with every step in use: noise, meander, walls, plateau and a shelf.
const SMALL: HeightfieldSpec<'static> = HeightfieldSpec {
    seed: 1995,
    columns: 64,
    rows: 96,
    layers: &[
        NoiseLayer { spacing: 16, amplitude: 4000 },
        NoiseLayer { spacing: 4, amplitude: 2000 },
        NoiseLayer { spacing: 1, amplitude: 500 },
    ],
    centre: 32,
    meander: NoiseLayer { spacing: 32, amplitude: 6 },
    floor_half_width: 5,
    terraces: Terraces { count: 3, width: 4, riser: 3, height: 20_000 },
    weights: NoiseWeights { floor: 1, wall: 8, plateau: 4 },
    shelves: &[Shelf { columns: [2, 26], rows: [10, 20], height: 30_000 }],
};

/// Oracle: the Python model, run on the small canyon below (noise, meander, walls, plateau and
/// a shelf all in use): the sum of its 6 144 heights, their sum weighted by position (index
/// plus one, row by row), two single vertices and the extremes.
#[test]
fn a_small_canyon_matches_the_python_model() {
    let all = heights(&SMALL);
    let weighted: i64 = all.iter().zip(1_i64..).map(|(h, k)| h * k).sum();
    assert_eq!(all.iter().sum::<i64>(), 237_518_935);
    assert_eq!(weighted, 736_715_592_605);
    assert_eq!((all[0], all[64 * 50 + 40]), (61_310, 7_703));
    assert_eq!((all.iter().min(), all.iter().max()), (Some(&-7_236), Some(&64_733)));
}

/// Oracle: the stated bound, `HeightfieldSpec::bounds`, derived from each layer's range;
/// every vertex of each of [`SPECS`] lies inside it. Witness: both ends of
/// the small spec's range are approached (its heights span more than half the bound).
#[test]
fn every_height_lies_within_the_stated_bounds() {
    for spec in SPECS {
        let [lowest, highest] = spec.bounds();
        let all = heights(&spec);
        assert!(all.iter().all(|h| (lowest..=highest).contains(h)), "{lowest}..={highest}");
        let (min, max) = (all.iter().min().copied(), all.iter().max().copied());
        let span = max.zip(min).map_or(0, |(max, min)| max - min);
        assert!(2 * span > highest - lowest, "{span} of {lowest}..={highest}");
    }
}

/// The specs the bound and channel tests run on: the small canyon.
const SPECS: [HeightfieldSpec<'static>; 1] = [SMALL];

/// Whether `(column, row)` is inside one of `spec`'s shelves.
fn shelved(spec: &HeightfieldSpec<'_>, column: u32, row: u32) -> bool {
    spec.shelves.iter().any(|shelf| {
        (shelf.columns[0]..=shelf.columns[1]).contains(&column)
            && (shelf.rows[0]..=shelf.rows[1]).contains(&row)
    })
}

/// Oracle: the bounds again, per zone. For every row, the floor at the centre line is at most
/// the floor's noise bound above 0, and every plateau vertex of the row outside a shelf is at
/// least the rim less the plateau's noise bound. The margin between the two is stated per
/// spec: 51 875 steps for the small canyon. Witness: every row has plateau on both sides of
/// the channel.
#[test]
fn the_channel_runs_below_the_rim_along_every_row() {
    for (spec, margin) in SPECS.iter().zip([51_875]) {
        let field = generate(spec).expect("valid");
        let noise: i64 = spec.layers.iter().map(|layer| layer.amplitude).sum();
        let floor_top = noise * spec.weights.floor / 4;
        let rim = i64::from(spec.terraces.count) * spec.terraces.height;
        let plateau_bottom = rim + (-noise * spec.weights.plateau).div_euclid(4);
        assert!(plateau_bottom - floor_top >= margin, "{plateau_bottom} - {floor_top}");
        let walls = i64::from(spec.floor_half_width + spec.terraces.count * spec.terraces.width);
        let mut both_sides = 0;
        for row in 0..spec.rows {
            let centre = spec.centre_256(row).div_euclid(256);
            let column = u32::try_from(centre).expect("the centre is on the grid");
            let floor = field.height(column, row).expect("on the grid");
            assert!(floor <= floor_top, "row {row}: floor {floor}");
            let mut sides = [false; 2];
            for column in 0..spec.columns {
                let distance = 256 * i64::from(column) - spec.centre_256(row);
                if distance.abs() > 256 * (walls + 1) && !shelved(spec, column, row) {
                    let height = field.height(column, row).expect("on the grid");
                    assert!(height >= plateau_bottom, "row {row} column {column}: {height}");
                    sides[usize::from(distance > 0)] = true;
                }
            }
            both_sides += usize::from(sides == [true, true]);
        }
        assert_eq!(both_sides, usize::try_from(spec.rows).expect("fits"));
    }
}

/// Oracle: the wall's slope bounds the change between rows. With no noise, a meander of at
/// most 8 columns over 64 rows moves the centre by at most a quarter of a column per row
/// (`16 / 64`), so a wall rising 256 steps per column changes by at most 64 steps per row,
/// plus one for each floor. A centre line in whole columns would jump by 256 at a step.
/// Witness: the centre moves, and some wall vertex changes from row to row.
#[test]
fn a_wall_stays_smooth_where_the_centre_line_bends() {
    let spec = HeightfieldSpec {
        seed: 3,
        columns: 40,
        rows: 256,
        centre: 20,
        meander: NoiseLayer { spacing: 64, amplitude: 8 },
        floor_half_width: 2,
        terraces: Terraces { count: 1, width: 12, riser: 12, height: 12 * 256 },
        ..FLAT
    };
    let field = generate(&spec).expect("valid");
    let (first, last) = (spec.centre_256(0), (0..spec.rows).map(|r| spec.centre_256(r)));
    assert!(last.clone().any(|c| (c - first).abs() >= 256), "the centre line does not move");
    let mut changed = 0;
    for row in 1..spec.rows {
        for column in 0..spec.columns {
            let step = |r| field.height(column, r).expect("on the grid");
            let change = (step(row) - step(row - 1)).abs();
            assert!(change <= 66, "row {row} column {column}: {change}");
            changed += usize::from(change > 0);
        }
    }
    assert!(changed > 100, "{changed} changes");
}

/// Oracle: the shelf's rule, a vertex inside it lowered to its height and none raised. The
/// small spec's shelf at 30 000 steps runs from its left plateau, which is higher, down the
/// wall to the floor, which is lower. Witness: vertices of both kinds (242 cut and 33 kept
/// when recorded).
#[test]
fn a_shelf_cuts_the_rock_down_to_its_height() {
    let without = heights(&HeightfieldSpec { shelves: &[], ..SMALL });
    let with = heights(&SMALL);
    let shelf = SMALL.shelves[0];
    let (mut cut, mut kept) = (0, 0);
    for row in 0..SMALL.rows {
        for column in 0..SMALL.columns {
            let index = usize::try_from(row * SMALL.columns + column).expect("fits");
            let inside = (shelf.columns[0]..=shelf.columns[1]).contains(&column)
                && (shelf.rows[0]..=shelf.rows[1]).contains(&row);
            let want = if inside { without[index].min(shelf.height) } else { without[index] };
            assert_eq!(with[index], want, "row {row} column {column}");
            cut += usize::from(inside && without[index] > shelf.height);
            kept += usize::from(inside && without[index] < shelf.height);
        }
    }
    assert!(cut > 20 && kept > 20, "{cut} vertices cut, {kept} kept");
}

/// Oracle: each parameter's stated range, one step outside it.
#[test]
fn a_spec_outside_its_ranges_is_refused_with_the_parameter() {
    let refused = |spec: HeightfieldSpec<'_>| generate(&spec).expect_err("refused");
    assert_eq!(
        refused(HeightfieldSpec { columns: 0, ..FLAT }),
        HeightfieldError::GridSize { columns: 0, rows: 1 }
    );
    let too_many = u32::try_from(MAX_VERTICES).expect("fits") + 1;
    assert_eq!(
        refused(HeightfieldSpec { columns: too_many, ..FLAT }),
        HeightfieldError::GridSize { columns: too_many, rows: 1 }
    );
    let layer = NoiseLayer { spacing: 0, amplitude: 5 };
    assert_eq!(
        refused(HeightfieldSpec { layers: &[layer], ..FLAT }),
        HeightfieldError::Layer { layer: Some(0), spacing: 0, amplitude: 5 }
    );
    assert_eq!(
        refused(HeightfieldSpec { meander: NoiseLayer { spacing: 1, amplitude: -1 }, ..FLAT }),
        HeightfieldError::Layer { layer: None, spacing: 1, amplitude: -1 }
    );
    let riser_too_wide = Terraces { count: 1, width: 2, riser: 3, height: 0 };
    assert_eq!(
        refused(HeightfieldSpec { terraces: riser_too_wide, ..FLAT }),
        HeightfieldError::Terraces
    );
    let weights = NoiseWeights { floor: 0, wall: 17, plateau: 0 };
    assert_eq!(
        refused(HeightfieldSpec { weights, ..FLAT }),
        HeightfieldError::Weight { weight: 17 }
    );
    let outside = Shelf { columns: [0, 1], rows: [0, 0], height: 0 };
    assert_eq!(
        refused(HeightfieldSpec { shelves: &[outside], ..FLAT }),
        HeightfieldError::Shelf { shelf: 0 }
    );
    assert_eq!(
        HeightfieldError::Weight { weight: 17 }.to_string(),
        "noise weight 17 is outside 0..=16"
    );
}
