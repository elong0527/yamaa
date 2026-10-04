//! Assess the opt-in portable math policy without changing default compilation.
//! Outputs are observations, never expected fixture values or backend qualification.

use std::convert::Infallible;
use yamaa_core::evaluation::NumericResolver;
use yamaa_core::numeric::Number;
use yamaa_core::numeric_compiler::{compile_numeric_with_policy, CompileLimits, MathPolicy};
use yamaa_core::value::{Selection, Value};

/// Supply normalized values through the same compiled evaluator used by callers.
struct Inputs(f64, f64);
impl NumericResolver for Inputs {
    type Error = Infallible;
    /// Resolve exactly the two probe operand names.
    fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
        Ok(Selection::Present(Value::float(match name {
            "A" => self.0,
            "B" => self.1,
            _ => panic!("unknown probe operand"),
        })))
    }
}

/// Emit exact bits from the opt-in compiled policy, before assessment normalization.
fn sample(id: &str, x: f64, y: f64) {
    for (name, a, b, source) in [
        ("EXP", x, 0.0, "EXP(A)"),
        ("LN", y, 0.0, "LN(A)"),
        ("POWER", y, x / 100.0, "POWER(A, B)"),
    ] {
        let plan = compile_numeric_with_policy(
            source,
            "assessment",
            CompileLimits::default(),
            MathPolicy::PortableLibmV1,
        )
        .unwrap();
        let result = match plan.evaluate(&mut Inputs(a, b)).unwrap() {
            Number::Float(value) => value.get(),
            Number::Missing => f64::NAN,
            Number::Int(_) => panic!("math policy must return float"),
        };
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
