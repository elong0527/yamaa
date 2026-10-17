#![allow(unused_imports)]
pub(crate) use std::{collections::BTreeSet, convert::Infallible};
pub(crate) use yamaa_core::json::Value as V;
pub(crate) use yamaa_core::{
    diagnostic::{ConditionCode, PredicateDiagnosticError as P},
    portable_regex as regex,
    predicate::{
        Comparison, Condition, ErrorKind, EvaluationError, Node, Plan, Resolver, Route, Scalar,
    },
    value::{Selection, Value},
};
pub(crate) fn text(s: &str) -> V {
    V::String(s.into())
}
pub(crate) struct Absent;
impl Resolver for Absent {
    type Error = Infallible;
    fn resolve(&mut self, _: &str) -> Result<Selection, Infallible> {
        Ok(Selection::Absent)
    }
}
pub(crate) fn plan(nodes: Vec<Node>) -> Plan {
    let root = nodes.len() - 1;
    Plan::new(
        nodes,
        root,
        "rows[2].filter".into(),
        "é\0🙂 authored predicate".into(),
    )
    .unwrap()
}
pub(crate) fn literal(v: Value) -> Scalar {
    Scalar::Literal(v)
}
pub fn reached() -> BTreeSet<ConditionCode> {
    let causes = [
        (
            vec![
                Node::Boolean(false),
                Node::Compare {
                    operator: Comparison::Equal,
                    left: Scalar::Identifier("UNKNOWN\0🙂".into()),
                    right: literal(Value::Int(1)),
                },
                Node::And(0, 1),
            ],
            "unknown_field",
            "REQ-0189",
            vec![("identifier", text("UNKNOWN\0🙂"))],
            vec!["Right", "Left"],
        ),
        (
            vec![
                Node::Compare {
                    operator: Comparison::Equal,
                    left: literal(Value::Bool(true)),
                    right: literal(Value::Int(1)),
                },
                Node::Not(0),
            ],
            "incompatible_input_type",
            "REQ-0190",
            vec![("left_type", text("bool")), ("right_type", text("int"))],
            vec!["Child"],
        ),
        (
            vec![Node::Like {
                value: literal(Value::Date("2024-02-29".parse().unwrap())),
                pattern: literal(Value::Str("%".into())),
                escape: None,
                negated: false,
            }],
            "incompatible_input_type",
            "REQ-0190",
            vec![("expected", text("str")), ("actual", text("date"))],
            vec![],
        ),
        (
            vec![
                Node::Boolean(true),
                Node::Contains {
                    value: literal(Value::Bool(true)),
                    pattern: "a".into(),
                },
                Node::Or(0, 1),
            ],
            "incompatible_input_type",
            "REQ-1244",
            vec![("expected", text("str")), ("actual", text("bool"))],
            vec!["Right"],
        ),
        (
            vec![Node::Like {
                value: literal(Value::Str("text".into())),
                pattern: literal(Value::Str("tail!".into())),
                escape: Some('!'),
                negated: false,
            }],
            "invalid_predicate",
            "REQ-0191",
            vec![("reason", text("LIKE pattern has a dangling escape"))],
            vec![],
        ),
    ];
    let mut reached = BTreeSet::new();
    for (nodes, condition, requirement, fields, route) in causes {
        let error = plan(nodes).evaluate(&mut Absent).unwrap_err();
        let d = error.diagnostic().unwrap();
        assert_eq!(
            (
                d.definition().phase,
                d.definition().condition,
                d.definition().requirement
            ),
            ("validation", condition, Some(requirement))
        );
        let ErrorKind::Condition(c) = &error.kind else {
            panic!("language condition")
        };
        assert_eq!(
            (c.phase(), c.condition(), c.requirement()),
            ("validation", condition, requirement)
        );
        assert_eq!(d.spec_paths, ["rows[2].filter"]);
        assert_eq!(
            d.context,
            fields.into_iter().map(|(k, v)| (k.into(), v)).collect()
        );
        assert_eq!(
            d.operand_route,
            Some(route.into_iter().map(String::from).collect())
        );
        assert_eq!(d.source_span, None);
        assert_eq!(error.expression, "é\0🙂 authored predicate");
        assert!(reached.insert(d.code));
    }
    reached
}
pub(crate) struct Payload {
    pub(crate) original: Box<u8>,
}
pub(crate) struct Port {
    pub(crate) payload: Option<Payload>,
    pub(crate) trace: Vec<String>,
}
impl Resolver for Port {
    type Error = Payload;
    fn resolve(&mut self, name: &str) -> Result<Selection, Payload> {
        self.trace.push(name.into());
        match name {
            "v" => Ok(Selection::Present(Value::Bool(true))),
            "low" => Ok(Selection::Present(Value::Int(0))),
            "high" => Err(self.payload.take().unwrap()),
            _ => Ok(Selection::Absent),
        }
    }
}
