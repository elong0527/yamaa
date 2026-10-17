pub use crate::support::document::Tree;
use yamaa_core::{
    producer_contract::{Link, Links},
    schema::{Document, SpecificationDocument},
};

pub const CONSUMER: &str = "/consumer.yaml";

pub fn document(
    input: Tree,
    columns: &[(&str, &str, Option<&str>)],
    output: &[&str],
    unsupported_parent: bool,
) -> SpecificationDocument {
    let first = match &input {
        Tree::Map(fields) => fields[0].0.as_str(),
        _ => panic!("input mapping"),
    };
    let variable = format!("{first}.ID");
    let mut fields = vec![
        ("schema_version", Tree::text("1.0")),
        ("domain", Tree::text("TEST")),
        ("base", Tree::text(first)),
        ("input", input),
        ("keys", Tree::List(vec![Tree::text("ID")])),
        (
            "columns",
            Tree::List(
                columns
                    .iter()
                    .map(|(name, kind, label)| {
                        let mut fields = vec![
                            ("name", Tree::text(name)),
                            ("type", Tree::text(kind)),
                            (
                                "derivation",
                                Tree::map(vec![(
                                    "value",
                                    Tree::map(vec![(
                                        "source",
                                        Tree::map(vec![("variable", Tree::text(&variable))]),
                                    )]),
                                )]),
                            ),
                        ];
                        if let Some(label) = label {
                            fields.push(("label", Tree::text(label)));
                        }
                        Tree::map(fields)
                    })
                    .collect(),
            ),
        ),
        (
            "output",
            Tree::map(vec![
                ("path", Tree::text("produced.csv")),
                (
                    "columns",
                    Tree::List(output.iter().copied().map(Tree::text).collect()),
                ),
            ]),
        ),
    ];
    if unsupported_parent {
        fields.push(("parents", Tree::List(vec![Tree::text("parent.yaml")])));
    }
    let mut nodes = Vec::new();
    let root = Tree::map(fields).append(&mut nodes);
    SpecificationDocument::admit(Document::new(nodes, root).unwrap())
        .unwrap()
        .unwrap()
}
pub fn producer() -> SpecificationDocument {
    document(
        Tree::map(vec![(
            "RAW",
            Tree::map(vec![("path", Tree::text("raw.csv"))]),
        )]),
        &[
            ("VALUE", "float", Some("Reported value")),
            ("ID", "int", Some("Identifier")),
            ("INTERNAL", "str", None),
        ],
        &["ID", "VALUE"],
        false,
    )
}
pub fn consumer(types: Option<Tree>) -> SpecificationDocument {
    let mut fields = vec![
        ("path", Tree::text("produced.csv")),
        ("schema", Tree::text("nested/producer.yaml")),
    ];
    if let Some(types) = types {
        fields.push(("types", types));
    }
    document(
        Tree::map(vec![("SRC", Tree::map(fields))]),
        &[("ID", "int", Some("ID"))],
        &["ID"],
        false,
    )
}
/// The `SRC` link to the producing specification held at `/producer.yaml`.
pub fn link(document: &SpecificationDocument) -> Link<'_> {
    Link {
        dataset: "SRC",
        identity: "/producer.yaml",
        document,
    }
}
/// Links of the consuming specification held at [`CONSUMER`].
pub fn links<'a>(links: &'a [Link<'a>]) -> Links<'a> {
    Links {
        consumer: CONSUMER,
        links,
    }
}
