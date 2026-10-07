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
