#[path = "support/project_call_compiler.rs"]
mod support;
use support::{call, function, specification_with_rows, Tree};
use yamaa_core::{
    bound_expression::Read,
    dataset::{Expression, FunctionInput},
    diagnostic::ContextValue,
    schema::DocumentNode as N,
    specification::{BindError, PrepareError, PreparedSpecification},
    table::{Column, TableSchema},
    value::{ColumnType, Value},
};
fn literal(value: i64) -> Tree {
    Tree::map(vec![(
        "literal",
        Tree::Scalar(N::Integer(value.to_string())),
    )])
}
fn row(id: &str, mut derivations: Vec<(&str, Tree)>, groups: Option<Vec<&str>>) -> Tree {
    derivations.insert(
        0,
        (
            "ID",
            Tree::map(vec![(
                "source",
                Tree::map(vec![("variable", Tree::text("SRC.ID"))]),
            )]),
        ),
    );
    let mut fields = vec![
        ("id", Tree::text(id)),
        (
            "derivations",
            Tree::map(
                derivations
                    .into_iter()
                    .map(|(name, value)| (name, Tree::map(vec![("value", value)])))
                    .collect(),
            ),
        ),
    ];
    if let Some(groups) = groups {
        fields.push((
            "group_by",
            Tree::List(groups.into_iter().map(Tree::text).collect()),
        ));
    }
    Tree::map(fields)
}
fn source() -> TableSchema {
    TableSchema::new(vec![Column {
        name: "ID".into(),
        kind: ColumnType::Int,
    }])
    .unwrap()
}

#[test]
fn row_reads_promote_transitive_defaults_and_schedule_each_override_graph() {
    let spec = specification_with_rows(
        vec![
            ("A", "int", call("id", vec![("x", Tree::text("B"))])),
            ("B", "int", call("id", vec![("x", Tree::text("C"))])),
            ("C", "int", literal(7)),
            ("D", "int", literal(0)),
        ],
        vec![
            row(
                "first",
                vec![("D", call("id", vec![("x", Tree::text("A"))]))],
                None,
            ),
            row(
                "second",
                vec![
                    ("B", literal(9)),
                    ("D", call("id", vec![("x", Tree::text("A"))])),
                ],
                None,
            ),
        ],
    );
    let prepared =
        PreparedSpecification::prepare_with_project(&spec, &[function("unused"), function("id")])
            .unwrap();
    assert_eq!(prepared.called_functions(), [1]);
    let plan = prepared.bind(&source()).unwrap();
    assert!(plan.columns().is_empty());
    assert_eq!(
        plan.templates()[0]
            .assignments
            .iter()
            .map(|a| a.column)
            .collect::<Vec<_>>(),
        [0, 3, 2, 1, 4]
    );
    assert_eq!(
        plan.templates()[1]
            .assignments
            .iter()
            .map(|a| a.column)
            .collect::<Vec<_>>(),
        [0, 2, 1, 3, 4]
    );
    let Expression::ProjectFunction(bound) = &plan.templates()[0].assignments[2].expression else {
        panic!("inherited B function")
    };
    assert_eq!(
        bound.arguments()[0].input,
        FunctionInput::Read(Read::Column(3))
    );
}

#[test]
fn fully_overridden_function_defaults_do_not_activate_but_remain_statically_checked() {
    let spec = specification_with_rows(
        vec![(
            "A",
            "int",
            call("id", vec![("x", Tree::Scalar(N::Integer("7".into())))]),
        )],
        vec![
            row("first", vec![("A", literal(8))], None),
            row("second", vec![("A", literal(9))], None),
        ],
    );
    let prepared = PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
    assert!(prepared.called_functions().is_empty());
    assert!(prepared.project_calls().unwrap().plans().is_empty());
    assert_eq!(prepared.project_calls().unwrap().calls()[0].slot(), None);
    assert!(prepared.bind(&source()).is_ok());
    let invalid = specification_with_rows(
        vec![(
            "A",
            "int",
            call("id", vec![("x", Tree::Scalar(N::Float(7.0)))]),
        )],
        vec![
            row("first", vec![("A", literal(8))], None),
            row("second", vec![("A", literal(9))], None),
        ],
    );
    let Err(PrepareError::Invalid(findings)) =
        PreparedSpecification::prepare_with_project(&invalid, &[function("id")])
    else {
        panic!("unused default still checked")
    };
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].diagnostic().spec_paths,
        ["columns.A.derivation.function.args.x"]
    );
}

#[test]
fn row_function_cycles_report_authored_paths_without_column_forward_rules() {
    let spec = specification_with_rows(
        vec![
            ("A", "int", call("id", vec![("x", Tree::text("B"))])),
            ("B", "int", call("id", vec![("x", Tree::text("A"))])),
            ("D", "int", literal(0)),
        ],
        vec![row(
            "record",
            vec![("D", call("id", vec![("x", Tree::text("A"))]))],
            None,
        )],
    );
    let prepared = PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
    let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
        panic!("cycle")
    };
    let diagnostics = findings
        .iter()
        .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].definition().condition, "dependency_cycle");
    assert_eq!(
        diagnostics[0].spec_paths,
        [
            "columns.A.derivation.function",
            "columns.B.derivation.function"
        ]
    );
    assert_eq!(
        diagnostics[0].context["cycle"],
        ContextValue::Sequence(
            vec!["A", "B", "A"]
                .into_iter()
                .map(|s| ContextValue::Scalar(Value::Str(s.into())))
                .collect()
        )
    );
}

#[test]
fn grouped_function_arguments_keep_record_reads_and_complete_scope_and_type_findings() {
    let spec = specification_with_rows(
        vec![("A", "int", literal(0))],
        vec![row(
            "group",
            vec![("A", call("id", vec![("x", Tree::text("SRC.ID"))]))],
            Some(vec!["SRC.ID"]),
        )],
    );
    let prepared = PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
    let plan = prepared.bind(&source()).unwrap();
    let Expression::ProjectFunction(bound) = &plan.templates()[0].assignments[1].expression else {
        panic!("group function")
    };
    assert_eq!(
        bound.arguments()[0].input,
        FunctionInput::Read(Read::Source(0))
    );
    let bad = specification_with_rows(
        vec![("A", "int", literal(0))],
        vec![row(
            "group",
            vec![("A", call("id", vec![("x", Tree::text("SRC.X"))]))],
            Some(vec!["SRC.ID"]),
        )],
    );
    let prepared = PreparedSpecification::prepare_with_project(&bad, &[function("id")]).unwrap();
    let source = TableSchema::new(vec![
        Column {
            name: "ID".into(),
            kind: ColumnType::Int,
        },
        Column {
            name: "X".into(),
            kind: ColumnType::Float,
        },
    ])
    .unwrap();
    let Err(BindError::Invalid(findings)) = prepared.bind(&source) else {
        panic!("group and type findings")
    };
    let diagnostics = findings
        .iter()
        .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(
        diagnostics[0].definition().condition,
        "ungrouped_driver_field"
    );
    assert_eq!(
        diagnostics[0].context["row"],
        ContextValue::Scalar(Value::Str("group".into()))
    );
    assert_eq!(
        diagnostics[1].definition().condition,
        "invalid_function_argument"
    );
    assert!(diagnostics
        .iter()
        .all(|d| d.spec_paths == ["rows[0].derivations.A.function.args.x"]));
}

#[test]
fn unpromoted_column_calls_keep_record_reads_and_column_declaration_order() {
    let spec = specification_with_rows(
        vec![
            ("A", "int", literal(7)),
            ("B", "int", call("id", vec![("x", Tree::text("SRC.ID"))])),
        ],
        vec![row("record", vec![], None)],
    );
    let prepared = PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
    let plan = prepared.bind(&source()).unwrap();
    assert_eq!(plan.templates()[0].assignments.len(), 1);
    assert_eq!(
        plan.columns().iter().map(|a| a.column).collect::<Vec<_>>(),
        [1, 2]
    );
    let Expression::ProjectFunction(bound) = &plan.columns()[1].expression else {
        panic!("column call")
    };
    assert_eq!(
        bound.arguments()[0].input,
        FunctionInput::Read(Read::Source(0))
    );
    let forward = specification_with_rows(
        vec![
            ("A", "int", call("id", vec![("x", Tree::text("B"))])),
            ("B", "int", literal(7)),
        ],
        vec![row("record", vec![], None)],
    );
    let prepared =
        PreparedSpecification::prepare_with_project(&forward, &[function("id")]).unwrap();
    let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
        panic!("column forward reference")
    };
    let diagnostics = findings
        .iter()
        .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].definition().condition, "forward_reference");
    assert_eq!(diagnostics[0].spec_paths, ["columns.A.derivation.function"]);
}

#[test]
fn row_reads_of_nonlocal_columns_return_complete_phase_findings_before_lowering() {
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
            ("B", "int", literal(0)),
            ("C", "int", literal(0)),
        ],
        vec![row(
            "record",
            vec![
                ("B", call("id", vec![("x", Tree::text("A"))])),
                ("C", call("id", vec![("x", Tree::text("A"))])),
            ],
            None,
        )],
    );
    let Err(PrepareError::Invalid(findings)) =
        PreparedSpecification::prepare_with_project(&spec, &[function("id")])
    else {
        panic!("row phase findings")
    };
    assert_eq!(findings.len(), 2);
    for (finding, column) in findings.iter().zip(["B", "C"]) {
        let diagnostic = finding.diagnostic();
        assert_eq!(diagnostic.definition().condition, "phase_boundary");
        assert_eq!(diagnostic.definition().requirement, Some("REQ-0069"));
        assert_eq!(
            diagnostic.spec_paths,
            [format!("rows[0].derivations.{column}.function.args.x")]
        );
    }
}

#[test]
fn grouped_filters_promote_function_defaults_and_qualified_fields_do_not_promote_outputs() {
    let mut grouped = row("group", vec![], Some(vec!["SRC.ID"]));
    let Tree::Map(fields) = &mut grouped else {
        unreachable!()
    };
    fields.push(("filter".into(), Tree::text("B >= 7")));
    let spec = specification_with_rows(
        vec![
            ("A", "int", literal(7)),
            ("B", "int", call("id", vec![("x", Tree::text("A"))])),
        ],
        vec![grouped],
    );
    let prepared = PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
    let plan = prepared.bind(&source()).unwrap();
    assert!(plan.columns().is_empty());
    assert_eq!(
        plan.templates()[0]
            .assignments
            .iter()
            .map(|a| a.column)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert!(plan.templates()[0].filter.is_some());
    let spec = specification_with_rows(
        vec![("A", "int", literal(7)), ("B", "int", literal(0))],
        vec![row(
            "record",
            vec![("B", call("id", vec![("x", Tree::text("SRC.A"))]))],
            None,
        )],
    );
    let prepared = PreparedSpecification::prepare_with_project(&spec, &[function("id")]).unwrap();
    let source = TableSchema::new(vec![
        Column {
            name: "ID".into(),
            kind: ColumnType::Int,
        },
        Column {
            name: "A".into(),
            kind: ColumnType::Int,
        },
    ])
    .unwrap();
    let plan = prepared.bind(&source).unwrap();
    assert_eq!(
        plan.templates()[0]
            .assignments
            .iter()
            .map(|a| a.column)
            .collect::<Vec<_>>(),
        [0, 2]
    );
    assert_eq!(plan.columns()[0].column, 1);
}

#[test]
fn cumulative_row_context_is_bounded_before_binding_or_activation() {
    use yamaa_core::project_function::{Case, Function, Language, Parameter};
    use yamaa_core::value::ValueType;
    let parameters = (0..8).map(|n| format!("p{n}")).collect::<Vec<_>>();
    let mut definition = function("id").definition().clone();
    definition.params = parameters
        .iter()
        .map(|name| Parameter {
            name: name.clone(),
            kind: ValueType::Int,
            required: false,
            default: Some(Value::Int(1)),
            accepts_missing: false,
        })
        .collect();
    definition.tests = vec![
        Case {
            id: "ordinary".into(),
            covers: std::iter::once("normal".into())
                .chain(parameters.iter().map(|name| format!("default:{name}")))
                .collect(),
            args: vec![],
            result: Value::Int(1),
        },
        Case {
            id: "boundary".into(),
            covers: vec!["boundary".into()],
            args: vec![("p0".into(), Value::Int(i64::MIN))],
            result: Value::Int(1),
        },
    ];
    definition.tests.extend(parameters.iter().map(|name| Case {
        id: format!("missing-{name}"),
        covers: vec![format!("short-circuit-missing:{name}")],
        args: vec![(name.clone(), Value::Missing)],
        result: Value::Missing,
    }));
    let function = Function::admit(Language::Python, definition).unwrap();
    let names = (0..63).map(|n| format!("C{n}")).collect::<Vec<_>>();
    let columns = names
        .iter()
        .map(|name| (name.as_str(), "int", literal(0)))
        .collect();
    let derivations = names
        .iter()
        .map(|name| {
            (
                name.as_str(),
                call(
                    "id",
                    parameters
                        .iter()
                        .map(|parameter| (parameter.as_str(), Tree::text("SRC.X")))
                        .collect(),
                ),
            )
        })
        .collect();
    let id = "r".repeat(200_000);
    let spec = specification_with_rows(columns, vec![row(&id, derivations, Some(vec!["SRC.ID"]))]);
    assert!(matches!(
        PreparedSpecification::prepare_with_project(&spec, &[function]),
        Err(PrepareError::Limit("function_row_context"))
    ));
}
