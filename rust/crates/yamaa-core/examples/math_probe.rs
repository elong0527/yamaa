//! Assess candidate libm operations without enabling them in the core evaluator.
//! Outputs are observations, never expected fixture values or backend qualification.

/// Emit exact input/output bits before the assessment runner normalizes nonfinite results.
fn sample(id: &str, x: f64, y: f64) {
    for (name, a, b, result) in [
        ("EXP", x, 0.0, libm::exp(x)),
        ("LN", y, 0.0, libm::log(y)),
        ("POWER", y, x / 100.0, libm::pow(y, x / 100.0)),
    ] {
        println!(
            "{id}\t{name}\t{:016x}\t{:016x}\t{:016x}",
            a.to_bits(),
            b.to_bits(),
            result.to_bits()
        );
    }
}

/// Cover reproducible finite domains, range transitions and observed mismatch witnesses.
fn main() {
    for (index, (x, y)) in [
        (-0.0, 1.0),
        (0.0, 2.0),
        (1.0, 0.5),
        (-1.0, f64::from_bits(1)),
        (709.0, f64::MIN_POSITIVE),
        (710.0, f64::MAX),
        (-744.0, f64::from_bits(0x3fef_ffff_ffff_ffff)),
        (-746.0, f64::from_bits(0x3ff0_0000_0000_0001)),
        (f64::MAX, 1.0),
        (-f64::MAX, 1.0),
        // Inputs observed to differ on macOS; no output is asserted as expected truth.
        (
            f64::from_bits(0xc050_98b1_4e6b_a5d0),
            f64::from_bits(0x4047_354b_5f3e_6095),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        sample(&format!("boundary-{index}"), x, y);
    }
    let mut state = 1585_u64;
    for index in 0..10_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let unit = (state >> 11) as f64 / ((1_u64 << 53) as f64);
        sample(
            &format!("sample-{index}"),
            unit * 1400.0 - 700.0,
            unit * 200.0 + 0.001,
        );
    }
}
