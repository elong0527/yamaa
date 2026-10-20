#![allow(unused_imports)]
pub(crate) use crate::support::project_call_compiler as support;
pub(crate) use support::{specification, Tree};
pub(crate) use yamaa_core::{
    bound_expression::Read,
    dataset::{Flag, FlagBinding, FlagError, FlagRead},
    diagnostic::ConditionCode as C,
    expression::{compile, Condition, Context, ErrorKind, EvaluationError, Step as Route, Truth},
    schema::DocumentNode as N,
    specification::{BindError, PreparedSpecification},
    table::{Column, TableSchema, ValueRef},
    value::{ColumnType, Value, ValueType},
};
pub(crate) fn flag(condition: &str, false_value: Option<Tree>, missing: Option<Tree>) -> Tree {
    let mut fields = vec![("condition", Tree::text(condition))];
    if let Some(v) = false_value {
        fields.push(("false_value", v));
    }
    if let Some(v) = missing {
        fields.push(("missing", v));
    }
    Tree::map(vec![("flag", Tree::map(fields))])
}
pub(crate) fn diagnostics(
    columns: Vec<(&str, &str, Tree)>,
) -> Vec<yamaa_core::diagnostic::Diagnostic> {
    let spec = specification(columns);
    let prepared = PreparedSpecification::prepare(&spec).unwrap();
    let source = TableSchema::new(vec![
        Column {
            name: "ID".into(),
            kind: ColumnType::Int,
        },
        Column {
            name: "EXTRA".into(),
            kind: ColumnType::Int,
        },
    ])
    .unwrap();
    let Err(BindError::Invalid(findings)) = prepared.bind(&source) else {
        panic!("findings")
    };
    findings
        .iter()
        .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
        .collect()
}

#[allow(dead_code)]
pub fn reached() -> std::collections::BTreeSet<C> {
    let mut reached = diagnostics(vec![(
        "F",
        "str",
        flag("ABSENT = 1", Some(Tree::text("N")), None),
    )])
    .into_iter()
    .map(|d| d.code)
    .collect::<std::collections::BTreeSet<_>>();
    reached.extend(
        diagnostics(vec![("F", "str", flag("EXTRA = 1", None, None))])
            .into_iter()
            .map(|d| d.code),
    );
    pub(crate) struct IntPort;
    impl yamaa_core::expression::Resolver for IntPort {
        type Error = std::convert::Infallible;
        fn resolve(&mut self, _: &str) -> Result<yamaa_core::value::Selection, Self::Error> {
            Ok(yamaa_core::value::Selection::Present(Value::Int(1)))
        }
    }
    let plan = compile(
        "str_contains(SRC.EXTRA,'a')",
        Context::Predicate,
        "flag.condition",
    )
    .unwrap();
    let finding = yamaa_core::dataset::flag_condition_diagnostic(
        &plan.evaluate_truth(&mut IntPort).unwrap_err(),
    )
    .unwrap();
    assert_eq!(finding.definition().requirement, Some("FORMULA-5"));
    reached.insert(finding.code);
    assert_eq!(
        reached,
        [
            C::FlagMissingValueRequired,
            C::PredicateUnknownField,
            C::PredicateUnresolvableName,
            C::PredicateInputType
        ]
        .into()
    );
    reached
}
