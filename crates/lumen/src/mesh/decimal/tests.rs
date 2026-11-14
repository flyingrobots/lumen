//! Tests of the decimal conversion.

use super::*;

fn raw(text: &str) -> i64 {
    decimal_to_q32_32(text).expect("a decimal").raw()
}

/// Exact values derived by hand. For `m / 10^k`, the raw value is `m * 2^32 / 10^k`
/// rounded to nearest, ties to even; `2^32 = 4294967296`.
#[test]
fn decimals_convert_exactly_with_ties_to_even() {
    assert_eq!(raw("0"), 0);
    assert_eq!(raw("-0"), 0);
    assert_eq!(raw("1"), 1 << 32);
    assert_eq!(raw("1.5"), 3 << 31);
    assert_eq!(raw("-2.25"), -(9 << 30));
    assert_eq!(raw("0.25e1"), 5 << 31);
    assert_eq!(raw("25E-2"), 1 << 30);
    assert_eq!(raw(".5"), 1 << 31);
    assert_eq!(raw("5."), 5 << 32);
    // 0.1: 4294967296 / 10 = 429496729.6, rounds up.
    assert_eq!(raw("0.1"), 429_496_730);
    // 2^-33 = 5^33 / 10^33 = 1.16415321826934814453125e-10, exactly half a raw unit: a tie,
    // to even (0). Three halves round to 2, five halves to 2, and the sign is symmetric.
    assert_eq!(raw("1.16415321826934814453125e-10"), 0);
    assert_eq!(raw("3.49245965480804443359375e-10"), 2);
    assert_eq!(raw("5.82076609134674072265625e-10"), 2);
    assert_eq!(raw("-3.49245965480804443359375e-10"), -2);
    // Just above and just below the tie.
    assert_eq!(raw("1.16415321826934814453126e-10"), 1);
    assert_eq!(raw("1.16415321826934814453124e-10"), 0);
    // Far below half a raw unit, with a denominator too large for u128.
    assert_eq!(raw("1e-40"), 0);
    // The largest value below 2^31 and the first one beyond.
    assert_eq!(raw("2147483647"), 2_147_483_647 << 32);
    assert_eq!(decimal_to_q32_32("2147483648"), Err(DecimalError::OutOfRange));
    assert_eq!(decimal_to_q32_32("1e30"), Err(DecimalError::OutOfRange));
}

/// The bunny's numbers, derived by hand (`q` and `r` are the quotient and remainder of
/// `m * 2^32` by `10^k`).
#[test]
fn bunny_coordinates_convert_to_their_hand_derived_raw_values() {
    // The first vertex, (-0.0369122, 0.127512, 0.00276757):
    // 369122 * 2^32 = 1585366918234112 = 158536691 * 10^7 + 8234112; 2r > 10^7, round up.
    assert_eq!(raw("-0.0369122"), -158_536_692);
    // 127512 * 2^32 = 547659869847552 = 547659869 * 10^6 + 847552; round up.
    assert_eq!(raw("0.127512"), 547_659_870);
    // 276757 * 2^32 = 1188662263939072 = 11886622 * 10^8 + 63939072; round up.
    assert_eq!(raw("0.00276757"), 11_886_623);
    // The bounds of `bunny.mesh.json`:
    // 943643 * 2^32 = 4052915824099328 = 405291582 * 10^7 + 4099328; 2r < 10^7, round down.
    assert_eq!(raw("-0.0943643"), -405_291_582);
    // 334143 * 2^32 = 1435133257187328 = 143513325 * 10^7 + 7187328; round up.
    assert_eq!(raw("0.0334143"), 143_513_326);
    // 616721 * 2^32 = 2648796525756416 = 264879652 * 10^7 + 5756416; round up.
    assert_eq!(raw("-0.0616721"), -264_879_653);
    // 609346 * 2^32 = 2617121141948416 = 261712114 * 10^7 + 1948416; round down.
    assert_eq!(raw("0.0609346"), 261_712_114);
    // 184813 * 2^32 = 793765790875648 = 793765790 * 10^6 + 875648; round up.
    assert_eq!(raw("0.184813"), 793_765_791);
    // 584651 * 2^32 = 2511056924573696 = 251105692 * 10^7 + 4573696; round down.
    assert_eq!(raw("0.0584651"), 251_105_692);
    // The file's one exponent, 8.20736e-05 = 820736 / 10^10:
    // 820736 * 2^32 = 3525034278649856 = 352503 * 10^10 + 4278649856; round down.
    assert_eq!(raw("8.20736e-05"), 352_503);
}

/// A xorshift generator, so the generated cases are the same on every run.
fn xorshift(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

/// Over generated decimals `m / 10^k` (up to 12 digits, `k` from 3 to 15, so below 10^9 and
/// inside the Q32.32 range, either sign), the raw
/// value `q` satisfies the definition of rounding to nearest, checked by multiplication
/// rather than division: `2 * |m * 2^32 - q * 10^k| <= 10^k`, and on equality `q` is even.
/// Over generated exact ties `(2n + 1) / 2^33 = (2n + 1) * 5^33 / 10^33`, the raw value is
/// the even neighbour of `n + 1/2`.
#[test]
fn generated_decimals_round_to_nearest_with_ties_to_even() {
    const SEED: u64 = 0x5eed_f1ce_b00c_0001;
    let mut state = SEED;
    for case in 0..20_000 {
        let mantissa = u128::from(xorshift(&mut state) % 1_000_000_000_000);
        let k = 3 + u32::try_from(xorshift(&mut state) % 13).expect("small");
        let negative = xorshift(&mut state) % 2 == 1;
        let digits = format!("{mantissa:0>width$}", width = usize::try_from(k).expect("small") + 1);
        let (int, frac) = digits.split_at(digits.len() - usize::try_from(k).expect("small"));
        let text = format!("{}{int}.{frac}", if negative { "-" } else { "" });
        let got = raw(&text);
        let q = u128::from(got.unsigned_abs());
        assert!(got == 0 || (got < 0) == negative, "seed {SEED:#x} case {case}: {text}");
        let ten_k = 10_u128.pow(k);
        let error = (mantissa << 32).abs_diff(q * ten_k);
        assert!(2 * error <= ten_k, "seed {SEED:#x} case {case}: {text} gave {got}");
        if 2 * error == ten_k {
            assert_eq!(q % 2, 0, "seed {SEED:#x} case {case}: {text} gave {got}");
        }
    }
    // Ties are rare among random decimals; the constructed ones below cover them.
    let five_33 = 5_u128.pow(33);
    for case in 0..2_000 {
        let n = u128::from(xorshift(&mut state) % 42_000);
        let text = format!("{}e-33", (2 * n + 1) * five_33);
        let want = i64::try_from(if n % 2 == 0 { n } else { n + 1 }).expect("small");
        assert_eq!(raw(&text), want, "seed {SEED:#x} tie case {case}: {text}");
        assert_eq!(raw(&format!("-{text}")), -want, "seed {SEED:#x} tie case {case}");
    }
}

#[test]
fn malformed_decimals_are_refused() {
    for (text, error) in [
        ("", DecimalError::Empty),
        ("-", DecimalError::Malformed),
        (".", DecimalError::Malformed),
        ("+1", DecimalError::Malformed),
        ("--1", DecimalError::Malformed),
        ("1-", DecimalError::Malformed),
        ("0x10", DecimalError::Malformed),
        ("1.0\r", DecimalError::Malformed),
        (" 1", DecimalError::Malformed),
        ("nan", DecimalError::Malformed),
        ("e5", DecimalError::Malformed),
        ("1.2.3", DecimalError::SecondPoint),
        ("-0.03.5", DecimalError::SecondPoint),
        ("1e", DecimalError::Exponent),
        ("1e+", DecimalError::Exponent),
        ("1e12345", DecimalError::Exponent),
        ("1e1.5", DecimalError::Malformed),
        ("1e-1e", DecimalError::Malformed),
        ("12345678901234567890123456789", DecimalError::TooManyDigits),
    ] {
        assert_eq!(decimal_to_q32_32(text), Err(error), "{text:?}");
    }
    // Leading zeros are not significant digits.
    assert_eq!(raw("0000000000000000000000000000001"), 1 << 32);
}
