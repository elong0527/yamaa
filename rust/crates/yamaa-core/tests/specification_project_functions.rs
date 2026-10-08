#[path = "support/project_call_compiler.rs"]
mod support;
use support::{call, function, specification, Tree};
use yamaa_core::{
    bound_expression::Read,
    dataset::{Expression, FunctionInput},
    diagnostic::ContextValue,
    schema::DocumentNode as N,
    specification::{
        BindError, BindFinding, FunctionCause, PreflightFinding, PrepareError,
        PreparedSpecification,
    },
    table::{Column, TableSchema},
    value::{ColumnType, Value},
};
fn source() -> TableSchema {
    TableSchema::new(vec![Column {
        name: "ID".into(),
        kind: ColumnType::Int,
    }])
    .unwrap()
}

#[test]
fn compiler_selects_environment_slots_and_lowers_output_argument_dependencies() {
    let spec = specification(vec![
        ("A", "int", call("beta", vec![("x", Tree::text("ID"))])),
        ("B", "int", call("alpha", vec![("x", Tree::text("A"))])),
    ]);
    let prepared = PreparedSpecification::prepare_with_project(
        &spec,
        &[function("alpha"), function("beta"), function("unused")],
    )
    .unwrap();
    assert_eq!(prepared.called_functions(), [0, 1]);
    let plan = prepared.bind(&source()).unwrap();
    assert_eq!(
        plan.columns().iter().map(|a| a.column).collect::<Vec<_>>(),
        [1, 2]
    );
    for (assignment, slot, column) in [(&plan.columns()[0], 1, 0), (&plan.columns()[1], 0, 1)] {
        let Expression::ProjectFunction(bound) = &assignment.expression else {
            panic!("versionless function expression")
        };
        assert_eq!(bound.slot(), slot);
        assert_eq!(
            bound.arguments()[0].input,
            FunctionInput::Read(Read::Column(column))
        );
    }
}

#[test]
fn complete_static_faults_are_projected_in_authored_call_order_without_source_binding() {
    let spec = specification(vec![
        (
            "A",
            "int",
            call("unknown", vec![("x", Tree::Scalar(N::Integer("7".into())))]),
        ),
        (
            "B",
            "int",
            call("id", vec![("x", Tree::Scalar(N::Float(7.0)))]),
        ),
        (
            "C",
            "int",
            call(
                "id",
                vec![("x", Tree::Scalar(N::Integer("9223372036854775808".into())))],
            ),
        ),
    ]);
    let Err(PrepareError::Invalid(findings)) =
        PreparedSpecification::prepare_with_project(&spec, &[function("id")])
    else {
        panic!("complete static findings")
    };
    assert_eq!(findings.len(), 3);
    let diagnostics = findings.iter().map(|f| f.diagnostic()).collect::<Vec<_>>();
    assert_eq!(
        diagnostics
            .iter()
            .map(|d| d.spec_paths[0].as_str())
            .collect::<Vec<_>>(),
        [
            "columns.A.derivation.function.name",
            "columns.B.derivation.function.args.x",
            "columns.C.derivation.function.args.x",
        ]
    );
    for (d, condition, requirement) in [
        (&diagnostics[0], "unknown_project_function", "REQ-0698"),
        (&diagnostics[1], "invalid_function_argument", "REQ-0700"),
        (&diagnostics[2], "invalid_function_argument", "REQ-0700"),
    ] {
        assert_eq!(
            (
                d.definition().phase,
                d.definition().condition,
                d.definition().requirement
            ),
            ("validation", condition, Some(requirement))
        );
    }
    assert_eq!(
        diagnostics[1].context["expected_type"],
        ContextValue::Scalar(Value::Str("int".into()))
    );
    assert_eq!(
        diagnostics[1].context["actual_type"],
        ContextValue::Scalar(Value::Str("float".into()))
    );
    assert_eq!(
        diagnostics[2].context["cause"],
        ContextValue::Scalar(Value::Str("integer_range".into()))
    );
}

#[test]
fn declared_output_reference_types_and_unknown_names_fail_during_prepare() {
    let spec = specification(vec![
        (
            "F",
            "float",
            Tree::map(vec![("literal", Tree::Scalar(N::Float(1.0)))]),
        ),
        ("A", "int", call("id", vec![("x", Tree::text("F"))])),
        (
            "B",
            "int",
            call("id", vec![("x", Tree::text("UNDECLARED"))]),
        ),
    ]);
    let Err(PrepareError::Invalid(findings)) =
        PreparedSpecification::prepare_with_project(&spec, &[function("id")])
    else {
        panic!("static reference findings")
    };
    assert_eq!(findings.len(), 2);
    assert_eq!(
        findings[0].diagnostic().spec_paths,
        ["columns.A.derivation.function.args.x"]
    );
    assert_eq!(
        findings[0].diagnostic().definition().condition,
        "invalid_function_argument"
    );
    assert_eq!(
        findings[1].diagnostic().definition().condition,
        "unknown_field"
    );
    assert_eq!(
        findings[1].diagnostic().context["identifier"],
        ContextValue::Scalar(Value::Str("UNDECLARED".into()))
    );
}

#[test]
fn stored_field_kinds_bind_exactly_and_nonkey_arguments_use_collection() {
    let spec = specification(vec![(
        "A",
        "int",
        call("id", vec![("x", Tree::text("SRC.ID"))]),
    )]);
    let prepared = PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
    let plan = prepared.bind(&source()).unwrap();
    let Expression::ProjectFunction(bound) = &plan.columns()[0].expression else {
        panic!("function")
    };
    assert_eq!(
        bound.arguments()[0].input,
        FunctionInput::Collect {
            column: 0,
            identifier: "SRC.ID".into()
        }
    );
    let wrong = TableSchema::new(vec![Column {
        name: "ID".into(),
        kind: ColumnType::Float,
    }])
    .unwrap();
    let Err(BindError::Invalid(findings)) = prepared.bind(&wrong) else {
        panic!("exact argument type")
    };
    let finding = findings
        .iter()
        .find_map(|f| match f {
            BindFinding::ProjectFunction(f) => Some(f),
            _ => None,
        })
        .unwrap();
    assert!(matches!(finding.cause, FunctionCause::ArgumentType { .. }));
    assert_eq!(
        finding.diagnostic().definition().requirement,
        Some("REQ-0700")
    );
    assert_eq!(
        finding.diagnostic().spec_paths,
        ["columns.A.derivation.function.args.x"]
    );
}

#[test]
fn missing_stored_fields_and_unknown_relations_keep_authored_argument_diagnostics() {
    let spec = specification(vec![
        ("A", "int", call("id", vec![("x", Tree::text("SRC.TYPO"))])),
        ("B", "int", call("id", vec![("x", Tree::text("OTHER.ID"))])),
    ]);
    let prepared = PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
    let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
        panic!("complete source-reference findings")
    };
    assert_eq!(findings.len(), 2);
    for (finding, column, name) in [
        (&findings[0], "A", "SRC.TYPO"),
        (&findings[1], "B", "OTHER.ID"),
    ] {
        let diagnostics = finding
            .diagnostics(prepared.source())
            .expect("authored diagnostic");
        assert_eq!(diagnostics.len(), 1);
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.definition().condition, "unknown_field");
        assert_eq!(diagnostic.definition().requirement, Some("REQ-0103"));
        assert_eq!(
            diagnostic.spec_paths,
            [format!("columns.{column}.derivation.function.args.x")]
        );
        assert_eq!(
            diagnostic.context["identifier"],
            ContextValue::Scalar(Value::Str(name.into()))
        );
        assert_eq!(diagnostic.context.len(), 1);
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
    }
}

#[test]
fn calls_keep_dependency_cycles_and_legacy_compiler_boundaries() {
    let spec = specification(vec![
        ("A", "int", call("id", vec![("x", Tree::text("B"))])),
        ("B", "int", call("id", vec![("x", Tree::text("A"))])),
    ]);
    assert!(matches!(
        PreparedSpecification::prepare(&spec),
        Err(PrepareError::Unsupported(_))
    ));
    let prepared = PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
    let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
        panic!("dependency findings")
    };
    assert!(findings
        .iter()
        .any(|f| matches!(f, BindFinding::Dependencies { .. })));
}

#[test]
fn retired_fields_and_malformed_literals_are_complete_static_findings() {
    let retired = Tree::map(vec![(
        "function",
        Tree::map(vec![
            ("name", Tree::text("id")),
            ("contract_version", Tree::text("1")),
            (
                "args",
                Tree::map(vec![(
                    "x",
                    Tree::map(vec![("literal", Tree::Scalar(N::Integer("7".into())))]),
                )]),
            ),
        ]),
    )]);
    let spec = specification(vec![("A", "int", retired)]);
    let Err(PrepareError::Invalid(findings)) =
        PreparedSpecification::prepare_with_project(&spec, &[function("id")])
    else {
        panic!("closed calls")
    };
    assert_eq!(findings.len(), 2);
    assert!(findings
        .iter()
        .all(|f| matches!(f, PreflightFinding::ProjectFunction(_))));
}

#[test]
fn unknown_function_is_retained_alongside_an_invalid_scalar_in_the_same_call() {
    let spec = specification(vec![(
        "VALUE",
        "int",
        call(
            "unknown",
            vec![("x", Tree::Scalar(N::Integer("9223372036854775808".into())))],
        ),
    )]);
    let Err(PrepareError::Invalid(findings)) =
        PreparedSpecification::prepare_with_project(&spec, &[function("id")])
    else {
        panic!("independent name and scalar findings")
    };
    assert_eq!(findings.len(), 2);
    assert_eq!(
        findings[0].diagnostic().definition().condition,
        "unknown_project_function"
    );
    assert_eq!(
        findings[0].diagnostic().spec_paths,
        ["columns.VALUE.derivation.function.name"]
    );
    assert_eq!(
        findings[1].diagnostic().context["cause"],
        ContextValue::Scalar(Value::Str("integer_range".into()))
    );
}
