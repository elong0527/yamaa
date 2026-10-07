//! Exercise the original-document compiler, not constructed failure enum values.
use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::{ConditionCode, ContextValue as V},
    schema::{Document, DocumentNode as N, SpecificationDocument, ValidationBudget},
    specification::{PrepareError, PreparedSpecification},
    value::Value,
};

enum Tree<'a> {
    Text(&'a str),
    Map(Vec<(&'a str, Tree<'a>)>),
    List(Vec<Tree<'a>>),
}
use Tree::*;
impl Tree<'_> {
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            Text(value) => N::Text(value.into()),
            Map(fields) => N::Mapping(
                fields
                    .into_iter()
                    .map(|(k, v)| (Text(k).append(nodes), v.append(nodes)))
                    .collect(),
            ),
            List(values) => N::Sequence(values.into_iter().map(|v| v.append(nodes)).collect()),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
}
fn source<'a>() -> Tree<'a> {
    Map(vec![(
        "value",
        Map(vec![("source", Map(vec![("variable", Text("SRC.ID"))]))]),
    )])
}
fn column<'a>(name: &'a str, derivation: Option<Tree<'a>>) -> Tree<'a> {
    let mut fields = vec![("name", Text(name)), ("type", Text("int"))];
    if let Some(derivation) = derivation {
        fields.push(("derivation", derivation));
    }
    Map(fields)
}
fn inputs<'a>() -> Tree<'a> {
    Map(vec![
        ("SRC", Map(vec![("path", Text("source.csv"))])),
        ("SECOND", Map(vec![("path", Text("second.csv"))])),
    ])
}
fn text(value: &str) -> V {
    V::Scalar(Value::Str(value.into()))
}
fn texts(values: &[&str]) -> V {
    V::Sequence(values.iter().map(|v| text(v)).collect())
}
type Expected<'a> = (&'a str, Option<&'a str>, Vec<&'a str>, Vec<(&'a str, V)>);

fn check(fields: Vec<(&str, Tree<'_>)>, expected: Vec<Expected<'_>>) -> BTreeSet<ConditionCode> {
    let mut nodes = Vec::new();
    let root = Map(fields).append(&mut nodes);
    let document = Document::new(nodes, root, Default::default()).unwrap();
    let model =
        SpecificationDocument::admit(document, &mut ValidationBudget::new(Default::default()))
            .unwrap()
            .unwrap();
    let Err(PrepareError::Invalid(findings)) = PreparedSpecification::prepare(&model) else {
        panic!("preflight error from compiler");
    };
    assert_eq!(findings.len(), expected.len());
    let mut reached = BTreeSet::new();
    for (finding, (condition, requirement, paths, context)) in findings.iter().zip(expected) {
        let diagnostic = finding.diagnostic();
        let definition = diagnostic.definition();
        assert_eq!(
            (
                definition.phase,
                definition.condition,
                definition.requirement
            ),
            ("validation", condition, requirement)
        );
        assert_eq!(diagnostic.spec_paths, paths);
        assert_eq!(
            diagnostic.context,
            context
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect()
        );
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
        reached.insert(diagnostic.code);
    }
    reached
}
fn base<'a>(
    domain: &'a str,
    input: Tree<'a>,
    columns: Vec<Tree<'a>>,
    keys: Vec<Tree<'a>>,
) -> Vec<(&'a str, Tree<'a>)> {
    vec![
        ("schema_version", Text("1.0")),
        ("domain", Text(domain)),
        ("input", input),
        ("keys", List(keys)),
        ("columns", List(columns)),
        (
            "output",
            Map(vec![
                ("path", Text("result.csv")),
                ("columns", List(vec![Text("ID")])),
            ]),
        ),
    ]
}

pub(super) fn reached() -> BTreeSet<ConditionCode> {
    let mut fields = base(
        "SRC",
        inputs(),
        vec![column("ID", None)],
        vec![Text("MISSING")],
    );
    fields.push(("base", Text("ABSENT")));
    let mut reached = check(
        fields,
        vec![
            (
                "missing_derivation",
                Some("REQ-0198"),
                vec!["columns.ID.derivation"],
                vec![("column", text("ID"))],
            ),
            (
                "duplicate_identifier",
                Some("REQ-0080"),
                vec!["input.SRC", "domain"],
                vec![("identifier", text("SRC"))],
            ),
            (
                "undeclared_column",
                Some("REQ-0220"),
                vec!["keys[0]"],
                vec![("column", text("MISSING"))],
            ),
            (
                "driver_unavailable",
                None,
                vec!["base"],
                vec![("dataset", text("ABSENT"))],
            ),
        ],
    );
    reached.extend(check(
        base(
            "TEST",
            inputs(),
            vec![column("ID", Some(source()))],
            vec![Text("ID")],
        ),
        vec![(
            "driver_unavailable",
            None,
            vec!["base"],
            vec![("row", V::Scalar(Value::Missing))],
        )],
    ));
    let aggregate = Map(vec![(
        "value",
        Map(vec![(
            "aggregate",
            Map(vec![("expr", Text("SUM(SRC.ID)"))]),
        )]),
    )]);
    let mut fields = base(
        "TEST",
        inputs(),
        vec![column("ID", None), column("X", Some(aggregate))],
        vec![Text("ID")],
    );
    fields.push(("filter", Text("TRUE")));
    fields.push((
        "rows",
        List(vec![
            Map(vec![
                ("id", Text("first")),
                ("dataset", Text("ABSENT")),
                ("group_by", List(vec![])),
                (
                    "derivations",
                    Map(vec![("BOGUS", source()), ("X", source())]),
                ),
            ]),
            Map(vec![
                ("id", Text("second")),
                ("dataset", Text("SRC")),
                ("group_by", List(vec![Text("SECOND.ID")])),
                ("derivations", Map(vec![("ID", source()), ("X", source())])),
            ]),
            Map(vec![
                ("id", Text("third")),
                ("derivations", Map(vec![("ID", source()), ("X", source())])),
            ]),
        ]),
    ));
    reached.extend(check(
        fields,
        vec![
            (
                "undeclared_column",
                None,
                vec!["rows[0].derivations.BOGUS"],
                vec![("column", text("BOGUS"))],
            ),
            (
                "missing_derivation",
                Some("REQ-0200"),
                vec!["columns.ID.derivation"],
                vec![("column", text("ID")), ("rows", texts(&["first"]))],
            ),
            (
                "duplicate_derivation",
                Some("REQ-1260"),
                vec!["columns.X.derivation"],
                vec![
                    ("column", text("X")),
                    ("rows", texts(&["first", "second", "third"])),
                ],
            ),
            (
                "conflicting_row_construction",
                Some("REQ-1171"),
                vec!["filter", "rows"],
                vec![],
            ),
            (
                "driver_unavailable",
                None,
                vec!["rows[0].dataset"],
                vec![("row", text("first")), ("dataset", text("ABSENT"))],
            ),
            (
                "invalid_field_type",
                Some("REQ-0065"),
                vec!["rows[0].group_by"],
                vec![("row", text("first")), ("group_by", texts(&[]))],
            ),
            (
                "unknown_field",
                Some("REQ-0066"),
                vec!["rows[1].group_by"],
                vec![
                    ("row", text("second")),
                    ("identifier", text("SECOND.ID")),
                    ("dataset", text("SRC")),
                ],
            ),
            (
                "driver_unavailable",
                None,
                vec!["rows[2].dataset"],
                vec![
                    ("row", text("third")),
                    ("dataset", V::Scalar(Value::Missing)),
                ],
            ),
        ],
    ));
    // Duplicate groups retain authored order instead of sorting or deduplicating.
    let mut fields = base(
        "TEST",
        inputs(),
        vec![column("ID", Some(source()))],
        vec![Text("ID")],
    );
    fields.push((
        "rows",
        List(vec![Map(vec![
            ("id", Text("duplicates")),
            ("dataset", Text("SRC")),
            ("group_by", List(vec![Text("SRC.ID"), Text("SRC.ID")])),
            ("derivations", Map(vec![])),
        ])]),
    ));
    reached.extend(check(
        fields,
        vec![(
            "invalid_field_type",
            Some("REQ-0065"),
            vec!["rows[0].group_by"],
            vec![
                ("row", text("duplicates")),
                ("group_by", texts(&["SRC.ID", "SRC.ID"])),
            ],
        )],
    ));
    for kind in ["str", "int", "float", "date", "datetime"] {
        let input = Map(vec![(
            "SRC",
            Map(vec![
                ("path", Text("source.PARQUET")),
                ("types", Map(vec![("ID", Text(kind))])),
            ]),
        )]);
        reached.extend(check(
            base(
                "TEST",
                input,
                vec![column("ID", Some(source()))],
                vec![Text("ID")],
            ),
            vec![(
                "redundant_field_type",
                Some("REQ-0533"),
                vec!["input.SRC.types.ID"],
                vec![
                    ("dataset", text("SRC")),
                    ("field", text("ID")),
                    ("type", text(kind)),
                ],
            )],
        ));
    }
    reached
}
