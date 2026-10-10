use yamaa_core::{
    producer_admission::{Candidate, WrittenPath},
    schema::{
        Document, DocumentLimits, DocumentNode as N, SpecificationDocument, ValidationBudget,
    },
};
pub enum Tree<'a> {
    Text(&'a str),
    Null,
    Map(Vec<(&'a str, Tree<'a>)>),
    List(Vec<Tree<'a>>),
}
impl Tree<'_> {
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            Self::Text(s) => N::Text(s.into()),
            Self::Null => N::Null,
            Self::Map(fields) => N::Mapping(
                fields
                    .into_iter()
                    .map(|(k, v)| (Self::Text(k).append(nodes), v.append(nodes)))
                    .collect(),
            ),
            Self::List(values) => {
                N::Sequence(values.into_iter().map(|v| v.append(nodes)).collect())
            }
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
}
pub fn document(
    input: Tree<'_>,
    columns: &[(&str, &str, Option<&str>)],
    output: &[&str],
    ordinal: bool,
) -> SpecificationDocument {
    use Tree::*;
    let first = match &input {
        Map(fields) => fields[0].0,
        _ => panic!("input mapping"),
    };
    let variable = format!("{first}.ID");
    let mut fields = vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("TEST")),
        ("base", Text(first)),
        ("input", input),
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
                                    Map(vec![("source", Map(vec![("variable", Text(&variable))]))]),
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
    ];
    if ordinal {
        fields.push(("filter", Text("ID > 0")));
    }
    let mut nodes = Vec::new();
    let root = Map(fields).append(&mut nodes);
    SpecificationDocument::admit(
        Document::new(nodes, root, DocumentLimits::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
    .unwrap()
    .unwrap()
}
pub fn producer() -> SpecificationDocument {
    use Tree::*;
    document(
        Map(vec![("RAW", Map(vec![("path", Text("raw.csv"))]))]),
        &[
            ("VALUE", "float", Some("Reported value")),
            ("ID", "int", Some("Identifier")),
            ("INTERNAL", "str", None),
        ],
        &["ID", "VALUE"],
        false,
    )
}
pub fn consumer(types: Option<Tree<'_>>) -> SpecificationDocument {
    use Tree::*;
    let mut fields = vec![
        ("path", Text("produced.csv")),
        ("schema", Text("nested/producer.yaml")),
    ];
    if let Some(types) = types {
        fields.push(("types", types));
    }
    document(
        Map(vec![("SRC", Map(fields))]),
        &[("ID", "int", Some("ID"))],
        &["ID"],
        false,
    )
}
pub fn candidate(document: &SpecificationDocument) -> Candidate<'_> {
    Candidate {
        dataset: "SRC",
        schema_path: "nested/producer.yaml",
        schema_origin: WrittenPath {
            declaring_source: "/parent.yaml",
            written: "../nested/producer.yaml",
        },
        input_origin: WrittenPath {
            declaring_source: "/consumer.yaml",
            written: "./produced.csv",
        },
        output_origin: WrittenPath {
            declaring_source: "/producer.yaml",
            written: "produced.csv",
        },
        schema_identity: "/producer.yaml",
        producer_identity: "/producer.yaml",
        source_identity: "/produced.csv",
        output_identity: "/produced.csv",
        document,
    }
}
