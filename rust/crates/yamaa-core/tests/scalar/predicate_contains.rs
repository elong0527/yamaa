//! Independent REQ-1244 truth, error ordering and the fixed per-match boundary.
use yamaa_core::{
    predicate::{Condition, ErrorKind, Node, Plan, PlanError, Resolver, Route, Scalar, Truth},
    regex::CompileError,
    value::{Selection, Value, ValueType},
};

struct Port {
    value: Value,
    reads: Vec<String>,
    failure: bool,
}
impl Resolver for Port {
    type Error = &'static str;
    /// Retain occurrence order, including eager reads after a decisive truth value.
    fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
        self.reads.push(name.into());
        if self.failure {
            Err("opaque failure")
        } else {
            Ok(Selection::Present(self.value.clone()))
        }
    }
}
/// A single observable source occurrence and an independently authored pattern.
fn contains(pattern: &str) -> Node {
    Node::Contains {
        value: Scalar::Identifier("s".into()),
        pattern: pattern.into(),
    }
}
/// Admit the entire arena before constructing or invoking any resolver.
fn plan(nodes: Vec<Node>) -> Plan {
    let root = nodes.len() - 1;
    Plan::new(
        nodes,
        root,
        "rows[0].filter".into(),
        "str_contains(s, 'a')".into(),
    )
    .unwrap()
}
/// Each execution receives its own observable source port.
fn port(value: Value) -> Port {
    Port {
        value,
        reads: vec![],
        failure: false,
    }
}

#[test]
fn contains_search_truth_missing_negation_and_type_requirement() {
    for (pattern, subject, expected) in [
        ("a", "cat", Truth::True),
        ("^a$", "cat", Truth::False),
        ("(?<letter>a)\\k<letter>", "baac", Truth::True),
        ("(?<=a)b", "ab", Truth::True),
        ("^.$", "😀", Truth::True),
        ("z", "cat", Truth::False),
        ("", "", Truth::True),
    ] {
        let p = plan(vec![contains(pattern)]);
        let mut source = port(Value::Str(subject.into()));
        assert_eq!(
            p.evaluate(&mut source).unwrap(),
            expected,
            "{pattern}: {subject}"
        );
        assert_eq!(source.reads, ["s"]);
        assert_eq!(p.identifiers(), ["s"]);
    }
    for negated in [false, true] {
        let mut nodes = vec![contains("a")];
        if negated {
            nodes.push(Node::Not(0));
        }
        let p = plan(nodes);
        assert_eq!(
            p.evaluate(&mut port(Value::Missing)).unwrap(),
            Truth::Unknown
        );
        for (value, actual) in [
            (Value::Int(1), ValueType::Int),
            (Value::Bool(false), ValueType::Bool),
        ] {
            let error = p.evaluate(&mut port(value)).unwrap_err();
            let condition = Condition::ContainsExpectedText { actual };
            assert_eq!(error.kind, ErrorKind::Condition(condition.clone()));
            assert_eq!(condition.condition(), "incompatible_input_type");
            assert_eq!(condition.requirement(), "REQ-1244");
            assert_eq!(error.spec_path, "rows[0].filter");
            assert_eq!(
                error.route,
                if negated { vec![Route::Child] } else { vec![] }
            );
        }
    }
}

#[test]
fn contains_admits_every_literal_including_unreachable_nodes() {
    let error = Plan::new(
        vec![contains("["), Node::Boolean(false)],
        1,
        "p".into(),
        "text".into(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        PlanError::Regex {
            node: 0,
            error: CompileError::Invalid { .. }
        }
    ));
    let pattern = format!("(a){}(?<=\\1)", "(a|aa)".repeat(8));
    plan(vec![contains(&pattern)]);
    Plan::new(
        vec![contains(&pattern); 512],
        511,
        "p".into(),
        "text".into(),
    )
    .unwrap();
}

#[test]
fn contains_eager_order_and_original_failure_precede_matching() {
    for (first, combine) in [(false, Node::And(0, 1)), (true, Node::Or(0, 1))] {
        let p = plan(vec![Node::Boolean(first), contains("a"), combine]);
        let mut source = port(Value::Missing);
        source.failure = true;
        let error = p.evaluate(&mut source).unwrap_err();
        assert_eq!(source.reads, ["s"]);
        assert_eq!(error.route, [Route::Right, Route::Value]);
        assert_eq!(
            error.kind,
            ErrorKind::Resolution {
                identifier: "s".into(),
                error: "opaque failure"
            }
        );
    }
    let p = plan(vec![contains("(a+)+$")]);
    assert_eq!(
        p.evaluate(&mut port(Value::Missing)).unwrap(),
        Truth::Unknown
    );
    assert_eq!(
        p.evaluate(&mut port(Value::Int(0))).unwrap_err().kind,
        ErrorKind::Condition(Condition::ContainsExpectedText {
            actual: ValueType::Int
        })
    );
}

#[test]
fn contains_enforces_the_fixed_per_match_boundary_and_keeps_small_matches_independent() {
    let single = plan(vec![contains("a+")]);
    for _ in 0..2 {
        let mut source = port(Value::Str("aaaa".into()));
        assert_eq!(single.evaluate(&mut source).unwrap(), Truth::True);
        assert_eq!(source.reads, ["s"]);
    }
    let repeated = plan(vec![contains("a+"), contains("a+"), Node::And(0, 1)]);
    let mut source = port(Value::Str("aaaa".into()));
    assert_eq!(repeated.evaluate(&mut source).unwrap(), Truth::True);
    assert_eq!(source.reads, ["s", "s"]);
    let bounded = plan(vec![contains("a+"), contains("(a+)+$"), Node::And(0, 1)]);
    for _ in 0..2 {
        let mut source = port(Value::Str("a".repeat(30) + "!"));
        let error = bounded.evaluate(&mut source).unwrap_err();
        assert_eq!(error.route, [Route::Right]);
        assert_eq!(
            error.kind,
            ErrorKind::Regex(yamaa_core::regex::MatchError {
                resource: yamaa_core::regex::Resource::Work,
                limit: 1_000_000,
            })
        );
        assert_eq!(source.reads, ["s", "s"]);
    }
}
