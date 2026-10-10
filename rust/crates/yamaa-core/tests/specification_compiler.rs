//! Compile normalized documents using only the core crate and immutable schemas.
use yamaa_core::{
    dataset::{DatasetPlan, Expression},
    schema::{
        Document, DocumentLimits, DocumentNode as N, SpecificationDocument, ValidationBudget,
    },
    specification::{
        BindError, BindFinding, CompilationLimits, PrepareError, PreparedSpecification,
    },
    table::{Column, TableSchema},
    value::ColumnType,
};

enum Tree<'a> {
    Scalar(N),
    Text(&'a str),
    Map(Vec<(&'a str, Tree<'a>)>),
    List(Vec<Tree<'a>>),
}
impl Tree<'_> {
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            Self::Scalar(node) => node,
            Self::Text(value) => N::Text(value.into()),
            Self::List(values) => {
                N::Sequence(values.into_iter().map(|v| v.append(nodes)).collect())
            }
            Self::Map(fields) => N::Mapping(
                fields
                    .into_iter()
                    .map(|(k, v)| (Self::Text(k).append(nodes), v.append(nodes)))
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
}

fn document(expression: &str, input_path: &str) -> SpecificationDocument {
    operation_document(
        "compute",
        Tree::Map(vec![("expr", Tree::Text(expression))]),
        input_path,
    )
}
fn operation_document(
    operation: &str,
    payload: Tree<'_>,
    input_path: &str,
) -> SpecificationDocument {
    operation_document_with_handler(operation, payload, input_path, None)
}
fn operation_document_with_handler(
    operation: &str,
    payload: Tree<'_>,
    input_path: &str,
    handler: Option<N>,
) -> SpecificationDocument {
    operation_document_with_rows(operation, payload, input_path, handler, None)
}
fn operation_document_with_rows(
    operation: &str,
    payload: Tree<'_>,
    input_path: &str,
    handler: Option<N>,
    rows: Option<Tree<'_>>,
) -> SpecificationDocument {
    use Tree::*;
    let column = |name, op, field, value| {
        Map(vec![
            ("name", Text(name)),
            ("type", Text("int")),
            (
                "derivation",
                Map(vec![(
                    "value",
                    Map(vec![(op, Map(vec![(field, Text(value))]))]),
                )]),
            ),
        ])
    };
    let mut wrapper = vec![("value", Map(vec![(operation, payload)]))];
    if let Some(value) = handler {
        wrapper.push(("unconvertible", Scalar(value)));
    }
    let mut fields = vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("TEST")),
        (
            "input",
            Map(vec![("SRC", Map(vec![("path", Text(input_path))]))]),
        ),
        ("keys", List(vec![Text("ID")])),
        (
            "columns",
            List(vec![
                column("ID", "source", "variable", "SRC.ID"),
                Map(vec![
                    ("name", Text("VALUE")),
                    ("type", Text("int")),
                    ("derivation", Map(wrapper)),
                ]),
            ]),
        ),
        (
            "output",
            Map(vec![
                ("path", Text("result.csv")),
                ("columns", List(vec![Text("ID"), Text("VALUE")])),
            ]),
        ),
    ];
    if let Some(rows) = rows {
        fields.push(("rows", rows));
    }
    let tree = Map(fields);
    let mut nodes = Vec::new();
    let root = tree.append(&mut nodes);
    let document = Document::new(nodes, root, DocumentLimits::default()).unwrap();
    SpecificationDocument::admit(document, &mut ValidationBudget::new(Default::default()))
        .unwrap()
        .unwrap()
}

fn source() -> TableSchema {
    TableSchema::new(vec![Column {
        name: "ID".into(),
        kind: ColumnType::Int,
    }])
    .unwrap()
}

fn case_document(when: &str, expression: &str) -> SpecificationDocument {
    use Tree::*;
    operation_document(
        "case",
        List(vec![
            Map(vec![
                ("when", Text(when)),
                (
                    "then",
                    Map(vec![("compute", Map(vec![("expr", Text(expression))]))]),
                ),
            ]),
            Map(vec![(
                "otherwise",
                Map(vec![("literal", Scalar(N::Integer("1".into())))]),
            )]),
        ]),
        "input.csv",
    )
}

#[test]
fn case_grammar_findings_retain_their_original_nested_paths() {
    let prepared = PreparedSpecification::prepare(&case_document("ID >", "1")).unwrap();
    let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
        panic!("predicate grammar finding")
    };
    let diagnostics = findings[0].diagnostics(prepared.source()).unwrap();
    assert_eq!(
        diagnostics[0].spec_paths,
        ["columns.VALUE.derivation.case[0].when"]
    );
    assert!(matches!(findings[0], BindFinding::Predicate(_)));
    let prepared = PreparedSpecification::prepare(&case_document("ID > 0", "ID +")).unwrap();
    let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
        panic!("numeric grammar finding")
    };
    let diagnostics = findings[0].diagnostics(prepared.source()).unwrap();
    assert_eq!(
        diagnostics[0].spec_paths,
        ["columns.VALUE.derivation.case[0].then.compute.expr"]
    );
    assert!(matches!(findings[0], BindFinding::Numeric { .. }));
}

#[test]
fn non_key_case_predicate_source_scope_is_refused_before_study_authority() {
    let Err(PrepareError::Unsupported(features)) =
        PreparedSpecification::prepare(&case_document("SRC.ID > 0", "1"))
    else {
        panic!("unsupported implicit-group record read")
    };
    assert_eq!(features[0].operation, "case_source_predicate");
    assert_eq!(features[0].path, "columns.VALUE.derivation.case[0].when");
}

#[test]
fn compiler_owns_bound_plan_without_an_engine_or_source_port() {
    let prepared = PreparedSpecification::prepare(&document("ID + 1 / 0", "input.csv")).unwrap();
    assert_eq!(prepared.source().path, "input.csv");
    assert_eq!(prepared.projection(), ["ID", "VALUE"]);
    let plan: DatasetPlan = prepared.bind(&source()).unwrap();
    assert_eq!(plan.source(), &source());
    assert_eq!(plan.keys(), [0]);
    assert_eq!(
        plan.templates()[0].assignments[0].expression,
        Expression::Source(0)
    );
    let Expression::Compute(compute) = &plan.columns()[0].expression else {
        panic!("compiled numeric declaration")
    };
    assert_eq!(
        compute.expression().spec_path(),
        "columns.VALUE.derivation.compute"
    );
    // Division by zero remains deferred until an application actually executes a row.
    assert_eq!(plan.columns()[0].path, "columns.VALUE.derivation.compute");
}

#[test]
fn compiler_retains_admission_binding_and_resource_boundaries() {
    let prepared = PreparedSpecification::prepare(&document("ID +", "input.csv")).unwrap();
    let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
        panic!("binding diagnostic")
    };
    assert!(
        matches!(&findings[..], [BindFinding::Numeric { path, expression, .. }] if path == "columns.VALUE.derivation.compute.expr" && expression == "ID +")
    );
    let Err(PrepareError::Unsupported(features)) =
        PreparedSpecification::prepare(&document("ID + 1", "input.bad"))
    else {
        panic!("unsupported admission")
    };
    assert!(features
        .iter()
        .any(|f| f.operation == "source_format" && f.path == "input.SRC.path"));
    let prepared = PreparedSpecification::prepare(&document("ID + 1", "input.PARQUET")).unwrap();
    assert_eq!(
        prepared.source().profile,
        yamaa_core::specification::SourceProfile::Parquet
    );
    assert!(!prepared.source().empty_string_present);
    assert!(prepared.source().text_is_missing(""));
    assert!(!prepared.source().text_is_missing("NA"));
    assert!(matches!(
        PreparedSpecification::prepare_with_limits(
            &document("ID + 1", "input.csv"),
            CompilationLimits {
                columns: 1,
                ..Default::default()
            }
        ),
        Err(PrepareError::Limit("columns"))
    ));
}

#[test]
fn parquet_type_field_limit_precedes_allocating_semantic_findings() {
    let original = document("ID + 1", "source.parquet");
    let d = original.document();
    let inputs = d.field(d.root(), "input").unwrap();
    let source = d.field(inputs, "SRC").unwrap();
    let mut nodes = d.nodes().to_vec();
    let types = Tree::Map(vec![
        ("ID", Tree::Text("int")),
        ("VALUE", Tree::Text("float")),
    ])
    .append(&mut nodes);
    let name = Tree::Text("types").append(&mut nodes);
    let N::Mapping(fields) = &mut nodes[source] else {
        unreachable!()
    };
    fields.push((name, types));
    // Reorder the arena after adding fields, preserving the model's ownership contract.
    fn copy(input: &[N], id: usize, out: &mut Vec<N>) -> usize {
        let value = match &input[id] {
            N::Sequence(items) => {
                N::Sequence(items.iter().map(|&id| copy(input, id, out)).collect())
            }
            N::Mapping(items) => N::Mapping(
                items
                    .iter()
                    .map(|&(k, v)| (copy(input, k, out), copy(input, v, out)))
                    .collect(),
            ),
            value => value.clone(),
        };
        let index = out.len();
        out.push(value);
        index
    }
    let mut ordered = Vec::new();
    let root = copy(&nodes, d.root(), &mut ordered);
    let model = SpecificationDocument::admit(
        Document::new(ordered, root, Default::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
    .unwrap()
    .unwrap();
    assert!(matches!(
        PreparedSpecification::prepare_with_limits(
            &model,
            CompilationLimits {
                source_fields: 1,
                ..Default::default()
            }
        ),
        Err(PrepareError::Limit("source_fields"))
    ));
    let Err(PrepareError::Invalid(findings)) = PreparedSpecification::prepare_with_limits(
        &model,
        CompilationLimits {
            source_fields: 2,
            ..Default::default()
        },
    ) else {
        panic!("at the exact field limit the declarations remain semantic findings");
    };
    assert_eq!(findings.len(), 2);
    assert!(findings.iter().all(|f| matches!(
        f,
        yamaa_core::specification::PreflightFinding::RedundantSourceType { .. }
    )));
}

fn window_document(filter: &str) -> SpecificationDocument {
    window_settings(filter, "row_number", None, None, None)
}
fn window_settings(
    filter: &str,
    operation: &str,
    method: Option<&str>,
    direction: Option<&str>,
    nulls: Option<&str>,
) -> SpecificationDocument {
    let original = document("0", "input.csv");
    let input = original.document();
    let N::Sequence(columns) = &input.nodes()[input.field(input.root(), "columns").unwrap()] else {
        unreachable!()
    };
    let derivation = input.field(columns[1], "derivation").unwrap();
    let value = input.field(derivation, "value").unwrap();
    let payload = input.field(value, "compute").unwrap();
    let mut nodes = input.nodes().to_vec();
    let mut order = vec![("variable", Tree::Text("ID"))];
    if let Some(value) = direction {
        order.push(("direction", Tree::Text(value)));
    }
    if let Some(value) = nulls {
        order.push(("nulls", Tree::Text(value)));
    }
    let window = Tree::Map(vec![
        ("order_by", Tree::List(vec![Tree::Map(order)])),
        ("filter", Tree::Text(filter)),
    ])
    .append(&mut nodes);
    let window_name = Tree::Text("window").append(&mut nodes);
    let operation = Tree::Text(operation).append(&mut nodes);
    let mut options = vec![(window_name, window)];
    if let Some(value) = method {
        options.push((
            Tree::Text("method").append(&mut nodes),
            Tree::Text(value).append(&mut nodes),
        ));
    }
    nodes[payload] = N::Mapping(options);
    nodes[value] = N::Mapping(vec![(operation, payload)]);
    fn copy_node(input: &[N], id: usize, output: &mut Vec<N>) -> usize {
        let node = match &input[id] {
            N::Sequence(items) => N::Sequence(
                items
                    .iter()
                    .map(|&id| copy_node(input, id, output))
                    .collect(),
            ),
            N::Mapping(items) => N::Mapping(
                items
                    .iter()
                    .map(|&(a, b)| (copy_node(input, a, output), copy_node(input, b, output)))
                    .collect(),
            ),
            node => node.clone(),
        };
        let id = output.len();
        output.push(node);
        id
    }
    let mut ordered = Vec::new();
    let root = copy_node(&nodes, input.root(), &mut ordered);
    let doc = Document::new(ordered, root, Default::default()).unwrap();
    SpecificationDocument::admit(doc, &mut ValidationBudget::new(Default::default()))
        .unwrap()
        .unwrap()
}

#[test]
fn window_compiler_binds_output_dependencies_without_engine_or_records() {
    use yamaa_core::dataset::WindowKind;
    let prepared = PreparedSpecification::prepare(&window_document("ID > 0")).unwrap();
    let plan = prepared.bind(&source()).unwrap();
    let Expression::Window(window) = &plan.columns()[0].expression else {
        panic!("compiled window")
    };
    assert_eq!(window.kind, WindowKind::RowNumber);
    assert!(window.group_by.is_empty());
    assert_eq!(window.order_by[0].column, 0);
    assert!(!window.order_by[0].descending);
    assert!(!window.order_by[0].nulls_first);
    assert_eq!(
        window.filter.as_ref().unwrap().plan().spec_path(),
        "columns.VALUE.derivation.row_number.window.filter"
    );
    let policy = CompilationLimits {
        source_fields: 0,
        ..Default::default()
    };
    assert!(matches!(
        PreparedSpecification::prepare_with_limits(&window_document("ID > 0"), policy),
        Err(PrepareError::Limit("window_order"))
    ));
    assert!(matches!(
        PreparedSpecification::prepare(&window_document("SRC.ID > 0")),
        Err(PrepareError::Unsupported(_))
    ));
}

#[test]
fn directly_admitted_windows_preserve_defaults_and_explicit_options() {
    use yamaa_core::dataset::WindowKind;
    for (method, kind) in [
        (None, WindowKind::Competition),
        (Some("competition"), WindowKind::Competition),
        (Some("dense"), WindowKind::Dense),
    ] {
        for direction in [None, Some("asc"), Some("desc")] {
            for nulls in [None, Some("last"), Some("first")] {
                let document = window_settings("ID > 0", "rank", method, direction, nulls);
                let plan = PreparedSpecification::prepare(&document)
                    .unwrap()
                    .bind(&source())
                    .unwrap();
                let Expression::Window(window) = &plan.columns()[0].expression else {
                    panic!("compiled rank")
                };
                assert_eq!(window.kind, kind);
                assert_eq!(window.order_by[0].descending, direction == Some("desc"));
                assert_eq!(window.order_by[0].nulls_first, nulls == Some("first"));
            }
        }
    }
}

fn literal_document(node: N) -> SpecificationDocument {
    operation_document("literal", Tree::Scalar(node), "input.csv")
}

#[test]
fn scalar_column_leaves_use_the_common_compiled_literal_expression() {
    use yamaa_core::value::Value;
    for (node, value) in [
        (N::Null, Value::Missing),
        (
            N::Text("λ,\"quoted\"".into()),
            Value::Str("λ,\"quoted\"".into()),
        ),
        (N::Boolean(true), Value::Bool(true)),
        (N::Float(1.5), Value::float(1.5)),
        (
            N::Integer("9223372036854775807".into()),
            Value::Int(i64::MAX),
        ),
        (
            N::Integer("-9223372036854775808".into()),
            Value::Int(i64::MIN),
        ),
    ] {
        let prepared = PreparedSpecification::prepare(&literal_document(node)).unwrap();
        let plan = prepared.bind(&source()).unwrap();
        assert_eq!(plan.columns()[0].expression, Expression::Literal(value));
        assert_eq!(plan.columns()[0].path, "columns.VALUE.derivation.literal");
        assert_eq!(plan.output().columns()[1].kind, ColumnType::Int);
        assert_eq!(plan.keys(), [0]);
    }
}

#[test]
fn unrepresented_literal_integers_remain_unsupported_before_binding() {
    let Err(PrepareError::Unsupported(features)) =
        PreparedSpecification::prepare(&literal_document(N::Integer("9223372036854775808".into())))
    else {
        panic!("wide integer must not narrow")
    };
    assert_eq!(features.len(), 1);
    assert_eq!(features[0].operation, "wide_integer_literal");
    assert_eq!(features[0].path, "columns.VALUE.derivation.literal");
}

#[test]
fn column_recovery_compiles_present_null_and_text_handlers_without_dependencies() {
    use yamaa_core::value::Value;
    for (node, value) in [
        (N::Null, Value::Missing),
        (N::Integer("7".into()), Value::Int(7)),
        (
            N::Text("NOT.A.REFERENCE".into()),
            Value::Str("NOT.A.REFERENCE".into()),
        ),
    ] {
        let document = operation_document_with_handler(
            "literal",
            Tree::Scalar(N::Boolean(true)),
            "input.csv",
            Some(node),
        );
        let prepared = PreparedSpecification::prepare(&document).unwrap();
        let plan = prepared.bind(&source()).unwrap();
        let handlers = plan.conversion_handlers();
        assert_eq!(handlers.len(), 1);
        assert_eq!(
            handlers[0].assignment_path,
            "columns.VALUE.derivation.literal"
        );
        assert_eq!(
            handlers[0].handler.spec_path,
            "columns.VALUE.derivation.unconvertible"
        );
        assert_eq!(handlers[0].handler.value, value);
        assert_eq!(
            plan.conversion_handler("columns.VALUE.derivation.literal"),
            Some(&handlers[0].handler)
        );
        assert_eq!(
            plan.columns()[0].expression,
            Expression::Literal(Value::Bool(true))
        );
        assert_eq!(plan.keys(), [0]);
    }
    let prepared = PreparedSpecification::prepare(&literal_document(N::Null)).unwrap();
    assert!(prepared
        .bind(&source())
        .unwrap()
        .conversion_handlers()
        .is_empty());
}

#[test]
fn wide_recovery_literal_remains_unsupported_before_source_binding() {
    let document = operation_document_with_handler(
        "literal",
        Tree::Scalar(N::Boolean(true)),
        "input.csv",
        Some(N::Integer("9223372036854775808".into())),
    );
    let Err(PrepareError::Unsupported(features)) = PreparedSpecification::prepare(&document) else {
        panic!("wide handler must not narrow")
    };
    assert_eq!(features.len(), 1);
    assert_eq!(features[0].operation, "wide_integer_literal");
    assert_eq!(features[0].path, "columns.VALUE.derivation.unconvertible");
}

fn recovery_rows(value: N, count: usize) -> Tree<'static> {
    use Tree::*;
    let mut rows = vec![Map(vec![
        ("id", Text("override")),
        (
            "derivations",
            Map(vec![(
                "VALUE",
                Map(vec![
                    ("value", Map(vec![("literal", Scalar(N::Boolean(true)))])),
                    ("unconvertible", Scalar(value)),
                ]),
            )]),
        ),
    ])];
    for id in ["default-one", "default-two"].into_iter().take(count - 1) {
        rows.push(Map(vec![("id", Text(id)), ("derivations", Map(vec![]))]));
    }
    List(rows)
}

#[test]
fn row_recovery_binds_only_effective_handlers_in_row_order_and_shares_defaults() {
    use yamaa_core::value::Value;
    for count in [1, 3] {
        let document = operation_document_with_rows(
            "literal",
            Tree::Scalar(N::Boolean(true)),
            "input.csv",
            Some(N::Null),
            Some(recovery_rows(N::Integer("7".into()), count)),
        );
        let plan = PreparedSpecification::prepare(&document)
            .unwrap()
            .bind(&source())
            .unwrap();
        let handlers = plan.conversion_handlers();
        assert_eq!(handlers.len(), if count == 1 { 1 } else { 2 });
        assert_eq!(
            handlers[0].assignment_path,
            "rows[0].derivations.VALUE.literal"
        );
        assert_eq!(
            handlers[0].handler.spec_path,
            "rows[0].derivations.VALUE.unconvertible"
        );
        assert_eq!(handlers[0].handler.value, Value::Int(7));
        if count == 3 {
            assert_eq!(handlers[1].handler.value, Value::Missing);
            assert_eq!(
                handlers[1].handler.spec_path,
                "columns.VALUE.derivation.unconvertible"
            );
            assert_eq!(
                plan.templates()[1].assignments[0].path,
                plan.templates()[2].assignments[0].path
            );
        }
    }
}

#[test]
fn wide_row_recovery_is_unsupported_at_its_authored_path_before_binding() {
    let document = operation_document_with_rows(
        "literal",
        Tree::Scalar(N::Boolean(true)),
        "input.csv",
        None,
        Some(recovery_rows(N::Integer("9223372036854775808".into()), 1)),
    );
    let Err(PrepareError::Unsupported(features)) = PreparedSpecification::prepare(&document) else {
        panic!("wide row handler must not narrow")
    };
    assert_eq!(features.len(), 1);
    assert_eq!(features[0].operation, "wide_integer_literal");
    assert_eq!(features[0].path, "rows[0].derivations.VALUE.unconvertible");
}

#[test]
fn row_filter_unsupported_carriers_reject_during_preparation_including_dead_branches() {
    use Tree::*;
    for filter in [
        "FALSE AND ID = 9223372036854775808",
        "TRUE OR ID = -9223372036854775809",
    ] {
        let rows = List(vec![Map(vec![
            ("id", Text("first")),
            ("filter", Text(filter)),
            (
                "derivations",
                Map(vec![
                    (
                        "ID",
                        Map(vec![(
                            "value",
                            Map(vec![("source", Map(vec![("variable", Text("SRC.ID"))]))]),
                        )]),
                    ),
                    (
                        "VALUE",
                        Map(vec![(
                            "value",
                            Map(vec![("literal", Scalar(N::Integer("2".into())))]),
                        )]),
                    ),
                ]),
            ),
        ])]);
        let document = operation_document_with_rows(
            "literal",
            Scalar(N::Integer("2".into())),
            "input.csv",
            None,
            Some(rows),
        );
        let Err(PrepareError::Unsupported(features)) = PreparedSpecification::prepare(&document)
        else {
            panic!("unsupported predicate carrier must reject before a study port can be supplied");
        };
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].operation, "predicate_literal");
        assert_eq!(features[0].path, "rows[0].filter");
    }
}

#[test]
fn row_filters_charge_the_whole_document_before_predicate_parsing() {
    use Tree::*;
    let rows = List(
        ["first", "second"]
            .into_iter()
            .map(|id| {
                Map(vec![
                    ("id", Text(id)),
                    ("filter", Text("ID >")),
                    (
                        "derivations",
                        Map(vec![
                            (
                                "ID",
                                Map(vec![(
                                    "value",
                                    Map(vec![("source", Map(vec![("variable", Text("SRC.ID"))]))]),
                                )]),
                            ),
                            (
                                "VALUE",
                                Map(vec![(
                                    "value",
                                    Map(vec![("literal", Scalar(N::Integer("2".into())))]),
                                )]),
                            ),
                        ]),
                    ),
                ])
            })
            .collect(),
    );
    let document = operation_document_with_rows(
        "literal",
        Scalar(N::Integer("2".into())),
        "input.csv",
        None,
        Some(rows),
    );
    assert!(matches!(
        PreparedSpecification::prepare_with_limits(
            &document,
            CompilationLimits {
                numeric_bytes: 7,
                ..Default::default()
            }
        ),
        Err(PrepareError::Limit("numeric_bytes"))
    ));
    // Grammar findings remain retained for post-ingestion binding when aggregate text fits.
    assert!(PreparedSpecification::prepare_with_limits(
        &document,
        CompilationLimits {
            numeric_bytes: 8,
            ..Default::default()
        }
    )
    .is_ok());
}

#[test]
fn grouped_filter_promotes_a_row_local_default_without_claiming_ungrouped_promotion() {
    use Tree::*;
    for grouped in [false, true] {
        let mut fields = vec![
            ("id", Text("first")),
            ("filter", Text("VALUE = 2")),
            (
                "derivations",
                Map(vec![(
                    "ID",
                    Map(vec![(
                        "value",
                        Map(vec![("source", Map(vec![("variable", Text("SRC.ID"))]))]),
                    )]),
                )]),
            ),
        ];
        if grouped {
            fields.push(("group_by", List(vec![Text("SRC.ID")])));
        }
        let document = operation_document_with_rows(
            "literal",
            Scalar(N::Integer("2".into())),
            "input.csv",
            None,
            Some(List(vec![Map(fields)])),
        );
        if grouped {
            let plan = PreparedSpecification::prepare(&document)
                .unwrap()
                .bind(&source())
                .unwrap();
            assert_eq!(plan.templates()[0].assignments.len(), 2);
            assert!(plan.templates()[0].filter.is_some());
            assert!(plan.columns().is_empty());
        } else {
            let Err(PrepareError::Unsupported(features)) =
                PreparedSpecification::prepare(&document)
            else {
                panic!("unreconciled promotion must remain unsupported");
            };
            assert_eq!(features.len(), 1);
            assert_eq!(features[0].operation, "row_filter_default");
            assert_eq!(features[0].path, "rows[0].filter");
        }
    }
}

#[test]
fn source_filters_charge_compilation_before_admission_and_reject_unimplemented_literals() {
    let document = operation_document(
        "source",
        Tree::Map(vec![
            ("variable", Tree::Text("SRC.ID")),
            ("filter", Tree::Text("SRC.ID = 1")),
        ]),
        "input.csv",
    );
    assert!(matches!(
        PreparedSpecification::prepare_with_limits(
            &document,
            CompilationLimits {
                numeric_bytes: 0,
                ..Default::default()
            }
        ),
        Err(PrepareError::Limit("numeric_bytes"))
    ));
    let document = operation_document(
        "source",
        Tree::Map(vec![
            ("variable", Tree::Text("SRC.ID")),
            ("filter", Tree::Text("SRC.ID = 9223372036854775808")),
        ]),
        "input.csv",
    );
    let Err(PrepareError::Unsupported(findings)) = PreparedSpecification::prepare(&document) else {
        panic!("unimplemented literal must reject before ingestion");
    };
    assert!(findings
        .iter()
        .any(|finding| finding.operation == "predicate_literal"));
}

#[test]
fn selection_metadata_and_filters_are_bounded_before_owned_compilation() {
    let payload = Tree::Map(vec![(
        "sources",
        Tree::List(vec![Tree::Map(vec![
            ("variable", Tree::Text("SRC.ID")),
            ("filter", Tree::Text("SRC.ID =")),
        ])]),
    )]);
    let document = operation_document("first_available", payload, "input.csv");
    for (limits, name) in [
        (
            CompilationLimits {
                source_operands: 0,
                ..Default::default()
            },
            "source_operands",
        ),
        (
            CompilationLimits {
                numeric_bytes: 0,
                ..Default::default()
            },
            "numeric_bytes",
        ),
    ] {
        assert!(
            matches!(PreparedSpecification::prepare_with_limits(&document,limits),Err(PrepareError::Limit(resource)) if resource==name)
        );
    }
}

#[test]
fn assertion_text_budget_precedes_deferred_grammar_and_charges_both_predicates() {
    use Tree::*;
    let when = "ID = 1";
    let require = "ID >";
    let tree = Map(vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("TEST")),
        (
            "input",
            Map(vec![("SRC", Map(vec![("path", Text("input.csv"))]))]),
        ),
        ("keys", List(vec![Text("ID")])),
        (
            "columns",
            List(vec![Map(vec![
                ("name", Text("ID")),
                ("type", Text("int")),
                (
                    "derivation",
                    Map(vec![(
                        "value",
                        Map(vec![("source", Map(vec![("variable", Text("SRC.ID"))]))]),
                    )]),
                ),
            ])]),
        ),
        (
            "output",
            Map(vec![
                ("path", Text("result.csv")),
                ("columns", List(vec![Text("ID")])),
            ]),
        ),
        (
            "verifications",
            List(vec![Map(vec![(
                "assert",
                Map(vec![("when", Text(when)), ("require", Text(require))]),
            )])]),
        ),
    ]);
    let mut nodes = Vec::new();
    let root = tree.append(&mut nodes);
    let document = SpecificationDocument::admit(
        Document::new(nodes, root, Default::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
    .unwrap()
    .unwrap();
    let bytes = when.len() + require.len();
    assert!(matches!(
        PreparedSpecification::prepare_with_limits(
            &document,
            CompilationLimits {
                numeric_bytes: bytes - 1,
                ..Default::default()
            }
        ),
        Err(PrepareError::Limit("numeric_bytes"))
    ));
    let plan = PreparedSpecification::prepare_with_limits(
        &document,
        CompilationLimits {
            numeric_bytes: bytes,
            ..Default::default()
        },
    )
    .unwrap()
    .bind(&source())
    .unwrap();
    assert!(matches!(
        &plan.verifications()[0].check,
        yamaa_core::dataset::Check::PredicateDeclaration(_)
    ));
    assert!(
        matches!(&plan.verifications()[1].check, yamaa_core::dataset::Check::InvalidDiagnostic(d)
        if d.code == yamaa_core::diagnostic::ConditionCode::PredicateInvalidExpression)
    );
}
