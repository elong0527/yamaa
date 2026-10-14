use std::{collections::BTreeSet, convert::Infallible};
use yamaa_core::{
    diagnostic::{ConditionCode, ContextValue as V, PredicateDiagnosticError as P},
    predicate::{
        Comparison, Condition, ErrorKind, EvaluationError, LimitError, Limits, Node, Plan,
        Resolver, Resource, Route, Scalar,
    },
    value::{Selection, Value},
};
fn text(s: &str) -> V {
    V::Scalar(Value::Str(s.into()))
}
#[test]
fn original_context_projection_admits_all_fields_and_retains_rich_parser_facts() {
    use yamaa_core::{
        diagnostic::PredicateGrammarDiagnosticError as G,
        predicate_parser::{parse_predicate, GrammarFailure, ParseError},
    };
    for (expression, requirement, character, detail) in [
        ("STR_CONTAINS(ID, '(')", "REQ-1244", 17, "pattern_byte"),
        ("ID = DATE '2026-02-30'", "REQ-0188", 10, "temporal_error"),
    ] {
        let error = parse_predicate(expression, Default::default()).unwrap_err();
        let ParseError::Grammar {
            failure,
            position,
            message,
        } = &error
        else {
            panic!("independent rejected original form")
        };
        assert_eq!(position.character, character);
        let detail_before = failure.syntax_diagnostic();
        assert!(detail_before.context.contains_key(detail));
        let message_address = message.as_ptr();
        let finding = failure
            .original_diagnostic(
                "columns.VALUE.derivation.row_number.window.filter",
                expression,
                position.character,
            )
            .unwrap();
        assert_eq!(finding.definition().requirement, Some(requirement));
        assert_eq!(
            finding.context,
            [
                ("predicate".into(), text(expression)),
                ("position".into(), V::Integer(character.to_string())),
            ]
            .into()
        );
        assert_eq!(finding.source_span, None);
        assert_eq!(finding.operand_route, None);
        assert_eq!(
            failure.original_diagnostic_with_limit("rows[0].filter", expression, usize::MAX, 1),
            Err(G::OutputLimit)
        );
        assert_eq!(failure.syntax_diagnostic(), detail_before);
        assert_eq!(message.as_ptr(), message_address);
    }
    // Preserve the recorded contradiction: native/rule ESCAPE owns REQ-0191,
    // while the unchanged original Python report currently uses REQ-0188.
    let ParseError::Grammar {
        failure: GrammarFailure::InvalidEscape,
        position,
        ..
    } = parse_predicate("ID LIKE '%' ESCAPE 'xx'", Default::default()).unwrap_err()
    else {
        panic!("invalid ESCAPE width")
    };
    let finding = GrammarFailure::InvalidEscape
        .original_diagnostic(
            "rows[0].filter",
            "ID LIKE '%' ESCAPE 'xx'",
            position.character,
        )
        .unwrap();
    assert_eq!(finding.definition().requirement, Some("REQ-0191"));
    assert_eq!(finding.context["position"], V::Integer("19".into()));
    let huge = "\0".repeat(1_400_000);
    assert_eq!(
        GrammarFailure::InvalidExpression.original_diagnostic("rows[0].filter", &huge, usize::MAX),
        Err(G::OutputLimit)
    );
    let full = GrammarFailure::InvalidExpression
        .original_diagnostic("rows[0].filter", "A >", usize::MAX)
        .unwrap();
    assert_eq!(full.context["position"], V::Integer(usize::MAX.to_string()));
}
struct Absent;
impl Resolver for Absent {
    type Error = Infallible;
    fn resolve(&mut self, _: &str) -> Result<Selection, Infallible> {
        Ok(Selection::Absent)
    }
}
fn plan(nodes: Vec<Node>) -> Plan {
    let root = nodes.len() - 1;
    Plan::new(
        nodes,
        root,
        "rows[2].filter".into(),
        "é\0🙂 authored predicate".into(),
        Limits::default(),
    )
    .unwrap()
}
fn literal(v: Value) -> Scalar {
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
#[test]
fn every_native_predicate_cause_retains_independent_context_and_structural_route() {
    assert_eq!(reached().len(), 5)
}
struct Payload {
    original: Box<u8>,
}
struct Port {
    payload: Option<Payload>,
    trace: Vec<String>,
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
#[test]
fn upper_resolution_outranks_lower_type_error_and_projection_never_retries() {
    let original = Box::new(41);
    let pointer = &*original as *const u8;
    let mut port = Port {
        payload: Some(Payload { original }),
        trace: vec![],
    };
    let p = plan(vec![Node::Between {
        value: Scalar::Identifier("v".into()),
        lower: Scalar::Identifier("low".into()),
        upper: Scalar::Identifier("high".into()),
        negated: false,
    }]);
    let Err(error) = p.evaluate(&mut port) else {
        panic!("opaque upper error")
    };
    assert_eq!(port.trace, ["v", "low", "v", "high"]);
    assert_eq!(error.route, [Route::Upper]);
    for maximum in [0, 16_777_216] {
        let Err(P::Resolution(payload)) = error.diagnostic_with_limit(maximum) else {
            panic!("borrowed original")
        };
        assert_eq!(&*payload.original as *const u8, pointer);
    }
    assert_eq!(port.trace, ["v", "low", "v", "high"]);
    let ErrorKind::Resolution {
        identifier,
        error: payload,
    } = error.kind
    else {
        panic!("still original")
    };
    assert_eq!(identifier, "high");
    assert_eq!(&*payload.original as *const u8, pointer);
}
#[test]
fn complete_borrowed_quota_and_full_width_routes_have_no_partial_finding() {
    let mut error: EvaluationError<Infallible> = EvaluationError {
        spec_path: "rows[0].filter".into(),
        expression: "unchanged".into(),
        route: vec![
            Route::Child,
            Route::Left,
            Route::Right,
            Route::Value,
            Route::Item(usize::MAX),
            Route::Lower,
            Route::RepeatedValue,
            Route::Upper,
            Route::Pattern,
        ],
        kind: ErrorKind::Condition(Condition::DanglingEscape),
    };
    let d = error.diagnostic().unwrap();
    assert_eq!(
        d.operand_route,
        Some(vec![
            "Child".into(),
            "Left".into(),
            "Right".into(),
            "Value".into(),
            format!("Item({})", usize::MAX),
            "Lower".into(),
            "RepeatedValue".into(),
            "Upper".into(),
            "Pattern".into()
        ])
    );
    assert!(matches!(
        error.diagnostic_with_limit(1),
        Err(P::OutputLimit)
    ));
    error.route = vec![Route::Left; 65_536];
    assert!(matches!(error.diagnostic(), Err(P::OutputLimit)));
    error.route = vec![];
    error.kind = ErrorKind::Condition(Condition::UnknownField {
        identifier: "\0".repeat(1_400_000),
    });
    assert!(matches!(error.diagnostic(), Err(P::OutputLimit)));
    error.kind = ErrorKind::Limit(LimitError {
        resource: Resource::RegexWork,
        limit: 7,
    });
    let ErrorKind::Limit(ref native) = error.kind else {
        unreachable!()
    };
    let Err(P::Resource(held)) = error.diagnostic_with_limit(0) else {
        panic!("original limit")
    };
    assert!(std::ptr::eq(held, native));
    assert_eq!((held.resource, held.limit), (Resource::RegexWork, 7));
    assert_eq!(error.expression, "unchanged");
}
