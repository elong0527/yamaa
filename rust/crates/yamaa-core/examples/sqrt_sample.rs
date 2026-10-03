//! Emit deterministic core results for cross-platform Python math.sqrt comparison.
use yamaa_core::numeric::{sqrt, Number};

/// Emit one finite nonnegative input and the exact result bits; keep negative zero.
fn sample(bits: u64) {
    let input = f64::from_bits(bits);
    if !input.is_finite() || input < 0.0 {
        return;
    }
    let Number::Float(value) = sqrt(Number::float(input), "SQRT(A)").unwrap() else {
        panic!("finite nonnegative input must return float");
    };
    println!("{bits:016x}\t{:016x}", value.get().to_bits());
}

/// Sample all finite exponents, significand boundaries, i64 promotions and random bits.
fn main() {
    sample((-0.0_f64).to_bits());
    for integer in [i64::MAX, (1_i64 << 53) - 1, (1_i64 << 53) + 1] {
        let Number::Float(value) = sqrt(Number::Int(integer), "SQRT(A)").unwrap() else {
            panic!("present integer must return float");
        };
        println!(
            "{:016x}\t{:016x}",
            (integer as f64).to_bits(),
            value.get().to_bits()
        );
    }
    for exponent in 0..2047_u64 {
        for fraction in [0, 1, (1_u64 << 51) - 1, 1_u64 << 51, (1_u64 << 52) - 1] {
            sample((exponent << 52) | fraction);
        }
    }
    let mut bits = 0x1234_5678_9abc_def0_u64;
    for _ in 0..100_000 {
        bits ^= bits << 13;
        bits ^= bits >> 7;
        bits ^= bits << 17;
        sample(bits & !(1_u64 << 63));
    }
}
