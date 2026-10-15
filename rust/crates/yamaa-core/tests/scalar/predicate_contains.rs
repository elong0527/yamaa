//! Independent REQ-1244 truth, error ordering and cumulative execution budgets.
use yamaa_core::{
    predicate::{
        Budget, Condition, ErrorKind, Limits, Node, Plan, PlanError, Resolver, Resource, Route,
        Scalar, Truth, Usage,
    },
    regex::{CompileError, MatchLimits, MatchUsage},
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
fn plan(nodes: Vec<Node>, limits: Limits) -> Plan {
    let root = nodes.len() - 1;
    Plan::new(
        nodes,
        root,
        "rows[0].filter".into(),
        "str_contains(s, 'a')".into(),
        limits,
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
        let p = plan(vec![contains(pattern)], Limits::default());
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
        let p = plan(nodes, Limits::default());
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
fn contains_admits_unreachable_literals_and_shares_compilation_budget() {
    let error = Plan::new(
        vec![contains("["), Node::Boolean(false)],
        1,
        "p".into(),
        "text".into(),
        Limits::default(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        PlanError::Regex {
            node: 0,
            error: CompileError::Invalid { .. }
        }
    ));
    // Every literal is legal in isolation. Together their path analysis must refuse.
    let pattern = format!("(a){}(?<=\\1)", "(a|aa)".repeat(8));
    plan(vec![contains(&pattern)], Limits::default());
    let error = Plan::new(
        vec![contains(&pattern); 512],
        511,
        "p".into(),
        "text".into(),
        Limits::default(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        PlanError::Regex {
            error: CompileError::Limit {
                resource: yamaa_core::regex::Resource::WidthWork
                    | yamaa_core::regex::Resource::WidthCells,
                ..
            },
            ..
        }
    ));
}

#[test]
fn contains_eager_order_and_opaque_failure_do_not_enter_matching() {
    for (first, combine) in [(false, Node::And(0, 1)), (true, Node::Or(0, 1))] {
        let p = plan(
            vec![Node::Boolean(first), contains("a"), combine],
            Limits::default(),
        );
        let mut source = port(Value::Missing);
        source.failure = true;
        let mut budget = Budget::new(Limits::default().into());
        let error = p
            .evaluate_with_budget(&mut source, &mut budget)
            .unwrap_err();
        assert_eq!(source.reads, ["s"]);
        assert_eq!(error.route, [Route::Right, Route::Value]);
        assert_eq!(
            error.kind,
            ErrorKind::Resolution {
                identifier: "s".into(),
                error: "opaque failure"
            }
        );
        assert_eq!(budget.used().regex, MatchUsage::default());
    }
    let p = plan(vec![contains("a")], Limits::default());
    for value in [Value::Missing, Value::Int(0)] {
        let mut budget = Budget::new(Limits::default().into());
        let _ = p.evaluate_with_budget(&mut port(value), &mut budget);
        assert_eq!(budget.used().regex, MatchUsage::default());
    }
}

#[test]
fn contains_budgets_span_nodes_and_evaluations_without_refunding_failures() {
    let single = plan(vec![contains("a+")], Limits::default());
    let mut measured = Budget::new(Limits::default().into());
    assert_eq!(
        single
            .evaluate_with_budget(&mut port(Value::Str("aaaa".into())), &mut measured)
            .unwrap(),
        Truth::True
    );
    let used = measured.used().regex;
    for (resource, regex) in [
        (
            Resource::RegexSubjectBytes,
            MatchLimits {
                subject_bytes: 7,
                ..MatchLimits::default()
            },
        ),
        (
            Resource::RegexWork,
            MatchLimits {
                work: used.work * 2 - 1,
                ..MatchLimits::default()
            },
        ),
        (
            Resource::RegexStateCells,
            MatchLimits {
                state_cells: used.state_cells * 2 - 1,
                ..MatchLimits::default()
            },
        ),
    ] {
        // Local predicate ceilings apply across both nodes, even with a generous caller.
        let p = plan(
            vec![contains("a+"), contains("a+"), Node::And(0, 1)],
            Limits {
                regex,
                ..Limits::default()
            },
        );
        let mut shared = Budget::new(Limits::default().into());
        let mut source = port(Value::Str("aaaa".into()));
        let error = p
            .evaluate_with_budget(&mut source, &mut shared)
            .unwrap_err();
        assert!(matches!(error.kind, ErrorKind::Limit(e) if e.resource == resource));
        assert_eq!(error.route, [Route::Right]);
        assert_eq!(source.reads, ["s", "s"]);
        // Shared ceilings persist across distinct evaluations of the same plan.
        let quotas = Usage::from(Limits {
            regex,
            ..Limits::default()
        });
        let mut shared = Budget::new(quotas);
        assert_eq!(
            single
                .evaluate_with_budget(&mut source, &mut shared)
                .unwrap(),
            Truth::True
        );
        let prefix = shared.used().regex;
        let error = single
            .evaluate_with_budget(&mut source, &mut shared)
            .unwrap_err();
        assert!(matches!(error.kind, ErrorKind::Limit(e) if e.resource == resource));
        assert!(shared.used().regex.work >= prefix.work);
        assert!(shared.used().regex.state_cells >= prefix.state_cells);
        assert!(shared.used().regex.subject_bytes >= prefix.subject_bytes);
        assert_eq!(
            single
                .evaluate_with_budget(&mut source, &mut Budget::new(quotas))
                .unwrap(),
            Truth::True
        );
    }
}
