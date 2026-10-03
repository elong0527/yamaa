//! Emit a deterministic conversion sample for the Python differential check.

use yamaa_core::{conversion::float_text, value::FiniteFloat};

/// Print lossless input bits beside the core's canonical positional output.
fn emit(bits: u64) {
    if let Some(value) = FiniteFloat::new(f64::from_bits(bits)) {
        println!("{bits:016x}\t{}", float_text(value));
    }
}

/// Include exact endpoints and a repeatable sample spanning signs and exponents.
fn main() {
    for bits in [
        0,
        1,
        0x8000000000000000,
        0x0010000000000000,
        0x7fefffffffffffff,
        0xffefffffffffffff,
    ] {
        emit(bits);
    }
    let mut bits = 0x123456789abcdef0_u64;
    for _ in 0..20000 {
        bits ^= bits << 13;
        bits ^= bits >> 7;
        bits ^= bits << 17;
        emit(bits);
    }
}
