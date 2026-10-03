use std::collections::BTreeMap;
use std::convert::Infallible;

use yamaa_core::evaluation::{
    EvaluationErrorKind, NumericCondition, NumericNode, NumericPlan, NumericResolver, Operand,
};
use yamaa_core::numeric::{ArithmeticErrorKind, BinaryOperator as B, Number, UnaryOperator as U};
use yamaa_core::numeric_compiler::{compile_numeric, CompileLimits};
use yamaa_core::value::{Selection, Value, ValueType};

const SPEC_PATH: &str = "columns.A.derivation.compute";

/// Decode fixture values without coercion or inference from the destination type.
fn value(token: &str) -> Value {
    if token == "missing" {
        return Value::Missing;
    }
    let (kind, text) = token.split_once(':').unwrap();
    match kind {
        "int" => Value::Int(text.parse().unwrap()),
        "float" => Value::float(text.parse().unwrap()),
        "str" => Value::Str(text.into()),
        "bool" => Value::Bool(text.parse().unwrap()),
        _ => panic!("bad fixture value {token}"),
    }
}

/// Convert test-only postfix notation into typed nodes, not the language grammar.
fn tree(postfix: &str) -> NumericNode {
    let mut stack = Vec::new();
    for token in postfix.split_whitespace() {
        let node = if let Some(name) = token.strip_prefix('$') {
            NumericNode::Identifier(name.into())
        } else if let Some(operator) = match token {
            "plus" => Some(U::Plus),
            "negate" => Some(U::Negate),
            "abs" => Some(U::Abs),
            _ => None,
        } {
            NumericNode::Unary {
                operator,
                operand: Box::new(stack.pop().unwrap()),
            }
        } else if let Some(operator) = match token {
            "add" => Some(B::Add),
            "subtract" => Some(B::Subtract),
            "multiply" => Some(B::Multiply),
            "divide" => Some(B::Divide),
            "modulo" => Some(B::Modulo),
            _ => None,
        } {
            let right = Box::new(stack.pop().unwrap());
            let left = Box::new(stack.pop().unwrap());
            NumericNode::Binary {
                operator,
                left,
                right,
            }
        } else {
            NumericNode::Literal(Number::try_from(&value(token)).unwrap())
        };
        stack.push(node);
    }
    assert_eq!(stack.len(), 1, "malformed test tree {postfix}");
    stack.pop().unwrap()
}

/// Associate a caller-supplied tree with the original expression's provenance.
fn plan(expression: &str, postfix: &str) -> NumericPlan {
    NumericPlan {
        spec_path: SPEC_PATH.into(),
        expression: expression.into(),
        root: tree(postfix),
    }
}

/// A fake port that records every occurrence, including absent identifiers.
struct RecordingResolver {
    values: BTreeMap<String, Value>,
    trace: Vec<String>,
}

impl RecordingResolver {
    /// Decode fixture bindings without coupling their order to evaluation order.
    fn new(bindings: &str) -> Self {
        Self {
            values: if bindings == "-" {
                BTreeMap::new()
            } else {
                bindings
                    .split(';')
                    .map(|entry| {
                        let (name, token) = entry.split_once('=').unwrap();
                        (name.into(), value(token))
                    })
                    .collect()
            },
            trace: Vec::new(),
        }
    }
}

impl NumericResolver for RecordingResolver {
    type Error = Infallible;

    /// Distinguish lookup absence from an explicitly present missing value.
    fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
        self.trace.push(name.into());
        Ok(self
            .values
            .get(name)
            .cloned()
            .map_or(Selection::Absent, Selection::Present))
    }
}

/// Encode exact numeric types and bits, including signed zero and missingness.
fn encoded(number: Number) -> String {
    match number {
        Number::Missing => "missing".into(),
        Number::Int(value) => format!("int:{value}"),
        Number::Float(value) => format!("float:{:016x}", value.get().to_bits()),
    }
}

/// Check portable condition metadata and encode exact failure-specific context.
fn encoded_condition(condition: &NumericCondition) -> String {
    let mut result = format!(
        "error:{}:{}",
        condition.condition(),
        condition.requirement()
    );
    match condition {
        NumericCondition::UnknownField { identifier } => {
            assert_eq!(condition.phase(), "validation");
            result.push_str(&format!(":{identifier}"));
        }
        NumericCondition::IncompatibleInput { identifier, actual } => {
            assert_eq!(condition.phase(), "validation");
            let actual = match actual {
                ValueType::Str => "str",
                ValueType::Bool => "bool",
                _ => panic!("unexpected fixture type"),
            };
            result.push_str(&format!(":{identifier}:{actual}"));
        }
        NumericCondition::LiteralOverflow { value } => {
            assert_eq!(condition.phase(), "derivation");
            result.push_str(&format!(":{value}"));
        }
        NumericCondition::Arithmetic(kind) => {
            assert_eq!(condition.phase(), "derivation");
            if let ArithmeticErrorKind::IntegerOverflow { value } = kind {
                result.push_str(&format!(":{value}"));
            }
        }
    }
    result
}

/// Replay the same fixtures through either caller-supplied IR or actual source compilation.
fn fixture_evaluate(
    expression: &str,
    postfix: &str,
    compiled: bool,
    resolver: &mut RecordingResolver,
) -> Result<Number, Box<yamaa_core::evaluation::EvaluationError<Infallible>>> {
    if compiled {
        compile_numeric(expression, SPEC_PATH, CompileLimits::default())
            .unwrap()
            .evaluate(resolver)
            .map_err(|failure| {
                assert!(failure.source_span.start < failure.source_span.end);
                assert!(expression
                    .get(failure.source_span.start..failure.source_span.end)
                    .is_some());
                failure.evaluation
            })
    } else {
        plan(expression, postfix)
            .evaluate(resolver)
            .map_err(Box::new)
    }
}

/// Replay independent Python/Rust results, call traces and structural failure paths.
#[test]
fn shared_evaluation_vectors() {
    for compiled in [false, true] {
        for row in include_str!("fixtures/evaluation.tsv").lines().skip(1) {
            let fields: Vec<_> = row.split('\t').collect();
            assert_eq!(fields.len(), 7);
            let mut resolver = RecordingResolver::new(fields[3]);
            let mut route = Vec::new();
            let actual = match fixture_evaluate(fields[1], fields[2], compiled, &mut resolver) {
                Ok(number) => encoded(number),
                Err(error) => {
                    let error = *error;
                    assert_eq!(error.location.spec_path, SPEC_PATH);
                    assert_eq!(error.location.expression, fields[1]);
                    route = error.location.operands;
                    let EvaluationErrorKind::Numeric(condition) = error.kind;
                    encoded_condition(&condition)
                }
            };
            assert_eq!(actual, fields[4], "{}", fields[0]);
            assert_eq!(
                resolver.trace.join(","),
                fields[5].trim_matches('-'),
                "{}",
                fields[0]
            );
            let route: Vec<_> = route
                .iter()
                .map(|position| match position {
                    Operand::Unary => "unary",
                    Operand::Left => "left",
                    Operand::Right => "right",
                })
                .collect();
            assert_eq!(
                route.join(","),
                fields[6].trim_matches('-'),
                "{}",
                fields[0]
            );
        }
    }
}

/// Exercise the evaluator against every existing independently specified primitive case.
#[test]
fn shared_arithmetic_vectors_through_typed_evaluation() {
    for compiled in [false, true] {
        for row in include_str!("fixtures/arithmetic.tsv").lines().skip(1) {
            let fields: Vec<_> = row.split('\t').collect();
            let mut resolver = RecordingResolver::new(&format!("L={};R={}", fields[3], fields[4]));
            let unary = matches!(fields[2], "plus" | "negate" | "abs");
            let postfix = format!("{} {}", if unary { "$L" } else { "$L $R" }, fields[2]);
            let actual = match fixture_evaluate(fields[1], &postfix, compiled, &mut resolver) {
                Ok(number) => encoded(number),
                Err(error) => {
                    let error = *error;
                    assert_eq!(error.location.expression, fields[1]);
                    assert_eq!(error.location.spec_path, SPEC_PATH);
                    assert!(error.location.operands.is_empty());
                    let EvaluationErrorKind::Numeric(condition) = error.kind;
                    encoded_condition(&condition)
                }
            };
            assert_eq!(actual, fields[5], "{}", fields[0]);
            assert_eq!(resolver.trace.join(","), if unary { "L" } else { "L,R" });
        }
    }
}

/// Cover every nonnumeric runtime type without silently converting a numeric string.
#[test]
fn identifiers_reject_every_nonnumeric_type() {
    for value in [
        Value::Str("42".into()),
        Value::Bool(true),
        Value::Date("2025-01-02".parse().unwrap()),
        Value::DateTime("2025-01-02T03:04".parse().unwrap()),
    ] {
        let actual = value.value_type().unwrap();
        let mut resolver = RecordingResolver {
            values: [("A".into(), value)].into(),
            trace: Vec::new(),
        };
        let failure = plan("+A", "$A plus").evaluate(&mut resolver).unwrap_err();
        assert_eq!(failure.location.operands, [Operand::Unary]);
        assert_eq!(
            failure.kind,
            EvaluationErrorKind::Numeric(NumericCondition::IncompatibleInput {
                identifier: "A".into(),
                actual,
            })
        );
    }
}

/// Preserve opaque, non-Clone resolution errors with their own diagnostic payload.
#[test]
fn resolver_failure_is_retained_and_stops_following_operands() {
    #[derive(Debug, PartialEq)]
    struct Failure {
        condition: &'static str,
        context: Vec<&'static str>,
    }
    struct Resolver {
        error: Option<Failure>,
        trace: Vec<String>,
    }
    impl NumericResolver for Resolver {
        type Error = Failure;

        /// Return missing first, then move the original boundary failure exactly once.
        fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
            self.trace.push(name.into());
            match name {
                "L" => Ok(Selection::Present(Value::Missing)),
                "A" => Err(self.error.take().unwrap()),
                _ => panic!("later operand must not be resolved"),
            }
        }
    }
    let mut resolver = Resolver {
        error: Some(Failure {
            condition: "multiple_matches",
            context: vec!["donor-1", "donor-2"],
        }),
        trace: Vec::new(),
    };
    let failure = plan("L + (A * B)", "$L $A $B multiply add")
        .evaluate(&mut resolver)
        .unwrap_err();
    assert_eq!(resolver.trace, ["L", "A"]);
    assert_eq!(failure.location.spec_path, SPEC_PATH);
    assert_eq!(failure.location.expression, "L + (A * B)");
    assert_eq!(failure.location.operands, [Operand::Right, Operand::Left]);
    assert_eq!(
        failure.kind,
        EvaluationErrorKind::Resolution {
            identifier: "A".into(),
            error: Failure {
                condition: "multiple_matches",
                context: vec!["donor-1", "donor-2"],
            },
        }
    );
}

/// Each occurrence and each plan execution reaches its resolver, without memoization.
#[test]
fn repeated_resolution_and_plan_reuse_do_not_cache_values() {
    struct CountingResolver(i64);
    impl NumericResolver for CountingResolver {
        type Error = Infallible;

        /// Return a distinct number for each observed occurrence.
        fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
            assert_eq!(name, "A");
            self.0 += 1;
            Ok(Selection::Present(Value::Int(self.0)))
        }
    }
    let plan = plan("A + A", "$A $A add");
    let mut resolver = CountingResolver(0);
    assert_eq!(plan.evaluate(&mut resolver), Ok(Number::Int(3)));
    assert_eq!(plan.evaluate(&mut resolver), Ok(Number::Int(7)));
    assert_eq!(resolver.0, 4);
}
