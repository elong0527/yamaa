use yamaa_core::{csv_precision::fixed_point, value::FiniteFloat};

#[test]
fn genuine_binary64_ties_and_adjacent_values_match_independent_text() {
    for (value, digits, expected) in [
        (0.125, 2, "0.13"),
        (-0.125, 2, "-0.13"),
        (2.675, 2, "2.67"),
        (25.0, 4, "25.0000"),
        (0.03125, 4, "0.0313"),
        (0.5, 0, "1"),
        (1.5, 0, "2"),
        (-0.5, 0, "-1"),
        (-0.004, 2, "0.00"),
        (0.0, 3, "0.000"),
        (-0.0, 3, "0.000"),
        (1.234, 2, "1.23"),
        (2.345, 2, "2.35"),
        (f64::from_bits(0x3fbf_ffff_ffff_ffff), 2, "0.12"),
        (f64::from_bits(0x3fc0_0000_0000_0001), 2, "0.13"),
        (-f64::from_bits(0x3fbf_ffff_ffff_ffff), 2, "-0.12"),
        (9.999, 2, "10.00"),
    ] {
        let finite = FiniteFloat::new(value).unwrap();
        assert_eq!(
            fixed_point(finite, digits, expected.len()).unwrap(),
            expected
        );
        assert!(fixed_point(finite, digits, expected.len() - 1).is_err());
        assert_eq!(finite.get().to_bits(), value.to_bits());
    }
}

#[test]
fn large_precision_uses_the_exact_fraction_and_retains_all_declared_zeros() {
    // Exact powers of two give independently known finite decimal expansions.
    for (value, prefix, zero_count) in [
        (0.125, "0.125", 4997),
        (-0.0009765625, "-0.0009765625", 4990),
        (25.0, "25.", 5000),
        (-0.0, "0.", 5000),
    ] {
        let expected = prefix.to_owned() + &"0".repeat(zero_count);
        assert_eq!(
            fixed_point(FiniteFloat::new(value).unwrap(), 5000, expected.len()).unwrap(),
            expected
        );
    }
}

#[test]
fn impossible_width_refuses_without_constructing_large_powers_or_text() {
    for digits in [usize::MAX, usize::MAX - 2, 1_000_000_000] {
        assert!(fixed_point(FiniteFloat::new(0.125).unwrap(), digits, 4096).is_err());
    }
    assert!(fixed_point(FiniteFloat::new(1e308).unwrap(), 0, 308).is_err());
}

#[test]
fn subnormal_values_and_large_integral_values_preserve_fixed_width() {
    let smallest = FiniteFloat::new(f64::from_bits(1)).unwrap();
    assert_eq!(
        fixed_point(smallest, 324, 326).unwrap(),
        "0.".to_owned() + &"0".repeat(323) + "5"
    );
    assert_eq!(
        fixed_point(smallest, 325, 327).unwrap(),
        "0.".to_owned() + &"0".repeat(323) + "49"
    );
    let expected = "100000000000000000000.0000";
    assert_eq!(
        fixed_point(FiniteFloat::new(1e20).unwrap(), 4, expected.len()).unwrap(),
        expected
    );
}
