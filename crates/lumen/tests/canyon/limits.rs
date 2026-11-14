//! The heightfield's stated ranges: every refusal reached by a constructed spec, the same refusal
//! from `generate`, `bounds` and `centre_256`, acceptance exactly at each limit, and the bounds'
//! arithmetic at the largest spec the ranges allow.

use lumen::scenes::canyon::heightfield::{
    generate, HeightfieldError, HeightfieldSpec, NoiseLayer, NoiseWeights, Shelf, Terraces,
    MAX_AMPLITUDE, MAX_SPACING, MAX_VERTICES,
};

use crate::heightfield::FLAT;

/// `MAX_SPACING` as a count.
const MAX: usize = 1 << 12;

/// The refusal of `spec`, which `generate`, `bounds` and `centre_256` all give, the same.
fn refused(spec: &HeightfieldSpec<'_>) -> HeightfieldError {
    let error = generate(spec).expect_err("refused");
    assert_eq!(spec.bounds(), Err(error), "bounds");
    assert_eq!(spec.centre_256(0), Err(error), "centre_256");
    error
}

/// `spec` accepted by all three.
fn accepted(spec: &HeightfieldSpec<'_>) {
    let field = generate(spec).expect("accepted");
    let [lowest, highest] = spec.bounds().expect("bounds");
    assert!(field.heights().iter().all(|h| (lowest..=highest).contains(h)));
    assert!(spec.centre_256(spec.rows - 1).is_ok(), "centre_256");
}

/// Oracle: each stated range of the grid and the layers, one step outside it on each side.
#[test]
fn a_grid_or_layer_outside_its_range_is_refused_with_the_parameter() {
    let grid = |columns, rows| refused(&HeightfieldSpec { columns, rows, ..FLAT });
    assert_eq!(grid(0, 1), HeightfieldError::GridSize { columns: 0, rows: 1 });
    assert_eq!(grid(1, 0), HeightfieldError::GridSize { columns: 1, rows: 0 });
    assert_eq!(grid(2049, 2048), HeightfieldError::GridSize { columns: 2049, rows: 2048 });
    let wide = u32::try_from(MAX_VERTICES).expect("fits") + 1;
    assert_eq!(grid(wide, 1), HeightfieldError::GridSize { columns: wide, rows: 1 });
    let layers = vec![NoiseLayer { spacing: 1, amplitude: 0 }; MAX + 1];
    let spec = HeightfieldSpec { layers: &layers, ..FLAT };
    assert_eq!(refused(&spec), HeightfieldError::TooManyLayers { layers: MAX + 1 });
    let ok = NoiseLayer { spacing: 1, amplitude: 0 };
    for (spacing, amplitude) in [(0, 0), (MAX_SPACING + 1, 0), (1, -1), (1, MAX_AMPLITUDE + 1)] {
        let bad = NoiseLayer { spacing, amplitude };
        let want = |layer| HeightfieldError::Layer { layer, spacing, amplitude };
        let layers = [ok, bad];
        assert_eq!(refused(&HeightfieldSpec { layers: &layers, ..FLAT }), want(Some(1)));
        assert_eq!(refused(&HeightfieldSpec { meander: bad, ..FLAT }), want(None));
    }
}

/// Oracle: each stated range of the terraces, the weights and the shelves, one step outside it.
#[test]
fn a_terrace_weight_or_shelf_outside_its_range_is_refused_with_the_parameter() {
    let tiers = |count, width, riser, height| Terraces { count, width, riser, height };
    for terraces in [
        tiers(1, 0, 1, 0),
        tiers(1, MAX_SPACING + 1, 1, 0),
        tiers(1, 2, 0, 0),
        tiers(1, 2, 3, 0),
        tiers(MAX_SPACING + 1, 1, 1, 0),
        tiers(1, 1, 1, -1),
        tiers(1, 1, 1, MAX_AMPLITUDE + 1),
    ] {
        let spec = HeightfieldSpec { terraces, ..FLAT };
        assert_eq!(refused(&spec), HeightfieldError::Terraces, "{terraces:?}");
    }
    for weight in [-1, 17] {
        for weights in [
            NoiseWeights { floor: weight, wall: 0, plateau: 0 },
            NoiseWeights { floor: 0, wall: weight, plateau: 0 },
            NoiseWeights { floor: 0, wall: 0, plateau: weight },
        ] {
            let spec = HeightfieldSpec { weights, ..FLAT };
            assert_eq!(refused(&spec), HeightfieldError::Weight { weight });
        }
    }
    let grid = HeightfieldSpec { columns: 3, rows: 3, ..FLAT };
    let fits = Shelf { columns: [0, 2], rows: [0, 2], height: 0 };
    for bad in [
        Shelf { columns: [2, 1], ..fits },
        Shelf { rows: [2, 1], ..fits },
        Shelf { columns: [0, 3], ..fits },
        Shelf { rows: [0, 3], ..fits },
        Shelf { height: MAX_AMPLITUDE + 1, ..fits },
        Shelf { height: -MAX_AMPLITUDE - 1, ..fits },
    ] {
        let shelves = [fits, bad];
        let spec = HeightfieldSpec { shelves: &shelves, ..grid };
        assert_eq!(refused(&spec), HeightfieldError::Shelf { shelf: 1 }, "{bad:?}");
    }
}

/// Oracle: each limit itself is inside its range. The grid of exactly `MAX_VERTICES` and the
/// layer, terrace, weight and shelf limits, each alone, all generate.
#[test]
fn a_spec_exactly_at_each_limit_is_accepted() {
    accepted(&HeightfieldSpec { columns: 2048, rows: 2048, ..FLAT });
    let layers = vec![NoiseLayer { spacing: 1, amplitude: 0 }; MAX];
    accepted(&HeightfieldSpec { layers: &layers, ..FLAT });
    let widest = NoiseLayer { spacing: MAX_SPACING, amplitude: MAX_AMPLITUDE };
    accepted(&HeightfieldSpec { layers: &[widest], meander: widest, ..FLAT });
    let terraces = Terraces { count: MAX_SPACING, width: 4, riser: 4, height: MAX_AMPLITUDE };
    accepted(&HeightfieldSpec { terraces, floor_half_width: 0, ..FLAT });
    let terraces = Terraces { count: 1, width: MAX_SPACING, riser: MAX_SPACING, height: 0 };
    accepted(&HeightfieldSpec { terraces, ..FLAT });
    for weight in [0, 16] {
        accepted(&HeightfieldSpec {
            weights: NoiseWeights { floor: weight, wall: 0, plateau: weight },
            ..FLAT
        });
    }
    let whole = |height| Shelf { columns: [0, 2], rows: [0, 1], height };
    let shelves = [whole(MAX_AMPLITUDE), whole(-MAX_AMPLITUDE)];
    accepted(&HeightfieldSpec { columns: 3, rows: 2, shelves: &shelves, ..FLAT });
}

/// Oracle: the review's probe. A first layer of amplitude `i64::MAX` is refused by `generate`;
/// `bounds` summed the amplitudes unchecked, wrapping to
/// `[-2305843009213693952, -2305843009213693942]` in release and panicking in debug. It now gives
/// `generate`'s refusal, as `centre_256` does for a meander `generate` refuses (an amplitude past
/// `2^48` overflowed its lattice value; a spacing of 0 was read as 1).
#[test]
fn the_helpers_refuse_a_spec_that_wrapped_or_panicked() {
    let layers =
        [NoiseLayer { spacing: 1, amplitude: i64::MAX }, NoiseLayer { spacing: 1, amplitude: 1 }];
    let spec = HeightfieldSpec { layers: &layers, ..FLAT };
    let want = HeightfieldError::Layer { layer: Some(0), spacing: 1, amplitude: i64::MAX };
    assert_eq!(refused(&spec), want);
    for (spacing, amplitude) in [(1, i64::MAX), (0, 1)] {
        let spec = HeightfieldSpec { meander: NoiseLayer { spacing, amplitude }, ..FLAT };
        assert_eq!(refused(&spec), HeightfieldError::Layer { layer: None, spacing, amplitude });
    }
}

/// Oracle: the bounds worked by hand at the largest spec the ranges allow: 4 096 layers of
/// amplitude `2^24` sum to `2^36`, weight 16 quarters spreads it by `2^38` either way, and
/// 4 096 tiers of `2^24` make the rim `2^36`; shelves at `-2^24` and `2^24` lie inside, so
/// `[-2^38, 2^36 + 2^38]`. Every height of a 3 by 2 grid of it lies inside (a debug build
/// would have panicked on any overflow), and so does the centre line at the largest centre
/// and meander, on the last row there is.
#[test]
fn the_bounds_and_the_centre_are_exact_at_the_largest_spec() {
    let layers = vec![NoiseLayer { spacing: MAX_SPACING, amplitude: MAX_AMPLITUDE }; MAX];
    let shelf = |height| Shelf { columns: [0, 2], rows: [0, 0], height };
    let shelves = [shelf(MAX_AMPLITUDE), shelf(-MAX_AMPLITUDE)];
    let spec = HeightfieldSpec {
        seed: u64::MAX,
        columns: 3,
        rows: 2,
        layers: &layers,
        floor_half_width: 0,
        terraces: Terraces { count: MAX_SPACING, width: 1, riser: 1, height: MAX_AMPLITUDE },
        weights: NoiseWeights { floor: 16, wall: 16, plateau: 16 },
        shelves: &shelves,
        ..FLAT
    };
    assert_eq!(spec.bounds(), Ok([-(1 << 38), (1 << 36) + (1 << 38)]));
    accepted(&spec);
    let meander = NoiseLayer { spacing: MAX_SPACING, amplitude: MAX_AMPLITUDE };
    let far = HeightfieldSpec { centre: u32::MAX, meander, ..FLAT };
    let centre = far.centre_256(u32::MAX).expect("accepted");
    let middle = 256 * i64::from(u32::MAX);
    assert!((centre - middle).abs() <= 256 * MAX_AMPLITUDE, "{centre}");
}

/// Oracle: the messages, written out. `OutOfMemory` has no constructed input: it needs the
/// allocator to refuse at most `2^22` heights.
#[test]
fn every_refusal_says_what_was_given() {
    let says = |error: HeightfieldError, text: &str| assert_eq!(error.to_string(), text);
    says(
        HeightfieldError::GridSize { columns: 0, rows: 1 },
        "a grid of 0 by 1 vertices is outside 1..=4194304 vertices",
    );
    says(HeightfieldError::TooManyLayers { layers: 4097 }, "4097 noise layers is more than 4096");
    says(
        HeightfieldError::Layer { layer: Some(2), spacing: 0, amplitude: 5 },
        "noise layer 2 has spacing 0 and amplitude 5; the limits are 1..=4096 and 0..=16777216",
    );
    says(
        HeightfieldError::Layer { layer: None, spacing: 1, amplitude: -1 },
        "the meander has spacing 1 and amplitude -1; the limits are 1..=4096 and 0..=16777216",
    );
    says(HeightfieldError::Terraces, "the terraces are outside their stated ranges");
    says(HeightfieldError::Weight { weight: 17 }, "noise weight 17 is outside 0..=16");
    says(HeightfieldError::Shelf { shelf: 1 }, "shelf 1 is not a rectangle inside the grid");
    says(
        HeightfieldError::OutOfMemory { vertices: 9 },
        "the heights of 9 vertices could not be reserved",
    );
}
