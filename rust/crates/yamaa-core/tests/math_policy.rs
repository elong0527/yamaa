use std::convert::Infallible;
use yamaa_core::evaluation::{EvaluationErrorKind, NumericCondition, NumericResolver, Operand};
use yamaa_core::numeric::{power, ArithmeticErrorKind, Number};
use yamaa_core::numeric_compiler::{
    compile_numeric, compile_numeric_with_policy, CompileError, CompileLimits, MathPolicy,
};
use yamaa_core::value::{Selection, Value};

/// Record each resolution to verify policy preflight and failure ordering.
#[derive(Default)]
struct Resolver(Vec<String>);
impl NumericResolver for Resolver {
    type Error = Infallible;
    /// Bind predictable typed values while retaining absent versus missing.
    fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
        self.0.push(name.into());
        Ok(match name {
            "A" => Selection::Present(Value::Int(2)),
            "M" => Selection::Present(Value::Missing),
            _ => Selection::Absent,
        })
    }
}

/// Existing callers reject the new functions; opt-in cannot bypass other preflight gates.
#[test]
fn numerical_policy_is_explicit_and_preserves_limits() {
    for text in ["EXP(A)", "LN(A)", "POWER(A, A)"] {
        assert!(matches!(
            compile_numeric(text, "spec", CompileLimits::default()),
            Err(CompileError::Unsupported { .. })
        ));
        let plan = compile_numeric_with_policy(
            text,
            "spec",
            CompileLimits::default(),
            MathPolicy::PortableLibmV1,
        )
        .unwrap();
        assert_eq!(plan.math_policy(), MathPolicy::PortableLibmV1);
        let mut resolver = Resolver::default();
        assert!(matches!(plan.evaluate(&mut resolver), Ok(Number::Float(_))));
        assert_eq!(resolver.0.len(), plan.resolution_count());
        assert!(matches!(
            compile_numeric_with_policy(
                text,
                "spec",
                CompileLimits {
                    resolutions: 0,
                    ..CompileLimits::default()
                },
                MathPolicy::PortableLibmV1
            ),
            Err(CompileError::ResolutionLimit { .. })
        ));
    }
    assert_eq!(
        compile_numeric("SQRT(4)", "spec", CompileLimits::default())
            .unwrap()
            .math_policy(),
        MathPolicy::ReferenceSubset
    );
    assert!(matches!(
        compile_numeric_with_policy(
            "EXP(A) + ROUND_HALF_AWAY_FROM_ZERO(A, 2)",
            "spec",
            CompileLimits::default(),
            MathPolicy::PortableLibmV1
        ),
        Err(CompileError::Unsupported { .. })
    ));
}

/// Domain failures belong to calls; operand failures retain their exact inner span.
#[test]
fn math_failure_spans_and_order_are_preserved() {
    for (text, span, route, trace) in [
        ("M + (LN(0))", "LN(0)", vec![Operand::Right], vec!["M"]),
        ("(POWER(-2, 0.5))", "POWER(-2, 0.5)", vec![], vec![]),
        (
            "EXP((UNKNOWN))",
            "UNKNOWN",
            vec![Operand::Unary],
            vec!["UNKNOWN"],
        ),
        (
            "POWER(M, UNKNOWN)",
            "UNKNOWN",
            vec![Operand::Right],
            vec!["M", "UNKNOWN"],
        ),
        ("LN(0) + UNKNOWN", "LN(0)", vec![Operand::Left], vec![]),
    ] {
        let plan = compile_numeric_with_policy(
            text,
            "spec",
            CompileLimits::default(),
            MathPolicy::PortableLibmV1,
        )
        .unwrap();
        let mut resolver = Resolver::default();
        let error = plan.evaluate(&mut resolver).unwrap_err();
        assert_eq!(&text[error.source_span.start..error.source_span.end], span);
        assert_eq!(error.evaluation.location.operands, route);
        assert_eq!(resolver.0, trace);
    }
}

/// Domain context preserves promoted arguments, including the sign of a zero base.
#[test]
fn invalid_power_retains_exact_promoted_diagnostic_arguments() {
    for (base, exponent) in [(-0.0, -1.0), (-2.0, 0.5)] {
        let error = power(Number::float(base), Number::float(exponent), "POWER(A, B)").unwrap_err();
        assert_eq!(error.phase(), "derivation");
        assert_eq!(error.condition(), "invalid_power");
        assert_eq!(error.requirement(), "REQ-0433");
        assert_eq!(
            error.kind,
            ArithmeticErrorKind::InvalidPower {
                base_bits: base.to_bits(),
                exponent_bits: exponent.to_bits()
            }
        );
    }
    let plan = compile_numeric_with_policy(
        "POWER(-0.0, -1)",
        "spec",
        CompileLimits::default(),
        MathPolicy::PortableLibmV1,
    )
    .unwrap();
    let error = plan.evaluate(&mut Resolver::default()).unwrap_err();
    assert_eq!(
        error.evaluation.kind,
        EvaluationErrorKind::Numeric(NumericCondition::Arithmetic(
            ArithmeticErrorKind::InvalidPower {
                base_bits: (-0.0_f64).to_bits(),
                exponent_bits: (-1.0_f64).to_bits()
            }
        ))
    );
}

/// Portable math cannot discard a resolver's opaque failure even after a missing operand.
#[test]
fn portable_math_retains_opaque_resolver_failures() {
    #[derive(Debug, PartialEq, Eq)]
    struct Failure(Box<i32>);
    struct FailedResolver;
    impl NumericResolver for FailedResolver {
        type Error = Failure;
        /// Return an owned non-Clone payload to prove no diagnostic reconstruction occurs.
        fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
            assert_eq!(name, "BROKEN");
            Err(Failure(Box::new(42)))
        }
    }
    for text in ["EXP(BROKEN)", "LN(BROKEN)", "POWER(NULL, BROKEN)"] {
        let plan = compile_numeric_with_policy(
            text,
            "spec",
            CompileLimits::default(),
            MathPolicy::PortableLibmV1,
        )
        .unwrap();
        let error = plan.evaluate(&mut FailedResolver).unwrap_err();
        assert_eq!(
            &text[error.source_span.start..error.source_span.end],
            "BROKEN"
        );
        assert_eq!(
            error.evaluation.kind,
            EvaluationErrorKind::Resolution {
                identifier: "BROKEN".into(),
                error: Failure(Box::new(42))
            }
        );
    }
}
