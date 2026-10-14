use std::{collections::BTreeMap, convert::Infallible};
use yamaa_core::conversion::{ConversionError, ConversionValue};
use yamaa_core::evaluation::{EvaluationErrorKind, NumericCondition, NumericResolver};
use yamaa_core::numeric::ArithmeticErrorKind;
use yamaa_core::numeric_compiler::{compile_numeric, CompileLimits};
use yamaa_core::value::{ColumnType, Selection, Value, ValueType};
use yamaa_engine::numeric_lifecycle::{
    HandlerCounter, HandlerKind, LiteralHandler, NumericDerivation, NumericLifecycleError,
};

const COLUMN: &str = "columns.A";
const EXPRESSION: &str = "columns.A.derivation.value.compute";
const HANDLER: &str = "columns.A.derivation.unconvertible";

/// Decode explicit scalar fixture inputs without using the declared output type.
fn value(token: &str) -> Value {
    if token == "missing" {
        return Value::Missing;
    }
    let (kind, text) = token.split_once(':').unwrap();
    match kind {
        "int" => Value::Int(text.parse().unwrap()),
        "float" => Value::float(text.parse().unwrap()),
        "bool" => Value::Bool(text.parse().unwrap()),
        "str" => Value::Str(text.into()),
        _ => panic!("unknown fixture value"),
    }
}

/// Encode types and exact binary64 bits, keeping zero signs observable.
fn encoded(value: &Value) -> String {
    match value {
        Value::Missing => "missing".into(),
        Value::Int(number) => format!("int:{number}"),
        Value::Float(number) => format!("float:{:016x}", number.get().to_bits()),
        Value::Bool(boolean) => format!("bool:{boolean}"),
        Value::Str(text) => format!("str:{text}"),
        Value::Date(date) => format!(
            "date:{date}:{}",
            format!("{:?}", date.collected_precision()).to_lowercase()
        ),
        Value::DateTime(date) => format!(
            "datetime:{date}:{}",
            format!("{:?}", date.collected_precision()).to_lowercase()
        ),
    }
}

/// Decode the closed destination vocabulary rather than infer it from a result.
fn column_type(text: &str) -> ColumnType {
    match text {
        "int" => ColumnType::Int,
        "float" => ColumnType::Float,
        "str" => ColumnType::Str,
        "date" => ColumnType::Date,
        "datetime" => ColumnType::DateTime,
        _ => panic!("invalid fixture target"),
    }
}

/// Keep overflow decimal diagnostic data distinct from ordinary runtime integers.
fn conversion_value(error: &ConversionError, target: ColumnType) -> String {
    assert_eq!(error.target, target);
    assert_eq!(error.applicable_handler(), "unconvertible");
    match &error.value {
        ConversionValue::Runtime(value) => {
            assert_eq!(error.source_type(), value.value_type());
            encoded(value)
        }
        ConversionValue::Integer(text) => {
            assert_eq!(error.source_type(), Some(ValueType::Int));
            format!("integer:{text}")
        }
    }
}

/// Fake input port that observes every resolution and keeps absence distinct.
struct Resolver {
    values: BTreeMap<String, Value>,
    trace: Vec<String>,
}
impl NumericResolver for Resolver {
    type Error = Infallible;
    /// Record written evaluation order before returning the selected source value.
    fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
        self.trace.push(name.into());
        Ok(self
            .values
            .get(name)
            .cloned()
            .map_or(Selection::Absent, Selection::Present))
    }
}

/// Build the same application declaration used by the independent Python runner.
fn derivation(expression: &str, target: ColumnType, handler: Option<Value>) -> NumericDerivation {
    NumericDerivation::new(
        compile_numeric(expression, EXPRESSION, CompileLimits::default()).unwrap(),
        target,
        COLUMN,
        handler.map(|value| LiteralHandler {
            spec_path: HANDLER.into(),
            value,
        }),
    )
}

/// Replay results, fatal contexts, paths, traces and handler counts from independent truth.
#[test]
fn shared_numeric_lifecycle_vectors() {
    for row in include_str!("fixtures/numeric_lifecycle.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 11);
        let target = column_type(fields[3]);
        let plan = derivation(
            fields[1],
            target,
            (fields[4] != "-").then(|| value(fields[4])),
        );
        let mut counter = HandlerCounter::default();
        plan.register_handlers(&mut counter);
        let mut resolver = Resolver {
            values: if fields[2] == "-" {
                BTreeMap::new()
            } else {
                fields[2]
                    .split(';')
                    .map(|binding| {
                        let (name, token) = binding.split_once('=').unwrap();
                        (name.into(), value(token))
                    })
                    .collect()
            },
            trace: Vec::new(),
        };
        let mut path = "-".to_string();
        let mut context = "-".to_string();
        let mut original = "-".to_string();
        let actual = match plan.evaluate(&mut resolver, &mut counter) {
            Ok(result) => encoded(&result),
            Err(failure) => match *failure {
                NumericLifecycleError::Evaluation(error) => {
                    assert!(fields[1]
                        .get(error.source_span.start..error.source_span.end)
                        .is_some());
                    assert_eq!(error.evaluation.location.expression, fields[1]);
                    path = error.evaluation.location.spec_path;
                    let EvaluationErrorKind::Numeric(condition) = error.evaluation.kind else {
                        panic!("infallible resolver");
                    };
                    context = match &condition {
                        NumericCondition::UnknownField { identifier } => {
                            format!("identifier:{identifier}")
                        }
                        NumericCondition::IncompatibleInput { identifier, actual } => {
                            assert_eq!(*actual, ValueType::Str);
                            format!("source:{identifier}:str")
                        }
                        NumericCondition::Arithmetic(ArithmeticErrorKind::IntegerOverflow {
                            value,
                        }) => format!("integer:{value}"),
                        NumericCondition::Arithmetic(
                            ArithmeticErrorKind::InvalidRoundingDigits,
                        ) => "digits:float".into(),
                        _ => "-".into(),
                    };
                    format!(
                        "error:{}:{}:{}",
                        condition.phase(),
                        condition.condition(),
                        condition.requirement()
                    )
                }
                NumericLifecycleError::Conversion { spec_path, error } => {
                    path = spec_path;
                    context = conversion_value(&error, target);
                    format!(
                        "error:{}:{}:{}",
                        error.phase(),
                        error.condition(),
                        error.requirement()
                    )
                }
                NumericLifecycleError::HandlerConversion {
                    spec_path,
                    column_path,
                    original: failure,
                    replacement,
                } => {
                    assert_eq!(column_path, COLUMN);
                    path = spec_path;
                    original = conversion_value(&failure, target);
                    context = conversion_value(&replacement, target);
                    format!(
                        "error:{}:{}:{}",
                        replacement.phase(),
                        replacement.condition(),
                        replacement.requirement()
                    )
                }
                NumericLifecycleError::Accounting { .. } => {
                    panic!("tiny fixture cannot overflow counters")
                }
            },
        };
        assert_eq!(actual, fields[5], "{}", fields[0]);
        assert_eq!(path, fields[6], "{}", fields[0]);
        assert_eq!(
            resolver.trace.join(","),
            fields[7].trim_matches('-'),
            "{}",
            fields[0]
        );
        assert_eq!(context, fields[9], "{}", fields[0]);
        assert_eq!(original, fields[10], "{}", fields[0]);
        if fields[8] == "-" {
            assert!(counter.snapshot().is_empty());
        } else {
            let [entry] = counter.snapshot() else {
                panic!("one declared handler")
            };
            assert_eq!(entry.spec_path, HANDLER);
            assert_eq!(entry.handler.name(), "unconvertible");
            assert_eq!(entry.count.to_string(), fields[8], "{}", fields[0]);
        }
    }
}

/// Count replacements exactly once on every evaluation; retain declaration order and zeros.
#[test]
fn accounting_retains_zero_declarations_and_prior_firings_on_failure() {
    let good = derivation("A", ColumnType::Int, Some(Value::Int(7)));
    let bad = NumericDerivation::new(
        compile_numeric("A", EXPRESSION, CompileLimits::default()).unwrap(),
        ColumnType::Int,
        "columns.B",
        Some(LiteralHandler {
            spec_path: "columns.B.derivation.unconvertible".into(),
            value: Value::Str("bad".into()),
        }),
    );
    let mut counter = HandlerCounter::default();
    bad.register_handlers(&mut counter);
    good.register_handlers(&mut counter);
    let mut resolver = Resolver {
        values: [("A".into(), Value::float(1.25))].into(),
        trace: Vec::new(),
    };
    for _ in 0..2 {
        assert_eq!(
            good.evaluate(&mut resolver, &mut counter),
            Ok(Value::Int(7))
        );
    }
    assert_eq!(
        counter
            .snapshot()
            .iter()
            .map(|c| c.count)
            .collect::<Vec<_>>(),
        [0, 2]
    );
    assert!(matches!(
        *bad.evaluate(&mut resolver, &mut counter).unwrap_err(),
        NumericLifecycleError::HandlerConversion { .. }
    ));
    good.register_handlers(&mut counter);
    assert_eq!(
        counter
            .snapshot()
            .iter()
            .map(|c| (c.spec_path.as_str(), c.count))
            .collect::<Vec<_>>(),
        [("columns.B.derivation.unconvertible", 1), (HANDLER, 2)]
    );
    assert_eq!(resolver.trace, ["A", "A", "A"]);
    assert_eq!(counter.snapshot()[0].handler, HandlerKind::Unconvertible);
}

/// A caller-owned failure has no Clone requirement and cannot be caught by conversion handling.
#[test]
fn opaque_resolver_failure_bypasses_handler_and_keeps_exact_provenance() {
    #[derive(Debug)]
    struct Opaque(u32);
    struct Failed(Vec<String>);
    impl NumericResolver for Failed {
        type Error = Opaque;
        /// Fail on the reached identifier, proving no subsequent resolver is invoked.
        fn resolve(&mut self, name: &str) -> Result<Selection, Opaque> {
            self.0.push(name.into());
            Err(Opaque(42))
        }
    }
    let plan = derivation(
        "NULL + BROKEN + LATER",
        ColumnType::Int,
        Some(Value::Int(7)),
    );
    let mut resolver = Failed(Vec::new());
    let mut counter = HandlerCounter::default();
    let failure = plan.evaluate(&mut resolver, &mut counter).unwrap_err();
    let NumericLifecycleError::Evaluation(error) = *failure else {
        panic!("evaluation failure")
    };
    assert_eq!(
        &"NULL + BROKEN + LATER"[error.source_span.start..error.source_span.end],
        "BROKEN"
    );
    let EvaluationErrorKind::Resolution {
        identifier,
        error: Opaque(payload),
    } = error.evaluation.kind
    else {
        panic!("opaque resolution failure")
    };
    assert_eq!((identifier.as_str(), payload), ("BROKEN", 42));
    assert_eq!(resolver.0, ["BROKEN"]);
    assert_eq!(counter.snapshot()[0].count, 0);
}

/// A caller publishes only the completed replacement to a dependent numeric plan.
#[test]
fn dependent_reads_converted_handler_value() {
    let first = derivation("RAW", ColumnType::Int, Some(Value::Str("7".into())));
    let next = NumericDerivation::new(
        compile_numeric(
            "A + 1",
            "columns.B.derivation.value.compute",
            CompileLimits::default(),
        )
        .unwrap(),
        ColumnType::Int,
        "columns.B",
        None,
    );
    let mut counter = HandlerCounter::default();
    let mut resolver = Resolver {
        values: [("RAW".into(), Value::float(1.25))].into(),
        trace: Vec::new(),
    };
    let completed = first.evaluate(&mut resolver, &mut counter).unwrap();
    assert_eq!(completed, Value::Int(7));
    resolver.values.insert("A".into(), completed);
    assert_eq!(
        next.evaluate(&mut resolver, &mut counter),
        Ok(Value::Int(8))
    );
    assert_eq!(resolver.trace, ["RAW", "A"]);
    assert_eq!(counter.snapshot()[0].count, 1);
}
