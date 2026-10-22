#![allow(unused_imports)]
use super::rendered;
pub(crate) use super::text;
pub(crate) use std::{collections::BTreeSet, convert::Infallible};
pub(crate) use yamaa_core::json::Value as V;
pub(crate) use yamaa_core::{
    diagnostic::{ConditionCode, EvaluationDiagnosticError as P},
    expression::{
        compile, Comparison, CompiledExpression, Condition, Context, ErrorKind, EvaluationError,
        Resolver, Step as Route,
    },
    portable_regex as regex,
    value::{Selection, Value},
};
/// Every name is absent.
pub(crate) struct Absent;
impl Resolver for Absent {
    type Error = Infallible;
    fn resolve(&mut self, _: &str) -> Result<Selection, Infallible> {
        Ok(Selection::Absent)
    }
}
/// Fixed typed values, including a bool and a date no literal can spell.
pub(crate) struct Typed;
impl Resolver for Typed {
    type Error = Infallible;
    fn resolve(&mut self, name: &str) -> Result<Selection, Infallible> {
        Ok(match name {
            "FLAG" => Selection::Present(Value::Bool(true)),
            "DAY" => Selection::Present(Value::Date("2024-02-29".parse().unwrap())),
            "TEXT" => Selection::Present(Value::Str("text".into())),
            "PATTERN" => Selection::Present(Value::Str("tail!".into())),
            _ => Selection::Absent,
        })
    }
}
pub(crate) fn plan(expression: &str) -> CompiledExpression {
    compile(expression, Context::Predicate, "rows[2].filter").unwrap()
}
pub fn reached() -> BTreeSet<ConditionCode> {
    let causes = [
        (
            "FALSE AND UNKNOWN = 1",
            "unknown_field",
            "FORMULA-9",
            vec![("identifier", text("UNKNOWN"))],
            vec!["Right", "Left"],
        ),
        (
            "NOT (FLAG = 1)",
            "incompatible_input_type",
            "FORMULA-5",
            vec![("left_type", text("bool")), ("right_type", text("int"))],
            vec!["Child"],
        ),
        (
            "DAY LIKE '%'",
            "incompatible_input_type",
            "FORMULA-5",
            vec![("expected", text("str")), ("actual", text("date"))],
            vec![],
        ),
        (
            "TRUE OR str_contains(FLAG, 'a')",
            "incompatible_input_type",
            "FORMULA-8",
            vec![("expected", text("str")), ("actual", text("bool"))],
            vec!["Right"],
        ),
        (
            "TEXT LIKE PATTERN ESCAPE '!'",
            "invalid_expression",
            "FORMULA-7",
            vec![("reason", text("LIKE pattern has a dangling escape"))],
            vec![],
        ),
    ];
    let mut reached = BTreeSet::new();
    for (expression, condition, rule, fields, route) in causes {
        let error = match expression {
            "FALSE AND UNKNOWN = 1" => plan(expression).evaluate_truth(&mut Absent),
            _ => plan(expression).evaluate_truth(&mut Typed),
        }
        .unwrap_err();
        let d = error.diagnostic().unwrap();
        assert_eq!(
            (
                d.definition().phase,
                d.definition().condition,
                d.definition().rule
            ),
            ("validation", condition, Some(rule))
        );
        let ErrorKind::Condition(c) = &error.kind else {
            panic!("language condition")
        };
        assert_eq!(
            (c.phase(), c.condition(), c.rule()),
            ("validation", condition, rule)
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
        assert_eq!(error.expression, expression);
        assert!(reached.insert(rendered(&d)));
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
