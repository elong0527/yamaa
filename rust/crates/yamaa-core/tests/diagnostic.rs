use std::{collections::BTreeSet, convert::Infallible};
use yamaa_core::{
    conversion::convert,
    diagnostic::{ContextValue, CONDITIONS},
    evaluation::{EvaluationErrorKind, NumericResolver},
    numeric_compiler::{compile_numeric, compile_numeric_with_policy, CompileLimits, MathPolicy},
    numeric_parser::SourceSpan,
    value::{ColumnType, Selection, Value},
};

struct Bindings;
#[path = "diagnostic/binding.rs"]
mod binding;
#[path = "diagnostic/csv.rs"]
mod csv;
#[path = "diagnostic/grammar.rs"]
mod grammar;
#[path = "diagnostic/parquet.rs"]
mod parquet;
#[path = "diagnostic/preflight.rs"]
mod preflight;
impl NumericResolver for Bindings {
    type Error = Infallible;
    fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
        Ok(if name == "TEXT" {
            Selection::Present(Value::Str("not numeric".into()))
        } else {
            Selection::Absent
        })
    }
}

/// Every registry entry must be reached by its evaluator, converter or resource cause. Expected
/// vocabulary is independent literal truth, not copied from the registry at runtime.
#[test]
fn every_registered_cause_is_reached_with_its_normative_mapping() {
    let mut reached = BTreeSet::new();
    for (expression, phase, condition, requirement) in [
        ("MISSING", "validation", "unknown_field", "REQ-0443"),
        ("TEXT", "validation", "incompatible_input_type", "REQ-0444"),
        (
            "ROUND_HALF_AWAY_FROM_ZERO(1, 1.0)",
            "validation",
            "incompatible_input_type",
            "REQ-0418",
        ),
        (
            "9223372036854775807 + 1",
            "derivation",
            "integer_overflow",
            "REQ-0434",
        ),
        ("1 / 0", "derivation", "division_by_zero", "REQ-0430"),
        ("SQRT(-1)", "derivation", "sqrt_of_negative", "REQ-0431"),
        ("LN(0)", "derivation", "ln_of_nonpositive", "REQ-0432"),
        ("POWER(-1, 0.5)", "derivation", "invalid_power", "REQ-0433"),
    ] {
        // Existing component-only policy exposes the two math domain failures;
        // this test grants no new production numerical-policy qualification.
        let plan = compile_numeric_with_policy(
            expression,
            "columns.X.compute",
            CompileLimits::default(),
            MathPolicy::PortableLibmV1,
        )
        .unwrap();
        let error = plan.evaluate(&mut Bindings).unwrap_err();
        let diagnostic = error.diagnostic().unwrap();
        let definition = diagnostic.definition();
        assert_eq!(
            (
                definition.phase,
                definition.condition,
                definition.requirement
            ),
            (phase, condition, Some(requirement))
        );
        assert_eq!(diagnostic.spec_paths, ["columns.X.compute"]);
        assert_eq!(
            diagnostic.context["expr"],
            ContextValue::Scalar(Value::Str(expression.into()))
        );
        reached.insert(diagnostic.code);
    }
    for (source, target, requirement) in [
        (Value::Bool(true), ColumnType::Int, "REQ-0013"),
        (Value::float(1.5), ColumnType::Int, "REQ-0021"),
        (
            Value::Str("2026-02-30".into()),
            ColumnType::Date,
            "REQ-0601",
        ),
    ] {
        let error = convert(&source, target).unwrap_err();
        let diagnostic = error.into_diagnostic("columns.X".into());
        let definition = diagnostic.definition();
        assert_eq!(
            (
                definition.phase,
                definition.condition,
                definition.requirement
            ),
            ("convert", "conversion_failed", Some(requirement))
        );
        assert_eq!(diagnostic.context["value"], ContextValue::Scalar(source));
        assert_eq!(diagnostic.spec_paths, ["columns.X"]);
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
        reached.insert(diagnostic.code);
    }
    for (cause, condition) in [
        (
            yamaa_core::resource::ResourceFailure::Missing,
            "resource_path_missing",
        ),
        (
            yamaa_core::resource::ResourceFailure::NotRegularFile,
            "resource_path_not_regular_file",
        ),
    ] {
        let diagnostic = cause.diagnostic("SRC", "input/data.csv");
        let definition = diagnostic.definition();
        assert_eq!(
            (
                definition.phase,
                definition.condition,
                definition.requirement
            ),
            ("validation", condition, Some("REQ-0785"))
        );
        assert_eq!(diagnostic.spec_paths, ["input.SRC.path"]);
        assert_eq!(
            diagnostic.context["dataset"],
            ContextValue::Scalar(Value::Str("SRC".into()))
        );
        assert_eq!(
            diagnostic.context["path"],
            ContextValue::Scalar(Value::Str("input/data.csv".into()))
        );
        assert_eq!(diagnostic.context.len(), 2);
        reached.insert(diagnostic.code);
    }
    reached.extend(preflight::reached());
    reached.extend(preflight::output_reached());
    reached.extend(grammar::reached());
    reached.extend(grammar::predicate_reached());
    reached.extend(binding::reached());
    reached.extend(binding::window_reached());
    reached.extend(binding::lookup_reached());
    reached.extend(binding::source_filter_reached());
    reached.extend(binding::verification_reached());
    reached.extend(binding::verification_declarations_reached());
    let result = yamaa_core::dataset_checks::not_missing(&[
        Value::Int(0),
        Value::Missing,
        Value::Str(String::new()),
        Value::Missing,
    ]);
    assert_eq!(result.offending_rows, [1, 3]);
    let diagnostic = result
        .diagnostic("columns.V.verifications[0].not_missing".into(), "V".into())
        .unwrap();
    assert_eq!(
        (
            diagnostic.definition().phase,
            diagnostic.definition().condition,
            diagnostic.definition().requirement
        ),
        ("verification", "not_missing_failed", Some("REQ-0375"))
    );
    assert_eq!(
        diagnostic.spec_paths,
        ["columns.V.verifications[0].not_missing"]
    );
    assert_eq!(
        diagnostic.context["column"],
        ContextValue::Scalar(Value::Str("V".into()))
    );
    assert_eq!(
        diagnostic.context["failure_count"],
        ContextValue::Integer("2".into())
    );
    assert!(
        yamaa_core::dataset_checks::not_missing(&[Value::Int(0), Value::Str(String::new())])
            .diagnostic("unused".into(), "V".into())
            .is_none()
    );
    reached.insert(diagnostic.code);
    for (check, values, offending, condition, requirement) in [
        (
            yamaa_core::dataset::Check::AllowedValues(vec![Value::Int(1)]),
            vec![Value::Int(1), Value::Missing, Value::Int(2)],
            vec![2],
            "allowed_values_failed",
            "REQ-0376",
        ),
        (
            yamaa_core::dataset::Check::Range {
                min: Some(Value::float(9223372036854775808.0)),
                max: None,
            },
            vec![Value::Int(i64::MAX), Value::Missing],
            vec![0],
            "range_failed",
            "REQ-0377",
        ),
        (
            yamaa_core::dataset::Check::MaxLength(2),
            vec![
                Value::Str("é🙂".into()),
                Value::Str("é🙂x".into()),
                Value::Missing,
            ],
            vec![1],
            "length_failed",
            "REQ-0378",
        ),
    ] {
        let rows = yamaa_core::dataset_checks::column_offenders(&check, &values);
        assert_eq!(rows, offending);
        let diagnostic = yamaa_core::dataset_checks::column_diagnostic(
            &check,
            "columns.V.verifications[0]".into(),
            "V".into(),
            rows.len(),
        )
        .unwrap();
        assert_eq!(
            (
                diagnostic.definition().phase,
                diagnostic.definition().condition,
                diagnostic.definition().requirement
            ),
            ("verification", condition, Some(requirement))
        );
        assert_eq!(diagnostic.spec_paths, ["columns.V.verifications[0]"]);
        assert_eq!(
            diagnostic.context["column"],
            ContextValue::Scalar(Value::Str("V".into()))
        );
        assert_eq!(
            diagnostic.context["failure_count"],
            ContextValue::Integer("1".into())
        );
        if condition == "length_failed" {
            assert_eq!(diagnostic.context["max"], ContextValue::Integer("2".into()));
        }
        assert!(yamaa_core::dataset_checks::column_diagnostic(
            &check,
            "unused".into(),
            "V".into(),
            0
        )
        .is_none());
        reached.insert(diagnostic.code);
    }
    let pattern = yamaa_core::regex::Pattern::compile("b", Default::default()).unwrap();
    // A caller explicitly grants matching resources, independently of ordinary visits.
    let mut budget = yamaa_core::predicate::Budget::new(yamaa_core::predicate::Usage {
        regex: yamaa_core::regex::MatchUsage {
            subject_bytes: 100,
            work: 1000,
            state_cells: 1000,
        },
        ..Default::default()
    });
    let rows = yamaa_core::dataset_checks::matches_offenders(
        &pattern,
        &[
            Value::Str("abc".into()),
            Value::Missing,
            Value::Str("xxx".into()),
        ],
        Default::default(),
        &mut budget,
    )
    .unwrap();
    assert_eq!(rows, [2]);
    let diagnostic = yamaa_core::dataset_checks::column_diagnostic(
        &yamaa_core::dataset::Check::Matches(pattern),
        "columns.V.verifications[0].matches".into(),
        "V".into(),
        rows.len(),
    )
    .unwrap();
    assert_eq!(
        (
            diagnostic.definition().phase,
            diagnostic.definition().condition,
            diagnostic.definition().requirement
        ),
        ("verification", "matches_failed", Some("REQ-0379"))
    );
    assert_eq!(diagnostic.context.len(), 2);
    assert_eq!(
        diagnostic.context["failure_count"],
        ContextValue::Integer("1".into())
    );
    reached.insert(diagnostic.code);
    assert!(matches!(
        yamaa_core::regex::Pattern::compile("(", Default::default()),
        Err(yamaa_core::regex::CompileError::Invalid { .. })
    ));
    let diagnostic = yamaa_core::dataset_checks::invalid_pattern_diagnostic(
        "columns.V.verifications[0].matches.pattern".into(),
        "(".into(),
    );
    assert_eq!(
        (
            diagnostic.definition().phase,
            diagnostic.definition().condition,
            diagnostic.definition().requirement
        ),
        ("validation", "invalid_regex", Some("REQ-0827"))
    );
    assert_eq!(diagnostic.context.len(), 1);
    assert_eq!(
        diagnostic.context["pattern"],
        ContextValue::Scalar(Value::Str("(".into()))
    );
    assert_eq!(diagnostic.source_span, None);
    reached.insert(diagnostic.code);
    reached.extend(csv::reached());
    reached.extend(csv::typing_reached());
    reached.extend(parquet::reached());
    assert_eq!(reached, CONDITIONS.iter().copied().collect());
    assert_eq!(
        CONDITIONS.len(),
        reached.len(),
        "registry causes are unique"
    );
}

#[test]
fn column_pattern_searches_share_a_nonrefundable_subject_budget() {
    use yamaa_core::{dataset_checks::matches_offenders, predicate, regex};
    let pattern = regex::Pattern::compile("z", Default::default()).unwrap();
    let usage = predicate::Usage {
        regex: regex::MatchUsage {
            subject_bytes: 4,
            work: 1000,
            state_cells: 1000,
        },
        ..Default::default()
    };
    let mut budget = predicate::Budget::new(usage);
    let values = [Value::Str("é".into()), Value::Missing];
    for _ in 0..2 {
        assert_eq!(
            matches_offenders(&pattern, &values, Default::default(), &mut budget).unwrap(),
            [0]
        );
    }
    let error = matches_offenders(&pattern, &values, Default::default(), &mut budget).unwrap_err();
    assert_eq!(error.resource, predicate::Resource::RegexSubjectBytes);
    assert_eq!(error.limit, 4);
    assert_eq!(budget.used().regex.subject_bytes, 4);
    assert!(matches_offenders(&pattern, &values, Default::default(), &mut budget).is_err());
    assert_eq!(
        matches_offenders(
            &pattern,
            &values,
            Default::default(),
            &mut predicate::Budget::new(usage)
        )
        .unwrap(),
        [0]
    );
}

/// A later nested error keeps its own span/route and full integer diagnostic,
/// independently of the surrounding successful operand and parentheses.
#[test]
fn nested_failure_retains_location_and_full_width_context() {
    let expression = "1 + (9223372036854775807 + 1)";
    let error = compile_numeric(expression, "columns.X.compute", CompileLimits::default())
        .unwrap()
        .evaluate(&mut Bindings)
        .unwrap_err();
    let diagnostic = error.diagnostic().unwrap();
    assert_eq!(
        diagnostic.source_span,
        Some(SourceSpan { start: 5, end: 28 })
    );
    assert_eq!(diagnostic.operand_route, Some(vec!["right".into()]));
    assert_eq!(
        diagnostic.context["value"],
        ContextValue::Scalar(Value::Str("9223372036854775808".into()))
    );
    assert_eq!(
        diagnostic.context["minimum"],
        ContextValue::Scalar(Value::Int(i64::MIN))
    );
    assert_eq!(
        diagnostic.context["maximum"],
        ContextValue::Scalar(Value::Int(i64::MAX))
    );
    let literal = compile_numeric(
        "9223372036854775808",
        "columns.Y.compute",
        CompileLimits::default(),
    )
    .unwrap()
    .evaluate(&mut Bindings)
    .unwrap_err()
    .diagnostic()
    .unwrap();
    assert_eq!(literal.code, diagnostic.code);
    assert_eq!(literal.context["value"], diagnostic.context["value"]);
}

#[test]
fn conversion_retains_unbounded_diagnostic_integer_as_decimal_text() {
    let digits = "9".repeat(5000);
    let diagnostic = convert(&Value::Str(digits.clone()), ColumnType::Int)
        .unwrap_err()
        .into_diagnostic("columns.X.unconvertible".into());
    assert_eq!(diagnostic.definition().requirement, Some("REQ-0021"));
    assert_eq!(
        diagnostic.context["from"],
        ContextValue::Scalar(Value::Str("int".into()))
    );
    assert_eq!(
        diagnostic.context["to"],
        ContextValue::Scalar(Value::Str("int".into()))
    );
    assert_eq!(diagnostic.context["value"], ContextValue::Integer(digits));
}

/// The diagnostic projection cannot consume or relabel a host error, and has no
/// Clone requirement on it. The next operand remains unevaluated.
#[test]
fn opaque_resolver_failure_is_retained_in_place() {
    #[derive(Debug)]
    struct Failure(Box<u8>);
    struct Resolver {
        failure: Option<Failure>,
        calls: Vec<String>,
    }
    impl NumericResolver for Resolver {
        type Error = Failure;
        fn resolve(&mut self, name: &str) -> Result<Selection, Failure> {
            self.calls.push(name.into());
            Err(self.failure.take().expect("only one call"))
        }
    }
    let failure = Failure(Box::new(9));
    let address = &*failure.0 as *const u8;
    let mut resolver = Resolver {
        failure: Some(failure),
        calls: vec![],
    };
    let error = compile_numeric("BAD + LATER", "columns.X", CompileLimits::default())
        .unwrap()
        .evaluate(&mut resolver)
        .unwrap_err();
    assert!(error.diagnostic().is_none());
    assert_eq!(resolver.calls, ["BAD"]);
    let EvaluationErrorKind::Resolution {
        identifier,
        error: Failure(value),
    } = error.evaluation.kind
    else {
        panic!("opaque failure replaced")
    };
    assert_eq!(identifier, "BAD");
    assert_eq!(&*value as *const u8, address);
}
