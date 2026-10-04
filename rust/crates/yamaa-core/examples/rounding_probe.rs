//! Execute supplied rounding inputs through both actual compiled math policies.
use std::convert::Infallible;
use std::io::{self, BufRead};
use yamaa_core::evaluation::NumericResolver;
use yamaa_core::numeric::Number;
use yamaa_core::numeric_compiler::{compile_numeric_with_policy, CompileLimits, MathPolicy};
use yamaa_core::value::{Selection, Value};

/// Supply normalized input and integer digits without host table types.
struct Inputs(f64, i64);
impl NumericResolver for Inputs {
    type Error = Infallible;
    /// Resolve the two named operands used by the compiled probe expression.
    fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
        Ok(Selection::Present(match name {
            "A" => Value::float(self.0),
            "D" => Value::Int(self.1),
            _ => panic!("unexpected probe name"),
        }))
    }
}

/// Echo case identity and inputs with exact result bits for independent checking.
fn main() {
    let plans = [MathPolicy::ReferenceSubset, MathPolicy::PortableLibmV1].map(|policy| {
        compile_numeric_with_policy(
            "ROUND_HALF_AWAY_FROM_ZERO(A, D)",
            "rounding-probe",
            CompileLimits::default(),
            policy,
        )
        .unwrap()
    });
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 3);
        let input = f64::from_bits(u64::from_str_radix(fields[1], 16).unwrap());
        assert!(input.is_finite());
        let digits = fields[2].parse().unwrap();
        for plan in &plans {
            let result = match plan.evaluate(&mut Inputs(input, digits)).unwrap() {
                Number::Float(value) => format!("{:016x}", value.get().to_bits()),
                Number::Missing => "missing".into(),
                Number::Int(_) => panic!("rounding must return float or missing"),
            };
            println!(
                "{}\t{:?}\t{}\t{}\t{}",
                fields[0],
                plan.math_policy(),
                fields[1],
                digits,
                result
            );
        }
    }
}
