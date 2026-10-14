//! Actual aggregate preparation and joined row binding reach their new causes.
use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::{ConditionCode, ContextValue, Diagnostic},
    schema::{Document, DocumentNode as N, SpecificationDocument, ValidationBudget},
    specification::{BindError, PreflightFinding, PrepareError, PreparedSpecification},
    table::{Column, TableSchema},
    value::{ColumnType, Value},
};
enum T<'a> {
    Text(&'a str),
    Map(Vec<(&'a str, T<'a>)>),
    List(Vec<T<'a>>),
    Int(i64),
}
use T::*;
impl T<'_> {
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            Text(v) => N::Text(v.into()),
            Int(v) => N::Integer(v.to_string()),
            Map(fields) => N::Mapping(
                fields
                    .into_iter()
                    .map(|(k, v)| (Text(k).append(nodes), v.append(nodes)))
                    .collect(),
            ),
            List(values) => N::Sequence(values.into_iter().map(|v| v.append(nodes)).collect()),
        };
        let index = nodes.len();
        nodes.push(node);
        index
    }
}
fn value(operation: T<'_>) -> T<'_> {
    Map(vec![("value", operation)])
}
fn source(name: &str) -> T<'_> {
    Map(vec![("source", Map(vec![("variable", Text(name))]))])
}
fn binding<'a>(name: &'a str, operation: T<'a>) -> T<'a> {
    Map(vec![
        ("name", Text(name)),
        ("type", Text("int")),
        ("derivation", value(operation)),
    ])
}
fn document(operation: T<'_>, grouped: Option<bool>, intermediate: bool) -> SpecificationDocument {
    let mut columns = vec![Map(vec![
        ("name", Text("ID")),
        ("type", Text("int")),
        ("derivation", value(source("SRC.ID"))),
    ])];
    let mut result = vec![("name", Text("VALUE")), ("type", Text("int"))];
    let mut fields = vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("TEST")),
        ("base", Text("SRC")),
        ("keys", List(vec![Text("ID")])),
        (
            "input",
            Map(vec![
                ("SRC", Map(vec![("path", Text("source.csv"))])),
                ("SECOND", Map(vec![("path", Text("second.csv"))])),
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
    if let Some(grouped) = grouped {
        let mut row = vec![
            ("id", Text("first")),
            ("dataset", Text("SRC")),
            (
                "derivations",
                Map(vec![
                    ("ID", value(source("SRC.ID"))),
                    ("VALUE", value(operation)),
                ]),
            ),
        ];
        if grouped {
            row.push(("group_by", List(vec![Text("SRC.ID")])));
        }
        fields.push(("rows", List(vec![Map(row)])));
    } else {
        result.push(("derivation", value(operation)));
    }
    if intermediate {
        fields.push((
            "intermediates",
            List(vec![Map(vec![
                ("id", Text("CAP")),
                ("dataset", Text("SRC")),
            ])]),
        ));
    }
    columns.push(Map(result));
    fields.push(("columns", List(columns)));
    let mut nodes = vec![];
    let root = Map(fields).append(&mut nodes);
    SpecificationDocument::admit(
        Document::new(nodes, root, Default::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
    .unwrap()
    .unwrap()
}
fn findings(model: SpecificationDocument) -> Vec<Diagnostic> {
    let prepared = match PreparedSpecification::prepare(&model) {
        Ok(value) => value,
        Err(PrepareError::Invalid(findings)) => {
            return findings
                .into_iter()
                .map(|finding| match finding {
                    PreflightFinding::Scalar(d) => d,
                    _ => panic!("unexpected preparation finding"),
                })
                .collect()
        }
        Err(error) => panic!("{error:?}"),
    };
    let schema = TableSchema::new(vec![
        Column {
            name: "ID".into(),
            kind: ColumnType::Int,
        },
        Column {
            name: "V".into(),
            kind: ColumnType::Int,
        },
    ])
    .unwrap();
    let schemas = prepared
        .sources()
        .iter()
        .map(|_| &schema)
        .collect::<Vec<_>>();
    let Err(BindError::Invalid(findings)) = prepared.bind_sources(&schemas) else {
        panic!("binding must fail")
    };
    findings
        .into_iter()
        .flat_map(|finding| finding.diagnostics(prepared.source()).unwrap())
        .collect()
}
fn driver_document(dataset: &str, extra: Vec<(&str, T<'_>)>) -> SpecificationDocument {
    let mut intermediate = vec![("id", Text("CAP")), ("dataset", Text(dataset))];
    intermediate.extend(extra);
    let fields = Map(vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("TEST")),
        ("keys", List(vec![Text("ID")])),
        (
            "input",
            Map(vec![
                ("SRC", Map(vec![("path", Text("source.csv"))])),
                ("SECOND", Map(vec![("path", Text("second.csv"))])),
            ]),
        ),
        ("intermediates", List(vec![Map(intermediate)])),
        (
            "columns",
            List(vec![Map(vec![("name", Text("ID")), ("type", Text("int"))])]),
        ),
        (
            "rows",
            List(vec![Map(vec![
                ("id", Text("first")),
                ("dataset", Text("CAP")),
                ("derivations", Map(vec![("ID", value(source("CAP.ID")))])),
            ])]),
        ),
        (
            "output",
            Map(vec![
                ("path", Text("result.csv")),
                ("columns", List(vec![Text("ID")])),
            ]),
        ),
    ]);
    let mut nodes = vec![];
    let root = fields.append(&mut nodes);
    SpecificationDocument::admit(
        Document::new(nodes, root, Default::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
    .unwrap()
    .unwrap()
}
pub(super) fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for (dataset, extra, field) in [
        ("SELF", vec![], "dataset"),
        ("SRC", vec![("key", List(vec![Text("ID")]))], "key"),
        ("SRC", vec![("no_match", Int(0))], "no_match"),
        ("SRC", vec![("filter", Text("SRC.V = SECOND.V"))], "filter"),
    ] {
        let ds = findings(driver_document(dataset, extra));
        let d = ds
            .iter()
            .find(|d| d.definition().condition == "invalid_intermediate_driver")
            .unwrap();
        assert_eq!(d.definition().phase, "validation");
        assert_eq!(d.definition().requirement, Some("REQ-1262"));
        assert_eq!(d.spec_paths, ["rows[0].dataset"]);
        assert_eq!(
            d.context["intermediate"],
            ContextValue::Scalar(Value::Str("CAP".into()))
        );
        assert_eq!(
            d.context["fields"],
            ContextValue::Sequence(vec![ContextValue::Scalar(Value::Str(field.into()))])
        );
        reached.insert(d.code);
    }
    for (derive, condition, requirement, context, intermediate) in [
        (
            List(vec![]),
            "invalid_derive_step",
            "REQ-1189",
            vec![("reason", "a derive step is a non-empty binding list")],
            false,
        ),
        (
            List(vec![binding("V", Map(vec![("literal", Int(1))]))]),
            "invalid_derive_step",
            "REQ-1191",
            vec![("reason", "a derive step names exactly one relation")],
            false,
        ),
        (
            List(vec![binding(
                "V",
                Map(vec![("compute", Map(vec![("expr", Text("W + 1"))]))]),
            )]),
            "unknown_derive_variable",
            "REQ-1189",
            vec![("variable", "W"), ("binding", "V")],
            false,
        ),
        (
            List(vec![
                binding("V", source("SRC.V")),
                binding("C", source("CAP.V")),
            ]),
            "invalid_derive_step",
            "REQ-1242",
            vec![
                (
                    "reason",
                    "a derive step reads only keep-declared named intermediates",
                ),
                ("intermediate", "CAP"),
            ],
            true,
        ),
    ] {
        let ds = findings(document(
            Map(vec![(
                "aggregate",
                Map(vec![("derive", derive), ("expr", Text("SUM(V)"))]),
            )]),
            None,
            intermediate,
        ));
        let d = ds
            .into_iter()
            .find(|d| {
                d.definition().condition == condition
                    && d.definition().requirement == Some(requirement)
            })
            .unwrap();
        assert_eq!(d.definition().phase, "validation");
        for (key, text) in context {
            assert_eq!(
                d.context[key],
                ContextValue::Scalar(Value::Str(text.into()))
            );
        }
        reached.insert(d.code);
    }
    for (grouped, requirement) in [(false, "REQ-0410"), (true, "REQ-0411")] {
        let ds = findings(document(
            Map(vec![("compute", Map(vec![("expr", Text("SECOND.V + 1"))]))]),
            Some(grouped),
            false,
        ));
        let d = ds
            .into_iter()
            .find(|d| d.definition().requirement == Some(requirement))
            .unwrap();
        assert_eq!(d.definition().condition, "unknown_field");
        assert_eq!(d.definition().phase, "validation");
        assert_eq!(d.spec_paths, ["rows[0].derivations.VALUE.compute.expr"]);
        assert_eq!(
            d.context["identifier"],
            ContextValue::Scalar(Value::Str("SECOND.V".into()))
        );
        reached.insert(d.code);
    }
    reached
}
