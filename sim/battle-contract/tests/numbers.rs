//! Safe-integer bounds on raw Rust values and on JSON tokens.

use sve_battle_contract::{HostError, Int, MAX_SAFE_INTEGER, Nat, decode, encode};

const MAX: &str = "9007199254740991";
const MIN: &str = "-9007199254740991";

#[test]
fn bounds_round_trip_exactly() {
    assert_eq!(MAX.parse(), Ok(MAX_SAFE_INTEGER));
    assert_eq!(decode::<Nat>(MAX), Ok(Nat::MAX));
    assert_eq!(decode::<Nat>("0"), Ok(Nat::ZERO));
    assert_eq!(decode::<Int>(MAX), Ok(Int::MAX));
    assert_eq!(decode::<Int>(MIN), Ok(Int::MIN));
    assert_eq!(decode::<Int>("0"), Ok(Int::ZERO));
    assert_eq!(encode(&Nat::MAX).as_deref(), Ok(MAX));
    assert_eq!(encode(&Int::MAX).as_deref(), Ok(MAX));
    assert_eq!(encode(&Int::MIN).as_deref(), Ok(MIN));
    assert_eq!(Nat::MAX.get(), MAX_SAFE_INTEGER);
    assert_eq!(Int::MIN.get().unsigned_abs(), MAX_SAFE_INTEGER);
    assert_eq!(Int::from(Nat::MAX), Int::MAX);
}

#[test]
fn json_numbers_outside_the_safe_range_are_a_range_error() {
    for token in [
        "9007199254740992",
        "9007199254740993",
        "18446744073709551615",
        "18446744073709551616",
        "-9007199254740992",
        "-9223372036854775808",
        "-18446744073709551616",
    ] {
        assert_eq!(decode::<Int>(token), Err(HostError::RANGE), "{token}");
        assert_eq!(decode::<Nat>(token), Err(HostError::RANGE), "{token}");
    }
}

#[test]
fn json_fractions_and_exponents_are_refused_without_rounding() {
    // 9007199254740991.4 would round to a safe integer if it were parsed as a float first.
    for token in [
        "1.0",
        "1.5",
        "1e3",
        "-0",
        "-0.0",
        "9007199254740991.4",
        "1E2",
    ] {
        assert_eq!(decode::<Int>(token), Err(HostError::RANGE), "{token}");
        assert_eq!(decode::<Nat>(token), Err(HostError::RANGE), "{token}");
    }
}

#[test]
fn non_finite_and_non_numeric_tokens_are_a_format_error() {
    for token in [
        "NaN",
        "Infinity",
        "-Infinity",
        "1e999",
        "\"1\"",
        "null",
        "true",
        "01",
        "",
    ] {
        assert_eq!(decode::<Int>(token), Err(HostError::FORMAT), "{token}");
        assert_eq!(decode::<Nat>(token), Err(HostError::FORMAT), "{token}");
    }
}

#[test]
fn a_negative_count_is_refused() {
    assert_eq!(decode::<Nat>("-1"), Err(HostError::FORMAT));
    assert_eq!(decode::<Int>("-1").map(Int::get), Ok(-1));
}

#[test]
fn unsafe_numbers_are_found_anywhere_in_a_message() {
    assert_eq!(
        decode::<Vec<Nat>>("[1, 9007199254740992]"),
        Err(HostError::RANGE)
    );
    assert_eq!(
        decode::<Vec<Nat>>(r#"[{"deep": [0.5]}]"#),
        Err(HostError::RANGE)
    );
    assert_eq!(decode::<Vec<Nat>>("[1, 2"), Err(HostError::FORMAT));
}

#[test]
fn raw_integers_are_checked_before_conversion() {
    assert_eq!(Nat::try_from(MAX_SAFE_INTEGER), Ok(Nat::MAX));
    assert_eq!(Nat::try_from(2_u64.pow(53)), Err(HostError::RANGE));
    assert_eq!(Nat::try_from(u64::MAX), Err(HostError::RANGE));
    assert_eq!(Nat::try_from(-1_i64), Err(HostError::RANGE));
    assert_eq!(Nat::try_from(7_i64).map(Nat::get), Ok(7));
    assert_eq!(Nat::try_from(i64::MAX), Err(HostError::RANGE));
    assert_eq!(Nat::try_from(7_usize).map(Nat::get), Ok(7));
    assert_eq!(Nat::try_from(usize::MAX), Err(HostError::RANGE));
    assert_eq!(Nat::from(7_u32).get(), 7);

    assert_eq!(Int::try_from(Int::MAX.get()), Ok(Int::MAX));
    assert_eq!(Int::try_from(Int::MIN.get()), Ok(Int::MIN));
    assert_eq!(Int::try_from(2_i64.pow(53)), Err(HostError::RANGE));
    assert_eq!(Int::try_from((-2_i64).pow(53)), Err(HostError::RANGE));
    assert_eq!(Int::try_from(i64::MIN), Err(HostError::RANGE));
    assert_eq!(Int::try_from(MAX_SAFE_INTEGER), Ok(Int::MAX));
    assert_eq!(Int::try_from(2_u64.pow(53)), Err(HostError::RANGE));
    assert_eq!(Int::try_from(u64::MAX), Err(HostError::RANGE));
    assert_eq!(Int::from(-7_i32).get(), -7);
}

#[test]
fn raw_floats_must_be_finite_safe_integers() {
    assert_eq!(Int::try_from(-3.0_f64).map(Int::get), Ok(-3));
    assert_eq!(Nat::try_from(3.0_f64).map(Nat::get), Ok(3));
    assert_eq!(Nat::try_from(-0.0_f64), Ok(Nat::ZERO));
    assert_eq!(Int::try_from(9_007_199_254_740_991.0_f64), Ok(Int::MAX));
    assert_eq!(Int::try_from(-9_007_199_254_740_991.0_f64), Ok(Int::MIN));
    for value in [
        0.5_f64,
        -1.5_f64,
        9_007_199_254_740_992.0_f64,
        -9_007_199_254_740_992.0_f64,
        f64::MAX,
        f64::MIN_POSITIVE,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        assert_eq!(Int::try_from(value), Err(HostError::RANGE), "{value}");
        assert_eq!(Nat::try_from(value), Err(HostError::RANGE), "{value}");
    }
    assert_eq!(Nat::try_from(-3.0_f64), Err(HostError::RANGE));
}
