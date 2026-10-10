use yamaa_core::{
    producer_contract::{prepare, Cause, Error, Limits},
    schema::{
        Document, DocumentLimits, DocumentNode as N, SpecificationDocument, ValidationBudget,
    },
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
            Self::Text(v) => N::Text(v.into()),
            Self::Map(v) => N::Mapping(
                v.into_iter()
                    .map(|(k, v)| (Self::Text(k).append(nodes), v.append(nodes)))
                    .collect(),
            ),
            Self::List(v) => N::Sequence(v.into_iter().map(|v| v.append(nodes)).collect()),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
}
fn document(columns: &[(&str, &str, Option<&str>)], output: &[&str]) -> SpecificationDocument {
    use Tree::*;
    let tree = Map(vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("PRODUCER")),
        (
            "input",
            Map(vec![("SRC", Map(vec![("path", Text("input.csv"))]))]),
        ),
        ("keys", List(vec![Text("ID")])),
        (
            "columns",
            List(
                columns
                    .iter()
                    .map(|(name, kind, label)| {
                        let mut fields = vec![
                            ("name", Text(name)),
                            ("type", Text(kind)),
                            (
                                "derivation",
                                Map(vec![(
                                    "value",
                                    Map(vec![("source", Map(vec![("variable", Text("SRC.ID"))]))]),
                                )]),
                            ),
                        ];
                        if let Some(label) = label {
                            fields.push(("label", Text(label)));
                        }
                        Map(fields)
                    })
                    .collect(),
            ),
        ),
        (
            "output",
            Map(vec![
                ("path", Text("produced.csv")),
                ("columns", List(output.iter().copied().map(Text).collect())),
            ]),
        ),
    ]);
    let mut nodes = Vec::new();
    let root = tree.append(&mut nodes);
    SpecificationDocument::admit(
        Document::new(nodes, root, DocumentLimits::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
    .unwrap()
    .unwrap()
}
const FIELDS: &[(&str, &str, Option<&str>)] = &[
    ("VALUE", "float", Some("Reported value")),
    ("ID", "int", Some("Identifier")),
    ("INTERNAL", "str", None),
];

#[test]
fn producer_projection_is_the_stored_order_and_internal_columns_are_excluded() {
    let contract = prepare(&document(FIELDS, &["ID", "VALUE"]), Limits::default()).unwrap();
    assert_eq!(contract.path(), "produced.csv");
    assert_eq!(
        contract
            .fields()
            .iter()
            .map(|f| (f.name.as_str(), f.kind, f.label.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("ID", ColumnType::Int, "Identifier"),
            ("VALUE", ColumnType::Float, "Reported value")
        ]
    );
    assert!(contract.matches_header(&["ID", "VALUE"]));
    for actual in [
        vec!["VALUE", "ID"],
        vec!["ID"],
        vec!["ID", "VALUE", "OTHER"],
        vec!["ID", "ID"],
        vec![],
    ] {
        assert!(!contract.matches_header(&actual));
    }
    assert!(contract.matches_typed_fields(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Float)]));
    assert!(!contract.matches_typed_fields(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Str)]));
    assert!(
        !contract.matches_typed_fields(&[("VALUE", ColumnType::Float), ("ID", ColumnType::Int)])
    );
}

#[test]
fn producer_projection_rejects_empty_duplicate_and_undeclared_fields_in_order() {
    assert_eq!(
        prepare(&document(FIELDS, &[]), Limits::default()),
        Err(Error::Invalid(vec![Cause::Empty]))
    );
    assert_eq!(
        prepare(&document(FIELDS, &["ID", "ID", "OTHER"]), Limits::default()),
        Err(Error::Invalid(vec![
            Cause::Duplicate {
                position: 1,
                field: "ID".into()
            },
            Cause::Declaration {
                position: 2,
                field: "OTHER".into(),
                count: 0
            }
        ]))
    );
}

#[test]
fn producer_requires_one_typed_declaration_and_a_nonblank_label_per_stored_field() {
    let duplicates = [
        ("ID", "int", Some("Identifier")),
        ("ID", "str", Some("Other")),
    ];
    assert_eq!(
        prepare(&document(&duplicates, &["ID"]), Limits::default()),
        Err(Error::Invalid(vec![Cause::Declaration {
            position: 0,
            field: "ID".into(),
            count: 2
        }]))
    );
    for label in [None, Some(""), Some(" \t\n")] {
        assert_eq!(
            prepare(
                &document(&[("ID", "int", label)], &["ID"]),
                Limits::default()
            ),
            Err(Error::Invalid(vec![Cause::Label { field: "ID".into() }]))
        );
    }
}

#[test]
fn every_declared_logical_type_survives_without_value_inference() {
    let fields = [
        ("ID", "int", Some("ID")),
        ("TEXT", "str", Some("Text")),
        ("VALUE", "float", Some("Value")),
        ("DATE", "date", Some("Date")),
        ("TIME", "datetime", Some("Time")),
    ];
    let names = ["TIME", "DATE", "VALUE", "TEXT", "ID"];
    let contract = prepare(&document(&fields, &names), Limits::default()).unwrap();
    assert_eq!(
        contract.fields().iter().map(|f| f.kind).collect::<Vec<_>>(),
        vec![
            ColumnType::DateTime,
            ColumnType::Date,
            ColumnType::Float,
            ColumnType::Str,
            ColumnType::Int
        ]
    );
}

#[test]
fn producer_aggregate_budgets_refuse_before_a_partial_contract_escapes() {
    let spec = document(FIELDS, &["ID", "VALUE"]);
    assert_eq!(
        prepare(
            &spec,
            Limits {
                declarations: 2,
                ..Limits::default()
            }
        ),
        Err(Error::Limit("producer_declarations"))
    );
    assert_eq!(
        prepare(
            &spec,
            Limits {
                fields: 1,
                ..Limits::default()
            }
        ),
        Err(Error::Limit("producer_fields"))
    );
    let bytes = spec
        .document()
        .nodes()
        .iter()
        .map(|n| match n {
            N::Text(s) | N::Integer(s) => s.len(),
            _ => 0,
        })
        .sum::<usize>();
    assert_eq!(
        prepare(
            &spec,
            Limits {
                text_bytes: bytes - 1,
                ..Limits::default()
            }
        ),
        Err(Error::Limit("producer_text_bytes"))
    );
    assert!(prepare(
        &spec,
        Limits {
            text_bytes: bytes,
            ..Limits::default()
        }
    )
    .is_ok());
}
