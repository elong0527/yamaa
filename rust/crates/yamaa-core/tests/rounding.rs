use yamaa_core::evaluation::{EvaluationErrorKind, NumericCondition, NumericResolver, Operand};
use yamaa_core::numeric::{round_half_away_from_zero as rounded, ArithmeticErrorKind, Number};
use yamaa_core::numeric_compiler::{
    compile_numeric_with_policy, CompileError, CompileLimits, MathPolicy,
};
use yamaa_core::value::{Selection, Value};

/// Missing suppresses digits validation only after both operand values are available.
#[test]
fn rounding_missing_type_and_range_contracts() {
    assert_eq!(
        rounded(Number::Missing, Number::float(1.0), "expr"),
        Ok(Number::Missing)
    );
    assert_eq!(
        rounded(Number::Int(1), Number::Missing, "expr"),
        Ok(Number::Missing)
    );
    let error = rounded(Number::Int(1), Number::float(1.0), "expr").unwrap_err();
    assert_eq!(error.kind, ArithmeticErrorKind::InvalidRoundingDigits);
    assert_eq!(error.expression, "expr");
    assert_eq!(error.phase(), "validation");
    assert_eq!(error.condition(), "incompatible_input_type");
    assert_eq!(error.requirement(), "REQ-0418");
    for input in [f64::MAX, -f64::MAX] {
        assert_eq!(
            rounded(Number::float(input), Number::Int(-308), "expr"),
            Ok(Number::Missing)
        );
    }
    for input in [
        -0.0,
        0.0,
        f64::from_bits(1),
        -f64::from_bits(1),
        f64::MAX,
        -f64::MAX,
    ] {
        for (digits, expected) in [
            (i64::MIN, 0.0),
            (i64::MAX, if input == 0.0 { 0.0 } else { input }),
        ] {
            let Number::Float(result) =
                rounded(Number::float(input), Number::Int(digits), "expr").unwrap()
            else {
                panic!("float required")
            };
            assert_eq!(result.get().to_bits(), expected.to_bits());
        }
    }
}

/// A non-clone resolver failure must survive rounding without being replaced by missing.
#[derive(Debug)]
struct Opaque(String);

struct Resolver(Vec<String>);
impl NumericResolver for Resolver {
    type Error = Opaque;
    /// Resolve numeric/missing values and a deliberate callback failure in written order.
    fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
        self.0.push(name.into());
        match name {
            "BROKEN" => Err(Opaque("opaque failure".into())),
            "A" => Ok(Selection::Present(Value::Int(1))),
            "D" => Ok(Selection::Present(Value::float(1.0))),
            _ => Ok(Selection::Absent),
        }
    }
}

/// Both policies retain budgets, call/operand source spans and eager resolver failures.
#[test]
fn rounding_compilation_preserves_provenance_and_limits() {
    for policy in [MathPolicy::ReferenceSubset, MathPolicy::PortableLibmV1] {
        assert!(matches!(
            compile_numeric_with_policy(
                "ROUND_HALF_AWAY_FROM_ZERO(A, D)",
                "spec",
                CompileLimits {
                    resolutions: 1,
                    ..CompileLimits::default()
                },
                policy
            ),
            Err(CompileError::ResolutionLimit { required: 2, .. })
        ));
        for (text, span, path, expected_trace) in [
            (
                "1 + ROUND_HALF_AWAY_FROM_ZERO(A, D)",
                "ROUND_HALF_AWAY_FROM_ZERO(A, D)",
                vec![Operand::Right],
                vec!["A", "D"],
            ),
            (
                "ROUND_HALF_AWAY_FROM_ZERO(NULL, BROKEN)",
                "BROKEN",
                vec![Operand::Right],
                vec!["BROKEN"],
            ),
        ] {
            let plan = compile_numeric_with_policy(text, "spec", CompileLimits::default(), policy)
                .unwrap();
            let mut resolver = Resolver(Vec::new());
            let error = plan.evaluate(&mut resolver).unwrap_err();
            assert_eq!(&text[error.source_span.start..error.source_span.end], span);
            assert_eq!(error.evaluation.location.spec_path, "spec");
            assert_eq!(error.evaluation.location.operands, path);
            assert_eq!(resolver.0, expected_trace);
            match error.evaluation.kind {
                EvaluationErrorKind::Numeric(NumericCondition::Arithmetic(
                    ArithmeticErrorKind::InvalidRoundingDigits,
                )) => assert!(text.starts_with("1 +")),
                EvaluationErrorKind::Resolution { identifier, error } => {
                    assert_eq!(identifier, "BROKEN");
                    assert_eq!(error.0, "opaque failure");
                }
                _ => panic!("unexpected error"),
            }
        }
    }
}
