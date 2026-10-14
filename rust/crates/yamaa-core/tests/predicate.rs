use std::collections::{BTreeMap, VecDeque};
use std::convert::Infallible;

use yamaa_core::predicate::{
    Comparison as C, Condition, ErrorKind, Limits, Node, Plan, PlanError, Resolver, Resource,
    Route, Scalar, Truth,
};
use yamaa_core::temporal::{Date, DatePrecision, DateTime, DateTimePrecision};
use yamaa_core::value::{Selection, Value, ValueType};

/// The fixture port records every occurrence and can vary repeated reads.
#[derive(Default)]
struct Port {
    values: BTreeMap<String, VecDeque<Value>>,
    trace: Vec<String>,
}

impl Port {
    /// Bind a stable value unless a test supplies an explicit sequence later.
    fn with(values: &[(&str, Value)]) -> Self {
        Self {
            values: values
                .iter()
                .map(|(name, value)| ((*name).into(), [value.clone()].into()))
                .collect(),
            trace: Vec::new(),
        }
    }
}

impl Resolver for Port {
    type Error = Infallible;
    /// Retain the last supplied value while consuming preceding sequence entries.
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error> {
        self.trace.push(identifier.into());
        Ok(match self.values.get_mut(identifier) {
            None => Selection::Absent,
            Some(values) => Selection::Present(if values.len() > 1 {
                values.pop_front().unwrap()
            } else {
                values[0].clone()
            }),
        })
    }
}

/// Keep authored source context observable in failures.
fn plan(nodes: Vec<Node>, limits: Limits) -> Plan {
    let root = nodes.len() - 1;
    Plan::new(
        nodes,
        root,
        "rows[0].filter".into(),
        "original predicate".into(),
        limits,
    )
    .unwrap()
}

/// Use identifiers to exercise deferred resolution rather than constant folding.
fn id(name: &str) -> Scalar {
    Scalar::Identifier(name.into())
}

/// Embed a normalized operand without host coercion.
fn lit(value: Value) -> Scalar {
    Scalar::Literal(value)
}

/// Produce a one-node comparison over the fixture's two names.
fn comparison(operator: C) -> Node {
    Node::Compare {
        operator,
        left: id("a"),
        right: id("b"),
    }
}

#[test]
fn complete_three_valued_tables_and_null_tests() {
    use Truth::{False as F, True as T, Unknown as U};
    let values = [T, F, U];
    let and = [[T, F, U], [F, F, F], [U, F, U]];
    let or = [[T, T, T], [T, F, U], [T, U, U]];
    for (i, left) in values.iter().enumerate() {
        assert_eq!(left.negate(), [F, T, U][i]);
        for (j, right) in values.iter().enumerate() {
            assert_eq!(left.and(*right), and[i][j]);
            assert_eq!(left.or(*right), or[i][j]);
        }
    }
    for (value, expected) in [
        (Value::Missing, T),
        (Value::Bool(false), F),
        (Value::Int(0), F),
    ] {
        for negated in [false, true] {
            let p = plan(
                vec![Node::IsNull {
                    value: lit(value.clone()),
                    negated,
                }],
                Limits::default(),
            );
            assert_eq!(
                p.evaluate(&mut Port::default()).unwrap(),
                if negated { expected.negate() } else { expected }
            );
        }
    }
}

#[test]
fn comparison_promotion_order_missing_and_temporal_precision() {
    use Truth::{False as F, True as T};
    let date = Date::new(2025, 1, 1, DatePrecision::Year).unwrap();
    let day = Date::new(2025, 1, 1, DatePrecision::Day).unwrap();
    let datetime = DateTime::new(day, 0, 0, 0, DateTimePrecision::Day).unwrap();
    let second = DateTime::new(day, 0, 0, 0, DateTimePrecision::Second).unwrap();
    let pairs = [
        (
            Value::Int(9_007_199_254_740_993),
            Value::float(9_007_199_254_740_992.0),
            [T, F, F, T, F, T],
        ),
        (
            Value::Int(9_007_199_254_740_993),
            Value::Int(9_007_199_254_740_992),
            [F, T, F, F, T, T],
        ),
        (
            Value::Int(i64::MAX),
            Value::float(9_223_372_036_854_775_808.0),
            [T, F, F, T, F, T],
        ),
        (
            Value::Int(i64::MIN),
            Value::Int(i64::MAX),
            [F, T, T, T, F, F],
        ),
        (Value::float(-0.0), Value::float(0.0), [T, F, F, T, F, T]),
        (
            Value::Str("A".into()),
            Value::Str("a".into()),
            [F, T, T, T, F, F],
        ),
        (
            Value::Str("\u{e9}".into()),
            Value::Str("e\u{301}".into()),
            [F, T, F, F, T, T],
        ),
        (Value::Date(date), Value::Date(day), [T, F, F, T, F, T]),
        (
            Value::DateTime(datetime),
            Value::DateTime(second),
            [T, F, F, T, F, T],
        ),
    ];
    for (a, b, truth) in pairs {
        for (operator, expected) in [
            C::Equal,
            C::NotEqual,
            C::Less,
            C::LessEqual,
            C::Greater,
            C::GreaterEqual,
        ]
        .into_iter()
        .zip(truth)
        {
            let p = plan(vec![comparison(operator)], Limits::default());
            let mut port = Port::with(&[("a", a.clone()), ("b", b.clone())]);
            assert_eq!(
                p.evaluate(&mut port).unwrap(),
                expected,
                "{operator:?} {a:?} {b:?}"
            );
            assert_eq!(port.trace, ["a", "b"]);
        }
    }
    for value in [
        Value::Bool(true),
        Value::Int(1),
        Value::Str("1".into()),
        Value::Date(day),
    ] {
        for (a, b) in [(Value::Missing, value.clone()), (value, Value::Missing)] {
            assert_eq!(
                plan(vec![comparison(C::Equal)], Limits::default())
                    .evaluate(&mut Port::with(&[("a", a), ("b", b)]))
                    .unwrap(),
                Truth::Unknown
            );
        }
    }
    for (a, b, left, right) in [
        (
            Value::Bool(true),
            Value::Bool(true),
            ValueType::Bool,
            ValueType::Bool,
        ),
        (
            Value::Int(1),
            Value::Str("1".into()),
            ValueType::Int,
            ValueType::Str,
        ),
        (
            Value::Date(day),
            Value::DateTime(second),
            ValueType::Date,
            ValueType::DateTime,
        ),
    ] {
        let error = plan(vec![comparison(C::Equal)], Limits::default())
            .evaluate(&mut Port::with(&[("a", a), ("b", b)]))
            .unwrap_err();
        assert_eq!(
            error.kind,
            ErrorKind::Condition(Condition::IncompatiblePair { left, right })
        );
        assert!(error.route.is_empty());
    }
}

#[test]
fn eager_boolean_and_membership_failure_order() {
    for (truth, and) in [(false, true), (true, false)] {
        let p = plan(
            vec![
                Node::Boolean(truth),
                comparison(C::Equal),
                if and { Node::And(0, 1) } else { Node::Or(0, 1) },
            ],
            Limits::default(),
        );
        let mut port = Port::default();
        let error = p.evaluate(&mut port).unwrap_err();
        assert_eq!(port.trace, ["a"]);
        assert_eq!(error.route, [Route::Right, Route::Left]);
        assert_eq!(error.spec_path, "rows[0].filter");
        assert_eq!(error.expression, "original predicate");
        assert_eq!(
            error.kind,
            ErrorKind::Condition(Condition::UnknownField {
                identifier: "a".into()
            })
        );
    }
    let p = plan(
        vec![Node::In {
            value: id("a"),
            items: vec![id("b"), id("c")],
            negated: false,
        }],
        Limits::default(),
    );
    let mut port = Port::with(&[("a", Value::Int(1)), ("b", Value::Int(1))]);
    assert_eq!(p.evaluate(&mut port).unwrap_err().route, [Route::Item(1)]);
    assert_eq!(port.trace, ["a", "b", "c"]);
    port = Port::with(&[("a", Value::Int(1)), ("b", Value::Str("1".into()))]);
    assert!(matches!(
        p.evaluate(&mut port).unwrap_err().kind,
        ErrorKind::Condition(Condition::IncompatiblePair { .. })
    ));
    assert_eq!(port.trace, ["a", "b"]);
    for negated in [false, true] {
        for (items, expected) in [
            (vec![lit(Value::Int(1)), lit(Value::Missing)], Truth::True),
            (
                vec![lit(Value::Int(2)), lit(Value::Missing)],
                Truth::Unknown,
            ),
            (vec![lit(Value::Int(2)), lit(Value::Int(3))], Truth::False),
        ] {
            assert_eq!(
                plan(
                    vec![Node::In {
                        value: lit(Value::Int(1)),
                        items,
                        negated
                    }],
                    Limits::default()
                )
                .evaluate(&mut Port::default())
                .unwrap(),
                if negated { expected.negate() } else { expected }
            );
        }
    }
}

#[test]
fn between_repeats_subject_before_comparisons() {
    let p = plan(
        vec![Node::Between {
            value: id("x"),
            lower: id("lo"),
            upper: id("hi"),
            negated: false,
        }],
        Limits::default(),
    );
    let mut port = Port::with(&[("x", Value::Int(2)), ("lo", Value::Str("bad".into()))]);
    let error = p.evaluate(&mut port).unwrap_err();
    assert_eq!(port.trace, ["x", "lo", "x", "hi"]);
    assert_eq!(error.route, [Route::Upper]);
    assert_eq!(
        error.kind,
        ErrorKind::Condition(Condition::UnknownField {
            identifier: "hi".into()
        })
    );
    port = Port::with(&[
        ("x", Value::Int(2)),
        ("lo", Value::Int(1)),
        ("hi", Value::Int(3)),
    ]);
    port.values
        .insert("x".into(), [Value::Int(2), Value::Int(4)].into());
    assert_eq!(p.evaluate(&mut port).unwrap(), Truth::False);
    assert_eq!(port.trace, ["x", "lo", "x", "hi"]);
    port = Port::with(&[
        ("x", Value::Missing),
        ("lo", Value::Int(1)),
        ("hi", Value::Int(3)),
    ]);
    assert_eq!(p.evaluate(&mut port).unwrap(), Truth::Unknown);
}

/// This deliberately lacks Clone, proving an opaque port failure is moved intact.
#[derive(Debug, PartialEq, Eq)]
struct Failure(Box<u8>);
struct FailingPort;
impl Resolver for FailingPort {
    type Error = Failure;
    /// Model an owning scope's condition without relabeling it as an absent field.
    fn resolve(&mut self, _: &str) -> Result<Selection, Self::Error> {
        Err(Failure(Box::new(37)))
    }
}

#[test]
fn opaque_resolution_and_condition_metadata() {
    let error = plan(vec![comparison(C::Equal)], Limits::default())
        .evaluate(&mut FailingPort)
        .unwrap_err();
    assert_eq!(
        error.kind,
        ErrorKind::Resolution {
            identifier: "a".into(),
            error: Failure(Box::new(37))
        }
    );
    for (condition, name, requirement) in [
        (
            Condition::UnknownField {
                identifier: "a".into(),
            },
            "unknown_field",
            "REQ-0189",
        ),
        (
            Condition::IncompatiblePair {
                left: ValueType::Bool,
                right: ValueType::Int,
            },
            "incompatible_input_type",
            "REQ-0190",
        ),
        (
            Condition::ExpectedText {
                actual: ValueType::Float,
            },
            "incompatible_input_type",
            "REQ-0190",
        ),
        (Condition::DanglingEscape, "invalid_predicate", "REQ-0191"),
    ] {
        assert_eq!(
            (
                condition.phase(),
                condition.condition(),
                condition.requirement()
            ),
            ("validation", name, requirement)
        );
    }
}

/// Construct LIKE over literals without involving a regex library.
fn like(value: &str, pattern: &str, escape: Option<char>, limits: Limits) -> Plan {
    plan(
        vec![Node::Like {
            value: lit(Value::Str(value.into())),
            pattern: lit(Value::Str(pattern.into())),
            escape,
            negated: false,
        }],
        limits,
    )
}

#[test]
fn unicode_like_escape_and_missing_precedence() {
    for (value, pattern, escape, expected) in [
        ("", "", None, Truth::True),
        ("", "%", None, Truth::True),
        ("", "_", None, Truth::False),
        ("abc", "a%c", None, Truth::True),
        ("abc", "b", None, Truth::False),
        ("a\nb", "a_b", None, Truth::True),
        ("\u{1f600}", "_", None, Truth::True),
        ("e\u{301}", "_", None, Truth::False),
        ("e\u{301}", "__", None, Truth::True),
        ("100%", "100!%", Some('!'), Truth::True),
        ("a_b", "a\u{1f600}_b", Some('\u{1f600}'), Truth::True),
        ("a\\b", "a\\_", None, Truth::True),
        ("%", "%%", Some('%'), Truth::True),
        ("_", "__", Some('_'), Truth::True),
        ("a\0b", "a_b", None, Truth::True),
    ] {
        assert_eq!(
            like(value, pattern, escape, Limits::default())
                .evaluate(&mut Port::default())
                .unwrap(),
            expected,
            "{value:?} {pattern:?}"
        );
    }
    let error = like("no match", "abc!", Some('!'), Limits::default())
        .evaluate(&mut Port::default())
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Condition(Condition::DanglingEscape));
    let p = plan(
        vec![Node::Like {
            value: lit(Value::Missing),
            pattern: lit(Value::Int(1)),
            escape: None,
            negated: true,
        }],
        Limits::default(),
    );
    assert_eq!(p.evaluate(&mut Port::default()).unwrap(), Truth::Unknown);
    let p = plan(
        vec![Node::Like {
            value: lit(Value::Str("a".into())),
            pattern: lit(Value::Bool(false)),
            escape: None,
            negated: false,
        }],
        Limits::default(),
    );
    assert_eq!(
        p.evaluate(&mut Port::default()).unwrap_err().kind,
        ErrorKind::Condition(Condition::ExpectedText {
            actual: ValueType::Bool
        })
    );
}

/// Exhaustive short matching oracle expands percent lengths recursively, not DP.
fn brute(value: &[char], pattern: &[char]) -> bool {
    match pattern.split_first() {
        None => value.is_empty(),
        Some(('%', rest)) => (0..=value.len()).any(|n| brute(&value[n..], rest)),
        Some(('_', rest)) => !value.is_empty() && brute(&value[1..], rest),
        Some((literal, rest)) => value.first() == Some(literal) && brute(&value[1..], rest),
    }
}

/// Enumerate bounded authored alphabets, including empty strings.
fn words(alphabet: &[char], max: usize) -> Vec<String> {
    let mut result = vec![String::new()];
    let mut layer = vec![String::new()];
    for _ in 0..max {
        layer = layer
            .iter()
            .flat_map(|prefix| {
                alphabet
                    .iter()
                    .map(move |character| format!("{prefix}{character}"))
            })
            .collect();
        result.extend(layer.clone());
    }
    result
}

#[test]
fn like_matches_independent_exhaustive_short_oracle() {
    for value in words(&['a', 'b', '\n', '\u{1f600}'], 3) {
        for pattern in words(&['a', 'b', '%', '_'], 3) {
            let expected = if brute(
                &value.chars().collect::<Vec<_>>(),
                &pattern.chars().collect::<Vec<_>>(),
            ) {
                Truth::True
            } else {
                Truth::False
            };
            assert_eq!(
                like(&value, &pattern, None, Limits::default())
                    .evaluate(&mut Port::default())
                    .unwrap(),
                expected,
                "{value:?} {pattern:?}"
            );
        }
    }
}

#[test]
fn structural_budgets_include_expanded_occurrences_and_hard_depth() {
    let admit = |nodes: Vec<Node>, limits| {
        Plan::new(
            nodes.clone(),
            nodes.len() - 1,
            String::new(),
            String::new(),
            limits,
        )
    };
    assert_eq!(
        admit(vec![Node::Not(0)], Limits::default()).unwrap_err(),
        PlanError::InvalidChild
    );
    assert_eq!(
        admit(
            vec![Node::In {
                value: lit(Value::Missing),
                items: vec![],
                negated: false
            }],
            Limits::default()
        )
        .unwrap_err(),
        PlanError::EmptyMembership
    );
    let limits = Limits {
        resolutions: 3,
        ..Limits::default()
    };
    let result = admit(
        vec![Node::Between {
            value: id("x"),
            lower: id("lo"),
            upper: id("hi"),
            negated: false,
        }],
        limits,
    );
    assert!(matches!(result,Err(PlanError::Limit(e)) if e.resource==Resource::Resolutions));
    let mut nodes = vec![Node::Boolean(true)];
    for i in 0..20 {
        nodes.push(Node::And(i, i));
    }
    assert!(
        matches!(admit(nodes,Limits::default()),Err(PlanError::Limit(e)) if e.resource==Resource::Work)
    );
    let mut nodes = vec![Node::Boolean(true)];
    for i in 0..64 {
        nodes.push(Node::Not(i));
    }
    assert!(
        matches!(admit(nodes,Limits {depth:usize::MAX,..Limits::default()}),Err(PlanError::Limit(e)) if e.resource==Resource::Depth && e.limit==64)
    );
    for limits in [
        Limits {
            nodes: 0,
            ..Limits::default()
        },
        Limits {
            work: 0,
            ..Limits::default()
        },
        Limits {
            depth: 0,
            ..Limits::default()
        },
    ] {
        assert!(matches!(
            admit(vec![Node::Boolean(true)], limits),
            Err(PlanError::Limit(_))
        ));
    }
}

#[test]
fn text_and_like_limits_are_cumulative_and_reset_between_runs() {
    // '%' costs one token, two initial cells and two cells per source scalar.
    let p = like(
        "abc",
        "%",
        None,
        Limits {
            like_work: 9,
            ..Limits::default()
        },
    );
    for _ in 0..2 {
        assert_eq!(p.evaluate(&mut Port::default()).unwrap(), Truth::True);
    }
    let p = like(
        "abc",
        "%",
        None,
        Limits {
            like_work: 8,
            ..Limits::default()
        },
    );
    assert!(
        matches!(p.evaluate(&mut Port::default()).unwrap_err().kind,ErrorKind::Limit(e) if e.resource==Resource::LikeWork)
    );
    let node = Node::Like {
        value: lit(Value::Str("abc".into())),
        pattern: lit(Value::Str("%".into())),
        escape: None,
        negated: false,
    };
    let p = plan(
        vec![node, Node::Or(0, 0)],
        Limits {
            like_work: 17,
            ..Limits::default()
        },
    );
    let error = p.evaluate(&mut Port::default()).unwrap_err();
    assert_eq!(error.route, [Route::Right]);
    assert!(matches!(error.kind,ErrorKind::Limit(e) if e.resource==Resource::LikeWork));
    let p = plan(
        vec![comparison(C::Equal)],
        Limits {
            text_bytes: 64,
            ..Limits::default()
        },
    );
    let mut port = Port::with(&[
        ("a", Value::Str("a".repeat(40))),
        ("b", Value::Str("a".repeat(40))),
    ]);
    assert!(
        matches!(p.evaluate(&mut port).unwrap_err().kind,ErrorKind::Limit(e) if e.resource==Resource::TextBytes)
    );
    assert_eq!(port.trace, ["a", "b"]);
    assert_eq!(
        p.evaluate(&mut Port::with(&[
            ("a", Value::Str("a".into())),
            ("b", Value::Str("a".into()))
        ]))
        .unwrap(),
        Truth::True
    );
}

/// Decode exact fixture types independently from the Python parser/evaluator.
fn fixture_value(token: &str) -> Value {
    if token == "missing" {
        return Value::Missing;
    }
    let (kind, value) = token.split_once(':').unwrap();
    match kind {
        "i" => Value::Int(value.parse().unwrap()),
        "f" => Value::float(value.parse().unwrap()),
        "b" => Value::Bool(value.parse().unwrap()),
        "s" => Value::Str(
            String::from_utf8(
                (0..value.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap())
                    .collect(),
            )
            .unwrap(),
        ),
        _ => panic!("invalid fixture value"),
    }
}

/// The fixture's small postfix notation constructs typed IR; it is not a parser
/// for the language. The Python test separately parses the authored expression.
fn fixture_tree(text: &str) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut predicates = Vec::new();
    let mut operands = Vec::new();
    for token in text.split_whitespace() {
        let node = match token {
            "true" | "false" => Node::Boolean(token == "true"),
            "not" => Node::Not(predicates.pop().unwrap()),
            "and" | "or" => {
                let right = predicates.pop().unwrap();
                let left = predicates.pop().unwrap();
                if token == "and" {
                    Node::And(left, right)
                } else {
                    Node::Or(left, right)
                }
            }
            "eq" => {
                let right = operands.pop().unwrap();
                let left = operands.pop().unwrap();
                Node::Compare {
                    operator: C::Equal,
                    left,
                    right,
                }
            }
            "between" => {
                let upper = operands.pop().unwrap();
                let lower = operands.pop().unwrap();
                let value = operands.pop().unwrap();
                Node::Between {
                    value,
                    lower,
                    upper,
                    negated: false,
                }
            }
            _ if token.starts_with("in:") => {
                let count = token[3..].parse::<usize>().unwrap();
                let items = operands.split_off(operands.len() - count);
                Node::In {
                    value: operands.pop().unwrap(),
                    items,
                    negated: false,
                }
            }
            _ if token == "like" || token.starts_with("like:") => {
                let escape = token
                    .strip_prefix("like:")
                    .map(|hex| char::from_u32(u32::from_str_radix(hex, 16).unwrap()).unwrap());
                let pattern = operands.pop().unwrap();
                let value = operands.pop().unwrap();
                Node::Like {
                    value,
                    pattern,
                    escape,
                    negated: false,
                }
            }
            _ => {
                operands.push(if let Some(name) = token.strip_prefix('$') {
                    id(name)
                } else {
                    lit(fixture_value(token))
                });
                continue;
            }
        };
        predicates.push(nodes.len());
        nodes.push(node);
    }
    assert!(operands.is_empty());
    assert_eq!(predicates, [nodes.len() - 1]);
    nodes
}

#[test]
fn shared_reference_truth_and_occurrence_traces() {
    for line in include_str!("fixtures/predicate_evaluation.tsv")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 6);
        let nodes = fixture_tree(fields[2]);
        let p = Plan::new(
            nodes.clone(),
            nodes.len() - 1,
            "rows[0].filter".into(),
            fields[1].into(),
            Limits::default(),
        )
        .unwrap();
        let compiled = yamaa_core::predicate_compiler::compile(
            fields[1],
            "rows[0].filter",
            Default::default(),
        )
        .unwrap();
        // Both independently authored typed IR and original syntax must satisfy
        // the same truth and read trace; neither supplies the other's oracle.
        for p in [p, compiled] {
            let mut port = Port::default();
            if fields[3] != "-" {
                for binding in fields[3].split(';') {
                    let (name, value) = binding.split_once('=').unwrap();
                    port.values
                        .insert(name.into(), [fixture_value(value)].into());
                }
            }
            let result = p.evaluate(&mut port);
            let actual = match &result {
                Ok(Truth::True) => "TRUE",
                Ok(Truth::False) => "FALSE",
                Ok(Truth::Unknown) => "UNKNOWN",
                Err(error) => match &error.kind {
                    ErrorKind::Condition(condition) => condition.condition(),
                    _ => panic!("unexpected failure {error:?}"),
                },
            };
            assert_eq!(actual, fields[4], "{}", fields[0]);
            assert_eq!(port.trace.join(","), fields[5], "{}", fields[0]);
        }
    }
}

#[test]
fn shared_budgets_accumulate_across_plans_and_owning_work() {
    use yamaa_core::predicate::{Budget, Usage};
    let p = plan(vec![comparison(C::Equal)], Limits::default());
    let mut budget = Budget::new(Usage {
        work: 100,
        resolutions: 4,
        text_bytes: 5,
        like_work: 100,
        regex: Default::default(),
    });
    budget.work(2).unwrap();
    budget.text(1).unwrap();
    let mut port = Port::with(&[("a", Value::Str("x".into())), ("b", Value::Str("x".into()))]);
    for _ in 0..2 {
        assert_eq!(
            p.evaluate_with_budget(&mut port, &mut budget).unwrap(),
            Truth::True
        );
    }
    assert_eq!(
        budget.used(),
        Usage {
            work: 8,
            resolutions: 4,
            text_bytes: 5,
            like_work: 0,
            regex: Default::default(),
        }
    );
    let failure = p.evaluate_with_budget(&mut port, &mut budget).unwrap_err();
    assert!(matches!(failure.kind,ErrorKind::Limit(e) if e.resource==Resource::Resolutions));
    assert_eq!(port.trace, ["a", "b", "a", "b"]);
    assert_eq!(budget.used().work, 10);
    // Ordinary evaluation still starts a new scope and does not consume this one.
    assert_eq!(p.evaluate(&mut port).unwrap(), Truth::True);
    assert_eq!(budget.used().resolutions, 4);
}

struct BorrowingPort {
    text: String,
    calls: usize,
}
impl Resolver for BorrowingPort {
    type Error = Infallible;
    /// Any owning call would bypass the table boundary that this regression proves.
    fn resolve(&mut self, _: &str) -> Result<Selection, Self::Error> {
        panic!("owning fallback was called")
    }
    /// Lend storage so refusal precedes copying its oversized contents.
    fn resolve_value(
        &mut self,
        _: &str,
    ) -> Result<yamaa_core::predicate::Resolved<'_>, Self::Error> {
        self.calls += 1;
        Ok(yamaa_core::predicate::Resolved::Borrowed(
            yamaa_core::table::ValueRef::Str(&self.text),
        ))
    }
}

#[test]
fn borrowed_text_checks_shared_limit_before_copy_and_next_resolution() {
    use yamaa_core::predicate::{Budget, Usage};
    let p = plan(vec![comparison(C::Equal)], Limits::default());
    let mut port = BorrowingPort {
        text: "large".repeat(100),
        calls: 0,
    };
    let mut budget = Budget::new(Usage {
        work: 100,
        resolutions: 100,
        text_bytes: 499,
        like_work: 100,
        regex: Default::default(),
    });
    let error = p.evaluate_with_budget(&mut port, &mut budget).unwrap_err();
    assert!(matches!(error.kind,ErrorKind::Limit(e) if e.resource==Resource::TextBytes));
    assert_eq!(error.route, [Route::Left]);
    assert_eq!(port.calls, 1);
    assert_eq!(budget.used().text_bytes, 0);
    assert_eq!(p.identifiers(), ["a", "b"]);
}
