use std::convert::Infallible;

use yamaa_core::evaluation::{EvaluationErrorKind, NumericCondition, NumericResolver, Operand};
use yamaa_core::numeric::Number;
use yamaa_core::numeric_compiler::{compile_numeric, CompileError, CompileLimits, CompiledNumeric};
use yamaa_core::numeric_parser::{GrammarFailure, NumericFunction, ParseError, ParseResource};
use yamaa_core::value::{Selection, Value};

/// Compile a test expression through the public source-to-plan entry point.
fn compile(text: &str) -> CompiledNumeric {
    compile_numeric(
        text,
        "columns.RESULT.derivation.compute",
        CompileLimits::default(),
    )
    .unwrap()
}

/// Record calls while changing A's value so plan reuse cannot hide memoization.
#[derive(Default)]
struct Resolver {
    trace: Vec<String>,
}
impl NumericResolver for Resolver {
    type Error = Infallible;
    /// Preserve present missing versus absent; each A occurrence has a new integer.
    fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
        self.trace.push(name.into());
        Ok(match name {
            "A" => Selection::Present(Value::Int(self.trace.len() as i64)),
            "M" => Selection::Present(Value::Missing),
            "B" => Selection::Present(Value::Int(2)),
            _ => Selection::Absent,
        })
    }
}

/// Extract the numeric outcome from a resolver that cannot return a boundary error.
fn numeric_condition(kind: EvaluationErrorKind<Infallible>) -> NumericCondition {
    let EvaluationErrorKind::Numeric(condition) = kind;
    condition
}

/// Replay exact types/bits and deferred literal diagnostics from independent truth.
#[test]
fn shared_literal_vectors_through_compilation() {
    for row in include_str!("fixtures/numeric_literals.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 3);
        let mut resolver = Resolver::default();
        let actual = match compile(fields[1]).evaluate(&mut resolver) {
            Ok(Number::Missing) => "missing".into(),
            Ok(Number::Int(value)) => format!("int:{value}"),
            Ok(Number::Float(value)) => format!("float:{:016x}", value.get().to_bits()),
            Err(error) => {
                assert_eq!(error.evaluation.location.expression, fields[1]);
                let condition = numeric_condition(error.evaluation.kind);
                assert_eq!(condition.phase(), "derivation");
                assert_eq!(condition.condition(), "integer_overflow");
                assert_eq!(condition.requirement(), "REQ-0434");
                let NumericCondition::LiteralOverflow { value } = condition else {
                    panic!("literal failure required");
                };
                assert_eq!(
                    fields[1][error.source_span.start..error.source_span.end]
                        .trim_start_matches('0'),
                    value
                );
                format!("error:integer_overflow:REQ-0434:{value}")
            }
        };
        assert_eq!(actual, fields[2], "{}", fields[0]);
        assert!(resolver.trace.is_empty());
    }
}

/// Literal failures happen at evaluation time and never hide earlier failures.
#[test]
fn oversized_literals_preserve_order_and_missing_does_not_short_circuit() {
    let digits = "9".repeat(5000);
    for (text, expected, trace) in [
        (format!("A + {digits} + B"), "integer_overflow", vec!["A"]),
        (format!("M + {digits} + B"), "integer_overflow", vec!["M"]),
        (format!("{digits} + A"), "integer_overflow", vec![]),
        (
            format!("UNKNOWN + {digits}"),
            "unknown_field",
            vec!["UNKNOWN"],
        ),
        (format!("1 / 0 + {digits}"), "division_by_zero", vec![]),
    ] {
        let plan = compile(&text);
        let mut resolver = Resolver::default();
        let error = plan.evaluate(&mut resolver).unwrap_err();
        let condition = numeric_condition(error.evaluation.kind);
        assert_eq!(condition.condition(), expected);
        assert_eq!(resolver.trace, trace);
    }
}

/// Supported plans retain exact semantic spans even under redundant groups/Unicode.
#[test]
fn source_spans_identify_the_failed_node() {
    for (text, expected, route) in [
        ("\u{2003}M + ((UNKNOWN))", "UNKNOWN", vec![Operand::Right]),
        ("M + ((1 / 0))", "1 / 0", vec![Operand::Right]),
        (
            "-((9223372036854775808))",
            "9223372036854775808",
            vec![Operand::Unary],
        ),
        ("ABS((1 / 0))", "1 / 0", vec![Operand::Unary]),
        ("MOD(A, (UNKNOWN))", "UNKNOWN", vec![Operand::Right]),
        ("(MOD(A, 0))", "MOD(A, 0)", vec![]),
    ] {
        let plan = compile(text);
        let error = plan.evaluate(&mut Resolver::default()).unwrap_err();
        assert_eq!(
            &text[error.source_span.start..error.source_span.end],
            expected
        );
        assert_eq!(error.evaluation.location.operands, route);
        assert_eq!(error.evaluation.location.spec_path, plan.spec_path());
        assert_eq!(error.evaluation.location.expression, plan.expression());
    }
}

/// Unknown grammar functions differ from allowed functions outside this executor.
#[test]
fn unsupported_features_are_preflighted_in_source_order() {
    let text = "A + POWER(SQRT(B), 2) + LN(A) + POWER(2, 3)";
    let error = compile_numeric(text, "spec", CompileLimits::default()).unwrap_err();
    let CompileError::Unsupported { functions } = error else {
        panic!("unsupported required");
    };
    assert_eq!(
        functions.iter().map(|f| f.function).collect::<Vec<_>>(),
        [
            NumericFunction::Power,
            NumericFunction::Sqrt,
            NumericFunction::Ln,
            NumericFunction::Power
        ]
    );
    assert_eq!(
        functions
            .iter()
            .map(|f| &text[f.name.start..f.name.end])
            .collect::<Vec<_>>(),
        ["POWER", "SQRT", "LN", "POWER"]
    );
    assert!(matches!(
        compile_numeric(
            "SQRT(A)",
            "spec",
            CompileLimits {
                resolutions: 0,
                ..CompileLimits::default()
            }
        ),
        Err(CompileError::Unsupported { .. })
    ));
    assert!(matches!(
        compile_numeric("SQRT(A) + ROUND(B)", "spec", CompileLimits::default()),
        Err(CompileError::Parse(ParseError::Grammar {
            failure: GrammarFailure::ProhibitedFunction { .. },
            ..
        }))
    ));
    // Neither an early known arithmetic failure nor a later literal failure can
    // turn a valid unimplemented expression into an executable partial plan.
    for text in ["1 / 0 + CEIL(A)", "9223372036854775808 + FLOOR(A)"] {
        assert!(matches!(
            compile_numeric(text, "spec", CompileLimits::default()),
            Err(CompileError::Unsupported { .. })
        ));
    }
    assert!(matches!(
        compile_numeric("A + ROUND(B)", "spec", CompileLimits::default()),
        Err(CompileError::Parse(ParseError::Grammar {
            failure: GrammarFailure::ProhibitedFunction { .. },
            ..
        }))
    ));
    for name in ["CEIL", "FLOOR", "TRUNC", "SQRT", "EXP", "LN"] {
        assert!(matches!(
            compile_numeric(&format!("{name}(A)"), "spec", CompileLimits::default()),
            Err(CompileError::Unsupported { .. })
        ));
    }
    for name in [
        "POWER",
        "GREATEST",
        "LEAST",
        "NULLIF",
        "COALESCE",
        "ROUND_HALF_AWAY_FROM_ZERO",
    ] {
        assert!(matches!(
            compile_numeric(&format!("{name}(A, B)"), "spec", CompileLimits::default()),
            Err(CompileError::Unsupported { .. })
        ));
    }
}

/// Static budgets count occurrences, protect deep execution/drop and reset per run.
#[test]
fn compilation_and_reuse_obey_resource_budgets() {
    let mut limits = CompileLimits {
        resolutions: 1,
        ..CompileLimits::default()
    };
    assert_eq!(
        compile_numeric("A + A", "spec", limits),
        Err(CompileError::ResolutionLimit {
            limit: 1,
            required: 2
        })
    );
    assert_eq!(
        compile_numeric("1 / 0 + A + A", "spec", limits),
        Err(CompileError::ResolutionLimit {
            limit: 1,
            required: 2
        })
    );
    limits.resolutions = 2;
    let plan = compile_numeric("A + A", "spec", limits).unwrap();
    assert_eq!(plan.resolution_count(), 2);
    let mut resolver = Resolver::default();
    assert_eq!(plan.evaluate(&mut resolver).unwrap(), Number::Int(3));
    assert_eq!(plan.evaluate(&mut resolver).unwrap(), Number::Int(7));
    assert_eq!(resolver.trace, ["A", "A", "A", "A"]);
    limits.resolutions = 0;
    assert!(compile_numeric("ABS(-3)", "spec", limits).is_ok());
    limits.parse.depth = usize::MAX;
    assert!(matches!(
        compile_numeric(&vec!["1"; 65].join("+"), "spec", limits),
        Err(CompileError::Parse(ParseError::Limit {
            resource: ParseResource::Depth,
            limit: 64,
            ..
        }))
    ));
    for text in [
        vec!["1"; 64].join("+"),
        format!("{}1{}", "ABS(".repeat(63), ")".repeat(63)),
    ] {
        assert!(compile_numeric(&text, "spec", limits)
            .unwrap()
            .evaluate(&mut resolver)
            .is_ok());
    }
}

/// The compiled wrapper does not clone, stringify or reclassify resolver failures.
#[test]
fn opaque_non_clone_resolution_failure_keeps_span_and_payload() {
    #[derive(Debug, PartialEq)]
    struct BoundaryError(Box<u8>);
    struct FailedResolver;
    impl NumericResolver for FailedResolver {
        type Error = BoundaryError;
        /// Move the opaque boundary payload through exactly one failed resolution.
        fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
            assert_eq!(name, "BROKEN");
            Err(BoundaryError(Box::new(7)))
        }
    }
    let text = "ABS((BROKEN)) + A";
    let error = compile(text).evaluate(&mut FailedResolver).unwrap_err();
    assert_eq!(
        &text[error.source_span.start..error.source_span.end],
        "BROKEN"
    );
    assert_eq!(
        error.evaluation.kind,
        EvaluationErrorKind::Resolution {
            identifier: "BROKEN".into(),
            error: BoundaryError(Box::new(7))
        }
    );
}
