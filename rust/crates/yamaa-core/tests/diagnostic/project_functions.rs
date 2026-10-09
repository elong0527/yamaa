#[path = "../support/project_call_compiler.rs"]
mod support;
use std::collections::BTreeSet;
use support::{call, function, specification, Tree};
use yamaa_core::{
    diagnostic::{ConditionCode, ContextValue},
    schema::DocumentNode as N,
    specification::{PrepareError, PreparedSpecification},
    value::Value,
};
pub fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for (name, value, functions, condition, requirement, path) in [
        (
            "unknown",
            N::Integer("7".into()),
            vec![],
            "unknown_project_function",
            "REQ-0698",
            "columns.VALUE.derivation.function.name",
        ),
        (
            "id",
            N::Float(7.0),
            vec![function("id")],
            "invalid_function_argument",
            "REQ-0700",
            "columns.VALUE.derivation.function.args.x",
        ),
    ] {
        let spec = specification(vec![(
            "VALUE",
            "int",
            call(name, vec![("x", Tree::Scalar(value))]),
        )]);
        let Err(PrepareError::Invalid(findings)) =
            PreparedSpecification::prepare_with_project(&spec, &functions)
        else {
            panic!("compiler-owned function cause")
        };
        assert_eq!(findings.len(), 1);
        let diagnostic = findings[0].diagnostic();
        assert_eq!(
            (
                diagnostic.definition().phase,
                diagnostic.definition().condition,
                diagnostic.definition().requirement
            ),
            ("validation", condition, Some(requirement))
        );
        assert_eq!(diagnostic.spec_paths, [path]);
        assert_eq!(
            diagnostic.context["function"],
            ContextValue::Scalar(Value::Str(name.into()))
        );
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
        if name == "id" {
            assert_eq!(
                diagnostic.context["argument"],
                ContextValue::Scalar(Value::Str("x".into()))
            );
            assert_eq!(
                diagnostic.context["cause"],
                ContextValue::Scalar(Value::Str("type_mismatch".into()))
            );
            assert_eq!(
                diagnostic.context["expected_type"],
                ContextValue::Scalar(Value::Str("int".into()))
            );
            assert_eq!(
                diagnostic.context["actual_type"],
                ContextValue::Scalar(Value::Str("float".into()))
            );
            assert_eq!(diagnostic.context.len(), 5);
        } else {
            assert_eq!(diagnostic.context.len(), 1);
        }
        reached.insert(diagnostic.code);
    }
    reached
}
