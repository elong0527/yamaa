#[path = "../support/project_call_compiler.rs"]
mod support;
use std::collections::BTreeSet;
use support::{call, function, specification, specification_with_rows, Tree};
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
    let source = Tree::map(vec![(
        "source",
        Tree::map(vec![("variable", Tree::text("SRC.ID"))]),
    )]);
    let spec = specification_with_rows(
        vec![
            (
                "A",
                "int",
                Tree::map(vec![(
                    "aggregate",
                    Tree::map(vec![("expr", Tree::text("SUM(SRC.ID)"))]),
                )]),
            ),
            (
                "B",
                "int",
                Tree::map(vec![("literal", Tree::Scalar(N::Integer("0".into())))]),
            ),
        ],
        vec![Tree::map(vec![
            ("id", Tree::text("record")),
            (
                "derivations",
                Tree::map(vec![
                    ("ID", Tree::map(vec![("value", source)])),
                    (
                        "B",
                        Tree::map(vec![("value", call("id", vec![("x", Tree::text("A"))]))]),
                    ),
                ]),
            ),
        ])],
    );
    let Err(PrepareError::Invalid(findings)) =
        PreparedSpecification::prepare_with_project(&spec, &[function("id")])
    else {
        panic!("row phase cause")
    };
    assert_eq!(findings.len(), 1);
    let diagnostic = findings[0].diagnostic();
    assert_eq!(
        (
            diagnostic.definition().phase,
            diagnostic.definition().condition,
            diagnostic.definition().requirement
        ),
        ("validation", "phase_boundary", Some("REQ-0069"))
    );
    assert_eq!(
        diagnostic.spec_paths,
        ["rows[0].derivations.B.function.args.x"]
    );
    for (key, value) in [
        ("function", "id"),
        ("argument", "x"),
        ("identifier", "A"),
        ("row", "record"),
        ("available_phase", "column_derivation"),
        ("required_phase", "row_construction"),
    ] {
        assert_eq!(
            diagnostic.context[key],
            ContextValue::Scalar(Value::Str(value.into()))
        );
    }
    assert_eq!(diagnostic.context.len(), 6);
    reached.insert(diagnostic.code);
    reached
}
