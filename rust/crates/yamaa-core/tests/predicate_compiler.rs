use std::{collections::BTreeMap, convert::Infallible};
use yamaa_core::{
    predicate::{self, Resolver, Truth},
    predicate_compiler::{compile, Error, Limits, UnsupportedLiteral},
    predicate_parser::ParseError,
    value::{Selection, Value},
};

#[derive(Default)]
struct Port {
    values: BTreeMap<String, Value>,
    reads: Vec<String>,
}
impl Resolver for Port {
    type Error = Infallible;
    fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
        self.reads.push(name.into());
        Ok(self
            .values
            .get(name)
            .cloned()
            .map_or(Selection::Absent, Selection::Present))
    }
}

#[test]
fn original_predicate_forms_execute_with_independent_truth() {
    for (expression, truth) in [
        ("AE.AEOUT = 'FATAL'", Truth::True),
        ("AE.AEOUT <> 'RECOVERED'", Truth::True),
        ("NOT AE.AEOUT = 'FATAL' OR TRUE AND FALSE", Truth::False),
        ("NOT (AE.AEOUT <> 'FATAL' OR FALSE)", Truth::True),
        ("X = 9007199254740993", Truth::True),
        ("X <> 9007199254740992", Truth::True),
        ("-9223372036854775808 < +0001", Truth::True),
        ("-2.50e+3 <= -2500", Truth::True),
        ("1e309 = 0", Truth::Unknown),
        ("-1e9999 IS NULL", Truth::True),
        ("TRUE OR X = 1e309", Truth::True),
        ("3 > 2 AND 3 >= 3", Truth::True),
        ("M IS NULL AND X IS NOT NULL", Truth::True),
        ("AE.AEOUT IN ('RECOVERED', 'FATAL', NULL)", Truth::True),
        ("AE.AEOUT NOT IN ('RECOVERED', NULL)", Truth::Unknown),
        (
            "X BETWEEN 9007199254740993 AND 9007199254740994",
            Truth::True,
        ),
        ("X NOT BETWEEN 0 AND 5", Truth::True),
        ("AE.AEOUT LIKE 'F_T%'", Truth::True),
        ("'F_T' LIKE 'F!_T' ESCAPE '!'", Truth::True),
        ("AE.AEOUT NOT LIKE 'R%'", Truth::True),
        ("str_contains(AE.AEOUT, '^FA(T|R)AL$')", Truth::True),
        ("str_contains(M, '^$')", Truth::Unknown),
        ("DATE '2024-02-29' < DATE '2024-03-01'", Truth::True),
        (
            "DATETIME '2024-02-29T12:34' = DATETIME '2024-02-29T12:34:00'",
            Truth::True,
        ),
        ("'it''s' = 'it''s'", Truth::True),
    ] {
        let plan = compile(expression, "intermediates[0].filter", Limits::default()).unwrap();
        let mut port = Port {
            values: [
                ("AE.AEOUT".into(), Value::Str("FATAL".into())),
                ("X".into(), Value::Int(9007199254740993)),
                ("M".into(), Value::Missing),
            ]
            .into(),
            ..Default::default()
        };
        assert_eq!(plan.spec_path(), "intermediates[0].filter");
        assert_eq!(plan.evaluate(&mut port).unwrap(), truth, "{expression}");
    }
}

#[test]
fn compilation_preserves_eager_order_and_repeated_occurrences() {
    for (text, truth, reads) in [
        ("FALSE AND X = 1", Truth::False, vec!["X"]),
        ("TRUE OR X = 1", Truth::True, vec!["X"]),
        ("X = 1 AND X = 1", Truth::True, vec!["X", "X"]),
        ("X BETWEEN 0 AND 2", Truth::True, vec!["X", "X"]),
    ] {
        let plan = compile(text, "site", Limits::default()).unwrap();
        let mut port = Port {
            values: [("X".into(), Value::Int(1))].into(),
            ..Default::default()
        };
        assert_eq!(plan.evaluate(&mut port).unwrap(), truth);
        assert_eq!(port.reads, reads);
        port.reads.clear();
        assert_eq!(plan.evaluate(&mut port).unwrap(), truth);
        assert_eq!(port.reads, reads, "reusing a plan must repeat resolution");
    }
    for text in ["FALSE AND ABSENT = 1", "TRUE OR ABSENT = 1"] {
        let plan = compile(text, "site", Limits::default()).unwrap();
        let mut port = Port::default();
        let error = plan.evaluate(&mut port).unwrap_err();
        assert_eq!(port.reads, ["ABSENT"]);
        assert_eq!(error.spec_path, "site");
        assert_eq!(error.expression, text);
        assert!(matches!(
            error.kind,
            predicate::ErrorKind::Condition(predicate::Condition::UnknownField { .. })
        ));
    }
}

#[test]
fn whole_expression_grammar_and_carrier_limits_precede_execution() {
    assert!(matches!(
        compile("FALSE AND X =", "site", Limits::default()),
        Err(Error::Parse(ParseError::Grammar { .. }))
    ));
    for text in [
        "FALSE AND X = 9223372036854775808",
        "X = -9223372036854775809",
    ] {
        let Err(Error::UnsupportedLiteral { kind: actual, span }) =
            compile(text, "site", Limits::default())
        else {
            panic!("expected literal rejection")
        };
        assert_eq!(actual, UnsupportedLiteral::WideInteger);
        assert!(text[span.start..span.end].parse::<f64>().is_ok());
    }
    let limits = Limits {
        plan: predicate::Limits {
            resolutions: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(matches!(
        compile("FALSE AND X = X", "site", limits),
        Err(Error::Plan(predicate::PlanError::Limit(
            predicate::LimitError {
                resource: predicate::Resource::Resolutions,
                ..
            }
        )))
    ));
    let limits = Limits {
        plan: predicate::Limits {
            text_bytes: 3,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(matches!(
        compile("TRUE", "site", limits),
        Err(Error::Plan(predicate::PlanError::Limit(
            predicate::LimitError {
                resource: predicate::Resource::TextBytes,
                ..
            }
        )))
    ));
}
