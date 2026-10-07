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
    Text(&'a str),
    Map(Vec<(&'a str, Tree<'a>)>),
    List(Vec<Tree<'a>>),
}
impl Tree<'_> {
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
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
    let tree = Map(vec![
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
                column("VALUE", "compute", "expr", expression),
            ]),
        ),
        (
            "output",
            Map(vec![
                ("path", Text("result.csv")),
                ("columns", List(vec![Text("ID"), Text("VALUE")])),
            ]),
        ),
    ]);
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
        PreparedSpecification::prepare(&document("ID + 1", "input.parquet"))
    else {
        panic!("unsupported admission")
    };
    assert!(features
        .iter()
        .any(|f| f.operation == "source_format" && f.path == "input.SRC.path"));
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
