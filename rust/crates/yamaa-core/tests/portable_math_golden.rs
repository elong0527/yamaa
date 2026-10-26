//! Portable math: the authored PortableLibmV1 table, evaluated through the
//! expression compiler to its exact bits and domain failures.
//!
//! The seam is the compiled expression's `evaluate_number`: its result's
//! exact bits, its typed arithmetic failure and span, and the operands its
//! resolver is asked for, which no replay case or benchmark observes. The
//! test is the only reader of `fixtures/portable_math.tsv`.
use std::convert::Infallible;
use yamaa_core::expression::{compile, Condition, Context, ErrorKind, Resolver as NumericResolver};
use yamaa_core::numeric::{ArithmeticErrorKind, Number};
use yamaa_core::value::{Selection, Value};

// Exact PortableLibmV1 inputs and results, the named NORM_CDF formula and
// NORM_INV algorithm among them, including binary64 range boundaries,
// signed zero, missingness, domain failures and full-range integer promotion.
const CASES: &str = include_str!("fixtures/portable_math.tsv");

fn input(text: &str) -> Value {
    match text {
        "missing" | "-" => Value::Missing,
        _ if text.starts_with("int:") => Value::Int(text[4..].parse().unwrap()),
        _ => Value::float(f64::from_bits(u64::from_str_radix(text, 16).unwrap())),
    }
}

struct Inputs {
    values: [Value; 2],
    resolutions: Vec<&'static str>,
}

impl NumericResolver for Inputs {
    type Error = Infallible;

    fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
        let (index, name) = match name {
            "A" => (0, "A"),
            "B" => (1, "B"),
            _ => panic!("unexpected operand {name}"),
        };
        self.resolutions.push(name);
        Ok(Selection::Present(self.values[index].clone()))
    }
}

#[test]
fn portable_math_matches_exact_bits_and_domain_failures() {
    let rows: Vec<_> = CASES.trim().lines().collect();
    assert_eq!(rows.len(), 100);
    for row in rows {
        let fields: Vec<_> = row.split_whitespace().collect();
        let [name, function, left, right, expected] = fields.as_slice() else {
            panic!("invalid math case: {row}");
        };
        let expression = match *function {
            "EXP" => "EXP(A)",
            "LN" => "LN(A)",
            "LOG10" => "LOG10(A)",
            "NORM_CDF" => "NORM_CDF(A)",
            "NORM_INV" => "NORM_INV(A)",
            "POWER" => "POWER(A, B)",
            _ => panic!("unexpected function {function}"),
        };
        let plan = compile(expression, Context::Derivation, "columns.VALUE.derivation").unwrap();
        let mut resolver = Inputs {
            values: [input(left), input(right)],
            resolutions: Vec::new(),
        };
        let result = plan.evaluate_number(&mut resolver);
        match (*expected, result) {
            ("missing", Ok(Number::Missing)) => {}
            ("ln_of_nonpositive" | "invalid_probability" | "invalid_power", Err(error)) => {
                let kind = match *expected {
                    "ln_of_nonpositive" => ArithmeticErrorKind::LnOfNonpositive { function },
                    "invalid_probability" => ArithmeticErrorKind::InvalidProbability {
                        value_bits: u64::from_str_radix(left, 16).unwrap(),
                    },
                    _ => ArithmeticErrorKind::InvalidPower {
                        base_bits: u64::from_str_radix(left, 16).unwrap(),
                        exponent_bits: u64::from_str_radix(right, 16).unwrap(),
                    },
                };
                assert_eq!(
                    error.kind,
                    ErrorKind::Condition(Condition::Arithmetic(kind)),
                    "{name}"
                );
                let span = error.source_span.expect("arithmetic span");
                assert_eq!(span.start, 0, "{name}");
                assert_eq!(span.end, expression.len(), "{name}");
                assert!(error.route.is_empty(), "{name}");
            }
            (bits, Ok(Number::Float(value))) => {
                assert_eq!(
                    value.get().to_bits(),
                    u64::from_str_radix(bits, 16).unwrap(),
                    "{name}"
                );
            }
            (_, actual) => panic!("{name}: expected {expected}, got {actual:?}"),
        }
        let expected_resolutions: &[&str] = if *function == "POWER" {
            &["A", "B"]
        } else {
            &["A"]
        };
        assert_eq!(resolver.resolutions, expected_resolutions, "{name}");
    }
}
