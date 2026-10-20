//! Actual aggregate preparation and joined row binding reach their new causes.
use crate::support::document::Tree as T;
use std::collections::BTreeSet;
use yamaa_core::json::Value as JsonValue;
use yamaa_core::{
    diagnostic::{ConditionCode, Diagnostic},
    schema::{Document, SpecificationDocument},
    specification::{BindError, PreflightFinding, PrepareError, PreparedSpecification},
    table::{Column, TableSchema},
    value::ColumnType,
};

fn value(operation: T) -> T {
    T::map(vec![("value", operation)])
}
fn source(name: &str) -> T {
    T::map(vec![("source", T::map(vec![("variable", T::text(name))]))])
}
fn binding(name: &str, operation: T) -> T {
    T::map(vec![
        ("name", T::text(name)),
        ("type", T::text("int")),
        ("derivation", value(operation)),
    ])
}
fn document(operation: T, grouped: Option<bool>, intermediate: bool) -> SpecificationDocument {
    let mut columns = vec![T::map(vec![
        ("name", T::text("ID")),
        ("type", T::text("int")),
        ("derivation", value(source("SRC.ID"))),
    ])];
    let mut result = vec![("name", T::text("VALUE")), ("type", T::text("int"))];
    let mut fields = vec![
        ("schema_version", T::text("1.0")),
        ("domain", T::text("TEST")),
        ("base", T::text("SRC")),
        ("keys", T::List(vec![T::text("ID")])),
        (
            "input",
            T::map(vec![
                ("SRC", T::map(vec![("path", T::text("source.csv"))])),
                ("SECOND", T::map(vec![("path", T::text("second.csv"))])),
            ]),
        ),
        (
            "output",
            T::map(vec![
                ("path", T::text("result.csv")),
                ("columns", T::List(vec![T::text("ID"), T::text("VALUE")])),
            ]),
        ),
    ];
    if let Some(grouped) = grouped {
        let mut row = vec![
            ("id", T::text("first")),
            ("dataset", T::text("SRC")),
            (
                "derivations",
                T::map(vec![
                    ("ID", value(source("SRC.ID"))),
                    ("VALUE", value(operation)),
                ]),
            ),
        ];
        if grouped {
            row.push(("group_by", T::List(vec![T::text("SRC.ID")])));
        }
        fields.push(("rows", T::List(vec![T::map(row)])));
    } else {
        result.push(("derivation", value(operation)));
    }
    if intermediate {
        fields.push((
            "intermediates",
            T::List(vec![T::map(vec![
                ("id", T::text("CAP")),
                ("dataset", T::text("SRC")),
                ("require_match", T::boolean(true)),
            ])]),
        ));
    }
    columns.push(T::map(result));
    fields.push(("columns", T::List(columns)));
    let mut nodes = vec![];
    let root = T::map(fields).append(&mut nodes);
    SpecificationDocument::admit(Document::new(nodes, root).unwrap())
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
fn driver_document(dataset: &str, extra: Vec<(&str, T)>) -> SpecificationDocument {
    let mut intermediate = vec![("id", T::text("CAP")), ("dataset", T::text(dataset))];
    intermediate.extend(extra);
    let fields = T::map(vec![
        ("schema_version", T::text("1.0")),
        ("domain", T::text("TEST")),
        ("keys", T::List(vec![T::text("ID")])),
        (
            "input",
            T::map(vec![
                ("SRC", T::map(vec![("path", T::text("source.csv"))])),
                ("SECOND", T::map(vec![("path", T::text("second.csv"))])),
            ]),
        ),
        ("intermediates", T::List(vec![T::map(intermediate)])),
        (
            "columns",
            T::List(vec![T::map(vec![
                ("name", T::text("ID")),
                ("type", T::text("int")),
            ])]),
        ),
        (
            "rows",
            T::List(vec![T::map(vec![
                ("id", T::text("first")),
                ("dataset", T::text("CAP")),
                ("derivations", T::map(vec![("ID", value(source("CAP.ID")))])),
            ])]),
        ),
        (
            "output",
            T::map(vec![
                ("path", T::text("result.csv")),
                ("columns", T::List(vec![T::text("ID")])),
            ]),
        ),
    ]);
    let mut nodes = vec![];
    let root = fields.append(&mut nodes);
    SpecificationDocument::admit(Document::new(nodes, root).unwrap())
        .unwrap()
        .unwrap()
}
pub(crate) fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for (dataset, extra, field) in [
        ("SELF", vec![], "dataset"),
        ("SRC", vec![("key", T::List(vec![T::text("ID")]))], "key"),
        ("SRC", vec![("no_match", T::integer(0))], "no_match"),
        (
            "SRC",
            vec![("filter", T::text("SRC.V = SECOND.V"))],
            "filter",
        ),
    ] {
        let ds = findings(driver_document(dataset, extra));
        let d = ds
            .iter()
            .find(|d| d.definition().condition == "invalid_intermediate_driver")
            .unwrap();
        assert_eq!(d.definition().phase, "validation");
        assert_eq!(d.definition().requirement, Some("ROW-2"));
        assert_eq!(d.spec_paths, ["rows[0].dataset"]);
        assert_eq!(d.context["intermediate"], JsonValue::String("CAP".into()));
        assert_eq!(
            d.context["fields"],
            JsonValue::Array(vec![JsonValue::String(field.into())])
        );
        reached.insert(d.code);
    }
    for (derive, condition, requirement, context, intermediate) in [
        (
            T::List(vec![]),
            "invalid_derive_step",
            "LOOKUP-22",
            vec![("reason", "a derive step is a non-empty binding list")],
            false,
        ),
        (
            T::List(vec![binding("V", T::map(vec![("literal", T::integer(1))]))]),
            "invalid_derive_step",
            "LOOKUP-22",
            vec![("reason", "a derive step names exactly one relation")],
            false,
        ),
        (
            T::List(vec![binding(
                "V",
                T::map(vec![("compute", T::map(vec![("expr", T::text("W + 1"))]))]),
            )]),
            "unknown_derive_variable",
            "LOOKUP-22",
            vec![("variable", "W"), ("binding", "V")],
            false,
        ),
        (
            T::List(vec![
                binding("V", source("SRC.V")),
                binding("C", source("CAP.V")),
            ]),
            "invalid_derive_step",
            "LOOKUP-22",
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
            T::map(vec![(
                "aggregate",
                T::map(vec![("derive", derive), ("expr", T::text("SUM(V)"))]),
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
            assert_eq!(d.context[key], JsonValue::String(text.into()));
        }
        reached.insert(d.code);
    }
    for (grouped, requirement) in [(false, "FORMULA-9"), (true, "FORMULA-9")] {
        let ds = findings(document(
            T::map(vec![(
                "compute",
                T::map(vec![("expr", T::text("SECOND.V + 1"))]),
            )]),
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
            JsonValue::String("SECOND.V".into())
        );
        reached.insert(d.code);
    }
    reached
}
